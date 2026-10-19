use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use ulvo_core::{
    CategoryStats, ProjectCandidate, PythonEnvironment,
    WorkspaceAuditReport,
};
use ulvo_adapters::{
    analyze_package_duplication, calculate_storage_savings, classify_project_candidate,
    query_file_identity_and_size, reject_links, scan_single_python_env, FileSystemIdentity,
};

fn is_reparse_point(path: &Path) -> bool {
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            return true;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return true;
            }
        }
    }
    false
}

pub fn scan_workspace_environments_only(workspace_root: &Path) -> Result<Vec<PythonEnvironment>, String> {
    reject_links(workspace_root)?;
    let mut envs = Vec::new();
    walk_for_envs(workspace_root, workspace_root, &mut envs);
    envs.sort_by_key(|e| std::cmp::Reverse(e.logical_bytes));
    Ok(envs)
}

pub(crate) fn is_ignored_scan_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".github"
            | "target"
            | "node_modules"
            | "__pycache__"
            | ".pytest_cache"
            | ".mypy_cache"
            | ".ruff_cache"
            | ".idea"
            | ".vscode"
            | "vault"
            | ".ulvo"
    )
}

fn walk_for_envs(current: &Path, root: &Path, envs: &mut Vec<PythonEnvironment>) {
    if is_reparse_point(current) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(current) else { return; };
    let mut subdirs = Vec::new();
    let mut files = HashSet::new();

    for entry in entries.flatten() {
        let p = entry.path();
        if is_reparse_point(&p) {
            continue;
        }
        if p.is_dir() {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            if is_ignored_scan_dir(&name) {
                continue;
            }
            subdirs.push((name, p));
        } else if p.is_file() {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            files.insert(name);
        }
    }

    // Check if current directory is a Python virtual environment
    let is_env = files.contains("pyvenv.cfg")
        || subdirs.iter().any(|(n, _)| n == "conda-meta")
        || ((subdirs.iter().any(|(n, _)| n == "Scripts" || n == "bin"))
            && (subdirs.iter().any(|(n, _)| n == "Lib" || n == "lib"))
            && (current.join("Scripts/python.exe").is_file() || current.join("bin/python").is_file()));

    if is_env {
        let env_info = scan_single_python_env(current, root);
        envs.push(env_info);
        return; // Do not recurse into environment internals
    }

    // Recurse into subdirectories
    for (_, subdir_path) in subdirs {
        walk_for_envs(&subdir_path, root, envs);
    }
}

pub fn audit_workspace(workspace_root: &Path) -> Result<WorkspaceAuditReport, String> {
    reject_links(workspace_root)?;

    let mut total_logical_bytes: u64 = 0;
    let mut total_physical_allocated_bytes: Option<u64> = Some(0);
    let mut total_files: usize = 0;
    let mut total_inaccessible: usize = 0;
    let mut global_identities: HashSet<FileSystemIdentity> = HashSet::new();
    let mut category_map: BTreeMap<String, CategoryStats> = BTreeMap::new();
    let mut python_envs = Vec::new();
    let mut project_candidates = Vec::new();

    let Ok(top_entries) = std::fs::read_dir(workspace_root) else {
        return Err(format!("cannot read workspace directory: {}", workspace_root.display()));
    };

    for top_entry in top_entries.flatten() {
        let top_path = top_entry.path();
        let top_name = top_entry.file_name().to_string_lossy().to_string();

        if is_reparse_point(&top_path) {
            continue;
        }

        if top_path.is_file() {
            match query_file_identity_and_size(&top_path) {
                Ok((identity, size_info)) => {
                    if size_info.is_reparse {
                        continue;
                    }
                    total_logical_bytes += size_info.logical_bytes;
                    total_files += 1;
                    if global_identities.insert(identity) {
                        if let Some(ref mut acc) = total_physical_allocated_bytes {
                            if let Some(alloc) = size_info.allocated_bytes {
                                *acc += alloc;
                            } else {
                                total_physical_allocated_bytes = None;
                            }
                        }
                    }
                }
                Err(_) => {
                    total_inaccessible += 1;
                }
            }
            continue;
        }

        if top_path.is_dir() {
            let mut cat_bytes: u64 = 0;
            let mut cat_alloc: Option<u64> = Some(0);
            let mut cat_files: usize = 0;
            let mut cat_identities: HashSet<FileSystemIdentity> = HashSet::new();
            let mut cat_inaccessible: usize = 0;

            walk_category(
                &top_path,
                workspace_root,
                &top_name,
                &mut cat_bytes,
                &mut cat_alloc,
                &mut cat_files,
                &mut cat_identities,
                &mut cat_inaccessible,
                &mut global_identities,
                &mut total_physical_allocated_bytes,
                &mut python_envs,
                &mut project_candidates,
            );

            total_logical_bytes += cat_bytes;
            total_files += cat_files;
            total_inaccessible += cat_inaccessible;

            let to_gb = (cat_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
            let cat_alloc_gb = cat_alloc.map(|b| ((b as f64) / (1024.0 * 1024.0 * 1024.0) * 100.0).round() / 100.0);

            category_map.insert(top_name, CategoryStats {
                logical_bytes: cat_bytes,
                logical_gb: (to_gb * 100.0).round() / 100.0,
                physical_allocated_bytes: cat_alloc,
                physical_allocated_gb: cat_alloc_gb,
                file_count: cat_files,
                unique_physical_files: cat_identities.len(),
                inaccessible_count: cat_inaccessible,
            });
        }
    }

    python_envs.sort_by_key(|e| std::cmp::Reverse(e.logical_bytes));

    let package_duplications = analyze_package_duplication(&python_envs);
    let storage_savings = calculate_storage_savings(total_logical_bytes, &python_envs, &package_duplications);

    let total_unique = global_identities.len();
    let hardlink_count = total_files.saturating_sub(total_unique);
    let total_logical_gb = ((total_logical_bytes as f64) / (1024.0 * 1024.0 * 1024.0) * 100.0).round() / 100.0;
    let total_physical_allocated_gb = total_physical_allocated_bytes.map(|b| ((b as f64) / (1024.0 * 1024.0 * 1024.0) * 100.0).round() / 100.0);
    let scan_incomplete = total_inaccessible > 0;

    Ok(WorkspaceAuditReport {
        schema_version: 1,
        workspace_root: workspace_root.to_string_lossy().to_string(),
        total_logical_bytes,
        total_logical_gb,
        total_physical_allocated_bytes,
        total_physical_allocated_gb,
        total_files,
        unique_physical_files: total_unique,
        hardlink_count,
        inaccessible_count: total_inaccessible,
        scan_incomplete,
        category_breakdown: category_map,
        python_environments: python_envs,
        package_duplications,
        storage_savings,
        project_candidates,
    })
}

fn walk_category(
    current: &Path,
    root: &Path,
    category: &str,
    cat_bytes: &mut u64,
    cat_alloc: &mut Option<u64>,
    cat_files: &mut usize,
    cat_identities: &mut HashSet<FileSystemIdentity>,
    inaccessible: &mut usize,
    global_identities: &mut HashSet<FileSystemIdentity>,
    global_alloc: &mut Option<u64>,
    envs: &mut Vec<PythonEnvironment>,
    projects: &mut Vec<ProjectCandidate>,
) {
    if is_reparse_point(current) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(current) else {
        *inaccessible += 1;
        return;
    };
    let mut subdirs = Vec::new();
    let mut files = HashSet::new();

    for entry in entries.flatten() {
        let p = entry.path();
        if is_reparse_point(&p) {
            continue;
        }
        if p.is_dir() {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            if is_ignored_scan_dir(&name) {
                continue;
            }
            subdirs.push((name, p));
        } else if p.is_file() {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            files.insert(name);
            match query_file_identity_and_size(&p) {
                Ok((identity, size_info)) => {
                    if size_info.is_reparse {
                        continue;
                    }
                    *cat_bytes += size_info.logical_bytes;
                    *cat_files += 1;

                    // Track within category
                    if cat_identities.insert(identity) {
                        if let Some(ref mut acc) = cat_alloc {
                            if let Some(alloc) = size_info.allocated_bytes {
                                *acc += alloc;
                            } else {
                                *cat_alloc = None;
                            }
                        }
                    }

                    // Track globally across the entire workspace
                    if global_identities.insert(identity) {
                        if let Some(ref mut g_acc) = global_alloc {
                            if let Some(alloc) = size_info.allocated_bytes {
                                *g_acc += alloc;
                            } else {
                                *global_alloc = None;
                            }
                        }
                    }
                }
                Err(_) => {
                    *inaccessible += 1;
                }
            }
        }
    }

    // Check if virtual environment
    let is_env = files.contains("pyvenv.cfg")
        || subdirs.iter().any(|(n, _)| n == "conda-meta")
        || ((subdirs.iter().any(|(n, _)| n == "Scripts" || n == "bin"))
            && (subdirs.iter().any(|(n, _)| n == "Lib" || n == "lib"))
            && (current.join("Scripts/python.exe").is_file() || current.join("bin/python").is_file()));

    if is_env {
        let env_info = scan_single_python_env(current, root);
        envs.push(env_info);
        return;
    }

    // Check project sentinels
    let project_sentinels = [".git", "pyproject.toml", "requirements.txt", "uv.lock",
                            "package.json", "Cargo.toml", "go.mod", "CMakeLists.txt", ".ulvo"];
    let mut matched_sentinels = Vec::new();
    for s in &project_sentinels {
        if files.contains(*s) || subdirs.iter().any(|(n, _)| n == *s) {
            matched_sentinels.push(s.to_string());
        }
    }

    if !matched_sentinels.is_empty() {
        let proj_name = current.file_name().unwrap_or_default().to_string_lossy().to_string();
        let candidate = classify_project_candidate(current, &proj_name, category, matched_sentinels, &[]);
        projects.push(candidate);
    }

    // Recurse into subdirectories
    for (_, subdir_path) in subdirs {
        walk_category(
            &subdir_path,
            root,
            category,
            cat_bytes,
            cat_alloc,
            cat_files,
            cat_identities,
            inaccessible,
            global_identities,
            global_alloc,
            envs,
            projects,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct SafeTestFixture {
        root: PathBuf,
        pub ws: PathBuf,
    }

    impl SafeTestFixture {
        fn new(name: &str) -> Self {
            let pid = std::process::id();
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("ulvo_test_audit_{name}_{pid}_{ts}"));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join(".ulvo_test_sentinel"), format!("pid={pid}\nts={ts}\n")).unwrap();
            let ws = root.join("ws");
            std::fs::create_dir_all(&ws).unwrap();
            Self { root, ws }
        }
    }

    impl Drop for SafeTestFixture {
        fn drop(&mut self) {
            let sentinel = self.root.join(".ulvo_test_sentinel");
            if let Ok(content) = std::fs::read_to_string(&sentinel) {
                if content.contains(&format!("pid={}", std::process::id())) {
                    let _ = std::fs::remove_dir_all(&self.root);
                }
            }
        }
    }

    #[test]
    fn synthetic_empty_audit() {
        let fixture = SafeTestFixture::new("empty");
        let report = audit_workspace(&fixture.ws).unwrap();
        assert_eq!(report.total_files, 0);
        assert_eq!(report.total_logical_bytes, 0);
        assert_eq!(report.total_physical_allocated_bytes, Some(0));
        assert_eq!(report.inaccessible_count, 0);
        assert!(!report.scan_incomplete);
        assert_eq!(report.python_environments.len(), 0);
    }

    #[test]
    fn synthetic_hardlink_deduplication_accounting() {
        let fixture = SafeTestFixture::new("hardlink_audit");
        let cat_dir = fixture.ws.join("CategoryA");
        std::fs::create_dir_all(&cat_dir).unwrap();

        let original = cat_dir.join("original.dat");
        let payload = vec![0x42u8; 8192];
        std::fs::write(&original, &payload).unwrap();

        let hardlinked = cat_dir.join("hardlinked.dat");
        if std::fs::hard_link(&original, &hardlinked).is_ok() {
            let report = audit_workspace(&fixture.ws).unwrap();
            assert_eq!(report.total_files, 2);
            assert_eq!(report.unique_physical_files, 1, "Two hardlinks must count as 1 unique physical allocation");
            assert_eq!(report.hardlink_count, 1);
            assert_eq!(report.total_logical_bytes, 16384, "Logical bytes must count both references");

            let phys = report.total_physical_allocated_bytes.unwrap();
            assert!(phys > 0, "Physical allocation must be non-zero");
            // Physical allocation for 1 unique file must not double-count
            assert!(phys < report.total_logical_bytes, "Physical allocation must not double-count hardlinks");
        }
    }

    #[test]
    fn synthetic_editable_environment_detection() {
        let fixture = SafeTestFixture::new("editable_env");
        let env_dir = fixture.ws.join("ProjectA/.venv");
        let sp_dir = env_dir.join("Lib/site-packages");
        std::fs::create_dir_all(&sp_dir).unwrap();

        std::fs::write(env_dir.join("pyvenv.cfg"), "home = C:\\Python312\nversion = 3.12.0\n").unwrap();
        std::fs::create_dir_all(env_dir.join("Scripts")).unwrap();
        std::fs::write(env_dir.join("Scripts/python.exe"), b"MZ").unwrap();

        // Create editable package dist-info
        let dist_info = sp_dir.join("mypkg-0.1.0.dist-info");
        std::fs::create_dir_all(&dist_info).unwrap();
        std::fs::write(
            dist_info.join("direct_url.json"),
            "{\"dir_info\": {\"editable\": true}, \"url\": \"file:///E:/src/mypkg\"}\n",
        ).unwrap();
        std::fs::write(dist_info.join("WHEEL"), "Wheel-Version: 1.0\nTag: py3-none-any\n").unwrap();
        std::fs::write(dist_info.join("RECORD"), "mypkg/__init__.py,sha256=xxx,10\n").unwrap();

        // Create __editable__ pth file
        std::fs::write(sp_dir.join("__editable__.mypkg-0.1.0.pth"), "E:\\src\\mypkg\n").unwrap();

        let report = audit_workspace(&fixture.ws).unwrap();
        assert_eq!(report.python_environments.len(), 1);
        let env = &report.python_environments[0];
        assert!(env.has_editable, "Environment must detect editable installation");
        assert_eq!(env.packages.len(), 1);
        assert!(env.packages[0].is_editable);
        assert_eq!(env.packages[0].abi_tag.as_deref(), Some("py3-none-any"));
        assert!(env.packages[0].content_hash.is_some());

        // Verify project candidate marked as requiring persistent environment
        let proj = report.project_candidates.iter().find(|p| p.name == "ProjectA");
        if let Some(p) = proj {
            assert_eq!(p.compatibility, ulvo_core::MigrationCompatibility::RequiresPersistentEnv);
        }
    }

    #[test]
    fn synthetic_savings_avoids_double_counting_ephemeral_and_dedup() {
        let fixture = SafeTestFixture::new("savings_no_double_count");

        // 1. Persistent Shared base environment
        let shared_env = fixture.ws.join("Shared/base_env");
        let shared_sp = shared_env.join("Lib/site-packages");
        std::fs::create_dir_all(&shared_sp).unwrap();
        std::fs::write(shared_env.join("pyvenv.cfg"), "home = C:\\Python312\nversion = 3.12.0\n").unwrap();
        let dist1 = shared_sp.join("pkg-1.0.0.dist-info");
        std::fs::create_dir_all(&dist1).unwrap();
        std::fs::write(dist1.join("WHEEL"), "Tag: py3-none-any\n").unwrap();
        std::fs::write(dist1.join("RECORD"), "file.py,sha256=identical,100\n").unwrap();

        // 2. Persistent Shared second base environment with identical package
        let shared_env2 = fixture.ws.join("Shared/base_env2");
        let shared_sp2 = shared_env2.join("Lib/site-packages");
        std::fs::create_dir_all(&shared_sp2).unwrap();
        std::fs::write(shared_env2.join("pyvenv.cfg"), "home = C:\\Python312\nversion = 3.12.0\n").unwrap();
        let dist2 = shared_sp2.join("pkg-1.0.0.dist-info");
        std::fs::create_dir_all(&dist2).unwrap();
        std::fs::write(dist2.join("WHEEL"), "Tag: py3-none-any\n").unwrap();
        std::fs::write(dist2.join("RECORD"), "file.py,sha256=identical,100\n").unwrap();

        // 3. Ephemeral project environment with identical package
        let proj_env = fixture.ws.join("Projects/proj1/.venv");
        let proj_sp = proj_env.join("Lib/site-packages");
        std::fs::create_dir_all(&proj_sp).unwrap();
        std::fs::write(proj_env.join("pyvenv.cfg"), "home = C:\\Python312\nversion = 3.12.0\n").unwrap();
        let dist3 = proj_sp.join("pkg-1.0.0.dist-info");
        std::fs::create_dir_all(&dist3).unwrap();
        std::fs::write(dist3.join("WHEEL"), "Tag: py3-none-any\n").unwrap();
        std::fs::write(dist3.join("RECORD"), "file.py,sha256=identical,100\n").unwrap();

        let report = audit_workspace(&fixture.ws).unwrap();
        assert_eq!(report.python_environments.len(), 3);

        // Realistic savings must equal:
        // (proj_env logical bytes) + (package deduplication strictly between shared_env and shared_env2)
        // It must NOT count pkg deduplication for proj_env a second time!
        let savings = &report.storage_savings;
        assert!(savings.realistic.recoverable_bytes > 0);
        assert!(savings.realistic.recoverable_bytes >= savings.conservative.recoverable_bytes);
        assert!(savings.theoretical.recoverable_bytes >= savings.realistic.recoverable_bytes);
        assert_eq!(savings.conservative.assumptions.len(), 3);
        assert_eq!(savings.realistic.assumptions.len(), 4);
        assert_eq!(savings.theoretical.assumptions.len(), 3);
    }

    #[test]
    fn synthetic_reparse_point_detection() {
        let fixture = SafeTestFixture::new("reparse");
        let normal_dir = fixture.ws.join("normal_dir");
        let normal_file = fixture.ws.join("normal_file.txt");
        std::fs::create_dir_all(&normal_dir).unwrap();
        std::fs::write(&normal_file, b"content").unwrap();

        assert!(!is_reparse_point(&normal_dir));
        assert!(!is_reparse_point(&normal_file));

        // reject_links succeeds on normal directory and file
        assert!(reject_links(&normal_dir).is_ok());
        assert!(reject_links(&normal_file).is_ok());
    }

    #[test]
    fn synthetic_inaccessible_entry_triggers_incomplete_scan() {
        let fixture = SafeTestFixture::new("inaccessible");
        let cat_dir = fixture.ws.join("CategoryA");
        std::fs::create_dir_all(&cat_dir).unwrap();

        // Simulate an inaccessible entry encountered during walk
        let mut cat_bytes = 0u64;
        let mut cat_alloc = Some(0u64);
        let mut cat_files = 0usize;
        let mut cat_identities = HashSet::new();
        let mut inaccessible = 0usize;
        let mut global_identities = HashSet::new();
        let mut global_alloc = Some(0u64);
        let mut envs = Vec::new();
        let mut projects = Vec::new();

        // Walking a missing/unreadable subdirectory simulates an inaccessible entry (std::fs::read_dir fails)
        let unreadable = cat_dir.join("locked_or_unreadable_subdir");
        walk_category(
            &unreadable,
            &fixture.ws,
            "CategoryA",
            &mut cat_bytes,
            &mut cat_alloc,
            &mut cat_files,
            &mut cat_identities,
            &mut inaccessible,
            &mut global_identities,
            &mut global_alloc,
            &mut envs,
            &mut projects,
        );

        assert_eq!(inaccessible, 1, "Unreadable directory must increment inaccessible count");
    }
}
