export interface OnboardingStatus {
  is_first_run: boolean;
  vault_path: string;
  total_managed_resources: number;
  system_tools_discovered: number;
  guided_recommendations: string[];
}

export type CatalogCategory = "runtime" | "compiler" | "package_manager" | "build_tool" | "sdk";

export type InstalledStatus =
  | "not_installed"
  | { installed: { version: string; managed_path: string } }
  | { update_available: { current: string; latest: string } };

export interface CatalogEntry {
  id: string;
  name: string;
  display_name: string;
  category: CatalogCategory;
  ecosystem: any;
  available_versions: string[];
  recommended_version: string;
  release_channel: string;
  platform: string;
  architecture: string;
  publisher: string;
  description: string;
  installation_method: string;
  expected_download_bytes?: number;
  installed_size_estimate_bytes?: number;
  compatibility_requirements: string[];
  installed_status: InstalledStatus;
  verification_status: string;
}

export interface SearchResolution {
  raw_query: string;
  intent: any;
  matched_entries: CatalogEntry[];
  ecosystem_hint?: any;
  suggested_action: string;
}

export interface ProvisioningPlan {
  transaction_id: string;
  entry_id: string;
  display_name: string;
  version: string;
  ecosystem: any;
  download_url: string;
  expected_sha256: string;
  target_vault_path: string;
  staging_path: string;
  backup_path?: string;
  required_bytes: number;
  safety_reserve_bytes: number;
  environment_variables: [string, string][];
}

export interface ProvisioningResult {
  transaction_id: string;
  entry_id: string;
  version: string;
  installed_path: string;
  sha256_verified: boolean;
  validation_output: string;
  registered_resource_id: string;
  final_stage: string;
}

export interface ComprehensiveStorageReport {
  drive_letter: string;
  total_capacity_bytes: number;
  available_free_bytes: number;
  safety_reserve_bytes: number;
  safety_floor_satisfied: boolean;
  vault_physical_bytes: number;
  package_cache_physical_bytes: number;
  shared_store_physical_bytes: number;
  ephemeral_env_physical_bytes: number;
  cas_physical_bytes: number;
  hardlink_sharing_ratio: number;
  package_cache_status: {
    uv_cache_dir: string;
    total_physical_bytes: number;
    total_logical_bytes: number;
    archive_count: number;
    built_wheels_count: number;
    source_artifacts_count: number;
    unreferenced_bytes: number;
  };
}

export interface PackageCachePruneReport {
  schema_version: number;
  pruned_archives_count: number;
  physical_bytes_reclaimed: number;
  reclaimed_human: string;
  dry_run: boolean;
  diagnostic: string;
}

export interface PackageCacheGcReport {
  schema_version: number;
  budget_bytes: number;
  initial_physical_bytes: number;
  final_physical_bytes: number;
  physical_bytes_reclaimed: number;
  deleted_items_count: number;
  dry_run: boolean;
  diagnostic: string;
}

export interface DiscoveredInstallation {
  name: string;
  ecosystem: any;
  path: string;
  version?: string;
  classification: "managed" | "external" | "adoptable" | "system_managed" | "blocked" | "unknown";
  recommended_action: "use_existing" | "provision_managed" | "plan_migration" | "retain_external";
  explanation: string;
  relocatable: boolean;
}

export interface AdoptionPlanReport {
  total_discovered: number;
  managed_count: number;
  adoptable_count: number;
  system_managed_count: number;
  external_count: number;
  installations: DiscoveredInstallation[];
}

export interface GpuTelemetry {
  name: string;
  driver_version: string;
  vram_total: string;
  vram_used: string;
  compute_capability: string;
  pytorch_compatible: boolean | null;
  kernel_mode: string;
}

export interface VaultEnvironmentsAuditReport {
  schema_version: number;
  vault_environments_dir: string;
  total_environments: number;
  active_environments: number;
  reclaimable_environments: number;
  aborted_environments: number;
  unknown_environments: number;
  total_logical_bytes: number;
  total_physical_bytes: number;
  items: {
    environment_id: string;
    project_path: string | null;
    state: string;
    is_quiescent: boolean;
    active_pids: number[];
    physical_bytes: number;
  }[];
}

export interface VaultEnvironmentsGcReport {
  schema_version: number;
  reclaimed_count: number;
  reclaimed_bytes: number;
  remaining_active_count: number;
  reclaimed_environments: string[];
}

export interface UserPreferences {
  vault_path: string;
  safety_reserve_mb: number;
  cache_budget_gb?: number;
  auto_update: boolean;
  enable_gateway_path: boolean;
  enable_background_agent: boolean;
}

export interface AgentStatusSummary {
  is_running: boolean;
  pid?: number;
  port: number;
  message: string;
}

export type ProjectRegistrationState =
  | "discovered"
  | "registered"
  | "managed"
  | "active"
  | "blocked"
  | "missing";

export type ProjectClassification =
  | "standard"
  | "template"
  | "test_fixture"
  | "backup_copy"
  | "git_worktree"
  | "monorepo_root"
  | "monorepo_package";

export interface RegisteredProject {
  id: string;
  name: string;
  canonical_path: string;
  ecosystem: string;
  manifests: string[];
  manifest_fingerprint: string;
  discovery_source: string;
  registration_state: ProjectRegistrationState;
  has_connector: boolean;
  is_monorepo: boolean;
  subprojects: string[];
  runtime_details?: string;
  fs_identity?: string | null;
  connector_status?: { status: "valid" | "corrupted" | "invalid" | "missing"; details?: string } | null;
  runtime_requirements?: string | null;
  classification?: ProjectClassification;
  validation_warnings?: string[];
  last_seen_utc: string;
}

export interface AgentDiagnosticsReport {
  is_running: boolean;
  pid?: number;
  port: number;
  vault_root: string;
  token_path: string;
  token_exists: boolean;
  port_path: string;
  port_exists: boolean;
  pid_path: string;
  pid_exists: boolean;
  active_leases: number;
  message: string;
}
