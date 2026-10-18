use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

/// Represents CUDA Compute Capability (e.g., sm_86 for Ampere GA107 / RTX 2050).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CudaComputeCapability {
    pub major: u32,
    pub minor: u32,
    pub arch_name: String,
}

impl CudaComputeCapability {
    pub fn new(major: u32, minor: u32) -> Self {
        Self {
            major,
            minor,
            arch_name: format!("sm_{major}{minor}"),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        let s = trimmed.strip_prefix("sm_").or_else(|| trimmed.strip_prefix("compute_")).unwrap_or(trimmed);
        if s.contains('.') {
            let mut parts = s.split('.');
            let maj = parts.next()?.parse().ok()?;
            let min = parts.next()?.parse().ok()?;
            Some(Self::new(maj, min))
        } else if s.len() == 2 && s.chars().all(|c| c.is_ascii_digit()) {
            let maj = s[0..1].parse().ok()?;
            let min = s[1..2].parse().ok()?;
            Some(Self::new(maj, min))
        } else {
            None
        }
    }

    pub fn is_compatible_with(&self, min_required: &CudaComputeCapability) -> bool {
        if self.major > min_required.major {
            true
        } else if self.major == min_required.major {
            self.minor >= min_required.minor
        } else {
            false
        }
    }
}

/// Represents the host NVIDIA GPU display driver version.
/// Distinguishes system-managed GPU drivers from user-space runtimes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NvidiaDriverVersion {
    pub raw_version: String,
    pub major: u32,
    pub minor: u32,
    pub max_cuda_support: String,
}

impl NvidiaDriverVersion {
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        let parts: Vec<&str> = trimmed.split('.').collect();
        if parts.is_empty() {
            return None;
        }
        let major = parts[0].parse().ok()?;
        let minor = if parts.len() > 1 { parts[1].parse().unwrap_or(0) } else { 0 };

        // Conservative mapping of major driver version to maximum supported CUDA version
        let max_cuda = if major >= 570 {
            "13.2".to_string()
        } else if major >= 550 {
            "12.4".to_string()
        } else if major >= 535 {
            "12.2".to_string()
        } else if major >= 525 {
            "12.0".to_string()
        } else if major >= 515 {
            "11.7".to_string()
        } else if major >= 470 {
            "11.4".to_string()
        } else {
            "10.2".to_string()
        };

        Some(Self {
            raw_version: trimmed.to_string(),
            major,
            minor,
            max_cuda_support: max_cuda,
        })
    }
}

/// A physical GPU device detected on the host system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuDevice {
    pub index: u32,
    pub name: String,
    pub vendor: String,
    pub total_vram_bytes: u64,
    pub compute_capability: CudaComputeCapability,
    pub pci_bus_id: Option<String>,
    pub uuid: Option<String>,
}

/// System-managed GPU driver state.
/// ULVO strictly prohibits relocating or copying kernel drivers into the vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemGpuDriver {
    pub driver_version: NvidiaDriverVersion,
    pub devices: Vec<GpuDevice>,
    pub is_system_managed: bool,
    pub kernel_driver_relocation_prohibited: bool,
}

impl SystemGpuDriver {
    pub fn new(driver_version: NvidiaDriverVersion, devices: Vec<GpuDevice>) -> Self {
        Self {
            driver_version,
            devices,
            is_system_managed: true,
            kernel_driver_relocation_prohibited: true,
        }
    }
}

/// Supported Deep Learning Frameworks.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DlFramework {
    PyTorch,
    TensorFlow,
    Jax,
    OnnxRuntime,
    Other(String),
}

/// Framework build variant (CPU-only vs GPU/CUDA-enabled build).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkVariant {
    pub framework: DlFramework,
    pub version: String,
    pub build_variant: String, // e.g. "+cu124", "+cu118", "+cpu"
    pub is_gpu_enabled: bool,
    pub cuda_runtime_version: Option<String>,
}

impl FrameworkVariant {
    pub fn parse_torch_version(raw: &str) -> Self {
        let trimmed = raw.trim();
        let parts: Vec<&str> = trimmed.split('+').collect();
        let version = parts[0].to_string();
        let build_variant = if parts.len() > 1 {
            format!("+{}", parts[1])
        } else {
            "".to_string()
        };

        let is_gpu = build_variant.starts_with("+cu") || build_variant.starts_with("+rocm");
        let cuda_runtime = if build_variant.starts_with("+cu") {
            let cu_str = &build_variant[3..];
            if cu_str.len() >= 3 {
                Some(format!("{}.{}", &cu_str[..2], &cu_str[2..]))
            } else {
                Some(cu_str.to_string())
            }
        } else {
            None
        };

        Self {
            framework: DlFramework::PyTorch,
            version,
            build_variant,
            is_gpu_enabled: is_gpu,
            cuda_runtime_version: cuda_runtime,
        }
    }
}

/// Native dynamic-link library dependency for CUDA workloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CudaRuntimeDependency {
    pub dll_name: String,
    pub required: bool,
    pub found: bool,
    pub resolved_path: Option<String>,
    pub file_size_bytes: Option<u64>,
    pub file_sha256: Option<String>,
}

/// Native wheel platform tag (e.g., cp311-cp311-win_amd64).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WheelPlatformTag {
    pub python_tag: String,
    pub abi_tag: String,
    pub platform_tag: String,
}

impl WheelPlatformTag {
    pub fn parse(tag_str: &str) -> Option<Self> {
        let parts: Vec<&str> = tag_str.split('-').collect();
        if parts.len() != 3 {
            return None;
        }
        Some(Self {
            python_tag: parts[0].to_string(),
            abi_tag: parts[1].to_string(),
            platform_tag: parts[2].to_string(),
        })
    }

    pub fn is_compatible_with(&self, target_py_ver: &str, target_abi: &str, target_platform: &str) -> bool {
        let py_compat = self.python_tag == "py3"
            || self.python_tag == "py2.py3"
            || self.python_tag.contains(target_py_ver);
        let abi_compat = self.abi_tag == "none"
            || self.abi_tag == "abi3"
            || self.abi_tag == target_abi;
        let plat_compat = self.platform_tag == "any"
            || self.platform_tag.eq_ignore_ascii_case(target_platform);

        py_compat && abi_compat && plat_compat
    }
}

/// Package provenance tracking origin and immutability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageProvenance {
    pub package_name: String,
    pub package_version: String,
    pub source_url: Option<String>,
    pub installer: String,
    pub wheel_filename: Option<String>,
    pub sha256: Option<String>,
    pub is_shared_vault_artifact: bool,
}

/// Dependency specification fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyFingerprint {
    pub specification_hash: String,
    pub pinned_packages: BTreeMap<String, String>,
    pub extra_index_urls: Vec<String>,
}

/// Storage capacity verdict for materializing GPU environments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityVerdict {
    SufficientSpace {
        available_bytes: u64,
        required_bytes: u64,
        remaining_after_bytes: u64,
    },
    BlockedCapacity {
        available_bytes: u64,
        required_bytes: u64,
        safety_reserve_bytes: u64,
        message: String,
    },
}

/// Overall GPU environment compatibility verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuCompatibilityVerdict {
    Compatible,
    IncompatibleDriver(String),
    IncompatibleComputeCapability(String),
    IncompatibleFrameworkVariant(String),
    MissingRuntimeDll(String),
    BlockedCapacity(String),
    NoGpuAvailable,
}

/// Comprehensive GPU compatibility assessment report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuCompatibilityAssessment {
    pub schema_version: u32,
    pub project_name: String,
    pub verdict: GpuCompatibilityVerdict,
    pub gpu_device: Option<GpuDevice>,
    pub host_driver: Option<NvidiaDriverVersion>,
    pub requested_framework: Option<FrameworkVariant>,
    pub detected_dependencies: Vec<CudaRuntimeDependency>,
    pub capacity_verdict: CapacityVerdict,
    pub notes: Vec<String>,
}

/// GPU manifest configuration in `.ulvo/manifest.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GpuManifestConfig {
    pub required: Option<bool>,
    pub framework: Option<String>,
    pub framework_version: Option<String>,
    pub cuda_version: Option<String>,
    pub min_compute_capability: Option<String>,
    pub extra_index_url: Option<String>,
    pub find_links: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_capability_comparison() {
        let sm_86 = CudaComputeCapability::new(8, 6);
        let sm_75 = CudaComputeCapability::new(7, 5);
        let sm_80 = CudaComputeCapability::new(8, 0);
        let sm_89 = CudaComputeCapability::new(8, 9);

        assert!(sm_86.is_compatible_with(&sm_75));
        assert!(sm_86.is_compatible_with(&sm_80));
        assert!(sm_86.is_compatible_with(&sm_86));
        assert!(!sm_86.is_compatible_with(&sm_89));
    }

    #[test]
    fn test_compute_capability_parsing() {
        let a = CudaComputeCapability::parse("sm_86").unwrap();
        assert_eq!(a.major, 8);
        assert_eq!(a.minor, 6);
        assert_eq!(a.arch_name, "sm_86");

        let b = CudaComputeCapability::parse("8.6").unwrap();
        assert_eq!(b.major, 8);
        assert_eq!(b.minor, 6);

        let c = CudaComputeCapability::parse("compute_75").unwrap();
        assert_eq!(c.major, 7);
        assert_eq!(c.minor, 5);
    }

    #[test]
    fn test_nvidia_driver_version_parsing() {
        let driver = NvidiaDriverVersion::parse("596.49").unwrap();
        assert_eq!(driver.major, 596);
        assert_eq!(driver.minor, 49);
        assert_eq!(driver.max_cuda_support, "13.2");

        let legacy = NvidiaDriverVersion::parse("470.82").unwrap();
        assert_eq!(legacy.major, 470);
        assert_eq!(legacy.max_cuda_support, "11.4");
    }

    #[test]
    fn test_framework_variant_parsing() {
        let cu = FrameworkVariant::parse_torch_version("2.6.0+cu124");
        assert_eq!(cu.version, "2.6.0");
        assert_eq!(cu.build_variant, "+cu124");
        assert!(cu.is_gpu_enabled);
        assert_eq!(cu.cuda_runtime_version.as_deref(), Some("12.4"));

        let cpu = FrameworkVariant::parse_torch_version("2.6.0+cpu");
        assert_eq!(cpu.version, "2.6.0");
        assert_eq!(cpu.build_variant, "+cpu");
        assert!(!cpu.is_gpu_enabled);
        assert!(cpu.cuda_runtime_version.is_none());
    }

    #[test]
    fn test_wheel_platform_tag_compatibility() {
        let tag = WheelPlatformTag::parse("cp311-cp311-win_amd64").unwrap();
        assert!(tag.is_compatible_with("cp311", "cp311", "win_amd64"));
        assert!(!tag.is_compatible_with("cp312", "cp312", "win_amd64"));
        assert!(!tag.is_compatible_with("cp311", "cp311", "linux_x86_64"));

        let universal = WheelPlatformTag::parse("py3-none-any").unwrap();
        assert!(universal.is_compatible_with("cp311", "cp311", "win_amd64"));
        assert!(universal.is_compatible_with("cp312", "cp312", "linux_x86_64"));
    }

    #[test]
    fn test_system_gpu_driver_relocation_prohibited() {
        let driver = SystemGpuDriver::new(
            NvidiaDriverVersion::parse("596.49").unwrap(),
            vec![GpuDevice {
                index: 0,
                name: "NVIDIA GeForce RTX 2050".into(),
                vendor: "NVIDIA".into(),
                total_vram_bytes: 4 * 1024 * 1024 * 1024,
                compute_capability: CudaComputeCapability::new(8, 6),
                pci_bus_id: Some("0000:01:00.0".into()),
                uuid: None,
            }],
        );

        assert!(driver.is_system_managed);
        assert!(driver.kernel_driver_relocation_prohibited);
    }
}
