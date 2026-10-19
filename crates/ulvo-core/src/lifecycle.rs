use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EnvironmentLifecycleState {
    Defined,
    Resolved,
    Materializing,
    Ready,
    Executing,
    Released,
    Reclaimable,
    Reclaimed,
    Failed,
}

impl std::fmt::Display for EnvironmentLifecycleState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Defined => write!(f, "DEFINED"),
            Self::Resolved => write!(f, "RESOLVED"),
            Self::Materializing => write!(f, "MATERIALIZING"),
            Self::Ready => write!(f, "READY"),
            Self::Executing => write!(f, "EXECUTING"),
            Self::Released => write!(f, "RELEASED"),
            Self::Reclaimable => write!(f, "RECLAIMABLE"),
            Self::Reclaimed => write!(f, "RECLAIMED"),
            Self::Failed => write!(f, "FAILED"),
        }
    }
}

impl EnvironmentLifecycleState {
    pub fn is_reclaimable(&self) -> bool {
        matches!(self, Self::Released | Self::Reclaimable | Self::Failed)
    }

    pub fn can_transition_to(&self, next: EnvironmentLifecycleState) -> bool {
        use EnvironmentLifecycleState::*;
        match (self, next) {
            (Defined, Resolved) => true,
            (Defined, Failed) => true,
            (Resolved, Materializing) => true,
            (Resolved, Failed) => true,
            (Materializing, Ready) => true,
            (Materializing, Failed) => true,
            (Ready, Executing) => true,
            (Ready, Released) => true,
            (Ready, Reclaimable) => true,
            (Executing, Released) => true,
            (Executing, Failed) => true,
            (Released, Executing) => true,
            (Released, Reclaimable) => true,
            (Released, Reclaimed) => true,
            (Reclaimable, Reclaimed) => true,
            (Reclaimable, Executing) => true,
            (Failed, Reclaimed) => true,
            (Failed, Materializing) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcessLiveness {
    Alive,
    Terminated,
    Unknown,
}

impl ProcessLiveness {
    pub fn is_alive(&self) -> bool {
        matches!(self, Self::Alive)
    }

    pub fn blocks_reclamation(&self) -> bool {
        matches!(self, Self::Alive | Self::Unknown)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LeasedProcess {
    pub pid: u32,
    #[serde(default)]
    pub process_creation_time: Option<u64>,
    pub owner_token: String,
    pub registered_at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DependencyManifestFingerprint {
    pub relative_path: String,
    pub content_sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentSpecification {
    pub project_canonical_path: String,
    pub runtime_ecosystem: String,
    pub runtime_constraint: String,
    pub runtime_resolved_path: String,
    pub package_manager: String,
    pub manifest_hash: String,
    pub dependency_manifests: Vec<DependencyManifestFingerprint>,
    pub platform: String,
    pub architecture: String,
}

pub const EPHEMERAL_OWNERSHIP_SENTINEL: &str = "ulvo-managed-ephemeral-environment-v1";

fn default_schema_version_1() -> u32 {
    1
}

pub fn canonical_paths_equal(p1: &str, p2: &str) -> bool {
    let clean1 = p1.trim_start_matches(r"\\?\").trim_end_matches(['\\', '/']).replace('/', "\\");
    let clean2 = p2.trim_start_matches(r"\\?\").trim_end_matches(['\\', '/']).replace('/', "\\");
    if clean1.eq_ignore_ascii_case(&clean2) {
        return true;
    }
    let lower1 = clean1.to_lowercase();
    let lower2 = clean2.to_lowercase();
    lower1.ends_with(&format!("\\{lower2}")) || lower2.ends_with(&format!("\\{lower1}"))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentOwnershipRecord {
    pub sentinel: String,
    #[serde(default = "default_schema_version_1")]
    pub schema_version: u32,
    pub environment_id: String,
    pub project_canonical_path: String,
    pub vault_root: String,
    #[serde(default)]
    pub specification_hash: String,
    pub created_by_pid: u32,
    pub created_at_unix: u64,
}

impl EnvironmentOwnershipRecord {
    pub fn new(
        environment_id: String,
        project_canonical_path: String,
        vault_root: String,
        specification_hash: String,
        created_by_pid: u32,
        created_at_unix: u64,
    ) -> Self {
        Self {
            sentinel: EPHEMERAL_OWNERSHIP_SENTINEL.to_string(),
            schema_version: 1,
            environment_id,
            project_canonical_path,
            vault_root,
            specification_hash,
            created_by_pid,
            created_at_unix,
        }
    }

    pub fn validate(
        &self,
        expected_project_canonical: &str,
        expected_vault_root: &str,
        expected_env_id: &str,
    ) -> Result<(), String> {
        if self.sentinel != EPHEMERAL_OWNERSHIP_SENTINEL {
            return Err(format!(
                "ownership sentinel mismatch (expected '{}', got '{}')",
                EPHEMERAL_OWNERSHIP_SENTINEL, self.sentinel
            ));
        }
        if self.schema_version != 1 {
            return Err(format!(
                "unsupported ownership record schema version: {}",
                self.schema_version
            ));
        }
        if !canonical_paths_equal(&self.project_canonical_path, expected_project_canonical) {
            return Err(format!(
                "ownership project path mismatch: recorded '{}' != expected '{}'",
                self.project_canonical_path, expected_project_canonical
            ));
        }
        if !canonical_paths_equal(&self.vault_root, expected_vault_root) {
            return Err(format!(
                "ownership vault root mismatch: recorded '{}' != expected '{}'",
                self.vault_root, expected_vault_root
            ));
        }
        if self.environment_id != expected_env_id {
            return Err(format!(
                "ownership environment_id mismatch: recorded '{}' != expected '{}'",
                self.environment_id, expected_env_id
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentLease {
    pub lease_id: String,
    pub environment_id: String,
    pub project_path: String,
    pub environment_path: String,
    pub state: EnvironmentLifecycleState,
    pub active_pids: Vec<u32>,
    #[serde(default)]
    pub leased_processes: Vec<LeasedProcess>,
    pub created_at_unix: u64,
    pub updated_at_unix: u64,
    pub lease_timeout_seconds: u64,
    pub owner_token: String,
    pub python_version: Option<String>,
    #[serde(default)]
    pub specification_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDoctorReport {
    pub schema_version: u32,
    pub project_path: String,
    pub manifest_valid: bool,
    pub uv_available: bool,
    pub uv_version: Option<String>,
    pub runtime_requested: Option<String>,
    pub runtime_resolved: Option<String>,
    pub runtime_available: bool,
    pub dependencies_resolvable: bool,
    pub environment_state: EnvironmentLifecycleState,
    pub active_processes: usize,
    pub checks: Vec<DoctorCheck>,
    #[serde(default)]
    pub connector: Option<crate::manifest::ConnectorContract>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectResolveReport {
    pub schema_version: u32,
    pub project_path: String,
    pub project_name: String,
    pub runtime_requested: String,
    pub runtime_resolved: String,
    pub package_manager: String,
    pub requirement_files: Vec<String>,
    pub environment_target_path: String,
    #[serde(default)]
    pub specification_hash: String,
    pub state: EnvironmentLifecycleState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentStatusReport {
    pub schema_version: u32,
    pub project_path: String,
    pub environment_id: String,
    pub environment_path: String,
    pub state: EnvironmentLifecycleState,
    pub is_materialized: bool,
    pub active_pids: Vec<u32>,
    #[serde(default)]
    pub leased_processes: Vec<LeasedProcess>,
    pub is_quiescent: bool,
    pub can_reclaim: bool,
    pub python_executable: Option<String>,
    pub python_version: Option<String>,
    #[serde(default)]
    pub specification_hash: Option<String>,
    #[serde(default)]
    pub connector: Option<crate::manifest::ConnectorContract>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentReclaimReport {
    pub schema_version: u32,
    pub project_path: String,
    pub environment_id: String,
    pub environment_path: String,
    pub reclaimed: bool,
    pub bytes_reclaimed: u64,
    #[serde(default)]
    pub physical_bytes_reclaimed: u64,
    #[serde(default)]
    pub volume_free_bytes_delta: Option<i64>,
    #[serde(default)]
    pub file_count_reclaimed: usize,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_state_transitions() {
        use EnvironmentLifecycleState::*;
        assert!(Defined.can_transition_to(Resolved));
        assert!(Resolved.can_transition_to(Materializing));
        assert!(Materializing.can_transition_to(Ready));
        assert!(Ready.can_transition_to(Executing));
        assert!(Executing.can_transition_to(Released));
        assert!(Released.can_transition_to(Reclaimable));
        assert!(Reclaimable.can_transition_to(Reclaimed));

        // Invalid transitions
        assert!(!Defined.can_transition_to(Executing));
        assert!(!Materializing.can_transition_to(Reclaimed));
        assert!(!Reclaimed.can_transition_to(Executing));
    }

    #[test]
    fn test_lifecycle_reclaimability() {
        assert!(EnvironmentLifecycleState::Released.is_reclaimable());
        assert!(EnvironmentLifecycleState::Reclaimable.is_reclaimable());
        assert!(EnvironmentLifecycleState::Failed.is_reclaimable());
        assert!(!EnvironmentLifecycleState::Executing.is_reclaimable());
        assert!(!EnvironmentLifecycleState::Materializing.is_reclaimable());
        assert!(!EnvironmentLifecycleState::Ready.is_reclaimable());
    }

    #[test]
    fn test_ownership_record_validation() {
        let record = EnvironmentOwnershipRecord::new(
            "env_12345".into(),
            r"E:\Workspace\Active\NexusNode\server".into(),
            r"E:\Workspace\Active\ULVO\vault".into(),
            "abcdef1234567890".into(),
            100,
            123456789,
        );

        // Success matching
        assert!(record.validate(
            r"\\?\E:\Workspace\Active\NexusNode\server",
            r"E:\Workspace\Active\ULVO\vault\",
            "env_12345",
        ).is_ok());

        // Project mismatch
        assert!(record.validate(
            r"E:\Workspace\Active\OtherProject",
            r"E:\Workspace\Active\ULVO\vault",
            "env_12345",
        ).is_err());

        // Vault mismatch
        assert!(record.validate(
            r"E:\Workspace\Active\NexusNode\server",
            r"C:\OtherVault",
            "env_12345",
        ).is_err());

        // Environment ID mismatch
        assert!(record.validate(
            r"E:\Workspace\Active\NexusNode\server",
            r"E:\Workspace\Active\ULVO\vault",
            "env_different",
        ).is_err());
    }

    #[test]
    fn test_process_liveness() {
        assert!(ProcessLiveness::Alive.is_alive());
        assert!(!ProcessLiveness::Terminated.is_alive());
        assert!(!ProcessLiveness::Unknown.is_alive());

        assert!(ProcessLiveness::Alive.blocks_reclamation());
        assert!(ProcessLiveness::Unknown.blocks_reclamation());
        assert!(!ProcessLiveness::Terminated.blocks_reclamation());
    }
}
