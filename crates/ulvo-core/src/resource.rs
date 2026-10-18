use serde::{Deserialize, Serialize};
use crate::{Backend, Ownership};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    LanguageRuntime,
    Compiler,
    Sdk,
    PackageManager,
    Toolchain,
    PackageStore,
    SharedLibrary,
    Environment,
    Executable,
    ExternalToolReference,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ecosystem {
    Rust,
    Python,
    NodeJs,
    Go,
    Java,
    Ruby,
    DotNet,
    Cpp,
    Generic(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Windows,
    Linux,
    Darwin,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    X86_64,
    X86,
    Aarch64,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Abi {
    Gnu,
    Msvc,
    Musl,
    None,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Discovered,
    Verified,
    Missing,
    Modified,
    Conflicted,
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Provenance {
    pub source_type: String,
    pub canonical_url: Option<String>,
    pub expected_sha256: Option<String>,
    pub actual_sha256: Option<String>,
    pub manifest_path: Option<String>,
    pub notes: Option<String>,
}

pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn system_time_to_rfc3339(time: std::time::SystemTime) -> String {
    let dur = time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = dur.as_secs();
    let z = (secs / 86400) as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let sec_in_day = secs % 86400;
    let hour = sec_in_day / 3600;
    let min = (sec_in_day % 3600) / 60;
    let sec = sec_in_day % 60;
    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}Z")
}

pub fn current_time_rfc3339() -> String {
    system_time_to_rfc3339(std::time::SystemTime::now())
}

pub fn normalize_path_for_platform(path: &str, platform: &Platform) -> String {
    let p = path.replace('\\', "/");
    let trimmed = p.trim_end_matches('/');
    match platform {
        Platform::Windows => trimmed.to_lowercase(),
        _ => trimmed.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceId(pub String);

impl ResourceId {
    pub fn new(
        ecosystem: &Ecosystem,
        backend: &Backend,
        platform: &Platform,
        architecture: &Architecture,
        abi: &Abi,
        kind: &ResourceKind,
        name: &str,
        version: Option<&str>,
        installation_path: Option<&str>,
    ) -> Self {
        let path_hash = match installation_path {
            Some(p) => {
                let norm = normalize_path_for_platform(p, platform);
                format!("{:016x}", fnv1a_64(norm.as_bytes()))
            }
            None => "unanchored".to_string(),
        };
        let ver = version.unwrap_or("unknown");
        let id_str = format!(
            "ulvo::{ecosystem:?}::{backend:?}::{platform:?}-{architecture:?}-{abi:?}::{kind:?}::{name}@{ver}#{path_hash}"
        ).to_lowercase();
        ResourceId(id_str)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Resource {
    pub resource_id: ResourceId,
    pub resource_kind: ResourceKind,
    pub ecosystem: Ecosystem,
    pub name: String,
    pub version: Option<String>,
    pub platform: Platform,
    pub architecture: Architecture,
    pub backend: Backend,
    pub abi: Abi,
    pub installation_path: Option<String>,
    pub manager: Option<String>,
    pub ownership: Ownership,
    pub verification_status: VerificationStatus,
    pub provenance: Option<Provenance>,
    pub capabilities: Vec<String>,
    pub created_at: Option<String>,
    pub last_verified_at: Option<String>,
}

impl Resource {
    pub fn new(
        resource_kind: ResourceKind,
        ecosystem: Ecosystem,
        name: impl Into<String>,
        version: Option<String>,
        platform: Platform,
        architecture: Architecture,
        backend: Backend,
        abi: Abi,
        installation_path: Option<String>,
        manager: Option<String>,
        ownership: Ownership,
        verification_status: VerificationStatus,
        provenance: Option<Provenance>,
        capabilities: Vec<String>,
    ) -> Self {
        let name = name.into();
        let resource_id = ResourceId::new(
            &ecosystem,
            &backend,
            &platform,
            &architecture,
            &abi,
            &resource_kind,
            &name,
            version.as_deref(),
            installation_path.as_deref(),
        );
        Self {
            resource_id,
            resource_kind,
            ecosystem,
            name,
            version,
            platform,
            architecture,
            backend,
            abi,
            installation_path,
            manager,
            ownership,
            verification_status,
            provenance,
            capabilities,
            created_at: None,
            last_verified_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_resource_id_same_inputs() {
        let id1 = ResourceId::new(
            &Ecosystem::Rust,
            &Backend::Windows,
            &Platform::Windows,
            &Architecture::X86_64,
            &Abi::Gnu,
            &ResourceKind::Compiler,
            "rustc",
            Some("1.96.0"),
            Some("E:\\Workspace\\Active\\ULVO\\vault\\runtimes\\rust\\bin\\rustc.exe"),
        );
        let id2 = ResourceId::new(
            &Ecosystem::Rust,
            &Backend::Windows,
            &Platform::Windows,
            &Architecture::X86_64,
            &Abi::Gnu,
            &ResourceKind::Compiler,
            "rustc",
            Some("1.96.0"),
            Some("e:/workspace/active/ulvo/vault/runtimes/rust/bin/rustc.exe"),
        );
        assert_eq!(id1, id2, "Windows paths must normalize to identical resource IDs regardless of slashes/case");
    }

    #[test]
    fn backend_separation_prevents_collision() {
        let win = ResourceId::new(
            &Ecosystem::Rust,
            &Backend::Windows,
            &Platform::Windows,
            &Architecture::X86_64,
            &Abi::Gnu,
            &ResourceKind::Compiler,
            "rustc",
            Some("1.96.0"),
            Some("C:/rust/rustc.exe"),
        );
        let wsl = ResourceId::new(
            &Ecosystem::Rust,
            &Backend::Wsl,
            &Platform::Linux,
            &Architecture::X86_64,
            &Abi::Gnu,
            &ResourceKind::Compiler,
            "rustc",
            Some("1.96.0"),
            Some("/usr/bin/rustc"),
        );
        assert_ne!(win, wsl, "Windows and WSL resources must not collide");
    }

    #[test]
    fn architecture_separation_prevents_collision() {
        let x64 = ResourceId::new(
            &Ecosystem::Rust,
            &Backend::Windows,
            &Platform::Windows,
            &Architecture::X86_64,
            &Abi::Gnu,
            &ResourceKind::Compiler,
            "rustc",
            Some("1.96.0"),
            None,
        );
        let aarch64 = ResourceId::new(
            &Ecosystem::Rust,
            &Backend::Windows,
            &Platform::Windows,
            &Architecture::Aarch64,
            &Abi::Gnu,
            &ResourceKind::Compiler,
            "rustc",
            Some("1.96.0"),
            None,
        );
        assert_ne!(x64, aarch64, "x86_64 and Aarch64 must have distinct IDs");
    }

    #[test]
    fn abi_separation_prevents_collision() {
        let gnu = ResourceId::new(
            &Ecosystem::Rust,
            &Backend::Windows,
            &Platform::Windows,
            &Architecture::X86_64,
            &Abi::Gnu,
            &ResourceKind::Compiler,
            "rustc",
            Some("1.96.0"),
            None,
        );
        let msvc = ResourceId::new(
            &Ecosystem::Rust,
            &Backend::Windows,
            &Platform::Windows,
            &Architecture::X86_64,
            &Abi::Msvc,
            &ResourceKind::Compiler,
            "rustc",
            Some("1.96.0"),
            None,
        );
        assert_ne!(gnu, msvc, "GNU and MSVC ABIs must have distinct IDs");
    }

    #[test]
    fn linux_paths_preserve_casing() {
        let norm1 = normalize_path_for_platform("/opt/MyTools/bin", &Platform::Linux);
        let norm2 = normalize_path_for_platform("/opt/mytools/bin", &Platform::Linux);
        assert_ne!(norm1, norm2, "Linux path normalization must preserve case");
    }
}
