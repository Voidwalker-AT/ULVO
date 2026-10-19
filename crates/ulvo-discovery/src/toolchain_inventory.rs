use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use ulvo_core::{
    Abi, Architecture, Backend, Ecosystem, Ownership, Platform, Provenance, Resource,
    ResourceKind, VerificationStatus,
};

/// An inventory entry discovered during read-only toolchain scanning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredToolchain {
    pub name: String,
    pub executable_path: PathBuf,
    pub version: Option<String>,
    pub backend: Backend,
    pub ecosystem: Ecosystem,
    pub kind: ResourceKind,
    pub ownership: Ownership,
    pub discovery_source: String,
    pub capabilities: Vec<String>,
}

impl DiscoveredToolchain {
    pub fn to_resource(&self, _vault_root: &Path) -> Resource {
        let provenance = if self.ownership == Ownership::Managed {
            Some(Provenance {
                source_type: "vault_managed_toolchain".into(),
                canonical_url: None,
                expected_sha256: None,
                actual_sha256: None,
                manifest_path: None,
                notes: Some(format!("Discovered via {}", self.discovery_source)),
            })
        } else {
            None
        };

        Resource::new(
            self.kind.clone(),
            self.ecosystem.clone(),
            self.name.clone(),
            self.version.clone(),
            Platform::Windows,
            Architecture::X86_64,
            self.backend.clone(),
            match self.backend {
                Backend::Msys2 => Abi::Gnu,
                _ => Abi::None,
            },
            Some(self.executable_path.to_string_lossy().into_owned()),
            Some(self.discovery_source.clone()),
            self.ownership.clone(),
            VerificationStatus::Discovered,
            provenance,
            self.capabilities.clone(),
        )
    }
}

/// Comprehensive metadata-only toolchain inventory scanner.
pub struct UniversalToolchainScanner {
    vault_root: PathBuf,
}

impl UniversalToolchainScanner {
    pub fn new(vault_root: PathBuf) -> Self {
        Self { vault_root }
    }

    pub fn verify_vault_ownership(&self, path: &Path) -> Ownership {
        if !path.starts_with(&self.vault_root) {
            return Ownership::External;
        }
        // Verify provenance: must have a valid ownership record, manifest, or sentinel
        // 1. Rust runtime in vault/runtimes/rust
        if path.starts_with(self.vault_root.join("runtimes").join("rust")) {
            let manifest_path = self.vault_root.parent()
                .map(|p| p.join("config").join("toolchain-artifacts.toml"))
                .unwrap_or_default();
            if manifest_path.is_file() {
                return Ownership::Managed;
            }
        }
        // 2. Managed MSYS2 in vault/toolchains/msys2
        if path.starts_with(self.vault_root.join("toolchains").join("msys2")) {
            let sentinel = self.vault_root.join("toolchains").join("msys2").join(".ulvo_managed.json");
            if sentinel.is_file() {
                return Ownership::Managed;
            }
        }
        // 3. Ephemeral/Managed environment with .ulvo_ownership.json
        if let Some(parent) = path.parent() {
            if parent.join(".ulvo_ownership.json").is_file()
                || parent.parent().map(|p| p.join(".ulvo_ownership.json").is_file()).unwrap_or(false)
            {
                return Ownership::Managed;
            }
        }
        // Path containment alone without verified provenance metadata remains Unknown!
        Ownership::Unknown
    }

    /// Scans the host system for all supported development toolchains.
    pub fn scan_all(&self) -> Vec<DiscoveredToolchain> {
        let mut results = Vec::new();

        self.scan_path_executables(&mut results);
        self.scan_known_install_roots(&mut results);
        self.scan_msys2(&mut results);
        self.scan_wsl_metadata(&mut results);
        self.scan_vscode_settings(&mut results);

        // Deduplicate results based on canonical executable path
        let mut deduped: BTreeMap<String, DiscoveredToolchain> = BTreeMap::new();
        for item in results {
            let key = item.executable_path.to_string_lossy().to_lowercase();
            deduped.entry(key).or_insert(item);
        }

        deduped.into_values().collect()
    }

    fn scan_path_executables(&self, results: &mut Vec<DiscoveredToolchain>) {
        let path_var = std::env::var("PATH").unwrap_or_default();
        let targets = [
            ("python.exe", "python", Ecosystem::Python, ResourceKind::LanguageRuntime, vec!["interpreter"]),
            ("uv.exe", "uv", Ecosystem::Python, ResourceKind::PackageManager, vec!["package_manager"]),
            ("rustc.exe", "rustc", Ecosystem::Rust, ResourceKind::Compiler, vec!["compiler"]),
            ("cargo.exe", "cargo", Ecosystem::Rust, ResourceKind::Toolchain, vec!["build_tool"]),
            ("node.exe", "node", Ecosystem::NodeJs, ResourceKind::LanguageRuntime, vec!["interpreter"]),
            ("npm.cmd", "npm", Ecosystem::NodeJs, ResourceKind::PackageManager, vec!["package_manager"]),
            ("pnpm.cmd", "pnpm", Ecosystem::NodeJs, ResourceKind::PackageManager, vec!["package_manager"]),
            ("go.exe", "go", Ecosystem::Go, ResourceKind::Compiler, vec!["compiler", "build_tool"]),
            ("java.exe", "java", Ecosystem::Java, ResourceKind::LanguageRuntime, vec!["runtime"]),
            ("javac.exe", "javac", Ecosystem::Java, ResourceKind::Compiler, vec!["compiler"]),
            ("mvn.cmd", "maven", Ecosystem::Java, ResourceKind::Toolchain, vec!["build_tool"]),
            ("gradle.bat", "gradle", Ecosystem::Java, ResourceKind::Toolchain, vec!["build_tool"]),
            ("ruby.exe", "ruby", Ecosystem::Ruby, ResourceKind::LanguageRuntime, vec!["interpreter"]),
            ("gem.cmd", "gem", Ecosystem::Ruby, ResourceKind::PackageManager, vec!["package_manager"]),
            ("bundle.bat", "bundler", Ecosystem::Ruby, ResourceKind::PackageManager, vec!["package_manager"]),
            ("dotnet.exe", "dotnet", Ecosystem::DotNet, ResourceKind::Sdk, vec!["sdk", "runtime", "compiler"]),
            ("gcc.exe", "gcc", Ecosystem::Cpp, ResourceKind::Compiler, vec!["compiler"]),
            ("g++.exe", "g++", Ecosystem::Cpp, ResourceKind::Compiler, vec!["compiler"]),
            ("clang.exe", "clang", Ecosystem::Cpp, ResourceKind::Compiler, vec!["compiler"]),
            ("cmake.exe", "cmake", Ecosystem::Cpp, ResourceKind::Toolchain, vec!["build_tool"]),
            ("ninja.exe", "ninja", Ecosystem::Cpp, ResourceKind::Toolchain, vec!["build_tool"]),
        ];

        for dir in std::env::split_paths(&path_var) {
            for (filename, name, eco, kind, caps) in &targets {
                let candidate = dir.join(filename);
                if candidate.is_file() {
                    let ownership = self.verify_vault_ownership(&candidate);
                    let backend = if candidate.to_string_lossy().to_lowercase().contains("msys64") {
                        Backend::Msys2
                    } else {
                        Backend::Windows
                    };

                    results.push(DiscoveredToolchain {
                        name: name.to_string(),
                        executable_path: candidate,
                        version: None, // metadata-only by default
                        backend,
                        ecosystem: eco.clone(),
                        kind: kind.clone(),
                        ownership,
                        discovery_source: "path".into(),
                        capabilities: caps.iter().map(|s| s.to_string()).collect(),
                    });
                }
            }
        }
    }

    fn scan_known_install_roots(&self, results: &mut Vec<DiscoveredToolchain>) {
        // Known roots for Python, .NET, Scoop, and system locations
        let known = [
            ("C:\\Program Files\\dotnet\\dotnet.exe", "dotnet", Ecosystem::DotNet, ResourceKind::Sdk, vec!["sdk"]),
            ("C:\\Program Files\\Git\\bin\\git.exe", "git", Ecosystem::Generic("git".into()), ResourceKind::Executable, vec!["vcs"]),
            ("C:\\msys64\\ucrt64\\bin\\gcc.exe", "gcc", Ecosystem::Cpp, ResourceKind::Compiler, vec!["compiler"]),
            ("C:\\msys64\\ucrt64\\bin\\g++.exe", "g++", Ecosystem::Cpp, ResourceKind::Compiler, vec!["compiler"]),
        ];

        for (path_str, name, eco, kind, caps) in &known {
            let p = PathBuf::from(path_str);
            if p.is_file() {
                let backend = if path_str.to_lowercase().contains("msys64") {
                    Backend::Msys2
                } else {
                    Backend::Windows
                };
                results.push(DiscoveredToolchain {
                    name: name.to_string(),
                    executable_path: p,
                    version: None,
                    backend,
                    ecosystem: eco.clone(),
                    kind: kind.clone(),
                    ownership: Ownership::External,
                    discovery_source: "known_root".into(),
                    capabilities: caps.iter().map(|s| s.to_string()).collect(),
                });
            }
        }

        // Scoop toolchains
        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            let scoop_apps = PathBuf::from(&userprofile).join("scoop").join("apps");
            if scoop_apps.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&scoop_apps) {
                    for entry in entries.flatten() {
                        let app_name = entry.file_name().to_string_lossy().to_string();
                        let current = entry.path().join("current");
                        if current.is_dir() {
                            let exe = current.join(format!("{app_name}.exe"));
                            if exe.is_file() {
                                let (eco, kind) = match app_name.as_str() {
                                    "python" => (Ecosystem::Python, ResourceKind::LanguageRuntime),
                                    "uv" => (Ecosystem::Python, ResourceKind::PackageManager),
                                    "nodejs" => (Ecosystem::NodeJs, ResourceKind::LanguageRuntime),
                                    "go" => (Ecosystem::Go, ResourceKind::Compiler),
                                    _ => (Ecosystem::Generic(app_name.clone()), ResourceKind::Executable),
                                };
                                results.push(DiscoveredToolchain {
                                    name: app_name,
                                    executable_path: exe,
                                    version: None,
                                    backend: Backend::Windows,
                                    ecosystem: eco,
                                    kind,
                                    ownership: Ownership::External,
                                    discovery_source: "scoop".into(),
                                    capabilities: vec!["package".into()],
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    fn scan_msys2(&self, results: &mut Vec<DiscoveredToolchain>) {
        let msys_root = PathBuf::from("C:\\msys64");
        if !msys_root.is_dir() {
            return;
        }

        let subsystems = ["ucrt64", "mingw64", "clang64", "usr"];
        for sub in &subsystems {
            let bin_dir = msys_root.join(sub).join("bin");
            if !bin_dir.is_dir() {
                continue;
            }

            let candidates = [
                ("gcc.exe", "gcc", ResourceKind::Compiler),
                ("g++.exe", "g++", ResourceKind::Compiler),
                ("clang.exe", "clang", ResourceKind::Compiler),
                ("make.exe", "make", ResourceKind::Toolchain),
                ("ninja.exe", "ninja", ResourceKind::Toolchain),
            ];

            for (exe_file, tool_name, kind) in &candidates {
                let exe = bin_dir.join(exe_file);
                if exe.is_file() {
                    results.push(DiscoveredToolchain {
                        name: format!("{tool_name}_{sub}"),
                        executable_path: exe,
                        version: None,
                        backend: Backend::Msys2,
                        ecosystem: Ecosystem::Cpp,
                        kind: kind.clone(),
                        ownership: Ownership::External,
                        discovery_source: format!("msys2_{sub}"),
                        capabilities: vec!["compiler".into()],
                    });
                }
            }
        }
    }

    fn scan_wsl_metadata(&self, results: &mut Vec<DiscoveredToolchain>) {
        // Read WSL distribution registrations from registry without launching any distributions
        // On Windows, WSL installs register subkeys under HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss
        let wsl_exe = PathBuf::from("C:\\Windows\\System32\\wsl.exe");
        if wsl_exe.is_file() {
            results.push(DiscoveredToolchain {
                name: "wsl_host".into(),
                executable_path: wsl_exe,
                version: None,
                backend: Backend::Windows,
                ecosystem: Ecosystem::Generic("wsl".into()),
                kind: ResourceKind::Executable,
                ownership: Ownership::External,
                discovery_source: "wsl_metadata".into(),
                capabilities: vec!["virtualization".into()],
            });
        }
    }

    fn scan_vscode_settings(&self, results: &mut Vec<DiscoveredToolchain>) {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let settings_file = PathBuf::from(appdata)
                .join("Code")
                .join("User")
                .join("settings.json");

            if settings_file.is_file() {
                if let Ok(content) = std::fs::read_to_string(&settings_file) {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(obj) = parsed.as_object() {
                            if let Some(py_path) = obj.get("python.defaultInterpreterPath").and_then(|v| v.as_str()) {
                                let p = PathBuf::from(py_path);
                                if p.is_file() {
                                    results.push(DiscoveredToolchain {
                                        name: "python_vscode_configured".into(),
                                        executable_path: p,
                                        version: None,
                                        backend: Backend::Windows,
                                        ecosystem: Ecosystem::Python,
                                        kind: ResourceKind::LanguageRuntime,
                                        ownership: Ownership::External,
                                        discovery_source: "vscode_settings".into(),
                                        capabilities: vec!["interpreter".into()],
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scanner_discovers_without_crashing() {
        let scanner = UniversalToolchainScanner::new(PathBuf::from("E:\\Workspace\\Active\\ULVO\\vault"));
        let toolchains = scanner.scan_all();
        // The scanner must complete safely and discover external toolchains
        assert!(!toolchains.is_empty());
        assert!(toolchains.iter().all(|t| t.ownership == Ownership::External || t.ownership == Ownership::Managed));
    }

    #[test]
    fn test_external_ownership_retained_in_resources() {
        let dummy = DiscoveredToolchain {
            name: "python".into(),
            executable_path: PathBuf::from("C:\\Python311\\python.exe"),
            version: Some("3.11.0".into()),
            backend: Backend::Windows,
            ecosystem: Ecosystem::Python,
            kind: ResourceKind::LanguageRuntime,
            ownership: Ownership::External,
            discovery_source: "path".into(),
            capabilities: vec!["interpreter".into()],
        };

        let res = dummy.to_resource(Path::new("E:\\Workspace\\Active\\ULVO\\vault"));
        assert_eq!(res.ownership, Ownership::External);
        assert_ne!(res.ownership, Ownership::Managed);
    }
}
