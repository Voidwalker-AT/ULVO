use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use ulvo_core::{
    Abi, Architecture, Backend, Ecosystem, InstalledPackage,
    MigrationCompatibility, Ownership, PackageDuplication, Platform,
    ProjectCandidate, Provenance, PythonEnvironment, Resource, ResourceKind,
    StorageSavingsEstimate, StorageSavingsReport, VerificationStatus,
};
use crate::{
    binary_architecture, bounded_read, query_file_identity_and_size, reject_links,
    run_bounded, sha256_hex, FileSystemIdentity,
};

pub fn find_uv_executable() -> Option<PathBuf> {
    // 1. Check local user directory
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        let candidate = PathBuf::from(&userprofile).join(".local/bin/uv.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    // 2. Check scoop or AppData
    if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
        let candidate = PathBuf::from(&localappdata).join("uv/uv.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    // 3. Check PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("uv.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn find_uv_cache_dir() -> Option<PathBuf> {
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        // Scoop persist directory
        let scoop_cache = PathBuf::from(&userprofile).join("scoop/persist/uv/cache");
        if scoop_cache.is_dir() {
            return Some(scoop_cache);
        }
    }
    if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
        let default_cache = PathBuf::from(&localappdata).join("uv/cache");
        if default_cache.is_dir() {
            return Some(default_cache);
        }
    }
    None
}

pub fn find_external_pythons() -> Vec<(PathBuf, String)> {
    let mut results = Vec::new();
    if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
        let programs = PathBuf::from(&localappdata).join("Programs/Python");
        if programs.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&programs) {
                for entry in entries.flatten() {
                    let exe = entry.path().join("python.exe");
                    if exe.is_file() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let ver = name.strip_prefix("Python").unwrap_or(&name).to_string();
                        results.push((exe, ver));
                    }
                }
            }
        }
    }
    results
}

pub fn discover_python_resources(vault_root: &Path) -> Result<Vec<Resource>, String> {
    reject_links(vault_root)?;
    let mut resources = Vec::new();

    // 1. Managed Python runtimes in vault/runtimes/python
    let managed_python_dir = vault_root.join("runtimes").join("python");
    if managed_python_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&managed_python_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() && !is_reparse_point(&p) {
                    let exe = p.join("python.exe");
                    let status = if exe.is_file() {
                        match binary_architecture(&exe) {
                            Ok(arch) if arch == "x86_64" => VerificationStatus::Discovered,
                            _ => VerificationStatus::Modified,
                        }
                    } else {
                        VerificationStatus::Missing
                    };
                    let ver = entry.file_name().to_string_lossy().to_string();
                    resources.push(Resource::new(
                        ResourceKind::LanguageRuntime,
                        Ecosystem::Python,
                        "python",
                        Some(ver),
                        Platform::Windows,
                        Architecture::X86_64,
                        Backend::Windows,
                        Abi::None,
                        Some(exe.to_string_lossy().to_string()),
                        Some("ulvo".into()),
                        Ownership::Managed,
                        status,
                        None,
                        vec!["interpreter".into(), "managed_runtime".into()],
                    ));
                }
            }
        }
    }

    // 2. Discover uv package manager
    if let Some(uv_path) = find_uv_executable() {
        let uv_status = match binary_architecture(&uv_path) {
            Ok(arch) if arch == "x86_64" => VerificationStatus::Discovered,
            _ => VerificationStatus::Modified,
        };

        let uv_ver = match run_bounded(&uv_path, &["--version"], Duration::from_secs(2)) {
            Ok(out) => {
                // e.g. "uv 0.13.1 (..."
                out.split_whitespace().nth(1).map(str::to_string)
            }
            Err(_) => None,
        };

        let cache_dir_opt = find_uv_cache_dir();
        let notes = cache_dir_opt.as_ref().map(|c| format!("Cache directory: {}", c.display()));

        resources.push(Resource::new(
            ResourceKind::PackageManager,
            Ecosystem::Python,
            "uv",
            uv_ver,
            Platform::Windows,
            Architecture::X86_64,
            Backend::Windows,
            Abi::Msvc,
            Some(uv_path.to_string_lossy().to_string()),
            None,
            Ownership::External,
            uv_status,
            Some(Provenance {
                source_type: "external_tool".into(),
                canonical_url: None,
                expected_sha256: None,
                actual_sha256: None,
                manifest_path: None,
                notes,
            }),
            vec!["package_manager".into(), "resolver".into(), "fast_installer".into()],
        ));

        // 3. Discover uv cache store if present
        if let Some(cache_dir) = cache_dir_opt {
            resources.push(Resource::new(
                ResourceKind::PackageStore,
                Ecosystem::Python,
                "uv-cache",
                None,
                Platform::Windows,
                Architecture::X86_64,
                Backend::Windows,
                Abi::None,
                Some(cache_dir.to_string_lossy().to_string()),
                Some("uv".into()),
                Ownership::External,
                VerificationStatus::Discovered,
                None,
                vec!["package_store".into(), "wheel_cache".into()],
            ));
        }
    }

    // 4. Discover external Python interpreters as references
    for (py_path, py_ver) in find_external_pythons() {
        let status = match binary_architecture(&py_path) {
            Ok(arch) if arch == "x86_64" => VerificationStatus::Discovered,
            _ => VerificationStatus::Modified,
        };
        resources.push(Resource::new(
            ResourceKind::ExternalToolReference,
            Ecosystem::Python,
            format!("python-{py_ver}"),
            Some(py_ver),
            Platform::Windows,
            Architecture::X86_64,
            Backend::Windows,
            Abi::None,
            Some(py_path.to_string_lossy().to_string()),
            None,
            Ownership::External,
            status,
            Some(Provenance {
                source_type: "host_system_installation".into(),
                canonical_url: None,
                expected_sha256: None,
                actual_sha256: None,
                manifest_path: None,
                notes: Some("Host Python detected in AppData".into()),
            }),
            vec!["interpreter".into(), "external_reference".into()],
        ));
    }

    Ok(resources)
}

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

pub fn parse_pyvenv_cfg_str(content: &str) -> (Option<String>, Option<String>) {
    let mut version = None;
    let mut home = None;
    for line in content.lines() {
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim().to_lowercase();
            let val = v.trim().to_string();
            if key == "version" || key == "version_info" {
                version = Some(val);
            } else if key == "home" || key == "executable" {
                home = Some(val);
            }
        }
    }
    (version, home)
}

pub fn scan_single_python_env(env_path: &Path, workspace_root: &Path) -> PythonEnvironment {
    let rel_path = env_path.strip_prefix(workspace_root).unwrap_or(env_path);
    let category = rel_path.components().next().map(|c| c.as_os_str().to_string_lossy().to_string())
        .unwrap_or_else(|| "root".into());

    let mut env = PythonEnvironment {
        path: env_path.to_string_lossy().to_string(),
        relative_path: rel_path.to_string_lossy().to_string(),
        category,
        env_type: "venv".into(),
        python_version: None,
        base_interpreter: None,
        logical_bytes: 0,
        physical_allocated_bytes: Some(0),
        physical_unique_files: 0,
        has_editable: false,
        has_native: false,
        packages: Vec::new(),
        inaccessible_count: 0,
    };

    let pyvenv = env_path.join("pyvenv.cfg");
    if pyvenv.is_file() {
        if let Ok(text) = bounded_read(&pyvenv) {
            let (ver, home) = parse_pyvenv_cfg_str(&text);
            env.python_version = ver;
            env.base_interpreter = home;
        }
    }

    if env_path.join("conda-meta").is_dir() {
        env.env_type = "conda".into();
    }

    // Locate site-packages
    let mut sp_dirs = Vec::new();
    let lib_candidate = env_path.join("Lib");
    let lib_candidate_lower = env_path.join("lib");
    let target_lib = if lib_candidate.is_dir() { lib_candidate } else { lib_candidate_lower };

    if target_lib.is_dir() {
        find_site_packages(&target_lib, &mut sp_dirs);
    }

    // Inspect site-packages
    for sp in &sp_dirs {
        scan_site_packages(sp, env_path, &mut env);
    }

    // Measure total environment size
    let mut identities = HashSet::new();
    let mut alloc_acc = Some(0u64);
    measure_dir(env_path, &mut env.logical_bytes, &mut alloc_acc, &mut identities, &mut env.inaccessible_count);
    env.physical_unique_files = identities.len();
    env.physical_allocated_bytes = alloc_acc;

    env
}

fn find_site_packages(dir: &Path, sp_dirs: &mut Vec<PathBuf>) {
    if is_reparse_point(dir) {
        return;
    }
    if dir.file_name().and_then(|n| n.to_str()) == Some("site-packages") {
        sp_dirs.push(dir.to_path_buf());
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() && !is_reparse_point(&p) {
                find_site_packages(&p, sp_dirs);
            }
        }
    }
}

fn scan_site_packages(sp: &Path, env_path: &Path, env: &mut PythonEnvironment) {
    // Check .pth files in site-packages for editable installs or external source references
    if let Ok(entries) = std::fs::read_dir(sp) {
        for entry in entries.flatten() {
            let p = entry.path();
            let fname = entry.file_name().to_string_lossy().to_string();
            if p.is_file() && fname.ends_with(".pth") {
                if fname.starts_with("__editable__") {
                    env.has_editable = true;
                }
                if let Ok(content) = bounded_read(&p) {
                    for line in content.lines() {
                        let l = line.trim();
                        if l.starts_with("import ") || l.contains("__editable__") {
                            env.has_editable = true;
                        } else if !l.is_empty() && !l.starts_with('#') {
                            let ref_path = PathBuf::from(l);
                            if ref_path.is_absolute() && !ref_path.starts_with(env_path) {
                                env.has_editable = true;
                            }
                        }
                    }
                }
            }
        }
    }

    let Ok(entries) = std::fs::read_dir(sp) else { return; };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if p.is_dir() && name.ends_with(".dist-info") {
            let (pkg_name, pkg_ver) = parse_dist_info_name(&name);

            // 1. Check editable install via direct_url.json
            let mut is_editable = false;
            let direct_url_file = p.join("direct_url.json");
            if direct_url_file.is_file() {
                if let Ok(content) = bounded_read(&direct_url_file) {
                    if content.contains("\"editable\": true")
                        || content.contains("\"editable\":true")
                        || content.contains("\"dir_info\"")
                    {
                        is_editable = true;
                        env.has_editable = true;
                    }
                }
            }

            // 2. Extract ABI tag from WHEEL
            let mut abi_tag: Option<String> = None;
            let wheel_file = p.join("WHEEL");
            if wheel_file.is_file() {
                if let Ok(content) = bounded_read(&wheel_file) {
                    for line in content.lines() {
                        let l = line.trim();
                        if let Some(tag_val) = l.strip_prefix("Tag:").or_else(|| l.strip_prefix("tag:")) {
                            abi_tag = Some(tag_val.trim().to_string());
                            break;
                        }
                    }
                }
            }

            // 3. Compute cryptographic content hash from RECORD or METADATA
            let record_file = p.join("RECORD");
            let content_hash = if record_file.is_file() {
                if let Ok(rec_content) = bounded_read(&record_file) {
                    Some(format!("sha256:{}", sha256_hex(rec_content.as_bytes())))
                } else {
                    None
                }
            } else {
                let metadata_file = p.join("METADATA");
                if metadata_file.is_file() {
                    if let Ok(meta_content) = bounded_read(&metadata_file) {
                        Some(format!("sha256-meta:{}", sha256_hex(meta_content.as_bytes())))
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            // 4. Measure package files
            let mut pkg_bytes = 0;
            let mut pkg_alloc = Some(0u64);
            let mut pkg_identities = HashSet::new();
            measure_dir(&p, &mut pkg_bytes, &mut pkg_alloc, &mut pkg_identities, &mut env.inaccessible_count);

            // Read top_level.txt to discover modules associated with this distribution
            let top_level_file = p.join("top_level.txt");
            let mut top_names = Vec::new();
            if top_level_file.is_file() {
                if let Ok(text) = bounded_read(&top_level_file) {
                    for line in text.lines() {
                        let t = line.trim();
                        if !t.is_empty() {
                            top_names.push(t.to_string());
                        }
                    }
                }
            }
            if top_names.is_empty() {
                top_names.push(pkg_name.replace('-', "_"));
            }

            let mut has_native = false;
            for mod_name in top_names {
                let mod_dir = sp.join(&mod_name);
                let mod_file = sp.join(format!("{}.py", mod_name));
                if mod_dir.is_dir() && !is_reparse_point(&mod_dir) {
                    measure_dir(&mod_dir, &mut pkg_bytes, &mut pkg_alloc, &mut pkg_identities, &mut env.inaccessible_count);
                    if check_has_native(&mod_dir) {
                        has_native = true;
                    }
                } else if mod_file.is_file() && !is_reparse_point(&mod_file) {
                    if let Ok((id, info)) = query_file_identity_and_size(&mod_file) {
                        pkg_bytes += info.logical_bytes;
                        if pkg_identities.insert(id) {
                            if let (Some(ref mut a), Some(alloc)) = (&mut pkg_alloc, info.allocated_bytes) {
                                *a += alloc;
                            }
                        }
                    }
                }
            }

            if has_native {
                env.has_native = true;
            }

            env.packages.push(InstalledPackage {
                name: pkg_name,
                version: pkg_ver,
                is_editable,
                has_native,
                logical_bytes: pkg_bytes,
                unique_files: pkg_identities.len(),
                content_hash,
                abi_tag,
            });
        }
    }
}

fn parse_dist_info_name(name: &str) -> (String, String) {
    let trimmed = name.strip_suffix(".dist-info").unwrap_or(name);
    if let Some((pkg, ver)) = trimmed.split_once('-') {
        (pkg.to_lowercase(), ver.to_string())
    } else {
        (trimmed.to_lowercase(), "unknown".into())
    }
}

fn check_has_native(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false; };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() {
            if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                let ext_l = ext.to_lowercase();
                if ext_l == "pyd" || ext_l == "dll" || ext_l == "so" {
                    return true;
                }
            }
        } else if p.is_dir() && !is_reparse_point(&p) {
            if check_has_native(&p) {
                return true;
            }
        }
    }
    false
}

fn measure_dir(
    dir: &Path,
    bytes: &mut u64,
    allocated_acc: &mut Option<u64>,
    identities: &mut HashSet<FileSystemIdentity>,
    inaccessible: &mut usize,
) {
    if is_reparse_point(dir) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        *inaccessible += 1;
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if is_reparse_point(&p) {
            continue;
        }
        if p.is_file() {
            match query_file_identity_and_size(&p) {
                Ok((identity, size_info)) => {
                    if size_info.is_reparse {
                        continue;
                    }
                    *bytes += size_info.logical_bytes;
                    if identities.insert(identity) {
                        if let Some(ref mut acc) = allocated_acc {
                            if let Some(alloc) = size_info.allocated_bytes {
                                *acc += alloc;
                            } else {
                                *allocated_acc = None;
                            }
                        }
                    }
                }
                Err(_) => {
                    *inaccessible += 1;
                }
            }
        } else if p.is_dir() {
            measure_dir(&p, bytes, allocated_acc, identities, inaccessible);
        }
    }
}

pub fn analyze_package_duplication(envs: &[PythonEnvironment]) -> Vec<PackageDuplication> {
    let mut instances: HashMap<(String, String), Vec<(&InstalledPackage, &PythonEnvironment)>> = HashMap::new();

    for env in envs {
        for pkg in &env.packages {
            let key = (pkg.name.clone(), pkg.version.clone());
            instances.entry(key).or_default().push((pkg, env));
        }
    }

    let mut dups = Vec::new();
    for ((pkg_name, pkg_ver), list) in instances {
        if list.len() > 1 {
            let unit_size = list.iter().map(|(p, _)| p.logical_bytes).max().unwrap_or(0);
            let has_native = list.iter().any(|(p, _)| p.has_native);
            let envs_list: Vec<String> = list.iter().map(|(_, e)| e.relative_path.clone()).collect();

            let mut abi_tags: Vec<String> = list.iter()
                .filter_map(|(p, _)| p.abi_tag.clone())
                .collect();
            abi_tags.sort();
            abi_tags.dedup();

            // Verify byte-level identity via content_hash
            // All instances must have Some(hash), and all hashes must match
            let all_have_hash = list.iter().all(|(p, _)| p.content_hash.is_some());
            let first_hash = list[0].0.content_hash.as_deref();
            let all_hashes_match = all_have_hash
                && list.iter().all(|(p, _)| p.content_hash.as_deref() == first_hash);

            let is_verified_identical = all_hashes_match && (abi_tags.len() <= 1);

            // Calculate total wasted bytes strictly for verified identical copies
            let total_wasted_bytes = if is_verified_identical {
                unit_size * (list.len() as u64 - 1)
            } else {
                // If not all are identical, sum wasted bytes only among identical subsets
                let mut hash_counts: HashMap<&str, usize> = HashMap::new();
                for (p, _) in &list {
                    if let Some(ref h) = p.content_hash {
                        *hash_counts.entry(h.as_str()).or_default() += 1;
                    }
                }
                let mut wasted = 0;
                for (_h, count) in hash_counts {
                    if count > 1 {
                        wasted += unit_size * (count as u64 - 1);
                    }
                }
                wasted
            };

            dups.push(PackageDuplication {
                package: pkg_name,
                version: pkg_ver,
                instance_count: list.len(),
                estimated_unit_bytes: unit_size,
                total_wasted_bytes,
                has_native,
                is_verified_identical,
                abi_tags,
                environments: envs_list,
            });
        }
    }

    dups.sort_by_key(|d| std::cmp::Reverse(d.total_wasted_bytes));
    dups
}

pub fn calculate_storage_savings(
    total_workspace_bytes: u64,
    envs: &[PythonEnvironment],
    dups: &[PackageDuplication],
) -> StorageSavingsReport {
    // 1. Conservative: Sum wasted bytes strictly from verified identical packages
    let conservative_bytes: u64 = dups.iter()
        .filter(|d| d.is_verified_identical)
        .map(|d| d.total_wasted_bytes)
        .sum();

    // 2. Realistic: Reclaim eligible ephemeral environments, PLUS deduplicate
    // packages strictly within the remaining persistent environments (zero double counting!)
    let mut ephemeral_bytes: u64 = 0;
    let mut persistent_envs: Vec<PythonEnvironment> = Vec::new();

    for env in envs {
        // Shared base environments are persistent; environments with editable packages must stay persistent
        if env.category.eq_ignore_ascii_case("Shared") || env.has_editable {
            persistent_envs.push(env.clone());
        } else {
            ephemeral_bytes += env.logical_bytes;
        }
    }

    let persistent_dups = analyze_package_duplication(&persistent_envs);
    let persistent_dedup_bytes: u64 = persistent_dups.iter()
        .filter(|d| d.is_verified_identical)
        .map(|d| d.total_wasted_bytes)
        .sum();

    // 3. Theoretical: Complete reclamation of all non-shared project environments
    // plus full package-store deduplication across shared base environments
    let theoretical_project_bytes: u64 = envs.iter()
        .filter(|e| !e.category.eq_ignore_ascii_case("Shared"))
        .map(|e| e.logical_bytes)
        .sum();
    let theoretical_bytes = (theoretical_project_bytes + persistent_dedup_bytes).min(total_workspace_bytes);

    let realistic_bytes = (ephemeral_bytes + persistent_dedup_bytes).max(conservative_bytes).min(theoretical_bytes);

    let to_gb = |b: u64| (b as f64) / (1024.0 * 1024.0 * 1024.0);
    let to_pct = |b: u64| {
        if total_workspace_bytes == 0 { 0.0 }
        else { (((b as f64) / (total_workspace_bytes as f64)) * 100.0).min(100.0) }
    };

    StorageSavingsReport {
        conservative: StorageSavingsEstimate {
            recoverable_bytes: conservative_bytes,
            recoverable_gb: (to_gb(conservative_bytes) * 100.0).round() / 100.0,
            percentage_of_workspace: (to_pct(conservative_bytes) * 100.0).round() / 100.0,
            method: "Package-store deduplication of verified byte-identical packages across environments".into(),
            risk: "Low".into(),
            confidence: "Moderate (verified via package content hash comparison)".into(),
            assumptions: vec![
                "Deduplication requires byte-identical distributions verified by package content hashes".into(),
                "Target environments support hardlink-based or symlink-based package store sharing".into(),
                "Preserves virtual environment directory isolation while reclaiming duplicate physical bytes".into(),
            ],
        },
        realistic: StorageSavingsEstimate {
            recoverable_bytes: realistic_bytes,
            recoverable_gb: (to_gb(realistic_bytes) * 100.0).round() / 100.0,
            percentage_of_workspace: (to_pct(realistic_bytes) * 100.0).round() / 100.0,
            method: "uv-managed shared cache for reproducible project environments plus package dedup for persistent environments".into(),
            risk: "Moderate".into(),
            confidence: "Moderate (projected based on non-editable project environments and reproducible manifests)".into(),
            assumptions: vec![
                "Projects with reproducible manifests and no editable installs transition to on-demand ephemeral environments".into(),
                "Environments with local editable packages or base shared runtimes remain persistent".into(),
                "Cross-environment package deduplication is applied strictly to persistent environments to prevent double-counting".into(),
                "Eliminated arbitrary heuristic percentage multipliers".into(),
            ],
        },
        theoretical: StorageSavingsEstimate {
            recoverable_bytes: theoretical_bytes,
            recoverable_gb: (to_gb(theoretical_bytes) * 100.0).round() / 100.0,
            percentage_of_workspace: (to_pct(theoretical_bytes) * 100.0).round() / 100.0,
            method: "Complete reclamation of all project-level virtual environments into a centralized package store".into(),
            risk: "High (assumes all projects run purely on-demand with zero persistent venvs)".into(),
            confidence: "Low (theoretical upper bound; requires complete tooling adaptation)".into(),
            assumptions: vec![
                "Upper bound assuming 100% of project environments are converted to ephemeral runtimes".into(),
                "Zero persistent virtual environment disk footprint outside runtime store".into(),
                "All toolchains and IDEs configured for on-demand ephemeral virtual environment activation".into(),
            ],
        },
    }
}

pub fn classify_project_candidate(
    project_path: &Path,
    project_name: &str,
    category: &str,
    sentinels: Vec<String>,
    associated_envs: &[&PythonEnvironment],
) -> ProjectCandidate {
    let mut risk_factors = Vec::new();
    let mut has_editable = false;
    let mut has_torch = false;
    let mut has_pyside = false;
    let mut total_env_bytes = 0;

    for env in associated_envs {
        total_env_bytes += env.logical_bytes;
        if env.has_editable {
            has_editable = true;
            risk_factors.push("Contains editable package installs".into());
        }
        for pkg in &env.packages {
            if pkg.name == "torch" || pkg.name.starts_with("nvidia-") {
                has_torch = true;
            }
            if pkg.name.starts_with("pyside") || pkg.name.starts_with("pyqt") {
                has_pyside = true;
            }
        }
    }

    if has_torch {
        risk_factors.push("Heavy PyTorch / CUDA runtime dependency".into());
    }
    if has_pyside {
        risk_factors.push("Desktop GUI framework (PySide/Qt)".into());
    }

    let compatibility = if associated_envs.is_empty() {
        MigrationCompatibility::Unknown
    } else if has_editable {
        MigrationCompatibility::RequiresPersistentEnv
    } else if has_torch || has_pyside {
        MigrationCompatibility::LikelyCompatible
    } else if category.eq_ignore_ascii_case("Academic") || category.eq_ignore_ascii_case("Experiments") {
        MigrationCompatibility::ReadyForPilot
    } else {
        MigrationCompatibility::LikelyCompatible
    };

    ProjectCandidate {
        path: project_path.to_string_lossy().to_string(),
        name: project_name.to_string(),
        category: category.to_string(),
        sentinels,
        compatibility,
        risk_factors,
        estimated_env_bytes: total_env_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pyvenv_cfg() {
        let content = "home = C:\\Python312\nversion = 3.12.2\ninclude-system-site-packages = false\n";
        let (ver, home) = parse_pyvenv_cfg_str(content);
        assert_eq!(ver.as_deref(), Some("3.12.2"));
        assert_eq!(home.as_deref(), Some("C:\\Python312"));
    }

    #[test]
    fn dist_info_parsing() {
        let (pkg, ver) = parse_dist_info_name("torch-2.6.0+cu124.dist-info");
        assert_eq!(pkg, "torch");
        assert_eq!(ver, "2.6.0+cu124");
    }

    #[test]
    fn deduplication_analysis_calculation() {
        let env1 = PythonEnvironment {
            path: "E:/Workspace/env1".into(),
            relative_path: "env1".into(),
            category: "test".into(),
            env_type: "venv".into(),
            python_version: Some("3.12".into()),
            base_interpreter: None,
            logical_bytes: 5000,
            physical_allocated_bytes: Some(8192),
            physical_unique_files: 5,
            has_editable: false,
            has_native: true,
            packages: vec![InstalledPackage {
                name: "numpy".into(),
                version: "2.2.0".into(),
                is_editable: false,
                has_native: true,
                logical_bytes: 4000,
                unique_files: 4,
                content_hash: Some("sha256:abcd1234".into()),
                abi_tag: Some("cp312-cp312-win_amd64".into()),
            }],
            inaccessible_count: 0,
        };
        let mut env2 = env1.clone();
        env2.relative_path = "env2".into();

        let dups = analyze_package_duplication(&[env1, env2]);
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].package, "numpy");
        assert_eq!(dups[0].instance_count, 2);
        assert_eq!(dups[0].total_wasted_bytes, 4000);
        assert!(dups[0].is_verified_identical);
        assert_eq!(dups[0].abi_tags, vec!["cp312-cp312-win_amd64".to_string()]);
    }

    #[test]
    fn deduplication_detects_abi_and_content_mismatch() {
        let env1 = PythonEnvironment {
            path: "E:/Workspace/env1".into(),
            relative_path: "env1".into(),
            category: "test".into(),
            env_type: "venv".into(),
            python_version: Some("3.11".into()),
            base_interpreter: None,
            logical_bytes: 5000,
            physical_allocated_bytes: Some(8192),
            physical_unique_files: 5,
            has_editable: false,
            has_native: true,
            packages: vec![InstalledPackage {
                name: "numpy".into(),
                version: "2.2.0".into(),
                is_editable: false,
                has_native: true,
                logical_bytes: 4000,
                unique_files: 4,
                content_hash: Some("sha256:hash_cp311".into()),
                abi_tag: Some("cp311-cp311-win_amd64".into()),
            }],
            inaccessible_count: 0,
        };
        let mut env2 = env1.clone();
        env2.relative_path = "env2".into();
        env2.packages[0].content_hash = Some("sha256:hash_cp312".into());
        env2.packages[0].abi_tag = Some("cp312-cp312-win_amd64".into());

        let dups = analyze_package_duplication(&[env1, env2]);
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].package, "numpy");
        assert_eq!(dups[0].instance_count, 2);
        // Different ABIs and different content hashes -> not verified identical, 0 wasted bytes
        assert!(!dups[0].is_verified_identical);
        assert_eq!(dups[0].total_wasted_bytes, 0);
        assert_eq!(dups[0].abi_tags.len(), 2);
    }

    #[test]
    fn test_native_wheel_incompatibility() {
        let env_native = PythonEnvironment {
            path: "E:/Workspace/env_native".into(),
            relative_path: "env_native".into(),
            category: "test".into(),
            env_type: "venv".into(),
            python_version: Some("3.11".into()),
            base_interpreter: None,
            logical_bytes: 20000,
            physical_allocated_bytes: Some(24576),
            physical_unique_files: 10,
            has_editable: false,
            has_native: true,
            packages: vec![InstalledPackage {
                name: "cffi".into(),
                version: "1.17.1".into(),
                is_editable: false,
                has_native: true,
                logical_bytes: 18000,
                unique_files: 8,
                content_hash: Some("sha256:native_cffi_binary_hash".into()),
                abi_tag: Some("cp311-cp311-win_amd64".into()),
            }],
            inaccessible_count: 0,
        };

        let env_pure = PythonEnvironment {
            path: "E:/Workspace/env_pure".into(),
            relative_path: "env_pure".into(),
            category: "test".into(),
            env_type: "venv".into(),
            python_version: Some("3.12".into()),
            base_interpreter: None,
            logical_bytes: 10000,
            physical_allocated_bytes: Some(12288),
            physical_unique_files: 6,
            has_editable: false,
            has_native: false,
            packages: vec![InstalledPackage {
                name: "cffi".into(),
                version: "1.17.1".into(),
                is_editable: false,
                has_native: false,
                logical_bytes: 8000,
                unique_files: 5,
                content_hash: Some("sha256:pure_cffi_wheel_hash".into()),
                abi_tag: Some("py3-none-any".into()),
            }],
            inaccessible_count: 0,
        };

        let dups = analyze_package_duplication(&[env_native, env_pure]);
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].package, "cffi");
        assert!(dups[0].has_native);
        assert!(!dups[0].is_verified_identical, "Native wheel and pure wheel cannot be verified identical");
        assert_eq!(dups[0].total_wasted_bytes, 0, "Incompatible wheels must not be claimed as reclaimable duplicate bytes");
        assert_eq!(dups[0].abi_tags, vec!["cp311-cp311-win_amd64".to_string(), "py3-none-any".to_string()]);
    }
}
