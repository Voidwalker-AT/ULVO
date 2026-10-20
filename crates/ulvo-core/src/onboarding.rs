use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingPolicy {
    Manual,
    Prompt,
    AutoForTrustedWorkspaces,
}

impl Default for OnboardingPolicy {
    fn default() -> Self {
        OnboardingPolicy::Prompt
    }
}

impl std::str::FromStr for OnboardingPolicy {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().replace('-', "_").as_str() {
            "manual" => Ok(OnboardingPolicy::Manual),
            "prompt" => Ok(OnboardingPolicy::Prompt),
            "auto" | "auto_for_trusted_workspaces" => Ok(OnboardingPolicy::AutoForTrustedWorkspaces),
            other => Err(format!("unknown onboarding policy '{other}'; valid: manual, prompt, auto")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EcosystemKind {
    Python,
    Node,
    Rust,
    Go,
    Ruby,
    Java,
    Cpp,
}

impl EcosystemKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EcosystemKind::Python => "python",
            EcosystemKind::Node => "node",
            EcosystemKind::Rust => "rust",
            EcosystemKind::Go => "go",
            EcosystemKind::Ruby => "ruby",
            EcosystemKind::Java => "java",
            EcosystemKind::Cpp => "cpp",
        }
    }

    pub fn default_runtime_version(&self) -> &'static str {
        match self {
            EcosystemKind::Python => "3.13",
            EcosystemKind::Node => "24.15",
            EcosystemKind::Rust => "1.96.0",
            EcosystemKind::Go => "1.26.5",
            EcosystemKind::Ruby => "4.0.1",
            EcosystemKind::Java => "21",
            EcosystemKind::Cpp => "gcc-14.2.0",
        }
    }
}

impl std::fmt::Display for EcosystemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedEcosystem {
    pub kind: EcosystemKind,
    pub relative_path: String,
    pub manifest_files: Vec<String>,
    pub recommended_runtime_version: String,
    #[serde(default)]
    pub details: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectOnboardingReport {
    pub project_path: String,
    pub project_name: String,
    pub is_monorepo: bool,
    pub subprojects: Vec<String>,
    pub detected_ecosystems: Vec<DetectedEcosystem>,
    pub has_existing_connector: bool,
    pub policy: OnboardingPolicy,
    pub is_trusted_workspace: bool,
    pub action_verdict: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnboardResult {
    pub project_path: String,
    pub project_name: String,
    pub manifest_path: String,
    pub policy_applied: OnboardingPolicy,
    pub ecosystems: Vec<String>,
    pub is_monorepo: bool,
    pub thin_connector_compliant: bool,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRegistrationState {
    Discovered,
    Registered,
    Managed,
    Active,
    Blocked,
    Missing,
}

impl ProjectRegistrationState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProjectRegistrationState::Discovered => "discovered",
            ProjectRegistrationState::Registered => "registered",
            ProjectRegistrationState::Managed => "managed",
            ProjectRegistrationState::Active => "active",
            ProjectRegistrationState::Blocked => "blocked",
            ProjectRegistrationState::Missing => "missing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoverySource {
    WorkspaceScan,
    ExplicitRegistration,
    CliOnboard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredProject {
    pub id: String,
    pub name: String,
    pub canonical_path: String,
    pub ecosystem: String,
    pub manifests: Vec<String>,
    pub manifest_fingerprint: String,
    pub discovery_source: DiscoverySource,
    pub registration_state: ProjectRegistrationState,
    pub has_connector: bool,
    pub is_monorepo: bool,
    pub subprojects: Vec<String>,
    pub runtime_details: Option<String>,
    pub last_seen_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRegistryFile {
    pub schema_version: u32,
    pub last_updated_utc: String,
    pub workspace_root: String,
    pub projects: Vec<RegisteredProject>,
    pub inaccessible_paths: Vec<String>,
}
