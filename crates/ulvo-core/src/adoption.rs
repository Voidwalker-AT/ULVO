use serde::{Deserialize, Serialize};
use crate::resource::{Abi, Architecture, Ecosystem};
use crate::Ownership;

/// The 9 formal lifecycle states of a toolchain adoption plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdoptionState {
    ExternalDiscovered,
    CompatibilityAssessed,
    AdoptionPlanned,
    Staging,
    Verified,
    Managed,
    ExternalRetained,
    Blocked,
    Failed,
}

impl std::fmt::Display for AdoptionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExternalDiscovered => write!(f, "EXTERNAL_DISCOVERED"),
            Self::CompatibilityAssessed => write!(f, "COMPATIBILITY_ASSESSED"),
            Self::AdoptionPlanned => write!(f, "ADOPTION_PLANNED"),
            Self::Staging => write!(f, "STAGING"),
            Self::Verified => write!(f, "VERIFIED"),
            Self::Managed => write!(f, "MANAGED"),
            Self::ExternalRetained => write!(f, "EXTERNAL_RETAINED"),
            Self::Blocked => write!(f, "BLOCKED"),
            Self::Failed => write!(f, "FAILED"),
        }
    }
}

/// Strategy for adopting an external toolchain into the ULVO vault.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdoptionStrategy {
    /// Reference external installation in place without moving or copying.
    ReferenceExternal,
    /// Reinstall cleanly through the native ecosystem manager into the ULVO vault.
    ReinstallNative,
    /// Copy files into vault ONLY if relocatability is proven (no embedded absolute paths).
    CopyRelocatable,
    /// Reuse existing package caches when supported by the native manager.
    ReuseSharedCache,
    /// Preserve system-managed dependencies externally (e.g., NVIDIA kernel display drivers).
    PreserveSystemManaged,
}

/// Detailed rollback instructions for a planned or in-progress adoption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackPlan {
    pub steps: Vec<String>,
    pub cleanup_paths: Vec<String>,
    pub restore_environment_vars: Vec<(String, Option<String>)>,
    pub can_rollback_automatically: bool,
}

impl RollbackPlan {
    pub fn empty() -> Self {
        Self {
            steps: Vec::new(),
            cleanup_paths: Vec::new(),
            restore_environment_vars: Vec::new(),
            can_rollback_automatically: true,
        }
    }
}

/// A comprehensive typed adoption plan for bringing an external toolchain under ULVO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolchainAdoptionPlan {
    pub plan_id: String,
    pub tool_id: String,
    pub ecosystem: Ecosystem,
    pub original_location: String,
    pub intended_ulvo_location: String,
    pub ecosystem_manager: String,
    pub version: Option<String>,
    pub architecture: Architecture,
    pub abi: Abi,
    pub dependencies: Vec<String>,
    pub required_disk_bytes: u64,
    pub verification_commands: Vec<String>,
    pub rollback_plan: RollbackPlan,
    pub ownership_classification: Ownership,
    pub expected_global_state_effects: Vec<String>,
    pub state: AdoptionState,
    pub state_reason: Option<String>,
    pub strategy: AdoptionStrategy,
    pub is_relocatable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdoptionError {
    InvalidStateTransition {
        current: AdoptionState,
        requested: AdoptionState,
        reason: String,
    },
    RelocationRejected {
        reason: String,
    },
    MissingVerificationCommands,
    InsufficientDiskSpace {
        required: u64,
        available: u64,
    },
}

impl std::fmt::Display for AdoptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidStateTransition { current, requested, reason } => {
                write!(f, "invalid state transition from {current} to {requested}: {reason}")
            }
            Self::RelocationRejected { reason } => {
                write!(f, "toolchain relocation rejected: {reason}")
            }
            Self::MissingVerificationCommands => {
                write!(f, "plan must provide verification commands prior to staging")
            }
            Self::InsufficientDiskSpace { required, available } => {
                write!(f, "insufficient disk space for adoption: required {required} bytes, available {available} bytes")
            }
        }
    }
}

impl std::error::Error for AdoptionError {}

impl ToolchainAdoptionPlan {
    pub fn new(
        tool_id: impl Into<String>,
        ecosystem: Ecosystem,
        original_location: impl Into<String>,
        intended_ulvo_location: impl Into<String>,
        ecosystem_manager: impl Into<String>,
        version: Option<String>,
        architecture: Architecture,
        abi: Abi,
        strategy: AdoptionStrategy,
    ) -> Self {
        let tool_id = tool_id.into();
        let plan_id = format!("plan_{}", tool_id.to_lowercase().replace(' ', "_"));
        Self {
            plan_id,
            tool_id,
            ecosystem,
            original_location: original_location.into(),
            intended_ulvo_location: intended_ulvo_location.into(),
            ecosystem_manager: ecosystem_manager.into(),
            version,
            architecture,
            abi,
            dependencies: Vec::new(),
            required_disk_bytes: 0,
            verification_commands: Vec::new(),
            rollback_plan: RollbackPlan::empty(),
            ownership_classification: Ownership::External,
            expected_global_state_effects: Vec::new(),
            state: AdoptionState::ExternalDiscovered,
            state_reason: None,
            strategy,
            is_relocatable: false,
        }
    }

    /// Enforces state transition invariants.
    /// Invariant: No direct filesystem move is permitted merely because a directory exists.
    pub fn transition_to(&mut self, next: AdoptionState, reason: Option<String>) -> Result<(), AdoptionError> {
        match (self.state, next) {
            // Normal forward transitions
            (AdoptionState::ExternalDiscovered, AdoptionState::CompatibilityAssessed) => {
                self.state = next;
                self.state_reason = reason;
                Ok(())
            }
            (AdoptionState::CompatibilityAssessed, AdoptionState::AdoptionPlanned) => {
                self.state = next;
                self.state_reason = reason;
                Ok(())
            }
            (AdoptionState::AdoptionPlanned, AdoptionState::Staging) => {
                // Must have verification commands before staging
                if self.verification_commands.is_empty() {
                    return Err(AdoptionError::MissingVerificationCommands);
                }
                // If strategy is CopyRelocatable, relocatability MUST be proven
                if self.strategy == AdoptionStrategy::CopyRelocatable && !self.is_relocatable {
                    return Err(AdoptionError::RelocationRejected {
                        reason: "Toolchain relocation rejected: binary or scripts contain embedded absolute paths or lack relocatability proof".into(),
                    });
                }
                self.state = next;
                self.state_reason = reason;
                Ok(())
            }
            (AdoptionState::Staging, AdoptionState::Verified) => {
                self.state = next;
                self.state_reason = reason;
                Ok(())
            }
            (AdoptionState::Verified, AdoptionState::Managed) => {
                self.ownership_classification = Ownership::Managed;
                self.state = next;
                self.state_reason = reason;
                Ok(())
            }
            // Branch to ExternalRetained
            (AdoptionState::CompatibilityAssessed, AdoptionState::ExternalRetained)
            | (AdoptionState::AdoptionPlanned, AdoptionState::ExternalRetained)
            | (AdoptionState::ExternalDiscovered, AdoptionState::ExternalRetained) => {
                self.ownership_classification = Ownership::External;
                self.state = next;
                self.state_reason = reason;
                Ok(())
            }
            // Transition to Blocked
            (_, AdoptionState::Blocked) => {
                self.state = next;
                self.state_reason = reason;
                Ok(())
            }
            // Transition to Failed
            (_, AdoptionState::Failed) => {
                self.state = next;
                self.state_reason = reason;
                Ok(())
            }
            // Direct move attempts from discovered directly to Managed without staging/verification are strictly rejected!
            (AdoptionState::ExternalDiscovered, AdoptionState::Managed)
            | (AdoptionState::CompatibilityAssessed, AdoptionState::Managed)
            | (AdoptionState::AdoptionPlanned, AdoptionState::Managed) => {
                Err(AdoptionError::InvalidStateTransition {
                    current: self.state,
                    requested: next,
                    reason: "Cannot jump directly to MANAGED without STAGING and VERIFIED execution gates".into(),
                })
            }
            _ => Err(AdoptionError::InvalidStateTransition {
                current: self.state,
                requested: next,
                reason: format!("Illegal transition from {} to {}", self.state, next),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_adoption_lifecycle() {
        let mut plan = ToolchainAdoptionPlan::new(
            "rust-nightly",
            Ecosystem::Rust,
            "C:\\Users\\hp\\.cargo\\bin\\rustc.exe",
            "E:\\Workspace\\Active\\ULVO\\vault\\runtimes\\rust\\bin\\rustc.exe",
            "rustup",
            Some("1.85.0-nightly".into()),
            Architecture::X86_64,
            Abi::Gnu,
            AdoptionStrategy::ReinstallNative,
        );

        assert_eq!(plan.state, AdoptionState::ExternalDiscovered);
        assert_eq!(plan.ownership_classification, Ownership::External);

        plan.transition_to(AdoptionState::CompatibilityAssessed, None).unwrap();
        assert_eq!(plan.state, AdoptionState::CompatibilityAssessed);

        plan.transition_to(AdoptionState::AdoptionPlanned, None).unwrap();
        assert_eq!(plan.state, AdoptionState::AdoptionPlanned);

        // Transition to Staging without verification commands fails
        assert!(plan.transition_to(AdoptionState::Staging, None).is_err());

        plan.verification_commands = vec!["rustc --version".into(), "cargo --version".into()];
        plan.transition_to(AdoptionState::Staging, None).unwrap();
        assert_eq!(plan.state, AdoptionState::Staging);

        plan.transition_to(AdoptionState::Verified, None).unwrap();
        assert_eq!(plan.state, AdoptionState::Verified);

        plan.transition_to(AdoptionState::Managed, None).unwrap();
        assert_eq!(plan.state, AdoptionState::Managed);
        assert_eq!(plan.ownership_classification, Ownership::Managed);
    }

    #[test]
    fn test_relocation_rejected_if_not_proven() {
        let mut plan = ToolchainAdoptionPlan::new(
            "go-toolchain",
            Ecosystem::Go,
            "C:\\Program Files\\Go",
            "E:\\Workspace\\Active\\ULVO\\vault\\runtimes\\go",
            "go",
            Some("1.23.0".into()),
            Architecture::X86_64,
            Abi::None,
            AdoptionStrategy::CopyRelocatable,
        );
        plan.verification_commands = vec!["go version".into()];
        plan.transition_to(AdoptionState::CompatibilityAssessed, None).unwrap();
        plan.transition_to(AdoptionState::AdoptionPlanned, None).unwrap();

        // is_relocatable is false -> CopyRelocatable MUST fail!
        let err = plan.transition_to(AdoptionState::Staging, None).unwrap_err();
        assert!(matches!(err, AdoptionError::RelocationRejected { .. }));

        // Only when proven relocatable does it succeed
        plan.is_relocatable = true;
        plan.transition_to(AdoptionState::Staging, None).unwrap();
        assert_eq!(plan.state, AdoptionState::Staging);
    }

    #[test]
    fn test_external_retained_transition() {
        let mut plan = ToolchainAdoptionPlan::new(
            "nvidia-display-driver",
            Ecosystem::Generic("cuda".into()),
            "C:\\Windows\\System32\\DriverStore",
            "N/A (System Driver)",
            "nvidia-installer",
            Some("596.49".into()),
            Architecture::X86_64,
            Abi::None,
            AdoptionStrategy::PreserveSystemManaged,
        );

        plan.transition_to(AdoptionState::CompatibilityAssessed, None).unwrap();
        plan.transition_to(AdoptionState::ExternalRetained, Some("Kernel display driver must remain system-managed".into())).unwrap();
        assert_eq!(plan.state, AdoptionState::ExternalRetained);
        assert_eq!(plan.ownership_classification, Ownership::External);
    }

    #[test]
    fn test_cannot_skip_staging_to_managed() {
        let mut plan = ToolchainAdoptionPlan::new(
            "python-313",
            Ecosystem::Python,
            "C:\\Python313",
            "E:\\Workspace\\Active\\ULVO\\vault\\runtimes\\python",
            "installer",
            Some("3.13.0".into()),
            Architecture::X86_64,
            Abi::None,
            AdoptionStrategy::ReinstallNative,
        );

        let err = plan.transition_to(AdoptionState::Managed, None).unwrap_err();
        assert!(matches!(err, AdoptionError::InvalidStateTransition { .. }));
    }
}
