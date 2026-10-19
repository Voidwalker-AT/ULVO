use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use crate::gpu::GpuManifestConfig;

/// Normal target size budget for a lightweight .ulvo connector directory: 100 KiB (102,400 bytes).
pub const CONNECTOR_TARGET_BUDGET_BYTES: u64 = 100 * 1024;

/// Absolute maximum allowed metadata budget for .ulvo: 10 MiB (10,485,760 bytes).
pub const CONNECTOR_MAX_BUDGET_BYTES: u64 = 10 * 1024 * 1024;

/// Prohibited directory or file patterns inside `<project>/.ulvo` that indicate
/// the directory is improperly acting as a heavy environment rather than a lightweight connector.
pub const PROHIBITED_CONNECTOR_ARTIFACTS: &[&str] = &[
    ".venv",
    "venv",
    "env",
    "virtualenv",
    "node_modules",
    "site-packages",
    "bin",
    "Scripts",
    "lib",
    "Lib",
    "include",
    "target",
    "build",
    "dist",
    "out",
    "cache",
    "caches",
    "__pycache__",
    ".git",
    ".pnpm-store",
];

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
pub struct NodeManifestConfig {
    pub manager: Option<String>,
    pub package_json: Option<String>,
    pub lockfile: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RustManifestConfig {
    pub toolchain: Option<String>,
    pub cargo_toml: Option<String>,
    pub lockfile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CppManifestConfig {
    pub toolchain: Option<String>,
    pub build_system: Option<String>,
    pub manifest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct JavaManifestConfig {
    pub jdk: Option<String>,
    pub build_tool: Option<String>,
    pub manifest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GoManifestConfig {
    pub version: Option<String>,
    pub gomod: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RubyManifestConfig {
    pub version: Option<String>,
    pub gemfile: Option<String>,
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
    pub runtimes: BTreeMap<String, String>,
    #[serde(default)]
    pub python: Option<PythonManifestConfig>,
    #[serde(default)]
    pub node: Option<NodeManifestConfig>,
    #[serde(default)]
    pub rust: Option<RustManifestConfig>,
    #[serde(default)]
    pub cpp: Option<CppManifestConfig>,
    #[serde(default)]
    pub java: Option<JavaManifestConfig>,
    #[serde(default)]
    pub go: Option<GoManifestConfig>,
    #[serde(default)]
    pub ruby: Option<RubyManifestConfig>,
    #[serde(default)]
    pub package_manager: Option<PackageManagerConfig>,
    #[serde(default)]
    pub environment: Option<EnvironmentManifestConfig>,
    #[serde(default)]
    pub lifecycle: Option<LifecycleConfig>,
    #[serde(default)]
    pub execution: Option<ExecutionConfig>,
    #[serde(default)]
    pub gpu: Option<GpuManifestConfig>,
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

    /// Returns list of standard native manifest files referenced by this project manifest.
    pub fn referenced_native_manifests(&self) -> Vec<String> {
        let mut manifests = Vec::new();

        // Python
        if let Some(ref py) = self.python {
            if let Some(ref lock) = py.lockfile {
                manifests.push(lock.clone());
            }
            for req in &py.requirements {
                manifests.push(req.clone());
            }
        }
        if let Some(ref pm) = self.package_manager {
            if let Some(ref lock) = pm.lockfile {
                manifests.push(lock.clone());
            }
            for req in &pm.requirements {
                manifests.push(req.clone());
            }
        }

        // Node.js
        if let Some(ref node) = self.node {
            manifests.push(node.package_json.clone().unwrap_or_else(|| "package.json".to_string()));
            if let Some(ref lock) = node.lockfile {
                manifests.push(lock.clone());
            }
        }

        // Rust
        if let Some(ref rust) = self.rust {
            manifests.push(rust.cargo_toml.clone().unwrap_or_else(|| "Cargo.toml".to_string()));
            if let Some(ref lock) = rust.lockfile {
                manifests.push(lock.clone());
            }
        }

        // C/C++
        if let Some(ref cpp) = self.cpp {
            if let Some(ref m) = cpp.manifest {
                manifests.push(m.clone());
            }
        }

        // Java
        if let Some(ref java) = self.java {
            if let Some(ref m) = java.manifest {
                manifests.push(m.clone());
            }
        }

        // Go
        if let Some(ref go) = self.go {
            manifests.push(go.gomod.clone().unwrap_or_else(|| "go.mod".to_string()));
        }

        // Ruby
        if let Some(ref ruby) = self.ruby {
            manifests.push(ruby.gemfile.clone().unwrap_or_else(|| "Gemfile".to_string()));
        }

        manifests.sort();
        manifests.dedup();
        manifests
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInspection {
    pub schema_version: u32,
    pub path: String,
    pub native_manifests: Vec<String>,
    pub ulvo_manifest: Option<ProjectManifest>,
    #[serde(default)]
    pub connector: Option<ConnectorContract>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorComplianceVerdict {
    Compliant,
    TargetBudgetExceeded,
    MaxBudgetExceeded,
    ProhibitedArtifactDetected,
    MissingManifest,
    InvalidManifest,
}

impl std::fmt::Display for ConnectorComplianceVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compliant => write!(f, "COMPLIANT (Thin Connector Invariant Met)"),
            Self::TargetBudgetExceeded => write!(f, "WARNING: Target Budget Exceeded (> 100 KiB)"),
            Self::MaxBudgetExceeded => write!(f, "VIOLATION: Max Budget Exceeded (> 10 MiB)"),
            Self::ProhibitedArtifactDetected => write!(f, "VIOLATION: Prohibited Environment Artifact Detected"),
            Self::MissingManifest => write!(f, "VIOLATION: Missing .ulvo/manifest.toml"),
            Self::InvalidManifest => write!(f, "VIOLATION: Invalid .ulvo/manifest.toml Syntax"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorContract {
    pub schema_version: u32,
    pub project_path: String,
    pub connector_dir: String,
    pub manifest_path: String,
    pub total_bytes: u64,
    pub file_count: usize,
    pub target_budget_bytes: u64,
    pub max_budget_bytes: u64,
    pub is_within_target_budget: bool,
    pub is_within_max_budget: bool,
    pub prohibited_entries_detected: Vec<String>,
    pub referenced_native_manifests: Vec<String>,
    pub compliance_verdict: ConnectorComplianceVerdict,
    pub diagnostics: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thin_connector_constants() {
        assert_eq!(CONNECTOR_TARGET_BUDGET_BYTES, 102400); // 100 KiB
        assert_eq!(CONNECTOR_MAX_BUDGET_BYTES, 10485760);  // 10 MiB
        assert!(PROHIBITED_CONNECTOR_ARTIFACTS.contains(&".venv"));
        assert!(PROHIBITED_CONNECTOR_ARTIFACTS.contains(&"node_modules"));
        assert!(PROHIBITED_CONNECTOR_ARTIFACTS.contains(&"site-packages"));
    }

    #[test]
    fn test_multi_ecosystem_manifest_parsing() {
        let toml_str = r#"
schema_version = 1
name = "polyglot_demo"

[runtimes]
python = ">=3.11"
node = ">=20.0"
rust = "stable"

[python]
manager = "uv"
requirements = ["requirements.txt"]

[node]
manager = "pnpm"
package_json = "package.json"
lockfile = "pnpm-lock.yaml"

[rust]
toolchain = "stable"
cargo_toml = "Cargo.toml"

[cpp]
toolchain = "msys2-ucrt64"
manifest = "CMakeLists.txt"

[java]
jdk = "21"
manifest = "pom.xml"

[go]
version = "1.23"
gomod = "go.mod"

[ruby]
version = "3.3"
gemfile = "Gemfile"
"#;
        let manifest: ProjectManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.project_name(), Some("polyglot_demo"));
        assert_eq!(manifest.runtimes.len(), 3);
        assert!(manifest.python.is_some());
        assert!(manifest.node.is_some());
        assert!(manifest.rust.is_some());
        assert!(manifest.cpp.is_some());
        assert!(manifest.java.is_some());
        assert!(manifest.go.is_some());
        assert!(manifest.ruby.is_some());

        let referenced = manifest.referenced_native_manifests();
        assert!(referenced.contains(&"requirements.txt".to_string()));
        assert!(referenced.contains(&"package.json".to_string()));
        assert!(referenced.contains(&"pnpm-lock.yaml".to_string()));
        assert!(referenced.contains(&"Cargo.toml".to_string()));
        assert!(referenced.contains(&"CMakeLists.txt".to_string()));
        assert!(referenced.contains(&"pom.xml".to_string()));
        assert!(referenced.contains(&"go.mod".to_string()));
        assert!(referenced.contains(&"Gemfile".to_string()));
    }
}
