use std::path::{Path, PathBuf};
use std::process::Command;
use ulvo_core::migration::*;
use ulvo_core::ProjectManifest;
use crate::{
    bounded_read, calculate_physical_dir_size, is_reparse_point,
    safe_reclaim_environment_hierarchy, write_atomic,
};

#[cfg(windows)]
mod win32_proc {
    use std::ffi::c_void;
    pub type HANDLE = *mut c_void;
    pub type DWORD = u32;
    pub type BOOL = i32;
    pub type LPWSTR = *mut u16;

    pub const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
    pub const TH32CS_SNAPPROCESS: DWORD = 0x00000002;
    pub const PROCESS_QUERY_LIMITED_INFORMATION: DWORD = 0x1000;
    pub const MAX_PATH: usize = 260;

    #[repr(C)]
    #[allow(non_snake_case)]
    pub struct PROCESSENTRY32W {
        pub dwSize: DWORD,
        pub cntUsage: DWORD,
        pub th32ProcessID: DWORD,
        pub th32DefaultHeapID: usize,
        pub th32ModuleID: DWORD,
        pub cntThreads: DWORD,
        pub th32ParentProcessID: DWORD,
        pub pcPriClassBase: i32,
        pub dwFlags: DWORD,
        pub szExeFile: [u16; MAX_PATH],
    }

    extern "system" {
        pub fn CreateToolhelp32Snapshot(dwFlags: DWORD, th32ProcessID: DWORD) -> HANDLE;
        pub fn Process32FirstW(hSnapshot: HANDLE, lppe: *mut PROCESSENTRY32W) -> BOOL;
        pub fn Process32NextW(hSnapshot: HANDLE, lppe: *mut PROCESSENTRY32W) -> BOOL;
        pub fn OpenProcess(dwDesiredAccess: DWORD, bInheritHandle: BOOL, dwProcessId: DWORD) -> HANDLE;
        pub fn QueryFullProcessImageNameW(
            hProcess: HANDLE,
            dwFlags: DWORD,
            lpExeName: LPWSTR,
            lpdwSize: *mut DWORD,
        ) -> BOOL;
        pub fn CloseHandle(hObject: HANDLE) -> BOOL;
    }
}

/// Inspect dependencies, reproducibility, and ML characteristics for a project candidate
pub fn inspect_candidate_dependencies(
    project_dir: &Path,
    env_path: &Path,
) -> (DependencyReproducibility, Vec<String>, bool) {
    let mut sources = Vec::new();
    let mut is_ml = false;

    // Check for lockfiles
    let uv_lock = project_dir.join("uv.lock");
    let poetry_lock = project_dir.join("poetry.lock");
    let pipfile_lock = project_dir.join("Pipfile.lock");
    let pdm_lock = project_dir.join("pdm.lock");
    let req_lock = project_dir.join("requirements.lock");
    let req_lock_txt = project_dir.join("requirements-lock.txt");
    let req_release_lock = project_dir.join("requirements-release.lock");

    let has_lock = if uv_lock.is_file() {
        sources.push("uv.lock".to_string());
        true
    } else if poetry_lock.is_file() {
        sources.push("poetry.lock".to_string());
        true
    } else if pipfile_lock.is_file() {
        sources.push("Pipfile.lock".to_string());
        true
    } else if pdm_lock.is_file() {
        sources.push("pdm.lock".to_string());
        true
    } else if req_lock.is_file() {
        sources.push("requirements.lock".to_string());
        true
    } else if req_lock_txt.is_file() {
        sources.push("requirements-lock.txt".to_string());
        true
    } else if req_release_lock.is_file() {
        sources.push("requirements-release.lock".to_string());
        true
    } else {
        // Also check any file ending in .lock
        let mut found = false;
        if let Ok(entries) = std::fs::read_dir(project_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.ends_with(".lock") && !name.starts_with('.') {
                        sources.push(name);
                        found = true;
                        break;
                    }
                }
            }
        }
        found
    };

    // Check for manifests
    let req_txt = project_dir.join("requirements.txt");
    let req_dev = project_dir.join("requirements-dev.txt");
    let pyproject = project_dir.join("pyproject.toml");
    let pipfile = project_dir.join("Pipfile");
    let setup_py = project_dir.join("setup.py");

    let mut has_manifest = false;
    if req_txt.is_file() {
        sources.push("requirements.txt".to_string());
        has_manifest = true;
    }
    if req_dev.is_file() {
        sources.push("requirements-dev.txt".to_string());
        has_manifest = true;
    }
    if pyproject.is_file() {
        sources.push("pyproject.toml".to_string());
        has_manifest = true;
    }
    if pipfile.is_file() {
        sources.push("Pipfile".to_string());
        has_manifest = true;
    }
    if setup_py.is_file() {
        sources.push("setup.py".to_string());
        has_manifest = true;
    }

    // Inspect files and environment packages for ML libraries
    let ml_keywords = [
        "torch", "torchvision", "torchaudio", "tensorflow", "tensorboard",
        "keras", "jax", "jaxlib", "nvidia-", "triton", "cuda", "scipy", "transformers", "accelerate"
    ];

    // Read sources if present
    for src in &sources {
        let p = project_dir.join(src);
        if let Ok(content) = bounded_read(&p) {
            let lower = content.to_lowercase();
            for kw in &ml_keywords {
                if lower.contains(kw) {
                    is_ml = true;
                    break;
                }
            }
        }
    }

    // Also check installed packages in the environment's site-packages
    let site_packages_dirs = [
        env_path.join("Lib").join("site-packages"),
        env_path.join("lib").join("site-packages"),
    ];

    let mut found_dist_info = false;
    for sp in &site_packages_dirs {
        if sp.is_dir() {
            if let Ok(entries) = std::fs::read_dir(sp) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if name.ends_with(".dist-info") {
                        found_dist_info = true;
                        for kw in &ml_keywords {
                            if name.starts_with(kw) {
                                is_ml = true;
                            }
                        }
                    }
                }
            }
        }
    }

    let reproducibility = if has_lock {
        DependencyReproducibility::LockfilePinned
    } else if has_manifest {
        DependencyReproducibility::ManifestSpecified
    } else if found_dist_info {
        DependencyReproducibility::DerivedFromInstalled
    } else {
        DependencyReproducibility::NonReproducible
    };

    (reproducibility, sources, is_ml)
}

/// Extract installed package names and versions from an environment's site-packages
pub fn extract_installed_packages(env_path: &Path) -> Vec<String> {
    let site_packages_dirs = [
        env_path.join("Lib").join("site-packages"),
        env_path.join("lib").join("site-packages"),
    ];

    let mut pkgs = Vec::new();
    for sp in &site_packages_dirs {
        if !sp.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(sp) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".dist-info") {
                    let dist_path = entry.path();
                    let stem = &name[..name.len() - 10];
                    if let Some(pos) = stem.rfind('-') {
                        let pkg_name = &stem[..pos];
                        let pkg_version = &stem[pos + 1..];
                        if !matches!(pkg_name, "pip" | "setuptools" | "wheel") {
                            // Check for PEP 610 direct_url.json
                            let direct_url_file = dist_path.join("direct_url.json");
                            let mut direct_handled = false;
                            if direct_url_file.is_file() {
                                if let Ok(json_str) = bounded_read(&direct_url_file) {
                                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                                        if let Some(url) = val.get("url").and_then(|u| u.as_str()) {
                                            if let Some(vcs_info) = val.get("vcs_info") {
                                                let vcs = vcs_info.get("vcs").and_then(|v| v.as_str()).unwrap_or("git");
                                                let commit = vcs_info.get("commit_id").and_then(|c| c.as_str());
                                                if let Some(c) = commit {
                                                    pkgs.push(format!("{pkg_name} @ {vcs}+{url}@{c}"));
                                                } else {
                                                    pkgs.push(format!("{pkg_name} @ {vcs}+{url}"));
                                                }
                                                direct_handled = true;
                                            } else {
                                                pkgs.push(format!("{pkg_name} @ {url}"));
                                                direct_handled = true;
                                            }
                                        }
                                    }
                                }
                            }
                            if !direct_handled {
                                pkgs.push(format!("{pkg_name}=={pkg_version}"));
                            }
                        }
                    }
                }
            }
        }
    }
    pkgs.sort();
    pkgs.dedup();
    pkgs
}

/// Detect Python version from pyvenv.cfg, .python-version, or pyproject.toml
pub fn detect_python_version(project_dir: &Path, env_path: &Path) -> Option<String> {
    // 1. pyvenv.cfg
    let cfg_path = env_path.join("pyvenv.cfg");
    if cfg_path.is_file() {
        if let Ok(content) = bounded_read(&cfg_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("version") {
                    if let Some((_, val)) = trimmed.split_once('=') {
                        let ver = val.trim().to_string();
                        if !ver.is_empty() {
                            return Some(ver);
                        }
                    }
                }
            }
        }
    }

    // 2. .python-version
    let pv_path = project_dir.join(".python-version");
    if pv_path.is_file() {
        if let Ok(content) = bounded_read(&pv_path) {
            let ver = content.trim().to_string();
            if !ver.is_empty() {
                return Some(ver);
            }
        }
    }

    None
}

/// Atomically generate `.ulvo/manifest.toml` for a project without altering legacy environments
pub fn generate_project_manifest(
    project_dir: &Path,
    project_name: &str,
    python_version: Option<&str>,
    env_path: &Path,
) -> Result<(PathBuf, bool), String> {
    let ulvo_dir = project_dir.join(".ulvo");
    let manifest_path = ulvo_dir.join("manifest.toml");

    if manifest_path.is_file() {
        return Ok((manifest_path, false));
    }

    // Determine requirements files or generate derived requirements.txt if none exist
    let mut req_files = Vec::new();
    let req_release_lock = project_dir.join("requirements-release.lock");
    let req_lock = project_dir.join("requirements.lock");
    let req_lock_txt = project_dir.join("requirements-lock.txt");
    let req_txt = project_dir.join("requirements.txt");
    let pyproject = project_dir.join("pyproject.toml");

    let (_, _, is_ml) = inspect_candidate_dependencies(project_dir, env_path);

    if req_release_lock.is_file() {
        req_files.push("requirements-release.lock".to_string());
    } else if req_lock.is_file() {
        req_files.push("requirements.lock".to_string());
    } else if req_lock_txt.is_file() {
        req_files.push("requirements-lock.txt".to_string());
    } else if is_ml {
        // AI/ML environment requires pinned lockfile to guarantee identical wheel resolution
        let pkgs = extract_installed_packages(env_path);
        if !pkgs.is_empty() {
            let mut lines = Vec::new();
            lines.push("# Auto-pinned from working legacy environment for ML reproducibility".to_string());
            for p in &pkgs {
                if let Some(pos) = p.find("+cu") {
                    let cu_tag = &p[pos + 1..];
                    lines.push(format!("--extra-index-url https://download.pytorch.org/whl/{cu_tag}"));
                    break;
                }
            }
            lines.extend(pkgs);
            let content = lines.join("\n") + "\n";
            write_atomic(&req_lock, content.as_bytes())?;
            req_files.push("requirements.lock".to_string());
        } else if req_txt.is_file() {
            req_files.push("requirements.txt".to_string());
        } else if pyproject.is_file() {
            req_files.push("pyproject.toml".to_string());
        }
    } else if req_txt.is_file() {
        req_files.push("requirements.txt".to_string());
    } else if pyproject.is_file() {
        req_files.push("pyproject.toml".to_string());
    } else {
        // Derive requirements from legacy environment
        let pkgs = extract_installed_packages(env_path);
        if !pkgs.is_empty() {
            let content = pkgs.join("\n") + "\n";
            write_atomic(&req_txt, content.as_bytes())?;
            req_files.push("requirements.txt".to_string());
        }
    }

    // Determine runtime requirement constraint
    let py_constraint = if let Some(ver) = python_version {
        let parts: Vec<&str> = ver.split('.').collect();
        if parts.len() >= 2 {
            format!(">={}.{}", parts[0], parts[1])
        } else {
            format!(">={ver}")
        }
    } else {
        ">=3.10".to_string()
    };

    let req_lines = req_files
        .iter()
        .map(|r| format!("    \"{r}\","))
        .collect::<Vec<_>>()
        .join("\n");

    let manifest_content = format!(
        r#"schema_version = 1

[project]
name = "{project_name}"
version = "0.1.0"
ecosystem = "python"

[runtimes]
python = "{py_constraint}"

[package_manager]
name = "uv"
requirements = [
{req_lines}
]
"#
    );

    write_atomic(&manifest_path, manifest_content.as_bytes())?;
    Ok((manifest_path, true))
}

/// Find active running processes executing from or referencing an environment path
pub fn check_active_processes_for_env(env_path: &Path) -> Result<Vec<u32>, String> {
    let mut matching_pids = Vec::new();
    let canon_env = {
        let raw = match env_path.canonicalize() {
            Ok(p) => p.to_string_lossy().to_lowercase(),
            Err(_) => env_path.to_string_lossy().to_lowercase(),
        };
        if raw.starts_with(r"\\?\") {
            raw[4..].to_string()
        } else {
            raw
        }
    };

    #[cfg(windows)]
    {
        use win32_proc::*;

        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot != INVALID_HANDLE_VALUE {
                let mut entry: PROCESSENTRY32W = std::mem::zeroed();
                entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as DWORD;

                if Process32FirstW(snapshot, &mut entry) != 0 {
                    loop {
                        let pid = entry.th32ProcessID;
                        if pid != 0 && pid != std::process::id() {
                            let h_proc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                            if !h_proc.is_null() {
                                let mut exe_buf = [0u16; 1024];
                                let mut size = exe_buf.len() as DWORD;
                                if QueryFullProcessImageNameW(h_proc, 0, exe_buf.as_mut_ptr(), &mut size) != 0 {
                                    let exe_path = String::from_utf16_lossy(&exe_buf[..size as usize]).to_lowercase();
                                    if exe_path.contains(&canon_env) {
                                        matching_pids.push(pid);
                                    }
                                }
                                CloseHandle(h_proc);
                            }
                        }

                        if Process32NextW(snapshot, &mut entry) == 0 {
                            break;
                        }
                    }
                }
                CloseHandle(snapshot);
            }
        }
    }

    // Supplementary check via PowerShell CIM query passing path via environment variable
    let script = "$t = $env:ULVO_TARGET_ENV; if ($t) { Get-CimInstance Win32_Process | Where-Object { $_.ProcessId -ne $PID -and (($_.ExecutablePath -and $_.ExecutablePath.ToLower().Contains($t)) -or ($_.CommandLine -and $_.CommandLine.ToLower().Contains($t))) } | Select-Object -ExpandProperty ProcessId }";
    if let Ok(output) = Command::new("powershell")
        .args(&["-NoProfile", "-Command", script])
        .env("ULVO_TARGET_ENV", &canon_env)
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                if let Ok(pid) = line.trim().parse::<u32>() {
                    if pid != std::process::id() && !matching_pids.contains(&pid) {
                        matching_pids.push(pid);
                    }
                }
            }
        }
    }

    Ok(matching_pids)
}

/// Check if any files in the environment are actively locked
pub fn check_file_locks(env_path: &Path) -> Result<bool, String> {
    if !env_path.is_dir() {
        return Ok(true);
    }

    let mut stack = vec![env_path.to_path_buf()];
    let mut checked = 0;

    while let Some(current) = stack.pop() {
        if is_reparse_point(&current) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&current) else { continue; };
        for entry in entries.flatten() {
            let p = entry.path();
            if is_reparse_point(&p) {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if p.is_file() {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                if matches!(ext.as_str(), "exe" | "dll" | "pyd") {
                    checked += 1;
                    match std::fs::OpenOptions::new().read(true).open(&p) {
                        Ok(_) => {}
                        Err(e) => {
                            let raw = e.raw_os_error().unwrap_or(0);
                            // 32 = ERROR_SHARING_VIOLATION, 33 = ERROR_LOCK_VIOLATION
                            if raw == 32 || raw == 33 {
                                return Ok(false);
                            }
                        }
                    }
                    if checked >= 50 {
                        break;
                    }
                }
            }
        }
        if checked >= 50 {
            break;
        }
    }

    Ok(true)
}

/// Check for irreplaceable files inside an environment directory
pub fn check_irreplaceable_files(env_path: &Path) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    let mut stack = vec![env_path.to_path_buf()];

    while let Some(current) = stack.pop() {
        if is_reparse_point(&current) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&current) else { continue; };
        for entry in entries.flatten() {
            let p = entry.path();
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();

            if name == ".git" {
                found.push(format!("Git repository inside environment: {}", p.display()));
                continue;
            }

            if p.is_dir() {
                stack.push(p);
            } else if p.is_file() {
                let rel = p.strip_prefix(env_path).unwrap_or(&p);
                let rel_str = rel.to_string_lossy().replace('\\', "/").to_lowercase();

                // Disregard files safely inside site-packages
                if rel_str.contains("/site-packages/") || rel_str.starts_with("site-packages/") {
                    continue;
                }

                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                if matches!(ext.as_str(), "pt" | "pth" | "safetensors" | "bin" | "h5" | "ckpt" | "onnx") {
                    if let Ok(meta) = p.metadata() {
                        if meta.len() > 1_000_000 {
                            found.push(format!("Suspicious model weight file outside site-packages: {}", p.display()));
                        }
                    }
                } else if matches!(ext.as_str(), "parquet" | "arrow" | "sqlite" | "db") {
                    if let Ok(meta) = p.metadata() {
                        if meta.len() > 5_000_000 {
                            found.push(format!("Suspicious large database/dataset file outside site-packages: {}", p.display()));
                        }
                    }
                } else if rel.parent().map(|p| p == Path::new("")).unwrap_or(false) && ext == "py" {
                    // Python script in root of .venv
                    found.push(format!("Top-level Python script in environment root: {}", p.display()));
                }
            }
        }
    }

    Ok(found)
}

/// Run Gates A through J against a candidate project and legacy environment
pub fn check_retirement_gates(
    project_dir: &Path,
    env_path: &Path,
    vault_root: &Path,
    drive_free_bytes: u64,
    safety_reserve_bytes: u64,
) -> MigrationVerificationReport {
    let mut gates = Vec::new();

    let project_str = project_dir.to_string_lossy().to_string();
    let env_str = env_path.to_string_lossy().to_string();

    // Gate A: Reparse Point Check
    let gate_a_passed = env_path.is_dir() && !is_reparse_point(env_path);
    gates.push(VerificationGateReport {
        gate_id: "GATE_A".into(),
        name: "Reparse Point Verification".into(),
        passed: gate_a_passed,
        message: if gate_a_passed {
            "Verified real NTFS directory (not a junction, symlink, or mount point)".into()
        } else {
            "Target is missing, not a directory, or is a reparse point".into()
        },
    });

    // Gate B: Structural Hierarchy & Direct Parentage
    let gate_b_passed = match (env_path.canonicalize(), project_dir.canonicalize()) {
        (Ok(canon_env), Ok(canon_proj)) => canon_env.parent() == Some(&canon_proj),
        _ => false,
    };
    gates.push(VerificationGateReport {
        gate_id: "GATE_B".into(),
        name: "Structural Hierarchy & Direct Parentage".into(),
        passed: gate_b_passed,
        message: if gate_b_passed {
            "Environment resides directly under project root".into()
        } else {
            "Environment does not reside directly under project root or path resolution failed".into()
        },
    });

    // Gate C: Vault Resource Exclusion
    let gate_c_passed = match (env_path.canonicalize(), vault_root.canonicalize()) {
        (Ok(canon_env), Ok(canon_vault)) => !canon_env.starts_with(&canon_vault),
        _ => true,
    };
    gates.push(VerificationGateReport {
        gate_id: "GATE_C".into(),
        name: "Vault Resource Exclusion".into(),
        passed: gate_c_passed,
        message: if gate_c_passed {
            "Target path is external to ULVO vault boundary".into()
        } else {
            "Target path resides inside ULVO vault boundary; deletion strictly prohibited".into()
        },
    });

    // Gate D: Excluded Categories Check
    let env_name = env_path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
    let is_conda = env_name == ".conda" || env_path.join("conda-meta").is_dir();
    let is_shared_base = env_str.to_lowercase().contains("shared\\envs") || env_name == "ds-base";
    let gate_d_passed = !is_conda && !is_shared_base;
    gates.push(VerificationGateReport {
        gate_id: "GATE_D".into(),
        name: "Excluded Categories Check".into(),
        passed: gate_d_passed,
        message: if gate_d_passed {
            "Target is a standard project-local virtual environment".into()
        } else {
            "Target is a Conda environment or shared base interpreter; excluded from retirement".into()
        },
    });

    // Gate E: Active Process Liveness Check
    let active_pids = check_active_processes_for_env(env_path).unwrap_or_default();
    let gate_e_passed = active_pids.is_empty();
    gates.push(VerificationGateReport {
        gate_id: "GATE_E".into(),
        name: "Active Process Liveness Check".into(),
        passed: gate_e_passed,
        message: if gate_e_passed {
            "Zero active processes executing from or referencing this environment".into()
        } else {
            format!("Found {} active process(es) using this environment: {:?}", active_pids.len(), active_pids)
        },
    });

    // Gate F: File Lock Check
    let gate_f_passed = check_file_locks(env_path).unwrap_or(false);
    gates.push(VerificationGateReport {
        gate_id: "GATE_F".into(),
        name: "File Lock & Handle Contention Check".into(),
        passed: gate_f_passed,
        message: if gate_f_passed {
            "No active file locks or handle contention detected".into()
        } else {
            "Detected file locking conflict on environment binaries".into()
        },
    });

    // Gate G: Dependency Reproducibility Check
    let (repro, _, is_ml) = inspect_candidate_dependencies(project_dir, env_path);
    let gate_g_passed = repro != DependencyReproducibility::NonReproducible && (!is_ml || repro == DependencyReproducibility::LockfilePinned);
    gates.push(VerificationGateReport {
        gate_id: "GATE_G".into(),
        name: "Dependency Reproducibility Check".into(),
        passed: gate_g_passed,
        message: if gate_g_passed {
            format!("Reproducibility verified: {repro}")
        } else if is_ml {
            "AI/ML environment requires pinned lockfile to ensure exact wheel reproducibility".into()
        } else {
            "Dependencies cannot be reproduced safely".into()
        },
    });

    // Gate H: ULVO Manifest Integration Check
    let manifest_path = project_dir.join(".ulvo").join("manifest.toml");
    let gate_h_passed = manifest_path.is_file() && {
        if let Ok(content) = bounded_read(&manifest_path) {
            toml::from_str::<ProjectManifest>(&content).is_ok()
        } else {
            false
        }
    };
    gates.push(VerificationGateReport {
        gate_id: "GATE_H".into(),
        name: "ULVO Manifest Integration Check".into(),
        passed: gate_h_passed,
        message: if gate_h_passed {
            "Valid .ulvo/manifest.toml found".into()
        } else {
            "Missing or invalid .ulvo/manifest.toml; run 'ulvo migrate apply' first".into()
        },
    });

    // Gate I: Capacity & Functional Readiness Check
    let (phys_bytes, _) = calculate_physical_dir_size(env_path);
    let capacity_ok = drive_free_bytes > safety_reserve_bytes && phys_bytes <= (drive_free_bytes - safety_reserve_bytes);
    let gate_i_passed = capacity_ok && gate_h_passed;
    gates.push(VerificationGateReport {
        gate_id: "GATE_I".into(),
        name: "Capacity & Functional Readiness Check".into(),
        passed: gate_i_passed,
        message: if gate_i_passed {
            "Environment size within available capacity budget and manifest ready".into()
        } else if !capacity_ok {
            format!("Capacity check failed: environment ({:.2} MB) exceeds available budget ({:.2} MB)",
                phys_bytes as f64 / (1024.0 * 1024.0),
                (drive_free_bytes.saturating_sub(safety_reserve_bytes)) as f64 / (1024.0 * 1024.0)
            )
        } else {
            "ULVO manifest must be applied before functional verification can pass".into()
        },
    });

    // Gate J: Irreplaceable Files Check
    let irreplaceable = check_irreplaceable_files(env_path).unwrap_or_default();
    let gate_j_passed = irreplaceable.is_empty();
    gates.push(VerificationGateReport {
        gate_id: "GATE_J".into(),
        name: "Irreplaceable Files Check".into(),
        passed: gate_j_passed,
        message: if gate_j_passed {
            "No irreplaceable user files, models, or repositories found inside environment".into()
        } else {
            format!("Detected irreplaceable files inside environment: {:?}", irreplaceable)
        },
    });

    let all_passed = gates.iter().all(|g| g.passed);
    let diagnostic = if all_passed {
        None
    } else {
        let failed_names: Vec<String> = gates.iter().filter(|g| !g.passed).map(|g| format!("{}: {}", g.gate_id, g.name)).collect();
        Some(format!("Gates failed: {}", failed_names.join(", ")))
    };

    MigrationVerificationReport {
        schema_version: 1,
        project_path: project_str,
        legacy_env_path: env_str,
        gates,
        all_passed,
        can_retire: all_passed,
        diagnostic,
    }
}

/// Safely retire legacy environment leaf-first within project boundary
pub fn retire_legacy_environment(
    project_dir: &Path,
    env_path: &Path,
) -> Result<(u64, usize), String> {
    safe_reclaim_environment_hierarchy(env_path, project_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inspect_candidate_dependencies_synthetic() {
        let temp_dir = std::env::temp_dir().join(format!("ulvo_test_cand_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let req_file = temp_dir.join("requirements.txt");
        std::fs::write(&req_file, "click>=8.0\nrequests>=2.28\n").unwrap();

        let env_dir = temp_dir.join(".venv");
        std::fs::create_dir_all(&env_dir).unwrap();

        let (repro, sources, is_ml) = inspect_candidate_dependencies(&temp_dir, &env_dir);
        assert_eq!(repro, DependencyReproducibility::ManifestSpecified);
        assert_eq!(sources, vec!["requirements.txt".to_string()]);
        assert!(!is_ml);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_inspect_candidate_dependencies_ml_detection() {
        let temp_dir = std::env::temp_dir().join(format!("ulvo_test_ml_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let req_file = temp_dir.join("requirements.txt");
        std::fs::write(&req_file, "torch==2.1.0\ntorchvision\n").unwrap();

        let env_dir = temp_dir.join(".venv");
        std::fs::create_dir_all(&env_dir).unwrap();

        let (_, _, is_ml) = inspect_candidate_dependencies(&temp_dir, &env_dir);
        assert!(is_ml);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_generate_manifest_atomic_does_not_touch_env() {
        let temp_dir = std::env::temp_dir().join(format!("ulvo_test_gen_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let env_dir = temp_dir.join(".venv");
        std::fs::create_dir_all(&env_dir).unwrap();
        let sentinel_file = env_dir.join("sentinel.txt");
        std::fs::write(&sentinel_file, "legacy data").unwrap();

        let req_file = temp_dir.join("requirements.txt");
        std::fs::write(&req_file, "flask>=2.0\n").unwrap();

        let (manifest_path, created) = generate_project_manifest(&temp_dir, "test_proj", Some("3.11.2"), &env_dir).unwrap();
        assert!(created);
        assert!(manifest_path.is_file());

        // Sentinel in .venv must be completely untouched!
        assert_eq!(std::fs::read_to_string(&sentinel_file).unwrap(), "legacy data");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_irreplaceable_files_detection() {
        let temp_dir = std::env::temp_dir().join(format!("ulvo_test_irrep_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let env_dir = temp_dir.join(".venv");
        std::fs::create_dir_all(&env_dir).unwrap();

        // Put a user script directly in root of .venv
        std::fs::write(env_dir.join("train.py"), "print('model')").unwrap();

        let found = check_irreplaceable_files(&env_dir).unwrap();
        assert!(!found.is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
