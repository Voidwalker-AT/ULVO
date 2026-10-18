# ULVO Stage 2.6 — Universal Vault Registry Foundation Report
**Engineering Milestone**: Universal Vault Registry Foundation & Native Windows CLI Integration  
**Date**: 2026-10-18  
**Verification Level**: GRAVITAS Deep Verification (`[VERIFIED]`)  
**Target Environment**: Windows 11 Native (NTFS, `x86_64-pc-windows-gnu`)  

---

## 1. Executive Summary

In ULVO Stage 2.6, we designed, implemented, and verified the **Universal Vault Registry Foundation** and integrated it directly into the native Windows ULVO CLI.

The implementation:
1. **Establishes a Generic, Versioned Resource Model**: Strongly typed representations for runtimes, compilers, SDKs, package managers, toolchains, package stores, and shared libraries with deterministic URN identity.
2. **Defines the Registry Storage Abstraction**: Clean `VaultRegistry` trait and zero-cost `MemoryVaultRegistry` snapshot backend enforcing collision defense, idempotent deduplication, and conflicting-version rejection without external database dependencies.
3. **Delivers the First Managed Resource Adapter**: Real Windows Rust adapter discovering components (`rustc`, `cargo`, `rust-std`, `rust-mingw`, `rustup`, aggregate toolchain) directly from filesystem evidence in `vault\runtimes\rust\`.
4. **Integrates Native Windows CLI Surface**: Provides `ulvo vault list [--json]`, `ulvo vault inspect <id> [--json]`, `ulvo vault verify <id> [--json]`, and `ulvo vault stats [--json]`.
5. **Maintains Zero Host Mutation & Zero Vault Tampering**: Independent cryptographic hash checks before and after execution proved 100% byte-for-byte preservation across all vault files, configuration files, and test fixtures.

---

## 2. Changed File Inventory

| File Path | Status | Purpose |
| :--- | :---: | :--- |
| [`crates/ulvo-core/src/resource.rs`](file:///E:/Workspace/Active/ULVO/crates/ulvo-core/src/resource.rs) | Created | Generic resource model, deterministic URN `ResourceId`, FNV-1a, platform path normalization, RFC 3339 timestamps |
| [`crates/ulvo-core/src/registry.rs`](file:///E:/Workspace/Active/ULVO/crates/ulvo-core/src/registry.rs) | Created | `VaultRegistry` trait, `MemoryVaultRegistry`, query filtering, stats, reconciliation engine, conflict detection |
| [`crates/ulvo-core/src/lib.rs`](file:///E:/Workspace/Active/ULVO/crates/ulvo-core/src/lib.rs) | Modified | Re-export `resource` and `registry` modules |
| [`crates/ulvo-adapters/src/vault_rust.rs`](file:///E:/Workspace/Active/ULVO/crates/ulvo-adapters/src/vault_rust.rs) | Created | Windows Rust vault adapter, settings/components parser, PE architecture checker, bounded subprocess verifier, filesystem integrity auditor |
| [`crates/ulvo-adapters/src/lib.rs`](file:///E:/Workspace/Active/ULVO/crates/ulvo-adapters/src/lib.rs) | Modified | Re-export `vault_rust` module; expose `run_bounded` and `run_bounded_with_env` |
| [`crates/ulvo-discovery/src/vault.rs`](file:///E:/Workspace/Active/ULVO/crates/ulvo-discovery/src/vault.rs) | Created | Vault registry orchestration, manifest provenance binding, resource discovery, verification dispatch, reconciliation execution |
| [`crates/ulvo-discovery/src/lib.rs`](file:///E:/Workspace/Active/ULVO/crates/ulvo-discovery/src/lib.rs) | Modified | Re-export `vault` module |
| [`crates/ulvo-cli/src/main.rs`](file:///E:/Workspace/Active/ULVO/crates/ulvo-cli/src/main.rs) | Modified | Implement `vault` subcommand (`list`, `inspect`, `verify`, `stats`) with stdout JSON and stderr diagnostic separation |
| [`tests/validate_stage2_6.py`](file:///E:/Workspace/Active/ULVO/tests/validate_stage2_6.py) | Created | Independent automated validation harness for CLI exit codes, JSON schemas, bounded execution, and zero-tampering guarantees |
| [`docs/evidence/stage2-6-vault-registry/`](file:///E:/Workspace/Active/ULVO/docs/evidence/stage2-6-vault-registry/) | Created | Raw execution logs, stdout/stderr captures, and machine-readable `summary.json` |

---

## 3. Resource Model Specification

Every resource managed or indexed by ULVO implements the full 17-field conceptual model:

```rust
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
```

### Deterministic Resource Identity (`ResourceId`)
The `ResourceId` is a deterministic canonical URN distinguishing all dimensions of resource variation:
$$\text{URN} = \text{ulvo}::\langle\text{ecosystem}\rangle::\langle\text{backend}\rangle::\langle\text{platform}\rangle\text{-}\langle\text{arch}\rangle\text{-}\langle\text{abi}\rangle::\langle\text{kind}\rangle::\langle\text{name}\rangle@\langle\text{version}\rangle\#\langle\text{path\_hash}\rangle$$

Where:
- **Platform-Aware Path Normalization**: On Windows, paths are converted to lowercase and slashes normalized (`/`) before computing the 64-bit FNV-1a hex digest. On Unix/Linux, path casing is strictly preserved.
- **Backend & Target Separation**: Windows GNU (`windows-x86_64-gnu`) never collides with WSL Linux (`linux-x86_64-gnu`) or MSVC (`windows-x86_64-msvc`), even for identical names and versions.
- **Relocation Defense**: If an installation path is relocated, the path discriminator changes deterministically, preventing stale verification from being silently inherited.

---

## 4. First Managed Resource Adapter: Windows Rust

The adapter reads actual filesystem evidence from `vault\runtimes\rust\`:
1. **Settings Inspection**: Reads `rustup\settings.toml` to extract `default_toolchain = "1.96.0-x86_64-pc-windows-gnu"`.
2. **Component Enumeration**: Inspects `toolchains\1.96.0-x86_64-pc-windows-gnu\lib\rustlib\components`.
3. **PE Binary Verification**: Audits executable headers (`0x8664` AMD64) for `rustc.exe`, `cargo.exe`, and `rustup.exe`. Corrupt or missing binaries are marked `Modified` or `Missing`.
4. **Manifest Provenance Linkage**: Connects `config/toolchain-artifacts.toml` provenance (`canonical_url`, `expected_sha256`, `verification_policy`) directly to the resource records.
5. **Bounded Verification**: Probes `rustc.exe --version`, `cargo.exe --version`, and `rustup.exe --version` within a 3-second bounded timeout using process-local `RUSTUP_HOME` and `CARGO_HOME`.

---

## 5. Native Windows CLI Surface

```powershell
# 1. List managed resources (human-readable table)
ulvo vault list

# 2. List managed resources (versioned JSON)
ulvo vault list --json

# 3. Inspect a specific resource by ID or name
ulvo vault inspect rustc
ulvo vault inspect rustc --json

# 4. Perform bounded verification of a resource
ulvo vault verify rustc
ulvo vault verify rustc --json

# 5. Display registry statistics
ulvo vault stats
ulvo vault stats --json
```

### Safety & Output Contract
- **Stdout vs Stderr Isolation**: All JSON responses are written exclusively to `stdout` with `schema_version: 1`. Diagnostic banners and operational errors are written strictly to `stderr`.
- **Controlled Exit Codes**:
  - `0`: Successful execution (including empty results).
  - `1`: Resource not found or verification error.
  - `2`: Invalid CLI syntax or rejected mutating subcommand.

---

## 6. Verification Evidence

### A. Workspace Test Suite (39 Tests Passed, 0 Failed)
```
running 7 tests in ulvo_adapters: 7 passed
running 2 tests in ulvo_cli: 2 passed
running 18 tests in ulvo_core: 18 passed
running 12 tests in ulvo_discovery: 12 passed
Total: 39 passed; 0 failed; 0 skipped
```

### B. Independent Validation Harness (`tests/validate_stage2_6.py`)
```
[RECON] Validating binary: E:\Workspace\Active\ULVO\target\x86_64-pc-windows-gnu\debug\ulvo.exe
[RECON] Evidence directory: E:\Workspace\Active\ULVO\docs\evidence\stage2-6-vault-registry
  [PASS] version (exit=0)
  [PASS] help (exit=0)
  [PASS] inventory (exit=0)
  [PASS] inspect-valid (exit=0)
  [PASS] vault-list (exit=0)
  [PASS] vault-list-json (exit=0)
  [PASS] vault-stats (exit=0)
  [PASS] vault-stats-json (exit=0)
  [PASS] vault-inspect-rustc (exit=0)
  [PASS] vault-inspect-rustc-json (exit=0)
  [PASS] vault-verify-rustc (exit=0)
  [PASS] vault-verify-rustc-json (exit=0)
  [PASS] vault-inspect-invalid (exit=1)
  [PASS] vault-inspect-invalid-json (exit=1)
  [PASS] vault-verify-invalid (exit=1)
  [PASS] vault-verify-invalid-json (exit=1)
  [PASS] empty-vault-isolation (exit=0, total=0)
  [PASS] Zero filesystem tampering: all vault, config, and fixture hashes match byte-for-byte.

VERDICT: PASS (17/17 checks passed)
```

### C. Repository Hardening Tests (`tests/test_hardening.py`)
```
Ran 6 tests in 0.219s
OK
```

---

## 7. Security & Reconciliation Limitations

1. **In-Memory Registry**: Stage 2.6 provides an in-memory snapshot populated on-demand from filesystem evidence. Persistent transactional writes (SQLite) are deferred to a later stage when mutations are introduced.
2. **Single-Ecosystem Adapter**: Only Windows Rust is currently indexed as a managed vault resource. Additional language adapters (Python/uv, Node.js/pnpm, Go) will be integrated in subsequent milestones.
3. **Bounded Probing**: Default discovery does not invoke compiler executables; version probing is only performed when explicitly requested via `ulvo vault verify <id>`.
4. **Reparse Point Defense**: All path resolutions strictly reject symlinks and NTFS reparse points/junctions across the vault traversal path.

---

## 8. Next Recommended Implementation Milestone

**Stage 2.7 / Phase 1: Universal Tool Gateway & Project Manifest Materialization**:
1. Implement `bin/ulvo.exe` proxy routing (the global development-tool gateway).
2. Implement declarative project environment materialization from `.ulvo/manifest.toml`.
3. Integrate the second managed ecosystem adapter (`Python` via `uv` or `Node.js`).
