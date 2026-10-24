use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use ulvo_adapters::{bounded_read, reject_links, sha256_hex, write_atomic, EnvironmentFileLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairPlan {
    pub plan_id: String,
    pub vault: PathBuf,
    pub backup: Option<PathBuf>,
    pub backup_sha256: Option<String>,
    pub target_sha256: Option<String>,
    pub finding: String,
    pub automatic_apply_supported: bool,
}
pub struct RepairService { config: PathBuf, vault: PathBuf }
impl RepairService {
    pub fn new(config: PathBuf, vault: PathBuf) -> Self { Self { config, vault } }
    fn target(&self) -> PathBuf { self.vault.join("registry/projects.json") }
    fn snapshot(path: &Path) -> Result<Option<String>, String> { if path.exists() { Ok(Some(bounded_read(path)?)) } else { Ok(None) } }
    fn validate(content: &str) -> Result<(), String> {
        let parsed: ulvo_core::ProjectRegistryFile = serde_json::from_str(content).map_err(|e| e.to_string())?;
        if parsed.schema_version != 1 { return Err("unsupported registry schema".into()); } Ok(())
    }
    fn identity(plan: &RepairPlan) -> String {
        sha256_hex(format!("{:?}|{:?}|{:?}|{:?}", plan.vault, plan.backup, plan.backup_sha256, plan.target_sha256).as_bytes())[..24].into()
    }
    pub fn plan(&self, backup: Option<PathBuf>) -> Result<RepairPlan, String> {
        let current = Self::snapshot(&self.target())?;
        let broken = current.as_deref().map(Self::validate).transpose().is_err();
        let mut plan = RepairPlan { plan_id: String::new(), vault: self.vault.clone(), backup, backup_sha256: None, target_sha256: current.as_deref().map(|s| sha256_hex(s.as_bytes())), finding: if broken { "Invalid project registry; bytes preserved" } else { "No automatic project-registry repair required" }.into(), automatic_apply_supported: false };
        if let Some(path) = &plan.backup {
            if !path.is_absolute() { return Err("backup must be an absolute file path".into()); }
            reject_links(path)?;
            let bytes = bounded_read(path)?; Self::validate(&bytes)?;
            plan.backup_sha256 = Some(sha256_hex(bytes.as_bytes()));
            plan.automatic_apply_supported = broken;
        }
        plan.plan_id = Self::identity(&plan);
        let file = self.config.join("repair").join(format!("{}.json", plan.plan_id));
        reject_links(&file)?;
        write_atomic(&file, &serde_json::to_vec_pretty(&plan).map_err(|e| e.to_string())?)?;
        Ok(plan)
    }
    pub fn apply(&self, approved: &str) -> Result<RepairPlan, String> {
        if approved.len() != 24 || !approved.bytes().all(|c| c.is_ascii_hexdigit()) { return Err("invalid repair plan ID".into()); }
        let path = self.config.join("repair").join(format!("{approved}.json"));
        let plan: RepairPlan = serde_json::from_str(&bounded_read(&path)?).map_err(|e| e.to_string())?;
        if plan.vault != self.vault || plan.plan_id != approved || Self::identity(&plan) != approved || !plan.automatic_apply_supported { return Err("repair plan requires fresh review".into()); }
        let _lock = EnvironmentFileLock::acquire(&self.vault.join("registry/projects.lock"), 1000)?;
        let current = Self::snapshot(&self.target())?;
        if current.as_deref().map(|s| sha256_hex(s.as_bytes())) != plan.target_sha256 { return Err("registry changed since repair review".into()); }
        let backup = bounded_read(plan.backup.as_deref().ok_or("backup missing")?)?;
        Self::validate(&backup)?;
        if Some(sha256_hex(backup.as_bytes())) != plan.backup_sha256 { return Err("backup changed since repair review".into()); }
        if let Some(bytes) = current { write_atomic(&self.config.join("repair").join(format!("before_{approved}.json")), bytes.as_bytes())?; }
        reject_links(&self.target())?;
        write_atomic(&self.target(), backup.as_bytes())?;
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repair_is_exact_reviewed_reversible_and_detects_stale_plans() {
        let root = std::env::temp_dir().join(format!("ulvo_repair_{}", std::process::id()));
        let vault = root.join("vault");
        let registry = ulvo_discovery::ProjectRegistry::load_or_create(&vault, None).unwrap(); registry.save().unwrap();
        let target = vault.join("registry/projects.json"); let backup = root.join("backup.json"); std::fs::copy(&target, &backup).unwrap();
        std::fs::write(&target, "{interrupted").unwrap();
        let repair = RepairService::new(root.join("config"), vault);
        let plan = repair.plan(Some(backup)).unwrap();
        assert!(plan.automatic_apply_supported);
        std::fs::write(&target, "{changed").unwrap(); assert!(repair.apply(&plan.plan_id).is_err());
        std::fs::write(&target, "{interrupted").unwrap(); repair.apply(&plan.plan_id).unwrap();
        RepairService::validate(&std::fs::read_to_string(&target).unwrap()).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("config/repair").join(format!("before_{}.json", plan.plan_id))).unwrap(), "{interrupted");
    }
}
