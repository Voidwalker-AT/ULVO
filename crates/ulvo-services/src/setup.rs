use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use ulvo_adapters::{BootstrapPreferences, ResolvedPaths, EnvironmentFileLock, bounded_read, reject_links, sha256_hex, validate_absolute_directory, write_atomic};

pub const SETUP_STAGES: &[&str] = &["welcome", "privacy_permissions", "read_only_discovery", "machine_assessment", "vault_location", "ecosystem_selection", "migration_plan", "review", "apply", "verify", "complete"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupRequest {
    pub vault_path: PathBuf,
    #[serde(default)] pub workspace_roots: Vec<PathBuf>,
    #[serde(default)] pub ecosystems: Vec<String>,
    #[serde(default)] pub discovery_consent: bool,
    #[serde(default)] pub enable_background_agent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupPlan {
    pub schema_version: u32,
    pub plan_id: String,
    pub request: SetupRequest,
    pub available_bytes: u64,
    pub metadata_estimate_bytes: u64,
    pub runtime_download_bytes: Option<u64>,
    pub downloads_authorized: bool,
    pub external_tools: Vec<ulvo_discovery::DiscoveredToolchain>,
    pub discovery_partial: bool,
    pub inaccessible_paths: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupState {
    pub schema_version: u32,
    pub stage: String,
    pub configured: bool,
    pub paths: ResolvedPaths,
    pub plan: Option<SetupPlan>,
    pub previous_preferences: Option<String>,
    pub committed_preferences_sha256: Option<String>,
}

pub struct SetupService { paths: ResolvedPaths }
impl SetupService {
    pub fn new(paths: ResolvedPaths) -> Self { Self { paths } }
    fn journal(&self) -> PathBuf { self.paths.config_root.join("setup.json") }
    pub fn status(&self) -> Result<SetupState, String> {
        if self.journal().exists() {
            let mut state: SetupState = serde_json::from_str(&bounded_read(&self.journal())?).map_err(|e| format!("setup journal requires repair: {e}"))?;
            if state.schema_version != 1 { return Err("unsupported setup journal schema".into()); }
            state.paths = self.paths.clone();
            return Ok(state);
        }
        let legacy = self.paths.vault_root.join("registry/vault_registry.json").is_file();
        Ok(SetupState { schema_version: 1, stage: if legacy { "complete" } else { "welcome" }.into(), configured: legacy, paths: self.paths.clone(), plan: None, previous_preferences: None, committed_preferences_sha256: None })
    }
    fn save(&self, state: &SetupState) -> Result<(), String> {
        reject_links(&self.journal())?;
        let bytes = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
        if bytes.len() > 512 * 1024 { return Err("setup journal exceeds the 512 KiB metadata budget".into()); }
        write_atomic(&self.journal(), &bytes)
    }
    pub fn plan(&self, request: SetupRequest) -> Result<SetupPlan, String> {
        validate_absolute_directory(&request.vault_path)?;
        if request.workspace_roots.len() > 100 || request.ecosystems.len() > 8 { return Err("setup selection exceeds metadata limits".into()); }
        for root in &request.workspace_roots { validate_absolute_directory(root)?; }
        let supported = ["python", "rust", "node", "go", "ruby", "java", "cpp", "dotnet"];
        if request.ecosystems.iter().any(|e| !supported.contains(&e.as_str())) { return Err("unsupported ecosystem selection".into()); }
        let ancestor = request.vault_path.ancestors().find(|p| p.is_dir()).ok_or("no existing volume for vault")?;
        let (free, _) = ulvo_adapters::get_disk_space_bytes(ancestor)?;
        let report = request.discovery_consent.then(|| ulvo_discovery::UniversalToolchainScanner::new(request.vault_path.clone()).scan_known_locations_report());
        let mut external_tools = report.as_ref().map(|r| r.tools.clone()).unwrap_or_default();
        let truncated = external_tools.len() > 128;
        external_tools.truncate(128);
        let mut warnings = vec!["Metadata setup only: selected ecosystems are preferences; provision runtimes separately with an explicit reviewed provisioning plan.".into(), "No project is onboarded, migrated, renamed, deleted, or adopted by this plan.".into(), "Discovery is bounded metadata-only; executable versions and absent tools are not inferred.".into()];
        if !request.discovery_consent { warnings.push("Discovery was not authorized and was not performed.".into()); }
        if free < 500_000_000 + 1_048_576 { warnings.push("Vault volume is below the safety reserve; apply will be blocked.".into()); }
        let identity = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        Ok(SetupPlan { schema_version: 1, plan_id: sha256_hex(&identity)[..24].into(), request, available_bytes: free, metadata_estimate_bytes: 1_048_576, runtime_download_bytes: None, downloads_authorized: false, external_tools, discovery_partial: truncated || report.as_ref().map(|r| r.partial).unwrap_or(false), inaccessible_paths: report.map(|r| r.inaccessible_paths).unwrap_or_default(), warnings })
    }
    pub fn stage_plan(&self, request: SetupRequest) -> Result<SetupState, String> {
        let _lock = EnvironmentFileLock::acquire(&self.paths.config_root.join("setup.lock"), 1000)?;
        let plan = self.plan(request)?;
        let mut state = self.status()?;
        if matches!(state.stage.as_str(), "applying" | "verify") { return Err("resume or cancel the interrupted plan before replacing it".into()); }
        state.stage = "review".into(); state.plan = Some(plan); self.save(&state)?; Ok(state)
    }
    pub fn apply(&self, approved_id: &str) -> Result<SetupState, String> {
        let _lock = EnvironmentFileLock::acquire(&self.paths.config_root.join("setup.lock"), 1000)?;
        let mut state = self.status()?;
        let plan = state.plan.clone().ok_or("create a setup plan first")?;
        if plan.plan_id != approved_id { return Err("approved plan ID does not match pending setup".into()); }
        if !matches!(state.stage.as_str(), "review" | "applying" | "verify") { return Err("setup is not ready to apply".into()); }
        let refreshed = self.plan(plan.request.clone())?;
        if refreshed.plan_id != plan.plan_id { return Err("setup plan content changed; review a new plan".into()); }
        if refreshed.available_bytes < 500_000_000 + plan.metadata_estimate_bytes { return Err("insufficient free storage for setup safety reserve".into()); }
        let vault = &plan.request.vault_path;
        let marker = vault.join(".ulvo-vault.json");
        if vault.is_dir() && !marker.exists() && !vault.join("registry/vault_registry.json").is_file() {
            if std::fs::read_dir(vault).map_err(|e| e.to_string())?.next().is_some() { return Err("refusing to claim a nonempty unowned vault directory".into()); }
        }
        let prefs_file = self.paths.config_root.join("preferences.json");
        reject_links(&prefs_file)?;
        if state.stage == "review" {
            state.previous_preferences = if prefs_file.is_file() { Some(bounded_read(&prefs_file)?) } else { None };
            state.stage = "applying".into(); self.save(&state)?;
        }
        std::fs::create_dir_all(vault).map_err(|e| e.to_string())?;
        reject_links(vault)?;
        if !marker.exists() { write_atomic(&marker, &serde_json::to_vec(&serde_json::json!({"schema_version":1,"kind":"ulvo_vault","setup_plan_id":plan.plan_id})).unwrap())?; }
        let prefs = BootstrapPreferences { vault_path: Some(vault.clone()), workspace_roots: plan.request.workspace_roots.clone(), ecosystems: plan.request.ecosystems.clone(), enable_background_agent: plan.request.enable_background_agent };
        let bytes = serde_json::to_vec_pretty(&prefs).map_err(|e| e.to_string())?;
        let current = if prefs_file.is_file() { Some(bounded_read(&prefs_file)?) } else { None };
        let already_published = state.committed_preferences_sha256.is_some() && current.as_deref().map(|s| sha256_hex(s.as_bytes())) == state.committed_preferences_sha256;
        if current != state.previous_preferences && !already_published {
            return Err("preferences changed concurrently; setup publication refused".into());
        }
        // Journal the exact intended value before publication for crash recovery.
        state.committed_preferences_sha256 = Some(sha256_hex(&bytes)); self.save(&state)?;
        write_atomic(&prefs_file, &bytes)?;
        if !vault.join("registry/projects.json").exists() {
            let mut registry = ulvo_discovery::ProjectRegistry::load_or_create(vault, None)?;
            registry.set_workspace_roots(&plan.request.workspace_roots)?;
            registry.save()?;
        }
        state.stage = "verify".into(); self.save(&state)?;
        self.verify_locked(state)
    }
    fn verify_locked(&self, mut state: SetupState) -> Result<SetupState, String> {
        let plan = state.plan.as_ref().ok_or("no setup plan")?;
        let prefs = bounded_read(&self.paths.config_root.join("preferences.json"))?;
        if Some(sha256_hex(prefs.as_bytes())) != state.committed_preferences_sha256 { return Err("preferences changed after setup publication; manual review required".into()); }
        let selected = &plan.request.vault_path;
        validate_absolute_directory(selected)?;
        if !selected.join(".ulvo-vault.json").is_file() { return Err("vault marker missing".into()); }
        state.stage = "complete".into(); state.configured = true; self.save(&state)?; Ok(state)
    }
    pub fn cancel(&self) -> Result<SetupState, String> {
        let _lock = EnvironmentFileLock::acquire(&self.paths.config_root.join("setup.lock"), 1000)?;
        let mut state = self.status()?;
        if state.stage == "complete" { return Err("completed setup cannot be cancelled; create a reviewed configuration change".into()); }
        if let Some(expected) = &state.committed_preferences_sha256 {
            let file = self.paths.config_root.join("preferences.json");
            let current = bounded_read(&file)?;
            if &sha256_hex(current.as_bytes()) != expected { return Err("preferences changed concurrently; rollback requires review".into()); }
            // Absence is restored as an empty preference object, avoiding deletion.
            write_atomic(&file, state.previous_preferences.as_deref().unwrap_or("{}").as_bytes())?;
        }
        state.stage = "cancelled".into(); state.configured = false; self.save(&state)?; Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn service(tag: &str) -> SetupService {
        let root = std::env::temp_dir().join(format!("ulvo_setup_{tag}_{}", std::process::id()));
        SetupService::new(ResolvedPaths { config_root: root.join("config"), vault_root: root.join("vault Δ space"), source: "test".into(), workspace_roots: vec![] })
    }
    fn request(s: &SetupService) -> SetupRequest { SetupRequest { vault_path: s.paths.vault_root.clone(), workspace_roots: vec![], ecosystems: vec![], discovery_consent: false, enable_background_agent: false } }
    #[test]
    fn no_tools_or_network_needed_for_setup_and_id_is_required() {
        let svc = service("apply");
        assert!(!svc.status().unwrap().configured);
        let state = svc.stage_plan(request(&svc)).unwrap();
        assert!(state.plan.as_ref().unwrap().external_tools.is_empty());
        assert!(!svc.paths.vault_root.exists());
        assert!(svc.apply("unapproved").is_err());
        let done = svc.apply(&state.plan.unwrap().plan_id).unwrap();
        assert!(done.configured);
        assert_eq!(done.stage, "complete");
    }
    #[test]
    fn interrupted_publication_resumes_but_concurrent_preferences_are_retained() {
        let svc = service("resume");
        let mut state = svc.stage_plan(request(&svc)).unwrap();
        let id = state.plan.as_ref().unwrap().plan_id.clone();
        state.stage = "applying".into();
        svc.save(&state).unwrap();
        assert_eq!(svc.apply(&id).unwrap().stage, "complete");

        let other = service("concurrent");
        let mut state = other.stage_plan(request(&other)).unwrap();
        let id = state.plan.as_ref().unwrap().plan_id.clone();
        state.stage = "applying".into();
        other.save(&state).unwrap();
        let prefs = other.paths.config_root.join("preferences.json");
        std::fs::write(&prefs, b"{\"enable_background_agent\":true}").unwrap();
        assert!(other.apply(&id).unwrap_err().contains("concurrently"));
        assert_eq!(std::fs::read_to_string(prefs).unwrap(), "{\"enable_background_agent\":true}");
    }
    #[test]
    fn altered_plan_content_requires_new_review() {
        let svc = service("tamper");
        let mut state = svc.stage_plan(request(&svc)).unwrap();
        let id = state.plan.as_ref().unwrap().plan_id.clone();
        state.plan.as_mut().unwrap().request.enable_background_agent = true;
        svc.save(&state).unwrap();
        assert!(svc.apply(&id).unwrap_err().contains("content changed"));
        assert!(!svc.paths.vault_root.exists());
    }
    #[test]
    fn unknown_files_are_never_claimed_and_cancel_preserves_them() {
        let svc = service("unowned");
        std::fs::create_dir_all(&svc.paths.vault_root).unwrap();
        let file = svc.paths.vault_root.join("user.txt"); std::fs::write(&file, b"retain").unwrap();
        let plan = svc.stage_plan(request(&svc)).unwrap().plan.unwrap();
        assert!(svc.apply(&plan.plan_id).is_err());
        assert_eq!(std::fs::read(file).unwrap(), b"retain");
        assert_eq!(svc.cancel().unwrap().stage, "cancelled");
    }
}
