use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use crate::resource::{Ecosystem, Resource, ResourceId, ResourceKind, VerificationStatus};
use crate::{Backend, Ownership};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegistryError {
    DuplicateResource(ResourceId),
    ConflictingResource {
        existing: ResourceId,
        incoming: ResourceId,
        reason: String,
    },
    ResourceNotFound(ResourceId),
    InvalidQuery(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateResource(id) => write!(f, "duplicate resource: {id}"),
            Self::ConflictingResource { existing, incoming, reason } => {
                write!(f, "conflicting resource: {incoming} conflicts with {existing}: {reason}")
            }
            Self::ResourceNotFound(id) => write!(f, "resource not found: {id}"),
            Self::InvalidQuery(msg) => write!(f, "invalid registry query: {msg}"),
        }
    }
}

impl std::error::Error for RegistryError {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryFilter {
    pub ecosystem: Option<Ecosystem>,
    pub backend: Option<Backend>,
    pub kind: Option<ResourceKind>,
    pub status: Option<VerificationStatus>,
    pub name: Option<String>,
}

impl RegistryFilter {
    pub fn matches(&self, r: &Resource) -> bool {
        if let Some(ref e) = self.ecosystem {
            if &r.ecosystem != e { return false; }
        }
        if let Some(ref b) = self.backend {
            if &r.backend != b { return false; }
        }
        if let Some(ref k) = self.kind {
            if &r.resource_kind != k { return false; }
        }
        if let Some(ref s) = self.status {
            if &r.verification_status != s { return false; }
        }
        if let Some(ref n) = self.name {
            if !r.name.eq_ignore_ascii_case(n) { return false; }
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryStats {
    pub total_resources: usize,
    pub by_kind: BTreeMap<String, usize>,
    pub by_ecosystem: BTreeMap<String, usize>,
    pub by_backend: BTreeMap<String, usize>,
    pub by_status: BTreeMap<String, usize>,
    pub by_ownership: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationIssueType {
    MissingPath,
    MissingExecutable,
    ModifiedExecutable,
    VersionMismatch,
    DuplicateIdentity,
    ConflictingVersions,
    DifferentBackendOrArchitecture,
    StaleVerification,
    UnknownOwnership,
    ReparseTraversalRejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciliationIssue {
    pub resource_id: ResourceId,
    pub issue_type: ReconciliationIssueType,
    pub severity: IssueSeverity,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciliationReport {
    pub schema_version: u32,
    pub total_checked: usize,
    pub clean: bool,
    pub issues: Vec<ReconciliationIssue>,
}

pub trait VaultRegistry {
    fn register(&mut self, resource: Resource) -> Result<(), RegistryError>;
    fn get(&self, id: &ResourceId) -> Option<&Resource>;
    fn list(&self) -> Vec<&Resource>;
    fn find(&self, filter: &RegistryFilter) -> Vec<&Resource>;
    fn stats(&self) -> RegistryStats;
}

use crate::adoption::ToolchainAdoptionPlan;
use crate::gpu::SystemGpuDriver;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryVaultRegistry {
    pub schema_version: u32,
    pub resources: BTreeMap<ResourceId, Resource>,
    #[serde(default)]
    pub adoption_plans: BTreeMap<String, ToolchainAdoptionPlan>,
    #[serde(default)]
    pub host_gpu: Option<SystemGpuDriver>,
}

impl MemoryVaultRegistry {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            resources: BTreeMap::new(),
            adoption_plans: BTreeMap::new(),
            host_gpu: None,
        }
    }

    pub fn add_adoption_plan(&mut self, plan: ToolchainAdoptionPlan) {
        self.adoption_plans.insert(plan.plan_id.clone(), plan);
    }

    pub fn get_adoption_plan(&self, plan_id: &str) -> Option<&ToolchainAdoptionPlan> {
        self.adoption_plans.get(plan_id)
    }

    pub fn set_host_gpu(&mut self, driver: SystemGpuDriver) {
        self.host_gpu = Some(driver);
    }

    pub fn get_by_str(&self, identifier: &str) -> Option<&Resource> {
        let key = ResourceId(identifier.to_string());
        if let Some(r) = self.resources.get(&key) {
            return Some(r);
        }
        // Case-insensitive exact ID match
        if let Some(r) = self.resources.values().find(|r| r.resource_id.0.eq_ignore_ascii_case(identifier)) {
            return Some(r);
        }
        // Substring / prefix search if unambiguous
        let matches: Vec<_> = self.resources.values().filter(|r| {
            r.resource_id.0.contains(identifier) || r.name.eq_ignore_ascii_case(identifier)
        }).collect();
        if matches.len() == 1 {
            Some(matches[0])
        } else {
            None
        }
    }

    pub fn reconcile_with<F>(&self, mut check_fn: F) -> ReconciliationReport
    where
        F: FnMut(&Resource) -> Vec<ReconciliationIssue>,
    {
        let mut issues = Vec::new();
        for resource in self.resources.values() {
            // Check semantic model consistency
            if resource.ownership == Ownership::Unknown {
                issues.push(ReconciliationIssue {
                    resource_id: resource.resource_id.clone(),
                    issue_type: ReconciliationIssueType::UnknownOwnership,
                    severity: IssueSeverity::Warning,
                    description: format!("Resource '{}' has unknown ownership", resource.name),
                });
            }
            if resource.verification_status == VerificationStatus::Unverified {
                issues.push(ReconciliationIssue {
                    resource_id: resource.resource_id.clone(),
                    issue_type: ReconciliationIssueType::StaleVerification,
                    severity: IssueSeverity::Warning,
                    description: format!("Resource '{}' has not been verified", resource.name),
                });
            }
            // Delegate filesystem/target checks to closure
            let extra = check_fn(resource);
            issues.extend(extra);
        }
        let clean = issues.iter().all(|i| i.severity == IssueSeverity::Warning);
        ReconciliationReport {
            schema_version: 1,
            total_checked: self.resources.len(),
            clean,
            issues,
        }
    }
}

impl VaultRegistry for MemoryVaultRegistry {
    fn register(&mut self, resource: Resource) -> Result<(), RegistryError> {
        if let Some(existing) = self.resources.get(&resource.resource_id) {
            if existing == &resource {
                // Idempotent duplicate: exact match is safe
                return Ok(());
            }
            return Err(RegistryError::ConflictingResource {
                existing: existing.resource_id.clone(),
                incoming: resource.resource_id.clone(),
                reason: "A resource with the same identifier already exists with different attributes".into(),
            });
        }

        // Detect conflicting versions for same installation path and name
        if let Some(ref path) = resource.installation_path {
            for existing in self.resources.values() {
                if existing.installation_path.as_ref() == Some(path)
                    && existing.name == resource.name
                    && existing.backend == resource.backend
                    && existing.version != resource.version
                {
                    return Err(RegistryError::ConflictingResource {
                        existing: existing.resource_id.clone(),
                        incoming: resource.resource_id.clone(),
                        reason: format!(
                            "Conflicting versions observed at '{}': existing {:?}, incoming {:?}",
                            path, existing.version, resource.version
                        ),
                    });
                }
            }
        }

        self.resources.insert(resource.resource_id.clone(), resource);
        Ok(())
    }

    fn get(&self, id: &ResourceId) -> Option<&Resource> {
        self.resources.get(id)
    }

    fn list(&self) -> Vec<&Resource> {
        self.resources.values().collect()
    }

    fn find(&self, filter: &RegistryFilter) -> Vec<&Resource> {
        self.resources.values().filter(|r| filter.matches(r)).collect()
    }

    fn stats(&self) -> RegistryStats {
        let mut by_kind = BTreeMap::new();
        let mut by_ecosystem = BTreeMap::new();
        let mut by_backend = BTreeMap::new();
        let mut by_status = BTreeMap::new();
        let mut by_ownership = BTreeMap::new();

        for r in self.resources.values() {
            *by_kind.entry(format!("{:?}", r.resource_kind).to_lowercase()).or_insert(0) += 1;
            *by_ecosystem.entry(format!("{:?}", r.ecosystem).to_lowercase()).or_insert(0) += 1;
            *by_backend.entry(format!("{:?}", r.backend).to_lowercase()).or_insert(0) += 1;
            *by_status.entry(format!("{:?}", r.verification_status).to_lowercase()).or_insert(0) += 1;
            *by_ownership.entry(format!("{:?}", r.ownership).to_lowercase()).or_insert(0) += 1;
        }

        RegistryStats {
            total_resources: self.resources.len(),
            by_kind,
            by_ecosystem,
            by_backend,
            by_status,
            by_ownership,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::{Abi, Architecture, Platform};

    fn sample_resource(name: &str, version: &str, backend: Backend) -> Resource {
        Resource::new(
            ResourceKind::Compiler,
            Ecosystem::Rust,
            name,
            Some(version.to_string()),
            Platform::Windows,
            Architecture::X86_64,
            backend,
            Abi::Gnu,
            Some(format!("E:/vault/bin/{name}.exe")),
            Some("rustup".into()),
            Ownership::Managed,
            VerificationStatus::Discovered,
            None,
            vec!["compiler".into()],
        )
    }

    #[test]
    fn register_and_list() {
        let mut reg = MemoryVaultRegistry::new();
        let r1 = sample_resource("rustc", "1.96.0", Backend::Windows);
        let r2 = sample_resource("cargo", "1.96.0", Backend::Windows);
        assert!(reg.register(r1.clone()).is_ok());
        assert!(reg.register(r2.clone()).is_ok());

        assert_eq!(reg.list().len(), 2);
        let found = reg.get(&r1.resource_id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "rustc");
    }

    #[test]
    fn register_idempotent_duplicate() {
        let mut reg = MemoryVaultRegistry::new();
        let r = sample_resource("rustc", "1.96.0", Backend::Windows);
        assert!(reg.register(r.clone()).is_ok());
        assert!(reg.register(r).is_ok(), "Identical resource registration must be idempotent");
        assert_eq!(reg.list().len(), 1);
    }

    #[test]
    fn register_conflicting_attributes_rejected() {
        let mut reg = MemoryVaultRegistry::new();
        let r1 = sample_resource("rustc", "1.96.0", Backend::Windows);
        let mut r2 = r1.clone();
        r2.capabilities.push("unexpected".into()); // modified attributes with same ID
        assert!(reg.register(r1).is_ok());
        let res = reg.register(r2);
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(), RegistryError::ConflictingResource { .. }));
    }

    #[test]
    fn filter_by_ecosystem_and_backend() {
        let mut reg = MemoryVaultRegistry::new();
        let r_win = sample_resource("rustc", "1.96.0", Backend::Windows);
        let mut r_wsl = sample_resource("rustc", "1.96.0", Backend::Wsl);
        r_wsl.platform = Platform::Linux;
        r_wsl.installation_path = Some("/usr/bin/rustc".into());
        r_wsl.resource_id = ResourceId::new(
            &r_wsl.ecosystem,
            &r_wsl.backend,
            &r_wsl.platform,
            &r_wsl.architecture,
            &r_wsl.abi,
            &r_wsl.resource_kind,
            &r_wsl.name,
            r_wsl.version.as_deref(),
            r_wsl.installation_path.as_deref(),
        );

        reg.register(r_win).unwrap();
        reg.register(r_wsl).unwrap();

        let filter_win = RegistryFilter {
            backend: Some(Backend::Windows),
            ..Default::default()
        };
        let matches = reg.find(&filter_win);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].backend, Backend::Windows);

        let filter_wsl = RegistryFilter {
            backend: Some(Backend::Wsl),
            ..Default::default()
        };
        let matches_wsl = reg.find(&filter_wsl);
        assert_eq!(matches_wsl.len(), 1);
        assert_eq!(matches_wsl[0].backend, Backend::Wsl);
    }

    #[test]
    fn stats_calculation() {
        let mut reg = MemoryVaultRegistry::new();
        reg.register(sample_resource("rustc", "1.96.0", Backend::Windows)).unwrap();
        reg.register(sample_resource("cargo", "1.96.0", Backend::Windows)).unwrap();

        let stats = reg.stats();
        assert_eq!(stats.total_resources, 2);
        assert_eq!(stats.by_ecosystem.get("rust"), Some(&2));
        assert_eq!(stats.by_backend.get("windows"), Some(&2));
        assert_eq!(stats.by_kind.get("compiler"), Some(&2));
    }

    #[test]
    fn conflicting_versions_at_same_path_rejected() {
        let mut reg = MemoryVaultRegistry::new();
        let r1 = sample_resource("rustc", "1.96.0", Backend::Windows);
        let mut r2 = sample_resource("rustc", "1.97.0", Backend::Windows);
        // Point r2 to the same installation path as r1
        r2.installation_path = r1.installation_path.clone();
        // Even if ID was recalculated with 1.97.0, registry must detect contradictory version at same path!
        reg.register(r1).unwrap();
        let res = reg.register(r2);
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(), RegistryError::ConflictingResource { .. }));
    }

    #[test]
    fn reconciliation_detects_unknown_ownership_and_stale_verification() {
        let mut reg = MemoryVaultRegistry::new();
        let mut r = sample_resource("rustc", "1.96.0", Backend::Windows);
        r.ownership = Ownership::Unknown;
        r.verification_status = VerificationStatus::Unverified;
        reg.register(r).unwrap();

        let report = reg.reconcile_with(|_| Vec::new());
        assert_eq!(report.total_checked, 1);
        assert!(report.issues.iter().any(|i| i.issue_type == ReconciliationIssueType::UnknownOwnership));
        assert!(report.issues.iter().any(|i| i.issue_type == ReconciliationIssueType::StaleVerification));
    }

    #[test]
    fn get_by_str_unambiguous_lookup() {
        let mut reg = MemoryVaultRegistry::new();
        let r = sample_resource("rustc", "1.96.0", Backend::Windows);
        let full_id = r.resource_id.0.clone();
        reg.register(r).unwrap();

        assert!(reg.get_by_str(&full_id).is_some());
        assert!(reg.get_by_str("rustc").is_some());
        assert!(reg.get_by_str("nonexistent").is_none());
    }

    #[test]
    fn test_registry_adoption_plan_and_gpu_driver() {
        let mut reg = MemoryVaultRegistry::new();
        let plan = ToolchainAdoptionPlan::new(
            "rustc",
            Ecosystem::Rust,
            "C:\\cargo\\bin\\rustc.exe",
            "vault\\runtimes\\rust\\bin\\rustc.exe",
            "rustup",
            Some("1.96.0".into()),
            Architecture::X86_64,
            Abi::Gnu,
            crate::adoption::AdoptionStrategy::ReferenceExternal,
        );
        reg.add_adoption_plan(plan);
        assert!(reg.get_adoption_plan("plan_rustc").is_some());

        let driver = SystemGpuDriver::new(
            crate::gpu::NvidiaDriverVersion::parse("596.49").unwrap(),
            vec![],
        );
        reg.set_host_gpu(driver);
        assert!(reg.host_gpu.is_some());
        assert!(reg.host_gpu.as_ref().unwrap().is_system_managed);
    }
}
