use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use ulvo_core::{
    CasArtifactDescriptor, CasArtifactId, CasManifest, Ecosystem,
    NativeCacheCoordinator, Sha256,
};
use crate::{bounded_read, write_atomic, EnvironmentFileLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CasGcReport {
    pub unreferenced_artifacts_found: usize,
    pub artifacts_reclaimed: usize,
    pub physical_bytes_reclaimed: u64,
    pub remaining_artifacts: usize,
    pub remaining_bytes: u64,
}

pub struct CasStorageManager {
    vault_root: PathBuf,
    cas_dir: PathBuf,
    objects_dir: PathBuf,
    staging_dir: PathBuf,
    manifest_path: PathBuf,
    lock_path: PathBuf,
}

impl CasStorageManager {
    pub fn new(vault_root: PathBuf) -> Result<Self, String> {
        let clean_vault = ulvo_core::strip_unc_prefix(&vault_root);
        let cas_dir = clean_vault.join("packages").join("cas");
        let objects_dir = cas_dir.join("objects");
        let staging_dir = cas_dir.join(".staging");
        let manifest_path = cas_dir.join("manifest.json");
        let lock_path = cas_dir.join(".cas.lock");

        std::fs::create_dir_all(&objects_dir).map_err(|e| format!("cannot create CAS objects dir: {e}"))?;
        std::fs::create_dir_all(&staging_dir).map_err(|e| format!("cannot create CAS staging dir: {e}"))?;

        Ok(Self {
            vault_root: clean_vault,
            cas_dir,
            objects_dir,
            staging_dir,
            manifest_path,
            lock_path,
        })
    }

    pub fn cas_directory(&self) -> &Path {
        &self.cas_dir
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    /// Loads the CAS manifest or creates a default empty ledger.
    pub fn load_manifest(&self) -> Result<CasManifest, String> {
        if !self.manifest_path.is_file() {
            return Ok(CasManifest::new(self.vault_root.to_string_lossy().into_owned()));
        }
        let content = bounded_read(&self.manifest_path)?;
        serde_json::from_str(&content).map_err(|e| format!("corrupted CAS manifest at '{}': {e}", self.manifest_path.display()))
    }

    /// Saves the CAS manifest atomically under lock.
    pub fn save_manifest(&self, manifest: &CasManifest) -> Result<(), String> {
        let json = serde_json::to_string_pretty(manifest)
            .map_err(|e| format!("failed to serialize CAS manifest: {e}"))?;
        write_atomic(&self.manifest_path, json.as_bytes())
    }

    /// Stores raw payload bytes into the CAS object store with SHA-256 verification.
    pub fn store_payload(
        &self,
        mut descriptor: CasArtifactDescriptor,
        data: &[u8],
    ) -> Result<CasArtifactId, String> {
        let _lock = EnvironmentFileLock::acquire(&self.lock_path, 5000)?;

        let mut hasher = Sha256::new();
        hasher.update(data);
        let actual_hash = format!("{:x}", hasher.finalize());

        if !descriptor.integrity_sha256.is_empty() && !descriptor.integrity_sha256.eq_ignore_ascii_case(&actual_hash) {
            return Err(format!(
                "CAS_CORRUPTION_DETECTED: Staging payload checksum '{}' does not match expected integrity hash '{}'",
                actual_hash, descriptor.integrity_sha256
            ));
        }

        let artifact_id = CasArtifactId::new(&actual_hash)?;
        descriptor.artifact_id = artifact_id.clone();
        descriptor.integrity_sha256 = actual_hash.clone();
        descriptor.size_bytes = data.len() as u64;

        // Path: objects/<prefix>/<full_sha256>
        let prefix = &actual_hash[..2];
        let target_dir = self.objects_dir.join(prefix);
        std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

        let final_path = target_dir.join(&actual_hash);
        let rel_path = format!("packages/cas/objects/{prefix}/{actual_hash}");
        descriptor.storage_relative_path = rel_path;

        if !final_path.exists() {
            // Write atomically via staging directory
            let staging_file = self.staging_dir.join(format!("stage_{}_{}", actual_hash, std::process::id()));
            write_atomic(&staging_file, data)?;
            std::fs::rename(&staging_file, &final_path)
                .map_err(|e| format!("failed to publish artifact to CAS: {e}"))?;
        }

        // Update manifest
        let mut manifest = self.load_manifest()?;
        manifest.total_bytes += descriptor.size_bytes;
        manifest.artifacts.insert(actual_hash.clone(), descriptor);
        manifest.total_artifacts = manifest.artifacts.len();
        self.save_manifest(&manifest)?;

        Ok(artifact_id)
    }

    /// Stores an existing file into the CAS object store via streaming hash and copy/hardlink.
    pub fn store_file(
        &self,
        mut descriptor: CasArtifactDescriptor,
        source_path: &Path,
    ) -> Result<CasArtifactId, String> {
        if !source_path.is_file() {
            return Err(format!("source path is not a regular file: {}", source_path.display()));
        }

        let mut file = File::open(source_path).map_err(|e| format!("cannot read source file: {e}"))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        let mut total_bytes = 0u64;

        loop {
            let n = file.read(&mut buffer).map_err(|e| format!("error hashing file: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            total_bytes += n as u64;
        }

        let actual_hash = format!("{:x}", hasher.finalize());

        if !descriptor.integrity_sha256.is_empty() && !descriptor.integrity_sha256.eq_ignore_ascii_case(&actual_hash) {
            return Err(format!(
                "CAS_CORRUPTION_DETECTED: File checksum '{}' does not match expected '{}'",
                actual_hash, descriptor.integrity_sha256
            ));
        }

        let artifact_id = CasArtifactId::new(&actual_hash)?;
        descriptor.artifact_id = artifact_id.clone();
        descriptor.integrity_sha256 = actual_hash.clone();
        descriptor.size_bytes = total_bytes;

        let _lock = EnvironmentFileLock::acquire(&self.lock_path, 5000)?;

        let prefix = &actual_hash[..2];
        let target_dir = self.objects_dir.join(prefix);
        std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

        let final_path = target_dir.join(&actual_hash);
        let rel_path = format!("packages/cas/objects/{prefix}/{actual_hash}");
        descriptor.storage_relative_path = rel_path;

        if !final_path.exists() {
            // Attempt atomic hardlink if on same filesystem, fallback to copy
            if std::fs::hard_link(source_path, &final_path).is_err() {
                std::fs::copy(source_path, &final_path)
                    .map_err(|e| format!("failed to copy file to CAS: {e}"))?;
            }
        }

        let mut manifest = self.load_manifest()?;
        manifest.total_bytes += descriptor.size_bytes;
        manifest.artifacts.insert(actual_hash.clone(), descriptor);
        manifest.total_artifacts = manifest.artifacts.len();
        self.save_manifest(&manifest)?;

        Ok(artifact_id)
    }

    /// Verifies the cryptographic integrity of a stored artifact.
    pub fn verify_artifact_integrity(&self, artifact_id: &str) -> Result<bool, String> {
        let manifest = self.load_manifest()?;
        let descriptor = manifest.artifacts.get(artifact_id).ok_or_else(|| {
            format!("artifact '{artifact_id}' not found in CAS manifest")
        })?;

        let object_path = self.vault_root.join(&descriptor.storage_relative_path);
        if !object_path.is_file() {
            return Ok(false);
        }

        let mut file = File::open(&object_path).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];

        loop {
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        let actual = format!("{:x}", hasher.finalize());
        Ok(actual.eq_ignore_ascii_case(&descriptor.integrity_sha256))
    }

    /// Links an artifact into a target consumer environment with verified reference tracking.
    pub fn link_artifact_to_environment(
        &self,
        artifact_id: &str,
        target_path: &Path,
        consumer_id: &str,
    ) -> Result<(), String> {
        let _lock = EnvironmentFileLock::acquire(&self.lock_path, 5000)?;

        let mut manifest = self.load_manifest()?;
        let descriptor = manifest.artifacts.get(artifact_id).ok_or_else(|| {
            format!("cannot link: artifact '{artifact_id}' not found in CAS")
        })?.clone();

        let object_path = self.vault_root.join(&descriptor.storage_relative_path);
        if !object_path.is_file() {
            return Err(format!("CAS payload missing at '{}'", object_path.display()));
        }

        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        if target_path.exists() {
            let _ = std::fs::remove_file(target_path);
        }

        // Hardlink or copy
        if std::fs::hard_link(&object_path, target_path).is_err() {
            std::fs::copy(&object_path, target_path).map_err(|e| {
                format!("failed to link CAS artifact into target '{}': {e}", target_path.display())
            })?;
        }

        // Update reference tracking
        manifest.reference_tracker.add_reference(artifact_id, consumer_id);
        if let Some(desc) = manifest.artifacts.get_mut(artifact_id) {
            if !desc.consumers.contains(&consumer_id.to_string()) {
                desc.consumers.push(consumer_id.to_string());
            }
        }
        self.save_manifest(&manifest)?;

        Ok(())
    }

    /// Unlinks a consumer reference from an artifact.
    pub fn release_artifact_consumer(&self, artifact_id: &str, consumer_id: &str) -> Result<usize, String> {
        let _lock = EnvironmentFileLock::acquire(&self.lock_path, 5000)?;
        let mut manifest = self.load_manifest()?;
        let remaining = manifest.reference_tracker.remove_reference(artifact_id, consumer_id);
        if let Some(desc) = manifest.artifacts.get_mut(artifact_id) {
            desc.consumers.retain(|c| c != consumer_id);
        }
        self.save_manifest(&manifest)?;
        Ok(remaining)
    }

    /// Performs safe garbage collection on artifacts with zero active consumers.
    pub fn garbage_collect(&self) -> Result<CasGcReport, String> {
        let _lock = EnvironmentFileLock::acquire(&self.lock_path, 5000)?;
        let mut manifest = self.load_manifest()?;

        let mut eligible_ids = Vec::new();
        for (id, desc) in &manifest.artifacts {
            if manifest.reference_tracker.is_eligible_for_gc(id) && desc.consumers.is_empty() {
                eligible_ids.push(id.clone());
            }
        }

        let unreferenced_found = eligible_ids.len();
        let mut reclaimed_count = 0;
        let mut bytes_reclaimed = 0u64;

        for id in eligible_ids {
            if let Some(desc) = manifest.artifacts.remove(&id) {
                let object_path = self.vault_root.join(&desc.storage_relative_path);
                if object_path.is_file() {
                    let _ = std::fs::remove_file(&object_path);
                    bytes_reclaimed += desc.size_bytes;
                    reclaimed_count += 1;
                }
            }
        }

        manifest.total_artifacts = manifest.artifacts.len();
        manifest.total_bytes = manifest.total_bytes.saturating_sub(bytes_reclaimed);
        self.save_manifest(&manifest)?;

        Ok(CasGcReport {
            unreferenced_artifacts_found: unreferenced_found,
            artifacts_reclaimed: reclaimed_count,
            physical_bytes_reclaimed: bytes_reclaimed,
            remaining_artifacts: manifest.total_artifacts,
            remaining_bytes: manifest.total_bytes,
        })
    }

    /// Coordinates and measures all ecosystem-native caches inside the vault.
    pub fn coordinate_native_caches(&self) -> Result<Vec<NativeCacheCoordinator>, String> {
        let mut coordinators = Vec::new();

        // 1. Python uv-cache
        let uv_dir = self.vault_root.join("packages").join("python").join("uv-cache");
        let (uv_bytes, uv_count) = scan_dir_shallow(&uv_dir);
        coordinators.push(NativeCacheCoordinator {
            ecosystem: Ecosystem::Python,
            manager_name: "uv".into(),
            relative_vault_path: "packages/python/uv-cache".into(),
            total_allocated_bytes: uv_bytes,
            item_count: uv_count,
            is_active: uv_dir.is_dir(),
            diagnostic: if uv_dir.is_dir() { Some("Active uv package cache".into()) } else { None },
        });

        // 2. Python shared packages (verified GPU/wheels)
        let py_shared = self.vault_root.join("packages").join("python").join("shared");
        let (shared_bytes, shared_count) = scan_dir_shallow(&py_shared);
        coordinators.push(NativeCacheCoordinator {
            ecosystem: Ecosystem::Python,
            manager_name: "pip/wheels".into(),
            relative_vault_path: "packages/python/shared".into(),
            total_allocated_bytes: shared_bytes,
            item_count: shared_count,
            is_active: py_shared.is_dir(),
            diagnostic: Some("Verified shared package distributions".into()),
        });

        // 3. Rust Cargo Cache
        let cargo_dir = self.vault_root.join("runtimes").join("rust").join("cargo").join("registry");
        let (cargo_bytes, cargo_count) = scan_dir_shallow(&cargo_dir);
        coordinators.push(NativeCacheCoordinator {
            ecosystem: Ecosystem::Rust,
            manager_name: "cargo".into(),
            relative_vault_path: "runtimes/rust/cargo/registry".into(),
            total_allocated_bytes: cargo_bytes,
            item_count: cargo_count,
            is_active: cargo_dir.is_dir(),
            diagnostic: Some("Cargo crates.io registry cache".into()),
        });

        // 4. Node.js pnpm Store
        let pnpm_dir = self.vault_root.join("packages").join("node").join("pnpm-store");
        let (pnpm_bytes, pnpm_count) = scan_dir_shallow(&pnpm_dir);
        coordinators.push(NativeCacheCoordinator {
            ecosystem: Ecosystem::NodeJs,
            manager_name: "pnpm".into(),
            relative_vault_path: "packages/node/pnpm-store".into(),
            total_allocated_bytes: pnpm_bytes,
            item_count: pnpm_count,
            is_active: pnpm_dir.is_dir(),
            diagnostic: if pnpm_dir.is_dir() { Some("Content-addressable pnpm package store".into()) } else { None },
        });

        // 5. Go Module Cache
        let go_mod_dir = self.vault_root.join("packages").join("go").join("mod");
        let (go_bytes, go_count) = scan_dir_shallow(&go_mod_dir);
        coordinators.push(NativeCacheCoordinator {
            ecosystem: Ecosystem::Go,
            manager_name: "go".into(),
            relative_vault_path: "packages/go/mod".into(),
            total_allocated_bytes: go_bytes,
            item_count: go_count,
            is_active: go_mod_dir.is_dir(),
            diagnostic: if go_mod_dir.is_dir() { Some("Go GOMODCACHE module cache".into()) } else { None },
        });

        // 6. Ruby Gems Cache
        let ruby_gem_dir = self.vault_root.join("packages").join("ruby").join("gems");
        let (ruby_bytes, ruby_count) = scan_dir_shallow(&ruby_gem_dir);
        coordinators.push(NativeCacheCoordinator {
            ecosystem: Ecosystem::Ruby,
            manager_name: "gem/bundler".into(),
            relative_vault_path: "packages/ruby/gems".into(),
            total_allocated_bytes: ruby_bytes,
            item_count: ruby_count,
            is_active: ruby_gem_dir.is_dir(),
            diagnostic: if ruby_gem_dir.is_dir() { Some("RubyGems package store".into()) } else { None },
        });

        // 7. MSYS2 pacman package cache
        let pacman_pkg_dir = self.vault_root.join("toolchains").join("msys2").join("var").join("cache").join("pacman").join("pkg");
        let (pac_bytes, pac_count) = scan_dir_shallow(&pacman_pkg_dir);
        coordinators.push(NativeCacheCoordinator {
            ecosystem: Ecosystem::Cpp,
            manager_name: "pacman".into(),
            relative_vault_path: "toolchains/msys2/var/cache/pacman/pkg".into(),
            total_allocated_bytes: pac_bytes,
            item_count: pac_count,
            is_active: pacman_pkg_dir.is_dir(),
            diagnostic: if pacman_pkg_dir.is_dir() { Some("MSYS2 pacman package archive cache".into()) } else { None },
        });

        Ok(coordinators)
    }
}

fn scan_dir_shallow(dir: &Path) -> (u64, usize) {
    if !dir.is_dir() {
        return (0, 0);
    }
    let mut total_bytes = 0;
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total_bytes += meta.len();
                    count += 1;
                } else if meta.is_dir() {
                    count += 1;
                }
            }
        }
    }
    (total_bytes, count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    use ulvo_core::{Abi, Architecture, CasArtifactKind, Platform};

    struct TestFixture {
        dir: PathBuf,
    }

    impl TestFixture {
        fn new(name: &str) -> Self {
            let pid = std::process::id();
            let ts = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
            let dir = std::env::temp_dir().join(format!("ulvo_cas_test_{name}_{pid}_{ts}"));
            std::fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }
    }

    impl Drop for TestFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn test_cas_store_and_verify_payload() {
        let fixture = TestFixture::new("payload");
        let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

        let payload = b"hello ulvo universal cas dependency store!";
        let desc = CasArtifactDescriptor {
            artifact_id: CasArtifactId::new("0000000000000000000000000000000000000000000000000000000000000000").unwrap(),
            kind: CasArtifactKind::GenericPayload,
            ecosystem: Ecosystem::Generic("test".into()),
            package_name: "test-pkg".into(),
            version: "1.0.0".into(),
            platform: Platform::Windows,
            architecture: Architecture::X86_64,
            abi: Abi::None,
            runtime_requirements: vec![],
            package_source: "test".into(),
            integrity_sha256: "".into(),
            size_bytes: 0,
            native_compatibility: None,
            storage_relative_path: "".into(),
            immutable: true,
            created_at_unix: 1700000000,
            consumers: vec![],
        };

        let art_id = manager.store_payload(desc, payload).unwrap();
        assert_eq!(art_id.as_str().len(), 64);

        // Verify integrity
        let ok = manager.verify_artifact_integrity(art_id.as_str()).unwrap();
        assert!(ok);
    }

    #[test]
    fn test_cas_link_and_gc() {
        let fixture = TestFixture::new("link_gc");
        let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

        let payload = b"reclaimable artifact payload";
        let desc = CasArtifactDescriptor {
            artifact_id: CasArtifactId::new("0000000000000000000000000000000000000000000000000000000000000000").unwrap(),
            kind: CasArtifactKind::GenericPayload,
            ecosystem: Ecosystem::Python,
            package_name: "dummy".into(),
            version: "0.1.0".into(),
            platform: Platform::Windows,
            architecture: Architecture::X86_64,
            abi: Abi::None,
            runtime_requirements: vec![],
            package_source: "test".into(),
            integrity_sha256: "".into(),
            size_bytes: 0,
            native_compatibility: None,
            storage_relative_path: "".into(),
            immutable: true,
            created_at_unix: 1700000000,
            consumers: vec![],
        };

        let art_id = manager.store_payload(desc, payload).unwrap();

        // Link into target
        let target_file = fixture.dir.join("env").join("dummy.txt");
        manager.link_artifact_to_environment(art_id.as_str(), &target_file, "project_alpha").unwrap();
        assert!(target_file.is_file());

        // GC while consumer exists should not reclaim
        let gc1 = manager.garbage_collect().unwrap();
        assert_eq!(gc1.artifacts_reclaimed, 0);

        // Release consumer
        manager.release_artifact_consumer(art_id.as_str(), "project_alpha").unwrap();

        // GC now should reclaim
        let gc2 = manager.garbage_collect().unwrap();
        assert_eq!(gc2.artifacts_reclaimed, 1);
        assert_eq!(gc2.physical_bytes_reclaimed, payload.len() as u64);
    }
}
