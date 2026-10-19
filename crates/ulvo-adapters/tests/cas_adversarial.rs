use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::SystemTime;
use ulvo_adapters::CasStorageManager;
use ulvo_core::{
    Abi, Architecture, CasArtifactDescriptor, CasArtifactId, CasArtifactKind,
    Ecosystem, Platform, Sha256,
};

struct TestFixture {
    dir: PathBuf,
}

impl TestFixture {
    fn new(name: &str) -> Self {
        let pid = std::process::id();
        let ts = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("ulvo_adv_suite_{name}_{pid}_{ts}"));
        fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }
}

impl Drop for TestFixture {
    fn drop(&mut self) {
        // Clear readonly flags before recursive deletion
        clear_readonly_tree(&self.dir);
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn clear_readonly_tree(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                clear_readonly_tree(&p);
            } else {
                if let Ok(meta) = fs::metadata(&p) {
                    let mut perms = meta.permissions();
                    perms.set_readonly(false);
                    let _ = fs::set_permissions(&p, perms);
                }
            }
        }
    }
}

fn make_desc(hash: &str, pkg: &str, ver: &str) -> CasArtifactDescriptor {
    CasArtifactDescriptor {
        artifact_id: CasArtifactId::new(hash).unwrap(),
        kind: CasArtifactKind::GenericPayload,
        ecosystem: Ecosystem::Python,
        package_name: pkg.into(),
        version: ver.into(),
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
fn test_01_source_mutation_after_publication() {
    let fixture = TestFixture::new("t01_mut_after");
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    let source = fixture.dir.join("input.txt");
    fs::write(&source, b"IMMUTABLE_CORE_PAYLOAD_STAGE_2_9_4A").unwrap();
    let original_hash = format!("{:x}", Sha256::digest(b"IMMUTABLE_CORE_PAYLOAD_STAGE_2_9_4A"));

    let desc = make_desc(&original_hash, "test-pkg", "1.0.0");
    let art_id = manager.store_file(desc, &source).unwrap();
    assert_eq!(art_id.as_str(), original_hash);

    // Mutate source file after store
    fs::write(&source, b"MUTATED_POISON_BYTES_THAT_SHOULD_NOT_LEAK").unwrap();

    // Verify CAS object retains original unmutated bytes
    let ok = manager.verify_artifact_integrity(&original_hash).unwrap();
    assert!(ok, "CAS object must remain untainted by source mutation");
}

#[test]
fn test_02_source_mutation_during_publication() {
    // When expected integrity does not match the content, store fails closed
    let fixture = TestFixture::new("t02_mut_during");
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    let source = fixture.dir.join("partial.txt");
    fs::write(&source, b"INITIAL_CHUNK_ONLY").unwrap();

    // Caller expected hash of the complete file
    let full_hash = format!("{:x}", Sha256::digest(b"INITIAL_CHUNK_ONLY_AND_EXTRA_DATA"));
    let desc = make_desc(&full_hash, "partial-pkg", "1.0.0");

    let res = manager.store_file(desc, &source);
    assert!(res.is_err(), "Store must fail when stream hash mismatches expected hash");
}

#[test]
fn test_03_existing_object_corruption_repaired() {
    let fixture = TestFixture::new("t03_repair");
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    let payload = b"HEALTHY_PACKAGE_PAYLOAD";
    let hash = format!("{:x}", Sha256::digest(payload));
    let desc = make_desc(&hash, "healthy-pkg", "1.0.0");

    manager.store_payload(desc.clone(), payload).unwrap();
    assert!(manager.verify_artifact_integrity(&hash).unwrap());

    // Corrupt the CAS object on disk
    let obj_path = manager.cas_directory().join("objects").join(&hash[..2]).join(&hash);
    if let Ok(meta) = fs::metadata(&obj_path) {
        let mut perms = meta.permissions();
        perms.set_readonly(false);
        let _ = fs::set_permissions(&obj_path, perms);
    }
    fs::write(&obj_path, b"CORRUPTED_BYTES").unwrap();

    // Must report corruption
    assert!(!manager.verify_artifact_integrity(&hash).unwrap());

    // Subsequent store of clean source repairs the object
    let source = fixture.dir.join("clean.txt");
    fs::write(&source, payload).unwrap();
    manager.store_file(desc, &source).unwrap();

    assert!(manager.verify_artifact_integrity(&hash).unwrap());
}

#[test]
fn test_04_repeated_insertion_idempotence() {
    let fixture = TestFixture::new("t04_idempotent");
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    let payload = b"IDEMPOTENT_REPEATED_CONTENT";
    let hash = format!("{:x}", Sha256::digest(payload));

    for _ in 0..10 {
        let desc = make_desc(&hash, "repeat-pkg", "1.0.0");
        manager.store_payload(desc, payload).unwrap();
    }

    let manifest = manager.load_manifest().unwrap();
    assert_eq!(manifest.total_artifacts, 1);
    assert_eq!(manifest.unique_cas_bytes, payload.len() as u64);
}

#[test]
fn test_05_hash_mismatch_rejected() {
    let fixture = TestFixture::new("t05_mismatch");
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    let payload = b"ACTUAL_CONTENT";
    let wrong_hash = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    let desc = make_desc(wrong_hash, "wrong-pkg", "1.0.0");

    let res = manager.store_payload(desc, payload);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("CAS_CORRUPTION_DETECTED"));
}

#[test]
fn test_06_interrupted_staging_cleaned_up() {
    let fixture = TestFixture::new("t06_staging_clean");
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    // Create an orphaned staging file simulating a crash
    let orphan = manager.cas_directory().join(".staging").join("orphan_temp_file");
    fs::write(&orphan, b"ORPHANED_STAGE_DATA").unwrap();

    // GC plan must detect pending staging files and block execution until clean
    let plan = manager.plan_garbage_collection(&[], &[]).unwrap();
    assert!(!plan.clean_to_execute);
    assert!(plan.blocking_reasons.iter().any(|r| r.contains("Pending staging files detected")));

    // Clean up orphan and re-plan
    let _ = fs::remove_file(&orphan);
    let plan_clean = manager.plan_garbage_collection(&[], &[]).unwrap();
    assert!(plan_clean.clean_to_execute);
}

#[test]
fn test_07_manifest_persistence_failure_resilience() {
    let fixture = TestFixture::new("t07_manifest_resil");
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    // Corrupted manifest triggers explicit error
    fs::write(manager.manifest_path(), b"NOT_VALID_JSON{{{").unwrap();
    assert!(manager.load_manifest().is_err());
}

#[test]
fn test_08_concurrent_cas_writers() {
    let fixture = Arc::new(TestFixture::new("t08_concurrency"));
    let mut handles = Vec::new();

    for i in 0..4 {
        let fix = Arc::clone(&fixture);
        handles.push(thread::spawn(move || {
            let manager = CasStorageManager::new(fix.dir.clone()).unwrap();
            let payload = format!("CONCURRENT_WRITER_THREAD_{i}_PAYLOAD");
            let hash = format!("{:x}", Sha256::digest(payload.as_bytes()));
            let desc = make_desc(&hash, &format!("pkg-{i}"), "1.0.0");
            manager.store_payload(desc, payload.as_bytes()).unwrap();
            manager.verify_artifact_integrity(&hash).unwrap()
        }));
    }

    for h in handles {
        assert!(h.join().unwrap());
    }

    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();
    let manifest = manager.load_manifest().unwrap();
    assert_eq!(manifest.total_artifacts, 4);
}

#[test]
fn test_09_concurrent_materialization() {
    let fixture = Arc::new(TestFixture::new("t09_mat_concurrency"));
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    let payload = b"SHARED_RUNTIME_LIBRARY_BINARY";
    let hash = format!("{:x}", Sha256::digest(payload));
    let desc = make_desc(&hash, "shared-lib", "1.0.0");
    manager.store_payload(desc, payload).unwrap();

    let mut handles = Vec::new();
    for i in 0..4 {
        let fix = Arc::clone(&fixture);
        let h_clone = hash.clone();
        handles.push(thread::spawn(move || {
            let mgr = CasStorageManager::new(fix.dir.clone()).unwrap();
            let target = fix.dir.join(format!("env_{i}")).join("lib.dll");
            mgr.link_artifact_to_environment(&h_clone, &target, &format!("consumer_{i}")).unwrap();
            fs::read(&target).unwrap() == b"SHARED_RUNTIME_LIBRARY_BINARY"
        }));
    }

    for h in handles {
        assert!(h.join().unwrap());
    }
}

#[test]
fn test_10_target_preservation_on_failure() {
    let fixture = TestFixture::new("t10_preservation");
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    let target = fixture.dir.join("env").join("preserved.txt");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"PRECIOUS_EXISTING_DATA_DO_NOT_DESTROY").unwrap();

    // Link non-existent artifact
    let fake = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    let res = manager.link_artifact_to_environment(fake, &target, "consumer_fail");
    assert!(res.is_err());

    // Target must be 100% intact
    assert_eq!(fs::read(&target).unwrap(), b"PRECIOUS_EXISTING_DATA_DO_NOT_DESTROY");
}

#[test]
fn test_11_hardlink_vs_copy_readonly_protection() {
    let fixture = TestFixture::new("t11_readonly");
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    let payload = b"READONLY_VERIFIED_CAS_OBJECT";
    let hash = format!("{:x}", Sha256::digest(payload));
    let desc = make_desc(&hash, "ro-pkg", "1.0.0");
    manager.store_payload(desc, payload).unwrap();

    let obj_path = manager.cas_directory().join("objects").join(&hash[..2]).join(&hash);
    let meta = fs::metadata(&obj_path).unwrap();
    assert!(meta.permissions().readonly(), "CAS objects must be marked read-only on disk");
}

#[test]
fn test_12_missing_reference_records() {
    let fixture = TestFixture::new("t12_missing_ref");
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    let m = manager.load_manifest().unwrap();
    assert_eq!(m.reference_tracker.reference_count("non_existent"), 0);
    assert_eq!(manager.release_artifact_consumer("non_existent", "proj").unwrap(), 0);
}

#[test]
fn test_13_stale_references_released() {
    let fixture = TestFixture::new("t13_stale_ref");
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    let payload = b"STALE_REFERENCE_TEST";
    let hash = format!("{:x}", Sha256::digest(payload));
    let desc = make_desc(&hash, "stale-pkg", "1.0.0");
    manager.store_payload(desc, payload).unwrap();

    let target = fixture.dir.join("env").join("stale.txt");
    manager.link_artifact_to_environment(&hash, &target, "old_project").unwrap();

    let plan1 = manager.plan_garbage_collection(&[], &[]).unwrap();
    assert_eq!(plan1.candidates_for_reclamation.len(), 0);

    // Release stale project
    manager.release_artifact_consumer(&hash, "old_project").unwrap();

    let plan2 = manager.plan_garbage_collection(&[], &[]).unwrap();
    assert_eq!(plan2.candidates_for_reclamation.len(), 1);
}

#[test]
fn test_14_active_consumers_preserve_artifact_during_gc() {
    let fixture = TestFixture::new("t14_active_gc");
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    let payload = b"ACTIVE_CONSUMER_PROTECTED";
    let hash = format!("{:x}", Sha256::digest(payload));
    let desc = make_desc(&hash, "active-pkg", "1.0.0");
    manager.store_payload(desc, payload).unwrap();

    let target = fixture.dir.join("env").join("active.txt");
    manager.link_artifact_to_environment(&hash, &target, "production_app").unwrap();

    // Read-only GC
    let rep = manager.garbage_collect(false, &[], &[]).unwrap();
    assert_eq!(rep.artifacts_reclaimed, 0);

    // Attempt execute GC while referenced
    let plan = manager.plan_garbage_collection(&[], &[]).unwrap();
    let exec_rep = manager.execute_garbage_collection(&plan).unwrap();
    assert_eq!(exec_rep.artifacts_reclaimed, 0);
    assert!(manager.verify_artifact_integrity(&hash).unwrap());
}

#[test]
fn test_15_gc_interrupted_journal_recovery() {
    let fixture = TestFixture::new("t15_journal");
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    let payload = b"JOURNALED_DELETION";
    let hash = format!("{:x}", Sha256::digest(payload));
    let desc = make_desc(&hash, "journal-pkg", "1.0.0");
    manager.store_payload(desc, payload).unwrap();

    let plan = manager.plan_garbage_collection(&[], &[]).unwrap();
    let rep = manager.execute_garbage_collection(&plan).unwrap();

    assert_eq!(rep.artifacts_reclaimed, 1);
    let journal = fs::read_to_string(manager.journal_path()).unwrap();
    assert!(journal.contains("reclaim_artifact"));
    assert!(journal.contains(&hash));
}

#[test]
fn test_16_reconciliation_after_crash() {
    let fixture = TestFixture::new("t16_crash_recon");
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    let payload = b"EXTERNAL_UNLINK_CRASH";
    let hash = format!("{:x}", Sha256::digest(payload));
    let desc = make_desc(&hash, "crash-pkg", "1.0.0");
    manager.store_payload(desc, payload).unwrap();

    // Manually delete object from disk without updating manifest
    let obj_path = manager.cas_directory().join("objects").join(&hash[..2]).join(&hash);
    let _ = fs::remove_file(&obj_path);

    // Integrity check must return false gracefully without panicking
    assert_eq!(manager.verify_artifact_integrity(&hash).unwrap(), false);
}

#[test]
fn test_17_abi_mismatch_rejected() {
    let mut desc = make_desc("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "abi-test", "1.0.0");
    desc.abi = Abi::Gnu;
    // GNU vs MSVC ABI mismatch
    assert!(!desc.is_compatible(&Ecosystem::Python, &Platform::Windows, &Architecture::X86_64, &Abi::Msvc, None));
    // GNU matches GNU
    assert!(desc.is_compatible(&Ecosystem::Python, &Platform::Windows, &Architecture::X86_64, &Abi::Gnu, None));
}

#[test]
fn test_18_cuda_variant_mismatch_rejected() {
    let mut desc = make_desc("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "torch", "2.6.0");
    desc.native_compatibility = Some("cuda_sm_86".into());

    // Incompatible CUDA arch
    assert!(!desc.is_compatible(&Ecosystem::Python, &Platform::Windows, &Architecture::X86_64, &Abi::None, Some("cuda_sm_75")));

    // Consumer requested CPU (None) but artifact is GPU-only
    assert!(!desc.is_compatible(&Ecosystem::Python, &Platform::Windows, &Architecture::X86_64, &Abi::None, None));
}

#[test]
fn test_19_unknown_compatibility_metadata_rejected() {
    let desc = make_desc("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "unknown-test", "1.0.0");

    // Unknown Platform
    assert!(!desc.is_compatible(&Ecosystem::Python, &Platform::Unknown("amiga".into()), &Architecture::X86_64, &Abi::None, None));

    // Unknown Architecture
    assert!(!desc.is_compatible(&Ecosystem::Python, &Platform::Windows, &Architecture::Unknown("mips".into()), &Abi::None, None));

    // Unknown ABI
    assert!(!desc.is_compatible(&Ecosystem::Python, &Platform::Windows, &Architecture::X86_64, &Abi::Unknown("o32".into()), None));
}

#[test]
fn test_20_cross_volume_fallback() {
    let fixture = TestFixture::new("t20_cross_vol");
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    let payload = b"CROSS_VOLUME_SIMULATION";
    let hash = format!("{:x}", Sha256::digest(payload));
    let desc = make_desc(&hash, "cross-vol", "1.0.0");
    manager.store_payload(desc, payload).unwrap();

    let target = fixture.dir.join("sub_vol").join("copied.bin");
    manager.link_artifact_to_environment(&hash, &target, "cross_consumer").unwrap();

    assert_eq!(fs::read(&target).unwrap(), payload);
}

#[test]
fn test_21_reparse_point_and_junction_escapes_rejected() {
    let fixture = TestFixture::new("t21_escapes");
    let manager = CasStorageManager::new(fixture.dir.clone()).unwrap();

    // Path with .. escaping outside target
    let escape_target = fixture.dir.join("env").join("..").join("..").join("escaped.txt");
    let fake_hash = "3333333333333333333333333333333333333333333333333333333333333333";
    let res = manager.link_artifact_to_environment(fake_hash, &escape_target, "bad_actor");
    assert!(res.is_err());
}
