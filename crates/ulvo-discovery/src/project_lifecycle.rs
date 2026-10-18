use std::path::{Path, PathBuf};
use std::time::SystemTime;
use ulvo_core::*;
use ulvo_adapters::{
    bounded_read, is_reparse_point, sha256_hex, UvManager,
};

pub struct ProjectOrchestrator {
    uv: UvManager,
    vault_root: PathBuf,
}

impl ProjectOrchestrator {
    pub fn new(vault_root: PathBuf) -> Result<Self, String> {
        let uv = UvManager::new(vault_root.clone())?;
        Ok(Self { uv, vault_root })
    }

    pub fn uv_manager(&self) -> &UvManager {
        &self.uv
    }

    pub fn resolve_project_root(&self, path: &Path) -> Result<PathBuf, String> {
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(path)
        };

        if abs.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err("parent traversal is not accepted; supply a normalized path".into());
        }

        let canonical = abs.canonicalize().map_err(|e| format!("project path '{}': {e}", abs.display()))?;
        if !canonical.is_dir() {
            return Err(format!("project path is not a directory: {}", canonical.display()));
        }
        Ok(canonical)
    }

    pub fn load_manifest(&self, project_root: &Path) -> Result<ProjectManifest, String> {
        let manifest_path = project_root.join(".ulvo").join("manifest.toml");
        if !manifest_path.is_file() {
            return Err(format!(
                "ULVO project manifest not found at {}. Run 'ulvo project init' or create .ulvo/manifest.toml",
                manifest_path.display()
            ));
        }

        let content = bounded_read(&manifest_path)?;
        let manifest: ProjectManifest = toml::from_str(&content)
            .map_err(|e| format!("invalid ULVO project manifest: {e}"))?;

        if manifest.schema_version != 1 {
            return Err("unsupported manifest schema_version (expected 1)".into());
        }

        Ok(manifest)
    }

    pub fn compute_environment_id(&self, project_root: &Path) -> String {
        let canonical_str = project_root.to_string_lossy().to_lowercase();
        let hash = sha256_hex(canonical_str.as_bytes());
        format!("env_{}", &hash[..16])
    }

    pub fn environment_path(&self, project_root: &Path) -> PathBuf {
        let env_id = self.compute_environment_id(project_root);
        self.uv.vault_paths().environments_dir.join(env_id)
    }

    pub fn inspect(&self, path: &Path) -> Result<ProjectInspection, String> {
        let root = self.resolve_project_root(path)?;
        crate::inspect(&root)
    }

    pub fn resolve(&self, path: &Path) -> Result<ProjectResolveReport, String> {
        let root = self.resolve_project_root(path)?;
        let manifest = self.load_manifest(&root)?;

        let py_req = manifest.runtimes.get("python")
            .cloned()
            .unwrap_or_else(|| ">=3.8".to_string());

        let resolved_py = self.uv.resolve_python_runtime(&py_req)?;
        let env_path = self.environment_path(&root);

        let mut req_files = Vec::new();
        if let Some(ref pm) = manifest.package_manager {
            for r in &pm.requirements {
                let r_path = root.join(r);
                if r_path.is_file() {
                    req_files.push(r.clone());
                }
            }
        }
        if req_files.is_empty() && root.join("requirements.txt").is_file() {
            req_files.push("requirements.txt".to_string());
        }

        let state = self.get_current_state(&env_path);

        Ok(ProjectResolveReport {
            schema_version: 1,
            project_path: root.to_string_lossy().into_owned(),
            project_name: manifest.project_name().unwrap_or("unnamed").to_string(),
            runtime_requested: py_req,
            runtime_resolved: resolved_py,
            package_manager: "uv".to_string(),
            requirement_files: req_files,
            environment_target_path: env_path.to_string_lossy().into_owned(),
            state,
        })
    }

    pub fn doctor(&self, path: &Path) -> Result<ProjectDoctorReport, String> {
        let root = self.resolve_project_root(path)?;
        let mut checks = Vec::new();

        // 1. Manifest check
        let manifest_res = self.load_manifest(&root);
        let manifest_valid = manifest_res.is_ok();
        checks.push(DoctorCheck {
            name: "Manifest Syntax".into(),
            passed: manifest_valid,
            message: match &manifest_res {
                Ok(m) => format!("Valid .ulvo/manifest.toml for project '{}'", m.project_name().unwrap_or("unnamed")),
                Err(e) => e.clone(),
            },
        });

        // 2. uv availability
        let uv_ver = self.uv.uv_version();
        let uv_available = uv_ver.is_some();
        checks.push(DoctorCheck {
            name: "uv Package Manager".into(),
            passed: uv_available,
            message: match &uv_ver {
                Some(v) => format!("Found: {v}"),
                None => "uv executable not found on PATH".into(),
            },
        });

        // 3. Runtime constraint resolution
        let mut runtime_req = None;
        let mut runtime_resolved = None;
        let mut runtime_available = false;
        if let Ok(ref m) = manifest_res {
            let req = m.runtimes.get("python").cloned().unwrap_or_else(|| ">=3.8".into());
            runtime_req = Some(req.clone());
            match self.uv.resolve_python_runtime(&req) {
                Ok(py) => {
                    runtime_available = true;
                    runtime_resolved = Some(py.clone());
                    checks.push(DoctorCheck {
                        name: "Python Runtime Resolution".into(),
                        passed: true,
                        message: format!("Resolved '{req}' to: {py}"),
                    });
                }
                Err(e) => {
                    checks.push(DoctorCheck {
                        name: "Python Runtime Resolution".into(),
                        passed: false,
                        message: e,
                    });
                }
            }
        }

        // 4. Dependencies
        let mut dependencies_resolvable = false;
        if let Ok(ref m) = manifest_res {
            let mut req_paths = Vec::new();
            if let Some(ref pm) = m.package_manager {
                for r in &pm.requirements {
                    let p = root.join(r);
                    if p.is_file() {
                        req_paths.push(p);
                    }
                }
            }
            if req_paths.is_empty() && root.join("requirements.txt").is_file() {
                req_paths.push(root.join("requirements.txt"));
            }

            if !req_paths.is_empty() {
                dependencies_resolvable = true;
                checks.push(DoctorCheck {
                    name: "Dependency Manifests".into(),
                    passed: true,
                    message: format!("Found {} requirement file(s)", req_paths.len()),
                });
            } else {
                checks.push(DoctorCheck {
                    name: "Dependency Manifests".into(),
                    passed: false,
                    message: "No requirements.txt or manifest dependencies found".into(),
                });
            }
        }

        let env_path = self.environment_path(&root);
        let state = self.get_current_state(&env_path);
        let active_pids = self.clean_dead_pids_and_get_active(&env_path);

        checks.push(DoctorCheck {
            name: "Environment Lifecycle State".into(),
            passed: true,
            message: format!("Current state: {state} ({} active process leases)", active_pids.len()),
        });

        Ok(ProjectDoctorReport {
            schema_version: 1,
            project_path: root.to_string_lossy().into_owned(),
            manifest_valid,
            uv_available,
            uv_version: uv_ver,
            runtime_requested: runtime_req,
            runtime_resolved,
            runtime_available,
            dependencies_resolvable,
            environment_state: state,
            active_processes: active_pids.len(),
            checks,
        })
    }

    pub fn status(&self, path: &Path) -> Result<EnvironmentStatusReport, String> {
        let root = self.resolve_project_root(path)?;
        let env_id = self.compute_environment_id(&root);
        let env_path = self.environment_path(&root);

        let is_materialized = env_path.is_dir() && env_path.join(".ulvo_ownership.json").is_file();
        let active_pids = self.clean_dead_pids_and_get_active(&env_path);
        let state = self.get_current_state(&env_path);
        let is_quiescent = active_pids.is_empty();
        let can_reclaim = is_materialized && is_quiescent && state.is_reclaimable();

        let py_exe = if is_materialized {
            let p = ulvo_adapters::get_env_python(&env_path);
            if p.is_file() { Some(p.to_string_lossy().into_owned()) } else { None }
        } else {
            None
        };

        let py_ver = if let Some(ref exe) = py_exe {
            std::process::Command::new(exe)
                .arg("--version")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
        } else {
            None
        };

        Ok(EnvironmentStatusReport {
            schema_version: 1,
            project_path: root.to_string_lossy().into_owned(),
            environment_id: env_id,
            environment_path: env_path.to_string_lossy().into_owned(),
            state,
            is_materialized,
            active_pids,
            is_quiescent,
            can_reclaim,
            python_executable: py_exe,
            python_version: py_ver,
        })
    }

    pub fn ensure_environment_ready(&self, project_root: &Path, manifest: &ProjectManifest) -> Result<PathBuf, String> {
        let env_path = self.environment_path(project_root);
        let ownership_file = env_path.join(".ulvo_ownership.json");

        if env_path.is_dir() && ownership_file.is_file() {
            // Environment exists and is owned by ULVO
            return Ok(env_path);
        }

        // Materialize
        let py_req = manifest.runtimes.get("python")
            .cloned()
            .unwrap_or_else(|| ">=3.8".to_string());
        let resolved_py = self.uv.resolve_python_runtime(&py_req)?;

        let mut req_files = Vec::new();
        if let Some(ref pm) = manifest.package_manager {
            for r in &pm.requirements {
                let p = project_root.join(r);
                if p.is_file() {
                    req_files.push(p);
                }
            }
        }
        if req_files.is_empty() && project_root.join("requirements.txt").is_file() {
            req_files.push(project_root.join("requirements.txt"));
        }

        // Record materializing state
        self.uv.materialize_environment(&env_path, Some(&resolved_py))?;

        if !req_files.is_empty() {
            self.uv.install_requirements(&env_path, &req_files)?;
        }

        // Write ownership record
        let record = EnvironmentOwnershipRecord {
            sentinel: "ulvo-managed-ephemeral-environment-v1".into(),
            environment_id: self.compute_environment_id(project_root),
            project_canonical_path: project_root.to_string_lossy().into_owned(),
            created_by_pid: std::process::id(),
            created_at_unix: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            vault_root: self.vault_root.to_string_lossy().into_owned(),
        };

        let rec_json = serde_json::to_string_pretty(&record)
            .map_err(|e| format!("failed to serialize ownership record: {e}"))?;
        std::fs::write(&ownership_file, rec_json)
            .map_err(|e| format!("failed to write ownership record: {e}"))?;

        // Write initial lease
        let lease = EnvironmentLease {
            lease_id: format!("lease_{}", std::process::id()),
            environment_id: record.environment_id.clone(),
            project_path: project_root.to_string_lossy().into_owned(),
            environment_path: env_path.to_string_lossy().into_owned(),
            state: EnvironmentLifecycleState::Ready,
            active_pids: Vec::new(),
            created_at_unix: record.created_at_unix,
            updated_at_unix: record.created_at_unix,
            lease_timeout_seconds: 300,
            owner_token: format!("pid_{}", std::process::id()),
            python_version: Some(resolved_py),
        };
        self.write_lease(&env_path, &lease)?;

        Ok(env_path)
    }

    pub fn run(
        &self,
        path: &Path,
        command_args: &[String],
        force_reclaim: bool,
    ) -> Result<i32, String> {
        let root = self.resolve_project_root(path)?;
        let manifest = self.load_manifest(&root)?;

        let env_path = self.ensure_environment_ready(&root, &manifest)?;
        let my_pid = std::process::id();

        // Acquire lease / transition to EXECUTING
        let mut lease = self.read_or_create_lease(&env_path, &root);
        if !lease.active_pids.contains(&my_pid) {
            lease.active_pids.push(my_pid);
        }
        lease.state = EnvironmentLifecycleState::Executing;
        lease.updated_at_unix = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.write_lease(&env_path, &lease)?;

        // Execute command in isolated environment
        let exit_code = self.uv.execute_in_environment(&env_path, &root, command_args)?;

        // Release lease / transition to RELEASED
        let mut lease = self.read_or_create_lease(&env_path, &root);
        lease.active_pids.retain(|&p| p != my_pid && self.uv.is_pid_alive(p));

        if lease.active_pids.is_empty() {
            lease.state = EnvironmentLifecycleState::Reclaimable;
        } else {
            lease.state = EnvironmentLifecycleState::Executing;
        }
        lease.updated_at_unix = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.write_lease(&env_path, &lease)?;

        // Reclaim if requested or if manifest policy is eager ephemeral
        let auto_reclaim = manifest.lifecycle.as_ref()
            .and_then(|l| l.allow_reclaim_on_quiescence)
            .unwrap_or(false);

        if (force_reclaim || auto_reclaim) && lease.active_pids.is_empty() {
            self.reclaim_internal(&env_path)?;
        }

        Ok(exit_code)
    }

    pub fn reclaim(&self, path: &Path) -> Result<EnvironmentReclaimReport, String> {
        let root = self.resolve_project_root(path)?;
        let env_id = self.compute_environment_id(&root);
        let env_path = self.environment_path(&root);

        if !env_path.exists() {
            return Ok(EnvironmentReclaimReport {
                schema_version: 1,
                project_path: root.to_string_lossy().into_owned(),
                environment_id: env_id,
                environment_path: env_path.to_string_lossy().into_owned(),
                reclaimed: false,
                bytes_reclaimed: 0,
                message: "Environment is not materialized or already reclaimed.".to_string(),
            });
        }

        let (bytes, _count) = ulvo_adapters::calculate_dir_size(&env_path);
        self.reclaim_internal(&env_path)?;

        Ok(EnvironmentReclaimReport {
            schema_version: 1,
            project_path: root.to_string_lossy().into_owned(),
            environment_id: env_id,
            environment_path: env_path.to_string_lossy().into_owned(),
            reclaimed: true,
            bytes_reclaimed: bytes,
            message: format!("Environment successfully reclaimed ({} bytes freed).", bytes),
        })
    }

    fn reclaim_internal(&self, env_path: &Path) -> Result<(), String> {
        if !env_path.exists() {
            return Ok(()); // Already reclaimed
        }

        // Safety gate 1: Must be inside vault environments dir
        let envs_dir = &self.uv.vault_paths().environments_dir;
        if !env_path.starts_with(envs_dir) {
            return Err(format!("refusing to delete environment outside vault: {}", env_path.display()));
        }

        // Safety gate 2: Verify ownership record and sentinel
        let ownership_file = env_path.join(".ulvo_ownership.json");
        if !ownership_file.is_file() {
            return Err("refusing to delete directory lacking ULVO ownership record".into());
        }
        let content = std::fs::read_to_string(&ownership_file).map_err(|e| e.to_string())?;
        if !content.contains("ulvo-managed-ephemeral-environment-v1") {
            return Err("ownership sentinel mismatch: refusing deletion".into());
        }

        // Safety gate 3: Check active processes
        let active = self.clean_dead_pids_and_get_active(env_path);
        if !active.is_empty() {
            return Err(format!(
                "cannot reclaim environment: {} active process(es) still running (PIDs: {:?})",
                active.len(), active
            ));
        }

        // Safety gate 4: Reject reparse points
        if is_reparse_point(env_path) {
            return Err(format!("refusing to delete reparse point: {}", env_path.display()));
        }

        // Delete environment directory only
        std::fs::remove_dir_all(env_path).map_err(|e| format!("failed to remove environment: {e}"))?;

        Ok(())
    }

    fn get_current_state(&self, env_path: &Path) -> EnvironmentLifecycleState {
        if !env_path.exists() {
            return EnvironmentLifecycleState::Defined;
        }
        let lease_file = env_path.join(".ulvo_lease.json");
        if let Ok(content) = std::fs::read_to_string(&lease_file) {
            if let Ok(lease) = serde_json::from_str::<EnvironmentLease>(&content) {
                let active = self.clean_dead_pids_and_get_active(env_path);
                if !active.is_empty() {
                    return EnvironmentLifecycleState::Executing;
                }
                return lease.state;
            }
        }
        if env_path.is_dir() && env_path.join(".ulvo_ownership.json").is_file() {
            EnvironmentLifecycleState::Ready
        } else {
            EnvironmentLifecycleState::Defined
        }
    }

    fn clean_dead_pids_and_get_active(&self, env_path: &Path) -> Vec<u32> {
        let lease_file = env_path.join(".ulvo_lease.json");
        if let Ok(content) = std::fs::read_to_string(&lease_file) {
            if let Ok(mut lease) = serde_json::from_str::<EnvironmentLease>(&content) {
                let initial_len = lease.active_pids.len();
                lease.active_pids.retain(|&pid| self.uv.is_pid_alive(pid));
                if lease.active_pids.len() != initial_len {
                    if lease.active_pids.is_empty() && lease.state == EnvironmentLifecycleState::Executing {
                        lease.state = EnvironmentLifecycleState::Reclaimable;
                    }
                    let _ = self.write_lease(env_path, &lease);
                }
                return lease.active_pids;
            }
        }
        Vec::new()
    }

    fn read_or_create_lease(&self, env_path: &Path, project_root: &Path) -> EnvironmentLease {
        let lease_file = env_path.join(".ulvo_lease.json");
        if let Ok(content) = std::fs::read_to_string(&lease_file) {
            if let Ok(lease) = serde_json::from_str::<EnvironmentLease>(&content) {
                return lease;
            }
        }
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        EnvironmentLease {
            lease_id: format!("lease_{}", std::process::id()),
            environment_id: self.compute_environment_id(project_root),
            project_path: project_root.to_string_lossy().into_owned(),
            environment_path: env_path.to_string_lossy().into_owned(),
            state: EnvironmentLifecycleState::Ready,
            active_pids: Vec::new(),
            created_at_unix: now,
            updated_at_unix: now,
            lease_timeout_seconds: 300,
            owner_token: format!("pid_{}", std::process::id()),
            python_version: None,
        }
    }

    fn write_lease(&self, env_path: &Path, lease: &EnvironmentLease) -> Result<(), String> {
        let lease_file = env_path.join(".ulvo_lease.json");
        let json = serde_json::to_string_pretty(lease)
            .map_err(|e| format!("failed to serialize lease: {e}"))?;
        std::fs::write(&lease_file, json)
            .map_err(|e| format!("failed to write lease: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SafeFixture {
        path: PathBuf,
    }

    impl SafeFixture {
        fn new(name: &str) -> Self {
            let pid = std::process::id();
            let ts = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("ulvo_test_{name}_{pid}_{ts}"));
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join(".ulvo_test_sentinel"), format!("pid={pid}\nts={ts}\n")).unwrap();
            Self { path }
        }
    }

    impl Drop for SafeFixture {
        fn drop(&mut self) {
            let sentinel = self.path.join(".ulvo_test_sentinel");
            if let Ok(content) = std::fs::read_to_string(&sentinel) {
                if content.contains(&format!("pid={}", std::process::id())) {
                    let _ = std::fs::remove_dir_all(&self.path);
                }
            }
        }
    }

    #[test]
    fn test_compute_environment_id_deterministic() {
        let vault_root = PathBuf::from(r"E:\Workspace\Active\ULVO\vault");
        let orch = ProjectOrchestrator::new(vault_root).unwrap();
        let path1 = PathBuf::from(r"C:\projects\my_app");
        let path2 = PathBuf::from(r"C:\projects\my_app");
        assert_eq!(orch.compute_environment_id(&path1), orch.compute_environment_id(&path2));
        assert!(orch.compute_environment_id(&path1).starts_with("env_"));
    }

    #[test]
    fn test_resolve_project_root_rejects_parent_traversal() {
        let vault_root = PathBuf::from(r"E:\Workspace\Active\ULVO\vault");
        let orch = ProjectOrchestrator::new(vault_root).unwrap();
        let traversal = Path::new("foo/../bar");
        assert!(orch.resolve_project_root(traversal).is_err());
    }

    #[test]
    fn test_reclaim_internal_outside_vault_rejected() {
        let vault_root = PathBuf::from(r"E:\Workspace\Active\ULVO\vault");
        let orch = ProjectOrchestrator::new(vault_root).unwrap();
        let outside = Path::new(r"C:\Windows\System32");
        assert!(orch.reclaim_internal(outside).is_err());
    }

    #[test]
    fn test_reclaim_internal_missing_ownership_rejected() {
        let fixture = SafeFixture::new("ownership_test");
        let vault_root = fixture.path.join("vault");
        let envs_dir = vault_root.join("environments").join("python");
        std::fs::create_dir_all(&envs_dir).unwrap();

        let orch = ProjectOrchestrator::new(vault_root).unwrap();
        let target_env = envs_dir.join("env_test");
        std::fs::create_dir_all(&target_env).unwrap();

        let res = orch.reclaim_internal(&target_env);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("lacking ULVO ownership record"));
    }

    #[test]
    fn test_reclaim_internal_sentinel_mismatch_rejected() {
        let fixture = SafeFixture::new("sentinel_test");
        let vault_root = fixture.path.join("vault");
        let envs_dir = vault_root.join("environments").join("python");
        std::fs::create_dir_all(&envs_dir).unwrap();

        let orch = ProjectOrchestrator::new(vault_root).unwrap();
        let target_env = envs_dir.join("env_test");
        std::fs::create_dir_all(&target_env).unwrap();

        let ownership_file = target_env.join(".ulvo_ownership.json");
        std::fs::write(&ownership_file, r#"{"sentinel": "counterfeit"}"#).unwrap();

        let res = orch.reclaim_internal(&target_env);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("sentinel mismatch"));
    }
}
