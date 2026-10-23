use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use ulvo_core::lifecycle::{
    PackageCacheGcReport, PackageCachePruneReport, PackageCacheStatusReport, VaultEnvironmentsAuditReport,
};
use ulvo_discovery::project_lifecycle::ProjectOrchestrator;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComprehensiveStorageReport {
    pub drive_letter: String,
    pub total_capacity_bytes: u64,
    pub available_free_bytes: u64,
    pub safety_reserve_bytes: u64,
    pub safety_floor_satisfied: bool,
    pub vault_physical_bytes: u64,
    pub package_cache_physical_bytes: u64,
    pub shared_store_physical_bytes: u64,
    pub ephemeral_env_physical_bytes: u64,
    pub cas_physical_bytes: u64,
    pub hardlink_sharing_ratio: f64,
    pub package_cache_status: PackageCacheStatusReport,
}

#[derive(Clone)]
pub struct StorageService {
    vault_root: PathBuf,
    cache: Arc<Mutex<Option<(Instant, ComprehensiveStorageReport)>>>,
}

impl StorageService {
    pub fn new<P: AsRef<Path>>(vault_root: P) -> Self {
        Self {
            vault_root: vault_root.as_ref().to_path_buf(),
            cache: Arc::new(Mutex::new(None)),
        }
    }

    /// Generates a comprehensive storage intelligence report across the drive, vault, and packages.
    pub fn get_storage_report(&self) -> Result<ComprehensiveStorageReport, String> {
        let vr = &self.vault_root;
        let drive = vr
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_else(|| "E:".into());

        let (free_bytes, total_capacity_bytes) = ulvo_adapters::get_disk_space_bytes(vr)?;
        let safety_reserve_bytes = 500_000_000; // 500 MB hard floor

        // If cached within the last 60 seconds, return cached report with freshly updated live free bytes
        if let Ok(mut guard) = self.cache.lock() {
            if let Some((cached_at, ref mut rep)) = *guard {
                if cached_at.elapsed() < Duration::from_secs(60) {
                    rep.available_free_bytes = free_bytes;
                    rep.total_capacity_bytes = total_capacity_bytes;
                    rep.safety_floor_satisfied = free_bytes > safety_reserve_bytes;
                    return Ok(rep.clone());
                }
            }
        }

        let (vault_bytes, _) = ulvo_adapters::calculate_physical_dir_size(vr);
        let (pkg_bytes, _) = ulvo_adapters::calculate_physical_dir_size(&vr.join("packages").join("python").join("uv-cache"));
        let (shared_bytes, _) = ulvo_adapters::calculate_physical_dir_size(&vr.join("packages").join("python").join("shared"));
        let (env_bytes, _) = ulvo_adapters::calculate_physical_dir_size(&vr.join("environments"));
        let (cas_bytes, _) = ulvo_adapters::calculate_physical_dir_size(&vr.join("packages").join("cas"));

        let orchestrator = ProjectOrchestrator::new(vr.clone())?;
        let env_audit = orchestrator.audit_vault_environments().unwrap_or_else(|_| VaultEnvironmentsAuditReport {
            schema_version: 1,
            vault_environments_dir: "".into(),
            total_environments: 0,
            active_environments: 0,
            reclaimable_environments: 0,
            aborted_environments: 0,
            unknown_environments: 0,
            total_logical_bytes: 0,
            total_physical_bytes: 0,
            items: Vec::new(),
        });

        let total_logical_env = env_audit.total_logical_bytes;
        let total_physical_env = env_audit.total_physical_bytes;

        let sharing_ratio = if total_physical_env > 0 {
            total_logical_env as f64 / total_physical_env as f64
        } else {
            1.0
        };

        let pkg_status = orchestrator.cache_status()?;

        let report = ComprehensiveStorageReport {
            drive_letter: drive,
            total_capacity_bytes,
            available_free_bytes: free_bytes,
            safety_reserve_bytes,
            safety_floor_satisfied: free_bytes > safety_reserve_bytes,
            vault_physical_bytes: vault_bytes,
            package_cache_physical_bytes: pkg_bytes,
            shared_store_physical_bytes: shared_bytes,
            ephemeral_env_physical_bytes: env_bytes,
            cas_physical_bytes: cas_bytes,
            hardlink_sharing_ratio: sharing_ratio,
            package_cache_status: pkg_status,
        };

        if let Ok(mut guard) = self.cache.lock() {
            *guard = Some((Instant::now(), report.clone()));
        }

        Ok(report)
    }

    /// Prunes unreferenced extracted package archives.
    pub fn prune_cache(&self, dry_run: bool) -> Result<PackageCachePruneReport, String> {
        let orchestrator = ProjectOrchestrator::new(self.vault_root.clone())?;
        let res = orchestrator.cache_prune(dry_run);
        if !dry_run {
            if let Ok(mut guard) = self.cache.lock() {
                *guard = None;
            }
        }
        res
    }

    /// Garbage-collects package cache down to a configurable storage quota in GB.
    pub fn gc_cache(&self, max_gb: Option<f64>, dry_run: bool) -> Result<PackageCacheGcReport, String> {
        let orchestrator = ProjectOrchestrator::new(self.vault_root.clone())?;
        let res = orchestrator.cache_gc(max_gb, dry_run);
        if !dry_run {
            if let Ok(mut guard) = self.cache.lock() {
                *guard = None;
            }
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_report_generation() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_storage_report");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::create_dir_all(&temp_dir);

        let svc = StorageService::new(&temp_dir);
        let rep = svc.get_storage_report();
        assert!(rep.is_ok());
    }
}
