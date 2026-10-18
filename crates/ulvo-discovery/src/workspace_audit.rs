use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use ulvo_core::{
    fnv1a_64, CategoryStats, ProjectCandidate, PythonEnvironment,
    WorkspaceAuditReport,
};
use ulvo_adapters::{
    analyze_package_duplication, calculate_storage_savings, classify_project_candidate,
    reject_links, scan_single_python_env,
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
    let mut total_files: usize = 0;
    let mut global_inodes = HashSet::new();
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
            if let Ok(meta) = top_path.metadata() {
                let sz = meta.len();
                total_logical_bytes += sz;
                total_files += 1;
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    global_inodes.insert((meta.file_size(), fnv1a_64(top_path.to_string_lossy().to_lowercase().as_bytes())));
                }
                #[cfg(not(windows))]
                {
                    use std::os::unix::fs::MetadataExt;
                    global_inodes.insert((meta.dev(), meta.ino()));
                }
            }
            continue;
        }

        if top_path.is_dir() {
            let mut cat_bytes: u64 = 0;
            let mut cat_files: usize = 0;
            let mut cat_inodes = HashSet::new();
            let mut cat_inaccessible: usize = 0;

            walk_category(
                &top_path,
                workspace_root,
                &top_name,
                &mut cat_bytes,
                &mut cat_files,
                &mut cat_inodes,
                &mut cat_inaccessible,
                &mut python_envs,
                &mut project_candidates,
            );

            total_logical_bytes += cat_bytes;
            total_files += cat_files;
            global_inodes.extend(cat_inodes.iter().cloned());

            let to_gb = (cat_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
            category_map.insert(top_name, CategoryStats {
                logical_bytes: cat_bytes,
                logical_gb: (to_gb * 100.0).round() / 100.0,
                file_count: cat_files,
                unique_physical_files: cat_inodes.len(),
                inaccessible_count: cat_inaccessible,
            });
        }
    }

    python_envs.sort_by_key(|e| std::cmp::Reverse(e.logical_bytes));

    let package_duplications = analyze_package_duplication(&python_envs);
    let storage_savings = calculate_storage_savings(total_logical_bytes, &python_envs, &package_duplications);

    let total_unique = global_inodes.len();
    let hardlink_count = total_files.saturating_sub(total_unique);
    let total_logical_gb = ((total_logical_bytes as f64) / (1024.0 * 1024.0 * 1024.0) * 100.0).round() / 100.0;

    // Physical allocated bytes on NTFS (unique files)
    let total_physical_allocated_bytes = total_logical_bytes; // Bound
    let total_physical_allocated_gb = total_logical_gb;

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
    cat_files: &mut usize,
    cat_inodes: &mut HashSet<(u64, u64)>,
    inaccessible: &mut usize,
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
            subdirs.push((name, p));
        } else if p.is_file() {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            files.insert(name);
            if let Ok(meta) = p.metadata() {
                *cat_bytes += meta.len();
                *cat_files += 1;
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    cat_inodes.insert((meta.file_size(), fnv1a_64(p.to_string_lossy().to_lowercase().as_bytes())));
                }
                #[cfg(not(windows))]
                {
                    use std::os::unix::fs::MetadataExt;
                    cat_inodes.insert((meta.dev(), meta.ino()));
                }
            } else {
                *inaccessible += 1;
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
        walk_category(&subdir_path, root, category, cat_bytes, cat_files, cat_inodes, inaccessible, envs, projects);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_empty_audit() {
        let temp = std::env::temp_dir().join("ulvo_synthetic_workspace_audit");
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();

        let report = audit_workspace(&temp).unwrap();
        assert_eq!(report.total_files, 0);
        assert_eq!(report.total_logical_bytes, 0);
        assert_eq!(report.python_environments.len(), 0);

        let _ = std::fs::remove_dir_all(&temp);
    }
}
