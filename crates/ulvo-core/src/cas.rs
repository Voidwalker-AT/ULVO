use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use crate::resource::{Abi, Architecture, Ecosystem, Platform};

/// Cryptographically verified SHA-256 artifact digest.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CasArtifactId(pub String);

impl CasArtifactId {
    pub fn new(sha256_hex: &str) -> Result<Self, String> {
        let clean = sha256_hex.trim().to_lowercase();
        if clean.len() != 64 || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("invalid SHA-256 hex string: '{clean}'"));
        }
        Ok(CasArtifactId(clean))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CasArtifactId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Category of content-addressable artifact in the ULVO CAS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CasArtifactKind {
    /// Python wheel or sdist
    PythonWheel,
    /// npm/pnpm package tarball
    NodePackage,
    /// Rust cargo crate archive (.crate)
    CargoCrate,
    /// Go module zip archive
    GoModule,
    /// Java jar or war archive
    JavaArchive,
    /// Ruby gem archive (.gem)
    RubyGem,
    /// MSYS2 / pacman pkg tarball (.pkg.tar.zst)
    PacmanPackage,
    /// Native binary executable or DLL
    NativeBinary,
    /// Generic immutable file payload
    GenericPayload,
}

/// Descriptor of an immutable package artifact stored in the CAS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CasArtifactDescriptor {
    pub artifact_id: CasArtifactId,
    pub kind: CasArtifactKind,
    pub ecosystem: Ecosystem,
    pub package_name: String,
    pub version: String,
    pub platform: Platform,
    pub architecture: Architecture,
    pub abi: Abi,
    #[serde(default)]
    pub runtime_requirements: Vec<String>,
    pub package_source: String,
    pub integrity_sha256: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub native_compatibility: Option<String>,
    pub storage_relative_path: String,
    pub immutable: bool,
    pub created_at_unix: u64,
    #[serde(default)]
    pub consumers: Vec<String>,
}

impl CasArtifactDescriptor {
    pub fn is_compatible(
        &self,
        ecosystem: &Ecosystem,
        platform: &Platform,
        arch: &Architecture,
        abi: &Abi,
        native_tag: Option<&str>,
    ) -> bool {
        if &self.ecosystem != ecosystem {
            return false;
        }
        // Strict platform check: Unknown must NOT be treated as verified compatibility
        if matches!(self.platform, Platform::Unknown(_)) || matches!(platform, Platform::Unknown(_)) {
            return false;
        }
        if &self.platform != platform {
            return false;
        }

        // Strict architecture check: Unknown must NOT be treated as verified compatibility
        if matches!(self.architecture, Architecture::Unknown(_)) || matches!(arch, Architecture::Unknown(_)) {
            return false;
        }
        if &self.architecture != arch {
            return false;
        }

        // Strict ABI check: Unknown must NOT be treated as verified compatibility
        if matches!(self.abi, Abi::Unknown(_)) || matches!(abi, Abi::Unknown(_)) {
            return false;
        }
        if &self.abi != abi && self.abi != Abi::None && abi != &Abi::None {
            return false;
        }

        // Strict Native/CUDA compatibility check:
        match (native_tag, &self.native_compatibility) {
            (Some(req), Some(actual)) => {
                if !actual.eq_ignore_ascii_case(req) {
                    return false;
                }
            }
            (Some(_req), None) => {
                // Requested GPU/native tag, but artifact is CPU-only / generic
                return false;
            }
            (None, Some(_actual)) => {
                // Consumer requested CPU (None), but artifact is built for a specific GPU/native tag
                return false;
            }
            (None, None) => {}
        }

        true
    }
}

/// Ledger of reference tracking for garbage-collection and consumer safety.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CasReferenceTracker {
    pub references: BTreeMap<String, Vec<String>>,
}

impl CasReferenceTracker {
    pub fn new() -> Self {
        Self {
            references: BTreeMap::new(),
        }
    }

    pub fn add_reference(&mut self, artifact_id: &str, consumer_id: &str) {
        let consumers = self.references.entry(artifact_id.to_string()).or_default();
        if !consumers.contains(&consumer_id.to_string()) {
            consumers.push(consumer_id.to_string());
        }
    }

    pub fn remove_reference(&mut self, artifact_id: &str, consumer_id: &str) -> usize {
        if let Some(consumers) = self.references.get_mut(artifact_id) {
            consumers.retain(|c| c != consumer_id);
            let remaining = consumers.len();
            if remaining == 0 {
                self.references.remove(artifact_id);
            }
            remaining
        } else {
            0
        }
    }

    pub fn reference_count(&self, artifact_id: &str) -> usize {
        self.references.get(artifact_id).map(|c| c.len()).unwrap_or(0)
    }

    pub fn is_eligible_for_gc(&self, artifact_id: &str) -> bool {
        self.reference_count(artifact_id) == 0
    }
}

/// Candidate artifact identified during GC planning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CasGcCandidate {
    pub artifact_id: String,
    pub package_name: String,
    pub version: String,
    pub size_bytes: u64,
    pub storage_relative_path: String,
}

/// Non-destructive, verifiable GC plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CasGcPlan {
    pub schema_version: u32,
    pub dry_run: bool,
    pub candidates_for_reclamation: Vec<CasGcCandidate>,
    pub preserved_artifacts: Vec<(String, String)>, // (artifact_id, reason)
    pub total_reclaimable_bytes: u64,
    pub active_leases_checked: usize,
    pub active_processes_checked: usize,
    pub clean_to_execute: bool,
    pub blocking_reasons: Vec<String>,
}

/// Configuration and telemetry for a coordinated native package cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeCacheCoordinator {
    pub ecosystem: Ecosystem,
    pub manager_name: String,
    pub relative_vault_path: String,
    pub total_allocated_bytes: u64,
    pub item_count: usize,
    pub is_active: bool,
    pub diagnostic: Option<String>,
}

/// Top-level manifest for the Universal Content-Addressable Dependency Store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CasManifest {
    pub schema_version: u32,
    pub vault_root: String,
    pub total_artifacts: usize,
    #[serde(alias = "total_bytes", default)]
    pub unique_cas_bytes: u64,
    #[serde(default)]
    pub total_logical_bytes: u64,
    #[serde(default)]
    pub allocated_ntfs_bytes: u64,
    pub artifacts: BTreeMap<String, CasArtifactDescriptor>,
    pub reference_tracker: CasReferenceTracker,
    #[serde(default)]
    pub native_coordinators: BTreeMap<String, NativeCacheCoordinator>,
}

impl CasManifest {
    pub fn new(vault_root: String) -> Self {
        Self {
            schema_version: 1,
            vault_root,
            total_artifacts: 0,
            unique_cas_bytes: 0,
            total_logical_bytes: 0,
            allocated_ntfs_bytes: 0,
            artifacts: BTreeMap::new(),
            reference_tracker: CasReferenceTracker::new(),
            native_coordinators: BTreeMap::new(),
        }
    }

    /// Backwards-compatible accessor for unique_cas_bytes.
    pub fn total_bytes(&self) -> u64 {
        self.unique_cas_bytes
    }

    /// Recalculates logical bytes and verifies invariant accounting.
    pub fn recompute_logical_bytes(&mut self) {
        self.total_artifacts = self.artifacts.len();
        let mut unique: u64 = 0;
        let mut logical: u64 = 0;
        for (hash, art) in &self.artifacts {
            unique = unique.saturating_add(art.size_bytes);
            let refs = self.reference_tracker.reference_count(hash).max(1) as u64;
            logical = logical.saturating_add(art.size_bytes.saturating_mul(refs));
        }
        self.unique_cas_bytes = unique;
        self.total_logical_bytes = logical;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cas_artifact_id_validation() {
        let valid_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let id = CasArtifactId::new(valid_hash);
        assert!(id.is_ok());
        assert_eq!(id.unwrap().as_str(), valid_hash);

        // Invalid length
        assert!(CasArtifactId::new("e3b0c442").is_err());
        // Non-hex
        assert!(CasArtifactId::new(&"z".repeat(64)).is_err());
    }

    #[test]
    fn test_cas_reference_tracker() {
        let mut tracker = CasReferenceTracker::new();
        let art_id = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

        assert!(tracker.is_eligible_for_gc(art_id));

        tracker.add_reference(art_id, "project_a");
        tracker.add_reference(art_id, "project_b");
        assert_eq!(tracker.reference_count(art_id), 2);
        assert!(!tracker.is_eligible_for_gc(art_id));

        // Adding duplicate reference does not artificially inflate count
        tracker.add_reference(art_id, "project_a");
        assert_eq!(tracker.reference_count(art_id), 2);

        // Remove project_a
        assert_eq!(tracker.remove_reference(art_id, "project_a"), 1);
        assert!(!tracker.is_eligible_for_gc(art_id));

        // Remove project_b
        assert_eq!(tracker.remove_reference(art_id, "project_b"), 0);
        assert!(tracker.is_eligible_for_gc(art_id));
    }

    #[test]
    fn test_cas_artifact_compatibility() {
        let desc = CasArtifactDescriptor {
            artifact_id: CasArtifactId::new("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855").unwrap(),
            kind: CasArtifactKind::PythonWheel,
            ecosystem: Ecosystem::Python,
            package_name: "torch".into(),
            version: "2.5.1+cu124".into(),
            platform: Platform::Windows,
            architecture: Architecture::X86_64,
            abi: Abi::None,
            runtime_requirements: vec!["python>=3.11".into()],
            package_source: "pypi".into(),
            integrity_sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
            size_bytes: 2_000_000_000,
            native_compatibility: Some("cuda_sm_86".into()),
            storage_relative_path: "packages/cas/objects/e3/e3b0...".into(),
            immutable: true,
            created_at_unix: 1700000000,
            consumers: vec![],
        };

        // Compatible check
        assert!(desc.is_compatible(&Ecosystem::Python, &Platform::Windows, &Architecture::X86_64, &Abi::None, Some("cuda_sm_86")));

        // Incompatible CUDA
        assert!(!desc.is_compatible(&Ecosystem::Python, &Platform::Windows, &Architecture::X86_64, &Abi::None, Some("cuda_sm_75")));

        // Incompatible OS
        assert!(!desc.is_compatible(&Ecosystem::Python, &Platform::Linux, &Architecture::X86_64, &Abi::None, None));

        // Unknown platform rejected (fail-closed)
        assert!(!desc.is_compatible(&Ecosystem::Python, &Platform::Unknown("solaris".into()), &Architecture::X86_64, &Abi::None, Some("cuda_sm_86")));

        // Unknown architecture rejected (fail-closed)
        assert!(!desc.is_compatible(&Ecosystem::Python, &Platform::Windows, &Architecture::Unknown("sparc".into()), &Abi::None, Some("cuda_sm_86")));

        // Unknown ABI rejected (fail-closed)
        assert!(!desc.is_compatible(&Ecosystem::Python, &Platform::Windows, &Architecture::X86_64, &Abi::Unknown("eabi".into()), Some("cuda_sm_86")));

        // Consumer requested CPU (None), but artifact is GPU-enabled -> REJECTED
        assert!(!desc.is_compatible(&Ecosystem::Python, &Platform::Windows, &Architecture::X86_64, &Abi::None, None));

        // Consumer requested GPU (cuda_sm_86), but artifact is CPU-only -> REJECTED
        let mut cpu_desc = desc.clone();
        cpu_desc.native_compatibility = None;
        assert!(!cpu_desc.is_compatible(&Ecosystem::Python, &Platform::Windows, &Architecture::X86_64, &Abi::None, Some("cuda_sm_86")));
        // CPU artifact with CPU request -> PASS
        assert!(cpu_desc.is_compatible(&Ecosystem::Python, &Platform::Windows, &Architecture::X86_64, &Abi::None, None));
    }

    #[test]
    fn test_cas_manifest_idempotent_accounting() {
        let mut manifest = CasManifest::new("E:\\vault".into());
        let hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let desc = CasArtifactDescriptor {
            artifact_id: CasArtifactId::new(hash).unwrap(),
            kind: CasArtifactKind::GenericPayload,
            ecosystem: Ecosystem::Python,
            package_name: "test-pkg".into(),
            version: "1.0.0".into(),
            platform: Platform::Windows,
            architecture: Architecture::X86_64,
            abi: Abi::None,
            runtime_requirements: vec![],
            package_source: "local".into(),
            integrity_sha256: hash.into(),
            size_bytes: 50_000,
            native_compatibility: None,
            storage_relative_path: format!("packages/cas/objects/e3/{hash}"),
            immutable: true,
            created_at_unix: 1700000000,
            consumers: vec!["proj_1".into()],
        };

        manifest.artifacts.insert(hash.into(), desc.clone());
        manifest.reference_tracker.add_reference(hash, "proj_1");
        manifest.recompute_logical_bytes();

        assert_eq!(manifest.unique_cas_bytes, 50_000);
        assert_eq!(manifest.total_logical_bytes, 50_000);

        // Add second consumer reference (logical increases, unique remains identical)
        manifest.reference_tracker.add_reference(hash, "proj_2");
        manifest.recompute_logical_bytes();
        assert_eq!(manifest.unique_cas_bytes, 50_000);
        assert_eq!(manifest.total_logical_bytes, 100_000);

        // Re-inserting the same artifact description does NOT double unique bytes
        manifest.artifacts.insert(hash.into(), desc);
        manifest.recompute_logical_bytes();
        assert_eq!(manifest.unique_cas_bytes, 50_000);
        assert_eq!(manifest.total_logical_bytes, 100_000);
    }

    #[test]
    fn sha256_standard_test_vectors() {
        assert_eq!(
            format!("{:x}", Sha256::digest(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// Cryptographic SHA-256 32-byte digest wrapper with hexadecimal formatting.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sha256Digest(pub [u8; 32]);

impl Sha256Digest {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Display for Sha256Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:x}", self)
    }
}

impl std::fmt::Debug for Sha256Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sha256Digest({:x})", self)
    }
}

impl std::fmt::LowerHex for Sha256Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in &self.0 {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

/// Pure-Rust, zero-dependency streaming cryptographic SHA-256 hasher (FIPS 180-4).
#[derive(Clone)]
pub struct Sha256 {
    h: [u32; 8],
    len: u64,
    buffer: [u8; 64],
    buf_len: usize,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    pub fn new() -> Self {
        Self {
            h: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
            ],
            len: 0,
            buffer: [0u8; 64],
            buf_len: 0,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        self.len += data.len() as u64;
        let mut offset = 0;
        while offset < data.len() {
            let space = 64 - self.buf_len;
            let take = (data.len() - offset).min(space);
            self.buffer[self.buf_len..self.buf_len + take].copy_from_slice(&data[offset..offset + take]);
            self.buf_len += take;
            offset += take;
            if self.buf_len == 64 {
                let block = self.buffer;
                self.process_block(&block);
                self.buf_len = 0;
            }
        }
    }

    fn process_block(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = self.h[0];
        let mut b = self.h[1];
        let mut c = self.h[2];
        let mut d = self.h[3];
        let mut e = self.h[4];
        let mut f = self.h[5];
        let mut g = self.h[6];
        let mut h = self.h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
        self.h[5] = self.h[5].wrapping_add(f);
        self.h[6] = self.h[6].wrapping_add(g);
        self.h[7] = self.h[7].wrapping_add(h);
    }

    pub fn finalize(mut self) -> Sha256Digest {
        let bit_len = self.len * 8;
        let mut pad = [0u8; 128];
        pad[0] = 0x80;
        let pad_len = if self.buf_len < 56 {
            56 - self.buf_len
        } else {
            120 - self.buf_len
        };
        let mut offset = 0;
        while offset < pad_len {
            let space = 64 - self.buf_len;
            let take = (pad_len - offset).min(space);
            self.buffer[self.buf_len..self.buf_len + take].copy_from_slice(&pad[offset..offset + take]);
            self.buf_len += take;
            offset += take;
            if self.buf_len == 64 {
                let block = self.buffer;
                self.process_block(&block);
                self.buf_len = 0;
            }
        }
        let len_bytes = bit_len.to_be_bytes();
        self.buffer[56..64].copy_from_slice(&len_bytes);
        let block = self.buffer;
        self.process_block(&block);

        let mut out = [0u8; 32];
        for (i, h_val) in self.h.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&h_val.to_be_bytes());
        }
        Sha256Digest(out)
    }

    pub fn digest(data: &[u8]) -> Sha256Digest {
        let mut hasher = Self::new();
        hasher.update(data);
        hasher.finalize()
    }
}
