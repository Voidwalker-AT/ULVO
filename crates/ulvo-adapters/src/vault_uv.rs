use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use ulvo_core::ProcessLiveness;
use crate::is_reparse_point;

#[derive(Debug, Clone)]
pub struct VaultPaths {
    pub vault_root: PathBuf,
    pub runtimes_dir: PathBuf,
    pub packages_dir: PathBuf,
    pub environments_dir: PathBuf,
    pub uv_cache_dir: PathBuf,
    pub shared_packages_dir: PathBuf,
}

impl VaultPaths {
    pub fn new(vault_root: PathBuf) -> Self {
        let runtimes_dir = vault_root.join("runtimes").join("python");
        let packages_dir = vault_root.join("packages").join("python");
        let environments_dir = vault_root.join("environments").join("python");
        let uv_cache_dir = packages_dir.join("uv-cache");
        let shared_packages_dir = packages_dir.join("shared");

        Self {
            vault_root,
            runtimes_dir,
            packages_dir,
            environments_dir,
            uv_cache_dir,
            shared_packages_dir,
        }
    }

    pub fn ensure_dirs(&self) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(&self.runtimes_dir)?;
        std::fs::create_dir_all(&self.packages_dir)?;
        std::fs::create_dir_all(&self.environments_dir)?;
        std::fs::create_dir_all(&self.uv_cache_dir)?;
        std::fs::create_dir_all(&self.shared_packages_dir)?;
        Ok(())
    }
}

pub struct UvManager {
    uv_executable: PathBuf,
    paths: VaultPaths,
}

impl UvManager {
    pub fn new(vault_root: PathBuf) -> Result<Self, String> {
        let uv_exe = find_uv_executable(Some(&vault_root)).ok_or_else(|| {
            "uv executable not found in ULVO vault or on system PATH. Please ensure uv is installed.".to_string()
        })?;

        // Purely read-only: do NOT call paths.ensure_dirs() here! Defer to materialize_environment!
        let paths = VaultPaths::new(vault_root);

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
        cmd.env("UV_PYTHON_INSTALL_DIR", &self.paths.runtimes_dir);

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
        self.paths.ensure_dirs().map_err(|e| format!("failed to initialize vault directories: {e}"))?;

        if let Some(parent) = env_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let mut cmd = Command::new(&self.uv_executable);
        cmd.arg("venv").arg(env_path);
        cmd.arg("--allow-existing");
        if let Some(py) = python_runtime {
            cmd.arg("--python").arg(py);
        }
        cmd.arg("--cache-dir").arg(&self.paths.uv_cache_dir);
        cmd.env("UV_CACHE_DIR", &self.paths.uv_cache_dir);
        cmd.env("UV_PYTHON_INSTALL_DIR", &self.paths.runtimes_dir);

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
                .arg(&self.paths.uv_cache_dir)
                .arg("--index-strategy")
                .arg("unsafe-best-match");
            cmd.env("UV_CACHE_DIR", &self.paths.uv_cache_dir);
            cmd.env("UV_PYTHON_INSTALL_DIR", &self.paths.runtimes_dir);

            let out = cmd.output().map_err(|e| format!("failed to execute uv pip install: {e}"))?;
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                return Err(format!("uv pip install failed for {}: {err}", req.display()));
            }
        }

        Ok(())
    }

    pub fn is_pid_alive(&self, pid: u32) -> bool {
        check_process_liveness(pid, None).is_alive()
    }

    pub fn execute_in_environment(
        &self,
        env_path: &Path,
        project_dir: &Path,
        command_args: &[String],
    ) -> Result<i32, String> {
        self.execute_in_environment_with_callback(env_path, project_dir, command_args, |_| Ok(()))
    }

    pub fn execute_in_environment_with_callback<F>(
        &self,
        env_path: &Path,
        project_dir: &Path,
        command_args: &[String],
        on_spawn: F,
    ) -> Result<i32, String>
    where
        F: FnOnce(u32) -> Result<(), String>,
    {
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
        cmd.env("UV_PYTHON_INSTALL_DIR", &self.paths.runtimes_dir);
        cmd.stdin(Stdio::inherit());
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());

        let job_supervisor = JobSupervisor::new().map_err(|e| format!("failed to initialize job supervisor: {e}"))?;

        let mut child = cmd.spawn().map_err(|e| format!("failed to spawn child command: {e}"))?;

        let child_pid = child.id();
        if let Err(e) = job_supervisor.assign_process(&child) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("failed to assign child process {child_pid} to job supervisor: {e}"));
        }

        if let Err(e) = on_spawn(child_pid) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("failed during on_spawn callback for child PID {child_pid}: {e}"));
        }

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

fn find_uv_executable(vault_root: Option<&Path>) -> Option<PathBuf> {
    // 0. Check inside explicit vault root
    if let Some(root) = vault_root {
        let vault_candidates = [
            root.join("bin").join("uv.exe"),
            root.join("bin").join("uv"),
            root.join("toolchains").join("uv").join("bin").join("uv.exe"),
        ];
        for candidate in &vault_candidates {
            if candidate.is_file() {
                return Some(candidate.clone());
            }
        }
    }

    // 1. Check ULVO_VAULT environment variable
    if let Ok(v) = std::env::var("ULVO_VAULT") {
        let vault_dir = PathBuf::from(v);
        let candidate = vault_dir.join("bin").join("uv.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    // 2. Check relative to current executable
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            let candidate = parent.join("uv.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
            let candidate2 = parent.join("vault").join("bin").join("uv.exe");
            if candidate2.is_file() {
                return Some(candidate2);
            }
            if let Some(grandparent) = parent.parent() {
                let candidate3 = grandparent.join("vault").join("bin").join("uv.exe");
                if candidate3.is_file() {
                    return Some(candidate3);
                }
            }
        }
    }

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
                format!("{userprofile}\\.local\\bin\\uv.exe"),
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

pub fn calculate_physical_dir_size(dir: &Path) -> (u64, usize) {
    let mut total_allocated_bytes = 0u64;
    let mut file_count = 0usize;
    let mut stack = vec![dir.to_path_buf()];
    let mut identities = std::collections::HashSet::new();

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
                    if let Ok((id, size_info)) = crate::fs_identity::query_file_identity_and_size(&p) {
                        if identities.insert(id) {
                            total_allocated_bytes += size_info.allocated_bytes.unwrap_or(0);
                        }
                        file_count += 1;
                    }
                }
            }
        }
    }
    (total_allocated_bytes, file_count)
}

#[cfg(windows)]
pub fn check_process_liveness(pid: u32, expected_creation_time: Option<u64>) -> ProcessLiveness {
    type HANDLE = *mut std::ffi::c_void;
    type DWORD = u32;
    type BOOL = i32;

    const PROCESS_QUERY_LIMITED_INFORMATION: DWORD = 0x1000;
    const STILL_ACTIVE: DWORD = 259;
    const ERROR_ACCESS_DENIED: DWORD = 5;
    const ERROR_INVALID_PARAMETER: DWORD = 87;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct FILETIME {
        dwLowDateTime: DWORD,
        dwHighDateTime: DWORD,
    }

    extern "system" {
        fn OpenProcess(dwDesiredAccess: DWORD, bInheritHandle: BOOL, dwProcessId: DWORD) -> HANDLE;
        fn GetExitCodeProcess(hProcess: HANDLE, lpExitCode: *mut DWORD) -> BOOL;
        fn GetProcessTimes(
            hProcess: HANDLE,
            lpCreationTime: *mut FILETIME,
            lpExitTime: *mut FILETIME,
            lpKernelTime: *mut FILETIME,
            lpUserTime: *mut FILETIME,
        ) -> BOOL;
        fn CloseHandle(hObject: HANDLE) -> BOOL;
        fn GetLastError() -> DWORD;
    }

    if pid == 0 {
        return ProcessLiveness::Terminated;
    }

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            let err = GetLastError();
            if err == ERROR_ACCESS_DENIED {
                // Process exists and is alive, but caller lacks rights to inspect it -> fail closed!
                return ProcessLiveness::Unknown;
            }
            if err == ERROR_INVALID_PARAMETER {
                // Process does not exist
                return ProcessLiveness::Terminated;
            }
            // For any other unexpected OS error, fail closed
            return ProcessLiveness::Unknown;
        }

        let mut exit_code: DWORD = 0;
        let success = GetExitCodeProcess(handle, &mut exit_code);
        if success == 0 {
            CloseHandle(handle);
            return ProcessLiveness::Unknown;
        }

        if exit_code != STILL_ACTIVE {
            CloseHandle(handle);
            return ProcessLiveness::Terminated;
        }

        if let Some(expected_time) = expected_creation_time {
            let mut ft_creation = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
            let mut ft_exit = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
            let mut ft_kernel = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
            let mut ft_user = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };

            if GetProcessTimes(handle, &mut ft_creation, &mut ft_exit, &mut ft_kernel, &mut ft_user) != 0 {
                let actual_time = ((ft_creation.dwHighDateTime as u64) << 32) | (ft_creation.dwLowDateTime as u64);
                if actual_time != expected_time {
                    // PID was recycled by the operating system! Original process is dead.
                    CloseHandle(handle);
                    return ProcessLiveness::Terminated;
                }
            }
        }

        CloseHandle(handle);
        ProcessLiveness::Alive
    }
}

#[cfg(not(windows))]
pub fn check_process_liveness(pid: u32, _expected_creation_time: Option<u64>) -> ProcessLiveness {
    if pid == 0 {
        return ProcessLiveness::Terminated;
    }
    unsafe {
        if libc::kill(pid as i32, 0) == 0 {
            ProcessLiveness::Alive
        } else {
            let err = std::io::Error::last_os_error().raw_os_error();
            if err == Some(libc::EPERM) {
                ProcessLiveness::Unknown
            } else {
                ProcessLiveness::Terminated
            }
        }
    }
}

#[cfg(windows)]
pub fn get_process_creation_time(pid: u32) -> Option<u64> {
    type HANDLE = *mut std::ffi::c_void;
    type DWORD = u32;
    type BOOL = i32;

    const PROCESS_QUERY_LIMITED_INFORMATION: DWORD = 0x1000;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct FILETIME {
        dwLowDateTime: DWORD,
        dwHighDateTime: DWORD,
    }

    extern "system" {
        fn OpenProcess(dwDesiredAccess: DWORD, bInheritHandle: BOOL, dwProcessId: DWORD) -> HANDLE;
        fn GetProcessTimes(
            hProcess: HANDLE,
            lpCreationTime: *mut FILETIME,
            lpExitTime: *mut FILETIME,
            lpKernelTime: *mut FILETIME,
            lpUserTime: *mut FILETIME,
        ) -> BOOL;
        fn CloseHandle(hObject: HANDLE) -> BOOL;
    }

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut ft_creation = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let mut ft_exit = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let mut ft_kernel = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let mut ft_user = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };

        let ok = GetProcessTimes(handle, &mut ft_creation, &mut ft_exit, &mut ft_kernel, &mut ft_user);
        CloseHandle(handle);
        if ok != 0 {
            Some(((ft_creation.dwHighDateTime as u64) << 32) | (ft_creation.dwLowDateTime as u64))
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
pub fn get_process_creation_time(_pid: u32) -> Option<u64> {
    None
}

pub struct JobSupervisor {
    #[cfg(windows)]
    job_handle: *mut std::ffi::c_void,
}

unsafe impl Send for JobSupervisor {}
unsafe impl Sync for JobSupervisor {}

impl JobSupervisor {
    pub fn new() -> Result<Self, String> {
        #[cfg(windows)]
        {
            type HANDLE = *mut std::ffi::c_void;
            type DWORD = u32;
            type BOOL = i32;
            type LPCWSTR = *const u16;
            type LPVOID = *mut std::ffi::c_void;

            const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: DWORD = 0x2000;
            const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: DWORD = 9;

            #[repr(C)]
            #[allow(non_snake_case)]
            struct IO_COUNTERS {
                ReadOperationCount: u64,
                WriteOperationCount: u64,
                OtherOperationCount: u64,
                ReadTransferCount: u64,
                WriteTransferCount: u64,
                OtherTransferCount: u64,
            }

            #[repr(C)]
            #[allow(non_snake_case)]
            struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
                PerProcessUserTimeLimit: i64,
                PerJobUserTimeLimit: i64,
                LimitFlags: DWORD,
                MinimumWorkingSetSize: usize,
                MaximumWorkingSetSize: usize,
                ActiveProcessLimit: DWORD,
                Affinity: usize,
                PriorityClass: DWORD,
                SchedulingClass: DWORD,
            }

            #[repr(C)]
            #[allow(non_snake_case)]
            struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
                BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION,
                IoInfo: IO_COUNTERS,
                ProcessMemoryLimit: usize,
                JobMemoryLimit: usize,
                PeakProcessMemoryLimit: usize,
                PeakJobMemoryLimit: usize,
            }

            extern "system" {
                fn CreateJobObjectW(lpJobAttributes: LPVOID, lpName: LPCWSTR) -> HANDLE;
                fn SetInformationJobObject(
                    hJob: HANDLE,
                    JobObjectInformationClass: DWORD,
                    lpJobObjectInformation: LPVOID,
                    cbJobObjectInformationLength: DWORD,
                ) -> BOOL;
                fn CloseHandle(hObject: HANDLE) -> BOOL;
            }

            unsafe {
                let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
                if job.is_null() {
                    return Err("failed to create Win32 Job Object".to_string());
                }

                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

                let ok = SetInformationJobObject(
                    job,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                    &mut info as *mut _ as LPVOID,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as DWORD,
                );

                if ok == 0 {
                    CloseHandle(job);
                    return Err("failed to set Job Object kill-on-close limit".to_string());
                }

                Ok(Self { job_handle: job })
            }
        }
        #[cfg(not(windows))]
        {
            Ok(Self {})
        }
    }

    pub fn assign_process(&self, child: &std::process::Child) -> Result<(), String> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            type HANDLE = *mut std::ffi::c_void;
            type BOOL = i32;

            extern "system" {
                fn AssignProcessToJobObject(hJob: HANDLE, hProcess: HANDLE) -> BOOL;
            }

            let raw_proc = child.as_raw_handle() as HANDLE;
            unsafe {
                let ok = AssignProcessToJobObject(self.job_handle, raw_proc);
                if ok == 0 {
                    return Err("failed to assign child process to Job Object".to_string());
                }
            }
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = child;
            Ok(())
        }
    }

    pub fn active_process_count(&self) -> usize {
        #[cfg(windows)]
        {
            type HANDLE = *mut std::ffi::c_void;
            type DWORD = u32;
            type BOOL = i32;
            type LPVOID = *mut std::ffi::c_void;

            const JOB_OBJECT_BASIC_PROCESS_ID_LIST: DWORD = 3;

            #[repr(C)]
            #[allow(non_snake_case)]
            struct JOBOBJECT_BASIC_PROCESS_ID_LIST {
                NumberOfAssignedProcesses: DWORD,
                NumberOfProcessIdsInList: DWORD,
                ProcessIdList: [usize; 64],
            }

            extern "system" {
                fn QueryInformationJobObject(
                    hJob: HANDLE,
                    JobObjectInformationClass: DWORD,
                    lpJobObjectInformation: LPVOID,
                    cbJobObjectInformationLength: DWORD,
                    lpReturnLength: *mut DWORD,
                ) -> BOOL;
            }

            unsafe {
                let mut list: JOBOBJECT_BASIC_PROCESS_ID_LIST = std::mem::zeroed();
                let mut return_len: DWORD = 0;
                let ok = QueryInformationJobObject(
                    self.job_handle,
                    JOB_OBJECT_BASIC_PROCESS_ID_LIST,
                    &mut list as *mut _ as LPVOID,
                    std::mem::size_of::<JOBOBJECT_BASIC_PROCESS_ID_LIST>() as DWORD,
                    &mut return_len,
                );
                if ok != 0 {
                    list.NumberOfProcessIdsInList as usize
                } else {
                    0
                }
            }
        }
        #[cfg(not(windows))]
        {
            0
        }
    }
}

impl Drop for JobSupervisor {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            type HANDLE = *mut std::ffi::c_void;
            type BOOL = i32;
            extern "system" {
                fn CloseHandle(hObject: HANDLE) -> BOOL;
            }
            if !self.job_handle.is_null() {
                unsafe {
                    CloseHandle(self.job_handle);
                }
            }
        }
    }
}

pub struct EnvironmentFileLock {
    #[cfg(windows)]
    handle: *mut std::ffi::c_void,
    #[allow(dead_code)]
    lock_path: PathBuf,
}

unsafe impl Send for EnvironmentFileLock {}
unsafe impl Sync for EnvironmentFileLock {}

impl EnvironmentFileLock {
    pub fn acquire(lock_path: &Path, timeout_ms: u64) -> Result<Self, String> {
        let parent = lock_path.parent().ok_or_else(|| "invalid lock path".to_string())?;
        std::fs::create_dir_all(parent).map_err(|e| format!("failed to create dir for lock: {e}"))?;

        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            type HANDLE = *mut std::ffi::c_void;
            type DWORD = u32;
            type BOOL = i32;
            type LPCWSTR = *const u16;
            type LPVOID = *mut std::ffi::c_void;

            const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
            const GENERIC_READ: DWORD = 0x80000000;
            const GENERIC_WRITE: DWORD = 0x40000000;
            const FILE_SHARE_READ: DWORD = 1;
            const FILE_SHARE_WRITE: DWORD = 2;
            const OPEN_ALWAYS: DWORD = 4;
            const FILE_ATTRIBUTE_NORMAL: DWORD = 0x80;
            const LOCKFILE_EXCLUSIVE_LOCK: DWORD = 2;
            const LOCKFILE_FAIL_IMMEDIATELY: DWORD = 1;

            #[repr(C)]
            #[allow(non_snake_case)]
            struct OVERLAPPED {
                Internal: usize,
                InternalHigh: usize,
                Offset: DWORD,
                OffsetHigh: DWORD,
                hEvent: HANDLE,
            }

            extern "system" {
                fn CreateFileW(
                    lpFileName: LPCWSTR,
                    dwDesiredAccess: DWORD,
                    dwShareMode: DWORD,
                    lpSecurityAttributes: LPVOID,
                    dwCreationDisposition: DWORD,
                    dwFlagsAndAttributes: DWORD,
                    hTemplateFile: HANDLE,
                ) -> HANDLE;
                fn LockFileEx(
                    hFile: HANDLE,
                    dwFlags: DWORD,
                    dwReserved: DWORD,
                    nNumberOfBytesToLockLow: DWORD,
                    nNumberOfBytesToLockHigh: DWORD,
                    lpOverlapped: *mut OVERLAPPED,
                ) -> BOOL;
                fn CloseHandle(hObject: HANDLE) -> BOOL;
            }

            let wide: Vec<u16> = lock_path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
            let handle = unsafe {
                CreateFileW(
                    wide.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null_mut(),
                    OPEN_ALWAYS,
                    FILE_ATTRIBUTE_NORMAL,
                    std::ptr::null_mut(),
                )
            };

            if handle == INVALID_HANDLE_VALUE {
                return Err(format!("failed to open lock file '{}'", lock_path.display()));
            }

            let start = std::time::Instant::now();
            let wait_duration = std::time::Duration::from_millis(timeout_ms);
            let poll_interval = std::time::Duration::from_millis(25);

            loop {
                let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
                let locked = unsafe {
                    LockFileEx(
                        handle,
                        LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                        0,
                        1,
                        0,
                        &mut overlapped,
                    )
                };

                if locked != 0 {
                    return Ok(Self {
                        handle,
                        lock_path: lock_path.to_path_buf(),
                    });
                }

                if start.elapsed() >= wait_duration {
                    unsafe { CloseHandle(handle); }
                    return Err(format!("timeout acquiring environment lock on '{}'", lock_path.display()));
                }

                std::thread::sleep(poll_interval);
            }
        }

        #[cfg(not(windows))]
        {
            let _ = timeout_ms;
            Ok(Self {
                lock_path: lock_path.to_path_buf(),
            })
        }
    }
}

impl Drop for EnvironmentFileLock {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            type HANDLE = *mut std::ffi::c_void;
            type DWORD = u32;
            type BOOL = i32;

            #[repr(C)]
            #[allow(non_snake_case)]
            struct OVERLAPPED {
                Internal: usize,
                InternalHigh: usize,
                Offset: DWORD,
                OffsetHigh: DWORD,
                hEvent: HANDLE,
            }

            extern "system" {
                fn UnlockFileEx(
                    hFile: HANDLE,
                    dwReserved: DWORD,
                    nNumberOfBytesToLockLow: DWORD,
                    nNumberOfBytesToLockHigh: DWORD,
                    lpOverlapped: *mut OVERLAPPED,
                ) -> BOOL;
                fn CloseHandle(hObject: HANDLE) -> BOOL;
            }

            unsafe {
                let mut overlapped: OVERLAPPED = std::mem::zeroed();
                UnlockFileEx(self.handle, 0, 1, 0, &mut overlapped);
                CloseHandle(self.handle);
            }
        }
    }
}

pub fn write_atomic(path: &Path, content: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "missing parent dir".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("failed to create parent dir: {e}"))?;

    let tmp_name = format!(
        "{}.tmp.{}.{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()
    );
    let tmp_path = parent.join(tmp_name);

    {
        use std::io::Write;
        let mut staged = std::fs::File::create(&tmp_path)
            .map_err(|e| format!("failed creating tmp file {}: {e}", tmp_path.display()))?;
        staged.write_all(content)
            .and_then(|_| staged.sync_all())
            .map_err(|e| format!("failed flushing tmp file {}: {e}", tmp_path.display()))?;
    }

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        type DWORD = u32;
        type BOOL = i32;
        type LPCWSTR = *const u16;

        const MOVEFILE_REPLACE_EXISTING: DWORD = 1;
        const MOVEFILE_WRITE_THROUGH: DWORD = 8;

        extern "system" {
            fn MoveFileExW(lpExistingFileName: LPCWSTR, lpNewFileName: LPCWSTR, dwFlags: DWORD) -> BOOL;
        }

        // std::fs supports long paths, but raw Win32 calls need absolute
        // extended-length paths, including when the destination is still absent.
        let from = tmp_path.canonicalize().map_err(|e| format!("cannot resolve staged path: {e}"))?;
        let to = parent.canonicalize().map_err(|e| format!("cannot resolve destination parent: {e}"))?
            .join(path.file_name().ok_or_else(|| "missing destination filename".to_string())?);
        let wide_from: Vec<u16> = from.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let wide_to: Vec<u16> = to.as_os_str().encode_wide().chain(std::iter::once(0)).collect();

        let ok = unsafe {
            MoveFileExW(wide_from.as_ptr(), wide_to.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
        };

        if ok == 0 {
            let err = std::io::Error::last_os_error();
            let _ = std::fs::remove_file(&tmp_path);
            return Err(format!("atomic rename from '{}' to '{}' failed: {err}", tmp_path.display(), path.display()));
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(&tmp_path, path)
            .map_err(|e| format!("atomic rename failed: {e}"))?;
    }

    Ok(())
}

pub fn safe_reclaim_environment_hierarchy(
    env_path: &Path,
    allowed_boundary: &Path,
) -> Result<(u64, usize), String> {
    if !env_path.exists() {
        return Ok((0, 0));
    }

    // Canonicalize both to verify strict containment
    let canon_env = env_path.canonicalize()
        .map_err(|e| format!("failed canonicalizing environment path {}: {e}", env_path.display()))?;
    let canon_boundary = allowed_boundary.canonicalize()
        .map_err(|e| format!("failed canonicalizing vault boundary {}: {e}", allowed_boundary.display()))?;

    // Boundary validation: env_path must be strictly inside allowed_boundary
    if !canon_env.starts_with(&canon_boundary) {
        return Err(format!(
            "security violation: target path '{}' is outside allowed boundary '{}'",
            canon_env.display(), canon_boundary.display()
        ));
    }

    if canon_env == canon_boundary {
        return Err("security violation: refusing to delete vault root boundary directory itself".to_string());
    }

    // Reparse check on the top target itself
    if is_reparse_point(&canon_env) {
        return Err(format!(
            "security violation: environment root '{}' is a reparse point",
            canon_env.display()
        ));
    }

    // Measure physical allocated size and collect entries leaf-first
    let (physical_bytes, file_count) = calculate_physical_dir_size(&canon_env);

    // Collect all entries
    let mut files_and_links = Vec::new();
    let mut dirs = Vec::new();

    let mut stack = vec![canon_env.clone()];
    while let Some(current_dir) = stack.pop() {
        if is_reparse_point(&current_dir) {
            return Err(format!(
                "security violation: unexpected reparse point directory '{}' encountered during traversal",
                current_dir.display()
            ));
        }

        let entries = match std::fs::read_dir(&current_dir) {
            Ok(e) => e,
            Err(e) => return Err(format!("failed reading directory '{}': {e}", current_dir.display())),
        };

        for entry_res in entries {
            let entry = entry_res.map_err(|e| format!("failed reading dir entry: {e}"))?;
            let path = entry.path();

            if !path.starts_with(&canon_env) {
                return Err(format!(
                    "security violation: entry '{}' escaped environment boundary '{}'",
                    path.display(), canon_env.display()
                ));
            }

            let is_symlink_or_junction = is_reparse_point(&path);
            let is_dir = path.is_dir() && !is_symlink_or_junction;

            if is_symlink_or_junction {
                // Leaf entry: link must be deleted directly, NOT traversed!
                files_and_links.push(path);
            } else if is_dir {
                dirs.push(path.clone());
                stack.push(path);
            } else {
                files_and_links.push(path);
            }
        }
    }

    // Sort directories by depth descending (deepest first)
    dirs.sort_by(|a, b| b.components().count().cmp(&a.components().count()));

    // 1. Delete all leaf files and reparse links first
    for file_path in files_and_links {
        if let Ok(meta) = std::fs::symlink_metadata(&file_path) {
            let mut perms = meta.permissions();
            if perms.readonly() {
                perms.set_readonly(false);
                let _ = std::fs::set_permissions(&file_path, perms);
            }
        }

        let mut deleted = false;
        let mut last_err = String::new();
        for attempt in 0..5 {
            let res = if file_path.is_dir() && is_reparse_point(&file_path) {
                std::fs::remove_dir(&file_path)
            } else {
                std::fs::remove_file(&file_path)
            };

            match res {
                Ok(_) => {
                    deleted = true;
                    break;
                }
                Err(e) => {
                    last_err = e.to_string();
                    std::thread::sleep(std::time::Duration::from_millis(20 * (attempt + 1)));
                }
            }
        }

        if !deleted {
            return Err(format!("failed deleting file '{}': {last_err}", file_path.display()));
        }
    }

    // 2. Delete all subdirectories (deepest first)
    for dir_path in dirs {
        if let Ok(meta) = std::fs::symlink_metadata(&dir_path) {
            let mut perms = meta.permissions();
            if perms.readonly() {
                perms.set_readonly(false);
                let _ = std::fs::set_permissions(&dir_path, perms);
            }
        }

        let mut deleted = false;
        let mut last_err = String::new();
        for attempt in 0..5 {
            match std::fs::remove_dir(&dir_path) {
                Ok(_) => {
                    deleted = true;
                    break;
                }
                Err(e) => {
                    last_err = e.to_string();
                    std::thread::sleep(std::time::Duration::from_millis(20 * (attempt + 1)));
                }
            }
        }

        if !deleted {
            return Err(format!("failed removing empty directory '{}': {last_err}", dir_path.display()));
        }
    }

    // 3. Delete top environment directory
    if let Ok(meta) = std::fs::symlink_metadata(&canon_env) {
        let mut perms = meta.permissions();
        if perms.readonly() {
            perms.set_readonly(false);
            let _ = std::fs::set_permissions(&canon_env, perms);
        }
    }

    let mut top_deleted = false;
    let mut top_err = String::new();
    for attempt in 0..5 {
        match std::fs::remove_dir(&canon_env) {
            Ok(_) => {
                top_deleted = true;
                break;
            }
            Err(e) => {
                top_err = e.to_string();
                std::thread::sleep(std::time::Duration::from_millis(20 * (attempt + 1)));
            }
        }
    }

    if !top_deleted {
        return Err(format!("failed removing environment root '{}': {top_err}", canon_env.display()));
    }

    Ok((physical_bytes, file_count))
}

#[cfg(windows)]
pub fn get_disk_space_bytes(path: &Path) -> Result<(u64, u64), String> {
    use std::os::windows::ffi::OsStrExt;
    type BOOL = i32;
    type LPCWSTR = *const u16;

    extern "system" {
        fn GetDiskFreeSpaceExW(
            lpDirectoryName: LPCWSTR,
            lpFreeBytesAvailableToCaller: *mut u64,
            lpTotalNumberOfBytes: *mut u64,
            lpTotalNumberOfFreeBytes: *mut u64,
        ) -> BOOL;
    }

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut free_caller: u64 = 0;
    let mut total: u64 = 0;
    let mut free_total: u64 = 0;

    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_caller,
            &mut total,
            &mut free_total,
        )
    };

    if ok != 0 {
        Ok((free_caller, total))
    } else {
        let err = std::io::Error::last_os_error();
        Err(format!("GetDiskFreeSpaceExW failed for '{}': {err}", path.display()))
    }
}

#[cfg(not(windows))]
pub fn get_disk_space_bytes(_path: &Path) -> Result<(u64, u64), String> {
    Err("volume capacity query is not implemented on this platform".into())
}

pub fn get_disk_free_bytes(path: &Path) -> Result<u64, String> {
    get_disk_space_bytes(path).map(|space| space.0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn physical_directory_size_counts_hardlink_allocation_once() {
        let dir = std::env::temp_dir().join(format!("ulvo_size_links_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("original.bin");
        std::fs::write(&source, vec![0x5a; 5001]).unwrap();
        std::fs::hard_link(&source, dir.join("linked.bin")).unwrap();
        let allocation = crate::query_file_identity_and_size(&source).unwrap().1.allocated_bytes.unwrap();
        let (bytes, files) = super::calculate_physical_dir_size(&dir);
        assert_eq!(files, 2);
        assert_eq!(bytes, allocation);
    }
    #[cfg(windows)]
    #[test]
    fn atomic_write_supports_long_paths() {
        let root = std::env::temp_dir().join(format!("ulvo_atomic_long_{}", std::process::id()));
        let dir = root.join("nested".repeat(35));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("registry.json");
        super::write_atomic(&path, b"first").unwrap();
        super::write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"second");
    }
    use super::*;

    #[test]
    fn test_process_liveness_self() {
        let my_pid = std::process::id();
        let creation_time = get_process_creation_time(my_pid);
        let liveness = check_process_liveness(my_pid, creation_time);
        assert_eq!(liveness, ProcessLiveness::Alive);

        // Intentionally wrong creation time simulates recycled PID
        if creation_time.is_some() {
            let stale_liveness = check_process_liveness(my_pid, Some(1));
            assert_eq!(stale_liveness, ProcessLiveness::Terminated);
        }
    }

    #[test]
    fn test_process_liveness_dead_pid() {
        // High unused PID
        let liveness = check_process_liveness(99999999, None);
        assert_eq!(liveness, ProcessLiveness::Terminated);
    }

    #[test]
    fn test_environment_file_lock_mutual_exclusion() {
        let temp = std::env::temp_dir().join(format!("ulvo_lock_test_{}", std::process::id()));
        let lock_path = temp.join(".ulvo_lease.lock");

        let lock1 = EnvironmentFileLock::acquire(&lock_path, 1000).expect("first lock acquire succeeds");

        // Concurrent acquisition with short timeout should fail/timeout
        let lock2_res = EnvironmentFileLock::acquire(&lock_path, 100);
        assert!(lock2_res.is_err(), "concurrent lock acquisition must timeout");

        // Dropping first lock allows subsequent acquisition
        drop(lock1);
        let lock3 = EnvironmentFileLock::acquire(&lock_path, 1000).expect("subsequent lock acquire succeeds");
        drop(lock3);

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_atomic_write() {
        let temp = std::env::temp_dir().join(format!("ulvo_atomic_test_{}", std::process::id()));
        let file_path = temp.join("test_file.json");

        write_atomic(&file_path, b"{\"hello\":\"world\"}").expect("write_atomic succeeds");
        let content = std::fs::read_to_string(&file_path).expect("read file");
        assert_eq!(content, "{\"hello\":\"world\"}");

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_safe_reclaim_hierarchy_boundary_enforcement() {
        let temp = std::env::temp_dir().join(format!("ulvo_reclaim_bound_test_{}", std::process::id()));
        let vault_bound = temp.join("vault").join("environments");
        let valid_env = vault_bound.join("env_test");
        let outside_env = temp.join("outside_env");

        std::fs::create_dir_all(&valid_env).unwrap();
        std::fs::create_dir_all(&outside_env).unwrap();

        // Valid deletion within boundary
        let res = safe_reclaim_environment_hierarchy(&valid_env, &vault_bound);
        assert!(res.is_ok());
        assert!(!valid_env.exists());

        // Rejection when target is outside boundary
        let err = safe_reclaim_environment_hierarchy(&outside_env, &vault_bound);
        assert!(err.is_err());
        assert!(outside_env.exists());

        // Rejection when target is the boundary itself
        let err_boundary = safe_reclaim_environment_hierarchy(&vault_bound, &vault_bound);
        assert!(err_boundary.is_err());

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_safe_reclaim_clears_readonly_files() {
        let temp = std::env::temp_dir().join(format!("ulvo_reclaim_ro_test_{}", std::process::id()));
        let vault_bound = temp.join("vault").join("environments");
        let valid_env = vault_bound.join("env_ro");
        let sub_dir = valid_env.join("subdir");
        std::fs::create_dir_all(&sub_dir).unwrap();

        let ro_file = sub_dir.join("locked.txt");
        std::fs::write(&ro_file, "read-only content").unwrap();

        // Mark as read-only
        let mut perms = std::fs::metadata(&ro_file).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&ro_file, perms).unwrap();

        let res = safe_reclaim_environment_hierarchy(&valid_env, &vault_bound);
        assert!(res.is_ok(), "reclaim must succeed even with read-only files");
        assert!(!valid_env.exists());

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_job_supervisor_creation_and_assignment() {
        let supervisor = JobSupervisor::new();
        assert!(supervisor.is_ok(), "JobSupervisor::new() succeeds");
        let supervisor = supervisor.unwrap();

        #[cfg(windows)]
        {
            let mut cmd = Command::new("cmd.exe");
            cmd.args(&["/c", "ping", "-n", "2", "127.0.0.1"]);
            if let Ok(child) = cmd.spawn() {
                let assign_res = supervisor.assign_process(&child);
                assert!(assign_res.is_ok(), "assign_process succeeds");
                let count = supervisor.active_process_count();
                assert!(count >= 1, "active_process_count reflects assigned process");
                let mut child = child;
                let _ = child.wait();
            }
        }
    }

    #[test]
    fn test_multithreaded_concurrent_lock_contention() {
        use std::sync::{Arc, Mutex};
        let temp = std::env::temp_dir().join(format!("ulvo_mt_lock_test_{}", std::process::id()));
        let lock_path = temp.join(".ulvo_lease.lock");
        let counter = Arc::new(Mutex::new(0usize));
        let num_threads = 4;
        let mut handles = Vec::new();

        for _ in 0..num_threads {
            let lp = lock_path.clone();
            let c = Arc::clone(&counter);
            handles.push(std::thread::spawn(move || {
                for _ in 0..3 {
                    let lock = EnvironmentFileLock::acquire(&lp, 10000).expect("lock acquire");
                    let mut val = c.lock().unwrap();
                    *val += 1;
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    drop(lock);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(*counter.lock().unwrap(), num_threads * 3);
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_execute_in_environment_on_spawn_failure_kills_child() {
        let temp = std::env::temp_dir().join(format!("ulvo_spawn_fail_test_{}", std::process::id()));
        let vault_root = temp.join("vault");
        std::fs::create_dir_all(&vault_root).unwrap();

        if let Ok(uv_mgr) = UvManager::new(vault_root.clone()) {
            let env_dir = temp.join("fake_env");
            let scripts_dir = get_env_scripts(&env_dir);
            std::fs::create_dir_all(&scripts_dir).unwrap();

            let cmd_args = if cfg!(windows) {
                vec!["cmd.exe".to_string(), "/c".to_string(), "ping".to_string(), "-n".to_string(), "5".to_string(), "127.0.0.1".to_string()]
            } else {
                vec!["sleep".to_string(), "5".to_string()]
            };

            let res = uv_mgr.execute_in_environment_with_callback(
                &env_dir,
                &temp,
                &cmd_args,
                |_spawned_pid| Err("simulated on_spawn failure".to_string()),
            );

            assert!(res.is_err(), "execute_in_environment_with_callback must propagate on_spawn error");
            assert!(res.unwrap_err().contains("simulated on_spawn failure"));
        }

        let _ = std::fs::remove_dir_all(&temp);
    }
}
