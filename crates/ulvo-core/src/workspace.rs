use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationCompatibility {
    ReadyForPilot,
    LikelyCompatible,
    RequiresPersistentEnv,
    IncompleteMetadata,
    HighRiskMigration,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    pub is_editable: bool,
    pub has_native: bool,
    pub logical_bytes: u64,
    pub unique_files: usize,
    #[serde(default)]
    pub content_hash: Option<String>,
    #[serde(default)]
    pub abi_tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PythonEnvironment {
    pub path: String,
    pub relative_path: String,
    pub category: String,
    pub env_type: String, // "venv" or "conda"
    pub python_version: Option<String>,
    pub base_interpreter: Option<String>,
    pub logical_bytes: u64,
    pub physical_allocated_bytes: Option<u64>,
    pub physical_unique_files: usize,
    pub has_editable: bool,
    pub has_native: bool,
    pub packages: Vec<InstalledPackage>,
    pub inaccessible_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageDuplication {
    pub package: String,
    pub version: String,
    pub instance_count: usize,
    pub estimated_unit_bytes: u64,
    pub total_wasted_bytes: u64,
    pub has_native: bool,
    pub is_verified_identical: bool,
    pub abi_tags: Vec<String>,
    pub environments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageSavingsEstimate {
    pub recoverable_bytes: u64,
    pub recoverable_gb: f64,
    pub percentage_of_workspace: f64,
    pub method: String,
    pub risk: String,
    pub confidence: String,
    #[serde(default)]
    pub assumptions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageSavingsReport {
    pub conservative: StorageSavingsEstimate,
    pub realistic: StorageSavingsEstimate,
    pub theoretical: StorageSavingsEstimate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCandidate {
    pub path: String,
    pub name: String,
    pub category: String,
    pub sentinels: Vec<String>,
    pub compatibility: MigrationCompatibility,
    pub risk_factors: Vec<String>,
    pub estimated_env_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoryStats {
    pub logical_bytes: u64,
    pub logical_gb: f64,
    pub physical_allocated_bytes: Option<u64>,
    pub physical_allocated_gb: Option<f64>,
    pub file_count: usize,
    pub unique_physical_files: usize,
    pub inaccessible_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceAuditReport {
    pub schema_version: u32,
    pub workspace_root: String,
    pub total_logical_bytes: u64,
    pub total_logical_gb: f64,
    pub total_physical_allocated_bytes: Option<u64>,
    pub total_physical_allocated_gb: Option<f64>,
    pub total_files: usize,
    pub unique_physical_files: usize,
    pub hardlink_count: usize,
    pub inaccessible_count: usize,
    pub scan_incomplete: bool,
    pub category_breakdown: BTreeMap<String, CategoryStats>,
    pub python_environments: Vec<PythonEnvironment>,
    pub package_duplications: Vec<PackageDuplication>,
    pub storage_savings: StorageSavingsReport,
    pub project_candidates: Vec<ProjectCandidate>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_enum_variants() {
        let compat = MigrationCompatibility::ReadyForPilot;
        assert_eq!(compat, MigrationCompatibility::ReadyForPilot);
    }

    #[test]
    fn package_duplication_sorting() {
        let mut dups = vec![
            PackageDuplication {
                package: "numpy".into(),
                version: "2.2.0".into(),
                instance_count: 2,
                estimated_unit_bytes: 50_000_000,
                total_wasted_bytes: 50_000_000,
                has_native: true,
                is_verified_identical: true,
                abi_tags: vec!["cp311-cp311-win_amd64".into()],
                environments: vec!["env1".into(), "env2".into()],
            },
            PackageDuplication {
                package: "torch".into(),
                version: "2.6.0".into(),
                instance_count: 3,
                estimated_unit_bytes: 4_000_000_000,
                total_wasted_bytes: 8_000_000_000,
                has_native: true,
                is_verified_identical: false,
                abi_tags: vec!["cp311-cp311-win_amd64".into(), "cp310-cp310-win_amd64".into()],
                environments: vec!["env1".into(), "env2".into(), "env3".into()],
            },
        ];
        dups.sort_by_key(|d| std::cmp::Reverse(d.total_wasted_bytes));
        assert_eq!(dups[0].package, "torch");
    }
}
