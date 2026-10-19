use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use ulvo_core::Ecosystem;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogCategory {
    Runtime,
    Compiler,
    PackageManager,
    BuildTool,
    Sdk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallationMethod {
    ArchiveExtract,
    NativeExecutable,
    ScriptProvision,
    PackageInstall,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstalledStatus {
    NotInstalled,
    Installed { version: String, managed_path: String },
    UpdateAvailable { current: String, latest: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogProvenance {
    OfficialSource { publisher: String, repository: String },
    TrustedDistributor { distributor: String, signature_fingerprint: Option<String> },
    CryptographicallySigned { signing_key_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub category: CatalogCategory,
    pub ecosystem: Ecosystem,
    pub available_versions: Vec<String>,
    pub recommended_version: String,
    pub release_channel: String,
    pub platform: String,
    pub architecture: String,
    pub publisher: String,
    pub description: String,
    pub installation_method: InstallationMethod,
    pub expected_download_bytes: Option<u64>,
    pub installed_size_estimate_bytes: Option<u64>,
    pub compatibility_requirements: Vec<String>,
    pub installed_status: InstalledStatus,
    pub verification_status: String,
    pub provenance: CatalogProvenance,
    pub download_url: String,
    pub integrity_sha256: String,
    pub environment_variables: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchIntentKind {
    RuntimeInstallation,
    ToolchainInstallation,
    GlobalToolInstallation,
    ProjectDependency { package_name: String, version_spec: Option<String> },
    PackageCacheOperation,
    Ambiguous { candidates: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResolution {
    pub raw_query: String,
    pub intent: SearchIntentKind,
    pub matched_entries: Vec<CatalogEntry>,
    pub ecosystem_hint: Option<Ecosystem>,
    pub suggested_action: String,
}

pub struct CatalogService {
    vault_root: PathBuf,
    cache_path: PathBuf,
}

impl CatalogService {
    pub fn new<P: AsRef<Path>>(vault_root: P) -> Self {
        let vr = vault_root.as_ref().to_path_buf();
        let cache_path = vr.join("catalog").join("cache.json");
        Self {
            vault_root: vr,
            cache_path,
        }
    }

    /// Returns the comprehensive verified catalog for all supported language runtimes, compilers, and toolchains.
    pub fn list_catalog(&self) -> Vec<CatalogEntry> {
        let mut entries = self.load_cached_catalog().unwrap_or_else(Self::builtin_catalog);
        self.reconcile_installed_statuses(&mut entries);
        entries
    }

    /// Resolves user search query into explicit action intent and matched catalog items.
    pub fn resolve_query(&self, query: &str) -> SearchResolution {
        let trimmed = query.trim();
        let catalog = self.list_catalog();

        if trimmed.is_empty() {
            return SearchResolution {
                raw_query: query.to_string(),
                intent: SearchIntentKind::Ambiguous { candidates: Vec::new() },
                matched_entries: catalog,
                ecosystem_hint: None,
                suggested_action: "Browse available development runtimes and tools".to_string(),
            };
        }

        // 1. Direct name or name@version match
        let (name_part, version_spec) = if let Some((n, v)) = trimmed.split_once('@') {
            (n.trim(), Some(v.trim().to_string()))
        } else if let Some((n, v)) = trimmed.split_once("==") {
            (n.trim(), Some(v.trim().to_string()))
        } else {
            (trimmed, None)
        };

        let lower_name = name_part.to_lowercase();

        // Check if it matches a catalog entry
        let matches: Vec<CatalogEntry> = catalog
            .iter()
            .filter(|e| {
                e.name.to_lowercase() == lower_name
                    || e.display_name.to_lowercase().contains(&lower_name)
                    || e.id.to_lowercase() == lower_name
            })
            .cloned()
            .collect();

        if !matches.is_empty() {
            let primary = matches[0].clone();
            let intent = match primary.category {
                CatalogCategory::Runtime => SearchIntentKind::RuntimeInstallation,
                CatalogCategory::Compiler | CatalogCategory::BuildTool | CatalogCategory::Sdk => {
                    SearchIntentKind::ToolchainInstallation
                }
                CatalogCategory::PackageManager => SearchIntentKind::GlobalToolInstallation,
            };

            let suggested_action = if let Some(ref ver) = version_spec {
                format!("Install {} version {} into ULVO vault", primary.display_name, ver)
            } else {
                format!("Install {} (recommended: {}) into ULVO vault", primary.display_name, primary.recommended_version)
            };

            return SearchResolution {
                raw_query: query.to_string(),
                intent,
                matched_entries: matches,
                ecosystem_hint: Some(primary.ecosystem),
                suggested_action,
            };
        }

        // 2. Python package / Project dependency detection
        if lower_name == "torch"
            || lower_name.starts_with("torch")
            || lower_name == "numpy"
            || lower_name == "scipy"
            || lower_name == "pandas"
            || lower_name == "transformers"
            || lower_name == "pytest"
            || lower_name == "flask"
            || lower_name == "fastapi"
        {
            return SearchResolution {
                raw_query: query.to_string(),
                intent: SearchIntentKind::ProjectDependency {
                    package_name: lower_name,
                    version_spec,
                },
                matched_entries: Vec::new(),
                ecosystem_hint: Some(Ecosystem::Python),
                suggested_action: "Add package to project dependency manifest (.ulvo/manifest.toml or pyproject.toml)".to_string(),
            };
        }

        // 3. Fallback partial substring matches
        let partial_matches: Vec<CatalogEntry> = catalog
            .into_iter()
            .filter(|e| {
                e.name.to_lowercase().contains(&lower_name)
                    || e.description.to_lowercase().contains(&lower_name)
            })
            .collect();

        let candidates = partial_matches.iter().map(|e| e.name.clone()).collect();
        SearchResolution {
            raw_query: query.to_string(),
            intent: SearchIntentKind::Ambiguous { candidates },
            matched_entries: partial_matches,
            ecosystem_hint: None,
            suggested_action: "Select a matched runtime or toolchain".to_string(),
        }
    }

    /// Reconciles installed status of catalog entries against the local vault.
    pub fn reconcile_installed_statuses(&self, entries: &mut [CatalogEntry]) {
        for entry in entries.iter_mut() {
            let check_dir = match entry.id.as_str() {
                "python" => self.vault_root.join("runtimes").join("python"),
                "rust" => self.vault_root.join("runtimes").join("rust").join("cargo").join("bin"),
                "msys2" => self.vault_root.join("toolchains").join("msys2"),
                "go" => self.vault_root.join("runtimes").join("go"),
                "ruby" => self.vault_root.join("runtimes").join("ruby"),
                "node" => self.vault_root.join("runtimes").join("node"),
                "java" => self.vault_root.join("runtimes").join("java"),
                _ => self.vault_root.join("toolchains").join(&entry.id),
            };

            if check_dir.is_dir() {
                entry.installed_status = InstalledStatus::Installed {
                    version: entry.recommended_version.clone(),
                    managed_path: check_dir.to_string_lossy().into_owned(),
                };
            } else {
                entry.installed_status = InstalledStatus::NotInstalled;
            }
        }
    }

    fn load_cached_catalog(&self) -> Option<Vec<CatalogEntry>> {
        if !self.cache_path.is_file() {
            return None;
        }
        let data = std::fs::read_to_string(&self.cache_path).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn save_cached_catalog(&self, entries: &[CatalogEntry]) -> Result<(), String> {
        if let Some(parent) = self.cache_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let data = serde_json::to_string_pretty(entries)
            .map_err(|e| format!("cannot serialize catalog cache: {e}"))?;
        std::fs::write(&self.cache_path, data)
            .map_err(|e| format!("cannot write catalog cache: {e}"))?;
        Ok(())
    }

    /// Builtin verified catalog metadata for supported ecosystems.
    pub fn builtin_catalog() -> Vec<CatalogEntry> {
        vec![
            CatalogEntry {
                id: "python".into(),
                name: "python".into(),
                display_name: "Python 3 Runtime".into(),
                category: CatalogCategory::Runtime,
                ecosystem: Ecosystem::Python,
                available_versions: vec!["3.10.11".into(), "3.11.9".into(), "3.12.8".into(), "3.13.1".into()],
                recommended_version: "3.12.8".into(),
                release_channel: "stable".into(),
                platform: "windows".into(),
                architecture: "x86_64".into(),
                publisher: "Python Software Foundation".into(),
                description: "Interpreted, high-level language runtime with comprehensive standard library and ML/data science ecosystem.".into(),
                installation_method: InstallationMethod::ArchiveExtract,
                expected_download_bytes: Some(28_311_552),
                installed_size_estimate_bytes: Some(95_000_000),
                compatibility_requirements: vec!["Windows 10/11 x64".into(), "NTFS volume".into()],
                installed_status: InstalledStatus::NotInstalled,
                verification_status: "VERIFIED".into(),
                provenance: CatalogProvenance::OfficialSource {
                    publisher: "Python Software Foundation".into(),
                    repository: "https://www.python.org/ftp/python".into(),
                },
                download_url: "https://www.python.org/ftp/python/3.12.8/python-3.12.8-embed-amd64.zip".into(),
                integrity_sha256: "b0f7e9ea8e7b30efcf9fb75c7dd884ad2d32db82e70e7e17cb95a32ecbe8c5bc".into(),
                environment_variables: [("PYTHONUTF8".into(), "1".into()), ("PYTHONIOENCODING".into(), "utf-8".into())].into(),
            },
            CatalogEntry {
                id: "uv".into(),
                name: "uv".into(),
                display_name: "uv Package Manager".into(),
                category: CatalogCategory::PackageManager,
                ecosystem: Ecosystem::Python,
                available_versions: vec!["0.5.21".into(), "0.5.20".into()],
                recommended_version: "0.5.21".into(),
                release_channel: "stable".into(),
                platform: "windows".into(),
                architecture: "x86_64".into(),
                publisher: "Astral Software Inc.".into(),
                description: "Extremely fast Python package installer and resolver, written in Rust. Managed by ULVO for zero-copy package materialization.".into(),
                installation_method: InstallationMethod::ArchiveExtract,
                expected_download_bytes: Some(15_200_000),
                installed_size_estimate_bytes: Some(45_000_000),
                compatibility_requirements: vec!["Windows 10/11 x64".into()],
                installed_status: InstalledStatus::NotInstalled,
                verification_status: "VERIFIED".into(),
                provenance: CatalogProvenance::OfficialSource {
                    publisher: "Astral Software".into(),
                    repository: "https://github.com/astral-sh/uv".into(),
                },
                download_url: "https://github.com/astral-sh/uv/releases/download/0.5.21/uv-x86_64-pc-windows-msvc.zip".into(),
                integrity_sha256: "9b4f98129df83ecb2a472901192fa9bd671d17983636fdbda262db8038eb9ee4".into(),
                environment_variables: [("UV_LINK_MODE".into(), "hardlink".into())].into(),
            },
            CatalogEntry {
                id: "rust".into(),
                name: "rust".into(),
                display_name: "Rust & Cargo Toolchain".into(),
                category: CatalogCategory::Compiler,
                ecosystem: Ecosystem::Rust,
                available_versions: vec!["1.96.0".into(), "1.84.0".into(), "stable".into()],
                recommended_version: "1.96.0".into(),
                release_channel: "stable".into(),
                platform: "windows".into(),
                architecture: "x86_64".into(),
                publisher: "Rust Foundation".into(),
                description: "Systems programming language that runs blazingly fast, prevents segfaults, and guarantees thread safety.".into(),
                installation_method: InstallationMethod::ArchiveExtract,
                expected_download_bytes: Some(185_000_000),
                installed_size_estimate_bytes: Some(950_000_000),
                compatibility_requirements: vec!["Windows 10/11 x64".into(), "MSYS2 UCRT64 GCC or MSVC Linker".into()],
                installed_status: InstalledStatus::NotInstalled,
                verification_status: "VERIFIED".into(),
                provenance: CatalogProvenance::OfficialSource {
                    publisher: "Rust Foundation".into(),
                    repository: "https://static.rust-lang.org/dist".into(),
                },
                download_url: "https://static.rust-lang.org/dist/rust-1.96.0-x86_64-pc-windows-gnu.tar.gz".into(),
                integrity_sha256: "0c9d784a9e22e921d723fa3cfd9e847385beac43ef1027170889c0a6b7de58a1".into(),
                environment_variables: [("CARGO_HOME".into(), "vault/runtimes/rust/cargo".into()), ("RUSTUP_HOME".into(), "vault/runtimes/rust/rustup".into())].into(),
            },
            CatalogEntry {
                id: "msys2".into(),
                name: "msys2".into(),
                display_name: "MSYS2 UCRT64 C/C++ Toolchain (GCC/G++)".into(),
                category: CatalogCategory::Compiler,
                ecosystem: Ecosystem::Cpp,
                available_versions: vec!["14.2.0".into(), "13.2.0".into()],
                recommended_version: "14.2.0".into(),
                release_channel: "rolling".into(),
                platform: "windows".into(),
                architecture: "x86_64".into(),
                publisher: "MSYS2 Project".into(),
                description: "Native Windows C and C++ compiler toolchain targeting Universal C Runtime (UCRT), providing GCC, G++, Make, and Binutils.".into(),
                installation_method: InstallationMethod::ArchiveExtract,
                expected_download_bytes: Some(110_000_000),
                installed_size_estimate_bytes: Some(1_450_000_000),
                compatibility_requirements: vec!["Windows 10/11 x64".into()],
                installed_status: InstalledStatus::NotInstalled,
                verification_status: "VERIFIED".into(),
                provenance: CatalogProvenance::OfficialSource {
                    publisher: "MSYS2 Team".into(),
                    repository: "https://repo.msys2.org".into(),
                },
                download_url: "https://repo.msys2.org/distrib/x86_64/msys2-base-x86_64-20240113.tar.xz".into(),
                integrity_sha256: "86cb691ee9ebc829e578c7739502157053e19864230b427845f9dbcd10be1aa7".into(),
                environment_variables: [("MSYSTEM".into(), "UCRT64".into())].into(),
            },
            CatalogEntry {
                id: "go".into(),
                name: "go".into(),
                display_name: "Go Programming Language".into(),
                category: CatalogCategory::Runtime,
                ecosystem: Ecosystem::Go,
                available_versions: vec!["1.23.4".into(), "1.22.10".into()],
                recommended_version: "1.23.4".into(),
                release_channel: "stable".into(),
                platform: "windows".into(),
                architecture: "x86_64".into(),
                publisher: "Google / Go Authors".into(),
                description: "Open source programming language that makes it easy to build simple, reliable, and efficient software with fast compilation.".into(),
                installation_method: InstallationMethod::ArchiveExtract,
                expected_download_bytes: Some(72_000_000),
                installed_size_estimate_bytes: Some(260_000_000),
                compatibility_requirements: vec!["Windows 10/11 x64".into()],
                installed_status: InstalledStatus::NotInstalled,
                verification_status: "VERIFIED".into(),
                provenance: CatalogProvenance::OfficialSource {
                    publisher: "Google LLC".into(),
                    repository: "https://go.dev/dl".into(),
                },
                download_url: "https://go.dev/dl/go1.23.4.windows-amd64.zip".into(),
                integrity_sha256: "679f2fe481c4c81db5c249a5aa9f1bf372a7c473c1ef7e29630c7974cb3bf322".into(),
                environment_variables: [("GOPATH".into(), "vault/packages/go/path".into()), ("GOMODCACHE".into(), "vault/packages/go/mod".into())].into(),
            },
            CatalogEntry {
                id: "ruby".into(),
                name: "ruby".into(),
                display_name: "Ruby Language Runtime".into(),
                category: CatalogCategory::Runtime,
                ecosystem: Ecosystem::Ruby,
                available_versions: vec!["3.3.6".into(), "3.2.6".into()],
                recommended_version: "3.3.6".into(),
                release_channel: "stable".into(),
                platform: "windows".into(),
                architecture: "x86_64".into(),
                publisher: "Ruby Community".into(),
                description: "Dynamic, open source programming language with a focus on simplicity and productivity.".into(),
                installation_method: InstallationMethod::ArchiveExtract,
                expected_download_bytes: Some(25_000_000),
                installed_size_estimate_bytes: Some(110_000_000),
                compatibility_requirements: vec!["Windows 10/11 x64".into()],
                installed_status: InstalledStatus::NotInstalled,
                verification_status: "VERIFIED".into(),
                provenance: CatalogProvenance::OfficialSource {
                    publisher: "Ruby Community".into(),
                    repository: "https://github.com/oneclick/rubyinstaller2".into(),
                },
                download_url: "https://github.com/oneclick/rubyinstaller2/releases/download/RubyInstaller-3.3.6-1/rubyinstaller-3.3.6-1-x64.7z".into(),
                integrity_sha256: "46f334bf402434fa36c4b18df56ec74c10a30ea693d2073998818f0a14fe21da".into(),
                environment_variables: [("GEM_HOME".into(), "vault/packages/ruby/gems".into())].into(),
            },
            CatalogEntry {
                id: "node".into(),
                name: "node".into(),
                display_name: "Node.js JavaScript Runtime".into(),
                category: CatalogCategory::Runtime,
                ecosystem: Ecosystem::NodeJs,
                available_versions: vec!["22.12.0".into(), "20.18.1".into(), "24.15.0".into()],
                recommended_version: "22.12.0".into(),
                release_channel: "lts".into(),
                platform: "windows".into(),
                architecture: "x86_64".into(),
                publisher: "OpenJS Foundation".into(),
                description: "JavaScript runtime built on Chrome's V8 JavaScript engine for scalable server-side and desktop applications.".into(),
                installation_method: InstallationMethod::ArchiveExtract,
                expected_download_bytes: Some(33_000_000),
                installed_size_estimate_bytes: Some(85_000_000),
                compatibility_requirements: vec!["Windows 10/11 x64".into()],
                installed_status: InstalledStatus::NotInstalled,
                verification_status: "VERIFIED".into(),
                provenance: CatalogProvenance::OfficialSource {
                    publisher: "OpenJS Foundation".into(),
                    repository: "https://nodejs.org/dist".into(),
                },
                download_url: "https://nodejs.org/dist/v22.12.0/node-v22.12.0-win-x64.zip".into(),
                integrity_sha256: "2d17466cfcf05bbdf56073bf9199d91f5313988ce23136a8d6be73b0635e8cb5".into(),
                environment_variables: [("NODE_PATH".into(), "vault/packages/node/node_modules".into())].into(),
            },
            CatalogEntry {
                id: "java".into(),
                name: "java".into(),
                display_name: "Eclipse Temurin OpenJDK 21".into(),
                category: CatalogCategory::Sdk,
                ecosystem: Ecosystem::Java,
                available_versions: vec!["21.0.5".into(), "17.0.13".into()],
                recommended_version: "21.0.5".into(),
                release_channel: "lts".into(),
                platform: "windows".into(),
                architecture: "x86_64".into(),
                publisher: "Eclipse Foundation (Adoptium)".into(),
                description: "High-performance, enterprise-grade, certified OpenJDK binaries for Java development and enterprise runtime execution.".into(),
                installation_method: InstallationMethod::ArchiveExtract,
                expected_download_bytes: Some(195_000_000),
                installed_size_estimate_bytes: Some(340_000_000),
                compatibility_requirements: vec!["Windows 10/11 x64".into()],
                installed_status: InstalledStatus::NotInstalled,
                verification_status: "VERIFIED".into(),
                provenance: CatalogProvenance::OfficialSource {
                    publisher: "Eclipse Foundation".into(),
                    repository: "https://github.com/adoptium/temurin21-binaries".into(),
                },
                download_url: "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.5%2B11/OpenJDK21U-jdk_x64_windows_hotspot_21.0.5_11.zip".into(),
                integrity_sha256: "729ff7ca1b4396a84d436cf0e352936230f8c381c81ef80a56ffdb024e0dae95".into(),
                environment_variables: [("JAVA_HOME".into(), "vault/runtimes/java/current".into())].into(),
            },
            CatalogEntry {
                id: "cmake".into(),
                name: "cmake".into(),
                display_name: "CMake Build System".into(),
                category: CatalogCategory::BuildTool,
                ecosystem: Ecosystem::Cpp,
                available_versions: vec!["3.30.5".into(), "3.29.0".into()],
                recommended_version: "3.30.5".into(),
                release_channel: "stable".into(),
                platform: "windows".into(),
                architecture: "x86_64".into(),
                publisher: "Kitware Inc.".into(),
                description: "Cross-platform open-source meta-build system to build, test, and package software across multiple languages.".into(),
                installation_method: InstallationMethod::ArchiveExtract,
                expected_download_bytes: Some(42_000_000),
                installed_size_estimate_bytes: Some(120_000_000),
                compatibility_requirements: vec!["Windows 10/11 x64".into()],
                installed_status: InstalledStatus::NotInstalled,
                verification_status: "VERIFIED".into(),
                provenance: CatalogProvenance::OfficialSource {
                    publisher: "Kitware".into(),
                    repository: "https://github.com/Kitware/CMake".into(),
                },
                download_url: "https://github.com/Kitware/CMake/releases/download/v3.30.5/cmake-3.30.5-windows-x86_64.zip".into(),
                integrity_sha256: "2508930190fa7cebebfad58cf680196720eaecfe85124b896f6424e4cce316d2".into(),
                environment_variables: HashMap::new(),
            },
            CatalogEntry {
                id: "ninja".into(),
                name: "ninja".into(),
                display_name: "Ninja Fast Build Tool".into(),
                category: CatalogCategory::BuildTool,
                ecosystem: Ecosystem::Cpp,
                available_versions: vec!["1.12.1".into()],
                recommended_version: "1.12.1".into(),
                release_channel: "stable".into(),
                platform: "windows".into(),
                architecture: "x86_64".into(),
                publisher: "Ninja Authors".into(),
                description: "Small build system with a focus on speed, designed to have its input files generated by a higher-level build system like CMake.".into(),
                installation_method: InstallationMethod::ArchiveExtract,
                expected_download_bytes: Some(600_000),
                installed_size_estimate_bytes: Some(1_800_000),
                compatibility_requirements: vec!["Windows 10/11 x64".into()],
                installed_status: InstalledStatus::NotInstalled,
                verification_status: "VERIFIED".into(),
                provenance: CatalogProvenance::OfficialSource {
                    publisher: "Ninja Authors".into(),
                    repository: "https://github.com/ninja-build/ninja".into(),
                },
                download_url: "https://github.com/ninja-build/ninja/releases/download/v1.12.1/ninja-win.zip".into(),
                integrity_sha256: "721a9c382eaec2d5e5e2da8f343a4e9b9cf2a67e5bb979e95ffcf398c8c57b32".into(),
                environment_variables: HashMap::new(),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_catalog_integrity() {
        let catalog = CatalogService::builtin_catalog();
        assert!(catalog.len() >= 8);
        for entry in &catalog {
            assert!(!entry.id.is_empty());
            assert!(!entry.download_url.is_empty());
            assert_eq!(entry.integrity_sha256.len(), 64);
            assert_eq!(entry.platform, "windows");
            assert_eq!(entry.architecture, "x86_64");
        }
    }

    #[test]
    fn test_resolve_query_runtime() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_cat1");
        let svc = CatalogService::new(&temp_dir);
        let res = svc.resolve_query("python@3.12");
        assert_eq!(res.intent, SearchIntentKind::RuntimeInstallation);
        assert_eq!(res.ecosystem_hint, Some(Ecosystem::Python));
        assert!(!res.matched_entries.is_empty());
    }

    #[test]
    fn test_resolve_query_dependency() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_cat2");
        let svc = CatalogService::new(&temp_dir);
        let res = svc.resolve_query("torch==2.6.0+cu124");
        match res.intent {
            SearchIntentKind::ProjectDependency { package_name, version_spec } => {
                assert_eq!(package_name, "torch");
                assert_eq!(version_spec, Some("2.6.0+cu124".to_string()));
            }
            _ => panic!("expected ProjectDependency intent"),
        }
        assert_eq!(res.ecosystem_hint, Some(Ecosystem::Python));
    }

    #[test]
    fn test_resolve_query_ambiguous() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_cat3");
        let svc = CatalogService::new(&temp_dir);
        let res = svc.resolve_query("unknown_custom_tool");
        match res.intent {
            SearchIntentKind::Ambiguous { .. } => {}
            _ => panic!("expected Ambiguous intent"),
        }
    }
}
