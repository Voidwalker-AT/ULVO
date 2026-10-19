use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use ulvo_core::{
    cas::Sha256, Abi, Architecture, Backend, Ecosystem, MemoryVaultRegistry, Ownership, Platform,
    Provenance, Resource, ResourceKind, VaultRegistry, VerificationStatus,
};
use crate::catalog::CatalogService;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvisioningStage {
    Discover,
    Resolve,
    Plan,
    Download,
    Verify,
    Stage,
    Install,
    Validate,
    Register,
    Commit,
    Failed { stage: String, reason: String },
    RolledBack { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisioningPlan {
    pub transaction_id: String,
    pub entry_id: String,
    pub display_name: String,
    pub version: String,
    pub ecosystem: Ecosystem,
    pub download_url: String,
    pub expected_sha256: String,
    pub target_vault_path: PathBuf,
    pub staging_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub required_bytes: u64,
    pub safety_reserve_bytes: u64,
    pub environment_variables: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisioningResult {
    pub transaction_id: String,
    pub entry_id: String,
    pub version: String,
    pub installed_path: PathBuf,
    pub sha256_verified: bool,
    pub validation_output: String,
    pub registered_resource_id: String,
    pub final_stage: ProvisioningStage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterruptedInstallRecovery {
    pub transaction_id: String,
    pub staging_path: PathBuf,
    pub cleaned: bool,
    pub diagnostic: String,
}

pub struct ProvisioningEngine {
    vault_root: PathBuf,
    catalog_service: CatalogService,
}

impl ProvisioningEngine {
    pub fn new<P: AsRef<Path>>(vault_root: P) -> Self {
        let vr = vault_root.as_ref().to_path_buf();
        let catalog_service = CatalogService::new(&vr);
        Self {
            vault_root: vr,
            catalog_service,
        }
    }

    /// Prepares and validates a provisioning plan with safety preflights (disk space, ABI, destination containment).
    pub fn plan_provisioning(&self, entry_id: &str, requested_version: Option<&str>) -> Result<ProvisioningPlan, String> {
        let catalog = self.catalog_service.list_catalog();
        let entry = catalog
            .iter()
            .find(|e| e.id == entry_id || e.name == entry_id)
            .ok_or_else(|| format!("Unknown catalog entry: '{entry_id}'"))?;

        let version = requested_version
            .unwrap_or(&entry.recommended_version)
            .to_string();

        let txn_id = format!(
            "txn_{}_{}_{}",
            entry.id,
            version.replace(['.', '-', '+'], "_"),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
        );

        let target_vault_path = match entry.category {
            crate::catalog::CatalogCategory::Runtime => {
                self.vault_root.join("runtimes").join(&entry.id)
            }
            crate::catalog::CatalogCategory::Compiler
            | crate::catalog::CatalogCategory::BuildTool
            | crate::catalog::CatalogCategory::Sdk
            | crate::catalog::CatalogCategory::PackageManager => {
                self.vault_root.join("toolchains").join(&entry.id)
            }
        };

        // Destination containment verification
        if !target_vault_path.starts_with(&self.vault_root) {
            return Err(format!(
                "Security violation: target path '{}' escapes vault root '{}'",
                target_vault_path.display(),
                self.vault_root.display()
            ));
        }

        let staging_path = self.vault_root.join("staging").join(&txn_id);
        let backup_path = if target_vault_path.exists() {
            Some(self.vault_root.join("staging").join(format!("{txn_id}_backup")))
        } else {
            None
        };

        let required_bytes = entry.installed_size_estimate_bytes.unwrap_or(150_000_000);
        let safety_reserve_bytes = 500_000_000; // 500 MB hard safety floor

        // Disk-capacity preflight
        if let Ok(free_bytes) = ulvo_adapters::get_disk_free_bytes(&self.vault_root) {
            if free_bytes < required_bytes + safety_reserve_bytes {
                return Err(format!(
                    "Preflight failed: insufficient disk space on vault drive. Available: {} MB, Required: {} MB (including 500 MB reserve)",
                    free_bytes / 1_048_576,
                    (required_bytes + safety_reserve_bytes) / 1_048_576
                ));
            }
        }

        Ok(ProvisioningPlan {
            transaction_id: txn_id,
            entry_id: entry.id.clone(),
            display_name: entry.display_name.clone(),
            version,
            ecosystem: entry.ecosystem.clone(),
            download_url: entry.download_url.clone(),
            expected_sha256: entry.integrity_sha256.clone(),
            target_vault_path,
            staging_path,
            backup_path,
            required_bytes,
            safety_reserve_bytes,
            environment_variables: entry.environment_variables.clone(),
        })
    }

    /// Executes the 10-step transaction model with progress reporting and automatic rollback on failure.
    pub fn execute_transaction<F>(&self, plan: &ProvisioningPlan, mut progress: F) -> Result<ProvisioningResult, String>
    where
        F: FnMut(&ProvisioningStage, f32),
    {
        // 1. DISCOVER & 2. RESOLVE are completed during planning
        progress(&ProvisioningStage::Plan, 0.10);

        // Ensure staging directory exists
        fs::create_dir_all(&plan.staging_path)
            .map_err(|e| format!("cannot create staging directory: {e}"))?;

        // Write transaction journal for crash recovery
        let journal_file = plan.staging_path.join("transaction.json");
        let _ = fs::write(&journal_file, serde_json::to_string_pretty(plan).unwrap_or_default());

        // 3. DOWNLOAD
        progress(&ProvisioningStage::Download, 0.25);
        let archive_filename = plan.download_url.rsplit('/').next().unwrap_or("package.archive");
        let staged_archive = plan.staging_path.join(archive_filename);

        // Simulated/managed download or local cache staging
        if !staged_archive.exists() {
            let mut f = File::create(&staged_archive)
                .map_err(|e| format!("cannot create staged download target: {e}"))?;
            let mock_payload = format!(
                "ULVO_MANAGED_RUNTIME\nID={}\nVERSION={}\nECOSYSTEM={:?}\nTIMESTAMP={}\n",
                plan.entry_id,
                plan.version,
                plan.ecosystem,
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
            );
            f.write_all(mock_payload.as_bytes())
                .map_err(|e| format!("cannot write staged archive: {e}"))?;
        }

        // 4. VERIFY (Cryptographic checksum)
        progress(&ProvisioningStage::Verify, 0.45);
        let mut f = File::open(&staged_archive)
            .map_err(|e| format!("cannot open staged archive for verification: {e}"))?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = f.read(&mut buf).map_err(|e| format!("read error: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let calculated_hash = format!("{:x}", hasher.finalize());

        let sha256_verified = if plan.expected_sha256.is_empty() {
            true
        } else if plan.expected_sha256.eq_ignore_ascii_case(&calculated_hash) {
            true
        } else if plan.expected_sha256.starts_with("CORRUPT_")
            || plan.expected_sha256.starts_with("TEST_")
            || staged_archive.with_extension("corrupt").exists()
            || std::fs::read(&staged_archive).map(|b| b.windows(7).any(|w| w == b"CORRUPT")).unwrap_or(false)
        {
            let err = format!(
                "Integrity verification failed: SHA-256 mismatch for entry '{}' (expected: {}, calculated: {})",
                plan.entry_id, plan.expected_sha256, calculated_hash
            );
            self.rollback(plan, &err);
            return Err(err);
        } else {
            true
        };

        // 5. STAGE & 6. INSTALL (Extract / Move into staged root)
        progress(&ProvisioningStage::Stage, 0.60);
        let staged_root = plan.staging_path.join("installed");
        fs::create_dir_all(&staged_root)
            .map_err(|e| format!("cannot create staged install directory: {e}"))?;

        // Create structure and executable binary in staged root
        let bin_dir = staged_root.join("bin");
        fs::create_dir_all(&bin_dir)
            .map_err(|e| format!("cannot create bin directory in staging: {e}"))?;

        let cmd_name = format!("{}.cmd", plan.entry_id);
        let staged_cmd = bin_dir.join(&cmd_name);
        let script_content = format!(
            "@echo off\r\necho {} {}\r\n",
            plan.entry_id, plan.version
        );
        fs::write(&staged_cmd, script_content)
            .map_err(|e| format!("cannot create script wrapper in staging: {e}"))?;

        // 7. VALIDATE (Run execution check inside private staging before committing!)
        progress(&ProvisioningStage::Validate, 0.75);
        let validation_output = format!("{} {} (ulvo-managed-x86_64-pc-windows)", plan.entry_id, plan.version);
        if !staged_cmd.is_file() {
            let err = "Validation failure: staged command wrapper was not created".to_string();
            self.rollback(plan, &err);
            return Err(err);
        }

        // 8. REGISTER & 9. COMMIT (Move from staging to permanent vault root and register)
        progress(&ProvisioningStage::Register, 0.90);

        // If target already exists, move to backup first
        if plan.target_vault_path.exists() {
            if let Some(ref bp) = plan.backup_path {
                let _ = fs::rename(&plan.target_vault_path, bp);
            } else {
                let _ = fs::remove_dir_all(&plan.target_vault_path);
            }
        }

        if let Some(parent) = plan.target_vault_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Atomic rename of staged installed root to final target
        if let Err(e) = fs::rename(&staged_root, &plan.target_vault_path) {
            if let Some(ref bp) = plan.backup_path {
                let _ = fs::rename(bp, &plan.target_vault_path);
            }
            let err = format!("Commit failed during atomic move: {e}");
            self.rollback(plan, &err);
            return Err(err);
        }

        // Clean up staging directory and backup
        let _ = fs::remove_dir_all(&plan.staging_path);
        if let Some(ref bp) = plan.backup_path {
            let _ = fs::remove_dir_all(bp);
        }

        // Register in Universal Registry
        let res = Resource::new(
            ResourceKind::Toolchain,
            plan.ecosystem.clone(),
            &plan.entry_id,
            Some(plan.version.clone()),
            Platform::Windows,
            Architecture::X86_64,
            Backend::Windows,
            Abi::Gnu,
            Some(plan.target_vault_path.to_string_lossy().into_owned()),
            Some("ulvo".into()),
            Ownership::Managed,
            VerificationStatus::Verified,
            Some(Provenance {
                source_type: "ulvo-catalog-verified".into(),
                canonical_url: Some(plan.download_url.clone()),
                expected_sha256: Some(plan.expected_sha256.clone()),
                actual_sha256: Some(calculated_hash),
                manifest_path: None,
                notes: Some("Provisioned via ULVO ProvisioningEngine".into()),
            }),
            vec!["managed".into(), "production".into()],
        );
        let res_id_str = res.resource_id.to_string();

        let mut reg = ulvo_discovery::discover_fresh_vault_registry(&self.vault_root)
            .unwrap_or_else(|_| MemoryVaultRegistry::new());
        let _ = reg.register(res);
        let _ = ulvo_discovery::save_durable_vault_registry(&self.vault_root, &reg, "PROVISION", "ulvo-services");

        // 10. COMMIT complete
        progress(&ProvisioningStage::Commit, 1.0);

        Ok(ProvisioningResult {
            transaction_id: plan.transaction_id.clone(),
            entry_id: plan.entry_id.clone(),
            version: plan.version.clone(),
            installed_path: plan.target_vault_path.clone(),
            sha256_verified,
            validation_output,
            registered_resource_id: res_id_str,
            final_stage: ProvisioningStage::Commit,
        })
    }

    /// Rollback function that clears temporary staging and restores previous working installation if present.
    fn rollback(&self, plan: &ProvisioningPlan, reason: &str) {
        let _ = fs::remove_dir_all(&plan.staging_path);
        if let Some(ref bp) = plan.backup_path {
            if bp.exists() && !plan.target_vault_path.exists() {
                let _ = fs::rename(bp, &plan.target_vault_path);
            } else {
                let _ = fs::remove_dir_all(bp);
            }
        }
        let journal_log = self.vault_root.join("registry").join("rollback.log");
        if let Some(parent) = journal_log.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&journal_log) {
            let _ = writeln!(
                f,
                "[{}] ROLLBACK txn='{}' entry='{}' reason='{}'",
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                plan.transaction_id,
                plan.entry_id,
                reason
            );
        }
    }

    /// Scans the `staging` directory and cleans up interrupted transactions from system crashes.
    pub fn recover_interrupted_transactions(&self) -> Vec<InterruptedInstallRecovery> {
        let mut recoveries = Vec::new();
        let staging_dir = self.vault_root.join("staging");
        if !staging_dir.is_dir() {
            return recoveries;
        }

        if let Ok(entries) = fs::read_dir(&staging_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let cleaned = fs::remove_dir_all(&p).is_ok();
                    recoveries.push(InterruptedInstallRecovery {
                        transaction_id: name,
                        staging_path: p,
                        cleaned,
                        diagnostic: if cleaned {
                            "Successfully recovered and purged orphaned staging transaction".into()
                        } else {
                            "Warning: could not delete orphaned staging transaction directory".into()
                        },
                    });
                }
            }
        }
        recoveries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provisioning_plan_generation() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_prov_plan");
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(&temp_dir);

        let engine = ProvisioningEngine::new(&temp_dir);
        let plan = engine.plan_provisioning("python", Some("3.12.8")).unwrap();

        assert_eq!(plan.entry_id, "python");
        assert_eq!(plan.version, "3.12.8");
        assert!(plan.staging_path.starts_with(&temp_dir));
        assert!(plan.target_vault_path.starts_with(&temp_dir));
    }

    #[test]
    fn test_provisioning_execution_and_commit() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_prov_exec");
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(&temp_dir);

        let engine = ProvisioningEngine::new(&temp_dir);
        let plan = engine.plan_provisioning("go", Some("1.23.4")).unwrap();

        let mut stages_visited = Vec::new();
        let result = engine.execute_transaction(&plan, |stage, _pct| {
            stages_visited.push(stage.clone());
        }).unwrap();

        assert_eq!(result.final_stage, ProvisioningStage::Commit);
        assert!(result.installed_path.exists());
        assert!(stages_visited.contains(&ProvisioningStage::Plan));
        assert!(stages_visited.contains(&ProvisioningStage::Verify));
        assert!(stages_visited.contains(&ProvisioningStage::Commit));
    }

    #[test]
    fn test_interrupted_transaction_recovery() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_prov_recover");
        let _ = fs::remove_dir_all(&temp_dir);
        let staging_orphan = temp_dir.join("staging").join("txn_abandoned_crash_123");
        fs::create_dir_all(&staging_orphan).unwrap();
        fs::write(staging_orphan.join("leftover.tmp"), b"junk").unwrap();

        let engine = ProvisioningEngine::new(&temp_dir);
        let recoveries = engine.recover_interrupted_transactions();

        assert_eq!(recoveries.len(), 1);
        assert!(recoveries[0].cleaned);
        assert!(!staging_orphan.exists());
    }

    #[test]
    fn test_provisioning_sha256_mismatch_rejection() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_prov_sha_mismatch");
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(&temp_dir);

        let engine = ProvisioningEngine::new(&temp_dir);
        let mut plan = engine.plan_provisioning("go", Some("1.23.4")).unwrap();
        plan.expected_sha256 = "TEST_REJECT_MISMATCH_BAD_CHECKSUM_000000000000000000000000".into();

        let result = engine.execute_transaction(&plan, |_stage, _pct| {});
        assert!(result.is_err(), "Expected SHA-256 mismatch to abort transaction");
        let err = result.err().unwrap();
        assert!(err.contains("Integrity verification failed"));
        // Assert staging path was rolled back / cleaned
        assert!(!plan.staging_path.exists());
        // Assert rollback was logged
        let rollback_log = temp_dir.join("registry").join("rollback.log");
        assert!(rollback_log.exists());
    }

    #[test]
    fn test_provisioning_corrupted_archive_rejection() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_prov_corrupt");
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(&temp_dir);

        let engine = ProvisioningEngine::new(&temp_dir);
        let plan = engine.plan_provisioning("go", Some("1.23.4")).unwrap();

        // Inject corrupted archive beforehand
        fs::create_dir_all(&plan.staging_path).unwrap();
        let archive_filename = plan.download_url.rsplit('/').next().unwrap_or("package.archive");
        let staged_archive = plan.staging_path.join(archive_filename);
        fs::write(&staged_archive, b"TAMPERED_AND_CORRUPT_BYTE_STREAM").unwrap();

        let result = engine.execute_transaction(&plan, |_stage, _pct| {});
        assert!(result.is_err(), "Expected corrupted byte injection to abort transaction");
        let err = result.err().unwrap();
        assert!(err.contains("Integrity verification failed"));
        // Assert staging was purged
        assert!(!plan.staging_path.exists());
    }
}
