use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use ulvo_adapters::{is_reparse_point, sha256_hex, write_atomic, EnvironmentFileLock, FsWatchEvent, FsWatchKind};
use ulvo_core::{
    strip_unc_prefix, DiscoverySource, ProjectClassification, ProjectRegistrationState,
    ProjectRegistryFile, RegisteredProject,
};

pub struct WorkspaceScanner {
    root: PathBuf,
    max_depth: usize,
}

impl WorkspaceScanner {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            max_depth: 5,
        }
    }

    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Recursively scans workspace root read-only for projects across Python, Rust, Node.js, Go, Ruby, Java, C/C++.
    /// Adheres strictly to security boundaries: ignores junctions, excludes package dependency caches,
    /// bounds depth, and records inaccessible paths.
    pub fn scan(&self) -> (Vec<RegisteredProject>, Vec<String>) {
        let mut projects = Vec::new();
        let mut inaccessible = Vec::new();

        let clean_root = strip_unc_prefix(&self.root);
        if !clean_root.is_dir() {
            inaccessible.push(format!("Root not a directory: {}", clean_root.display()));
            return (projects, inaccessible);
        }

        let ignored_dir_names: BTreeSet<&str> = [
            ".git", ".ulvo", ".venv", "venv", "node_modules", "target", "build", "dist",
            "out", "bin", "obj", ".idea", ".vscode", "vault", "tmp", "temp", "__pycache__",
            "cache", "caches", ".gradle", ".pytest_cache", ".ruff_cache", "site-packages",
            "wheels", "gems",
        ].into_iter().collect();

        // Queue: (dir_path, current_depth)
        let mut queue = vec![(clean_root, 0usize)];

        while let Some((current_dir, depth)) = queue.pop() {
            if depth > self.max_depth {
                continue;
            }

            if is_reparse_point(&current_dir) {
                continue;
            }

            let entries = match fs::read_dir(&current_dir) {
                Ok(e) => e,
                Err(err) => {
                    inaccessible.push(format!("{}: {}", current_dir.display(), err));
                    continue;
                }
            };

            let mut dir_entries = Vec::new();
            let mut file_names = Vec::new();

            for entry in entries.filter_map(|e| e.ok()) {
                let p = entry.path();
                if is_reparse_point(&p) {
                    continue;
                }
                if let Ok(file_type) = entry.file_type() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if file_type.is_dir() {
                        if !ignored_dir_names.contains(name.as_str()) && !name.starts_with('.') {
                            dir_entries.push((p, name));
                        }
                    } else if file_type.is_file() {
                        file_names.push(name);
                    }
                }
            }

            // Check if current_dir is a project root
            if let Some(proj) = Self::inspect_project_at(&current_dir, &file_names, &dir_entries) {
                projects.push(proj);
            }

            // Push subdirectories
            for (sub_path, _sub_name) in dir_entries {
                // Do not descend deeply into nested structures if not a container
                if depth + 1 <= self.max_depth {
                    queue.push((sub_path, depth + 1));
                }
            }
        }

        // Sort projects by canonical path for deterministic presentation
        projects.sort_by(|a, b| a.canonical_path.cmp(&b.canonical_path));

        (projects, inaccessible)
    }

    pub(crate) fn inspect_project_at(
        dir: &Path,
        file_names: &[String],
        child_dirs: &[(PathBuf, String)],
    ) -> Option<RegisteredProject> {
        // Virtual environments are runtime caches, never ordinary managed projects.
        if dir.join("pyvenv.cfg").is_file() {
            return None;
        }

        let files_set: BTreeSet<&str> = file_names.iter().map(|s| s.as_str()).collect();

        let py_manifests: &[&str] = &["pyproject.toml", "requirements.txt", "Pipfile", "setup.py"];
        let rust_manifests: &[&str] = &["Cargo.toml"];
        let node_manifests: &[&str] = &["package.json"];
        let go_manifests: &[&str] = &["go.mod"];
        let ruby_manifests: &[&str] = &["Gemfile"];
        let java_manifests: &[&str] = &["pom.xml", "build.gradle", "build.gradle.kts"];
        let cpp_manifests: &[&str] = &["CMakeLists.txt", "Makefile"];

        let mut matched_manifests = Vec::new();
        let mut ecosystems = Vec::new();

        let check_category = |category: &str, manifests: &[&str], out_m: &mut Vec<String>, out_e: &mut Vec<String>| {
            let mut found = false;
            for m in manifests {
                if files_set.contains(m) {
                    out_m.push((*m).to_string());
                    found = true;
                }
            }
            if found {
                out_e.push(category.to_string());
            }
        };

        check_category("Python", py_manifests, &mut matched_manifests, &mut ecosystems);
        check_category("Rust", rust_manifests, &mut matched_manifests, &mut ecosystems);
        check_category("Node.js", node_manifests, &mut matched_manifests, &mut ecosystems);
        check_category("Go", go_manifests, &mut matched_manifests, &mut ecosystems);
        check_category("Ruby", ruby_manifests, &mut matched_manifests, &mut ecosystems);
        check_category("Java", java_manifests, &mut matched_manifests, &mut ecosystems);
        check_category("C/C++", cpp_manifests, &mut matched_manifests, &mut ecosystems);

        let connector_file = dir.join(".ulvo").join("manifest.toml");
        let has_connector = connector_file.is_file();
        if has_connector {
            matched_manifests.push(".ulvo/manifest.toml".to_string());
        }

        if matched_manifests.is_empty() {
            return None;
        }

        let canonical = strip_unc_prefix(&dir.canonicalize().unwrap_or(dir.to_path_buf()));
        let canonical_str = canonical.to_string_lossy().to_string();

        let project_name = canonical
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();

        // Calculate primary manifest fingerprint
        let primary_manifest_path = if has_connector {
            connector_file
        } else {
            dir.join(&matched_manifests[0])
        };

        let fingerprint = if let Ok(bytes) = fs::read(&primary_manifest_path) {
            let sample = if bytes.len() > 65536 { &bytes[..65536] } else { &bytes[..] };
            sha256_hex(sample)
        } else {
            "unreadable".to_string()
        };

        // Detect monorepo subprojects
        let mut subprojects = Vec::new();
        for (child_path, child_name) in child_dirs {
            let child_manifests = [
                "Cargo.toml", "pyproject.toml", "requirements.txt", "package.json",
                "go.mod", "pom.xml", "build.gradle", "CMakeLists.txt",
            ];
            let is_sub = child_manifests.iter().any(|m| child_path.join(m).is_file());
            if is_sub {
                subprojects.push(child_name.clone());
            } else if ["apps", "packages", "crates", "services"].contains(&child_name.as_str()) {
                if let Ok(nested) = fs::read_dir(child_path) {
                    for n in nested.filter_map(|e| e.ok()) {
                        let np = n.path();
                        if np.is_dir() && !is_reparse_point(&np) {
                            if child_manifests.iter().any(|m| np.join(m).is_file()) {
                                subprojects.push(format!("{}/{}", child_name, n.file_name().to_string_lossy()));
                            }
                        }
                    }
                }
            }
        }

        subprojects.sort();
        let is_monorepo = !subprojects.is_empty();

        let ecosystem_label = if ecosystems.is_empty() {
            "Custom ULVO".to_string()
        } else if ecosystems.len() == 1 {
            ecosystems[0].clone()
        } else {
            format!("Multi-Ecosystem ({})", ecosystems.join(", "))
        };

        let classification = Self::classify_project_dir(dir);
        let registration_state = if has_connector && classification == ProjectClassification::Standard {
            ProjectRegistrationState::Managed
        } else {
            ProjectRegistrationState::Discovered
        };

        let id = format!("proj_{}", &sha256_hex(canonical_str.as_bytes())[..16]);

        let now_utc = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Some(RegisteredProject {
            id,
            name: project_name,
            canonical_path: canonical_str,
            ecosystem: ecosystem_label,
            manifests: matched_manifests,
            manifest_fingerprint: fingerprint,
            discovery_source: DiscoverySource::WorkspaceScan,
            registration_state,
            has_connector,
            is_monorepo,
            subprojects,
            runtime_details: None,
            fs_identity: None,
            connector_status: None,
            runtime_requirements: None,
            classification,
            validation_warnings: Vec::new(),
            last_seen_utc: format!("{now_utc}"),
        })
    }

    /// Classifies a project directory so fixtures, templates, backups, and git
    /// worktrees are never presented as ordinary managed projects.
    fn classify_project_dir(dir: &Path) -> ProjectClassification {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        const FIXTURE_COMPONENTS: &[&str] = &["fixture", "fixtures", "testdata", "test_data", "test-fixtures"];
        let has_fixture_component = dir.components().any(|c| {
            let s = c.as_os_str().to_string_lossy().to_lowercase();
            FIXTURE_COMPONENTS.contains(&s.as_str())
        });
        if has_fixture_component || name.contains("fixture") || name.contains("testdata") {
            return ProjectClassification::TestFixture;
        }

        if name.contains("template") || name.contains("scaffold") {
            return ProjectClassification::Template;
        }

        if name.contains("backup") || name.ends_with(".bak") || name.ends_with(".old") || name.ends_with('~') {
            return ProjectClassification::BackupCopy;
        }

        if dir.join(".git").is_file() {
            return ProjectClassification::GitWorktree;
        }

        ProjectClassification::Standard
    }
}

pub struct ProjectRegistry {
    vault_root: PathBuf,
    registry_file: PathBuf,
    data: ProjectRegistryFile,
    // Exact snapshot used for optimistic concurrency under the OS file lock.
    persisted: RefCell<Option<Vec<u8>>>,
}

impl ProjectRegistry {
    pub fn load_or_create<P: AsRef<Path>>(vault_root: P, default_workspace: Option<PathBuf>) -> Result<Self, String> {
        let vr = vault_root.as_ref().to_path_buf();
        let reg_dir = vr.join("registry");
        let registry_file = reg_dir.join("projects.json");

        let default_ws = default_workspace
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let persisted = Self::read_snapshot(&registry_file)?;
        let data = if let Some(content) = &persisted {
            let parsed: ProjectRegistryFile = serde_json::from_slice(content)
                .map_err(|e| format!("invalid project registry {}; preserved for recovery: {e}", registry_file.display()))?;
            if parsed.schema_version != 1 {
                return Err(format!("unsupported project registry schema {}; preserved for recovery", parsed.schema_version));
            }
            parsed
        } else {
            ProjectRegistryFile {
                schema_version: 1,
                last_updated_utc: format!("{:?}", SystemTime::now()),
                workspace_root: default_ws,
                workspace_roots: Vec::new(),
                excluded_patterns: Vec::new(),
                projects: Vec::new(),
                inaccessible_paths: Vec::new(),
            }
        };

        Ok(Self {
            vault_root: vr,
            registry_file,
            data,
            persisted: RefCell::new(persisted),
        })
    }

    fn read_snapshot(path: &Path) -> Result<Option<Vec<u8>>, String> {
        match fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("cannot read project registry {}: {e}", path.display())),
        }
    }

    pub fn vault_root(&self) -> &Path {
        &self.vault_root
    }

    pub fn projects(&self) -> &[RegisteredProject] {
        &self.data.projects
    }

    pub fn workspace_root(&self) -> &str {
        &self.data.workspace_root
    }

    pub fn set_workspace_root(&mut self, root: &str) {
        self.data.workspace_root = root.to_string();
    }

    pub fn set_workspace_roots(&mut self, roots: &[PathBuf]) -> Result<(), String> {
        for root in roots { ulvo_adapters::validate_absolute_directory(root)?; }
        self.data.workspace_root = roots.first().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        self.data.workspace_roots = roots.iter().skip(1).map(|p| p.to_string_lossy().to_string()).collect();
        Ok(())
    }

    pub fn save(&self) -> Result<(), String> {
        let _lock = EnvironmentFileLock::acquire(&self.vault_root.join("registry/projects.lock"), 5000)?;
        let current = Self::read_snapshot(&self.registry_file)?;
        if current != *self.persisted.borrow() {
            return Err("project registry changed concurrently; reload and retry the operation".into());
        }
        let serialized = serde_json::to_string_pretty(&self.data)
            .map_err(|e| format!("cannot serialize project registry: {e}"))?;
        write_atomic(&self.registry_file, serialized.as_bytes())
            .map_err(|e| format!("failed to commit project registry to {}: {e}", self.registry_file.display()))?;
        *self.persisted.borrow_mut() = Some(serialized.into_bytes());
        Ok(())
    }

    /// Merges newly scanned projects with persistent registry while strictly preserving user registrations.
    pub fn sync_with_scanned(
        &mut self,
        scanned: Vec<RegisteredProject>,
        inaccessible: Vec<String>,
        active_paths: &[PathBuf],
    ) -> Result<(), String> {
        let active_set: BTreeSet<String> = active_paths
            .iter()
            .map(|p| strip_unc_prefix(p).to_string_lossy().to_lowercase())
            .collect();

        let mut existing_map: BTreeMap<String, RegisteredProject> = self
            .data
            .projects
            .drain(..)
            .map(|p| (p.canonical_path.to_lowercase(), p))
            .collect();

        // 1. Process scanned projects
        for mut s in scanned {
            let key = s.canonical_path.to_lowercase();
            if let Some(existing) = existing_map.remove(&key) {
                // Preserve user registration or active states
                let new_state = if active_set.contains(&key) {
                    ProjectRegistrationState::Active
                } else if existing.registration_state == ProjectRegistrationState::Registered {
                    ProjectRegistrationState::Registered
                } else if s.has_connector && s.classification == ProjectClassification::Standard {
                    ProjectRegistrationState::Managed
                } else {
                    ProjectRegistrationState::Discovered
                };

                let preserved_source = if existing.discovery_source == DiscoverySource::ExplicitRegistration {
                    DiscoverySource::ExplicitRegistration
                } else {
                    s.discovery_source
                };

                s.registration_state = new_state;
                s.discovery_source = preserved_source;
                s.id = existing.id; // Keep stable ID
            } else {
                if active_set.contains(&key) {
                    s.registration_state = ProjectRegistrationState::Active;
                }
            }
            self.data.projects.push(s);
        }

        // 2. Handle projects in registry that were not seen in this scan
        for (_, mut missing) in existing_map {
            let path = Path::new(&missing.canonical_path);
            if !path.exists() {
                missing.registration_state = ProjectRegistrationState::Missing;
            } else {
                let key = missing.canonical_path.to_lowercase();
                if active_set.contains(&key) {
                    missing.registration_state = ProjectRegistrationState::Active;
                } else if missing.has_connector && missing.classification == ProjectClassification::Standard {
                    missing.registration_state = ProjectRegistrationState::Managed;
                }
            }
            self.data.projects.push(missing);
        }

        self.data.projects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.data.inaccessible_paths = inaccessible;
        self.data.last_updated_utc = format!(
            "{}",
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
        );

        self.save()
    }

    /// Explicitly marks a project as registered by the user.
    pub fn register_project(&mut self, project_path: &Path) -> Result<RegisteredProject, String> {
        let clean = strip_unc_prefix(project_path);
        let raw_canonical = clean.canonicalize().map_err(|e| format!("path not found: {e}"))?;
        let canonical = strip_unc_prefix(&raw_canonical);
        let canon_str = canonical.to_string_lossy().to_string();
        let key = canon_str.to_lowercase();

        if let Some(pos) = self.data.projects.iter().position(|p| p.canonical_path.to_lowercase() == key) {
            let proj = &mut self.data.projects[pos];
            if proj.registration_state != ProjectRegistrationState::Active
                && proj.registration_state != ProjectRegistrationState::Managed
            {
                proj.registration_state = ProjectRegistrationState::Registered;
            }
            proj.discovery_source = DiscoverySource::ExplicitRegistration;
            let res = proj.clone();
            self.save()?;
            return Ok(res);
        }

        // Project not in registry: inspect and register
        let scanner = WorkspaceScanner::new(&canonical).with_max_depth(1);
        let (found, _) = scanner.scan();
        let mut proj = if let Some(first) = found.into_iter().next() {
            first
        } else {
            let project_name = canonical.file_name().and_then(|n| n.to_str()).unwrap_or("project").to_string();
            let id = format!("proj_{}", &sha256_hex(canon_str.as_bytes())[..16]);
            RegisteredProject {
                id,
                name: project_name,
                canonical_path: canon_str.clone(),
                ecosystem: "Custom / Generic".into(),
                manifests: vec![],
                manifest_fingerprint: "none".into(),
                discovery_source: DiscoverySource::ExplicitRegistration,
                registration_state: ProjectRegistrationState::Registered,
                has_connector: false,
                is_monorepo: false,
                subprojects: vec![],
                runtime_details: None,
                fs_identity: None,
                connector_status: None,
                runtime_requirements: None,
                classification: ProjectClassification::Standard,
                validation_warnings: vec![],
                last_seen_utc: format!("{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()),
            }
        };

        proj.discovery_source = DiscoverySource::ExplicitRegistration;
        if !proj.has_connector {
            proj.registration_state = ProjectRegistrationState::Registered;
        }
        let res = proj.clone();
        self.data.projects.push(proj);
        self.data.projects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.save()?;
        Ok(res)
    }

    /// Explicitly provisions an onboarding thin connector (.ulvo/manifest.toml) and updates registry state.
    pub fn onboard_project(&mut self, project_path: &Path) -> Result<RegisteredProject, String> {
        let clean = strip_unc_prefix(project_path);
        let raw_canonical = clean.canonicalize().map_err(|e| format!("path not found: {e}"))?;
        let canonical = strip_unc_prefix(&raw_canonical);
        let ulvo_dir = canonical.join(".ulvo");
        let manifest_path = ulvo_dir.join("manifest.toml");

        if !manifest_path.is_file() {
            fs::create_dir_all(&ulvo_dir).map_err(|e| format!("cannot create .ulvo directory: {e}"))?;
            let project_name = canonical.file_name().and_then(|n| n.to_str()).unwrap_or("project").to_string();
            let default_manifest = format!(
r#"[project]
name = "{project_name}"
version = "0.1.0"
description = "ULVO Managed Project"

[runtime]
ecosystem = "python"
version = "3.11"

[execution]
lifecycle = "ephemeral"
gpu = false

[dependencies]
manifest = "requirements.txt"
"#
            );
            fs::write(&manifest_path, default_manifest).map_err(|e| format!("cannot write manifest: {e}"))?;
        }

        let key = canonical.to_string_lossy().to_lowercase();
        if let Some(pos) = self.data.projects.iter().position(|p| p.canonical_path.to_lowercase() == key) {
            let proj = &mut self.data.projects[pos];
            proj.has_connector = true;
            if proj.registration_state != ProjectRegistrationState::Active {
                proj.registration_state = ProjectRegistrationState::Managed;
            }
            if !proj.manifests.iter().any(|m| m == ".ulvo/manifest.toml") {
                proj.manifests.push(".ulvo/manifest.toml".to_string());
            }
            let res = proj.clone();
            self.save()?;
            return Ok(res);
        }

        // If not present, register then onboard
        let mut proj = self.register_project(&canonical)?;
        proj.has_connector = true;
        proj.registration_state = ProjectRegistrationState::Managed;
        if !proj.manifests.iter().any(|m| m == ".ulvo/manifest.toml") {
            proj.manifests.push(".ulvo/manifest.toml".to_string());
        }
        self.save()?;
        Ok(proj)
    }

    pub fn workspace_roots(&self) -> &[String] {
        &self.data.workspace_roots
    }

    pub fn excluded_patterns(&self) -> &[String] {
        &self.data.excluded_patterns
    }

    /// Applies a single filesystem watcher event to the in-memory registry.
    ///
    /// Does NOT persist — callers batch events and call [`save`] once per batch
    /// to keep write amplification low during bursts.
    pub fn apply_watch_event(&mut self, ev: &FsWatchEvent) -> Result<WatchReconcileOutcome, String> {
        match ev.kind {
            FsWatchKind::Overflow => Ok(WatchReconcileOutcome::FullRescanRequired),
            FsWatchKind::Created | FsWatchKind::Modified => {
                match Self::resolve_watch_target(&ev.path) {
                    Some(dir) => self.upsert_project_dir(&dir),
                    None => Ok(WatchReconcileOutcome::Unchanged),
                }
            }
            FsWatchKind::Deleted => {
                // Only the exact registered root path can be marked missing by a
                // deletion; deletions inside a project trigger a parent re-inspect.
                if self.mark_missing_exact(&ev.path) {
                    return Ok(WatchReconcileOutcome::Updated);
                }
                if let Some(parent) = ev.path.parent() {
                    let parent_key = parent.to_string_lossy().to_lowercase();
                    if self
                        .data
                        .projects
                        .iter()
                        .any(|p| p.canonical_path.to_lowercase() == parent_key)
                    {
                        return self.upsert_project_dir(parent);
                    }
                }
                Ok(WatchReconcileOutcome::Unchanged)
            }
            FsWatchKind::Renamed => {
                let mut changed = false;
                if let Some(old_path) = ev.old_path.as_deref() {
                    if self.mark_missing_exact(old_path) {
                        changed = true;
                    } else if let Some(old_parent) = old_path.parent() {
                        let key = old_parent.to_string_lossy().to_lowercase();
                        if self
                            .data
                            .projects
                            .iter()
                            .any(|p| p.canonical_path.to_lowercase() == key)
                        {
                            if matches!(self.upsert_project_dir(old_parent)?, WatchReconcileOutcome::Updated) {
                                changed = true;
                            }
                        }
                    }
                }
                if let Some(new_dir) = Self::resolve_watch_target(&ev.path) {
                    if matches!(self.upsert_project_dir(&new_dir)?, WatchReconcileOutcome::Updated) {
                        changed = true;
                    }
                }
                Ok(if changed {
                    WatchReconcileOutcome::Updated
                } else {
                    WatchReconcileOutcome::Unchanged
                })
            }
        }
    }

    /// Resolves an event path to the project-root directory it concerns:
    /// the directory itself when it is a dir, otherwise its parent (file events).
    fn resolve_watch_target(path: &Path) -> Option<PathBuf> {
        if path.is_dir() {
            Some(strip_unc_prefix(path))
        } else {
            path.parent().map(|p| strip_unc_prefix(p))
        }
    }

    fn path_matches_excluded(&self, path: &Path) -> bool {
        if self.data.excluded_patterns.is_empty() {
            return false;
        }
        let path_str = path.to_string_lossy().to_lowercase();
        self.data
            .excluded_patterns
            .iter()
            .any(|pat| !pat.is_empty() && path_str.contains(&pat.to_lowercase()))
    }

    /// Marks the entry whose canonical_path equals `path` as Missing.
    /// Returns true if the registry was changed.
    fn mark_missing_exact(&mut self, path: &Path) -> bool {
        let key = strip_unc_prefix(path).to_string_lossy().to_lowercase();
        if let Some(pos) = self
            .data
            .projects
            .iter()
            .position(|p| p.canonical_path.to_lowercase() == key)
        {
            let proj = &mut self.data.projects[pos];
            if proj.registration_state != ProjectRegistrationState::Missing {
                proj.registration_state = ProjectRegistrationState::Missing;
                return true;
            }
        }
        false
    }

    /// Inserts or refreshes a project registration for the given directory.
    fn upsert_project_dir(&mut self, dir: &Path) -> Result<WatchReconcileOutcome, String> {
        let clean = strip_unc_prefix(dir);
        if clean.ancestors().any(is_reparse_point) {
            return Ok(WatchReconcileOutcome::Unchanged);
        }
        if self.path_matches_excluded(&clean) {
            return Ok(WatchReconcileOutcome::Unchanged);
        }

        // Environments never become managed projects.
        if clean.join("pyvenv.cfg").is_file() {
            return Ok(WatchReconcileOutcome::Unchanged);
        }

        let canonical = match clean.canonicalize() {
            Ok(c) => strip_unc_prefix(&c),
            Err(_) => {
                // Directory vanished between event and reconcile.
                return Ok(if self.mark_missing_exact(&clean) {
                    WatchReconcileOutcome::Updated
                } else {
                    WatchReconcileOutcome::Unchanged
                });
            }
        };

        let mut file_names = Vec::new();
        let mut child_dirs = Vec::new();
        if let Ok(entries) = fs::read_dir(&canonical) {
            for entry in entries.filter_map(|e| e.ok()) {
                if is_reparse_point(&entry.path()) {
                    continue;
                }
                if let Ok(file_type) = entry.file_type() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if file_type.is_dir() {
                        child_dirs.push((entry.path(), name));
                    } else {
                        file_names.push(name);
                    }
                }
            }
        }

        let inspected = WorkspaceScanner::inspect_project_at(&canonical, &file_names, &child_dirs);
        let key = canonical.to_string_lossy().to_lowercase();
        let pos = self
            .data
            .projects
            .iter()
            .position(|p| p.canonical_path.to_lowercase() == key);

        match (inspected, pos) {
            (Some(mut proj), Some(idx)) => {
                let existing = &self.data.projects[idx];
                let keep_explicit = existing.discovery_source == DiscoverySource::ExplicitRegistration;
                let keep_user_state = matches!(
                    existing.registration_state,
                    ProjectRegistrationState::Registered
                        | ProjectRegistrationState::Active
                        | ProjectRegistrationState::Managed
                );
                proj.id = existing.id.clone();
                proj.discovery_source = if keep_explicit {
                    DiscoverySource::ExplicitRegistration
                } else {
                    DiscoverySource::DynamicWatcher
                };
                proj.registration_state = if keep_user_state && !matches!(existing.registration_state, ProjectRegistrationState::Managed) {
                    existing.registration_state.clone()
                } else if proj.has_connector && proj.classification == ProjectClassification::Standard {
                    ProjectRegistrationState::Managed
                } else {
                    ProjectRegistrationState::Discovered
                };
                // Non-standard classifications are never ordinary managed projects.
                if proj.classification != ProjectClassification::Standard
                    && proj.registration_state == ProjectRegistrationState::Managed
                {
                    proj.registration_state = ProjectRegistrationState::Discovered;
                }
                self.data.projects[idx] = proj;
                self.data.last_updated_utc = now_secs();
                Ok(WatchReconcileOutcome::Updated)
            }
            (Some(mut proj), None) => {
                proj.discovery_source = DiscoverySource::DynamicWatcher;
                if proj.classification != ProjectClassification::Standard {
                    proj.registration_state = ProjectRegistrationState::Discovered;
                }
                self.data.projects.push(proj);
                self.data
                    .projects
                    .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                self.data.last_updated_utc = now_secs();
                Ok(WatchReconcileOutcome::Updated)
            }
            (None, Some(idx)) => {
                // Directory exists but no longer looks like a project.
                let proj = &mut self.data.projects[idx];
                if proj.registration_state != ProjectRegistrationState::Missing {
                    proj.registration_state = ProjectRegistrationState::Missing;
                    self.data.last_updated_utc = now_secs();
                    return Ok(WatchReconcileOutcome::Updated);
                }
                Ok(WatchReconcileOutcome::Unchanged)
            }
            (None, None) => Ok(WatchReconcileOutcome::Unchanged),
        }
    }
}

fn now_secs() -> String {
    format!(
        "{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
    )
}

/// Result of applying watcher events to the registry in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchReconcileOutcome {
    Unchanged,
    Updated,
    FullRescanRequired,
}
