use std::{fs, path::PathBuf};
use ulvo_discovery::ProjectRegistry;

fn fixture(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ulvo_registry_{tag}_{}", std::process::id()));
    fs::create_dir_all(dir.join("registry")).unwrap();
    dir
}

#[test]
fn corrupt_registry_is_preserved_and_rejected() {
    let dir = fixture("corrupt");
    let path = dir.join("registry/projects.json");
    fs::write(&path, b"{interrupted").unwrap();
    assert!(ProjectRegistry::load_or_create(&dir, None).is_err(), "corruption must not become an empty registry");
    assert_eq!(fs::read(&path).unwrap(), b"{interrupted");
}

#[test]
fn stale_writer_cannot_erase_another_commit() {
    let dir = fixture("stale");
    let mut first = ProjectRegistry::load_or_create(&dir, None).unwrap();
    let mut stale = ProjectRegistry::load_or_create(&dir, None).unwrap();
    first.set_workspace_root("first");
    first.save().unwrap();
    stale.set_workspace_root("stale");
    assert!(stale.save().is_err(), "stale snapshots must not overwrite committed state");
    assert_eq!(ProjectRegistry::load_or_create(&dir, None).unwrap().workspace_root(), "first");
    first.set_workspace_root("second");
    first.save().unwrap();
    assert_eq!(ProjectRegistry::load_or_create(&dir, None).unwrap().workspace_root(), "second");
}

#[test]
fn unsupported_schema_is_preserved_and_rejected() {
    let dir = fixture("schema");
    let reg = ProjectRegistry::load_or_create(&dir, None).unwrap();
    reg.save().unwrap();
    let path = dir.join("registry/projects.json");
    let mut data: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    data["schema_version"] = 999.into();
    let bytes = serde_json::to_vec(&data).unwrap();
    fs::write(&path, &bytes).unwrap();
    assert!(ProjectRegistry::load_or_create(&dir, None).is_err());
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn loading_an_absent_registry_does_not_create_directories() {
    let dir = fixture("readonly").join("absent");
    ProjectRegistry::load_or_create(&dir, None).unwrap();
    assert!(!dir.exists());
}

#[test]
fn full_scan_does_not_manage_fixtures_with_connectors() {
    let dir = fixture("classification");
    let project = dir.join("tests/fixtures/sample");
    fs::create_dir_all(project.join(".ulvo")).unwrap();
    fs::write(project.join("Cargo.toml"), "[package]\nname = 'sample'\n").unwrap();
    fs::write(project.join(".ulvo/manifest.toml"), "[project]\nname = 'sample'\n").unwrap();
    let (scanned, _) = ulvo_discovery::WorkspaceScanner::new(&dir).scan();
    let found = scanned.iter().find(|p| p.name == "sample").unwrap();
    assert_eq!(found.classification, ulvo_core::ProjectClassification::TestFixture);
    assert_eq!(found.registration_state, ulvo_core::ProjectRegistrationState::Discovered);
}

#[test]
fn concurrent_registration_child() {
    let Some(root) = std::env::var_os("ULVO_REGISTRY_CHILD_ROOT") else { return };
    let id = std::env::var("ULVO_REGISTRY_CHILD_ID").unwrap();
    let root = PathBuf::from(root);
    for _ in 0..20 {
        let mut reg = ProjectRegistry::load_or_create(root.join("vault"), None).unwrap();
        if reg.register_project(&root.join(&id)).is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("registration did not converge after contention");
}

#[cfg(windows)]
#[test]
fn concurrent_processes_preserve_all_registrations() {
    use std::{process::Command, time::{Duration, Instant}};
    let dir = fixture("multiprocess");
    let mut children = Vec::new();
    for i in 0..4 {
        let id = format!("project_{i}");
        let project = dir.join(&id);
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("Cargo.toml"), "[package]\nname = 'sample'\nversion = '0.1.0'\n").unwrap();
        children.push(Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "concurrent_registration_child", "--test-threads=1"])
            .env("ULVO_REGISTRY_CHILD_ROOT", &dir)
            .env("ULVO_REGISTRY_CHILD_ID", id)
            .spawn().unwrap());
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut passed = true;
    for child in &mut children {
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                passed &= status.success();
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                passed = false;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    assert!(passed, "all concurrent writer processes must complete successfully");
    assert_eq!(ProjectRegistry::load_or_create(dir.join("vault"), None).unwrap().projects().len(), 4);
}
