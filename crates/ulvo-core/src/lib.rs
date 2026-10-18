use serde::{Deserialize, Serialize};

pub mod resource;
pub mod registry;
pub mod workspace;
pub mod lifecycle;

pub use resource::*;
pub use registry::*;
pub use workspace::*;
pub use lifecycle::*;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend { Windows, Wsl, Msys2 }
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ownership { External, Experimental, Managed, Unknown }
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verification { Configured, Present, Verified, Failed, Unsupported }
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Toolchain {
    pub tool: String,
    pub version: Option<String>,
    pub executable: String,
    pub file_identity: Option<String>,
    pub backend: Backend,
    pub distribution: Option<String>,
    pub architecture: Option<String>,
    pub vendor: Option<String>,
    pub sources: Vec<String>,
    pub ownership: Ownership,
    pub verification: Verification,
    pub capabilities: Vec<String>,
    pub diagnostic: Option<String>,
    /// Original observations; snapshots have empty evidence to avoid recursive nesting.
    #[serde(default)]
    pub evidence: Vec<Toolchain>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory {
    pub schema_version: u32,
    pub host_backend: Backend,
    pub toolchains: Vec<Toolchain>,
    pub diagnostics: Vec<String>,
}

pub fn parse_version(text: &str) -> Option<String> {
    text.split_whitespace().find_map(|word| {
        let s = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.');
        let s = s.strip_prefix('v').unwrap_or(s);
        let first = s.split('.').next()?;
        if !first.is_empty() && first.chars().all(|c| c.is_ascii_digit())
            && s.contains('.') && s.chars().any(|c| c.is_ascii_digit()) {
            Some(s.to_string())
        } else { None }
    })
}

fn same_identity(a: &Toolchain, b: &Toolchain) -> bool {
    a.backend == b.backend && a.distribution == b.distribution
        && (a.verification == Verification::Configured) == (b.verification == Verification::Configured)
        && (a.executable == b.executable ||
            (a.file_identity.is_some() && a.file_identity == b.file_identity))
}

fn quality(r: &Toolchain) -> (u8, bool) {
    (match r.verification {
        Verification::Verified => 4, Verification::Present => 3,
        Verification::Failed => 2, Verification::Unsupported => 1,
        Verification::Configured => 0,
    }, r.version.is_some())
}

pub fn deduplicate(records: Vec<Toolchain>) -> Vec<Toolchain> {
    let mut pending = Vec::new();
    for mut r in records {
        if r.evidence.is_empty() { pending.push(r); }
        else { pending.append(&mut r.evidence); }
    }
    pending.sort();
    let mut result = Vec::new();
    while let Some(first) = pending.pop() {
        let mut group = vec![first];
        loop {
            let Some(index) = pending.iter().position(|r| group.iter().any(|g| same_identity(g, r))) else { break };
            group.push(pending.remove(index));
        }
        for r in &mut group {
            r.evidence.clear(); r.sources.sort(); r.sources.dedup();
            r.capabilities.sort(); r.capabilities.dedup();
        }
        group.sort(); group.dedup();
        let mut summary = group.iter().min_by_key(|r| (std::cmp::Reverse(quality(r)), (*r).clone())).unwrap().clone();
        summary.sources = group.iter().flat_map(|r| r.sources.clone()).collect();
        summary.capabilities = group.iter().flat_map(|r| r.capabilities.clone()).collect();
        summary.sources.sort(); summary.sources.dedup();
        summary.capabilities.sort(); summary.capabilities.dedup();
        let mut versions: Vec<_> = group.iter().filter_map(|r| r.version.clone()).collect();
        versions.sort(); versions.dedup();
        if versions.len() > 1 {
            summary.diagnostic = Some(format!("Conflicting observed versions: {}; see evidence", versions.join(", ")));
        }
        summary.evidence = group;
        result.push(summary);
    }
    result.sort();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn versions() {
        for (text, expected) in [("Python 3.12.15", "3.12.15"),
            ("v22.23.3", "22.23.3"), ("openjdk version \"21.0.2\"", "21.0.2"),
            ("rustc 1.96.0 (hash)", "1.96.0")] {
            assert_eq!(parse_version(text).as_deref(), Some(expected));
        }
        assert_eq!(parse_version("not a version"), None);
    }
    #[test] fn ownership_categories() {
        let values = [Ownership::External, Ownership::Experimental, Ownership::Managed, Ownership::Unknown];
        assert_eq!(values.len(), 4);
    }
    fn record(path: &str, status: Verification, version: Option<&str>) -> Toolchain {
        Toolchain { tool: "python".into(), version: version.map(str::to_string), executable: path.into(),
            file_identity: Some("1:2".into()), backend: Backend::Wsl, distribution: Some("Ubuntu".into()),
            architecture: None, vendor: None, sources: vec![path.into()], ownership: Ownership::Unknown,
            verification: status, capabilities: vec![path.into()], diagnostic: None, evidence: vec![] }
    }
    #[test] fn merge_retains_versions_status_and_capabilities() {
        let a = record("/a", Verification::Present, None);
        let b = record("/b", Verification::Verified, Some("3.12"));
        let mut c = record("/c", Verification::Failed, Some("3.13"));
        c.diagnostic = Some("failed probe".into());
        let merged = deduplicate(vec![a.clone(), b.clone(), c.clone()]);
        assert_eq!(merged, deduplicate(vec![c, b, a]));
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].verification, Verification::Verified);
        assert_eq!(merged[0].version.as_deref(), Some("3.12"));
        assert_eq!(merged[0].evidence.len(), 3);
        assert_eq!(merged[0].sources.len(), 3);
        assert_eq!(merged[0].capabilities.len(), 3);
        assert!(merged[0].diagnostic.as_ref().unwrap().contains("3.13"));
        assert!(merged[0].evidence.iter().any(|r| r.diagnostic.as_deref() == Some("failed probe")));
        assert_eq!(merged, deduplicate(merged.clone()));
    }
    #[test] fn backend_distribution_and_configured_stay_separate() {
        let a = record("/a", Verification::Verified, Some("3.12"));
        let mut b = a.clone(); b.verification = Verification::Configured; b.version = None;
        let mut c = a.clone(); c.backend = Backend::Windows;
        let mut d = a.clone(); d.distribution = Some("Other".into());
        let result = deduplicate(vec![a, b, c, d]);
        assert_eq!(result.len(), 4);
        assert!(result.iter().any(|r| r.verification == Verification::Configured && r.version.is_none()));
    }
    #[test] fn transitive_alias_bridge() {
        let a = record("/a", Verification::Present, None);
        let mut b = a.clone(); b.file_identity = None;
        let c = record("/c", Verification::Present, None);
        assert_eq!(deduplicate(vec![b, c, a]).len(), 1);
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PythonManifestConfig {
    pub manager: Option<String>,
    pub lockfile: Option<String>,
    #[serde(default)]
    pub requirements: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PackageManagerConfig {
    pub name: Option<String>,
    #[serde(default)]
    pub requirements: Vec<String>,
    pub lockfile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EnvironmentManifestConfig {
    pub mode: Option<String>,
    pub cleanup: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LifecycleConfig {
    pub policy: Option<String>,
    pub lease_timeout_seconds: Option<u64>,
    pub allow_reclaim_on_quiescence: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ExecutionConfig {
    pub entry_point: Option<String>,
    pub test_command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    pub schema_version: u32,
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub project: Option<ProjectMetadata>,
    #[serde(default)]
    pub runtimes: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub python: Option<PythonManifestConfig>,
    #[serde(default)]
    pub package_manager: Option<PackageManagerConfig>,
    #[serde(default)]
    pub environment: Option<EnvironmentManifestConfig>,
    #[serde(default)]
    pub lifecycle: Option<LifecycleConfig>,
    #[serde(default)]
    pub execution: Option<ExecutionConfig>,
}

impl ProjectManifest {
    pub fn project_name(&self) -> Option<&str> {
        if let Some(ref n) = self.name {
            Some(n.as_str())
        } else if let Some(ref p) = self.project {
            p.name.as_deref()
        } else {
            None
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInspection {
    pub schema_version: u32,
    pub path: String,
    pub native_manifests: Vec<String>,
    pub ulvo_manifest: Option<ProjectManifest>,
    pub diagnostics: Vec<String>,
}
