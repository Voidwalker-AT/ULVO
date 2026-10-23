use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use ulvo_adapters::{DirectoryWatcher, FsWatchEvent};
use ulvo_discovery::{ProjectRegistry, WatchReconcileOutcome, WorkspaceScanner};

/// Background supervisor that watches configured workspace roots with native
/// filesystem watchers and reconciles the persistent project registry as
/// projects appear, change, rename, or disappear — no manual rescan required.
///
/// Change notifications are delivered as monotonically increasing version
/// numbers through the optional `notify` channel after each persisted update.
pub struct WatcherSupervisor {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl WatcherSupervisor {
    /// Starts watching the workspace roots configured in the registry
    /// (`workspace_root` + `workspace_roots`). Returns an error when no
    /// configured root currently exists on disk.
    pub fn start<P: AsRef<Path>>(vault_root: P, notify: Option<Sender<u64>>) -> Result<Self, String> {
        let vault = vault_root.as_ref().to_path_buf();
        let registry = ProjectRegistry::load_or_create(&vault, None)?;
        let roots = collect_watch_roots(&registry);
        if roots.is_empty() {
            return Err(format!(
                "no existing workspace roots to watch (configured root: {})",
                registry.workspace_root()
            ));
        }

        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let handle = thread::Builder::new()
            .name("ulvo-watch-supervisor".into())
            .spawn(move || Self::supervise_loop(vault, roots, worker_stop, notify))
            .map_err(|e| format!("cannot spawn watcher supervisor thread: {e}"))?;

        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }

    /// Signals the supervisor to stop and waits for it to exit. Bounded: the
    /// worker polls the stop flag frequently, and each `DirectoryWatcher` stop
    /// is itself hard-bounded by the watcher's cancellation guarantees.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }

    fn supervise_loop(
        vault: PathBuf,
        roots: Vec<PathBuf>,
        stop: Arc<AtomicBool>,
        notify: Option<Sender<u64>>,
    ) {
        let mut watchers: Vec<DirectoryWatcher> = Vec::new();
        for root in &roots {
            match DirectoryWatcher::watch(root) {
                Ok(w) => watchers.push(w),
                Err(e) => eprintln!("[ULVO WatchSupervisor] cannot watch {}: {e}", root.display()),
            }
        }
        if watchers.is_empty() {
            eprintln!("[ULVO WatchSupervisor] no watchable roots; supervisor exiting");
            return;
        }

        let mut version: u64 = 0;
        while !stop.load(Ordering::SeqCst) {
            let mut batch: Vec<FsWatchEvent> = Vec::new();
            for w in &watchers {
                for _ in 0..2048 {
                    match w.receiver().try_recv() {
                        Ok(ev) => batch.push(ev),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => break,
                    }
                }
            }

            if batch.is_empty() {
                thread::sleep(Duration::from_millis(50));
                continue;
            }

            // Debounce bursty directory activity (mkdir + manifest writes arrive
            // as several events milliseconds apart) so a single reconcile pass
            // sees the settled state.
            let settle_deadline = Instant::now() + Duration::from_millis(250);
            while Instant::now() < settle_deadline && !stop.load(Ordering::SeqCst) {
                for w in &watchers {
                    for _ in 0..2048 {
                        match w.receiver().try_recv() {
                            Ok(ev) => batch.push(ev),
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => break,
                        }
                    }
                }
                thread::sleep(Duration::from_millis(30));
            }

            if stop.load(Ordering::SeqCst) {
                break;
            }

            for attempt in 0..3 {
            if stop.load(Ordering::SeqCst) { break; }
            // Reload-before-write: if another process (desktop app or CLI)
            // persisted changes concurrently, apply on top of the latest state.
            let mut registry = match ProjectRegistry::load_or_create(&vault, None) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[ULVO WatchSupervisor] cannot load registry: {e}");
                    break;
                }
            };

            let mut changed = false;
            let mut needs_full_rescan = false;
            for ev in &batch {
                match registry.apply_watch_event(ev) {
                    Ok(WatchReconcileOutcome::Updated) => changed = true,
                    Ok(WatchReconcileOutcome::FullRescanRequired) => needs_full_rescan = true,
                    Ok(WatchReconcileOutcome::Unchanged) => {}
                    Err(e) => eprintln!("[ULVO WatchSupervisor] reconcile error: {e}"),
                }
            }

            if needs_full_rescan {
                let mut all_scanned = Vec::new();
                let mut all_inaccessible = Vec::new();
                for root in &roots {
                    let (scanned, inaccessible) = WorkspaceScanner::new(root).scan();
                    all_scanned.extend(scanned);
                    all_inaccessible.extend(inaccessible);
                }
                if let Err(e) = registry.sync_with_scanned(all_scanned, all_inaccessible, &[]) {
                    eprintln!("[ULVO WatchSupervisor] full rescan commit failed (attempt {}): {e}", attempt + 1);
                    thread::sleep(Duration::from_millis(50));
                    continue;
                }
                changed = true;
            }

            if changed {
                match registry.save() {
                    Ok(()) => {
                        version += 1;
                        if let Some(tx) = &notify {
                            let _ = tx.send(version);
                        }
                        break;
                    }
                    Err(e) => {
                        eprintln!("[ULVO WatchSupervisor] registry save failed (attempt {}): {e}", attempt + 1);
                        thread::sleep(Duration::from_millis(50));
                    }
                }
            } else {
                break;
            }
            }
        }

        // Dropping each watcher triggers its bounded stop via Drop.
        drop(watchers);
    }
}

impl Drop for WatcherSupervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn collect_watch_roots(registry: &ProjectRegistry) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let configured = std::iter::once(registry.workspace_root().to_string())
        .chain(registry.workspace_roots().iter().cloned());
    for r in configured {
        if r.trim().is_empty() {
            continue;
        }
        let p = PathBuf::from(&r);
        if p.is_dir() && !roots.iter().any(|x| x == &p) {
            roots.push(p);
        }
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::mpsc;
    use ulvo_core::{DiscoverySource, ProjectClassification, ProjectRegistrationState};

    struct TestEnv {
        vault: PathBuf,
        workspace: PathBuf,
    }

    fn setup_env(tag: &str) -> TestEnv {
        let base = std::env::temp_dir().join(format!("ulvo_wsup_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let vault = base.join("vault");
        let workspace = base.join("workspace");
        fs::create_dir_all(&vault).unwrap();
        fs::create_dir_all(&workspace).unwrap();

        let mut registry = ProjectRegistry::load_or_create(&vault, Some(workspace.clone())).unwrap();
        registry
            .set_workspace_root(workspace.to_string_lossy().as_ref());
        registry.save().unwrap();
        TestEnv { vault, workspace }
    }

    fn find_entry(vault: &Path, canonical_tail: &str) -> Option<ulvo_core::RegisteredProject> {
        let reg = ProjectRegistry::load_or_create(vault, None).unwrap();
        reg.projects()
            .iter()
            .find(|p| p.canonical_path.to_lowercase().ends_with(&canonical_tail.to_lowercase()))
            .cloned()
    }

    fn wait_for_change(rx: &mpsc::Receiver<u64>, timeout: Duration) -> bool {
        rx.recv_timeout(timeout).is_ok()
    }

    #[test]
    fn test_supervisor_detects_new_project_without_rescan() {
        let env = setup_env("detect");
        let (tx, rx) = mpsc::channel();
        let mut sup = WatcherSupervisor::start(&env.vault, Some(tx)).expect("supervisor start");
        thread::sleep(Duration::from_millis(100));

        let proj = env.workspace.join("live_proj");
        fs::create_dir_all(&proj).unwrap();
        fs::write(proj.join("Cargo.toml"), "[package]\nname = \"live_proj\"\nversion = \"0.1.0\"\n").unwrap();

        assert!(wait_for_change(&rx, Duration::from_secs(5)), "expected change notification");
        // Allow the batch to settle and save
        thread::sleep(Duration::from_millis(200));

        let entry = find_entry(&env.vault, "live_proj").expect("project should be registered without rescan");
        assert_eq!(entry.discovery_source, DiscoverySource::DynamicWatcher);
        assert_eq!(entry.registration_state, ProjectRegistrationState::Discovered);
        assert_eq!(entry.classification, ProjectClassification::Standard);

        let start = Instant::now();
        sup.stop();
        assert!(start.elapsed() < Duration::from_secs(2), "supervisor stop must be bounded");
        let _ = fs::remove_dir_all(env.workspace.parent().unwrap());
    }

    #[test]
    fn test_supervisor_marks_missing_on_deletion() {
        let env = setup_env("delete");
        let (tx, rx) = mpsc::channel();
        let mut sup = WatcherSupervisor::start(&env.vault, Some(tx)).expect("supervisor start");
        thread::sleep(Duration::from_millis(100));

        let proj = env.workspace.join("doomed_proj");
        fs::create_dir_all(&proj).unwrap();
        fs::write(proj.join("Cargo.toml"), "[package]\nname = \"doomed\"\n").unwrap();
        assert!(wait_for_change(&rx, Duration::from_secs(5)), "expected discovery notification");
        thread::sleep(Duration::from_millis(150));

        fs::remove_dir_all(&proj).unwrap();
        assert!(wait_for_change(&rx, Duration::from_secs(5)), "expected deletion notification");
        thread::sleep(Duration::from_millis(200));

        let entry = find_entry(&env.vault, "doomed_proj").expect("entry should persist with Missing state");
        assert_eq!(entry.registration_state, ProjectRegistrationState::Missing);

        sup.stop();
        let _ = fs::remove_dir_all(env.workspace.parent().unwrap());
    }

    #[test]
    fn test_supervisor_rename_moves_registration() {
        let env = setup_env("rename");
        let (tx, rx) = mpsc::channel();
        let mut sup = WatcherSupervisor::start(&env.vault, Some(tx)).expect("supervisor start");
        thread::sleep(Duration::from_millis(100));

        let old_dir = env.workspace.join("before_rename");
        fs::create_dir_all(&old_dir).unwrap();
        fs::write(old_dir.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        assert!(wait_for_change(&rx, Duration::from_secs(5)), "expected initial discovery");
        thread::sleep(Duration::from_millis(150));

        let new_dir = env.workspace.join("after_rename");
        fs::rename(&old_dir, &new_dir).unwrap();
        assert!(wait_for_change(&rx, Duration::from_secs(5)), "expected rename notification");
        thread::sleep(Duration::from_millis(200));

        let old_entry = find_entry(&env.vault, "before_rename").expect("old entry kept as history");
        assert_eq!(old_entry.registration_state, ProjectRegistrationState::Missing);
        let new_entry = find_entry(&env.vault, "after_rename").expect("new path registered");
        assert_eq!(new_entry.discovery_source, DiscoverySource::DynamicWatcher);

        sup.stop();
        let _ = fs::remove_dir_all(env.workspace.parent().unwrap());
    }

    #[test]
    fn test_supervisor_does_not_manage_fixtures_templates_or_envs() {
        let env = setup_env("classify");
        let (tx, rx) = mpsc::channel();
        let mut sup = WatcherSupervisor::start(&env.vault, Some(tx)).expect("supervisor start");
        thread::sleep(Duration::from_millis(100));

        let fixture = env.workspace.join("tests").join("fixtures").join("sample_proj");
        fs::create_dir_all(&fixture).unwrap();
        fs::write(fixture.join("Cargo.toml"), "[package]\nname = \"sample\"\n").unwrap();

        let template = env.workspace.join("project_template");
        fs::create_dir_all(&template).unwrap();
        fs::write(template.join("Cargo.toml"), "[package]\nname = \"tmpl\"\n").unwrap();
        // Template WITH a connector must still not become Managed.
        fs::create_dir_all(template.join(".ulvo")).unwrap();
        fs::write(template.join(".ulvo").join("manifest.toml"), "[project]\nname = \"tmpl\"\n").unwrap();

        let venv = env.workspace.join("env_proj");
        fs::create_dir_all(&venv).unwrap();
        fs::write(venv.join("pyvenv.cfg"), "home = C:\\Python\n").unwrap();
        fs::write(venv.join("requirements.txt"), "flask\n").unwrap();

        // Debounced batch covers all three creates.
        assert!(wait_for_change(&rx, Duration::from_secs(5)), "expected discovery notification");
        thread::sleep(Duration::from_millis(400));

        let fixture_entry = find_entry(&env.vault, "sample_proj").expect("fixture dir still listed but classified");
        assert_eq!(fixture_entry.classification, ProjectClassification::TestFixture);
        assert_ne!(fixture_entry.registration_state, ProjectRegistrationState::Managed);

        let template_entry = find_entry(&env.vault, "project_template").expect("template listed but classified");
        assert_eq!(template_entry.classification, ProjectClassification::Template);
        assert_ne!(template_entry.registration_state, ProjectRegistrationState::Managed);

        assert!(find_entry(&env.vault, "env_proj").is_none(), "venv must never be registered");

        sup.stop();
        let _ = fs::remove_dir_all(env.workspace.parent().unwrap());
    }

    #[test]
    fn test_supervisor_repeated_start_stop_and_persistence() {
        let env = setup_env("repeat");

        for i in 0..3 {
            let (tx, rx) = mpsc::channel();
            let mut sup = WatcherSupervisor::start(&env.vault, Some(tx)).expect("supervisor start");
            thread::sleep(Duration::from_millis(80));

            if i == 0 {
                let proj = env.workspace.join("persisted_proj");
                fs::create_dir_all(&proj).unwrap();
                fs::write(proj.join("Cargo.toml"), "[package]\nname = \"persisted\"\n").unwrap();
                assert!(wait_for_change(&rx, Duration::from_secs(5)), "expected discovery in cycle {i}");
                thread::sleep(Duration::from_millis(150));
            }

            let start = Instant::now();
            sup.stop();
            assert!(start.elapsed() < Duration::from_secs(2), "cycle {i} stop must be bounded");
        }

        // Registry persisted across supervisor restarts.
        let entry = find_entry(&env.vault, "persisted_proj").expect("registry survives agent/supervisor restarts");
        assert_eq!(entry.discovery_source, DiscoverySource::DynamicWatcher);

        let _ = fs::remove_dir_all(env.workspace.parent().unwrap());
    }
}
