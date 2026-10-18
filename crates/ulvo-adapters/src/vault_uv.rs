use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use crate::is_reparse_point;


#[derive(Debug, Clone)]
pub struct VaultPaths {
    pub vault_root: PathBuf,
    pub runtimes_dir: PathBuf,
    pub packages_dir: PathBuf,
    pub environments_dir: PathBuf,
    pub uv_cache_dir: PathBuf,
}

impl VaultPaths {
    pub fn new(vault_root: PathBuf) -> Self {
        let runtimes_dir = vault_root.join("runtimes").join("python");
        let packages_dir = vault_root.join("packages").join("python");
        let environments_dir = vault_root.join("environments").join("python");
        let uv_cache_dir = packages_dir.join("uv-cache");

        Self {
            vault_root,
            runtimes_dir,
            packages_dir,
            environments_dir,
            uv_cache_dir,
        }
    }

    pub fn ensure_dirs(&self) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(&self.runtimes_dir)?;
        std::fs::create_dir_all(&self.packages_dir)?;
        std::fs::create_dir_all(&self.environments_dir)?;
        std::fs::create_dir_all(&self.uv_cache_dir)?;
        Ok(())
    }
}

pub struct UvManager {
    uv_executable: PathBuf,
    paths: VaultPaths,
}

impl UvManager {
    pub fn new(vault_root: PathBuf) -> Result<Self, String> {
        let uv_exe = find_uv_executable().ok_or_else(|| {
            "uv executable not found on system PATH. Please ensure uv is installed.".to_string()
        })?;

        let paths = VaultPaths::new(vault_root);
        paths.ensure_dirs().map_err(|e| format!("failed to initialize vault directories: {e}"))?;

        Ok(Self {
            uv_executable: uv_exe,
            paths,
        })
    }

    pub fn uv_executable(&self) -> &Path {
        &self.uv_executable
    }

    pub fn vault_paths(&self) -> &VaultPaths {
        &self.paths
    }

    pub fn uv_version(&self) -> Option<String> {
        let out = Command::new(&self.uv_executable)
            .arg("--version")
            .output()
            .ok()?;
        if out.status.success() {
            String::from_utf8(out.stdout)
                .ok()
                .map(|s| s.trim().to_string())
        } else {
            None
        }
    }

    pub fn resolve_python_runtime(&self, version_constraint: &str) -> Result<String, String> {
        let mut cmd = Command::new(&self.uv_executable);
        cmd.arg("python").arg("find");
        if !version_constraint.is_empty() {
            cmd.arg(version_constraint);
        }
        cmd.env("UV_CACHE_DIR", &self.paths.uv_cache_dir);

        let out = cmd.output().map_err(|e| format!("failed to execute uv python find: {e}"))?;
        if out.status.success() {
            let path_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path_str.is_empty() {
                Ok(path_str)
            } else {
                Err(format!("no matching Python runtime found for constraint: {version_constraint}"))
            }
        } else {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(format!("uv python find failed: {err}"))
        }
    }

    pub fn materialize_environment(
        &self,
        env_path: &Path,
        python_runtime: Option<&str>,
    ) -> Result<(), String> {
        if let Some(parent) = env_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let mut cmd = Command::new(&self.uv_executable);
        cmd.arg("venv").arg(env_path);
        if let Some(py) = python_runtime {
            cmd.arg("--python").arg(py);
        }
        cmd.arg("--cache-dir").arg(&self.paths.uv_cache_dir);

        let out = cmd.output().map_err(|e| format!("failed to execute uv venv: {e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(format!("uv venv failed: {err}"));
        }

        Ok(())
    }

    pub fn install_requirements(
        &self,
        env_path: &Path,
        requirement_files: &[PathBuf],
    ) -> Result<(), String> {
        let env_python = get_env_python(env_path);
        if !env_python.is_file() {
            return Err(format!("environment python not found at {}", env_python.display()));
        }

        for req in requirement_files {
            if !req.is_file() {
                return Err(format!("requirement file not found: {}", req.display()));
            }

            let mut cmd = Command::new(&self.uv_executable);
            cmd.arg("pip")
                .arg("install")
                .arg("-r")
                .arg(req)
                .arg("--python")
                .arg(&env_python)
                .arg("--cache-dir")
                .arg(&self.paths.uv_cache_dir);

            let out = cmd.output().map_err(|e| format!("failed to execute uv pip install: {e}"))?;
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                return Err(format!("uv pip install failed for {}: {err}", req.display()));
            }
        }

        Ok(())
    }

    pub fn is_pid_alive(&self, pid: u32) -> bool {
        #[cfg(windows)]
        {
            type HANDLE = *mut std::ffi::c_void;
            type DWORD = u32;
            type BOOL = i32;

            const PROCESS_QUERY_LIMITED_INFORMATION: DWORD = 0x1000;
            const STILL_ACTIVE: DWORD = 259;

            extern "system" {
                fn OpenProcess(dwDesiredAccess: DWORD, bInheritHandle: BOOL, dwProcessId: DWORD) -> HANDLE;
                fn GetExitCodeProcess(hProcess: HANDLE, lpExitCode: *mut DWORD) -> BOOL;
                fn CloseHandle(hObject: HANDLE) -> BOOL;
            }

            unsafe {
                let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                if handle.is_null() {
                    return false;
                }
                let mut exit_code: DWORD = 0;
                let success = GetExitCodeProcess(handle, &mut exit_code);
                CloseHandle(handle);
                success != 0 && exit_code == STILL_ACTIVE
            }
        }

        #[cfg(not(windows))]
        {
            unsafe { libc::kill(pid as i32, 0) == 0 }
        }
    }

    pub fn execute_in_environment(
        &self,
        env_path: &Path,
        project_dir: &Path,
        command_args: &[String],
    ) -> Result<i32, String> {
        if command_args.is_empty() {
            return Err("no command specified for execution".to_string());
        }

        let env_scripts = get_env_scripts(env_path);
        let env_python = get_env_python(env_path);

        // Sanitize PATH: exclude any old project-local .venv to enforce isolation!
        let current_path = std::env::var("PATH").unwrap_or_default();
        let sanitized_path = sanitize_path(&current_path, project_dir, &env_scripts);

        let mut cmd = if command_args[0] == "python" || command_args[0] == "python.exe" {
            let mut c = Command::new(&env_python);
            if command_args.len() > 1 {
                c.args(&command_args[1..]);
            }
            c
        } else {
            let exe_name = &command_args[0];
            let resolved_exe = env_scripts.join(exe_name);
            let final_exe = if resolved_exe.is_file() {
                resolved_exe
            } else {
                PathBuf::from(exe_name)
            };
            let mut c = Command::new(final_exe);
            if command_args.len() > 1 {
                c.args(&command_args[1..]);
            }
            c
        };

        cmd.current_dir(project_dir);
        cmd.env("VIRTUAL_ENV", env_path);
        cmd.env("PATH", sanitized_path);
        cmd.env_remove("PYTHONHOME");
        cmd.env("UV_CACHE_DIR", &self.paths.uv_cache_dir);
        cmd.stdin(Stdio::inherit());
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());

        let mut child = cmd.spawn().map_err(|e| format!("failed to spawn child command: {e}"))?;
        let status = child.wait().map_err(|e| format!("failed waiting for child command: {e}"))?;

        Ok(status.code().unwrap_or(-1))
    }
}

pub fn get_env_scripts(env_path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        env_path.join("Scripts")
    }
    #[cfg(not(windows))]
    {
        env_path.join("bin")
    }
}

pub fn get_env_python(env_path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        env_path.join("Scripts").join("python.exe")
    }
    #[cfg(not(windows))]
    {
        env_path.join("bin").join("python")
    }
}

fn sanitize_path(current_path: &str, project_dir: &Path, prepend_scripts: &Path) -> String {
    let sep = if cfg!(windows) { ";" } else { ":" };
    let project_str = project_dir.to_string_lossy().to_lowercase();
    let mut entries = Vec::new();

    // Prepend the ULVO-managed environment scripts directory
    entries.push(prepend_scripts.to_string_lossy().to_string());

    for entry in current_path.split(if cfg!(windows) { ';' } else { ':' }) {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_lowercase();
        // Discard any entries pointing into the project's local venv!
        if lower.contains(&project_str) && (lower.contains(".venv") || lower.contains("venv")) {
            continue;
        }
        entries.push(trimmed.to_string());
    }

    entries.join(sep)
}

fn find_uv_executable() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("UV_PATH") {
        let p = PathBuf::from(path);
        if p.is_file() {
            return Some(p);
        }
    }

    #[cfg(windows)]
    let names = &["uv.exe", "uv.cmd", "uv"];
    #[cfg(not(windows))]
    let names = &["uv"];

    // Check system PATH
    if let Ok(paths) = std::env::var("PATH") {
        for dir in paths.split(if cfg!(windows) { ';' } else { ':' }) {
            let dir_p = PathBuf::from(dir.trim());
            for name in names {
                let candidate = dir_p.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    // Common Windows install locations
    #[cfg(windows)]
    {
        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            let common_locations = [
                format!("{userprofile}\\.cargo\\bin\\uv.exe"),
                format!("{userprofile}\\scoop\\shims\\uv.exe"),
                format!("{userprofile}\\AppData\\Local\\Programs\\uv\\uv.exe"),
            ];
            for loc in &common_locations {
                let p = PathBuf::from(loc);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }

    None
}

pub fn calculate_dir_size(dir: &Path) -> (u64, usize) {
    let mut total_bytes = 0u64;
    let mut file_count = 0usize;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        if is_reparse_point(&current) {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&current) {
            for entry in entries.flatten() {
                let p = entry.path();
                if is_reparse_point(&p) {
                    continue;
                }
                if p.is_dir() {
                    stack.push(p);
                } else if p.is_file() {
                    if let Ok(meta) = p.metadata() {
                        total_bytes += meta.len();
                        file_count += 1;
                    }
                }
            }
        }
    }
    (total_bytes, file_count)
}
