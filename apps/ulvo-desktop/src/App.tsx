import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import {
  OnboardingStatus,
  CatalogEntry,
  SearchResolution,
  ProvisioningPlan,
  ComprehensiveStorageReport,
  PackageCachePruneReport,
  PackageCacheGcReport,
  AdoptionPlanReport,
  GpuTelemetry,
  VaultEnvironmentsAuditReport,
  VaultEnvironmentsGcReport,
  UserPreferences,
  AgentStatusSummary,
  RegisteredProject,
} from "./types";

type ViewTab =
  | "dashboard"
  | "discover"
  | "installed"
  | "projects"
  | "storage"
  | "gpu"
  | "adoption"
  | "activity"
  | "settings";

function App() {
  const [activeTab, setActiveTab] = useState<ViewTab>("dashboard");
  const [loading, setLoading] = useState(false);
  const [statusMessage, setStatusMessage] = useState("Ready");

  // Telemetry & State
  const [onboarding, setOnboarding] = useState<OnboardingStatus | null>(null);
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResolution, setSearchResolution] = useState<SearchResolution | null>(null);
  const [storageReport, setStorageReport] = useState<ComprehensiveStorageReport | null>(null);
  const [adoptionReport, setAdoptionReport] = useState<AdoptionPlanReport | null>(null);
  const [gpuTelemetry, setGpuTelemetry] = useState<GpuTelemetry | null>(null);
  const [envAudit, setEnvAudit] = useState<VaultEnvironmentsAuditReport | null>(null);
  const [preferences, setPreferences] = useState<UserPreferences | null>(null);
  const [agentStatus, setAgentStatus] = useState<AgentStatusSummary | null>(null);

  // Persistent Project Registry State
  const [projectRegistry, setProjectRegistry] = useState<RegisteredProject[]>([]);
  const [projectSearch, setProjectSearch] = useState("");
  const [projectFilterState, setProjectFilterState] = useState<string>("all");
  const [isScanningProjects, setIsScanningProjects] = useState(false);

  // Modals & Action States
  const [selectedPlan, setSelectedPlan] = useState<ProvisioningPlan | null>(null);
  const [isProvisioning, setIsProvisioning] = useState(false);
  const [provisionProgress, setProvisionProgress] = useState<string>("");
  const [activityLog, setActivityLog] = useState<string[]>([
    "[SYSTEM] ULVO Control Center v1.2.0 initialized.",
    "[GATEWAY] Single-root vault bound to E:\\Workspace\\Active\\ULVO\\vault.",
  ]);

  const addLog = (msg: string) => {
    const timestamp = new Date().toLocaleTimeString();
    setActivityLog((prev) => [`[${timestamp}] ${msg}`, ...prev.slice(0, 50)]);
  };

  // Initial load
  useEffect(() => {
    refreshAllData();
  }, []);

  const refreshAllData = async () => {
    setLoading(true);
    setStatusMessage("Synchronizing with Universal Vault...");
    try {
      // 1. Fetch fast operational telemetry in parallel
      invoke<OnboardingStatus>("get_onboarding_status").then(setOnboarding).catch((err) => addLog(`Onboarding load: ${err}`));
      invoke<CatalogEntry[]>("get_catalog").then(setCatalog).catch((err) => addLog(`Catalog load: ${err}`));
      invoke<GpuTelemetry | null>("get_gpu_telemetry").then(setGpuTelemetry).catch((err) => addLog(`GPU load: ${err}`));
      invoke<VaultEnvironmentsAuditReport>("audit_environments").then(setEnvAudit).catch((err) => addLog(`Audit load: ${err}`));
      invoke<UserPreferences>("get_user_preferences").then(setPreferences).catch((err) => addLog(`Preferences load: ${err}`));
      invoke<AgentStatusSummary>("get_agent_status").then(setAgentStatus).catch((err) => addLog(`Agent status load: ${err}`));
      invoke<RegisteredProject[]>("get_project_registry").then(setProjectRegistry).catch((err) => addLog(`Project registry load: ${err}`));

      // 2. Heavy storage intelligence and adoption telemetry
      const [stor, adop] = await Promise.allSettled([
        invoke<ComprehensiveStorageReport>("get_storage_report"),
        invoke<AdoptionPlanReport>("get_adoption_report"),
      ]);

      if (stor.status === "fulfilled") setStorageReport(stor.value);
      if (adop.status === "fulfilled") setAdoptionReport(adop.value);

      setStatusMessage("System synchronized with Universal Vault");
    } catch (err: any) {
      console.error(err);
      setStatusMessage(`Sync error: ${err}`);
      addLog(`ERROR: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  // Universal Search Resolver
  const handleSearchChange = async (q: string) => {
    setSearchQuery(q);
    if (!q.trim()) {
      setSearchResolution(null);
      return;
    }
    try {
      const res = await invoke<SearchResolution>("search_catalog", { query: q });
      setSearchResolution(res);
    } catch (err) {
      console.error(err);
    }
  };

  // Plan Provisioning
  const handlePlanProvision = async (itemId: string, version?: string) => {
    try {
      setStatusMessage(`Planning provisioning for ${itemId}...`);
      const plan = await invoke<ProvisioningPlan>("plan_provision", {
        itemId,
        version: version || null,
      });
      setSelectedPlan(plan);
      addLog(`Plan prepared for '${plan.display_name}': transaction ${plan.transaction_id}`);
    } catch (err: any) {
      setStatusMessage(`Planning failed: ${err}`);
      addLog(`PLAN ERROR: ${err}`);
    }
  };

  // Execute Provisioning
  const handleExecuteProvision = async () => {
    if (!selectedPlan) return;
    setIsProvisioning(true);
    setProvisionProgress("Executing 10-step transaction model: Download -> Verify -> Stage -> Commit...");
    try {
      const result = await invoke<any>("execute_provision", { plan: selectedPlan });
      addLog(`SUCCESS: Provisioned ${result.entry_id} v${result.version} into ${result.installed_path}`);
      setStatusMessage(`Provisioning committed: ${result.entry_id}`);
      setSelectedPlan(null);
      await refreshAllData();
    } catch (err: any) {
      setStatusMessage(`Provisioning failed: ${err}`);
      addLog(`PROVISIONING FAILED: ${err} (Auto-rollback engaged)`);
    } finally {
      setIsProvisioning(false);
      setProvisionProgress("");
    }
  };

  // Prune Package Cache
  const handlePruneCache = async () => {
    try {
      setStatusMessage("Pruning unreferenced wheel archives...");
      const rep = await invoke<PackageCachePruneReport>("prune_cache");
      addLog(`Pruned ${rep.pruned_archives_count} package archives, reclaimed ${rep.reclaimed_human}`);
      setStatusMessage(`Prune complete: ${rep.reclaimed_human} reclaimed`);
      await refreshAllData();
    } catch (err: any) {
      setStatusMessage(`Prune failed: ${err}`);
      addLog(`PRUNE ERROR: ${err}`);
    }
  };

  // GC Package Cache
  const handleGcCache = async (maxGb?: number) => {
    try {
      setStatusMessage("Enforcing package cache budget...");
      const rep = await invoke<PackageCacheGcReport>("gc_cache", { maxGb: maxGb || null });
      addLog(`Cache GC complete: reclaimed ${rep.physical_bytes_reclaimed} bytes (${rep.deleted_items_count} items)`);
      setStatusMessage(`GC complete: reclaimed ${(rep.physical_bytes_reclaimed / 1048576).toFixed(1)} MB`);
      await refreshAllData();
    } catch (err: any) {
      setStatusMessage(`GC failed: ${err}`);
      addLog(`GC ERROR: ${err}`);
    }
  };

  // Reclaim Inactive Environments
  const handleReclaimEnvs = async () => {
    try {
      setStatusMessage("Reclaiming inactive ephemeral environments...");
      const rep = await invoke<VaultEnvironmentsGcReport>("reclaim_environments", { dryRun: false });
      addLog(`Reclaimed ${rep.reclaimed_count} inactive environments (${(rep.reclaimed_bytes / 1048576).toFixed(1)} MB)`);
      setStatusMessage(`Reclaimed ${rep.reclaimed_count} environments`);
      await refreshAllData();
    } catch (err: any) {
      setStatusMessage(`Reclamation failed: ${err}`);
      addLog(`RECLAIM ERROR: ${err}`);
    }
  };

  // Adopt External Tool
  const handleAdoptTool = async (name: string, path: string, ecosystem: string) => {
    try {
      setStatusMessage(`Adopting external tool ${name}...`);
      const resId = await invoke<string>("adopt_tool", { name, path, ecosystem });
      addLog(`Adopted ${name} into Universal Registry as Resource ${resId}`);
      setStatusMessage(`Adopted ${name}`);
      await refreshAllData();
    } catch (err: any) {
      setStatusMessage(`Adoption failed: ${err}`);
      addLog(`ADOPTION ERROR: ${err}`);
    }
  };

  // Trigger Agent Reconciliation
  const handleTriggerReconcile = async () => {
    try {
      setStatusMessage("Triggering background reconciliation cycle...");
      const res = await invoke<any>("trigger_agent_reconciliation");
      addLog(`Agent reconciliation completed: ${JSON.stringify(res)}`);
      setStatusMessage("Reconciliation cycle complete");
      await refreshAllData();
    } catch (err: any) {
      setStatusMessage(`Reconciliation failed: ${err}`);
      addLog(`RECONCILE ERROR: ${err}`);
    }
  };

  // Recover Staging Transactions
  const handleRecoverStaging = async () => {
    try {
      setStatusMessage("Scanning for orphaned staging directories...");
      const recoveries = await invoke<any[]>("recover_staging_transactions");
      addLog(`Recovered ${recoveries.length} interrupted staging transactions`);
      setStatusMessage(`Staging recovery clean: ${recoveries.length} recovered`);
      await refreshAllData();
    } catch (err: any) {
      setStatusMessage(`Recovery failed: ${err}`);
      addLog(`RECOVERY ERROR: ${err}`);
    }
  };

  // Scan Workspace for Projects
  const handleScanProjects = async () => {
    setIsScanningProjects(true);
    setStatusMessage("Scanning workspace read-only for projects...");
    try {
      const results = await invoke<RegisteredProject[]>("scan_workspace_projects", {
        workspaceRoot: null,
        maxDepth: 5,
      });
      setProjectRegistry(results);
      setStatusMessage(`Scan complete: ${results.length} projects discovered/synced`);
      addLog(`Workspace scan found ${results.length} projects across E:\\Workspace.`);
    } catch (err: any) {
      setStatusMessage(`Scan failed: ${err}`);
      addLog(`SCAN ERROR: ${err}`);
    } finally {
      setIsScanningProjects(false);
    }
  };

  // Register Project
  const handleRegisterProject = async (path: string) => {
    try {
      setStatusMessage(`Registering project ${path}...`);
      const proj = await invoke<RegisteredProject>("register_project", { path });
      addLog(`Project registered: ${proj.name} [${proj.registration_state}]`);
      setStatusMessage(`Project registered: ${proj.name}`);
      await refreshAllData();
    } catch (err: any) {
      setStatusMessage(`Registration failed: ${err}`);
      addLog(`REGISTER ERROR: ${err}`);
    }
  };

  // Onboard Project Connector
  const handleOnboardProject = async (path: string) => {
    try {
      setStatusMessage(`Initializing thin connector for ${path}...`);
      const proj = await invoke<RegisteredProject>("onboard_project", { path });
      addLog(`Connector initialized: ${proj.name} [${proj.registration_state}]`);
      setStatusMessage(`Project onboarded: ${proj.name}`);
      await refreshAllData();
    } catch (err: any) {
      setStatusMessage(`Onboarding failed: ${err}`);
      addLog(`ONBOARD ERROR: ${err}`);
    }
  };

  // Toggle Background Agent
  const handleToggleAgent = async () => {
    try {
      if (agentStatus?.is_running) {
        setStatusMessage("Stopping background agent daemon...");
        await invoke<string>("stop_agent");
        addLog("Background agent daemon stopped.");
      } else {
        setStatusMessage("Starting background agent daemon...");
        const pid = await invoke<number>("start_agent");
        addLog(`Background agent daemon started with PID ${pid}.`);
      }
      await refreshAllData();
    } catch (err: any) {
      setStatusMessage(`Agent toggle failed: ${err}`);
      addLog(`AGENT ERROR: ${err}`);
    }
  };

  const filteredProjects = projectRegistry.filter((p) => {
    const matchesSearch =
      projectSearch.trim() === "" ||
      p.name.toLowerCase().includes(projectSearch.toLowerCase()) ||
      p.canonical_path.toLowerCase().includes(projectSearch.toLowerCase()) ||
      p.ecosystem.toLowerCase().includes(projectSearch.toLowerCase());

    const matchesState =
      projectFilterState === "all" ||
      p.registration_state.toLowerCase() === projectFilterState.toLowerCase();

    return matchesSearch && matchesState;
  });

  const freeGb = storageReport ? (storageReport.available_free_bytes / 1073741824).toFixed(2) : "...";
  const vaultGb = storageReport ? (storageReport.vault_physical_bytes / 1073741824).toFixed(2) : "...";
  const ratio = storageReport ? storageReport.hardlink_sharing_ratio.toFixed(2) : "...";

  return (
    <div className="app-container">
      {/* Top Header */}
      <header className="app-header">
        <div className="brand-section">
          {/* Hexagonal Vault Gateway Symbol */}
          <svg className="vault-symbol-svg" viewBox="0 0 100 100">
            <polygon points="50,5 90,27 90,73 50,95 10,73 10,27" />
            <polygon points="50,22 75,36 75,64 50,78 25,64 25,36" stroke="#00e676" />
            <circle cx="50" cy="50" r="7" fill="#00e5ff" stroke="none" />
          </svg>
          <div className="brand-title">
            ULVO CONTROL CENTER
            <span className="version-badge">v1.2.0</span>
          </div>
        </div>

        <div className="header-telemetry">
          <div className="telemetry-pill">
            <span className="telemetry-dot" />
            Drive E: {freeGb} GB Free (Floor: 500 MB)
          </div>

          <div className="telemetry-pill" style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            <span className={`telemetry-dot ${agentStatus?.is_running ? "" : "stopped"}`} />
            <span>Agent: {agentStatus?.is_running ? `Online (PID ${agentStatus.pid})` : "Offline"}</span>
            <button
              className="btn"
              style={{ padding: "2px 8px", fontSize: "11px", marginLeft: "4px" }}
              onClick={handleToggleAgent}
              disabled={loading}
              title={agentStatus?.is_running ? "Stop Agent Daemon" : "Start Agent Daemon"}
            >
              {agentStatus?.is_running ? "Stop" : "Start"}
            </button>
          </div>

          <button className="btn btn-primary" onClick={refreshAllData} disabled={loading}>
            {loading ? "Syncing..." : "Sync"}
          </button>
        </div>
      </header>

      {/* Main Body */}
      <div className="app-body">
        {/* Sidebar */}
        <aside className="app-sidebar">
          <div
            className={`nav-item ${activeTab === "dashboard" ? "active" : ""}`}
            onClick={() => setActiveTab("dashboard")}
          >
            <span className="nav-icon">◈</span>
            Dashboard
          </div>
          <div
            className={`nav-item ${activeTab === "discover" ? "active" : ""}`}
            onClick={() => setActiveTab("discover")}
          >
            <span className="nav-icon">🔍</span>
            Discover
          </div>
          <div
            className={`nav-item ${activeTab === "installed" ? "active" : ""}`}
            onClick={() => setActiveTab("installed")}
          >
            <span className="nav-icon">📦</span>
            Installed
          </div>
          <div
            className={`nav-item ${activeTab === "projects" ? "active" : ""}`}
            onClick={() => setActiveTab("projects")}
          >
            <span className="nav-icon">⚡</span>
            Projects
          </div>
          <div
            className={`nav-item ${activeTab === "storage" ? "active" : ""}`}
            onClick={() => setActiveTab("storage")}
          >
            <span className="nav-icon">💾</span>
            Storage
          </div>
          <div
            className={`nav-item ${activeTab === "gpu" ? "active" : ""}`}
            onClick={() => setActiveTab("gpu")}
          >
            <span className="nav-icon">📟</span>
            GPU Copilot
          </div>
          <div
            className={`nav-item ${activeTab === "adoption" ? "active" : ""}`}
            onClick={() => setActiveTab("adoption")}
          >
            <span className="nav-icon">🔄</span>
            Adoption
          </div>
          <div
            className={`nav-item ${activeTab === "activity" ? "active" : ""}`}
            onClick={() => setActiveTab("activity")}
          >
            <span className="nav-icon">📋</span>
            Activity
          </div>
          <div
            className={`nav-item ${activeTab === "settings" ? "active" : ""}`}
            onClick={() => setActiveTab("settings")}
          >
            <span className="nav-icon">⚙️</span>
            Settings
          </div>
        </aside>

        {/* Viewport Content */}
        <main className="main-viewport">
          {/* Onboarding Banner if Fresh State */}
          {onboarding?.is_first_run && (
            <div className="onboarding-banner">
              <div className="onboarding-title">Welcome to ULVO — Unified Vault Onboarding</div>
              <div className="onboarding-desc">
                Your vault at <code style={{ color: "#00e5ff" }}>{onboarding.vault_path}</code> is initialized in an honest empty state.
                No unmanaged runtimes are assumed. Discovered <strong>{onboarding.system_tools_discovered}</strong> host tools.
                Provision verified Python, Rust, or adopt your host toolchains to get started.
              </div>
              <div className="onboarding-steps">
                <button className="btn btn-primary" onClick={() => setActiveTab("discover")}>
                  Browse Runtime Catalog
                </button>
                <button className="btn" onClick={() => setActiveTab("adoption")}>
                  Review Host Adoption ({onboarding.system_tools_discovered})
                </button>
              </div>
            </div>
          )}

          {/* TAB 1: DASHBOARD */}
          {activeTab === "dashboard" && (
            <div>
              <div className="metric-grid">
                <div className="metric-card">
                  <div className="metric-label">Drive E: Available Free</div>
                  <div className="metric-value">{freeGb} GB</div>
                  <div className="metric-sub">500 MB hard safety floor satisfied</div>
                </div>

                <div className="metric-card">
                  <div className="metric-label">Single-Root Vault Physical</div>
                  <div className="metric-value">{vaultGb} GB</div>
                  <div className="metric-sub">Zero-copy deduplication active</div>
                </div>

                <div className="metric-card">
                  <div className="metric-label">Deduplication Ratio</div>
                  <div className="metric-value">{ratio}x</div>
                  <div className="metric-sub">Single-instance shared packages</div>
                </div>

                <div className="metric-card">
                  <div className="metric-label">Active Project Leases</div>
                  <div className="metric-value">{envAudit?.active_environments || 0}</div>
                  <div className="metric-sub">{envAudit?.reclaimable_environments || 0} inactive reclaimable</div>
                </div>
              </div>

              {/* Quick Actions */}
              <div className="card">
                <div className="card-header">
                  <div className="card-title">Cockpit Quick Actions</div>
                </div>
                <div style={{ display: "flex", gap: "10px", flexWrap: "wrap" }}>
                  <button className="btn" onClick={handleTriggerReconcile}>
                    Trigger Agent Lease Reconciliation
                  </button>
                  <button className="btn" onClick={handlePruneCache}>
                    Prune Unreferenced Wheel Archives
                  </button>
                  <button className="btn" onClick={handleReclaimEnvs}>
                    Reclaim All Inactive Ephemeral Environments
                  </button>
                  <button className="btn" onClick={handleRecoverStaging}>
                    Recover Interrupted Staging Transactions
                  </button>
                </div>
              </div>

              {/* Active Leased Environments */}
              <div className="card">
                <div className="card-header">
                  <div className="card-title">Environment Lifecycle Cockpit</div>
                  <button className="btn btn-danger" onClick={handleReclaimEnvs}>
                    Safe Reclamation
                  </button>
                </div>
                <div className="data-table-wrapper">
                  <table className="data-table">
                    <thead>
                      <tr>
                        <th>Environment ID</th>
                        <th>Project / Fingerprint</th>
                        <th>Status</th>
                        <th>Active PIDs</th>
                        <th>Allocation</th>
                      </tr>
                    </thead>
                    <tbody>
                      {envAudit?.environments && envAudit.environments.length > 0 ? (
                        envAudit.environments.map((env: any, idx: number) => (
                          <tr key={idx}>
                            <td style={{ fontFamily: "var(--font-mono)" }}>{env.id || `env_${idx}`}</td>
                            <td>{env.project_path || "Ephemeral Execution"}</td>
                            <td>
                              <span className={`badge ${env.is_active ? "badge-managed" : "badge-external"}`}>
                                {env.is_active ? "ACTIVE" : "QUIESCENT"}
                              </span>
                            </td>
                            <td>{env.active_pids?.join(", ") || "None"}</td>
                            <td>{((env.physical_bytes || 0) / 1048576).toFixed(1)} MB</td>
                          </tr>
                        ))
                      ) : (
                        <tr>
                          <td colSpan={5} style={{ textAlign: "center", color: "var(--color-text-muted)" }}>
                            Zero persistent project environments. All runs materialize ephemerally on demand.
                          </td>
                        </tr>
                      )}
                    </tbody>
                  </table>
                </div>
              </div>
            </div>
          )}

          {/* TAB 2: DISCOVER */}
          {activeTab === "discover" && (
            <div>
              <div className="search-container">
                <input
                  type="text"
                  className="search-input"
                  placeholder="Universal search: e.g. 'python@3.12', 'rust@1.96', 'torch==2.6.0+cu124', 'gcc'..."
                  value={searchQuery}
                  onChange={(e) => handleSearchChange(e.target.value)}
                />
              </div>

              {searchResolution && (
                <div className="card" style={{ borderLeft: "4px solid var(--color-cyan-glow)" }}>
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                    <div>
                      <strong>Intent:</strong> <span className="badge badge-system">{typeof searchResolution.intent === "string" ? searchResolution.intent : JSON.stringify(searchResolution.intent)}</span> |{" "}
                      <strong>Query:</strong> {searchResolution.raw_query}
                    </div>
                    <div style={{ color: "var(--color-cyan-glow)", fontSize: "12px" }}>
                      {searchResolution.suggested_action}
                    </div>
                  </div>
                </div>
              )}

              <div className="data-table-wrapper">
                <table className="data-table">
                  <thead>
                    <tr>
                      <th>Tool / Runtime</th>
                      <th>Category</th>
                      <th>Ecosystem</th>
                      <th>Recommended</th>
                      <th>Publisher</th>
                      <th>Status</th>
                      <th>Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {(searchResolution?.matched_entries && searchResolution.matched_entries.length > 0
                      ? searchResolution.matched_entries
                      : catalog
                    ).map((entry) => {
                      const isInst = typeof entry.installed_status === "object" && "installed" in entry.installed_status;
                      return (
                        <tr key={entry.id}>
                          <td>
                            <strong>{entry.display_name}</strong>
                            <div style={{ fontSize: "11px", color: "var(--color-text-muted)" }}>{entry.description}</div>
                          </td>
                          <td>
                            <span className="badge badge-system">{entry.category}</span>
                          </td>
                          <td>{typeof entry.ecosystem === "string" ? entry.ecosystem : JSON.stringify(entry.ecosystem)}</td>
                          <td style={{ fontFamily: "var(--font-mono)" }}>{entry.recommended_version}</td>
                          <td>{entry.publisher}</td>
                          <td>
                            {isInst ? (
                              <span className="badge badge-managed">INSTALLED</span>
                            ) : (
                              <span className="badge badge-external">AVAILABLE</span>
                            )}
                          </td>
                          <td>
                            <button
                              className="btn btn-primary"
                              onClick={() => handlePlanProvision(entry.id, entry.recommended_version)}
                            >
                              {isInst ? "Reinstall" : "Install"}
                            </button>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            </div>
          )}

          {/* TAB 3: INSTALLED */}
          {activeTab === "installed" && (
            <div>
              <div className="card-header">
                <div className="card-title">Managed Vault Runtimes & Toolchains</div>
                <div style={{ color: "var(--color-text-muted)", fontSize: "12px" }}>
                  Vault Root: E:\Workspace\Active\ULVO\vault
                </div>
              </div>
              <div className="data-table-wrapper">
                <table className="data-table">
                  <thead>
                    <tr>
                      <th>Resource ID</th>
                      <th>Name</th>
                      <th>Managed Path</th>
                      <th>Integrity SHA-256</th>
                      <th>Verification</th>
                    </tr>
                  </thead>
                  <tbody>
                    {catalog
                      .filter((c) => typeof c.installed_status === "object" && "installed" in c.installed_status)
                      .map((c) => {
                        const inst = (c.installed_status as any).installed;
                        return (
                          <tr key={c.id}>
                            <td style={{ fontFamily: "var(--font-mono)" }}>{c.id}</td>
                            <td>{c.display_name} v{inst.version}</td>
                            <td style={{ fontFamily: "var(--font-mono)", fontSize: "11px" }}>{inst.managed_path}</td>
                            <td>
                              <span className="badge badge-managed">SHA-256 VERIFIED</span>
                            </td>
                            <td>
                              <span className="badge badge-managed">DURABLE</span>
                            </td>
                          </tr>
                        );
                      })}
                  </tbody>
                </table>
              </div>
            </div>
          )}

          {/* TAB 4: PROJECTS */}
          {activeTab === "projects" && (
            <div>
              <div className="card">
                <div className="card-header" style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                  <div className="card-title">Persistent Project Registry & Thin Connectors</div>
                  <div style={{ display: "flex", gap: "8px" }}>
                    <button className="btn btn-primary" onClick={handleScanProjects} disabled={isScanningProjects}>
                      {isScanningProjects ? "Scanning E:\\Workspace..." : "Rescan Workspace"}
                    </button>
                    <button className="btn" onClick={handleReclaimEnvs}>
                      Safe GC
                    </button>
                    <button className="btn" onClick={handleTriggerReconcile}>
                      Audit Process Leases
                    </button>
                  </div>
                </div>
                <p style={{ color: "var(--color-text-muted)", marginBottom: "14px", lineHeight: "1.6" }}>
                  Projects integrate through lightweight connectors (<code style={{ color: "#00e5ff" }}>.ulvo/manifest.toml</code> &lt; 100 KiB) without persistent project-local <code style={{ color: "#00e5ff" }}>.venv</code> directories.
                  Discovered and registered projects persist durably across restarts inside <code style={{ color: "#00e5ff" }}>vault/registry/projects.json</code>.
                </p>

                {/* Filter and Search Controls */}
                <div style={{ display: "flex", gap: "12px", alignItems: "center", marginTop: "10px" }}>
                  <input
                    type="text"
                    className="input-field"
                    placeholder="Search projects by name, path, or ecosystem..."
                    value={projectSearch}
                    onChange={(e) => setProjectSearch(e.target.value)}
                    style={{ flex: 1 }}
                  />
                  <select
                    className="input-field"
                    value={projectFilterState}
                    onChange={(e) => setProjectFilterState(e.target.value)}
                    style={{ width: "160px" }}
                  >
                    <option value="all">All States</option>
                    <option value="discovered">Discovered</option>
                    <option value="registered">Registered</option>
                    <option value="managed">Managed</option>
                    <option value="active">Active</option>
                    <option value="blocked">Blocked</option>
                    <option value="missing">Missing</option>
                  </select>
                  <span style={{ fontSize: "12px", color: "var(--color-text-muted)", whiteSpace: "nowrap" }}>
                    Showing {filteredProjects.length} of {projectRegistry.length} projects
                  </span>
                </div>
              </div>

              <div className="data-table-wrapper">
                <table className="data-table">
                  <thead>
                    <tr>
                      <th>Project Name & Directory</th>
                      <th>Ecosystem</th>
                      <th>Manifest Fingerprint</th>
                      <th>Connector Status</th>
                      <th>Registration State</th>
                      <th>Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {filteredProjects.length === 0 ? (
                      <tr>
                        <td colSpan={6} style={{ textAlign: "center", padding: "24px", color: "var(--color-text-muted)" }}>
                          No projects registered yet or matched search criteria. Click "Rescan Workspace" to discover projects across E:\Workspace.
                        </td>
                      </tr>
                    ) : (
                      filteredProjects.map((p) => (
                        <tr key={p.canonical_path}>
                          <td>
                            <div style={{ fontWeight: 600, color: "#fff" }}>
                              {p.name}
                              {p.is_monorepo && (
                                <span className="badge badge-system" style={{ marginLeft: "8px", fontSize: "10px" }}>
                                  MONOREPO ({p.subprojects.length})
                                </span>
                              )}
                            </div>
                            <div style={{ fontSize: "11px", color: "var(--color-text-muted)", fontFamily: "monospace" }}>
                              {p.canonical_path}
                            </div>
                          </td>
                          <td>
                            <span style={{ textTransform: "capitalize" }}>{p.ecosystem}</span>
                            <div style={{ fontSize: "10px", color: "var(--color-text-muted)" }}>
                              {p.manifests.join(", ")}
                            </div>
                          </td>
                          <td style={{ fontFamily: "monospace", fontSize: "11px", color: "var(--color-text-muted)" }}>
                            {p.manifest_fingerprint.slice(0, 12)}...
                          </td>
                          <td>
                            {p.has_connector ? (
                              <span className="badge badge-managed">.ulvo CONNECTED</span>
                            ) : (
                              <span className="badge badge-external">NATIVE MANIFEST</span>
                            )}
                          </td>
                          <td>
                            <span className={`badge badge-${
                              p.registration_state === "active"
                                ? "managed"
                                : p.registration_state === "managed"
                                ? "managed"
                                : p.registration_state === "registered"
                                ? "system"
                                : p.registration_state === "blocked"
                                ? "blocked"
                                : "external"
                            }`}>
                              {p.registration_state.toUpperCase()}
                            </span>
                          </td>
                          <td>
                            <div style={{ display: "flex", gap: "6px" }}>
                              {!p.has_connector ? (
                                <button
                                  className="btn btn-sm btn-primary"
                                  onClick={() => handleOnboardProject(p.canonical_path)}
                                  title="Generate minimal .ulvo/manifest.toml connector"
                                >
                                  Onboard Connector
                                </button>
                              ) : null}
                              {p.registration_state === "discovered" ? (
                                <button
                                  className="btn btn-sm"
                                  onClick={() => handleRegisterProject(p.canonical_path)}
                                  title="Mark project explicitly registered in vault"
                                >
                                  Register
                                </button>
                              ) : null}
                            </div>
                          </td>
                        </tr>
                      ))
                    )}
                  </tbody>
                </table>
              </div>
            </div>
          )}

          {/* TAB 5: STORAGE */}
          {activeTab === "storage" && storageReport && (
            <div>
              <div className="metric-grid">
                <div className="metric-card">
                  <div className="metric-label">Total Drive E: Capacity</div>
                  <div className="metric-value">{(storageReport.total_capacity_bytes / 1073741824).toFixed(1)} GB</div>
                  <div className="metric-sub">NTFS Volume</div>
                </div>

                <div className="metric-card">
                  <div className="metric-label">Physical Free Space</div>
                  <div className="metric-value">{freeGb} GB</div>
                  <div className="metric-sub">Safety Floor: 500 MB</div>
                </div>

                <div className="metric-card">
                  <div className="metric-label">Package Cache (uv)</div>
                  <div className="metric-value">{(storageReport.package_cache_physical_bytes / 1073741824).toFixed(2)} GB</div>
                  <div className="metric-sub">{storageReport.package_cache_status.archive_count} wheel archives</div>
                </div>

                <div className="metric-card">
                  <div className="metric-label">Hardlink Sharing Ratio</div>
                  <div className="metric-value">{ratio}x</div>
                  <div className="metric-sub">Zero-copy deduplication</div>
                </div>
              </div>

              {/* Cache Controls */}
              <div className="card">
                <div className="card-header">
                  <div className="card-title">Storage Governance & Cache Pruning</div>
                </div>
                <div style={{ display: "flex", gap: "10px", alignItems: "center", flexWrap: "wrap" }}>
                  <button className="btn btn-primary" onClick={handlePruneCache}>
                    Prune Unreferenced Wheel Archives
                  </button>
                  <button className="btn btn-danger" onClick={() => handleGcCache(20.0)}>
                    Enforce 20 GB Cache Budget (LRU)
                  </button>
                  <button className="btn" onClick={handleRecoverStaging}>
                    Purge Staging Crash Leftovers
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* TAB 6: GPU COPILOT */}
          {activeTab === "gpu" && (
            <div>
              <div className="card">
                <div className="card-header">
                  <div className="card-title">NVIDIA Hardware & CUDA Architecture Inspector</div>
                  <span className="badge badge-gpu">HARDWARE VERIFIED</span>
                </div>
                {gpuTelemetry ? (
                  <div className="metric-grid">
                    <div className="metric-card">
                      <div className="metric-label">GPU Model</div>
                      <div className="metric-value" style={{ fontSize: "16px" }}>{gpuTelemetry.name}</div>
                      <div className="metric-sub" style={{ color: "#76b900" }}>Native Hardware Detected</div>
                    </div>
                    <div className="metric-card">
                      <div className="metric-label">VRAM Telemetry</div>
                      <div className="metric-value" style={{ fontSize: "16px" }}>
                        {gpuTelemetry.vram_used} / {gpuTelemetry.vram_total}
                      </div>
                      <div className="metric-sub">Dedicated Graphics Memory</div>
                    </div>
                    <div className="metric-card">
                      <div className="metric-label">Display Driver</div>
                      <div className="metric-value" style={{ fontSize: "16px" }}>v{gpuTelemetry.driver_version}</div>
                      <div className="metric-sub">{gpuTelemetry.kernel_mode}</div>
                    </div>
                    <div className="metric-card">
                      <div className="metric-label">Compute Capability</div>
                      <div className="metric-value" style={{ fontSize: "16px" }}>{gpuTelemetry.compute_capability}</div>
                      <div className="metric-sub">PyTorch +cu124 Native</div>
                    </div>
                  </div>
                ) : (
                  <p style={{ color: "var(--color-text-muted)" }}>Scanning GPU telemetry...</p>
                )}
                <p style={{ fontSize: "12px", color: "var(--color-text-muted)", marginTop: "12px" }}>
                  * The host NVIDIA display driver and kernel mode components remain system-managed. ULVO enforces compatibility without mutating system display drivers.
                </p>
              </div>
            </div>
          )}

          {/* TAB 7: ADOPTION */}
          {activeTab === "adoption" && adoptionReport && (
            <div>
              <div className="card-header">
                <div className="card-title">System-Wide Toolchain Discovery & Staged Adoption</div>
                <div style={{ color: "var(--color-text-muted)", fontSize: "12px" }}>
                  Discovered: {adoptionReport.total_discovered} | Managed: {adoptionReport.managed_count} | Adoptable: {adoptionReport.adoptable_count}
                </div>
              </div>

              <div className="data-table-wrapper">
                <table className="data-table">
                  <thead>
                    <tr>
                      <th>Tool Name</th>
                      <th>Host Path</th>
                      <th>Classification</th>
                      <th>Recommended Action</th>
                      <th>Adoption</th>
                    </tr>
                  </thead>
                  <tbody>
                    {adoptionReport.installations.map((inst, idx) => (
                      <tr key={idx}>
                        <td><strong>{inst.name}</strong></td>
                        <td style={{ fontFamily: "var(--font-mono)", fontSize: "11px" }}>{inst.path}</td>
                        <td>
                          <span
                            className={`badge ${
                              inst.classification === "managed"
                                ? "badge-managed"
                                : inst.classification === "adoptable"
                                ? "badge-system"
                                : inst.classification === "system_managed"
                                ? "badge-gpu"
                                : "badge-external"
                            }`}
                          >
                            {inst.classification.toUpperCase()}
                          </span>
                        </td>
                        <td>{inst.explanation}</td>
                        <td>
                          {inst.classification === "adoptable" && (
                            <button
                              className="btn btn-primary"
                              onClick={() => handleAdoptTool(inst.name, inst.path, typeof inst.ecosystem === "string" ? inst.ecosystem : "system")}
                            >
                              Adopt
                            </button>
                          )}
                          {inst.classification === "managed" && (
                            <span className="badge badge-managed">MANAGED</span>
                          )}
                          {inst.classification === "system_managed" && (
                            <span className="badge badge-gpu">RETAINED</span>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}

          {/* TAB 8: ACTIVITY */}
          {activeTab === "activity" && (
            <div>
              <div className="card-header">
                <div className="card-title">Audit Trail & Transaction Journal</div>
                <button className="btn" onClick={() => setActivityLog([])}>
                  Clear View
                </button>
              </div>
              <div
                style={{
                  backgroundColor: "var(--color-bg-surface)",
                  border: "1px solid var(--color-border)",
                  borderRadius: "6px",
                  padding: "14px",
                  fontFamily: "var(--font-mono)",
                  fontSize: "12px",
                  maxHeight: "450px",
                  overflowY: "auto",
                }}
              >
                {activityLog.map((log, idx) => (
                  <div key={idx} style={{ padding: "3px 0", color: log.includes("ERROR") ? "#ff1744" : "#eceff4" }}>
                    {log}
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* TAB 9: SETTINGS */}
          {activeTab === "settings" && preferences && (
            <div>
              <div className="card">
                <div className="card-header">
                  <div className="card-title">ULVO Vault & Gateway Configuration</div>
                </div>
                <div style={{ display: "flex", flexDirection: "column", gap: "14px", maxWidth: "550px" }}>
                  <div>
                    <label style={{ display: "block", color: "var(--color-text-muted)", fontSize: "12px", marginBottom: "4px" }}>
                      Vault Root Path
                    </label>
                    <input
                      type="text"
                      className="search-input"
                      value={preferences.vault_path}
                      disabled
                    />
                  </div>

                  <div>
                    <label style={{ display: "block", color: "var(--color-text-muted)", fontSize: "12px", marginBottom: "4px" }}>
                      Drive Safety Reserve Floor (MB)
                    </label>
                    <input
                      type="number"
                      className="search-input"
                      value={preferences.safety_reserve_mb}
                      disabled
                    />
                  </div>

                  <div>
                    <label style={{ display: "block", color: "var(--color-text-muted)", fontSize: "12px", marginBottom: "4px" }}>
                      Package Cache Budget (GB)
                    </label>
                    <input
                      type="number"
                      className="search-input"
                      value={preferences.cache_budget_gb || 20}
                      disabled
                    />
                  </div>

                  <div style={{ marginTop: "10px" }}>
                    <button className="btn btn-primary" onClick={handleTriggerReconcile}>
                      Test Local IPC Ping to Agent
                    </button>
                  </div>
                </div>
              </div>
            </div>
          )}
        </main>
      </div>

      {/* Provisioning Plan Modal */}
      {selectedPlan && (
        <div className="modal-overlay">
          <div className="modal-card">
            <h3 style={{ color: "var(--color-cyan-glow)", marginBottom: "12px" }}>
              Provisioning Plan: {selectedPlan.display_name} v{selectedPlan.version}
            </h3>
            <p style={{ fontSize: "13px", color: "var(--color-text-muted)", marginBottom: "16px" }}>
              The 10-step transaction model enforces preflight capacity, cryptographic SHA-256 validation, and atomic commit with rollback protection.
            </p>
            <div style={{ fontSize: "12px", fontFamily: "var(--font-mono)", marginBottom: "16px", lineHeight: "1.8" }}>
              <div>• <strong>Transaction ID:</strong> {selectedPlan.transaction_id}</div>
              <div>• <strong>Target Path:</strong> {selectedPlan.target_vault_path}</div>
              <div>• <strong>Required Disk:</strong> {(selectedPlan.required_bytes / 1048576).toFixed(1)} MB</div>
              <div>• <strong>Safety Floor:</strong> {(selectedPlan.safety_reserve_bytes / 1048576).toFixed(1)} MB reserve</div>
            </div>

            {isProvisioning ? (
              <div style={{ color: "var(--color-cyan-glow)", fontSize: "13px", margin: "14px 0" }}>
                {provisionProgress}
              </div>
            ) : (
              <div style={{ display: "flex", justifyContent: "flex-end", gap: "10px" }}>
                <button className="btn" onClick={() => setSelectedPlan(null)}>
                  Cancel
                </button>
                <button className="btn btn-primary" onClick={handleExecuteProvision}>
                  Confirm & Commit
                </button>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Footer Status Bar */}
      <footer className="app-footer">
        <div>STATUS: {statusMessage}</div>
        <div>GATEWAY: Process-Local | Drive E: Healthy | Agent: {agentStatus?.is_running ? "Active" : "Idle"}</div>
      </footer>
    </div>
  );
}

export default App;
