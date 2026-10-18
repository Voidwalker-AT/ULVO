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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentLease {
    pub lease_id: String,
    pub environment_id: String,
    pub project_path: String,
    pub environment_path: String,
    pub state: EnvironmentLifecycleState,
    pub active_pids: Vec<u32>,
    pub created_at_unix: u64,
    pub updated_at_unix: u64,
    pub lease_timeout_seconds: u64,
    pub owner_token: String,
    pub python_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentOwnershipRecord {
    pub sentinel: String,
    pub environment_id: String,
    pub project_canonical_path: String,
    pub created_by_pid: u32,
    pub created_at_unix: u64,
    pub vault_root: String,
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
    pub is_quiescent: bool,
    pub can_reclaim: bool,
    pub python_executable: Option<String>,
    pub python_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentReclaimReport {
    pub schema_version: u32,
    pub project_path: String,
    pub environment_id: String,
    pub environment_path: String,
    pub reclaimed: bool,
    pub bytes_reclaimed: u64,
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
}
