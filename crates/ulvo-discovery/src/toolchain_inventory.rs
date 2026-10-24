use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use ulvo_core::{
    Abi, Architecture, Backend, Ecosystem, Ownership, Platform, Provenance, Resource,
    ResourceKind, VerificationStatus,
};

/// An inventory entry discovered during read-only toolchain scanning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolchainDiscoveryReport {
    pub tools: Vec<DiscoveredToolchain>,
    pub partial: bool,
    pub inaccessible_paths: Vec<String>,
    pub scanned_directories: usize,
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
        let path = ulvo_core::strip_unc_prefix(&path.canonicalize().unwrap_or_else(|_| path.to_path_buf()));
        let vault_root = ulvo_core::strip_unc_prefix(&self.vault_root.canonicalize().unwrap_or_else(|_| self.vault_root.clone()));
        if !path.starts_with(&vault_root) {
            return Ownership::External;
        }
        // Verify provenance: must have a valid ownership record, manifest, or sentinel
        // 1. Rust runtime in vault/runtimes/rust
        if path.starts_with(vault_root.join("runtimes").join("rust")) {
            let manifest_path = vault_root.parent()
                .map(|p| p.join("config").join("toolchain-artifacts.toml"))
                .unwrap_or_default();
            if manifest_path.is_file() {
                return Ownership::Managed;
            }
        }
        // 2. Managed MSYS2 in vault/toolchains/msys2
        if path.starts_with(vault_root.join("toolchains").join("msys2")) {
            let sentinel = vault_root.join("toolchains").join("msys2").join(".ulvo_managed.json");
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
        self.scan_vault_toolchains(&mut results);
        self.scan_wsl_metadata(&mut results);
        self.scan_vscode_settings(&mut results);

        // Deduplicate results based on canonical executable path
        let mut deduped: BTreeMap<String, DiscoveredToolchain> = BTreeMap::new();
        for item in results {
            let canonical = item.executable_path.canonicalize().unwrap_or_else(|_| item.executable_path.clone());
            let key = ulvo_core::strip_unc_prefix(&canonical).to_string_lossy().to_lowercase();
            deduped.entry(key).or_insert(item);
        }

        deduped.into_values().collect()
    }

    /// Cheap reference refresh; broad installation discovery is explicit.
    pub fn scan_gateway_references(&self) -> Vec<DiscoveredToolchain> {
        let mut results = Vec::new();
        self.scan_path_executables(&mut results);
        self.scan_vault_toolchains(&mut results);
        results
    }

    fn scan_path_executables(&self, results: &mut Vec<DiscoveredToolchain>) {
        let path_var = std::env::var("PATH").unwrap_or_default();
        let targets = [
            ("python.exe", "python", Ecosystem::Python, ResourceKind::LanguageRuntime, vec!["interpreter"]),
            ("python3.exe", "python3", Ecosystem::Python, ResourceKind::LanguageRuntime, vec!["interpreter"]),
            ("uv.exe", "uv", Ecosystem::Python, ResourceKind::PackageManager, vec!["package_manager"]),
            ("pip.exe", "pip", Ecosystem::Python, ResourceKind::PackageManager, vec!["package_manager"]),
            ("conda.exe", "conda", Ecosystem::Python, ResourceKind::PackageManager, vec!["package_manager", "environment_manager"]),
            ("rustc.exe", "rustc", Ecosystem::Rust, ResourceKind::Compiler, vec!["compiler"]),
            ("cargo.exe", "cargo", Ecosystem::Rust, ResourceKind::Toolchain, vec!["build_tool"]),
            ("rustup.exe", "rustup", Ecosystem::Rust, ResourceKind::Toolchain, vec!["toolchain_manager"]),
            ("node.exe", "node", Ecosystem::NodeJs, ResourceKind::LanguageRuntime, vec!["interpreter"]),
            ("npm.cmd", "npm", Ecosystem::NodeJs, ResourceKind::PackageManager, vec!["package_manager"]),
            ("pnpm.cmd", "pnpm", Ecosystem::NodeJs, ResourceKind::PackageManager, vec!["package_manager"]),
            ("corepack.cmd", "corepack", Ecosystem::NodeJs, ResourceKind::PackageManager, vec!["package_manager"]),
            ("go.exe", "go", Ecosystem::Go, ResourceKind::Compiler, vec!["compiler", "build_tool"]),
            ("java.exe", "java", Ecosystem::Java, ResourceKind::LanguageRuntime, vec!["runtime"]),
            ("javac.exe", "javac", Ecosystem::Java, ResourceKind::Compiler, vec!["compiler"]),
            ("mvn.cmd", "maven", Ecosystem::Java, ResourceKind::Toolchain, vec!["build_tool"]),
            ("gradle.bat", "gradle", Ecosystem::Java, ResourceKind::Toolchain, vec!["build_tool"]),
            ("ruby.exe", "ruby", Ecosystem::Ruby, ResourceKind::LanguageRuntime, vec!["interpreter"]),
            ("gem.cmd", "gem", Ecosystem::Ruby, ResourceKind::PackageManager, vec!["package_manager"]),
            ("bundle.bat", "bundler", Ecosystem::Ruby, ResourceKind::PackageManager, vec!["package_manager"]),
            ("dotnet.exe", "dotnet", Ecosystem::DotNet, ResourceKind::Sdk, vec!["sdk", "runtime", "compiler"]),
            ("nuget.exe", "nuget", Ecosystem::DotNet, ResourceKind::PackageManager, vec!["package_manager"]),
            ("gcc.exe", "gcc", Ecosystem::Cpp, ResourceKind::Compiler, vec!["compiler"]),
            ("g++.exe", "g++", Ecosystem::Cpp, ResourceKind::Compiler, vec!["compiler"]),
            ("clang.exe", "clang", Ecosystem::Cpp, ResourceKind::Compiler, vec!["compiler"]),
            ("clang++.exe", "clang++", Ecosystem::Cpp, ResourceKind::Compiler, vec!["compiler"]),
            ("make.exe", "make", Ecosystem::Cpp, ResourceKind::Toolchain, vec!["build_tool"]),
            ("cmake.exe", "cmake", Ecosystem::Cpp, ResourceKind::Toolchain, vec!["build_tool"]),
            ("ninja.exe", "ninja", Ecosystem::Cpp, ResourceKind::Toolchain, vec!["build_tool"]),
            ("nvcc.exe", "nvcc", Ecosystem::Cpp, ResourceKind::Compiler, vec!["compiler", "cuda"]),
        ];

        for dir in std::env::split_paths(&path_var).take(128) {
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
        results.extend(self.scan_known_locations_report().tools);
    }

    /// Metadata-only, bounded discovery in installation directories, never home documents.
    pub fn scan_known_locations_report(&self) -> ToolchainDiscoveryReport {
        let mut roots = Vec::new();
        for key in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Some(value) = std::env::var_os(key) { roots.push(PathBuf::from(value)); }
        }
        if let Ok(local) = ulvo_adapters::user_data_root() {
            if let Some(parent) = local.parent() { roots.push(parent.join("Programs")); }
        }
        if let Some(profile) = std::env::var_os("USERPROFILE") { roots.push(PathBuf::from(profile).join("scoop/apps")); }
        roots.sort(); roots.dedup();
        let mut queue = roots.into_iter().map(|p| (p, 0usize)).collect::<Vec<_>>();
        let start = std::time::Instant::now();
        let mut report = ToolchainDiscoveryReport { tools: vec![], partial: false, inaccessible_paths: vec![], scanned_directories: 0 };
        let names = ["python", "python3", "uv", "pip", "conda", "cargo", "rustc", "rustup", "node", "npm", "npx", "pnpm", "go", "ruby", "gem", "java", "javac", "dotnet", "gcc", "g++", "clang", "clang++", "cmake", "ninja", "mvn", "gradle", "pacman", "nvcc", "git"];
        while let Some((directory, depth)) = queue.pop() {
            if report.scanned_directories >= 512 || start.elapsed() > std::time::Duration::from_secs(2) { report.partial = true; break; }
            if ulvo_adapters::reject_links(&directory).is_err() { continue; }
            let entries = match std::fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(_) => { if directory.exists() && report.inaccessible_paths.len() < 128 { report.inaccessible_paths.push(directory.to_string_lossy().into_owned()); report.partial = true; } continue; }
            };
            report.scanned_directories += 1;
            for (index, entry) in entries.take(2049).enumerate() {
                if index == 2048 { report.partial = true; break; }
                let entry = match entry { Ok(e) => e, Err(_) => { report.partial = true; continue; } };
                let path = entry.path();
                if ulvo_adapters::reject_links(&path).is_err() { continue; }
                let kind = match entry.file_type() { Ok(k) => k, Err(_) => { report.partial = true; continue; } };
                if kind.is_dir() {
                    if depth < 3 { queue.push((path, depth + 1)); } else { report.partial = true; }
                } else if kind.is_file() {
                    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                    if !names.contains(&stem.as_str()) || !["exe", "cmd", "bat"].contains(&extension.as_str()) { continue; }
                    let ecosystem = match stem.as_str() {
                        "python" | "python3" | "uv" | "pip" | "conda" => Ecosystem::Python,
                        "cargo" | "rustc" | "rustup" => Ecosystem::Rust,
                        "node" | "npm" | "npx" | "pnpm" => Ecosystem::NodeJs,
                        "go" => Ecosystem::Go, "ruby" | "gem" => Ecosystem::Ruby,
                        "java" | "javac" | "mvn" | "gradle" => Ecosystem::Java,
                        "dotnet" => Ecosystem::DotNet,
                        "gcc" | "g++" | "clang" | "clang++" | "nvcc" => Ecosystem::Cpp,
                        _ => Ecosystem::Generic(stem.clone()),
                    };
                    report.tools.push(DiscoveredToolchain { name: stem, executable_path: path, version: None, backend: Backend::Windows, ecosystem, kind: ResourceKind::Executable, ownership: Ownership::External, discovery_source: "bounded_known_location_metadata".into(), capabilities: vec!["present_unverified".into()] });
                }
            }
        }
        if !queue.is_empty() { report.partial = true; }
        report
    }
    fn scan_msys2(&self, results: &mut Vec<DiscoveredToolchain>) {
        let msys_root = std::env::var_os("SystemDrive").map(|drive| PathBuf::from(format!("{}\\msys64", drive.to_string_lossy()))).unwrap_or_default();
        if ulvo_adapters::reject_links(&msys_root).is_err() { return; }
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

    fn scan_vault_toolchains(&self, results: &mut Vec<DiscoveredToolchain>) {
        let rust_cargo = self.vault_root.join("runtimes").join("rust").join("cargo").join("bin");
        if rust_cargo.is_dir() {
            let rustc = rust_cargo.join("rustc.exe");
            let cargo = rust_cargo.join("cargo.exe");
            if rustc.is_file() {
                results.push(DiscoveredToolchain {
                    name: "rustc_managed".into(),
                    executable_path: rustc,
                    version: None,
                    backend: Backend::Windows,
                    ecosystem: Ecosystem::Rust,
                    kind: ResourceKind::Compiler,
                    ownership: Ownership::Managed,
                    discovery_source: "vault_runtimes_rust".into(),
                    capabilities: vec!["compiler".into()],
                });
            }
            if cargo.is_file() {
                results.push(DiscoveredToolchain {
                    name: "cargo_managed".into(),
                    executable_path: cargo,
                    version: None,
                    backend: Backend::Windows,
                    ecosystem: Ecosystem::Rust,
                    kind: ResourceKind::Toolchain,
                    ownership: Ownership::Managed,
                    discovery_source: "vault_runtimes_rust".into(),
                    capabilities: vec!["build_tool".into()],
                });
            }
        }

        let msys2_ucrt = self.vault_root.join("toolchains").join("msys2").join("ucrt64").join("bin");
        if msys2_ucrt.is_dir() {
            let gcc = msys2_ucrt.join("gcc.exe");
            let gpp = msys2_ucrt.join("g++.exe");
            if gcc.is_file() {
                results.push(DiscoveredToolchain {
                    name: "gcc_managed".into(),
                    executable_path: gcc,
                    version: None,
                    backend: Backend::Msys2,
                    ecosystem: Ecosystem::Cpp,
                    kind: ResourceKind::Compiler,
                    ownership: Ownership::Managed,
                    discovery_source: "vault_toolchains_msys2".into(),
                    capabilities: vec!["compiler".into()],
                });
            }
            if gpp.is_file() {
                results.push(DiscoveredToolchain {
                    name: "g++_managed".into(),
                    executable_path: gpp,
                    version: None,
                    backend: Backend::Msys2,
                    ecosystem: Ecosystem::Cpp,
                    kind: ResourceKind::Compiler,
                    ownership: Ownership::Managed,
                    discovery_source: "vault_toolchains_msys2".into(),
                    capabilities: vec!["compiler".into()],
                });
            }
        }

        let msys2_usr = self.vault_root.join("toolchains").join("msys2").join("usr").join("bin");
        if msys2_usr.is_dir() {
            let make = msys2_usr.join("make.exe");
            let pacman = msys2_usr.join("pacman.exe");
            if make.is_file() {
                results.push(DiscoveredToolchain {
                    name: "make_managed".into(),
                    executable_path: make,
                    version: None,
                    backend: Backend::Msys2,
                    ecosystem: Ecosystem::Cpp,
                    kind: ResourceKind::Toolchain,
                    ownership: Ownership::Managed,
                    discovery_source: "vault_toolchains_msys2".into(),
                    capabilities: vec!["build_tool".into()],
                });
            }
            if pacman.is_file() {
                results.push(DiscoveredToolchain {
                    name: "pacman_managed".into(),
                    executable_path: pacman,
                    version: None,
                    backend: Backend::Msys2,
                    ecosystem: Ecosystem::Cpp,
                    kind: ResourceKind::PackageManager,
                    ownership: Ownership::Managed,
                    discovery_source: "vault_toolchains_msys2".into(),
                    capabilities: vec!["package_manager".into()],
                });
            }
        }
    }

    fn scan_wsl_metadata(&self, results: &mut Vec<DiscoveredToolchain>) {
        // Read WSL distribution registrations from registry without launching any distributions
        // On Windows, WSL installs register subkeys under HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss
        let wsl_exe = ulvo_adapters::windows_root().unwrap_or_default().join("System32/wsl.exe");
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

    #[cfg(windows)]
    #[test]
    fn ownership_matches_extended_and_normal_paths() {
        let base = std::env::temp_dir().join(format!("ulvo_ownership_paths_{}", std::process::id()));
        let vault = base.join("vault");
        let bin = vault.join("toolchains/msys2/ucrt64/bin/gcc.exe");
        std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
        std::fs::write(&bin, b"fixture").unwrap();
        std::fs::write(vault.join("toolchains/msys2/.ulvo_managed.json"), b"{}").unwrap();
        let scanner = UniversalToolchainScanner::new(vault.canonicalize().unwrap());
        assert_eq!(scanner.verify_vault_ownership(&bin), Ownership::Managed);
        assert_eq!(scanner.verify_vault_ownership(&bin.canonicalize().unwrap()), Ownership::Managed);
        assert_eq!(scanner.verify_vault_ownership(&base.join("external.exe")), Ownership::External);
        assert_eq!(scanner.verify_vault_ownership(&vault.join("unowned.exe")), Ownership::Unknown);
    }

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
