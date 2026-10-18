use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DependencyReproducibility {
    /// Pinned lockfile available (uv.lock, poetry.lock, Pipfile.lock, etc.)
    LockfilePinned,
    /// Explicit requirements file or pyproject.toml dependencies
    ManifestSpecified,
    /// Candidate requirements derived from installed distribution metadata
    DerivedFromInstalled,
    /// Cannot be reproduced safely (missing sources, unpinned native/local wheels, etc.)
    NonReproducible,
}

impl std::fmt::Display for DependencyReproducibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LockfilePinned => write!(f, "LOCKFILE_PINNED"),
            Self::ManifestSpecified => write!(f, "MANIFEST_SPECIFIED"),
            Self::DerivedFromInstalled => write!(f, "DERIVED_FROM_INSTALLED"),
            Self::NonReproducible => write!(f, "NON_REPRODUCIBLE"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MigrationRiskLevel {
    Low,
    Medium,
    High,
    Blocked,
}

impl std::fmt::Display for MigrationRiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "LOW"),
            Self::Medium => write!(f, "MEDIUM"),
            Self::High => write!(f, "HIGH"),
            Self::Blocked => write!(f, "BLOCKED"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MigrationStageStatus {
    Discovered,
    Integrated,
    Verified,
    Retired,
    Blocked,
}

impl std::fmt::Display for MigrationStageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Discovered => write!(f, "DISCOVERED"),
            Self::Integrated => write!(f, "INTEGRATED"),
            Self::Verified => write!(f, "VERIFIED"),
            Self::Retired => write!(f, "RETIRED"),
            Self::Blocked => write!(f, "BLOCKED"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationCandidate {
    pub project_path: String,
    pub project_name: String,
    pub legacy_env_path: String,
    pub python_version: Option<String>,
    pub logical_bytes: u64,
    pub physical_allocated_bytes: Option<u64>,
    pub unique_files: usize,
    pub reproducibility: DependencyReproducibility,
    pub manifest_sources: Vec<String>,
    pub is_ml: bool,
    pub risk_level: MigrationRiskLevel,
    pub blocker_reason: Option<String>,
    pub ulvo_integrated: bool,
    pub status: MigrationStageStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationScanReport {
    pub schema_version: u32,
    pub workspace_root: String,
    pub scanned_at_unix: u64,
    pub total_candidates: usize,
    pub total_logical_bytes: u64,
    pub total_physical_allocated_bytes: u64,
    pub candidates: Vec<MigrationCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPlan {
    pub schema_version: u32,
    pub workspace_root: String,
    pub drive_free_bytes: u64,
    pub safety_reserve_bytes: u64,
    pub available_budget_bytes: u64,
    pub candidates: Vec<MigrationCandidate>,
    pub prioritized_queue: Vec<String>,
    pub total_recoverable_bytes: u64,
    pub capacity_blocked_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationGateReport {
    pub gate_id: String,
    pub name: String,
    pub passed: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationVerificationReport {
    pub schema_version: u32,
    pub project_path: String,
    pub legacy_env_path: String,
    pub gates: Vec<VerificationGateReport>,
    pub all_passed: bool,
    pub can_retire: bool,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationApplyReport {
    pub schema_version: u32,
    pub project_path: String,
    pub manifest_path: String,
    pub manifest_created: bool,
    pub legacy_env_untouched: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRetireReport {
    pub schema_version: u32,
    pub project_path: String,
    pub legacy_env_path: String,
    pub retired: bool,
    pub physical_bytes_reclaimed: u64,
    pub volume_free_bytes_delta: Option<i64>,
    pub post_verification_passed: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationTableEntry {
    pub project_name: String,
    pub project_path: String,
    pub legacy_env_path: String,
    pub python_version: String,
    pub logical_bytes: u64,
    pub physical_bytes: u64,
    pub reproducibility: String,
    pub ulvo_integration_status: String,
    pub functional_verification: String,
    pub retirement_eligibility: String,
    pub retirement_result: String,
    pub actual_recovered_bytes: u64,
    pub blocker: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMigrationReport {
    pub schema_version: u32,
    pub workspace_root: String,
    pub total_environments_discovered: usize,
    pub successfully_integrated_projects: usize,
    pub verified_retirements: usize,
    pub blocked_environments: usize,
    pub remaining_legacy_environments: usize,
    pub total_measured_physical_recovered_bytes: u64,
    pub net_volume_free_delta_bytes: Option<i64>,
    pub summary_table: Vec<MigrationTableEntry>,
}
