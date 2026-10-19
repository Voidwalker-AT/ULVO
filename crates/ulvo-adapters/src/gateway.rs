use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;
use ulvo_core::{
    Backend, MemoryVaultRegistry, Ownership, Resource, ResourceKind, VaultRegistry,
    VerificationStatus,
};

/// Result of resolving a tool through the ULVO Unified Execution Gateway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayResolution {
    pub requested_tool: String,
    pub resolved_resource_id: String,
    pub executable_path: PathBuf,
    pub ownership: Ownership,
    pub backend: Backend,
    pub process_env: BTreeMap<String, String>,
}

/// Unified Execution Gateway Resolver.
/// Resolves requested development tools using Vault Registry identities and builds
/// clean process-local environments without polluting or modifying global system state.
pub struct GatewayResolver<'a> {
    registry: &'a MemoryVaultRegistry,
    system_path: String,
    vault_root: Option<PathBuf>,
}

impl<'a> GatewayResolver<'a> {
    pub fn new(registry: &'a MemoryVaultRegistry) -> Self {
        let system_path = std::env::var("PATH").unwrap_or_default();
        let vault_root = std::env::var("ULVO_VAULT_ROOT").ok().map(PathBuf::from).or_else(|| {
            registry.list().iter().find_map(|r| {
                if r.ownership == ulvo_core::Ownership::Managed {
                    if let Some(ref p) = r.installation_path {
                        let pb = PathBuf::from(p);
                        if let Some(pos) = pb.to_string_lossy().find("vault") {
                            let candidate = PathBuf::from(&pb.to_string_lossy()[..pos + 5]);
                            if candidate.is_dir() {
                                return Some(candidate);
                            }
                        }
                    }
                }
                None
            })
        });
        Self {
            registry,
            system_path,
            vault_root,
        }
    }

    pub fn with_system_path(registry: &'a MemoryVaultRegistry, custom_path: String) -> Self {
        let mut r = Self::new(registry);
        r.system_path = custom_path;
        r
    }

    pub fn with_vault_root(mut self, vault_root: PathBuf) -> Self {
        self.vault_root = Some(vault_root);
        self
    }

    /// Resolves a requested tool name or resource identifier deterministically.
    pub fn resolve_tool(&self, query: &str) -> Result<GatewayResolution, String> {
        // 1. Resolve resource with deterministic collision handling
        let resource = self.resolve_resource_deterministic(query)?;

        let exe_path_str = resource.installation_path.as_deref().ok_or_else(|| {
            format!("resource '{}' does not have an associated executable or installation path", resource.resource_id)
        })?;

        let raw_exe = PathBuf::from(exe_path_str);
        let exe_path = ulvo_core::strip_unc_prefix(&raw_exe);
        if !exe_path.is_file() {
            return Err(format!(
                "resolved executable for '{}' does not exist at '{}'",
                resource.name,
                exe_path.display()
            ));
        }

        // 2. Construct clean, backend-specific isolated process environment
        let mut process_env = BTreeMap::new();
        let mut path_entries = Vec::new();

        // Step 2a: Tool's own parent directory at head of PATH
        if let Some(parent) = exe_path.parent() {
            let p_clean = ulvo_core::strip_unc_prefix(parent);
            path_entries.push(p_clean.to_string_lossy().to_string());
        }

        // Step 2b: Companion ecosystem directories (e.g. MSYS2 usr\bin, Python Scripts)
        match resource.ecosystem {
            ulvo_core::Ecosystem::Cpp if resource.backend == Backend::Msys2 => {
                let mut msys2_root = exe_path.clone();
                let mut subsystem = "UCRT64".to_string();

                if let Some(parent) = exe_path.parent() {
                    let parent_name = parent.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if parent_name.eq_ignore_ascii_case("bin") {
                        if let Some(sub_dir) = parent.parent() {
                            let sub_name = sub_dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
                            if sub_name.eq_ignore_ascii_case("usr") {
                                subsystem = "MSYS".into();
                                msys2_root = sub_dir.parent().unwrap_or(sub_dir).to_path_buf();
                            } else if sub_name.eq_ignore_ascii_case("ucrt64") {
                                subsystem = "UCRT64".into();
                                msys2_root = sub_dir.parent().unwrap_or(sub_dir).to_path_buf();
                            } else if sub_name.eq_ignore_ascii_case("mingw64") {
                                subsystem = "MINGW64".into();
                                msys2_root = sub_dir.parent().unwrap_or(sub_dir).to_path_buf();
                            } else if sub_name.eq_ignore_ascii_case("clang64") {
                                subsystem = "CLANG64".into();
                                msys2_root = sub_dir.parent().unwrap_or(sub_dir).to_path_buf();
                            } else {
                                msys2_root = sub_dir.to_path_buf();
                            }
                        }
                    }
                }

                let ucrt_bin = ulvo_core::strip_unc_prefix(&msys2_root.join("ucrt64").join("bin"));
                let usr_bin = ulvo_core::strip_unc_prefix(&msys2_root.join("usr").join("bin"));
                if ucrt_bin.is_dir() {
                    let s = ucrt_bin.to_string_lossy().to_string();
                    if !path_entries.contains(&s) {
                        path_entries.push(s);
                    }
                }
                if usr_bin.is_dir() {
                    let s = usr_bin.to_string_lossy().to_string();
                    if !path_entries.contains(&s) {
                        path_entries.push(s);
                    }
                }

                process_env.insert("MSYSTEM".into(), subsystem);
                process_env.insert("MSYS2_PATH_TYPE".into(), "minimal".into());
                if let Ok(temp) = std::env::var("TEMP") { process_env.insert("TEMP".into(), temp); }
                if let Ok(tmp) = std::env::var("TMP") { process_env.insert("TMP".into(), tmp); }
                if let Ok(profile) = std::env::var("USERPROFILE") { process_env.insert("USERPROFILE".into(), profile); }
                if let Ok(drive) = std::env::var("SystemDrive") { process_env.insert("SystemDrive".into(), drive); }
            }
            ulvo_core::Ecosystem::Python => {
                if let Some(parent) = exe_path.parent() {
                    let scripts = parent.join("Scripts");
                    if scripts.is_dir() {
                        let s = ulvo_core::strip_unc_prefix(&scripts).to_string_lossy().to_string();
                        if !path_entries.contains(&s) {
                            path_entries.push(s);
                        }
                    }
                }
                process_env.insert("PYTHONNOUSERSITE".into(), "1".into());
            }
            ulvo_core::Ecosystem::Rust => {
                let p_str = exe_path.to_string_lossy();
                if p_str.contains("runtimes\\rust") || p_str.contains("runtimes/rust") {
                    if let Some(pos) = p_str.find("runtimes") {
                        let rust_root = PathBuf::from(&p_str[..pos]).join("runtimes").join("rust");
                        process_env.insert("CARGO_HOME".into(), rust_root.join("cargo").to_string_lossy().to_string());
                        process_env.insert("RUSTUP_HOME".into(), rust_root.join("rustup").to_string_lossy().to_string());
                    }
                }
            }
            ulvo_core::Ecosystem::Ruby => {
                if let Ok(temp) = std::env::var("TEMP") { process_env.insert("TEMP".into(), temp); }
                if let Ok(tmp) = std::env::var("TMP") { process_env.insert("TMP".into(), tmp); }
                if let Ok(profile) = std::env::var("USERPROFILE") { process_env.insert("USERPROFILE".into(), profile); }
                if let Ok(drive) = std::env::var("SystemDrive") { process_env.insert("SystemDrive".into(), drive); }
                // Companion MSYS2 UCRT64 toolchain directory for compiling native Ruby gem C extensions
                if let Some(ref vr) = self.vault_root {
                    let msys_ucrt = vr.join("toolchains").join("msys2").join("ucrt64").join("bin");
                    if msys_ucrt.is_dir() {
                        let s = msys_ucrt.to_string_lossy().to_string();
                        if !path_entries.contains(&s) { path_entries.push(s); }
                    }
                }
                let fallback_msys = PathBuf::from(r"C:\msys64\ucrt64\bin");
                if fallback_msys.is_dir() {
                    let s = fallback_msys.to_string_lossy().to_string();
                    if !path_entries.contains(&s) { path_entries.push(s); }
                }
            }
            ulvo_core::Ecosystem::Go => {
                if let Some(parent) = exe_path.parent() {
                    if parent.file_name().and_then(|n| n.to_str()) == Some("bin") {
                        if let Some(goroot) = parent.parent() {
                            process_env.insert("GOROOT".into(), goroot.to_string_lossy().to_string());
                        }
                    }
                }
                // Managed Go module cache under vault
                if let Some(ref vr) = self.vault_root {
                    let vault_go_mod = vr.join("packages").join("go").join("mod");
                    if vault_go_mod.is_dir() {
                        process_env.insert("GOMODCACHE".into(), vault_go_mod.to_string_lossy().to_string());
                    }
                }
                if let Ok(temp) = std::env::var("TEMP") { process_env.insert("TEMP".into(), temp); }
                if let Ok(tmp) = std::env::var("TMP") { process_env.insert("TMP".into(), tmp); }
                if let Ok(profile) = std::env::var("USERPROFILE") { process_env.insert("USERPROFILE".into(), profile); }
                if let Ok(drive) = std::env::var("SystemDrive") { process_env.insert("SystemDrive".into(), drive); }
            }
            ulvo_core::Ecosystem::Java => {
                if let Some(parent) = exe_path.parent() {
                    if parent.file_name().and_then(|n| n.to_str()) == Some("bin") {
                        if let Some(java_home) = parent.parent() {
                            process_env.insert("JAVA_HOME".into(), java_home.to_string_lossy().to_string());
                        }
                    }
                }
                if let Ok(temp) = std::env::var("TEMP") { process_env.insert("TEMP".into(), temp); }
                if let Ok(tmp) = std::env::var("TMP") { process_env.insert("TMP".into(), tmp); }
                if let Ok(profile) = std::env::var("USERPROFILE") { process_env.insert("USERPROFILE".into(), profile); }
                if let Ok(drive) = std::env::var("SystemDrive") { process_env.insert("SystemDrive".into(), drive); }
            }
            ulvo_core::Ecosystem::NodeJs => {
                if let Ok(temp) = std::env::var("TEMP") { process_env.insert("TEMP".into(), temp); }
                if let Ok(tmp) = std::env::var("TMP") { process_env.insert("TMP".into(), tmp); }
                if let Ok(profile) = std::env::var("USERPROFILE") { process_env.insert("USERPROFILE".into(), profile); }
                if let Ok(appdata) = std::env::var("APPDATA") { process_env.insert("APPDATA".into(), appdata); }
                if let Ok(localappdata) = std::env::var("LOCALAPPDATA") { process_env.insert("LOCALAPPDATA".into(), localappdata); }
                if let Ok(drive) = std::env::var("SystemDrive") { process_env.insert("SystemDrive".into(), drive); }
                if let Some(ref vr) = self.vault_root {
                    let pnpm_store = vr.join("packages").join("node").join("pnpm-store");
                    if pnpm_store.is_dir() {
                        process_env.insert("PNPM_HOME".into(), pnpm_store.to_string_lossy().to_string());
                    }
                }
            }
            _ => {}
        }

        // Step 2c: Retain approved essential Windows system directories ONLY (for DLL resolution and OS services)
        let sys_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        let approved_system_dirs = [
            PathBuf::from(&sys_root).join("System32"),
            PathBuf::from(&sys_root),
            PathBuf::from(&sys_root).join("System32").join("Wbem"),
            PathBuf::from(&sys_root).join("System32").join("WindowsPowerShell").join("v1.0"),
            PathBuf::from("C:\\Windows\\System32"),
            PathBuf::from("C:\\Windows"),
            PathBuf::from("C:\\System32"),
        ];

        for sys_dir in &approved_system_dirs {
            if sys_dir.is_dir() {
                let s = sys_dir.to_string_lossy().to_string();
                if !path_entries.contains(&s) {
                    path_entries.push(s);
                }
            }
        }

        // Strictly preserve only canonical matches to approved system directories (reject arbitrary paths containing words)
        for entry in std::env::split_paths(&self.system_path) {
            let is_approved = approved_system_dirs.iter().any(|appr| {
                appr.to_string_lossy().eq_ignore_ascii_case(&entry.to_string_lossy())
                    || (entry.canonicalize().is_ok() && appr.canonicalize().is_ok() && entry.canonicalize().unwrap() == appr.canonicalize().unwrap())
            });
            if is_approved {
                let s = entry.to_string_lossy().to_string();
                if !path_entries.contains(&s) {
                    path_entries.push(s);
                }
            }
        }

        let process_local_path = path_entries.join(if cfg!(windows) { ";" } else { ":" });
        process_env.insert("PATH".to_string(), process_local_path);

        // Ecosystem-specific process-local variable setup (non-global)
        if resource.ecosystem == ulvo_core::Ecosystem::Python && resource.resource_kind == ResourceKind::Environment {
            if let Some(parent) = exe_path.parent() {
                if let Some(venv_root) = parent.parent() {
                    process_env.insert("VIRTUAL_ENV".into(), venv_root.to_string_lossy().to_string());
                }
            }
        }

        Ok(GatewayResolution {
            requested_tool: query.to_string(),
            resolved_resource_id: resource.resource_id.0.clone(),
            executable_path: exe_path,
            ownership: resource.ownership.clone(),
            backend: resource.backend.clone(),
            process_env,
        })
    }

    /// Resolves a resource identifier with deterministic collision handling:
    /// 1. Exact ResourceId match
    /// 2. Managed ownership over External/Unknown
    /// 3. Backend preference
    /// 4. Highest semver version
    fn resolve_resource_deterministic(&self, query: &str) -> Result<&Resource, String> {
        let key = ulvo_core::ResourceId(query.to_string());
        if let Some(r) = self.registry.get(&key) {
            return Ok(r);
        }

        // Case-insensitive exact name matches
        let mut candidates: Vec<&Resource> = self.registry.list().into_iter().filter(|r| {
            r.name.eq_ignore_ascii_case(query) || r.resource_id.0.eq_ignore_ascii_case(query)
        }).collect();

        if candidates.is_empty() {
            // Substring search
            candidates = self.registry.list().into_iter().filter(|r| {
                r.resource_id.0.contains(query) || r.name.to_lowercase().contains(&query.to_lowercase())
            }).collect();
        }

        if candidates.is_empty() {
            return Err(format!("tool '{query}' could not be resolved in the ULVO Vault Registry"));
        }

        if candidates.len() == 1 {
            return Ok(candidates[0]);
        }

        // Deterministic collision ranking:
        // Priority 1: Managed > External > Unknown
        // Priority 2: Verified > Discovered > Unverified
        // Priority 3: Version (numeric descending)
        candidates.sort_by(|a, b| {
            let ownership_rank = |o: &Ownership| match o {
                Ownership::Managed => 3,
                Ownership::External => 2,
                Ownership::Experimental => 1,
                Ownership::Unknown => 0,
            };
            let status_rank = |s: &VerificationStatus| match s {
                VerificationStatus::Verified => 3,
                VerificationStatus::Discovered => 2,
                _ => 1,
            };

            let ord_own = ownership_rank(&b.ownership).cmp(&ownership_rank(&a.ownership));
            if ord_own != std::cmp::Ordering::Equal {
                return ord_own;
            }

            let ord_stat = status_rank(&b.verification_status).cmp(&status_rank(&a.verification_status));
            if ord_stat != std::cmp::Ordering::Equal {
                return ord_stat;
            }

            let parse_ver = |ver_opt: &Option<String>| -> (u64, u64, u64) {
                if let Some(v) = ver_opt {
                    let parts: Vec<u64> = v
                        .split(['.', '-', '+'])
                        .filter_map(|s| s.parse::<u64>().ok())
                        .collect();
                    (
                        parts.get(0).copied().unwrap_or(0),
                        parts.get(1).copied().unwrap_or(0),
                        parts.get(2).copied().unwrap_or(0),
                    )
                } else {
                    (0, 0, 0)
                }
            };
            let ord_ver = parse_ver(&b.version).cmp(&parse_ver(&a.version));
            if ord_ver != std::cmp::Ordering::Equal {
                return ord_ver;
            }

            b.resource_id.0.cmp(&a.resource_id.0)
        });

        Ok(candidates[0])
    }

    /// Constructs a `std::process::Command` pre-configured with the process-local environment.
    pub fn build_command(&self, tool_query: &str, args: &[String]) -> Result<Command, String> {
        let resolution = self.resolve_tool(tool_query)?;

        let mut cmd = Command::new(&resolution.executable_path);
        cmd.args(args);

        // Apply constructed process-local environment variables
        for (k, v) in &resolution.process_env {
            cmd.env(k, v);
        }

        Ok(cmd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ulvo_core::{
        Abi, Architecture, Ecosystem, Platform, Resource, ResourceKind,
        VaultRegistry, VerificationStatus,
    };

    #[test]
    fn test_resolve_tool_constructs_process_local_path() {
        let mut reg = MemoryVaultRegistry::new();

        // Create temporary dummy executable for testing
        let temp_dir = std::env::temp_dir().join("ulvo_gateway_test");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let dummy_exe = temp_dir.join("test_tool.exe");
        std::fs::write(&dummy_exe, b"dummy").unwrap();

        let resource = Resource::new(
            ResourceKind::Compiler,
            Ecosystem::Rust,
            "rustc",
            Some("1.85.0".into()),
            Platform::Windows,
            Architecture::X86_64,
            Backend::Windows,
            Abi::Gnu,
            Some(dummy_exe.to_string_lossy().to_string()),
            Some("rustup".into()),
            Ownership::External,
            VerificationStatus::Verified,
            None,
            vec!["compiler".into()],
        );

        reg.register(resource).unwrap();

        let resolver = GatewayResolver::with_system_path(&reg, "C:\\System32;C:\\Windows".into());
        let res = resolver.resolve_tool("rustc").unwrap();

        assert_eq!(res.requested_tool, "rustc");
        assert_eq!(res.ownership, Ownership::External);
        assert_eq!(res.executable_path, dummy_exe);

        // Verify that PATH in process_env begins with the tool's directory
        let local_path = res.process_env.get("PATH").unwrap();
        assert!(local_path.starts_with(&temp_dir.to_string_lossy().to_string()));
        assert!(local_path.contains("C:\\System32"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_resolve_tool_retains_external_ownership() {
        let mut reg = MemoryVaultRegistry::new();
        let temp_dir = std::env::temp_dir().join("ulvo_gateway_external");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let dummy_exe = temp_dir.join("gcc.exe");
        std::fs::write(&dummy_exe, b"dummy").unwrap();

        let resource = Resource::new(
            ResourceKind::Compiler,
            Ecosystem::Cpp,
            "gcc",
            Some("14.2.0".into()),
            Platform::Windows,
            Architecture::X86_64,
            Backend::Msys2,
            Abi::Gnu,
            Some(dummy_exe.to_string_lossy().to_string()),
            Some("pacman".into()),
            Ownership::External,
            VerificationStatus::Verified,
            None,
            vec!["compiler".into()],
        );
        reg.register(resource).unwrap();

        let resolver = GatewayResolver::new(&reg);
        let res = resolver.resolve_tool("gcc").unwrap();
        assert_eq!(res.ownership, Ownership::External);
        assert_ne!(res.ownership, Ownership::Managed);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_dirty_path_entries_not_inherited() {
        let mut reg = MemoryVaultRegistry::new();
        let temp_dir = std::env::temp_dir().join("ulvo_gateway_clean_path");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let dummy_exe = temp_dir.join("clean_tool.exe");
        std::fs::write(&dummy_exe, b"dummy").unwrap();

        let resource = Resource::new(
            ResourceKind::Executable,
            Ecosystem::Generic("tool".into()),
            "clean_tool",
            Some("1.0.0".into()),
            Platform::Windows,
            Architecture::X86_64,
            Backend::Windows,
            Abi::None,
            Some(dummy_exe.to_string_lossy().to_string()),
            None,
            Ownership::External,
            VerificationStatus::Verified,
            None,
            vec!["tool".into()],
        );
        reg.register(resource).unwrap();

        // Pass a dirty PATH containing random user directories
        let dirty_path = "C:\\dirty_user\\bin;C:\\random_vendor\\tools;C:\\Windows\\System32";
        let resolver = GatewayResolver::with_system_path(&reg, dirty_path.into());
        let res = resolver.resolve_tool("clean_tool").unwrap();

        let process_path = res.process_env.get("PATH").unwrap();
        // Tool directory must be present
        assert!(process_path.contains(&temp_dir.to_string_lossy().to_string()));
        // Windows system directory must be retained for DLL resolution
        assert!(process_path.contains("System32"));
        // Dirty arbitrary entries must NOT be inherited
        assert!(!process_path.contains("dirty_user"));
        assert!(!process_path.contains("random_vendor"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_deterministic_collision_resolution() {
        let mut reg = MemoryVaultRegistry::new();
        let temp_dir = std::env::temp_dir().join("ulvo_gateway_collision");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let ext_exe = temp_dir.join("gcc_ext.exe");
        let mng_exe = temp_dir.join("gcc_mng.exe");
        std::fs::write(&ext_exe, b"dummy").unwrap();
        std::fs::write(&mng_exe, b"dummy").unwrap();

        let ext_res = Resource::new(
            ResourceKind::Compiler,
            Ecosystem::Cpp,
            "gcc",
            Some("14.1.0".into()),
            Platform::Windows,
            Architecture::X86_64,
            Backend::Windows,
            Abi::Gnu,
            Some(ext_exe.to_string_lossy().to_string()),
            Some("external".into()),
            Ownership::External,
            VerificationStatus::Verified,
            None,
            vec!["compiler".into()],
        );

        let mng_res = Resource::new(
            ResourceKind::Compiler,
            Ecosystem::Cpp,
            "gcc",
            Some("14.2.0".into()),
            Platform::Windows,
            Architecture::X86_64,
            Backend::Msys2,
            Abi::Gnu,
            Some(mng_exe.to_string_lossy().to_string()),
            Some("ulvo".into()),
            Ownership::Managed,
            VerificationStatus::Verified,
            Some(ulvo_core::Provenance {
                source_type: "vault_managed_toolchain".into(),
                canonical_url: None,
                expected_sha256: None,
                actual_sha256: None,
                manifest_path: None,
                notes: None,
            }),
            vec!["compiler".into()],
        );

        reg.register(ext_res).unwrap();
        reg.register(mng_res).unwrap();

        let resolver = GatewayResolver::new(&reg);
        let res = resolver.resolve_tool("gcc").unwrap();

        // Deterministic collision resolution must prioritize Managed over External!
        assert_eq!(res.ownership, Ownership::Managed);
        assert_eq!(res.executable_path, mng_exe);
        assert_eq!(res.backend, Backend::Msys2);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
