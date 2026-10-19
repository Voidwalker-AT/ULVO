use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use ulvo_core::lifecycle::{RunExecutionResult, VaultEnvironmentsAuditReport, VaultEnvironmentsGcReport};
use ulvo_discovery::project_lifecycle::ProjectOrchestrator;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub project_name: String,
    pub project_path: PathBuf,
    pub has_connector: bool,
    pub connector_budget_ok: bool,
    pub active_pids: Vec<u32>,
    pub allocated_bytes: u64,
}

pub struct ProjectService {
    vault_root: PathBuf,
}

impl ProjectService {
    pub fn new<P: AsRef<Path>>(vault_root: P) -> Self {
        Self {
            vault_root: vault_root.as_ref().to_path_buf(),
        }
    }

    /// Initializes a compliant thin connector (.ulvo/manifest.toml) inside the specified project root.
    pub fn initialize_project_connector<P: AsRef<Path>>(&self, project_path: P) -> Result<PathBuf, String> {
        let root = project_path.as_ref();
        if !root.is_dir() {
            return Err(format!("Project root directory not found: {}", root.display()));
        }

        let ulvo_dir = root.join(".ulvo");
        let manifest_path = ulvo_dir.join("manifest.toml");

        if manifest_path.is_file() {
            return Ok(manifest_path);
        }

        std::fs::create_dir_all(&ulvo_dir)
            .map_err(|e| format!("cannot create .ulvo directory: {e}"))?;

        let project_name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();

        let default_manifest = format!(
r#"[project]
name = "{project_name}"
version = "0.1.0"
description = "ULVO Managed Project"

[runtime]
ecosystem = "python"
version = "3.12"

[execution]
lifecycle = "ephemeral"
gpu = false

[dependencies]
# List package requirements or reference requirements.txt
manifest = "requirements.txt"
"#
        );

        std::fs::write(&manifest_path, default_manifest)
            .map_err(|e| format!("cannot write manifest.toml: {e}"))?;

        Ok(manifest_path)
    }

    /// Inspects all materialized environments under the vault.
    pub fn audit_vault_environments(&self) -> Result<VaultEnvironmentsAuditReport, String> {
        let orchestrator = ProjectOrchestrator::new(self.vault_root.clone())?;
        orchestrator.audit_vault_environments()
    }

    /// Executes a command in the project environment with try-finally reclamation and structured telemetry.
    pub fn execute_project_command<P: AsRef<Path>>(
        &self,
        project_path: P,
        command: &[String],
        keep_env: bool,
    ) -> Result<RunExecutionResult, String> {
        let orchestrator = ProjectOrchestrator::new(self.vault_root.clone())?;
        orchestrator.run_structured(project_path.as_ref(), command, keep_env)
    }

    /// Safely reclaims all inactive and aborted environments.
    pub fn prune_inactive_environments(&self, dry_run: bool) -> Result<VaultEnvironmentsGcReport, String> {
        let orchestrator = ProjectOrchestrator::new(self.vault_root.clone())?;
        orchestrator.reclaim_all_inactive_environments(dry_run)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_connector_init() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_proj_init");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::create_dir_all(&temp_dir);

        let svc = ProjectService::new(&temp_dir);
        let manifest = svc.initialize_project_connector(&temp_dir).unwrap();
        assert!(manifest.is_file());
    }
}
