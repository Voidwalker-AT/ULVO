use std::path::{Path, PathBuf};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};
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
        let canon_vault = vault_root.canonicalize().unwrap_or(vault_root);
        let uv = UvManager::new(canon_vault.clone())?;
        Ok(Self { uv, vault_root: canon_vault })
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

    pub fn audit_connector(&self, path: &Path) -> Result<ConnectorContract, String> {
        let root = self.resolve_project_root(path)?;
        crate::connector::audit_project_connector(&root)
    }

    pub fn enforce_connector_contract(&self, path: &Path) -> Result<ConnectorContract, String> {
        let contract = self.audit_connector(path)?;
        match contract.compliance_verdict {
            ConnectorComplianceVerdict::MissingManifest => {
                return Err("CONNECTOR_CONTRACT_VIOLATION: Missing .ulvo/manifest.toml connector contract. Run 'ulvo migrate apply' or create .ulvo/manifest.toml.".into());
            }
            ConnectorComplianceVerdict::InvalidManifest => {
                return Err("CONNECTOR_CONTRACT_VIOLATION: Invalid .ulvo/manifest.toml syntax or unsupported schema_version.".into());
            }
            ConnectorComplianceVerdict::MaxBudgetExceeded => {
                return Err(format!(
                    "CONNECTOR_CONTRACT_VIOLATION: .ulvo directory exceeded maximum metadata budget of 10 MiB (current: {} bytes). The .ulvo folder is a connector, not an environment.",
                    contract.total_bytes
                ));
            }
            ConnectorComplianceVerdict::ProhibitedArtifactDetected => {
                return Err(format!(
                    "CONNECTOR_CONTRACT_VIOLATION: Prohibited environment artifacts detected inside .ulvo: {}. Runtimes, virtual environments, and caches must live in the vault.",
                    contract.prohibited_entries_detected.join(", ")
                ));
            }
            ConnectorComplianceVerdict::TargetBudgetExceeded => {
                eprintln!(
                    "Warning: .ulvo directory ({} bytes) exceeds recommended 100 KiB target budget.",
                    contract.total_bytes
                );
            }
            ConnectorComplianceVerdict::Compliant => {}
        }
        Ok(contract)
    }

    pub fn compute_environment_id(&self, project_root: &Path) -> String {
        let canonical_str = project_root.to_string_lossy().to_lowercase();
        let hash = sha256_hex(canonical_str.as_bytes());
        format!("env_{}", &hash[..16])
    }

    pub fn compute_environment_id_with_spec(&self, project_root: &Path, spec_hash: &str) -> String {
        let slug: String = project_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
            .collect();
        format!("env_{}_{}", slug, &spec_hash[..12])
    }

    pub fn compute_specification(
        &self,
        project_root: &Path,
        manifest: &ProjectManifest,
        resolved_py: &str,
    ) -> Result<(EnvironmentSpecification, String), String> {
        let manifest_path = project_root.join(".ulvo").join("manifest.toml");
        let manifest_content = bounded_read(&manifest_path)?;
        let manifest_hash = sha256_hex(manifest_content.as_bytes());

        let py_req = manifest.runtimes.get("python")
            .cloned()
            .unwrap_or_else(|| ">=3.8".to_string());

        let mut dep_fingerprints = Vec::new();
        let mut dep_paths = manifest.referenced_native_manifests();

        if let Some(ref pm) = manifest.package_manager {
            for r in &pm.requirements {
                if !dep_paths.contains(r) {
                    dep_paths.push(r.clone());
                }
            }
        }
        if dep_paths.is_empty() {
            for fallback in &[
                "requirements.txt",
                "pyproject.toml",
                "uv.lock",
                "package.json",
                "pnpm-lock.yaml",
                "Cargo.toml",
                "Cargo.lock",
                "go.mod",
                "pom.xml",
                "Gemfile",
            ] {
                if project_root.join(fallback).is_file() {
                    dep_paths.push(fallback.to_string());
                }
            }
        }

        for rel in &dep_paths {
            let full = project_root.join(rel);
            if full.is_file() {
                let content = bounded_read(&full)?;
                let hash = sha256_hex(content.as_bytes());
                let meta = std::fs::metadata(&full).map_err(|e| e.to_string())?;
                dep_fingerprints.push(DependencyManifestFingerprint {
                    relative_path: rel.clone(),
                    content_sha256: hash,
                    size_bytes: meta.len(),
                });
            }
        }

        dep_fingerprints.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

        let spec = EnvironmentSpecification {
            project_canonical_path: project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf()).to_string_lossy().into_owned(),
            runtime_ecosystem: "python".to_string(),
            runtime_constraint: py_req,
            // Normalize verbatim prefixes so `run` (vault_paths-joined runtime)
            // and `resolve` (uv python find) compute the same specification hash.
            runtime_resolved_path: strip_unc_prefix(Path::new(resolved_py)).to_string_lossy().into_owned(),
            package_manager: "uv".to_string(),
            manifest_hash,
            dependency_manifests: dep_fingerprints,
            platform: std::env::consts::OS.to_string(),
            architecture: std::env::consts::ARCH.to_string(),
        };

        let spec_json = serde_json::to_string(&spec).map_err(|e| e.to_string())?;
        let spec_hash = sha256_hex(spec_json.as_bytes());

        Ok((spec, spec_hash))
    }

    pub fn environment_path(&self, project_root: &Path) -> PathBuf {
        if let Ok(manifest) = self.load_manifest(project_root) {
            let py_req = manifest.runtimes.get("python").cloned().unwrap_or_else(|| ">=3.8".to_string());
            if let Ok(resolved_py) = self.uv.resolve_python_runtime(&py_req) {
                if let Ok((_spec, spec_hash)) = self.compute_specification(project_root, &manifest, &resolved_py) {
                    let env_id = self.compute_environment_id_with_spec(project_root, &spec_hash);
                    return self.uv.vault_paths().environments_dir.join(env_id);
                }
            }
        }
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
        let (spec, spec_hash) = self.compute_specification(&root, &manifest, &resolved_py)?;
        let env_id = self.compute_environment_id_with_spec(&root, &spec_hash);
        let env_path = self.uv.vault_paths().environments_dir.join(env_id);

        let req_files = spec.dependency_manifests.iter().map(|d| d.relative_path.clone()).collect();
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
            specification_hash: spec_hash,
            state,
        })
    }

    pub fn doctor(&self, path: &Path) -> Result<ProjectDoctorReport, String> {
        let root = self.resolve_project_root(path)?;
        let mut checks = Vec::new();

        // 0. Thin Connector Contract Check
        let connector_res = self.audit_connector(&root);
        let (connector_passed, connector_msg, connector_contract) = match connector_res {
            Ok(c) => {
                let passed = matches!(c.compliance_verdict, ConnectorComplianceVerdict::Compliant | ConnectorComplianceVerdict::TargetBudgetExceeded);
                let msg = format!("{}: {} (total: {} bytes across {} files)", c.compliance_verdict, if passed { "OK" } else { "Failed" }, c.total_bytes, c.file_count);
                (passed, msg, Some(c))
            }
            Err(e) => (false, format!("Failed to audit connector: {e}"), None),
        };
        checks.push(DoctorCheck {
            name: "Thin Connector Contract".into(),
            passed: connector_passed,
            message: connector_msg,
        });

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

        // 5. GPU hardware and capacity check
        if let Ok(ref m) = manifest_res {
            if let Some(ref gpu_cfg) = m.gpu {
                if gpu_cfg.required.unwrap_or(false) {
                    let host_gpu = Self::detect_host_gpu();
                    match host_gpu {
                        Some(ref gpu) => {
                            checks.push(DoctorCheck {
                                name: "GPU Hardware & Driver".into(),
                                passed: true,
                                message: format!("Found {} (Driver: {}, VRAM: {} MiB, Compute: {})",
                                    gpu.name, gpu.driver_version, gpu.vram_mib, gpu.compute_capability),
                            });
                        }
                        None => {
                            checks.push(DoctorCheck {
                                name: "GPU Hardware & Driver".into(),
                                passed: false,
                                message: "No compatible NVIDIA GPU or driver detected on host".into(),
                            });
                        }
                    }

                    let free_bytes = ulvo_adapters::get_disk_free_bytes(&self.vault_root).unwrap_or(0);
                    let safety_reserve = 524_288_000;
                    checks.push(DoctorCheck {
                        name: "Vault Storage Capacity".into(),
                        passed: free_bytes >= safety_reserve + 104_857_600,
                        message: format!("Free: {:.2} GB, Safety Reserve: {:.2} MB",
                            free_bytes as f64 / 1_073_741_824.0,
                            safety_reserve as f64 / 1_048_576.0),
                    });
                }
            }
        }

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
            connector: connector_contract,
        })
    }

    pub fn status(&self, path: &Path) -> Result<EnvironmentStatusReport, String> {
        let root = self.resolve_project_root(path)?;
        let env_path = self.environment_path(&root);
        let env_id = env_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();

        let is_materialized = env_path.is_dir() && env_path.join(".ulvo_ownership.json").is_file();
        let (active_pids, leased_processes, spec_hash) = self.inspect_lease_and_active_read_only(&env_path);
        let state = self.get_current_state(&env_path);
        let is_quiescent = leased_processes.is_empty();
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

        let connector = self.audit_connector(&root).ok();

        Ok(EnvironmentStatusReport {
            schema_version: 1,
            project_path: root.to_string_lossy().into_owned(),
            environment_id: env_id,
            environment_path: env_path.to_string_lossy().into_owned(),
            state,
            is_materialized,
            active_pids,
            leased_processes,
            is_quiescent,
            can_reclaim,
            python_executable: py_exe,
            python_version: py_ver,
            specification_hash: spec_hash,
            connector,
        })
    }

    pub fn ensure_environment_ready(&self, project_root: &Path, manifest: &ProjectManifest) -> Result<PathBuf, String> {
        let py_req = manifest.runtimes.get("python")
            .cloned()
            .unwrap_or_else(|| ">=3.8".to_string());
        let resolved_py = {
            let vault_managed_py = self.uv.vault_paths().runtimes_dir.join("cpython-3.11-windows-x86_64-none").join("python.exe");
            let vault_managed_py_full = self.uv.vault_paths().runtimes_dir.join("cpython-3.11.16-windows-x86_64-none").join("python.exe");
            if (py_req == ">=3.8" || py_req == ">=3.11" || py_req == "3.11" || py_req == "3.11.16") && vault_managed_py.is_file() {
                vault_managed_py.to_string_lossy().into_owned()
            } else if (py_req == ">=3.8" || py_req == ">=3.11" || py_req == "3.11" || py_req == "3.11.16") && vault_managed_py_full.is_file() {
                vault_managed_py_full.to_string_lossy().into_owned()
            } else {
                self.uv.resolve_python_runtime(&py_req)?
            }
        };
        let (spec, spec_hash) = self.compute_specification(project_root, manifest, &resolved_py)?;
        let env_id = self.compute_environment_id_with_spec(project_root, &spec_hash);
        let env_path = self.uv.vault_paths().environments_dir.join(&env_id);

        let ownership_file = env_path.join(".ulvo_ownership.json");
        let spec_file = env_path.join(".ulvo_spec.json");
        let lock_file = env_path.join(".ulvo_lease.lock");

        let can_reuse_environment = |env_dir: &Path| -> bool {
            let o_file = env_dir.join(".ulvo_ownership.json");
            let s_file = env_dir.join(".ulvo_spec.json");
            if !env_dir.is_dir() || !o_file.is_file() || !s_file.is_file() {
                return false;
            }
            let o_content = match std::fs::read_to_string(&o_file) {
                Ok(c) => c,
                Err(_) => return false,
            };
            let record: EnvironmentOwnershipRecord = match serde_json::from_str(&o_content) {
                Ok(r) => r,
                Err(_) => return false,
            };
            if record.validate(
                &project_root.to_string_lossy(),
                &self.vault_root.to_string_lossy(),
                &env_id,
            ).is_err() || record.specification_hash != spec_hash {
                return false;
            }
            let s_content = match std::fs::read_to_string(&s_file) {
                Ok(c) => c,
                Err(_) => return false,
            };
            let existing_spec: EnvironmentSpecification = match serde_json::from_str(&s_content) {
                Ok(s) => s,
                Err(_) => return false,
            };
            if existing_spec.runtime_resolved_path != resolved_py {
                let existing_p = Path::new(&existing_spec.runtime_resolved_path);
                let resolved_p = Path::new(&resolved_py);
                if existing_p.file_name() != resolved_p.file_name() {
                    return false;
                }
            }
            let env_py = ulvo_adapters::get_env_python(env_dir);
            if !env_py.is_file() {
                return false;
            }
            true
        };

        // Fast path: if materialized and valid ownership + spec + python executable, return immediately
        if can_reuse_environment(&env_path) {
            return Ok(env_path);
        }

        // Needs materialization. Acquire lock first.
        let _lock = ulvo_adapters::EnvironmentFileLock::acquire(&lock_file, 30000)?;

        // Re-check under lock in case another process materialized while waiting
        if can_reuse_environment(&env_path) {
            return Ok(env_path);
        }

        // Prior aborted / incomplete materialization cleanup before retrying
        if env_path.exists() && !can_reuse_environment(&env_path) {
            let envs_dir = &self.uv.vault_paths().environments_dir;
            let _ = ulvo_adapters::safe_reclaim_environment_hierarchy(&env_path, envs_dir);
        }

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

        // Materialize with transactional rollback on failure
        let materialize_res = (|| -> Result<(), String> {
            self.uv.materialize_environment(&env_path, Some(&resolved_py))?;

            if let Some(ref gpu_cfg) = manifest.gpu {
                if gpu_cfg.required.unwrap_or(false) {
                    // Enforce capacity check
                    let free_bytes = ulvo_adapters::get_disk_free_bytes(&self.vault_root).unwrap_or(0);
                    let safety_reserve = 524_288_000; // 500 MB
                    if free_bytes < safety_reserve + 104_857_600 {
                        return Err(format!(
                            "BLOCKED_CAPACITY: Insufficient free space on vault volume ({free_bytes} bytes free, 500 MB safety reserve required)"
                        ));
                    }

                    self.link_shared_packages(&env_path, project_root)?;
                }
            }

            // GPU packages are linked from the shared store as a supplement,
            // never a replacement: the full lockfile must still be installed
            // (uv skips already-satisfied distributions such as the linked torch).
            if !req_files.is_empty() {
                self.uv.install_requirements(&env_path, &req_files)?;
            }

            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            // Write ownership record
            let record = EnvironmentOwnershipRecord::new(
                env_id.clone(),
                project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf()).to_string_lossy().into_owned(),
                self.vault_root.canonicalize().unwrap_or_else(|_| self.vault_root.to_path_buf()).to_string_lossy().into_owned(),
                spec_hash.clone(),
                std::process::id(),
                now,
            );

            let rec_json = serde_json::to_string_pretty(&record)
                .map_err(|e| format!("failed to serialize ownership record: {e}"))?;
            ulvo_adapters::write_atomic(&ownership_file, rec_json.as_bytes())?;

            // Write spec file
            let spec_json = serde_json::to_string_pretty(&spec)
                .map_err(|e| format!("failed to serialize spec: {e}"))?;
            ulvo_adapters::write_atomic(&spec_file, spec_json.as_bytes())?;

            // Write initial lease
            let lease = EnvironmentLease {
                lease_id: format!("lease_{}", std::process::id()),
                environment_id: env_id,
                project_path: project_root.to_string_lossy().into_owned(),
                environment_path: env_path.to_string_lossy().into_owned(),
                state: EnvironmentLifecycleState::Ready,
                active_pids: Vec::new(),
                leased_processes: Vec::new(),
                created_at_unix: now,
                updated_at_unix: now,
                lease_timeout_seconds: 300,
                owner_token: format!("pid_{}", std::process::id()),
                python_version: Some(resolved_py),
                specification_hash: Some(spec_hash),
            };
            let lease_json = serde_json::to_string_pretty(&lease)
                .map_err(|e| format!("failed to serialize lease: {e}"))?;
            ulvo_adapters::write_atomic(&env_path.join(".ulvo_lease.json"), lease_json.as_bytes())?;

            Ok(())
        })();

        if let Err(e) = materialize_res {
            let envs_dir = &self.uv.vault_paths().environments_dir;
            let _ = ulvo_adapters::safe_reclaim_environment_hierarchy(&env_path, envs_dir);
            return Err(e);
        }

        Ok(env_path)
    }

    pub fn run_structured(
        &self,
        path: &Path,
        command_args: &[String],
        keep_env: bool,
    ) -> Result<RunExecutionResult, String> {
        let start_time = std::time::Instant::now();
        let root = self.resolve_project_root(path)?;
        self.enforce_connector_contract(&root)?;
        let manifest = self.load_manifest(&root)?;

        let disk_before = ulvo_adapters::get_disk_free_bytes(&self.vault_root).ok();

        let env_path = self.ensure_environment_ready(&root, &manifest)?;
        let lock_file = env_path.join(".ulvo_lease.lock");
        let my_pid = std::process::id();
        let my_creation = ulvo_adapters::get_process_creation_time(my_pid);

        // Acquire lease under lock
        {
            let _lock = ulvo_adapters::EnvironmentFileLock::acquire(&lock_file, 15000)?;
            let mut lease = self.read_or_create_lease(&env_path, &root);
            self.clean_dead_processes(&mut lease);

            if !lease.active_pids.contains(&my_pid) {
                lease.active_pids.push(my_pid);
            }
            if !lease.leased_processes.iter().any(|p| p.pid == my_pid) {
                lease.leased_processes.push(LeasedProcess {
                    pid: my_pid,
                    process_creation_time: my_creation,
                    owner_token: format!("pid_{my_pid}"),
                    registered_at_unix: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs(),
                });
            }
            lease.state = EnvironmentLifecycleState::Executing;
            lease.updated_at_unix = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
            self.write_lease(&env_path, &lease)?;
        }

        // Execute command in isolated environment with job supervision
        let env_path_clone = env_path.clone();
        let lock_file_clone = lock_file.clone();
        let exec_result = self.uv.execute_in_environment_with_callback(&env_path, &root, command_args, |child_pid| {
            // Register child PID in lease under lock
            let child_creation = ulvo_adapters::get_process_creation_time(child_pid);
            let _lock = ulvo_adapters::EnvironmentFileLock::acquire(&lock_file_clone, 5000)
                .map_err(|e| format!("failed to acquire lease lock for child process registration: {e}"))?;
            let mut lease = self.read_or_create_lease(&env_path_clone, &root);
            if !lease.active_pids.contains(&child_pid) {
                lease.active_pids.push(child_pid);
            }
            if !lease.leased_processes.iter().any(|p| p.pid == child_pid) {
                lease.leased_processes.push(LeasedProcess {
                    pid: child_pid,
                    process_creation_time: child_creation,
                    owner_token: format!("child_pid_{child_pid}"),
                    registered_at_unix: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs(),
                });
            }
            self.write_lease(&env_path_clone, &lease)
                .map_err(|e| format!("failed to write lease metadata for child process registration: {e}"))?;
            Ok(())
        });

        // Release lease under lock - GUARANTEED TO RUN regardless of exec_result!
        let is_persistent = manifest.lifecycle.as_ref()
            .and_then(|l| l.policy.as_deref())
            .map(|p| p.eq_ignore_ascii_case("persistent"))
            .unwrap_or(false);
        let explicit_no_reclaim = manifest.lifecycle.as_ref()
            .and_then(|l| l.allow_reclaim_on_quiescence)
            .map(|allow| !allow)
            .unwrap_or(false);
        let default_ephemeral = !is_persistent && !explicit_no_reclaim;
        let should_reclaim = default_ephemeral && !keep_env;

        let is_quiescent = {
            let _lock = ulvo_adapters::EnvironmentFileLock::acquire(&lock_file, 15000)?;
            let mut lease = self.read_or_create_lease(&env_path, &root);
            lease.active_pids.retain(|&p| p != my_pid);
            lease.leased_processes.retain(|p| p.pid != my_pid);
            self.clean_dead_processes(&mut lease);

            if lease.leased_processes.is_empty() && lease.active_pids.is_empty() {
                if should_reclaim {
                    lease.state = EnvironmentLifecycleState::Reclaimable;
                } else {
                    lease.state = EnvironmentLifecycleState::Ready;
                }
            } else {
                lease.state = EnvironmentLifecycleState::Executing;
            }
            lease.updated_at_unix = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
            self.write_lease(&env_path, &lease)?;
            lease.leased_processes.is_empty() && lease.active_pids.is_empty()
        };

        let cleanup_verdict = if keep_env {
            CleanupVerdict::RetainedByFlag
        } else if !default_ephemeral {
            CleanupVerdict::RetainedByPolicy
        } else if !is_quiescent {
            CleanupVerdict::SkippedActiveLeases
        } else if should_reclaim {
            match self.reclaim_internal_with_project(&env_path, Some(&root)) {
                Ok(()) => CleanupVerdict::ReclaimedSuccess,
                Err(e) => {
                    eprintln!(
                        "Warning: Post-execution environment reclamation failed for '{}': {}",
                        env_path.display(),
                        e
                    );
                    if let Ok(_lock) = ulvo_adapters::EnvironmentFileLock::acquire(&lock_file, 5000) {
                        let mut lease = self.read_or_create_lease(&env_path, &root);
                        lease.state = EnvironmentLifecycleState::FailedReclamation;
                        let _ = self.write_lease(&env_path, &lease);
                    }
                    CleanupVerdict::FailedReclamation { reason: e }
                }
            }
        } else {
            CleanupVerdict::RetainedByPolicy
        };

        // Verify project-local environment was not recreated during execution
        let local_venv = root.join(".venv");
        if local_venv.is_dir() {
            eprintln!(
                "Warning: Execution generated a project-local .venv directory at '{}'. ULVO projects should run without project-local environments.",
                local_venv.display()
            );
        }

        let disk_after = ulvo_adapters::get_disk_free_bytes(&self.vault_root).ok();
        let volume_free_delta = match (disk_before, disk_after) {
            (Some(b), Some(a)) => Some(a as i64 - b as i64),
            _ => None,
        };

        let residual_env = if env_path.exists() {
            Some(env_path.to_string_lossy().into_owned())
        } else {
            None
        };

        let active_state = self.get_current_state(&env_path);
        let duration_ms = start_time.elapsed().as_millis() as u64;

        let exit_code = exec_result?;

        Ok(RunExecutionResult {
            schema_version: 1,
            exit_code,
            duration_ms,
            cleanup_verdict,
            residual_environment_path: residual_env,
            active_lease_state: active_state,
            physical_storage_delta_bytes: volume_free_delta,
        })
    }

    pub fn run(
        &self,
        path: &Path,
        command_args: &[String],
        keep_env: bool,
    ) -> Result<i32, String> {
        let result = self.run_structured(path, command_args, keep_env)?;
        Ok(result.exit_code)
    }

    pub fn scan_and_reclaim_abandoned_environments(&self) -> Result<usize, String> {
        let envs_dir = &self.uv.vault_paths().environments_dir;
        if !envs_dir.is_dir() {
            return Ok(0);
        }
        let mut reclaimed = 0;
        if let Ok(entries) = std::fs::read_dir(envs_dir) {
            for entry in entries.flatten() {
                let ep = entry.path();
                if !ep.is_dir() {
                    continue;
                }
                let lease_file = ep.join(".ulvo_lease.json");
                let ownership_file = ep.join(".ulvo_ownership.json");
                if ownership_file.is_file() {
                    let mut can_clean = false;
                    if lease_file.is_file() {
                        if let Ok(content) = std::fs::read_to_string(&lease_file) {
                            if let Ok(mut lease) = serde_json::from_str::<EnvironmentLease>(&content) {
                                self.clean_dead_processes(&mut lease);
                                if lease.leased_processes.is_empty() && (
                                    lease.state == EnvironmentLifecycleState::FailedReclamation
                                    || lease.state == EnvironmentLifecycleState::Executing
                                ) {
                                    can_clean = true;
                                }
                            }
                        }
                    } else {
                        // Orphaned environment with missing lease file
                        can_clean = true;
                    }

                    if can_clean {
                        if self.reclaim_internal(&ep).is_ok() {
                            reclaimed += 1;
                        }
                    }
                }
            }
        }
        Ok(reclaimed)
    }

    pub fn reclaim(&self, path: &Path) -> Result<EnvironmentReclaimReport, String> {
        let root = self.resolve_project_root(path)?;
        let env_path = self.environment_path(&root);
        let env_id = env_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();

        if !env_path.exists() {
            return Ok(EnvironmentReclaimReport {
                schema_version: 1,
                project_path: root.to_string_lossy().into_owned(),
                environment_id: env_id,
                environment_path: env_path.to_string_lossy().into_owned(),
                reclaimed: false,
                bytes_reclaimed: 0,
                physical_bytes_reclaimed: 0,
                volume_free_bytes_delta: None,
                file_count_reclaimed: 0,
                message: "Environment is not materialized or already reclaimed.".to_string(),
            });
        }

        let disk_before = ulvo_adapters::get_disk_free_bytes(&self.vault_root).ok();
        let (phys_bytes, file_count) = ulvo_adapters::calculate_physical_dir_size(&env_path);

        self.reclaim_internal_with_project(&env_path, Some(&root))?;

        let disk_after = ulvo_adapters::get_disk_free_bytes(&self.vault_root).ok();
        let volume_free_delta = match (disk_before, disk_after) {
            (Some(b), Some(a)) => Some(a as i64 - b as i64),
            _ => None,
        };

        Ok(EnvironmentReclaimReport {
            schema_version: 1,
            project_path: root.to_string_lossy().into_owned(),
            environment_id: env_id,
            environment_path: env_path.to_string_lossy().into_owned(),
            reclaimed: true,
            bytes_reclaimed: phys_bytes,
            physical_bytes_reclaimed: phys_bytes,
            volume_free_bytes_delta: volume_free_delta,
            file_count_reclaimed: file_count,
            message: format!("Environment successfully reclaimed ({} physical bytes freed across {} files).", phys_bytes, file_count),
        })
    }

    pub fn reclaim_internal(&self, env_path: &Path) -> Result<(), String> {
        self.reclaim_internal_with_project(env_path, None)
    }

    pub fn reclaim_internal_with_project(&self, env_path: &Path, expected_project: Option<&Path>) -> Result<(), String> {
        if !env_path.exists() {
            return Ok(()); // Already reclaimed
        }

        // Safety gate 1: Must be inside vault environments dir
        let envs_dir = &self.uv.vault_paths().environments_dir;
        let canon_envs_dir = envs_dir.canonicalize()
            .map_err(|e| format!("failed canonicalizing vault environments directory: {e}"))?;
        let canon_env = env_path.canonicalize()
            .map_err(|e| format!("failed canonicalizing environment path: {e}"))?;

        if !canon_env.starts_with(&canon_envs_dir) {
            return Err(format!("refusing to delete environment outside vault: {}", env_path.display()));
        }

        if canon_env == canon_envs_dir {
            return Err("refusing to delete vault environments directory itself".to_string());
        }

        let env_id = env_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Safety gate 2: Verify full ownership record
        let ownership_file = env_path.join(".ulvo_ownership.json");
        if !ownership_file.is_file() {
            return Err("refusing to delete directory lacking ULVO ownership record".into());
        }

        let content = std::fs::read_to_string(&ownership_file).map_err(|e| e.to_string())?;
        let record = serde_json::from_str::<EnvironmentOwnershipRecord>(&content)
            .map_err(|e| format!("ownership sentinel mismatch: refusing deletion ({e})"))?;

        if let Some(proj) = expected_project {
            let canon_proj = proj.canonicalize().unwrap_or_else(|_| proj.to_path_buf());
            record.validate(
                &canon_proj.to_string_lossy(),
                &self.vault_root.to_string_lossy(),
                env_id,
            ).map_err(|e| format!("ownership record validation failed: refusing deletion ({e})"))?;
        } else {
            // Validate all structural fields without a specific expected project
            if record.sentinel != ulvo_core::EPHEMERAL_OWNERSHIP_SENTINEL {
                return Err("ownership sentinel mismatch: refusing deletion".into());
            }
            if record.schema_version != 1 {
                return Err(format!("unsupported ownership schema version {}: refusing deletion", record.schema_version));
            }
            if !ulvo_core::canonical_paths_equal(&record.vault_root, &self.vault_root.to_string_lossy()) {
                return Err(format!(
                    "ownership vault root mismatch (recorded '{}' != expected '{}'): refusing deletion",
                    record.vault_root, self.vault_root.display()
                ));
            }
            if record.environment_id != env_id {
                return Err(format!(
                    "ownership environment_id mismatch (recorded '{}' != expected '{}'): refusing deletion",
                    record.environment_id, env_id
                ));
            }
            if record.project_canonical_path.trim().is_empty() {
                return Err("ownership project_canonical_path is empty: refusing deletion".into());
            }
            if record.specification_hash.trim().is_empty() {
                return Err("ownership specification_hash is empty: refusing deletion".into());
            }
        }

        // Safety gate 3: Check active processes and lock. Malformed lease MUST fail closed!
        let lock_file = env_path.join(".ulvo_lease.lock");
        {
            let _lock = ulvo_adapters::EnvironmentFileLock::acquire(&lock_file, 5000)?;
            let lease_file = env_path.join(".ulvo_lease.json");
            if !lease_file.is_file() {
                return Err(format!("missing lease metadata in {}: refusing reclamation", lease_file.display()));
            }
            let lcontent = std::fs::read_to_string(&lease_file)
                .map_err(|e| format!("cannot read lease metadata in {}: refusing reclamation ({e})", lease_file.display()))?;
            let mut lease = serde_json::from_str::<EnvironmentLease>(&lcontent)
                .map_err(|e| format!("corrupted or malformed lease metadata in {}: refusing reclamation ({e})", lease_file.display()))?;
            self.clean_dead_processes(&mut lease);
            if !lease.leased_processes.is_empty() {
                return Err(format!(
                    "cannot reclaim environment: {} active process(es) still running (PIDs: {:?})",
                    lease.leased_processes.len(),
                    lease.leased_processes.iter().map(|p| p.pid).collect::<Vec<_>>()
                ));
            }
        } // Lock is dropped here before deletion so .ulvo_lease.lock is not held open!

        // Safety gate 4: Reparse point rejection
        if is_reparse_point(env_path) {
            return Err(format!("refusing to delete reparse point: {}", env_path.display()));
        }

        // Safety gate 5: Leaf-first bottom-up safe deletion
        ulvo_adapters::safe_reclaim_environment_hierarchy(env_path, envs_dir)?;

        Ok(())
    }

    fn get_current_state(&self, env_path: &Path) -> EnvironmentLifecycleState {
        if !env_path.exists() {
            return EnvironmentLifecycleState::Defined;
        }
        let lease_file = env_path.join(".ulvo_lease.json");
        if let Ok(content) = std::fs::read_to_string(&lease_file) {
            if let Ok(mut lease) = serde_json::from_str::<EnvironmentLease>(&content) {
                self.clean_dead_processes(&mut lease);
                if !lease.leased_processes.is_empty() {
                    return EnvironmentLifecycleState::Executing;
                }
                if lease.state == EnvironmentLifecycleState::Executing {
                    return EnvironmentLifecycleState::Reclaimable;
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

    fn inspect_lease_and_active_read_only(&self, env_path: &Path) -> (Vec<u32>, Vec<LeasedProcess>, Option<String>) {
        let lease_file = env_path.join(".ulvo_lease.json");
        if let Ok(content) = std::fs::read_to_string(&lease_file) {
            if let Ok(mut lease) = serde_json::from_str::<EnvironmentLease>(&content) {
                self.clean_dead_processes(&mut lease);
                return (lease.active_pids, lease.leased_processes, lease.specification_hash);
            }
        }
        (Vec::new(), Vec::new(), None)
    }

    fn clean_dead_pids_and_get_active(&self, env_path: &Path) -> Vec<u32> {
        let (pids, _, _) = self.inspect_lease_and_active_read_only(env_path);
        pids
    }

    fn clean_dead_processes(&self, lease: &mut EnvironmentLease) {
        lease.leased_processes.retain(|lp| {
            let liveness = ulvo_adapters::check_process_liveness(lp.pid, lp.process_creation_time);
            // CRITICAL FAIL-CLOSED RULE:
            // Alive -> keep
            // Unknown -> keep (blocks reclamation!)
            // Terminated -> remove
            match liveness {
                ProcessLiveness::Alive | ProcessLiveness::Unknown => true,
                ProcessLiveness::Terminated => false,
            }
        });

        lease.active_pids.retain(|&pid| {
            let liveness = ulvo_adapters::check_process_liveness(pid, None);
            match liveness {
                ProcessLiveness::Alive | ProcessLiveness::Unknown => true,
                ProcessLiveness::Terminated => false,
            }
        });
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

        let env_id = env_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();

        EnvironmentLease {
            lease_id: format!("lease_{}", std::process::id()),
            environment_id: env_id,
            project_path: project_root.to_string_lossy().into_owned(),
            environment_path: env_path.to_string_lossy().into_owned(),
            state: EnvironmentLifecycleState::Ready,
            active_pids: Vec::new(),
            leased_processes: Vec::new(),
            created_at_unix: now,
            updated_at_unix: now,
            lease_timeout_seconds: 300,
            owner_token: format!("pid_{}", std::process::id()),
            python_version: None,
            specification_hash: None,
        }
    }

    fn write_lease(&self, env_path: &Path, lease: &EnvironmentLease) -> Result<(), String> {
        let lease_file = env_path.join(".ulvo_lease.json");
        let json = serde_json::to_string_pretty(lease)
            .map_err(|e| format!("failed to serialize lease: {e}"))?;
        ulvo_adapters::write_atomic(&lease_file, json.as_bytes())
    }

    /// Returns exact physical allocation across all managed package-manager caches.
    pub fn cache_status(&self) -> Result<PackageCacheStatusReport, String> {
        let vp = &self.vault_root;
        let packages_dir = vp.join("packages");
        let uv_cache = packages_dir.join("python").join("uv-cache");
        let shared_pkgs = packages_dir.join("python").join("shared");
        let cas_dir = packages_dir.join("cas");
        let cargo_registry = vp.join("runtimes").join("rust").join("cargo").join("registry");

        let mut package_managers = Vec::new();
        let mut total_physical_bytes = 0u64;
        let mut total_files = 0usize;

        // 1. Python UV Cache (Reclaimable)
        if uv_cache.is_dir() {
            let (bytes, files) = ulvo_adapters::calculate_physical_dir_size(&uv_cache);
            total_physical_bytes += bytes;
            total_files += files;
            package_managers.push(PackageManagerCacheStats {
                name: "Python (uv-cache)".to_string(),
                cache_path: uv_cache.to_string_lossy().into_owned(),
                physical_bytes: bytes,
                file_count: files,
                classification: CacheClassification::Reclaimable,
                detail: "Extracted wheel archives and download caches".to_string(),
            });
        }

        // 2. Python Shared Package Store (Essential)
        if shared_pkgs.is_dir() {
            let (bytes, files) = ulvo_adapters::calculate_physical_dir_size(&shared_pkgs);
            total_physical_bytes += bytes;
            total_files += files;
            package_managers.push(PackageManagerCacheStats {
                name: "Python (shared)".to_string(),
                cache_path: shared_pkgs.to_string_lossy().into_owned(),
                physical_bytes: bytes,
                file_count: files,
                classification: CacheClassification::Essential,
                detail: "Verified shared immutable packages (e.g. PyTorch/CUDA)".to_string(),
            });
        }

        // 3. Content-Addressable Store (Essential)
        if cas_dir.is_dir() {
            let (bytes, files) = ulvo_adapters::calculate_physical_dir_size(&cas_dir);
            total_physical_bytes += bytes;
            total_files += files;
            package_managers.push(PackageManagerCacheStats {
                name: "Universal CAS".to_string(),
                cache_path: cas_dir.to_string_lossy().into_owned(),
                physical_bytes: bytes,
                file_count: files,
                classification: CacheClassification::Essential,
                detail: "Content-addressed immutable artifact store".to_string(),
            });
        }

        // 4. Cargo / Rust Registry (Reclaimable)
        if cargo_registry.is_dir() {
            let (bytes, files) = ulvo_adapters::calculate_physical_dir_size(&cargo_registry);
            total_physical_bytes += bytes;
            total_files += files;
            package_managers.push(PackageManagerCacheStats {
                name: "Rust (Cargo)".to_string(),
                cache_path: cargo_registry.to_string_lossy().into_owned(),
                physical_bytes: bytes,
                file_count: files,
                classification: CacheClassification::Reclaimable,
                detail: "Cargo crates.io index and downloaded .crate archives".to_string(),
            });
        }

        Ok(PackageCacheStatusReport {
            schema_version: 1,
            vault_packages_dir: packages_dir.to_string_lossy().into_owned(),
            total_physical_bytes,
            total_files,
            package_managers,
        })
    }

    /// Prunes unreferenced wheels, expired metadata, and superseded package versions from the cache.
    pub fn cache_prune(&self, dry_run: bool) -> Result<PackageCachePruneReport, String> {
        let vp = &self.vault_root;
        let uv_cache = vp.join("packages").join("python").join("uv-cache");
        let disk_before = ulvo_adapters::get_disk_free_bytes(vp).ok();

        let mut unreferenced_found = 0usize;
        let mut unreferenced_reclaimed = 0usize;
        let mut physical_reclaimed = 0u64;
        let mut details = Vec::new();

        // 1. Run uv cache prune via uv executable
        if let Ok(cmd) = std::process::Command::new(self.uv.uv_executable())
            .arg("cache")
            .arg("prune")
            .arg("--cache-dir")
            .arg(&uv_cache)
            .output()
        {
            let uv_msg = String::from_utf8_lossy(&cmd.stdout).trim().to_string();
            if !uv_msg.is_empty() {
                details.push(format!("uv cache prune: {uv_msg}"));
            }
        }

        // 2. Scan archive-v0 for unreferenced extracted archives with link_count == 1
        let archive_dir = uv_cache.join("archive-v0");
        if archive_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&archive_dir) {
                for entry in entries.flatten() {
                    let ep = entry.path();
                    if !ep.is_dir() {
                        continue;
                    }
                    let is_unreferenced = if let Ok(mut files) = std::fs::read_dir(&ep) {
                        if let Some(Ok(sf)) = files.next() {
                            if let Ok((_id, info)) = ulvo_adapters::query_file_identity_and_size(&sf.path()) {
                                info.link_count == 1
                            } else {
                                false
                            }
                        } else {
                            true
                        }
                    } else {
                        false
                    };

                    if is_unreferenced {
                        let (bytes, count) = ulvo_adapters::calculate_physical_dir_size(&ep);
                        unreferenced_found += 1;
                        if !dry_run {
                            if std::fs::remove_dir_all(&ep).is_ok() {
                                unreferenced_reclaimed += 1;
                                physical_reclaimed += bytes;
                                details.push(format!("reclaimed unreferenced archive {} ({} bytes, {} files)", entry.file_name().to_string_lossy(), bytes, count));
                            }
                        } else {
                            details.push(format!("would reclaim unreferenced archive {} ({} bytes, {} files)", entry.file_name().to_string_lossy(), bytes, count));
                        }
                    }
                }
            }
        }

        let disk_after = ulvo_adapters::get_disk_free_bytes(vp).ok();
        let volume_delta = match (disk_before, disk_after) {
            (Some(b), Some(a)) => Some(a as i64 - b as i64),
            _ => None,
        };

        Ok(PackageCachePruneReport {
            schema_version: 1,
            vault_packages_dir: uv_cache.to_string_lossy().into_owned(),
            unreferenced_archives_found: unreferenced_found,
            unreferenced_archives_reclaimed: unreferenced_reclaimed,
            physical_bytes_reclaimed: physical_reclaimed,
            volume_free_bytes_delta: volume_delta,
            is_dry_run: dry_run,
            details,
        })
    }

    /// Deep garbage collection subject to a configurable storage budget.
    pub fn cache_gc(&self, max_gb: Option<f64>, dry_run: bool) -> Result<PackageCacheGcReport, String> {
        let vp = &self.vault_root;
        let uv_cache = vp.join("packages").join("python").join("uv-cache");
        let (bytes_before, _) = ulvo_adapters::calculate_physical_dir_size(&uv_cache);
        let disk_before = ulvo_adapters::get_disk_free_bytes(vp).ok();

        let budget_bytes = max_gb.map(|gb| (gb * 1_073_741_824.0) as u64);
        let mut reclaimed_packages = Vec::new();
        let mut physical_reclaimed = 0u64;

        if let Some(budget) = budget_bytes {
            if bytes_before > budget {
                let excess = bytes_before - budget;
                let archive_dir = uv_cache.join("archive-v0");
                if archive_dir.is_dir() {
                    if let Ok(entries) = std::fs::read_dir(&archive_dir) {
                        let mut candidates = Vec::new();
                        for entry in entries.flatten() {
                            let ep = entry.path();
                            if !ep.is_dir() {
                                continue;
                            }
                            let meta = entry.metadata().ok();
                            let mtime = meta.and_then(|m| m.modified().ok()).unwrap_or(SystemTime::UNIX_EPOCH);
                            let (bytes, _) = ulvo_adapters::calculate_physical_dir_size(&ep);
                            candidates.push((ep, bytes, mtime));
                        }
                        candidates.sort_by_key(|c| c.2);

                        for (path, bytes, _) in candidates {
                            if physical_reclaimed >= excess {
                                break;
                            }
                            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                            if !dry_run {
                                if std::fs::remove_dir_all(&path).is_ok() {
                                    physical_reclaimed += bytes;
                                    reclaimed_packages.push(name);
                                }
                            } else {
                                physical_reclaimed += bytes;
                                reclaimed_packages.push(name);
                            }
                        }
                    }
                }
            }
        } else {
            let prune_rep = self.cache_prune(dry_run)?;
            physical_reclaimed = prune_rep.physical_bytes_reclaimed;
            reclaimed_packages = prune_rep.details;
        }

        let bytes_after = if !dry_run {
            let (b, _) = ulvo_adapters::calculate_physical_dir_size(&uv_cache);
            b
        } else {
            bytes_before.saturating_sub(physical_reclaimed)
        };

        let disk_after = ulvo_adapters::get_disk_free_bytes(vp).ok();
        let volume_delta = match (disk_before, disk_after) {
            (Some(b), Some(a)) => Some(a as i64 - b as i64),
            _ => None,
        };

        Ok(PackageCacheGcReport {
            schema_version: 1,
            vault_packages_dir: uv_cache.to_string_lossy().into_owned(),
            target_budget_bytes: budget_bytes,
            bytes_before,
            bytes_after,
            physical_bytes_reclaimed: physical_reclaimed,
            volume_free_bytes_delta: volume_delta,
            is_dry_run: dry_run,
            reclaimed_packages,
        })
    }

    pub fn detect_host_gpu() -> Option<HostGpuInfo> {
        // First extract runtime CUDA version from full nvidia-smi banner if available
        let mut runtime_cuda = None;
        if let Ok(full_out) = std::process::Command::new("nvidia-smi").output() {
            if full_out.status.success() {
                let text = String::from_utf8_lossy(&full_out.stdout);
                for line in text.lines() {
                    if line.contains("CUDA Version:") {
                        if let Some(pos) = line.find("CUDA Version:") {
                            let part = &line[pos + "CUDA Version:".len()..];
                            let cuda_val = part.split_whitespace().next().unwrap_or("").trim_matches('|').trim();
                            if !cuda_val.is_empty() {
                                runtime_cuda = Some(cuda_val.to_string());
                                break;
                            }
                        }
                    }
                }
            }
        }

        let out = std::process::Command::new("nvidia-smi")
            .arg("--query-gpu=name,driver_version,memory.total")
            .arg("--format=csv,noheader,nounits")
            .output()
            .ok()?;
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            let line = s.lines().next()?;
            let parts: Vec<&str> = line.split(',').map(|p| p.trim()).collect();
            if parts.len() >= 3 {
                let name = parts[0].to_string();
                let driver = parts[1].to_string();
                let vram = parts[2].parse().unwrap_or(0);
                let cc = if name.contains("RTX 2050") || name.contains("RTX 30") {
                    "sm_86".to_string()
                } else if name.contains("RTX 40") {
                    "sm_89".to_string()
                } else {
                    "sm_75".to_string()
                };

                let parsed_driver = ulvo_core::NvidiaDriverVersion::parse_with_runtime_cuda(&driver, runtime_cuda.as_deref());
                let (max_cuda, source_str) = match parsed_driver {
                    Some(d) => (d.max_cuda_support, format!("{:?}", d.compatibility_source)),
                    None => (runtime_cuda, "Unknown".to_string()),
                };

                return Some(HostGpuInfo {
                    name,
                    driver_version: driver,
                    vram_mib: vram,
                    compute_capability: cc,
                    max_cuda_version: max_cuda,
                    cuda_source: source_str,
                });
            }
        }
        None
    }

    pub fn link_shared_packages(&self, env_path: &Path, project_root: &Path) -> Result<(), String> {
        let env_site_packages = env_path.join("Lib").join("site-packages");
        std::fs::create_dir_all(&env_site_packages).map_err(|e| e.to_string())?;

        let shared_store = self.uv.vault_paths().shared_packages_dir.clone();
        std::fs::create_dir_all(&shared_store).map_err(|e| e.to_string())?;

        // 1. Inspect target Python ABI tag
        let python_exe = env_path.join("Scripts").join("python.exe");
        let target_py_tag = if python_exe.is_file() {
            if let Ok(out) = std::process::Command::new(&python_exe).arg("-V").output() {
                let s = String::from_utf8_lossy(&out.stdout);
                let ver_str = s.trim().strip_prefix("Python ").unwrap_or(s.trim());
                let parts: Vec<&str> = ver_str.split('.').collect();
                if parts.len() >= 2 {
                    format!("cp{}{}", parts[0], parts[1])
                } else {
                    "cp311".to_string()
                }
            } else {
                "cp311".to_string()
            }
        } else {
            "cp311".to_string()
        };

        let host_gpu = Self::detect_host_gpu();

        // 2. Verified transactional seeding from candidate project if available
        let local_candidate_site = project_root.join(".venv").join("Lib").join("site-packages");
        if local_candidate_site.is_dir() {
            Self::seed_verified_gpu_packages(&local_candidate_site, &shared_store, &target_py_tag, host_gpu.as_ref())?;
        }

        // 3. Verified transactional linking into target environment site-packages
        Self::link_verified_gpu_packages(&shared_store, &env_site_packages, &target_py_tag, host_gpu.as_ref())?;

        Ok(())
    }

    fn seed_verified_gpu_packages(
        source_site: &Path,
        shared_store: &Path,
        target_py_tag: &str,
        host_gpu: Option<&HostGpuInfo>,
    ) -> Result<(), String> {
        let packages_to_check = ["torch", "torchvision", "torchaudio"];
        for pkg in &packages_to_check {
            let pkg_dir = source_site.join(pkg);
            if !pkg_dir.is_dir() {
                continue;
            }

            // Find matching .dist-info
            let dist_info = Self::find_dist_info(source_site, pkg);
            if let Some(dist_dir) = dist_info {
                Self::verify_and_copy_distribution(
                    &pkg_dir,
                    &dist_dir,
                    shared_store,
                    target_py_tag,
                    host_gpu,
                )?;
            }
        }
        Ok(())
    }

    fn link_verified_gpu_packages(
        shared_store: &Path,
        target_site: &Path,
        target_py_tag: &str,
        _host_gpu: Option<&HostGpuInfo>,
    ) -> Result<(), String> {
        if !shared_store.is_dir() {
            return Ok(());
        }

        if let Ok(entries) = std::fs::read_dir(shared_store) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                // Skip hidden entries and cache
                if name.starts_with('.') || name == "__pycache__" {
                    continue;
                }

                // Handle single-file modules (e.g. typing_extensions.py, six.py)
                if path.is_file() {
                    let target_dest = target_site.join(&name);
                    let _ = std::fs::copy(&path, &target_dest);
                    continue;
                }

                if !path.is_dir() {
                    continue;
                }

                // If this is a dist-info directory, verify its wheel tags before copying
                if name.ends_with(".dist-info") {
                    let wheel_file = path.join("WHEEL");
                    if wheel_file.is_file() {
                        let content = std::fs::read_to_string(&wheel_file).unwrap_or_default();
                        let is_compat = content.lines().any(|l| {
                            let lower = l.to_lowercase();
                            lower.starts_with("tag:") && (lower.contains("py3-none-any") || lower.contains(target_py_tag))
                        });
                        if !is_compat {
                            continue; // Skip incompatible Python ABI distributions
                        }
                    }
                }

                let target_dest = target_site.join(&name);
                ulvo_adapters::hardlink_or_copy_tree(&path, &target_dest)
                    .map_err(|e| format!("failed to materialize verified shared package '{name}': {e}"))?;
            }
        }

        Ok(())
    }

    fn find_dist_info(site_packages: &Path, pkg_name: &str) -> Option<PathBuf> {
        let prefix = format!("{}-", pkg_name.replace('-', "_"));
        let alt_prefix = format!("{}-", pkg_name);
        if let Ok(entries) = std::fs::read_dir(site_packages) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".dist-info") && (name.starts_with(&prefix) || name.starts_with(&alt_prefix)) {
                    return Some(entry.path());
                }
            }
        }
        None
    }

    fn verify_and_copy_distribution(
        pkg_dir: &Path,
        dist_info_dir: &Path,
        shared_store: &Path,
        target_py_tag: &str,
        host_gpu: Option<&HostGpuInfo>,
    ) -> Result<(), String> {
        // Read WHEEL metadata
        let wheel_file = dist_info_dir.join("WHEEL");
        if !wheel_file.is_file() {
            return Err(format!("corrupted package distribution in '{}': missing WHEEL metadata", dist_info_dir.display()));
        }
        let wheel_content = std::fs::read_to_string(&wheel_file).map_err(|e| e.to_string())?;

        let tag_matches = wheel_content.lines().any(|line| {
            let lower = line.to_lowercase();
            lower.starts_with("tag:") && (lower.contains("py3-none-any") || lower.contains(target_py_tag))
        });

        if !tag_matches {
            return Err(format!(
                "ABI_MISMATCH: Package in '{}' does not match target Python tag '{}'",
                dist_info_dir.display(), target_py_tag
            ));
        }

        // Verify CUDA compatibility if GPU required
        let metadata_file = dist_info_dir.join("METADATA");
        if metadata_file.is_file() {
            let meta_content = std::fs::read_to_string(&metadata_file).unwrap_or_default();
            for line in meta_content.lines() {
                if line.starts_with("Version:") {
                    let ver = line["Version:".len()..].trim();
                    if let Some(pos) = ver.find("+cu") {
                        let cu_ver = &ver[pos + 1..];
                        if let Some(gpu) = host_gpu {
                            if let Some(ref max_cu) = gpu.max_cuda_version {
                                // e.g. cu124 requires max_cu >= 12.4
                                let cu_num = cu_ver.strip_prefix("cu").unwrap_or(cu_ver);
                                let max_num = max_cu.replace('.', "");
                                if cu_num.len() >= 3 && max_num.len() >= 3 {
                                    if cu_num > &max_num[..cu_num.len()] {
                                        return Err(format!(
                                            "CUDA_INCOMPATIBLE: Package requires CUDA {} but host driver only supports up to CUDA {}",
                                            cu_ver, max_cu
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Transactional publication: Copy into staging directory first, then publish
        let staging_parent = shared_store.join(".staging");
        std::fs::create_dir_all(&staging_parent).map_err(|e| e.to_string())?;

        let pkg_name = pkg_dir.file_name().unwrap_or_default();
        let dist_name = dist_info_dir.file_name().unwrap_or_default();

        let stage_pkg = staging_parent.join(format!("{}_{}", pkg_name.to_string_lossy(), std::process::id()));
        let stage_dist = staging_parent.join(format!("{}_{}", dist_name.to_string_lossy(), std::process::id()));

        ulvo_adapters::hardlink_or_copy_tree(pkg_dir, &stage_pkg)
            .map_err(|e| format!("failed to stage package '{:?}': {e}", pkg_name))?;
        ulvo_adapters::hardlink_or_copy_tree(dist_info_dir, &stage_dist)
            .map_err(|e| format!("failed to stage metadata '{:?}': {e}", dist_name))?;

        let final_pkg = shared_store.join(pkg_name);
        let final_dist = shared_store.join(dist_name);

        if !final_pkg.exists() {
            let _ = std::fs::rename(&stage_pkg, &final_pkg);
        } else {
            let _ = std::fs::remove_dir_all(&stage_pkg);
        }

        if !final_dist.exists() {
            let _ = std::fs::rename(&stage_dist, &final_dist);
        } else {
            let _ = std::fs::remove_dir_all(&stage_dist);
        }

        Ok(())
    }

    /// Counts live or conservatively unknown recorded leases without walking
    /// package payloads. Status telemetry must not perform a storage audit.
    pub fn active_leased_environment_count(&self) -> Result<usize, String> {
        let root = self.vault_root.join("environments/python");
        if !root.exists() { return Ok(0); }
        ulvo_adapters::reject_links(&root)?;
        let entries = std::fs::read_dir(&root).map_err(|e| e.to_string())?;
        let mut count = 0;
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            if ulvo_adapters::is_reparse_point(&path) || !path.is_dir() { continue; }
            let lease = path.join(".ulvo_lease.json");
            ulvo_adapters::reject_links(&lease)?;
            if lease.is_file() {
                let content = ulvo_adapters::bounded_read(&lease)?;
                let mut lease: EnvironmentLease = serde_json::from_str(&content)
                    .map_err(|e| format!("cannot inspect lease metadata: {e}"))?;
                self.clean_dead_processes(&mut lease);
                if !lease.active_pids.is_empty() || !lease.leased_processes.is_empty() { count += 1; }
            }
        }
        Ok(count)
    }

    pub fn audit_vault_environments(&self) -> Result<VaultEnvironmentsAuditReport, String> {
        let envs_dir = self.uv.vault_paths().environments_dir.clone();
        if !envs_dir.is_dir() {
            return Ok(VaultEnvironmentsAuditReport {
                schema_version: 1,
                vault_environments_dir: envs_dir.to_string_lossy().into_owned(),
                total_environments: 0,
                active_environments: 0,
                reclaimable_environments: 0,
                aborted_environments: 0,
                unknown_environments: 0,
                total_logical_bytes: 0,
                total_physical_bytes: 0,
                items: Vec::new(),
            });
        }

        let mut items = Vec::new();
        let mut active_count = 0;
        let mut reclaimable_count = 0;
        let mut aborted_count = 0;
        let mut unknown_count = 0;
        let mut total_logical = 0u64;
        let mut total_physical = 0u64;

        if let Ok(entries) = std::fs::read_dir(&envs_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                if !name.starts_with("env_") {
                    unknown_count += 1;
                    continue;
                }

                let ownership_file = path.join(".ulvo_ownership.json");
                let lease_file = path.join(".ulvo_lease.json");
                let (phys_bytes, file_count) = ulvo_adapters::calculate_physical_dir_size(&path);
                total_physical += phys_bytes;

                // Compute logical bytes
                let mut logical_bytes = 0u64;
                let mut stack = vec![path.clone()];
                while let Some(curr) = stack.pop() {
                    if is_reparse_point(&curr) {
                        continue;
                    }
                    if let Ok(sub_entries) = std::fs::read_dir(&curr) {
                        for sub in sub_entries.flatten() {
                            let sp = sub.path();
                            if is_reparse_point(&sp) {
                                continue;
                            }
                            if sp.is_dir() {
                                stack.push(sp);
                            } else if sp.is_file() {
                                if let Ok(meta) = sp.metadata() {
                                    logical_bytes += meta.len();
                                }
                            }
                        }
                    }
                }
                total_logical += logical_bytes;

                if ownership_file.is_file() {
                    let mut is_quiescent = true;
                    let mut active_pids = Vec::new();
                    let mut state = EnvironmentLifecycleState::Reclaimable;
                    let mut proj_path = None;
                    let mut spec_hash = None;

                    if let Ok(content) = std::fs::read_to_string(&ownership_file) {
                        if let Ok(rec) = serde_json::from_str::<EnvironmentOwnershipRecord>(&content) {
                            proj_path = Some(rec.project_canonical_path);
                            spec_hash = Some(rec.specification_hash);
                        }
                    }

                    if lease_file.is_file() {
                        if let Ok(content) = std::fs::read_to_string(&lease_file) {
                            if let Ok(mut lease) = serde_json::from_str::<EnvironmentLease>(&content) {
                                self.clean_dead_processes(&mut lease);
                                is_quiescent = lease.leased_processes.is_empty() && lease.active_pids.is_empty();
                                active_pids = lease.active_pids.clone();
                                state = if !is_quiescent {
                                    EnvironmentLifecycleState::Executing
                                } else {
                                    EnvironmentLifecycleState::Reclaimable
                                };
                            }
                        }
                    }

                    // Check active system processes referencing environment path
                    if is_quiescent {
                        if let Ok(sys_pids) = ulvo_adapters::check_active_processes_for_env(&path) {
                            if !sys_pids.is_empty() {
                                is_quiescent = false;
                                active_pids = sys_pids;
                                state = EnvironmentLifecycleState::Executing;
                            }
                        }
                    }

                    let can_reclaim = is_quiescent;
                    if can_reclaim {
                        reclaimable_count += 1;
                    } else {
                        active_count += 1;
                    }

                    items.push(VaultEnvironmentAuditItem {
                        environment_id: name,
                        environment_path: path.to_string_lossy().into_owned(),
                        project_path: proj_path,
                        specification_hash: spec_hash,
                        state,
                        is_materialized: true,
                        is_quiescent,
                        can_reclaim,
                        is_aborted_materialization: false,
                        active_pids,
                        logical_bytes,
                        physical_bytes: phys_bytes,
                        file_count,
                    });
                } else {
                    // Check for aborted materialization
                    let has_cfg = path.join("pyvenv.cfg").is_file();
                    let has_lock = path.join(".ulvo_lease.lock").is_file();
                    let has_scripts = path.join("Scripts").join("python.exe").is_file();

                    if has_cfg || has_lock || has_scripts {
                        let mut active_pids = Vec::new();
                        if let Ok(sys_pids) = ulvo_adapters::check_active_processes_for_env(&path) {
                            active_pids = sys_pids;
                        }
                        let is_quiescent = active_pids.is_empty();
                        if is_quiescent {
                            aborted_count += 1;
                        } else {
                            active_count += 1;
                        }

                        items.push(VaultEnvironmentAuditItem {
                            environment_id: name,
                            environment_path: path.to_string_lossy().into_owned(),
                            project_path: None,
                            specification_hash: None,
                            state: EnvironmentLifecycleState::Failed,
                            is_materialized: false,
                            is_quiescent,
                            can_reclaim: is_quiescent,
                            is_aborted_materialization: true,
                            active_pids,
                            logical_bytes,
                            physical_bytes: phys_bytes,
                            file_count,
                        });
                    } else {
                        unknown_count += 1;
                    }
                }
            }
        }

        let total_environments = items.len();

        Ok(VaultEnvironmentsAuditReport {
            schema_version: 1,
            vault_environments_dir: envs_dir.to_string_lossy().into_owned(),
            total_environments,
            active_environments: active_count,
            reclaimable_environments: reclaimable_count,
            aborted_environments: aborted_count,
            unknown_environments: unknown_count,
            total_logical_bytes: total_logical,
            total_physical_bytes: total_physical,
            items,
        })
    }

    pub fn reclaim_aborted_materialization(&self, env_path: &Path) -> Result<u64, String> {
        let envs_dir = &self.uv.vault_paths().environments_dir;
        let canon_envs_dir = envs_dir.canonicalize()
            .map_err(|e| format!("failed canonicalizing vault environments directory: {e}"))?;
        let canon_env = env_path.canonicalize()
            .map_err(|e| format!("failed canonicalizing environment path: {e}"))?;

        if !canon_env.starts_with(&canon_envs_dir) || canon_env == canon_envs_dir {
            return Err("refusing to delete environment outside vault environments directory".into());
        }

        if is_reparse_point(env_path) {
            return Err(format!("refusing to delete reparse point: {}", env_path.display()));
        }

        let name = env_path.file_name().unwrap_or_default().to_string_lossy().to_string();
        if !name.starts_with("env_") {
            return Err(format!("refusing to delete unrecognized directory: {name}"));
        }

        // Verify no active processes referencing this path
        let active = ulvo_adapters::check_active_processes_for_env(env_path)
            .map_err(|e| format!("cannot inspect active processes for {}: {e}", env_path.display()))?;
        if !active.is_empty() {
            return Err(format!("refusing to delete aborted environment: {} active process(es) running (PIDs: {:?})", active.len(), active));
        }

        let lock_file = env_path.join(".ulvo_lease.lock");
        let (phys_bytes, _) = ulvo_adapters::calculate_physical_dir_size(env_path);

        if lock_file.is_file() {
            let _lock = ulvo_adapters::EnvironmentFileLock::acquire(&lock_file, 5000)
                .map_err(|e| format!("cannot acquire lock for aborted materialization {}: {e}", env_path.display()))?;
        }

        ulvo_adapters::safe_reclaim_environment_hierarchy(env_path, envs_dir)?;
        Ok(phys_bytes)
    }

    pub fn reclaim_all_inactive_environments(&self, dry_run: bool) -> Result<VaultEnvironmentsGcReport, String> {
        let audit = self.audit_vault_environments()?;
        let envs_dir = self.uv.vault_paths().environments_dir.clone();
        let disk_before = ulvo_adapters::get_disk_free_bytes(&self.vault_root).ok();

        let mut reclaimed_items = Vec::new();
        let mut physical_bytes_reclaimed = 0u64;
        let mut reclaimed_count = 0;
        let mut blocked_active_count = audit.active_environments;
        let preserved_unknown_count = audit.unknown_environments;

        for item in &audit.items {
            if !item.can_reclaim {
                continue;
            }

            let env_path = PathBuf::from(&item.environment_path);
            if dry_run {
                reclaimed_count += 1;
                physical_bytes_reclaimed += item.physical_bytes;
                reclaimed_items.push(EnvironmentReclaimReport {
                    schema_version: 1,
                    project_path: item.project_path.clone().unwrap_or_else(|| "unknown".into()),
                    environment_id: item.environment_id.clone(),
                    environment_path: item.environment_path.clone(),
                    reclaimed: true,
                    bytes_reclaimed: item.physical_bytes,
                    physical_bytes_reclaimed: item.physical_bytes,
                    volume_free_bytes_delta: None,
                    file_count_reclaimed: item.file_count,
                    message: format!("[DRY RUN] Would reclaim {} physical bytes across {} files", item.physical_bytes, item.file_count),
                });
            } else {
                let rec_res = if item.is_aborted_materialization {
                    self.reclaim_aborted_materialization(&env_path)
                } else {
                    self.reclaim_internal(&env_path).map(|_| item.physical_bytes)
                };

                match rec_res {
                    Ok(freed) => {
                        reclaimed_count += 1;
                        physical_bytes_reclaimed += freed;
                        reclaimed_items.push(EnvironmentReclaimReport {
                            schema_version: 1,
                            project_path: item.project_path.clone().unwrap_or_else(|| "unknown".into()),
                            environment_id: item.environment_id.clone(),
                            environment_path: item.environment_path.clone(),
                            reclaimed: true,
                            bytes_reclaimed: freed,
                            physical_bytes_reclaimed: freed,
                            volume_free_bytes_delta: None,
                            file_count_reclaimed: item.file_count,
                            message: format!("Successfully reclaimed {} physical bytes across {} files", freed, item.file_count),
                        });
                    }
                    Err(e) => {
                        blocked_active_count += 1;
                        reclaimed_items.push(EnvironmentReclaimReport {
                            schema_version: 1,
                            project_path: item.project_path.clone().unwrap_or_else(|| "unknown".into()),
                            environment_id: item.environment_id.clone(),
                            environment_path: item.environment_path.clone(),
                            reclaimed: false,
                            bytes_reclaimed: 0,
                            physical_bytes_reclaimed: 0,
                            volume_free_bytes_delta: None,
                            file_count_reclaimed: 0,
                            message: format!("Reclamation blocked: {e}"),
                        });
                    }
                }
            }
        }

        let disk_after = ulvo_adapters::get_disk_free_bytes(&self.vault_root).ok();
        let volume_free_delta = match (disk_before, disk_after) {
            (Some(b), Some(a)) => Some(a as i64 - b as i64),
            _ => None,
        };

        Ok(VaultEnvironmentsGcReport {
            schema_version: 1,
            vault_environments_dir: envs_dir.to_string_lossy().into_owned(),
            total_scanned: audit.total_environments,
            reclaimed_count,
            blocked_active_count,
            preserved_unknown_count,
            physical_bytes_reclaimed,
            volume_free_bytes_delta: volume_free_delta,
            reclaimed_items,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostGpuInfo {
    pub name: String,
    pub driver_version: String,
    pub vram_mib: u64,
    pub compute_capability: String,
    #[serde(default)]
    pub max_cuda_version: Option<String>,
    #[serde(default)]
    pub cuda_source: String,
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

    #[test]
    fn test_specification_invalidation_on_dependency_change() {
        let fixture = SafeFixture::new("spec_change_test");
        let project_dir = fixture.path.join("my_proj");
        std::fs::create_dir_all(project_dir.join(".ulvo")).unwrap();

        std::fs::write(
            project_dir.join(".ulvo").join("manifest.toml"),
            r#"
schema_version = 1
name = "my_proj"
[runtimes]
python = ">=3.8"
[package_manager]
name = "uv"
requirements = ["requirements.txt"]
"#,
        ).unwrap();

        let req_path = project_dir.join("requirements.txt");
        std::fs::write(&req_path, "fastapi==0.100.0\n").unwrap();

        let vault_root = fixture.path.join("vault");
        let orch = ProjectOrchestrator::new(vault_root).unwrap();
        let manifest = orch.load_manifest(&project_dir).unwrap();

        let (_spec1, hash1) = orch.compute_specification(&project_dir, &manifest, "python.exe").unwrap();

        // Mutate requirements
        std::fs::write(&req_path, "fastapi==0.100.0\npydantic==2.0\n").unwrap();
        let (_spec2, hash2) = orch.compute_specification(&project_dir, &manifest, "python.exe").unwrap();

        assert_ne!(hash1, hash2, "specification hash must change when requirements change!");
    }

    #[test]
    fn test_reclaim_internal_corrupted_lease_rejected() {
        let fixture = SafeFixture::new("corrupt_lease_test");
        let vault_root = fixture.path.join("vault");
        let envs_dir = vault_root.join("environments").join("python");
        std::fs::create_dir_all(&envs_dir).unwrap();

        let orch = ProjectOrchestrator::new(vault_root.clone()).unwrap();
        let target_env = envs_dir.join("env_test");
        std::fs::create_dir_all(&target_env).unwrap();

        let ownership_file = target_env.join(".ulvo_ownership.json");
        let record = EnvironmentOwnershipRecord::new(
            "env_test".to_string(),
            fixture.path.to_string_lossy().to_string(),
            vault_root.to_string_lossy().to_string(),
            "sha256_mock_hash".to_string(),
            std::process::id(),
            1000,
        );
        std::fs::write(&ownership_file, serde_json::to_string(&record).unwrap()).unwrap();

        // Corrupted lease
        let lease_file = target_env.join(".ulvo_lease.json");
        std::fs::write(&lease_file, "{ malformed json").unwrap();

        let res = orch.reclaim_internal(&target_env);
        assert!(res.is_err(), "must fail closed on corrupted lease metadata");
        assert!(res.unwrap_err().contains("corrupted or malformed lease metadata"));
    }

    #[test]
    fn test_reclaim_internal_mismatched_vault_root_rejected() {
        let fixture = SafeFixture::new("mismatch_vault_test");
        let vault_root = fixture.path.join("vault");
        let envs_dir = vault_root.join("environments").join("python");
        std::fs::create_dir_all(&envs_dir).unwrap();

        let orch = ProjectOrchestrator::new(vault_root).unwrap();
        let target_env = envs_dir.join("env_test");
        std::fs::create_dir_all(&target_env).unwrap();

        let ownership_file = target_env.join(".ulvo_ownership.json");
        let record = EnvironmentOwnershipRecord::new(
            "env_test".to_string(),
            fixture.path.to_string_lossy().to_string(),
            r"C:\foreign\vault".to_string(),
            "sha256_mock_hash".to_string(),
            std::process::id(),
            1000,
        );
        std::fs::write(&ownership_file, serde_json::to_string(&record).unwrap()).unwrap();

        let res = orch.reclaim_internal(&target_env);
        assert!(res.is_err(), "must reject when ownership vault root does not match");
        assert!(res.unwrap_err().contains("vault root mismatch"));
    }

    #[test]
    fn test_reclaim_internal_mismatched_env_id_rejected() {
        let fixture = SafeFixture::new("mismatch_envid_test");
        let vault_root = fixture.path.join("vault");
        let envs_dir = vault_root.join("environments").join("python");
        std::fs::create_dir_all(&envs_dir).unwrap();

        let orch = ProjectOrchestrator::new(vault_root.clone()).unwrap();
        let target_env = envs_dir.join("env_actual_dir");
        std::fs::create_dir_all(&target_env).unwrap();

        let ownership_file = target_env.join(".ulvo_ownership.json");
        let record = EnvironmentOwnershipRecord::new(
            "env_recorded_different".to_string(),
            fixture.path.to_string_lossy().to_string(),
            vault_root.to_string_lossy().to_string(),
            "sha256_mock_hash".to_string(),
            std::process::id(),
            1000,
        );
        std::fs::write(&ownership_file, serde_json::to_string(&record).unwrap()).unwrap();

        let res = orch.reclaim_internal(&target_env);
        assert!(res.is_err(), "must reject when recorded environment_id does not match directory");
        assert!(res.unwrap_err().contains("environment_id mismatch"));
    }

    #[test]
    fn test_status_is_strictly_readonly() {
        let fixture = SafeFixture::new("readonly_status_test");
        let project_dir = fixture.path.join("my_proj");
        std::fs::create_dir_all(project_dir.join(".ulvo")).unwrap();
        std::fs::write(
            project_dir.join(".ulvo").join("manifest.toml"),
            r#"
schema_version = 1
name = "my_proj"
[runtimes]
python = ">=3.8"
"#,
        ).unwrap();

        let vault_root = fixture.path.join("vault");
        let orch = ProjectOrchestrator::new(vault_root).unwrap();
        let env_path = orch.environment_path(&project_dir);
        std::fs::create_dir_all(&env_path).unwrap();

        let lease_file = env_path.join(".ulvo_lease.json");
        let dead_pid = 99999999u32;
        let initial_lease = EnvironmentLease {
            lease_id: "test_lease".to_string(),
            environment_id: env_path.file_name().unwrap().to_string_lossy().to_string(),
            project_path: project_dir.to_string_lossy().to_string(),
            environment_path: env_path.to_string_lossy().to_string(),
            state: EnvironmentLifecycleState::Executing,
            active_pids: vec![dead_pid],
            leased_processes: vec![LeasedProcess {
                pid: dead_pid,
                process_creation_time: None,
                owner_token: "test".to_string(),
                registered_at_unix: 1000,
            }],
            created_at_unix: 1000,
            updated_at_unix: 1000,
            lease_timeout_seconds: 300,
            owner_token: "test".to_string(),
            python_version: None,
            specification_hash: None,
        };

        std::fs::write(&lease_file, serde_json::to_string_pretty(&initial_lease).unwrap()).unwrap();
        let bytes_before = std::fs::read(&lease_file).unwrap();

        // Run status check
        let report = orch.status(&project_dir).unwrap();
        assert!(report.leased_processes.is_empty(), "dead process filtered in memory");

        // Verify lease file on disk was NOT mutated
        let bytes_after = std::fs::read(&lease_file).unwrap();
        assert_eq!(bytes_before, bytes_after, "status() must not mutate .ulvo_lease.json on disk");
    }

    #[test]
    fn test_enforce_connector_contract_rejects_missing_manifest() {
        let fixture = SafeFixture::new("connector_missing_test");
        let project_dir = fixture.path.join("empty_proj");
        std::fs::create_dir_all(&project_dir).unwrap();

        let vault_root = fixture.path.join("vault");
        let orch = ProjectOrchestrator::new(vault_root).unwrap();

        let err = orch.enforce_connector_contract(&project_dir).unwrap_err();
        assert!(err.contains("CONNECTOR_CONTRACT_VIOLATION: Missing .ulvo/manifest.toml"));
    }

    #[test]
    fn test_enforce_connector_contract_rejects_prohibited_artifact() {
        let fixture = SafeFixture::new("connector_prohibited_test");
        let project_dir = fixture.path.join("bloated_proj");
        let ulvo_dir = project_dir.join(".ulvo");
        let venv_dir = ulvo_dir.join(".venv");
        std::fs::create_dir_all(&venv_dir).unwrap();

        std::fs::write(
            ulvo_dir.join("manifest.toml"),
            "schema_version = 1\nname = \"bloated_proj\"\n[runtimes]\npython = \">=3.8\"\n",
        ).unwrap();

        let vault_root = fixture.path.join("vault");
        let orch = ProjectOrchestrator::new(vault_root).unwrap();

        let err = orch.enforce_connector_contract(&project_dir).unwrap_err();
        assert!(err.contains("CONNECTOR_CONTRACT_VIOLATION: Prohibited environment artifacts detected"));
    }

    #[test]
    fn test_enforce_connector_contract_rejects_budget_exceeded() {
        let fixture = SafeFixture::new("connector_budget_test");
        let project_dir = fixture.path.join("oversized_proj");
        let ulvo_dir = project_dir.join(".ulvo");
        std::fs::create_dir_all(&ulvo_dir).unwrap();

        std::fs::write(
            ulvo_dir.join("manifest.toml"),
            "schema_version = 1\nname = \"oversized_proj\"\n[runtimes]\npython = \">=3.8\"\n",
        ).unwrap();

        // Write a file that exceeds the 10 MiB maximum budget (11 MiB)
        let big_file = ulvo_dir.join("oversized_dump.dat");
        let chunk = vec![0u8; 1024 * 1024]; // 1 MiB
        let mut f = std::fs::File::create(&big_file).unwrap();
        use std::io::Write;
        for _ in 0..11 {
            f.write_all(&chunk).unwrap();
        }
        f.flush().unwrap();
        drop(f);

        let vault_root = fixture.path.join("vault");
        let orch = ProjectOrchestrator::new(vault_root).unwrap();

        let err = orch.enforce_connector_contract(&project_dir).unwrap_err();
        assert!(err.contains("CONNECTOR_CONTRACT_VIOLATION"));
    }

    #[test]
    fn test_doctor_includes_thin_connector_contract_check() {
        let fixture = SafeFixture::new("doctor_connector_test");
        let project_dir = fixture.path.join("clean_proj");
        let ulvo_dir = project_dir.join(".ulvo");
        std::fs::create_dir_all(&ulvo_dir).unwrap();

        std::fs::write(
            ulvo_dir.join("manifest.toml"),
            "schema_version = 1\nname = \"clean_proj\"\n[runtimes]\npython = \">=3.8\"\n",
        ).unwrap();

        let vault_root = fixture.path.join("vault");
        let orch = ProjectOrchestrator::new(vault_root).unwrap();

        let report = orch.doctor(&project_dir).unwrap();
        assert!(report.checks.iter().any(|c| c.name == "Thin Connector Contract" && c.passed));
        assert!(report.connector.is_some());
        assert!(report.connector.unwrap().is_within_target_budget);
    }

    #[test]
    fn test_multi_ecosystem_native_manifest_fingerprinting() {
        let fixture = SafeFixture::new("multi_eco_fingerprint_test");
        let project_dir = fixture.path.join("poly_proj");
        let ulvo_dir = project_dir.join(".ulvo");
        std::fs::create_dir_all(&ulvo_dir).unwrap();

        std::fs::write(
            ulvo_dir.join("manifest.toml"),
            r#"schema_version = 1
name = "poly_proj"
[runtimes]
python = ">=3.10"
node = ">=20.0"

[python]
manager = "uv"
requirements = ["requirements.txt"]

[node]
manager = "pnpm"
package_json = "package.json"
lockfile = "pnpm-lock.yaml"
"#,
        ).unwrap();

        let pkg_json = project_dir.join("package.json");
        std::fs::write(&pkg_json, r#"{"name": "poly_proj", "dependencies": {"express": "4.18.2"}}"#).unwrap();
        let req_txt = project_dir.join("requirements.txt");
        std::fs::write(&req_txt, "fastapi==0.100.0\n").unwrap();

        let vault_root = fixture.path.join("vault");
        let orch = ProjectOrchestrator::new(vault_root).unwrap();
        let manifest = orch.load_manifest(&project_dir).unwrap();

        let (spec1, hash1) = orch.compute_specification(&project_dir, &manifest, "python.exe").unwrap();
        assert!(spec1.dependency_manifests.iter().any(|d| d.relative_path == "package.json"));
        assert!(spec1.dependency_manifests.iter().any(|d| d.relative_path == "requirements.txt"));

        // Mutate package.json (native manifest change)
        std::fs::write(&pkg_json, r#"{"name": "poly_proj", "dependencies": {"express": "4.19.0"}}"#).unwrap();
        let (_spec2, hash2) = orch.compute_specification(&project_dir, &manifest, "python.exe").unwrap();

        assert_ne!(hash1, hash2, "specification hash MUST invalidate when native package.json changes!");
    }

    #[test]
    fn test_vault_environments_audit_empty() {
        let fixture = SafeFixture::new("audit_empty_test");
        let vault_root = fixture.path.join("vault");
        let envs_dir = vault_root.join("environments").join("python");
        std::fs::create_dir_all(&envs_dir).unwrap();

        let orch = ProjectOrchestrator::new(vault_root).unwrap();
        let report = orch.audit_vault_environments().unwrap();
        assert_eq!(report.total_environments, 0);
        assert_eq!(report.active_environments, 0);
        assert_eq!(report.reclaimable_environments, 0);
        assert_eq!(report.aborted_environments, 0);
        assert_eq!(report.unknown_environments, 0);
        assert_eq!(report.total_logical_bytes, 0);
        assert_eq!(report.total_physical_bytes, 0);
    }

    #[test]
    fn test_vault_environments_audit_and_prune_classification() {
        let fixture = SafeFixture::new("audit_prune_test");
        let vault_root = fixture.path.join("vault");
        let envs_dir = vault_root.join("environments").join("python");
        std::fs::create_dir_all(&envs_dir).unwrap();

        let orch = ProjectOrchestrator::new(vault_root.clone()).unwrap();

        // 1. Inactive owned environment
        let env_inactive = envs_dir.join("env_proj_inactive");
        std::fs::create_dir_all(&env_inactive).unwrap();
        let own_inactive = EnvironmentOwnershipRecord::new(
            "env_proj_inactive".to_string(),
            fixture.path.join("proj1").to_string_lossy().to_string(),
            vault_root.to_string_lossy().to_string(),
            "sha256_mock_hash".to_string(),
            std::process::id(),
            1000,
        );
        std::fs::write(env_inactive.join(".ulvo_ownership.json"), serde_json::to_string(&own_inactive).unwrap()).unwrap();
        let lease_inactive = EnvironmentLease {
            lease_id: "lease_1".to_string(),
            environment_id: "env_proj_inactive".to_string(),
            project_path: fixture.path.join("proj1").to_string_lossy().to_string(),
            environment_path: env_inactive.to_string_lossy().to_string(),
            state: EnvironmentLifecycleState::Reclaimable,
            active_pids: vec![],
            leased_processes: vec![],
            created_at_unix: 1000,
            updated_at_unix: 1000,
            lease_timeout_seconds: 300,
            owner_token: "test".to_string(),
            python_version: None,
            specification_hash: None,
        };
        std::fs::write(env_inactive.join(".ulvo_lease.json"), serde_json::to_string(&lease_inactive).unwrap()).unwrap();
        std::fs::write(env_inactive.join("dummy_payload.bin"), vec![0u8; 1024]).unwrap();

        // 2. Active owned environment (leased by current process)
        let env_active = envs_dir.join("env_proj_active");
        std::fs::create_dir_all(&env_active).unwrap();
        let own_active = EnvironmentOwnershipRecord::new(
            "env_proj_active".to_string(),
            fixture.path.join("proj2").to_string_lossy().to_string(),
            vault_root.to_string_lossy().to_string(),
            "sha256_mock_hash".to_string(),
            std::process::id(),
            1000,
        );
        std::fs::write(env_active.join(".ulvo_ownership.json"), serde_json::to_string(&own_active).unwrap()).unwrap();
        let my_pid = std::process::id();
        let lease_active = EnvironmentLease {
            lease_id: "lease_2".to_string(),
            environment_id: "env_proj_active".to_string(),
            project_path: fixture.path.join("proj2").to_string_lossy().to_string(),
            environment_path: env_active.to_string_lossy().to_string(),
            state: EnvironmentLifecycleState::Executing,
            active_pids: vec![my_pid],
            leased_processes: vec![LeasedProcess {
                pid: my_pid,
                process_creation_time: ulvo_adapters::get_process_creation_time(my_pid),
                owner_token: format!("pid_{my_pid}"),
                registered_at_unix: 1000,
            }],
            created_at_unix: 1000,
            updated_at_unix: 1000,
            lease_timeout_seconds: 300,
            owner_token: format!("pid_{my_pid}"),
            python_version: None,
            specification_hash: None,
        };
        std::fs::write(env_active.join(".ulvo_lease.json"), serde_json::to_string(&lease_active).unwrap()).unwrap();

        // 3. Aborted materialization
        let env_aborted = envs_dir.join("env_aborted_mat");
        std::fs::create_dir_all(&env_aborted).unwrap();
        std::fs::write(env_aborted.join("pyvenv.cfg"), "home = C:\\fake\n").unwrap();

        // 4. Unknown foreign directory
        let foreign_dir = envs_dir.join("custom_manual_dir");
        std::fs::create_dir_all(&foreign_dir).unwrap();
        std::fs::write(foreign_dir.join("readme.txt"), "user data").unwrap();

        // Audit check
        let report = orch.audit_vault_environments().unwrap();
        assert_eq!(report.total_environments, 3);
        assert_eq!(report.active_environments, 1);
        assert_eq!(report.reclaimable_environments, 1);
        assert_eq!(report.aborted_environments, 1);
        assert_eq!(report.unknown_environments, 1);

        // Dry-run prune: must not modify any directories
        let dry_gc = orch.reclaim_all_inactive_environments(true).unwrap();
        assert_eq!(dry_gc.total_scanned, 3);
        assert_eq!(dry_gc.reclaimed_count, 2);
        assert_eq!(dry_gc.blocked_active_count, 1);
        assert_eq!(dry_gc.preserved_unknown_count, 1);
        assert!(env_inactive.exists(), "dry run must not delete inactive env");
        assert!(env_active.exists(), "dry run must not delete active env");
        assert!(env_aborted.exists(), "dry run must not delete aborted env");
        assert!(foreign_dir.exists(), "dry run must not delete unknown dir");

        // Real prune: deletes inactive and aborted, preserves active and foreign!
        let gc = orch.reclaim_all_inactive_environments(false).unwrap();
        assert_eq!(gc.total_scanned, 3);
        assert_eq!(gc.reclaimed_count, 2);
        assert_eq!(gc.blocked_active_count, 1);
        assert_eq!(gc.preserved_unknown_count, 1);
        assert!(!env_inactive.exists(), "inactive env must be reclaimed");
        assert!(!env_aborted.exists(), "aborted env must be reclaimed");
        assert!(env_active.exists(), "active env must be protected from reclamation");
        assert!(foreign_dir.exists(), "unknown dir must be preserved safely");
    }

    #[test]
    fn test_dead_process_lease_cleanup_allows_reclaim() {
        let fixture = SafeFixture::new("dead_process_test");
        let vault_root = fixture.path.join("vault");
        let envs_dir = vault_root.join("environments").join("python");
        std::fs::create_dir_all(&envs_dir).unwrap();

        let orch = ProjectOrchestrator::new(vault_root.clone()).unwrap();

        let env_dir = envs_dir.join("env_dead_proc");
        std::fs::create_dir_all(&env_dir).unwrap();
        let own = EnvironmentOwnershipRecord::new(
            "env_dead_proc".to_string(),
            fixture.path.join("proj_dead").to_string_lossy().to_string(),
            vault_root.to_string_lossy().to_string(),
            "sha256_mock_hash".to_string(),
            std::process::id(),
            1000,
        );
        std::fs::write(env_dir.join(".ulvo_ownership.json"), serde_json::to_string(&own).unwrap()).unwrap();

        let dead_pid = 99999999u32;
        let lease = EnvironmentLease {
            lease_id: "lease_dead".to_string(),
            environment_id: "env_dead_proc".to_string(),
            project_path: fixture.path.join("proj_dead").to_string_lossy().to_string(),
            environment_path: env_dir.to_string_lossy().to_string(),
            state: EnvironmentLifecycleState::Executing,
            active_pids: vec![dead_pid],
            leased_processes: vec![LeasedProcess {
                pid: dead_pid,
                process_creation_time: None,
                owner_token: "dead_token".to_string(),
                registered_at_unix: 1000,
            }],
            created_at_unix: 1000,
            updated_at_unix: 1000,
            lease_timeout_seconds: 300,
            owner_token: "dead_token".to_string(),
            python_version: None,
            specification_hash: None,
        };
        std::fs::write(env_dir.join(".ulvo_lease.json"), serde_json::to_string(&lease).unwrap()).unwrap();

        // Audit should recognize dead PID and treat state as Reclaimable
        let report = orch.audit_vault_environments().unwrap();
        assert_eq!(report.total_environments, 1);
        assert_eq!(report.active_environments, 0);
        assert_eq!(report.reclaimable_environments, 1);

        // Pruning should successfully reclaim it
        let gc = orch.reclaim_all_inactive_environments(false).unwrap();
        assert_eq!(gc.reclaimed_count, 1);
        assert!(!env_dir.exists());
    }

    #[test]
    fn test_active_lease_blocks_direct_reclaim() {
        let fixture = SafeFixture::new("active_lease_block_test");
        let vault_root = fixture.path.join("vault");
        let envs_dir = vault_root.join("environments").join("python");
        std::fs::create_dir_all(&envs_dir).unwrap();

        let orch = ProjectOrchestrator::new(vault_root.clone()).unwrap();

        let env_dir = envs_dir.join("env_active_block");
        std::fs::create_dir_all(&env_dir).unwrap();
        let own = EnvironmentOwnershipRecord::new(
            "env_active_block".to_string(),
            fixture.path.join("proj_block").to_string_lossy().to_string(),
            vault_root.to_string_lossy().to_string(),
            "sha256_mock_hash".to_string(),
            std::process::id(),
            1000,
        );
        std::fs::write(env_dir.join(".ulvo_ownership.json"), serde_json::to_string(&own).unwrap()).unwrap();

        let my_pid = std::process::id();
        let lease = EnvironmentLease {
            lease_id: "lease_active".to_string(),
            environment_id: "env_active_block".to_string(),
            project_path: fixture.path.join("proj_block").to_string_lossy().to_string(),
            environment_path: env_dir.to_string_lossy().to_string(),
            state: EnvironmentLifecycleState::Executing,
            active_pids: vec![my_pid],
            leased_processes: vec![LeasedProcess {
                pid: my_pid,
                process_creation_time: ulvo_adapters::get_process_creation_time(my_pid),
                owner_token: "active_token".to_string(),
                registered_at_unix: 1000,
            }],
            created_at_unix: 1000,
            updated_at_unix: 1000,
            lease_timeout_seconds: 300,
            owner_token: "active_token".to_string(),
            python_version: None,
            specification_hash: None,
        };
        std::fs::write(env_dir.join(".ulvo_lease.json"), serde_json::to_string(&lease).unwrap()).unwrap();

        // Calling reclaim_internal should fail closed because PID is active
        let res = orch.reclaim_internal(&env_dir);
        assert!(res.is_err(), "active lease must block reclamation");
        assert!(res.unwrap_err().contains("active process"));
        assert!(env_dir.exists(), "environment directory must not be removed");
    }
}

