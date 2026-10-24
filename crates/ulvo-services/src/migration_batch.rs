use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use ulvo_adapters::{bounded_read, reject_links, sha256_hex, write_atomic, EnvironmentFileLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCandidate {
    pub project: PathBuf,
    pub manifest_sha256: String,
    pub manifest_name: String,
    pub python_version: Option<String>,
    pub connector_existed: bool,
    pub eligible_for_connector: bool,
    pub removal_authorized: bool,
    pub disposition: String,
    pub rollback: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationBatchPlan { pub plan_id: String, pub candidates: Vec<BatchCandidate>, pub applied: Vec<PathBuf> }
pub struct MigrationBatchService { config: PathBuf, vault: PathBuf }
impl MigrationBatchService {
    pub fn new(config: PathBuf, vault: PathBuf) -> Self { Self { config, vault } }
    fn inspect(project: &Path) -> Result<BatchCandidate, String> {
        reject_links(project)?;
        let project = project.canonicalize().map_err(|e| e.to_string())?;
        let manifest = ["requirements.lock", "requirements.txt", "uv.lock", "pyproject.toml"].iter().find(|m| project.join(m).is_file()).ok_or("native dependency manifest is missing")?;
        let bytes = bounded_read(&project.join(manifest))?;
        let legacy = [".venv", "venv"].iter().find_map(|name| project.join(name).is_dir().then(|| project.join(name)));
        let mut blocked = "Unknown consumers, actual replacement execution, ABI and rollback remain unverified. Removal is blocked.".to_string();
        let env_path = legacy.clone().unwrap_or_else(|| project.join(".venv"));
        let python_version = ulvo_adapters::detect_python_version(&project, &env_path);
        let mut eligible = !bytes.trim().is_empty() && ["requirements.lock", "requirements.txt"].contains(manifest) && python_version.is_some();
        if bytes.to_lowercase().contains("torch") || bytes.to_lowercase().contains("tensorflow") || bytes.to_lowercase().contains("cupy") { eligible = false; blocked = "ML runtime/ABI/GPU requirements need project-specific review; no batch connector generated".into(); }
        if let Some(env) = &legacy {
            reject_links(env)?;
            if !env.join("pyvenv.cfg").is_file() { eligible = false; blocked = "Legacy environment metadata is incomplete".into(); }
            let site = env.join("Lib/site-packages");
            if let Ok(entries) = std::fs::read_dir(site) {
                for entry in entries.take(20000) {
                    let p = entry.map_err(|e| e.to_string())?.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("pth") {
                        eligible = false; blocked = "Editable/path-hook dependencies require manual review before migration".into(); break;
                    }
                }
            } else { eligible = false; blocked = "Installed dependency metadata is inaccessible or incomplete".into(); }
        }
        let connector_existed = project.join(".ulvo/manifest.toml").is_file();
        Ok(BatchCandidate { project, manifest_sha256: sha256_hex(bytes.as_bytes()), manifest_name: (*manifest).into(), python_version, connector_existed, eligible_for_connector: eligible, removal_authorized: false, disposition: blocked, rollback: "Legacy environment is retained. Original connector, if present, is never overwritten. New connector content and hash are recorded in the batch receipt; removal requires separate exact-resource approval.".into() })
    }
    fn identity(candidates: &[BatchCandidate]) -> String { sha256_hex(&serde_json::to_vec(candidates).unwrap())[..24].into() }
    fn path(&self, id: &str) -> PathBuf { self.config.join("migration").join(format!("{id}.json")) }
    pub fn review(&self, projects: Vec<PathBuf>) -> Result<MigrationBatchPlan, String> {
        if projects.is_empty() || projects.len() > 100 { return Err("review requires 1..100 explicit project candidates".into()); }
        let mut candidates = projects.iter().map(|p| Self::inspect(p)).collect::<Result<Vec<_>, _>>()?;
        candidates.sort_by(|a,b| a.project.cmp(&b.project)); candidates.dedup_by(|a,b| a.project == b.project);
        let plan = MigrationBatchPlan { plan_id: Self::identity(&candidates), candidates, applied: vec![] };
        write_atomic(&self.path(&plan.plan_id), &serde_json::to_vec_pretty(&plan).unwrap())?; Ok(plan)
    }
    /// Connector-only batch: no downloads, environment mutation or retirement.
    pub fn apply(&self, approved: &str) -> Result<MigrationBatchPlan, String> { self.apply_selected(approved, &[]) }
    pub fn apply_selected(&self, approved: &str, selected: &[PathBuf]) -> Result<MigrationBatchPlan, String> {
        if approved.len() != 24 || !approved.bytes().all(|c| c.is_ascii_hexdigit()) { return Err("invalid approved migration plan ID".into()); }
        let _lock = EnvironmentFileLock::acquire(&self.config.join("migration/batch.lock"), 1000)?;
        let mut plan: MigrationBatchPlan = serde_json::from_str(&bounded_read(&self.path(approved))?).map_err(|e| e.to_string())?;
        if plan.plan_id != approved || Self::identity(&plan.candidates) != approved { return Err("migration plan changed; review again".into()); }
        let selected = selected.iter().map(|p| p.canonicalize().map_err(|e| e.to_string())).collect::<Result<Vec<_>, _>>()?;
        if selected.iter().any(|p| !plan.candidates.iter().any(|c| &c.project == p && c.eligible_for_connector)) { return Err("selection contains an unreviewed or blocked candidate".into()); }
        for candidate in &plan.candidates {
            if !candidate.eligible_for_connector || (!selected.is_empty() && !selected.contains(&candidate.project)) { continue; }
            if plan.applied.contains(&candidate.project) { continue; }
            let current = Self::inspect(&candidate.project)?;
            if !candidate.eligible_for_connector || !current.eligible_for_connector || current.manifest_sha256 != candidate.manifest_sha256 || current.python_version != candidate.python_version || current.connector_existed != candidate.connector_existed { return Err("candidate changed or is blocked; review a new batch".into()); }
        }
        for candidate in plan.candidates.clone() {
            if !candidate.eligible_for_connector || (!selected.is_empty() && !selected.contains(&candidate.project)) { continue; }
            if plan.applied.contains(&candidate.project) { continue; }
            let connector = candidate.project.join(".ulvo/manifest.toml");
            reject_links(&connector)?;
            if !candidate.connector_existed {
                let value = serde_json::json!({"schema_version":1,"project":{"name":candidate.project.file_name().unwrap_or_default().to_string_lossy(),"ecosystem":"python"},"runtimes":{"python":candidate.python_version},"package_manager":{"name":"uv","requirements":[candidate.manifest_name]},"lifecycle":{"policy":"persistent","allow_reclaim_on_quiescence":false}});
                let text = toml::to_string(&value).map_err(|e| e.to_string())?;
                ulvo_discovery::parse_manifest(&text)?;
                write_atomic(&connector, text.as_bytes())?;
            }
            let receipt = serde_json::json!({"project":candidate.project,"vault_reference":self.vault,"connector_sha256":sha256_hex(bounded_read(&connector)?.as_bytes()),"legacy_environment_retained":true,"removal_authorized":false});
            let receipt_name = sha256_hex(candidate.project.to_string_lossy().as_bytes());
            write_atomic(&self.config.join("migration").join(format!("{approved}_{receipt_name}.receipt.json")), &serde_json::to_vec_pretty(&receipt).unwrap())?;
            plan.applied.push(candidate.project);
            write_atomic(&self.path(approved), &serde_json::to_vec_pretty(&plan).unwrap())?;
        }
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reviewed_batch_creates_only_connectors_and_preserves_legacy_payloads() {
        let root = std::env::temp_dir().join(format!("ulvo_batch_{}", std::process::id()));
        let project = root.join("project");
        std::fs::create_dir_all(project.join(".venv/Lib/site-packages")).unwrap();
        std::fs::write(project.join(".venv/pyvenv.cfg"), "version = 3.11.16\n").unwrap();
        std::fs::write(project.join("requirements.txt"), "flask==3.0.0\n").unwrap();
        let sentinel = project.join(".venv/preserve.txt"); std::fs::write(&sentinel, b"legacy").unwrap();
        let service = MigrationBatchService::new(root.join("config"), root.join("vault"));
        let plan = service.review(vec![project.clone()]).unwrap();
        assert!(plan.candidates[0].eligible_for_connector);
        assert!(!plan.candidates[0].removal_authorized);
        let done = service.apply(&plan.plan_id).unwrap();
        assert_eq!(done.applied.len(), 1);
        assert_eq!(std::fs::read(sentinel).unwrap(), b"legacy");
        assert!(!project.join("requirements.lock").exists());
        assert!(project.join(".ulvo/manifest.toml").metadata().unwrap().len() < 100 * 1024);
    }
}
