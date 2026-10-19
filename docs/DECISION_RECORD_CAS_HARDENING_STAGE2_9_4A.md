# DECISION RECORD: CAS IMMUTABILITY, TRANSACTIONAL MATERIALIZATION & SAFE GC (STAGE 2.9.4a)

**Status**: Approved & Implemented  
**Date**: 2026-10-19  
**Author**: Voidwalker-AT <anmol392005@gmail.com>  
**Supervising Invariant**: `STORE ONCE. ISOLATE PER PROJECT. MATERIALIZE ON DEMAND. EXECUTE THROUGH ULVO. RECLAIM SAFELY.`

---

## 1. Context & Motivation

Stage 2.9.4 introduced the initial Universal Content-Addressable Dependency Store (CAS) foundation. However, an independent architectural audit revealed critical reliability vulnerabilities:

1. **Source Mutation Vulnerability in `store_file()`**:
   - The initial implementation attempted `std::fs::hard_link(source_path, final_path)`.
   - When a source file on the same NTFS volume was hardlinked into the CAS objects directory, subsequent modification of the source file directly mutated the CAS object payload because both paths shared the same underlying MFT record.
   - This violated the core CAS invariant: *published artifacts must be strictly immutable*.

2. **In-Flight Source Mutation**:
   - If a source file was modified concurrently during the hashing or copy phase, a truncated or corrupted file could be published without detection.

3. **Non-Idempotent Storage Accounting**:
   - When an existing artifact was re-inserted into the CAS, `manifest.total_bytes` was incremented repeatedly, leading to inaccurate physical byte accounting.

4. **Non-Transactional Target Materialization**:
   - `link_artifact_to_environment()` deleted the existing target file (`std::fs::remove_file(target_path)`) before hardlinking/copying or verifying the replacement. If materialization failed, the target environment was left in a broken state with missing files.

5. **Destructive / Unguarded Garbage Collection**:
   - The initial GC implementation immediately deleted unreferenced files without checking running process PIDs, active environment leases, or providing a non-destructive planning mode.
   - Native ecosystem caches (`uv-cache`, `shared wheels`, `cargo registry`) lacked explicit structural immunity.

6. **Permissive Compatibility Verification**:
   - `CasArtifactDescriptor::is_compatible()` treated `Platform::Unknown`, `Architecture::Unknown`, and `Abi::Unknown` as matching, and allowed GPU requirements to be satisfied by CPU packages if `native_compatibility` was omitted.

---

## 2. Architectural Decisions & Specifications

### Decision 1: Strict Source Copy, Pre-Publish Verification, and Read-Only Objects
- **Never hardlink from mutable external sources**: `store_file()` always copies content into a private staging directory (`vault/packages/cas/.staging`).
- **Post-Copy Hash Verification**: The content hash of the staged file is computed and compared against the initial digest and expected integrity hash. If the source mutated during copying, staging fails immediately and the staging file is unlinked.
- **Pre-Existing Object Verification**: If a target CAS object path (`objects/<prefix>/<sha256>`) already exists, its content hash is verified. If it matches, the object is reused without redundant copy; if it is corrupted, it is atomically replaced from staging.
- **Read-Only Permissions**: Published CAS objects receive `readonly = true` file attributes on NTFS to prevent inadvertent in-situ modification.

### Decision 2: Transactional Target Materialization
- `link_artifact_to_environment()` follows a 4-phase transactional protocol:
  1. **Phase 1 (Staging)**: Hardlink (or copy) the CAS object to a private staging file in the destination directory (`<target>.ulvo_stage_<timestamp>`).
  2. **Phase 2 (Verification)**: Hash the staged file to verify complete, uncorrupted materialization.
  3. **Phase 3 (Backup)**: If an existing target file exists, rename it to `<target>.ulvo_backup_<timestamp>`.
  4. **Phase 4 (Atomic Publication & Commit)**: Rename the staged file to `<target>`, then remove the backup file.
  - If any failure occurs prior to final commit, the backup file is restored and the staging file is cleaned up, guaranteeing zero downtime or file loss for existing targets.

### Decision 3: Multi-Tiered Idempotent Storage Accounting
- The CAS ledger differentiates:
  - `unique_cas_bytes`: Sum of unique immutable CAS object payloads stored in `vault/packages/cas/objects`.
  - `total_logical_bytes`: Sum of `artifact.size_bytes * reference_count`.
  - `allocated_ntfs_bytes`: Physical disk allocation measured via Win32 `GetCompressedFileSizeW`.
  - `hardlinked_references`: Active hardlinks across environments.
- Re-storing an existing artifact ID updates consumer metadata but never increments `unique_cas_bytes`.

### Decision 4: Fail-Closed Garbage Collection & Immunity Protocol
- Automatic destructive GC is disabled.
- GC operates by default in **read-only planning mode** (`--plan`).
- Destructive GC requires explicit `--execute` and fails closed if:
  - Any consumer reference exists in the ledger.
  - Any active environment lease is registered with the Project Orchestrator.
  - Any monitored child process is currently alive.
  - Staging or lock transactions are active.
  - Ownership or consumer state is unknown.
- **Native Cache Immunity**: Native package caches (`vault/packages/python/uv-cache`, `vault/packages/python/shared`, `vault/runtimes/rust/cargo/registry`, etc.) are permanently excluded from CAS GC deletion.
- **Transactional Journaling**: Deletions are journaled to `vault/packages/cas/gc_journal.jsonl` prior to unlinking.

### Decision 5: Strict Fail-Closed Compatibility Engine
- In `CasArtifactDescriptor::is_compatible()`:
  - Unknown platform, architecture, or ABI is rejected (`Unknown` != valid match).
  - If the consumer requests a native tag (e.g. `cuda_sm_86`), the artifact MUST specify a matching `native_compatibility`.
  - If the artifact specifies a native GPU requirement, but the consumer requests CPU (`None`), compatibility is rejected.

---

## 3. Verification Strategy

The decisions above are validated through 21 distinct adversarial test cases executed against isolated, disposable test fixtures in `crates/ulvo-adapters/src/cas_storage.rs` and `crates/ulvo-core/src/cas.rs`, followed by end-to-end integration tests on real projects:
- Python and PyTorch GPU execution on NVIDIA RTX 2050.
- Ruby 4.0.1 and Go 1.26.5 gateway execution.
- MSYS2 GCC/G++ 14.2.0 native C/C++ compilation.
