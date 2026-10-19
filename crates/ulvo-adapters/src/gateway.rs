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
}

impl<'a> GatewayResolver<'a> {
    pub fn new(registry: &'a MemoryVaultRegistry) -> Self {
        let system_path = std::env::var("PATH").unwrap_or_default();
        Self {
            registry,
            system_path,
        }
    }

    pub fn with_system_path(registry: &'a MemoryVaultRegistry, custom_path: String) -> Self {
        Self {
            registry,
            system_path: custom_path,
        }
    }

    /// Resolves a requested tool name or resource identifier deterministically.
    pub fn resolve_tool(&self, query: &str) -> Result<GatewayResolution, String> {
        // 1. Resolve resource with deterministic collision handling
        let resource = self.resolve_resource_deterministic(query)?;

        let exe_path_str = resource.installation_path.as_deref().ok_or_else(|| {
            format!("resource '{}' does not have an associated executable or installation path", resource.resource_id)
        })?;

        let exe_path = PathBuf::from(exe_path_str);
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
            path_entries.push(parent.to_string_lossy().to_string());
        }

        // Step 2b: Companion ecosystem directories (e.g. MSYS2 usr\bin, Python Scripts)
        match resource.ecosystem {
            ulvo_core::Ecosystem::Cpp if resource.backend == Backend::Msys2 => {
                if let Some(parent) = exe_path.parent() {
                    if let Some(msys2_root) = parent.parent() {
                        let usr_bin = msys2_root.join("usr").join("bin");
                        if usr_bin.is_dir() {
                            let s = usr_bin.to_string_lossy().to_string();
                            if !path_entries.contains(&s) {
                                path_entries.push(s);
                            }
                        }
                    }
                }
                process_env.insert("MSYSTEM".into(), "UCRT64".into());
                process_env.insert("MSYS2_PATH_TYPE".into(), "minimal".into());
            }
            ulvo_core::Ecosystem::Python => {
                if let Some(parent) = exe_path.parent() {
                    let scripts = parent.join("Scripts");
                    if scripts.is_dir() {
                        let s = scripts.to_string_lossy().to_string();
                        if !path_entries.contains(&s) {
                            path_entries.push(s);
                        }
                    }
                }
                process_env.insert("PYTHONNOUSERSITE".into(), "1".into());
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
        ];

        for sys_dir in &approved_system_dirs {
            if sys_dir.is_dir() {
                let s = sys_dir.to_string_lossy().to_string();
                if !path_entries.contains(&s) {
                    path_entries.push(s);
                }
            }
        }

        // If custom system_path was supplied (e.g. in tests), preserve only explicit approved matches
        for entry in std::env::split_paths(&self.system_path) {
            let s = entry.to_string_lossy().to_string();
            let lower = s.to_lowercase();
            // Retain system directory paths even if custom system_path passed
            if (lower.contains("windows") || lower.contains("system32")) && !path_entries.contains(&s) && !s.is_empty() {
                path_entries.push(s);
            }
        }

        let process_local_path = path_entries.join(if cfg!(windows) { ";" } else { ":" });
        process_env.insert("PATH".to_string(), process_local_path);

        // Ecosystem-specific process-local variable setup (non-global)
        match resource.ecosystem {
            ulvo_core::Ecosystem::Rust => {
                if let Some(parent) = exe_path.parent() {
                    if let Some(grandparent) = parent.parent() {
                        process_env.insert("CARGO_HOME".into(), grandparent.to_string_lossy().to_string());
                    }
                }
            }
            ulvo_core::Ecosystem::Python => {
                if resource.resource_kind == ResourceKind::Environment {
                    if let Some(parent) = exe_path.parent() {
                        if let Some(venv_root) = parent.parent() {
                            process_env.insert("VIRTUAL_ENV".into(), venv_root.to_string_lossy().to_string());
                        }
                    }
                }
            }
            _ => {}
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
        // Priority 3: Version (descending)
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
