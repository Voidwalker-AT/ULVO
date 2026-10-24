use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use ulvo_core::{
    Abi, Architecture, Backend, Ecosystem, MemoryVaultRegistry, Ownership, Platform, Resource,
    ResourceKind, VaultRegistry, VerificationStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallationClassification {
    Managed,
    External,
    Adoptable,
    SystemManaged,
    Blocked,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdoptionAction {
    UseExisting,
    ProvisionManaged,
    PlanMigration,
    RetainExternal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredInstallation {
    pub name: String,
    pub ecosystem: Ecosystem,
    pub path: PathBuf,
    pub version: Option<String>,
    pub classification: InstallationClassification,
    pub recommended_action: AdoptionAction,
    pub explanation: String,
    pub relocatable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdoptionPlanReport {
    pub total_discovered: usize,
    pub managed_count: usize,
    pub adoptable_count: usize,
    pub system_managed_count: usize,
    pub external_count: usize,
    pub installations: Vec<DiscoveredInstallation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuTelemetry {
    pub name: String,
    pub driver_version: String,
    pub vram_total: String,
    pub vram_used: String,
    pub compute_capability: String,
    pub pytorch_compatible: Option<bool>,
    pub kernel_mode: String,
}

pub struct AdoptionService {
    vault_root: PathBuf,
}

impl AdoptionService {
    pub fn new<P: AsRef<Path>>(vault_root: P) -> Self {
        Self {
            vault_root: vault_root.as_ref().to_path_buf(),
        }
    }

    /// Queries GPU hardware and driver telemetry via nvidia-smi.
    pub fn get_gpu_telemetry(&self) -> Option<GpuTelemetry> {
        let executable = ulvo_adapters::trusted_nvidia_smi()?;
        if let Ok(out) = ulvo_adapters::run_bounded(&executable,
            &["--query-gpu=name,driver_version,memory.total,memory.used,compute_cap", "--format=csv,noheader"],
            std::time::Duration::from_secs(2)) {
            {
                let parts: Vec<&str> = out.trim().split(',').map(|s| s.trim()).collect();
                if parts.len() >= 5 {
                    return Some(GpuTelemetry {
                        name: parts[0].to_string(),
                        driver_version: parts[1].to_string(),
                        vram_total: parts[2].to_string(),
                        vram_used: parts[3].to_string(),
                        compute_capability: format!("sm_{}", parts[4].replace('.', "")),
                        pytorch_compatible: None,
                        kernel_mode: "System-managed; mode not queried".to_string(),
                    });
                }
            }
        }
        None
    }

    /// Discovers development tools installed across the system and classifies each entry safely.
    pub fn discover_system_installations(&self) -> AdoptionPlanReport {
        let mut installations = Vec::new();

        // 1. Check Python
        self.probe_executable("python", Ecosystem::Python, &mut installations);

        // 2. Check Rust / Cargo
        self.probe_executable("cargo", Ecosystem::Rust, &mut installations);
        self.probe_executable("rustc", Ecosystem::Rust, &mut installations);

        // 3. Check MSYS2 / GCC
        self.probe_executable("gcc", Ecosystem::Cpp, &mut installations);

        // 4. Check Node.js & npm / pnpm
        self.probe_executable("node", Ecosystem::NodeJs, &mut installations);
        self.probe_executable("pnpm", Ecosystem::NodeJs, &mut installations);

        // 5. Check Go
        self.probe_executable("go", Ecosystem::Go, &mut installations);

        // 6. Check Ruby
        self.probe_executable("ruby", Ecosystem::Ruby, &mut installations);

        // 7. Check Java / JDK
        self.probe_executable("javac", Ecosystem::Java, &mut installations);

        // 8. System Managed Check: NVIDIA GPU driver
        if let Some(Ok(output)) = ulvo_adapters::trusted_nvidia_smi().map(|path| ulvo_adapters::run_bounded(&path,
            &["--query-gpu=driver_version", "--format=csv,noheader"], std::time::Duration::from_secs(2)))
        {
            {
                let ver = output.trim().to_string();
                installations.push(DiscoveredInstallation {
                    name: "NVIDIA Display Driver & CUDA Kernel".into(),
                    ecosystem: Ecosystem::Python,
                    path: ulvo_adapters::windows_root().unwrap_or_default().join("System32/nvml.dll"),
                    version: Some(ver),
                    classification: InstallationClassification::SystemManaged,
                    recommended_action: AdoptionAction::RetainExternal,
                    explanation: "Host GPU display driver and kernel mode components are system-managed and must never be relocated.".into(),
                    relocatable: false,
                });
            }
        }

        let mut managed_count = 0;
        let mut adoptable_count = 0;
        let mut system_managed_count = 0;
        let mut external_count = 0;

        for inst in &installations {
            match inst.classification {
                InstallationClassification::Managed => managed_count += 1,
                InstallationClassification::Adoptable => adoptable_count += 1,
                InstallationClassification::SystemManaged => system_managed_count += 1,
                InstallationClassification::External => external_count += 1,
                _ => {}
            }
        }

        AdoptionPlanReport {
            total_discovered: installations.len(),
            managed_count,
            adoptable_count,
            system_managed_count,
            external_count,
            installations,
        }
    }

    fn probe_executable(&self, tool: &str, ecosystem: Ecosystem, out: &mut Vec<DiscoveredInstallation>) {
        let Some(where_exe) = ulvo_adapters::windows_root().ok().map(|p| p.join("System32/where.exe")) else { return };
        if let Ok(stdout) = ulvo_adapters::run_bounded(&where_exe, &[tool], std::time::Duration::from_secs(2)) {
            {
                for line in stdout.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let p = PathBuf::from(trimmed);
                    let ownership = ulvo_discovery::UniversalToolchainScanner::new(self.vault_root.clone()).verify_vault_ownership(&p);
                    let system = ulvo_adapters::windows_root().ok();
                    let (classification, action, explanation, relocatable) = if ownership == ulvo_core::Ownership::Managed {
                        (
                            InstallationClassification::Managed,
                            AdoptionAction::UseExisting,
                            "Fully managed within the ULVO vault root.".into(),
                            false,
                        )
                    } else if system.as_ref().map(|root| p.starts_with(root)).unwrap_or(false) {
                        (
                            InstallationClassification::SystemManaged,
                            AdoptionAction::RetainExternal,
                            "Windows operating system system32 binary; modification prohibited.".into(),
                            false,
                        )
                    } else if ownership == ulvo_core::Ownership::External {
                        (
                            InstallationClassification::External,
                            AdoptionAction::UseExisting,
                            "External reference only; consumers, relocatability and ownership have not been verified.".into(),
                            false,
                        )
                    } else {
                        (
                            InstallationClassification::External,
                            AdoptionAction::RetainExternal,
                            "External installation discovered on host PATH.".into(),
                            false,
                        )
                    };

                    out.push(DiscoveredInstallation {
                        name: tool.to_string(),
                        ecosystem: ecosystem.clone(),
                        path: p,
                        version: None,
                        classification,
                        recommended_action: action,
                        explanation,
                        relocatable,
                    });
                }
            }
        }
    }

    /// Registers an adoptable external tool into the Universal Registry under External Retained ownership.
    pub fn adopt_external_installation(&self, tool_name: &str, path: &Path, ecosystem: Ecosystem) -> Result<String, String> {
        if !path.exists() {
            return Err(format!("Cannot adopt nonexistent path: {}", path.display()));
        }

        let res = Resource::new(
            ResourceKind::Toolchain,
            ecosystem,
            tool_name,
            Some("external".into()),
            Platform::Windows,
            Architecture::X86_64,
            Backend::Windows,
            Abi::Gnu,
            Some(path.to_string_lossy().into_owned()),
            Some("host".into()),
            Ownership::External,
            VerificationStatus::Verified,
            None,
            vec!["adopted".into(), "external_retained".into()],
        );

        let res_id_str = res.resource_id.to_string();

        let mut reg = ulvo_discovery::discover_fresh_vault_registry(&self.vault_root)
            .unwrap_or_else(|_| MemoryVaultRegistry::new());
        reg.register(res).map_err(|e| format!("Registration error: {e}"))?;
        ulvo_discovery::save_durable_vault_registry(&self.vault_root, &reg, "ADOPT", "ulvo-services")?;

        Ok(res_id_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_installation_discovery() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_adoption");
        let svc = AdoptionService::new(&temp_dir);
        let rep = svc.discover_system_installations();
        assert_eq!(rep.total_discovered, rep.installations.len());
        assert!(rep.installations.iter().all(|i| !i.relocatable));
    }
}
