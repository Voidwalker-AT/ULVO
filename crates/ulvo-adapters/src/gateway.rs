use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;
use ulvo_core::{Backend, MemoryVaultRegistry, Ownership, ResourceKind};

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

    /// Resolves a requested tool name or resource identifier.
    pub fn resolve_tool(&self, query: &str) -> Result<GatewayResolution, String> {
        // 1. First attempt exact or substring lookup in VaultRegistry
        let resource = self.registry.get_by_str(query).ok_or_else(|| {
            format!("tool '{query}' could not be resolved in the ULVO Vault Registry")
        })?;

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

        // 2. Construct process-local environment
        let mut process_env = BTreeMap::new();

        // Process-local PATH construction:
        // Place the directory containing the resolved executable at the head of PATH.
        let mut path_entries = Vec::new();
        if let Some(parent) = exe_path.parent() {
            path_entries.push(parent.to_string_lossy().to_string());
        }

        // Append non-conflicting existing PATH entries
        for entry in std::env::split_paths(&self.system_path) {
            let s = entry.to_string_lossy().to_string();
            if !path_entries.contains(&s) && !s.is_empty() {
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
}
