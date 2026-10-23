use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;
use ulvo_core::lifecycle::{
    PackageCacheGcReport, PackageCachePruneReport, VaultEnvironmentsAuditReport,
    VaultEnvironmentsGcReport,
};
use ulvo_services::*;

pub struct DesktopState {
    pub services: Mutex<UlvoServices>,
    pub watcher: Mutex<Option<WatcherSupervisor>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentStatusSummary {
    pub is_running: bool,
    pub pid: Option<u64>,
    pub port: u16,
    pub message: String,
}

#[tauri::command]
fn get_onboarding_status(state: State<'_, DesktopState>) -> Result<OnboardingStatus, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    Ok(svc.check_onboarding_status())
}

#[tauri::command]
fn get_catalog(state: State<'_, DesktopState>) -> Result<Vec<CatalogEntry>, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    Ok(svc.catalog().list_catalog())
}

#[tauri::command]
fn search_catalog(state: State<'_, DesktopState>, query: String) -> Result<SearchResolution, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    Ok(svc.catalog().resolve_query(&query))
}

#[tauri::command]
fn plan_provision(
    state: State<'_, DesktopState>,
    item_id: String,
    version: Option<String>,
) -> Result<ProvisioningPlan, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    svc.provisioning().plan_provisioning(&item_id, version.as_deref())
}

#[tauri::command]
fn execute_provision(
    state: State<'_, DesktopState>,
    plan: ProvisioningPlan,
) -> Result<ProvisioningResult, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    svc.provisioning().execute_transaction(&plan, |_stage, _prog| {})
}

#[tauri::command]
fn recover_staging_transactions(
    state: State<'_, DesktopState>,
) -> Result<Vec<InterruptedInstallRecovery>, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    Ok(svc.provisioning().recover_interrupted_transactions())
}

#[tauri::command]
async fn get_storage_report(state: State<'_, DesktopState>) -> Result<ComprehensiveStorageReport, String> {
    let storage = state.services.lock().map_err(|e| e.to_string())?.storage().clone();
    tauri::async_runtime::spawn_blocking(move || storage.get_storage_report())
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn prune_cache(
    state: State<'_, DesktopState>,
) -> Result<PackageCachePruneReport, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    svc.storage().prune_cache(false)
}

#[tauri::command]
fn gc_cache(
    state: State<'_, DesktopState>,
    max_gb: Option<f64>,
) -> Result<PackageCacheGcReport, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    svc.storage().gc_cache(max_gb, false)
}

#[tauri::command]
async fn get_adoption_report(state: State<'_, DesktopState>) -> Result<AdoptionPlanReport, String> {
    let root = state.services.lock().map_err(|e| e.to_string())?.vault_root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || AdoptionService::new(root).discover_system_installations())
        .await.map_err(|e| e.to_string())
}

#[tauri::command]
fn adopt_tool(
    state: State<'_, DesktopState>,
    name: String,
    path: String,
    ecosystem: String,
) -> Result<String, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    let eco = match ecosystem.to_lowercase().as_str() {
        "python" => ulvo_core::Ecosystem::Python,
        "rust" => ulvo_core::Ecosystem::Rust,
        "nodejs" | "node" => ulvo_core::Ecosystem::NodeJs,
        "go" | "golang" => ulvo_core::Ecosystem::Go,
        "ruby" => ulvo_core::Ecosystem::Ruby,
        "java" => ulvo_core::Ecosystem::Java,
        "cpp" | "c++" | "c" => ulvo_core::Ecosystem::Cpp,
        "dotnet" | ".net" => ulvo_core::Ecosystem::DotNet,
        _ => ulvo_core::Ecosystem::Generic(ecosystem),
    };
    svc.adoption()
        .adopt_external_installation(&name, std::path::Path::new(&path), eco)
}

#[tauri::command]
fn get_gpu_telemetry(state: State<'_, DesktopState>) -> Result<Option<GpuTelemetry>, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    Ok(svc.adoption().get_gpu_telemetry())
}

#[tauri::command]
async fn audit_environments(state: State<'_, DesktopState>) -> Result<VaultEnvironmentsAuditReport, String> {
    let root = state.services.lock().map_err(|e| e.to_string())?.vault_root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || ProjectService::new(root).audit_vault_environments())
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn reclaim_environments(
    state: State<'_, DesktopState>,
    dry_run: bool,
) -> Result<VaultEnvironmentsGcReport, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    svc.projects().prune_inactive_environments(dry_run)
}

#[tauri::command]
fn get_user_preferences(state: State<'_, DesktopState>) -> Result<UserPreferences, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    Ok(svc.load_preferences())
}

#[tauri::command]
fn save_user_preferences(
    state: State<'_, DesktopState>,
    prefs: UserPreferences,
) -> Result<(), String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    svc.save_preferences(&prefs)
}

#[tauri::command]
async fn get_agent_status(state: State<'_, DesktopState>) -> Result<AgentStatusSummary, String> {
    let root = state.services.lock().map_err(|e| e.to_string())?.vault_root().to_path_buf();
    let response = tauri::async_runtime::spawn_blocking(move || AgentIpcServer::new(root).send_request(AgentAction::Status))
        .await.map_err(|e| e.to_string())?;
    match response {
        Ok(resp) => {
            let pid = resp
                .data
                .as_ref()
                .and_then(|d| d.get("pid"))
                .and_then(|p| p.as_u64());
            Ok(AgentStatusSummary {
                is_running: resp.success,
                pid,
                port: DEFAULT_AGENT_PORT,
                message: resp.message,
            })
        }
        Err(e) => Ok(AgentStatusSummary {
            is_running: false,
            pid: None,
            port: DEFAULT_AGENT_PORT,
            message: e,
        }),
    }
}

#[tauri::command]
fn trigger_agent_reconciliation(state: State<'_, DesktopState>) -> Result<serde_json::Value, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    match svc.agent().send_request(AgentAction::ReconcileLeases) {
        Ok(resp) => Ok(resp.data.unwrap_or(serde_json::Value::Null)),
        Err(_) => svc.agent().run_reconciliation_cycle(),
    }
}

#[tauri::command]
async fn get_project_registry(state: State<'_, DesktopState>) -> Result<Vec<ulvo_core::onboarding::RegisteredProject>, String> {
    let root = state.services.lock().map_err(|e| e.to_string())?.vault_root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || ProjectService::new(root).list_registered_projects())
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn scan_workspace_projects(
    state: State<'_, DesktopState>,
    workspace_root: Option<String>,
    max_depth: Option<usize>,
) -> Result<Vec<ulvo_core::onboarding::RegisteredProject>, String> {
    let root = state.services.lock().map_err(|e| e.to_string())?.vault_root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || ProjectService::new(root).scan_and_sync_workspace(workspace_root.as_deref(), max_depth))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn register_project(
    state: State<'_, DesktopState>,
    path: String,
) -> Result<ulvo_core::onboarding::RegisteredProject, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    svc.projects().register_project(&path)
}

#[tauri::command]
fn onboard_project(
    state: State<'_, DesktopState>,
    path: String,
) -> Result<ulvo_core::onboarding::RegisteredProject, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    svc.projects().onboard_project(&path)
}

#[tauri::command]
fn start_agent(state: State<'_, DesktopState>) -> Result<u32, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    svc.agent().ensure_running()
}

#[tauri::command]
fn stop_agent(state: State<'_, DesktopState>) -> Result<String, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    svc.agent().stop_daemon()
}

#[tauri::command]
fn get_agent_diagnostics(state: State<'_, DesktopState>) -> Result<AgentDiagnosticsReport, String> {
    let svc = state.services.lock().map_err(|e| e.to_string())?;
    Ok(svc.agent().get_diagnostics())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let default_vault = std::env::var("ULVO_VAULT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("E:\\Workspace\\Active\\ULVO\\vault"));

    let services = UlvoServices::new(&default_vault);

    let preferences = services.load_preferences();
    if preferences.enable_background_agent {
        let _ = services.agent().ensure_running();
    }

    // Dynamic project discovery: watch workspace roots and emit
    // "projects-changed" so the Projects page updates without a manual rescan.
    let (watch_tx, watch_rx) = std::sync::mpsc::channel::<u64>();
    let watcher = match WatcherSupervisor::start(&default_vault, Some(watch_tx)) {
        Ok(sup) => Some(sup),
        Err(e) => {
            eprintln!("[ULVO Desktop] dynamic project watcher disabled: {e}");
            None
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            use tauri::Emitter;
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                while let Ok(version) = watch_rx.recv() {
                    let _ = app_handle.emit(
                        "projects-changed",
                        serde_json::json!({ "version": version }),
                    );
                }
            });
            Ok(())
        })
        .manage(DesktopState {
            services: Mutex::new(services),
            watcher: Mutex::new(watcher),
        })
        .invoke_handler(tauri::generate_handler![
            get_onboarding_status,
            get_catalog,
            search_catalog,
            plan_provision,
            execute_provision,
            recover_staging_transactions,
            get_storage_report,
            prune_cache,
            gc_cache,
            get_adoption_report,
            adopt_tool,
            get_gpu_telemetry,
            audit_environments,
            reclaim_environments,
            get_user_preferences,
            save_user_preferences,
            get_agent_status,
            trigger_agent_reconciliation,
            get_project_registry,
            scan_workspace_projects,
            register_project,
            onboard_project,
            start_agent,
            stop_agent,
            get_agent_diagnostics
        ])
        .run(tauri::generate_context!())
        .expect("error while running ULVO desktop application");
}
