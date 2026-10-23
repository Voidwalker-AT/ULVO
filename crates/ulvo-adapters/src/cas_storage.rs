use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};
use ulvo_core::{
    CasArtifactDescriptor, CasArtifactId, CasGcCandidate, CasGcPlan, CasManifest,
    Ecosystem, NativeCacheCoordinator, Sha256,
};
use crate::{bounded_read, write_atomic, EnvironmentFileLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CasGcReport {
    pub unreferenced_artifacts_found: usize,
    pub artifacts_reclaimed: usize,
    /// Data allocation unlinked at last reference, not an observed volume delta.
    pub physical_bytes_reclaimed: u64,
    pub volume_free_bytes_delta: Option<i64>,
    pub remaining_artifacts: usize,
    pub remaining_bytes: u64,
    pub is_dry_run: bool,
    pub journal_path: Option<String>,
}

pub struct CasStorageManager {
    vault_root: PathBuf,
    cas_dir: PathBuf,
    objects_dir: PathBuf,
    staging_dir: PathBuf,
    manifest_path: PathBuf,
    lock_path: PathBuf,
    journal_path: PathBuf,
}

impl CasStorageManager {
    pub fn new(vault_root: PathBuf) -> Result<Self, String> {
        let clean_vault = ulvo_core::strip_unc_prefix(&vault_root);
        let cas_dir = clean_vault.join("packages").join("cas");
        let objects_dir = cas_dir.join("objects");
        let staging_dir = cas_dir.join(".staging");
        let manifest_path = cas_dir.join("manifest.json");
        let lock_path = cas_dir.join(".cas.lock");
        let journal_path = cas_dir.join("gc_journal.jsonl");

        fs::create_dir_all(&objects_dir).map_err(|e| format!("cannot create CAS objects dir: {e}"))?;
        fs::create_dir_all(&staging_dir).map_err(|e| format!("cannot create CAS staging dir: {e}"))?;

        Ok(Self {
            vault_root: clean_vault,
            cas_dir,
            objects_dir,
            staging_dir,
            manifest_path,
            lock_path,
            journal_path,
        })
    }

    pub fn cas_directory(&self) -> &Path {
        &self.cas_dir
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    pub fn journal_path(&self) -> &Path {
        &self.journal_path
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

    /// Stores raw payload bytes into the CAS object store with SHA-256 verification and read-only protection.
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

        let prefix = &actual_hash[..2];
        let target_dir = self.objects_dir.join(prefix);
        fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

        let final_path = target_dir.join(&actual_hash);
        let rel_path = format!("packages/cas/objects/{prefix}/{actual_hash}");
        descriptor.storage_relative_path = rel_path;
        descriptor.immutable = true;

        let mut manifest = self.load_manifest()?;

        if final_path.is_file() {
            // Check pre-existing object integrity before reuse
            let existing_hash = hash_file_path(&final_path)?;
            if !existing_hash.eq_ignore_ascii_case(&actual_hash) {
                let has_active_consumers = manifest.artifacts.get(&actual_hash)
                    .map_or(false, |desc| !desc.consumers.is_empty())
                    || manifest.reference_tracker.reference_count(&actual_hash) > 0;
                if has_active_consumers {
                    return Err(format!(
                        "CAS_CORRUPTED_OBJECT_HELD_BY_ACTIVE_CONSUMERS: Cannot repair corrupted CAS object '{}' while active consumers reference it",
                        actual_hash
                    ));
                }
                // Object is corrupted; atomically repair from clean bytes
                let ts = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
                let staging_file = self.staging_dir.join(format!("repair_{}_{}_{}", actual_hash, std::process::id(), ts));
                write_atomic(&staging_file, data)?;
                let _ = set_file_readonly(&final_path, false);
                fs::rename(&staging_file, &final_path)
                    .map_err(|e| format!("failed to repair corrupted CAS object: {e}"))?;
                let _ = set_file_readonly(&final_path, true);
            }
        } else {
            // Write atomically via staging directory
            let ts = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
            let staging_file = self.staging_dir.join(format!("stage_{}_{}_{}", actual_hash, std::process::id(), ts));
            write_atomic(&staging_file, data)?;
            fs::rename(&staging_file, &final_path)
                .map_err(|e| format!("failed to publish artifact to CAS: {e}"))?;
            let _ = set_file_readonly(&final_path, true);
        }

        // Idempotent manifest update: recompute_logical_bytes ensures no duplicate byte accounting
        manifest.artifacts.insert(actual_hash.clone(), descriptor);
        manifest.recompute_logical_bytes();
        self.save_manifest(&manifest)?;

        Ok(artifact_id)
    }

    /// Stores an existing file into the CAS object store via streaming copy, verified hashing,
    /// atomic publication, and read-only protection. Never hardlinks from a mutable source.
    pub fn store_file(
        &self,
        mut descriptor: CasArtifactDescriptor,
        source_path: &Path,
    ) -> Result<CasArtifactId, String> {
        if !source_path.is_file() {
            return Err(format!("source path is not a regular file: {}", source_path.display()));
        }

        let _lock = EnvironmentFileLock::acquire(&self.lock_path, 5000)?;

        let ts = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
        let staging_file = self.staging_dir.join(format!("stage_file_{}_{}_{}", std::process::id(), ts, rand_suffix()));

        // Phase 1: Stream copy into private staging file while computing SHA-256
        let mut source_file = File::open(source_path)
            .map_err(|e| format!("cannot open source file '{}': {e}", source_path.display()))?;
        let mut staging_out = File::create(&staging_file)
            .map_err(|e| format!("cannot create CAS staging file: {e}"))?;

        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        let mut total_bytes = 0u64;

        loop {
            let n = source_file.read(&mut buffer).map_err(|e| {
                let _ = fs::remove_file(&staging_file);
                format!("error reading source file: {e}")
            })?;
            if n == 0 {
                break;
            }
            if let Err(e) = staging_out.write_all(&buffer[..n]) {
                let _ = fs::remove_file(&staging_file);
                return Err(format!("error writing staging file: {e}"));
            }
            hasher.update(&buffer[..n]);
            total_bytes += n as u64;
        }
        drop(staging_out);
        drop(source_file);

        let actual_hash = format!("{:x}", hasher.finalize());

        // Validate caller expected hash if provided
        if !descriptor.integrity_sha256.is_empty() && !descriptor.integrity_sha256.eq_ignore_ascii_case(&actual_hash) {
            let _ = fs::remove_file(&staging_file);
            return Err(format!(
                "CAS_CORRUPTION_DETECTED: Staged file checksum '{}' does not match expected '{}'",
                actual_hash, descriptor.integrity_sha256
            ));
        }

        // Phase 2: Post-copy verification on the staged file to detect any in-flight source mutation
        let staged_verify_hash = hash_file_path(&staging_file)?;
        if !staged_verify_hash.eq_ignore_ascii_case(&actual_hash) {
            let _ = fs::remove_file(&staging_file);
            return Err(format!(
                "SOURCE_MUTATED_DURING_COPY: Staged file hash '{}' diverges from initial stream hash '{}'",
                staged_verify_hash, actual_hash
            ));
        }

        let artifact_id = CasArtifactId::new(&actual_hash)?;
        descriptor.artifact_id = artifact_id.clone();
        descriptor.integrity_sha256 = actual_hash.clone();
        descriptor.size_bytes = total_bytes;

        let prefix = &actual_hash[..2];
        let target_dir = self.objects_dir.join(prefix);
        fs::create_dir_all(&target_dir).map_err(|e| {
            let _ = fs::remove_file(&staging_file);
            e.to_string()
        })?;

        let final_path = target_dir.join(&actual_hash);
        let rel_path = format!("packages/cas/objects/{prefix}/{actual_hash}");
        descriptor.storage_relative_path = rel_path;
        descriptor.immutable = true;

        // Phase 3: Check pre-existing object
        let mut manifest = self.load_manifest()?;
        if final_path.is_file() {
            let existing_hash = hash_file_path(&final_path)?;
            if existing_hash.eq_ignore_ascii_case(&actual_hash) {
                // Object exists and is valid: safely discard staging file and reuse
                let _ = fs::remove_file(&staging_file);
            } else {
                let has_active_consumers = manifest.artifacts.get(&actual_hash)
                    .map_or(false, |desc| !desc.consumers.is_empty())
                    || manifest.reference_tracker.reference_count(&actual_hash) > 0;
                if has_active_consumers {
                    let _ = fs::remove_file(&staging_file);
                    return Err(format!(
                        "CAS_CORRUPTED_OBJECT_HELD_BY_ACTIVE_CONSUMERS: Cannot repair corrupted CAS object '{}' while active consumers reference it",
                        actual_hash
                    ));
                }
                // Existing object corrupted and unreferenced: repair by atomic replace from verified staging file
                let _ = set_file_readonly(&final_path, false);
                fs::rename(&staging_file, &final_path)
                    .map_err(|e| format!("failed to repair corrupted CAS object: {e}"))?;
                let _ = set_file_readonly(&final_path, true);
            }
        } else {
            // Atomic publish via rename
            fs::rename(&staging_file, &final_path)
                .map_err(|e| format!("failed to publish artifact to CAS: {e}"))?;
            let _ = set_file_readonly(&final_path, true);
        }

        // Phase 4: Idempotent manifest update
        manifest.artifacts.insert(actual_hash.clone(), descriptor);
        manifest.recompute_logical_bytes();
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

        let actual = hash_file_path(&object_path)?;
        Ok(actual.eq_ignore_ascii_case(&descriptor.integrity_sha256))
    }

    /// Links an artifact into a target consumer environment using a 4-phase transactional protocol:
    /// staging -> integrity verification -> target backup -> atomic publication.
    /// Preserves existing target file on any failure.
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

        // Pre-check object integrity
        let actual_obj_hash = hash_file_path(&object_path)?;
        if !actual_obj_hash.eq_ignore_ascii_case(&descriptor.integrity_sha256) {
            return Err(format!(
                "CORRUPT_CAS_OBJECT: payload at '{}' failed hash verification",
                object_path.display()
            ));
        }

        let target_dir = target_path.parent().ok_or_else(|| "target path has no parent".to_string())?;
        fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;

        let file_name = target_path.file_name().and_then(|n| n.to_str()).unwrap_or("artifact");
        let ts = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
        let pid = std::process::id();

        // Phase 1: Stage replacement inside target filesystem
        let stage_path = target_dir.join(format!(".ulvo_stage_{}_{}_{}", file_name, pid, ts));
        let backup_path = target_dir.join(format!(".ulvo_backup_{}_{}_{}", file_name, pid, ts));

        // Hardlink or copy into staging
        let link_res = fs::hard_link(&object_path, &stage_path);
        if link_res.is_err() {
            fs::copy(&object_path, &stage_path).map_err(|e| {
                let _ = fs::remove_file(&stage_path);
                format!("failed to copy CAS artifact to staging target: {e}")
            })?;
        }

        // Phase 2: Verify staged replacement integrity before modifying any existing targets
        let staged_hash = hash_file_path(&stage_path)?;
        if !staged_hash.eq_ignore_ascii_case(&descriptor.integrity_sha256) {
            let _ = fs::remove_file(&stage_path);
            return Err(format!(
                "MATERIALIZATION_VERIFICATION_FAILED: Staged file hash mismatch for '{}'",
                target_path.display()
            ));
        }

        // Phase 3 & 4: Atomic publication with backup preservation
        if target_path.exists() {
            let _ = set_file_readonly(target_path, false);
            if let Err(e) = fs::rename(target_path, &backup_path) {
                let _ = fs::remove_file(&stage_path);
                return Err(format!("failed to stage backup of existing target: {e}"));
            }

            if let Err(e) = fs::rename(&stage_path, target_path) {
                // Rollback: restore backup
                let _ = fs::rename(&backup_path, target_path);
                let _ = fs::remove_file(&stage_path);
                return Err(format!("failed to atomically publish replacement to target: {e}"));
            }

            // Successfully committed replacement, remove backup
            let _ = fs::remove_file(&backup_path);
        } else {
            if let Err(e) = fs::rename(&stage_path, target_path) {
                let _ = fs::remove_file(&stage_path);
                return Err(format!("failed to publish staged file to target: {e}"));
            }
        }

        // Ensure read-only protection on target
        let _ = set_file_readonly(target_path, true);

        // Update reference tracking under lock
        manifest.reference_tracker.add_reference(artifact_id, consumer_id);
        if let Some(desc) = manifest.artifacts.get_mut(artifact_id) {
            if !desc.consumers.contains(&consumer_id.to_string()) {
                desc.consumers.push(consumer_id.to_string());
            }
        }
        manifest.recompute_logical_bytes();
        if let Err(e) = self.save_manifest(&manifest) {
            // Write recovery journal record preserving transaction evidence
            if let Ok(mut journal) = OpenOptions::new().create(true).append(true).open(&self.journal_path) {
                let _ = writeln!(
                    journal,
                    r#"{{"action":"materialization_unpersisted","artifact_id":"{}","consumer_id":"{}","target":"{}","timestamp":{}}}"#,
                    artifact_id, consumer_id, target_path.display(), ts
                );
            }
            return Err(format!("CRITICAL: Materialization succeeded but manifest persistence failed: {e}. Preserved in journal"));
        }

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
        manifest.recompute_logical_bytes();
        self.save_manifest(&manifest)?;
        Ok(remaining)
    }

    /// Produces a non-destructive Garbage Collection plan considering durable references,
    /// active environment leases, running processes, and protected native ecosystem caches.
    pub fn plan_garbage_collection(
        &self,
        active_pids: &[u32],
        active_leases: &[String],
    ) -> Result<CasGcPlan, String> {
        let manifest = self.load_manifest()?;

        let mut candidates = Vec::new();
        let mut preserved = Vec::new();
        let mut total_reclaimable = 0u64;
        let mut blocking_reasons = Vec::new();

        // Check if pending transactions / staging files exist
        if let Ok(entries) = fs::read_dir(&self.staging_dir) {
            let active_stages = entries.flatten().count();
            if active_stages > 0 {
                blocking_reasons.push(format!("Pending staging files detected in CAS staging directory ({active_stages} files)"));
            }
        }

        for (id, desc) in &manifest.artifacts {
            // 1. Durable reference check
            let ref_count = manifest.reference_tracker.reference_count(id);
            if ref_count > 0 {
                preserved.push((id.clone(), format!("Has {ref_count} durable consumer reference(s)")));
                continue;
            }

            // 2. Declared consumer check
            if !desc.consumers.is_empty() {
                preserved.push((id.clone(), format!("Declared consumer entries present ({})", desc.consumers.join(", "))));
                continue;
            }

            // 3. Active lease check
            let lease_conflict = active_leases.iter().any(|l| desc.consumers.contains(l));
            if lease_conflict {
                preserved.push((id.clone(), "Associated with an active environment lease".into()));
                continue;
            }

            // 4. Native Cache Immunity: Native caches are NEVER deleted by CAS GC
            let rel = &desc.storage_relative_path;
            if rel.starts_with("packages/python/uv-cache")
                || rel.starts_with("packages/python/shared")
                || rel.starts_with("runtimes/rust/cargo")
                || rel.starts_with("packages/node/pnpm-store")
                || rel.starts_with("packages/go/mod")
                || rel.starts_with("packages/ruby/gems")
                || rel.starts_with("toolchains/msys2/var/cache")
            {
                preserved.push((id.clone(), "Protected native ecosystem package cache".into()));
                continue;
            }

            // Candidate identified
            candidates.push(CasGcCandidate {
                artifact_id: id.clone(),
                package_name: desc.package_name.clone(),
                version: desc.version.clone(),
                size_bytes: desc.size_bytes,
                storage_relative_path: desc.storage_relative_path.clone(),
            });
            total_reclaimable += desc.size_bytes;
        }

        let clean_to_execute = blocking_reasons.is_empty() && active_pids.is_empty();
        if !active_pids.is_empty() {
            blocking_reasons.push(format!("Active processes detected holding potential environment leases ({:?})", active_pids));
        }

        Ok(CasGcPlan {
            schema_version: 1,
            dry_run: true,
            candidates_for_reclamation: candidates,
            preserved_artifacts: preserved,
            total_reclaimable_bytes: total_reclaimable,
            active_leases_checked: active_leases.len(),
            active_processes_checked: active_pids.len(),
            clean_to_execute,
            blocking_reasons,
        })
    }

    /// Executes garbage collection according to an approved plan with journaled recovery.
    pub fn execute_garbage_collection(
        &self,
        plan: &CasGcPlan,
    ) -> Result<CasGcReport, String> {
        if !plan.clean_to_execute {
            return Err(format!(
                "GC_EXECUTION_BLOCKED: Safety gates failed: {}",
                plan.blocking_reasons.join("; ")
            ));
        }

        let disk_before = crate::get_disk_free_bytes(&self.vault_root).ok();
        let _lock = EnvironmentFileLock::acquire(&self.lock_path, 5000)?;
        let mut manifest = self.load_manifest()?;

        let mut reclaimed_count = 0;
        let mut bytes_reclaimed = 0u64;

        // Plans are caller-supplied and may be stale or tampered with. Validate
        // every deletion against current ownership before deleting anything.
        for candidate in &plan.candidates_for_reclamation {
            let id = &candidate.artifact_id;
            CasArtifactId::new(id)?;
            let descriptor = manifest.artifacts.get(id).ok_or("GC candidate is no longer registered")?;
            let expected = format!("packages/cas/objects/{}/{id}", &id[..2]);
            if candidate.storage_relative_path != expected || descriptor.storage_relative_path != expected {
                return Err("GC candidate path does not match its owned CAS object".into());
            }
            if !descriptor.consumers.is_empty() || manifest.reference_tracker.reference_count(id) > 0 {
                return Err("GC candidate acquired a consumer; replan before deletion".into());
            }
            crate::reject_links(&self.vault_root.join(&expected))?;
        }

        // Open journal for append
        let mut journal = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.journal_path)
            .map_err(|e| format!("cannot open GC journal: {e}"))?;

        for candidate in &plan.candidates_for_reclamation {
            let id = &candidate.artifact_id;
            // Re-verify that artifact still has 0 references under lock
            if manifest.reference_tracker.reference_count(id) > 0 {
                continue;
            }

            let object_path = self.vault_root.join(&candidate.storage_relative_path);
            if object_path.is_file() {
                // Journal the intent before unlinking
                let journal_entry = serde_json::json!({
                    "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
                    "action": "reclaim_artifact",
                    "artifact_id": id,
                    "relative_path": candidate.storage_relative_path,
                    "size_bytes": candidate.size_bytes
                });
                writeln!(journal, "{}", journal_entry)
                    .and_then(|_| journal.sync_data())
                    .map_err(|e| format!("cannot durably journal GC intent: {e}"))?;

                // Check hardlink count: physical recovery is 0 if other hardlinks reference the same NTFS inode!
                let mut freed_phys = 0u64;
                if let Ok((_id, info)) = crate::query_file_identity_and_size(&object_path) {
                    if info.link_count == 1 {
                        freed_phys = info.allocated_bytes.unwrap_or(0);
                    }
                }

                // Unlink file; clear read-only first
                crate::reject_links(&object_path)?;
                let _ = set_file_readonly(&object_path, false);
                if fs::remove_file(&object_path).is_ok() {
                    manifest.artifacts.remove(id);
                    bytes_reclaimed += freed_phys;
                    reclaimed_count += 1;
                } else {
                    // Filesystem deletion failed; retain manifest record to avoid phantom deletion
                    continue;
                }
            } else {
                // File was already missing; clean manifest
                manifest.artifacts.remove(id);
            }
        }

        manifest.recompute_logical_bytes();
        self.save_manifest(&manifest)?;

        let disk_after = crate::get_disk_free_bytes(&self.vault_root).ok();
        let volume_free_delta = match (disk_before, disk_after) {
            (Some(b), Some(a)) => Some(a as i64 - b as i64),
            _ => None,
        };

        Ok(CasGcReport {
            unreferenced_artifacts_found: plan.candidates_for_reclamation.len(),
            artifacts_reclaimed: reclaimed_count,
            physical_bytes_reclaimed: bytes_reclaimed,
            volume_free_bytes_delta: volume_free_delta,
            remaining_artifacts: manifest.total_artifacts,
            remaining_bytes: manifest.unique_cas_bytes,
            is_dry_run: false,
            journal_path: Some(self.journal_path.to_string_lossy().into_owned()),
        })
    }

    /// Default entrypoint for GC: executes in read-only planning mode unless `execute: true`.
    pub fn garbage_collect(
        &self,
        execute: bool,
        active_pids: &[u32],
        active_leases: &[String],
    ) -> Result<CasGcReport, String> {
        let plan = self.plan_garbage_collection(active_pids, active_leases)?;
        if !execute {
            // Read-only planning mode
            return Ok(CasGcReport {
                unreferenced_artifacts_found: plan.candidates_for_reclamation.len(),
                artifacts_reclaimed: 0,
                physical_bytes_reclaimed: 0,
                volume_free_bytes_delta: None,
                remaining_artifacts: self.load_manifest()?.total_artifacts,
                remaining_bytes: plan.total_reclaimable_bytes,
                is_dry_run: true,
                journal_path: None,
            });
        }
        self.execute_garbage_collection(&plan)
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
    if let Ok(entries) = fs::read_dir(dir) {
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

fn hash_file_path(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("cannot open file '{}': {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| format!("error hashing file: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn set_file_readonly(path: &Path, readonly: bool) -> Result<(), String> {
    if let Ok(meta) = fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_readonly(readonly);
        let _ = fs::set_permissions(path, perms);
    }
    Ok(())
}

fn rand_suffix() -> u32 {
    let t = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
    (t & 0xFFFFFFFF) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use ulvo_core::{Abi, Architecture, CasArtifactKind, Platform};

    struct TestFixture {
        dir: PathBuf,
    }

    impl TestFixture {
        fn new(name: &str) -> Self {
            let pid = std::process::id();
            let ts = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
            let dir = std::env::temp_dir().join(format!("ulvo_cas_adv_{name}_{pid}_{ts}"));
            fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }
    }

    impl Drop for TestFixture {
        fn drop(&mut self) {
            // Clear any readonly attributes in test fixture before removal
            if let Ok(entries) = fs::read_dir(&self.dir) {
                for e in entries.flatten() {
                    let _ = set_file_readonly(&e.path(), false);
                }
            }
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn sample_descriptor(hash: &str) -> CasArtifactDescriptor {
        CasArtifactDescriptor {
            artifact_id: CasArtifactId::new(hash).unwrap(),
            kind: CasArtifactKind::GenericPayload,
            ecosystem: Ecosystem::Python,
            package_name: "test-artifact".into(),
            version: "1.0.0".into(),
            platform: Platform::Windows,
            architecture: Architecture::X86_64,
            abi: Abi::None,
            runtime_requirements: vec![],
            package_source: "test".into(),
            integrity_sha256: hash.into(),
            size_bytes: 0,
            native_compatibility: None,
            storage_relative_path: "".into(),
            immutable: true,
            created_at_unix: 1700000000,
            consumers: vec![],
        }
    }

    #[test]
    fn test_cas_store_and_verify_payload() {
        let fixture = TestFixture::new("payload");
        let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

        let payload = b"hello ulvo universal cas dependency store!";
        let dummy_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let mut desc = sample_descriptor(dummy_hash);
        desc.integrity_sha256 = "".into();

        let art_id = manager.store_payload(desc, payload).unwrap();
        assert_eq!(art_id.as_str().len(), 64);

        // Verify integrity
        let ok = manager.verify_artifact_integrity(art_id.as_str()).unwrap();
        assert!(ok);
    }

    #[test]
    fn test_cas_source_mutation_after_publication_does_not_mutate_cas() {
        let fixture = TestFixture::new("immutability");
        let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

        // 1. Create source file
        let source_file = fixture.dir.join("source.txt");
        fs::write(&source_file, b"ORIGINAL_IMMUTABLE_CONTENT").unwrap();
        let original_hash = format!("{:x}", Sha256::digest(b"ORIGINAL_IMMUTABLE_CONTENT"));

        let mut desc = sample_descriptor(&original_hash);
        desc.integrity_sha256 = original_hash.clone();

        let art_id = manager.store_file(desc, &source_file).unwrap();
        assert_eq!(art_id.as_str(), original_hash);

        // 2. Mutate source file AFTER publication
        fs::write(&source_file, b"MUTATED_SOURCE_CORRUPTED").unwrap();
        let mutated_hash = format!("{:x}", Sha256::digest(b"MUTATED_SOURCE_CORRUPTED"));
        assert_ne!(original_hash, mutated_hash);

        // 3. Verify CAS object is completely UNTOUCHED
        let ok = manager.verify_artifact_integrity(&original_hash).unwrap();
        assert!(ok, "CAS object must remain untainted despite source file mutation");
    }

    #[test]
    fn test_cas_repeated_insertion_is_idempotent() {
        let fixture = TestFixture::new("idempotency");
        let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

        let payload = b"IDENTICAL_CONTENT_MULTIPLE_INSERTS";
        let hash = format!("{:x}", Sha256::digest(payload));

        let desc1 = sample_descriptor(&hash);
        manager.store_payload(desc1, payload).unwrap();

        let m1 = manager.load_manifest().unwrap();
        assert_eq!(m1.total_artifacts, 1);
        assert_eq!(m1.unique_cas_bytes, payload.len() as u64);

        // Re-insert exact same artifact 4 more times
        for _ in 0..4 {
            let desc = sample_descriptor(&hash);
            manager.store_payload(desc, payload).unwrap();
        }

        let m2 = manager.load_manifest().unwrap();
        assert_eq!(m2.total_artifacts, 1);
        assert_eq!(m2.unique_cas_bytes, payload.len() as u64, "Unique CAS bytes must NOT inflate on duplicate insert");
    }

    #[test]
    fn test_cas_hash_mismatch_rejected() {
        let fixture = TestFixture::new("mismatch");
        let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

        let source_file = fixture.dir.join("payload.txt");
        fs::write(&source_file, b"ACTUAL_DATA").unwrap();

        let fake_hash = "1111111111111111111111111111111111111111111111111111111111111111";
        let desc = sample_descriptor(fake_hash);

        let res = manager.store_file(desc, &source_file);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("CAS_CORRUPTION_DETECTED"));
    }

    #[test]
    fn test_cas_existing_object_corruption_repaired() {
        let fixture = TestFixture::new("repair");
        let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

        let payload = b"LEGITIMATE_CONTENT";
        let hash = format!("{:x}", Sha256::digest(payload));

        let desc = sample_descriptor(&hash);
        manager.store_payload(desc.clone(), payload).unwrap();

        // Deliberately tamper with published CAS object on disk
        let object_path = manager.cas_dir.join("objects").join(&hash[..2]).join(&hash);
        let _ = set_file_readonly(&object_path, false);
        fs::write(&object_path, b"TAMPERED_MALICIOUS_BYTES").unwrap();

        // Verification must detect corruption
        let ok = manager.verify_artifact_integrity(&hash).unwrap();
        assert!(!ok, "Integrity check must detect tampered bytes");

        // Storing the file again must repair the corrupted object
        let source_file = fixture.dir.join("valid_source.txt");
        fs::write(&source_file, payload).unwrap();
        manager.store_file(desc, &source_file).unwrap();

        // Verification must now pass
        let ok_repaired = manager.verify_artifact_integrity(&hash).unwrap();
        assert!(ok_repaired, "Corrupted object must be repaired on subsequent store");
    }

    #[test]
    fn test_transactional_materialization_preserves_target_on_failure() {
        let fixture = TestFixture::new("tx_materialize");
        let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

        let env_target = fixture.dir.join("env").join("critical_module.dll");
        fs::create_dir_all(env_target.parent().unwrap()).unwrap();
        fs::write(&env_target, b"EXISTING_FUNCTIONAL_TARGET").unwrap();

        // Attempt to link a non-existent artifact ID
        let non_existent_id = "2222222222222222222222222222222222222222222222222222222222222222";
        let res = manager.link_artifact_to_environment(non_existent_id, &env_target, "consumer_x");
        assert!(res.is_err());

        // Target must remain completely intact!
        let current_content = fs::read(&env_target).unwrap();
        assert_eq!(current_content, b"EXISTING_FUNCTIONAL_TARGET");
    }

    #[test]
    fn test_safe_gc_planning_and_execution() {
        let fixture = TestFixture::new("safe_gc");
        let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

        let payload = b"reclaimable_payload";
        let hash = format!("{:x}", Sha256::digest(payload));
        let desc = sample_descriptor(&hash);
        let art_id = manager.store_payload(desc, payload).unwrap();

        // Link to project
        let target_file = fixture.dir.join("env").join("test.txt");
        manager.link_artifact_to_environment(art_id.as_str(), &target_file, "proj_1").unwrap();

        // Plan while referenced: 0 candidates
        let plan1 = manager.plan_garbage_collection(&[], &[]).unwrap();
        assert_eq!(plan1.candidates_for_reclamation.len(), 0);
        assert_eq!(plan1.preserved_artifacts.len(), 1);

        // Read-only GC command produces report with 0 reclaimed
        let rep1 = manager.garbage_collect(false, &[], &[]).unwrap();
        assert!(rep1.is_dry_run);
        assert_eq!(rep1.artifacts_reclaimed, 0);

        // Release reference and clean up consumer environment file
        fs::remove_file(&target_file).unwrap();
        manager.release_artifact_consumer(art_id.as_str(), "proj_1").unwrap();

        // Plan with active PID: blocked
        let plan2 = manager.plan_garbage_collection(&[99999], &[]).unwrap();
        assert!(!plan2.clean_to_execute);

        // Attempting execute with active PID fails closed
        let rep_blocked = manager.execute_garbage_collection(&plan2);
        assert!(rep_blocked.is_err());

        // Plan with clean state
        let plan_clean = manager.plan_garbage_collection(&[], &[]).unwrap();
        assert!(plan_clean.clean_to_execute);
        assert_eq!(plan_clean.candidates_for_reclamation.len(), 1);
        let expected_allocation = crate::query_file_identity_and_size(
            &fixture.dir.join(&plan_clean.candidates_for_reclamation[0].storage_relative_path)
        ).unwrap().1.allocated_bytes.unwrap();

        // Execute GC
        let rep_executed = manager.execute_garbage_collection(&plan_clean).unwrap();
        assert_eq!(rep_executed.artifacts_reclaimed, 1);
        assert_eq!(rep_executed.physical_bytes_reclaimed, expected_allocation);
        assert!(rep_executed.journal_path.is_some());

        // Verify journal recorded the deletion
        let journal_content = fs::read_to_string(manager.journal_path()).unwrap();
        assert!(journal_content.contains("reclaim_artifact"));
        assert!(journal_content.contains(&hash));
    }

    #[test]
    fn forged_gc_path_cannot_delete_unowned_file() {
        let fixture = TestFixture::new("forged_gc");
        let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();
        let payload = b"owned payload";
        let hash = format!("{:x}", Sha256::digest(payload));
        manager.store_payload(sample_descriptor(&hash), payload).unwrap();
        let unowned = fixture.dir.join("user-owned.txt");
        fs::write(&unowned, b"preserve me").unwrap();
        let mut plan = manager.plan_garbage_collection(&[], &[]).unwrap();
        plan.candidates_for_reclamation[0].storage_relative_path = "user-owned.txt".into();
        assert!(manager.execute_garbage_collection(&plan).is_err());
        assert_eq!(fs::read(unowned).unwrap(), b"preserve me");
    }
}
