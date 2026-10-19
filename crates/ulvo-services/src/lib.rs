use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

pub mod catalog;
pub mod provisioning;
pub mod adoption;
pub mod projects;
pub mod storage;
pub mod agent_ipc;

pub use catalog::*;
pub use provisioning::*;
pub use adoption::*;
pub use projects::*;
pub use storage::*;
pub use agent_ipc::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub vault_path: PathBuf,
    pub safety_reserve_mb: u64,
    pub cache_budget_gb: Option<f64>,
    pub auto_update: bool,
    pub enable_gateway_path: bool,
    pub enable_background_agent: bool,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            vault_path: PathBuf::from("E:\\Workspace\\Active\\ULVO\\vault"),
            safety_reserve_mb: 500,
            cache_budget_gb: Some(20.0),
            auto_update: false,
            enable_gateway_path: false,
            enable_background_agent: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingStatus {
    pub is_first_run: bool,
    pub vault_path: PathBuf,
    pub total_managed_resources: usize,
    pub system_tools_discovered: usize,
    pub guided_recommendations: Vec<String>,
}

pub struct UlvoServices {
    vault_root: PathBuf,
    catalog: CatalogService,
    provisioning: ProvisioningEngine,
    adoption: AdoptionService,
    projects: ProjectService,
    storage: StorageService,
    agent: AgentIpcServer,
}

impl UlvoServices {
    pub fn new<P: AsRef<Path>>(vault_root: P) -> Self {
        let vr = vault_root.as_ref().to_path_buf();
        Self {
            catalog: CatalogService::new(&vr),
            provisioning: ProvisioningEngine::new(&vr),
            adoption: AdoptionService::new(&vr),
            projects: ProjectService::new(&vr),
            storage: StorageService::new(&vr),
            agent: AgentIpcServer::new(&vr),
            vault_root: vr,
        }
    }

    pub fn vault_root(&self) -> &Path {
        &self.vault_root
    }

    pub fn catalog(&self) -> &CatalogService {
        &self.catalog
    }

    pub fn provisioning(&self) -> &ProvisioningEngine {
        &self.provisioning
    }

    pub fn adoption(&self) -> &AdoptionService {
        &self.adoption
    }

    pub fn projects(&self) -> &ProjectService {
        &self.projects
    }

    pub fn storage(&self) -> &StorageService {
        &self.storage
    }

    pub fn agent(&self) -> &AgentIpcServer {
        &self.agent
    }

    /// Evaluates if the current vault represents a fresh first-run empty state.
    pub fn check_onboarding_status(&self) -> OnboardingStatus {
        let catalog = self.catalog.list_catalog();
        let installed_count = catalog
            .iter()
            .filter(|e| matches!(e.installed_status, InstalledStatus::Installed { .. }))
            .count();

        let adoption = self.adoption.discover_system_installations();
        let is_first_run = installed_count == 0;

        let mut recommendations = Vec::new();
        if is_first_run {
            recommendations.push("Welcome to ULVO! No managed runtimes installed yet.".into());
            recommendations.push("Recommended: Provision Python 3.12 or Rust 1.96 into the vault.".into());
            recommendations.push("Review discovered host tools in the Adoption tab.".into());
        }

        OnboardingStatus {
            is_first_run,
            vault_path: self.vault_root.clone(),
            total_managed_resources: installed_count,
            system_tools_discovered: adoption.total_discovered,
            guided_recommendations: recommendations,
        }
    }

    /// Loads user configuration preferences from vault/config/preferences.json or returns default.
    pub fn load_preferences(&self) -> UserPreferences {
        let pref_file = self.vault_root.join("config").join("preferences.json");
        if pref_file.is_file() {
            if let Ok(data) = std::fs::read_to_string(&pref_file) {
                if let Ok(prefs) = serde_json::from_str::<UserPreferences>(&data) {
                    return prefs;
                }
            }
        }
        let mut def = UserPreferences::default();
        def.vault_path = self.vault_root.clone();
        def
    }

    /// Saves user preferences to vault/config/preferences.json.
    pub fn save_preferences(&self, prefs: &UserPreferences) -> Result<(), String> {
        let pref_file = self.vault_root.join("config").join("preferences.json");
        if let Some(parent) = pref_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let data = serde_json::to_string_pretty(prefs)
            .map_err(|e| format!("cannot serialize preferences: {e}"))?;
        std::fs::write(&pref_file, data)
            .map_err(|e| format!("cannot write preferences file: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_services_initialization_and_onboarding() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_services_facade");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::create_dir_all(&temp_dir);

        let svc = UlvoServices::new(&temp_dir);
        let status = svc.check_onboarding_status();
        assert!(status.is_first_run);
        assert_eq!(status.total_managed_resources, 0);

        let mut prefs = svc.load_preferences();
        prefs.safety_reserve_mb = 1000;
        svc.save_preferences(&prefs).unwrap();

        let loaded = svc.load_preferences();
        assert_eq!(loaded.safety_reserve_mb, 1000);
    }
}
