# Changelog

All notable changes to the ULVO project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Planned
- **Content-Addressable Package Store**: Shared hardlinked package caching across isolated environments.
- **Node.js / pnpm Adapter**: JavaScript/TypeScript manifest detection and package store orchestration.

## [0.2.3] - 2026-10-19

### Stage 2.9 — Automated Workspace-Wide Python Virtual-Environment Migration & Storage Reclamation

#### Added
- **Automated Migration CLI Subcommands**: Added `ulvo migrate <scan|plan|verify|apply|retire|status|report>` providing full lifecycle orchestration from read-only scanning to fail-closed verification, atomic integration, and safe legacy environment retirement with `--json` schema support.
- **10 Formal Verification Gates (Gates A–J)**: Implemented exhaustive pre-retirement safety checks:
  - *Gate A*: Reparse point verification (rejects directory junctions, symlinks, and mount points).
  - *Gate B*: Structural hierarchy & direct parentage (enforces `.venv` resides directly inside project root).
  - *Gate C*: Vault resource exclusion (forbids deletion of ULVO vault runtimes or cached environments).
  - *Gate D*: Prohibited category check (excludes Conda environments and shared base interpreters).
  - *Gate E*: Active process liveness check (uses Win32 Toolhelp snapshot and CIM query without self-matching).
  - *Gate F*: File lock & handle contention check (verifies exclusive access across executable binaries and libraries).
  - *Gate G*: Dependency reproducibility check (`LockfilePinned`, `ManifestSpecified`, `DerivedFromInstalled`).
  - *Gate H*: ULVO manifest integration check (validates syntax and schema of `.ulvo/manifest.toml`).
  - *Gate I*: Capacity & functional readiness check (verifies available storage budget and executes ULVO resolve).
  - *Gate J*: Irreplaceable files check (scans for `.git`, user Python scripts, model weights, and datasets).
- **Capacity-Aware Low-Storage Migration Planning**: Formulates prioritized migration queues with a strict 500 MB safety reserve, safely categorizing multi-gigabyte AI/ML environments as `BLOCKED_CAPACITY` to prevent storage exhaustion on constrained volumes.
- **Atomic Manifest Synthesis (`ulvo migrate apply`)**: Synthesizes standard `.ulvo/manifest.toml` integration contracts atomically via `write_atomic` without altering, touching, or invalidating legacy `.venv` directories.
- **Win32 Directory Read-Only Attribute Hardening**: Hardened `safe_reclaim_environment_hierarchy` to strip Windows `ReadOnly` attributes on subdirectories and root directories with exponential retry backoff, fixing `ERROR_ACCESS_DENIED (os error 5)` on protected package subtrees.
- **Verified Production Workspace Retirements**: Safely migrated and retired project-local `.venv`s for `Tools/whichllm` and `Active/ProofStack` after passing 100% of all 10 verification gates, achieving 100.98 MB (100,989,500 bytes) of total verified physical storage recovery across the workspace.
- **Post-Retirement Functional Verification**: Independently verified that retired projects execute cleanly through ULVO's ephemeral vault environments with zero recreation of project-local `.venv` directories.
- **Comprehensive Test Suite Expansion**: Added 6 new unit and synthetic integration tests in `ulvo-adapters` and `ulvo-discovery`, bringing workspace total to 89 passing native tests.

---

## [0.2.2] - 2026-10-19

### Stage 2.8.2 — Legacy .venv Safe Retirement & Reclamation Hardening

#### Added
- **Verified Legacy Environment Retirement**: Safely retired legacy `NexusNode/server/.venv` after independently passing all 11 pre-deletion gates, recovering 34.8 MB (34,807,808 bytes) of physical disk storage across 1,828 files and 147 directories.
- **Post-Deletion Independence Verification**: Independently verified NexusNode/server executes its 40-test suite and `nexus.bat --version` cleanly through ULVO without regenerating or depending on `.venv`.
- **Fail-Closed Lease Metadata Validation**: Hardened `reclaim_internal` so unreadable, corrupted, or malformed `.ulvo_lease.json` metadata strictly aborts reclamation instead of falling through.
- **Full Structural Ownership Record Validation**: Enforced multi-field ownership verification (sentinel, schema version, vault root, environment ID, canonical project path, specification hash) in `reclaim_internal` and `ensure_environment_ready`.
- **Environment Reuse Verification**: Added pre-reuse validation checking ownership record integrity, specification file matching, runtime path consistency, and presence of the environment Python executable.
- **Process Supervisor & Child Lifecycle Hardening**: Process assignment errors from Win32 `JobSupervisor` and child registration failures in `on_spawn` now immediately terminate child processes and propagate errors, preventing unmonitored process leaks.
- **Strict Read-Only Queries**: Eliminated filesystem mutations from `ulvo env status`, `ulvo project doctor`, and inspection workflows when cleaning dead PIDs in memory.
- **Comprehensive Regression Tests**: Added 5 new unit and integration tests bringing workspace total to 83 passing native tests.

---

## [0.2.1] - 2026-10-19

### Stage 2.8.1 — Project-Local .venv Independence: Ephemeral Lifecycle Safety & Concurrency Hardening

#### Added
- **Leaf-First Safe Deletion Engine**: Built `safe_reclaim_environment_hierarchy` in `ulvo-adapters` performing bottom-up leaf-first traversal, rejecting reparse points/junctions, clearing Windows read-only file attributes, and retrying deletion with exponential backoff on transient handles.
- **Cryptographic Specification Fingerprinting**: Implemented `EnvironmentSpecification` and `DependencyManifestFingerprint` in `ulvo-core` and `ulvo-discovery`, generating deterministic 16-character environment hashes across manifests (`requirements.txt`, `pyproject.toml`, `.ulvo/manifest.toml`), Python versions, platforms, and architectures to detect dependency drift and prevent cross-project collisions.
- **Win32 Job Object Process Supervision**: Implemented `JobSupervisor` using `CreateJobObjectW` and `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (0x2000), binding spawned runners and background worker processes to guarantee automatic termination on crash or exit.
- **Fail-Closed Process Liveness Engine**: Added `ProcessLiveness` (`Alive`, `Terminated`, `Unknown`) with `GetProcessTimes` creation timestamp validation to defend against PID reuse, mapping `ERROR_ACCESS_DENIED` to `Unknown` to strictly block reclamation while indeterminate processes exist.
- **Cross-Process Concurrency & Atomic Leases**: Built `EnvironmentFileLock` using Win32 `LockFileEx` exclusive locking with bounded timeout backoff, pairing with `write_atomic` via `MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)` to eliminate lease read-modify-write race conditions.
- **Structural Ownership Record Validation**: Added `validate()` on `EnvironmentOwnershipRecord` enforcing multi-field verification (sentinel, schema version, canonical project path, vault root boundary, and environment ID) before authorizing reclamation.
- **Volume Delta Free Space Telemetry**: Integrated Win32 `GetDiskFreeSpaceExW` to calculate volume free space delta alongside physical cluster allocation from `GetCompressedFileSizeW`.
- **Read-Only Safety Enforcement**: Removed eager directory creation from `UvManager::new()` and `VaultPaths::new()`, deferring writes exclusively to explicit materialization so inspection, doctor, resolution, and status queries remain strictly read-only.
- **Adversarial Regression Test Suite**: Added 11 new unit and adversarial integration tests bringing workspace total to 78 passing tests covering multithreaded lock contention, Job Object assignment, read-only file deletion, PID reuse simulation, and dependency invalidation.

---

## [0.2.0] - 2026-10-19

### Stage 2.8 — Project-to-ULVO Ephemeral Python Environment Integration

#### Added
- **9-State Environment Lifecycle Engine**: Implemented complete state model (`DEFINED`, `RESOLVED`, `MATERIALIZING`, `READY`, `EXECUTING`, `RELEASED`, `RECLAIMABLE`, `RECLAIMED`, `FAILED`) in `ulvo-core` with persistent ownership records (`.ulvo_ownership.json`) and lease tracking (`.ulvo_lease.json`).
- **uv Package & Environment Adapter**: Built `UvManager` in `ulvo-adapters` supporting isolated `--cache-dir` (`vault/packages/python/uv-cache`) and isolated target environments (`vault/environments/python/env_<hash>`).
- **Fail-Closed Process Protection**: Added Win32 `OpenProcess`/`GetExitCodeProcess` liveness checks to prevent environment deletion while active leased processes or child processes exist.
- **PATH Sanitization**: Implemented strict `PATH` filtering removing any references to project-local `.venv` to enforce complete vault execution isolation.
- **CLI Subcommands**: Added `ulvo project inspect`, `ulvo project resolve`, `ulvo project doctor`, `ulvo run [--reclaim]`, `ulvo env status`, and `ulvo env reclaim` with full `--json` support.
- **Pilot Project Integration**: Integrated and verified `NexusNode/server` pilot running 40/40 tests cleanly through ULVO without accessing the old `.venv`.
- **Reclamation Telemetry**: Implemented directory size calculation and verified safe reclamation of 29.25 MB (30,669,189 bytes) upon quiescence.

---

## [0.1.0] - 2026-10-19

### Stage 2.7.1 — Storage-Accounting Hardening & NTFS File Identity

#### Added
- **Native NTFS Filesystem Identity**: Implemented Win32 `CreateFileW` with `FILE_READ_ATTRIBUTES` and `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT` plus `GetFileInformationByHandle` to query exact `(dwVolumeSerialNumber, nFileIndexHigh:Low)` 64-bit Master File Table identities.
- **Physical Cluster Allocation**: Integrated Win32 `GetCompressedFileSizeW` to measure actual allocated disk bytes per unique file identity.
- **Distribution Discrimination**: Added `.dist-info/WHEEL` ABI tag parsing and `.dist-info/RECORD` distribution catalog cryptographic SHA-256 digest checks.
- **Editable Install Detection**: Added scanning for editable virtual environment packages via `.dist-info/direct_url.json` (`"editable": true`, `dir_info`) and `site-packages/__editable__.*.pth` paths.
- **Inaccessible Directory Accounting**: Added tracking of permission-denied and locked directories, surfacing `inaccessible_count` and reporting `INCOMPLETE (<N> inaccessible entries)`.
- **Process-Level Fixture Sentinels**: Added `.ulvo_test_sentinel` validation verifying process PID before executing test fixture cleanup.
- **Regression Test Suite**: Added comprehensive test coverage bringing workspace total to 60 passing tests.

#### Changed
- **Storage Savings Model**: Removed arbitrary heuristic percentage multipliers (e.g. `0.85 * project_env_bytes`). Replaced with a strictly bounded set-theoretic model (`Conservative <= Realistic <= Theoretical <= Total Workspace`).
- **Duplicate Waste Calculation**: Restricted wasted byte reclamation strictly to `[verified identical]` packages, attributing zero wasted bytes to candidates with differing ABI or RECORD digests.
- **Storage Measurement Correction**: Removed the flawed identity assignment that copied logical file size directly as physical allocation.

### Stage 2.7 — Python/uv Workspace Intelligence

#### Added
- **Python/uv Discovery Adapter**: Recursive discovery of Python virtual environments (`pyvenv.cfg`, `conda-meta`).
- **Workspace Audit Engine**: Recursive category-based workspace scanning in `ulvo-discovery`.
- **CLI Workspace Subcommands**: Added `ulvo workspace scan`, `report`, `python-envs`, `duplicates`, and `savings`.

### Stage 2.6 — Universal Vault Registry Foundation

#### Added
- **Universal Vault Registry**: Implemented in `ulvo-core` with deterministic `ResourceId` hashing (`<ecosystem>:<backend>:<category>:<name>:<version>`).
- **Domain Models**: Added `ManagedResource`, `OwnershipCategory`, `VerificationState`, and `ResourceCapability`.
- **Reconciliation Engine**: Added idempotent registration, conflicting version rejection, and stale state transitions.
- **CLI Vault Commands**: Added `ulvo vault list`, `inspect`, `verify`, and `stats` with `--json` formatting.

### Stage 2.5 — Windows-Native Rust Toolchain Provisioning

#### Added
- **Native Windows Rust Toolchain**: Provisioned native Rust 1.96.0 (`x86_64-pc-windows-gnu`) inside single-root `vault/runtimes/rust`.
- **Process-Local Redirection**: Configured process-scoped `CARGO_HOME` and `RUSTUP_HOME`.
- **GNU Linker Integration**: Validated native compilation and linking with MSYS2 UCRT64 GCC linker.

### Stage 2.4 — Windows Rust Provisioning Security Hardening

#### Added
- **Manifest Pinned Artifacts**: Added `config/toolchain-artifacts.toml` with cryptographically pinned URLs and SHA-256 hashes.
- **Safe Download Flow**: Implemented `Invoke-DownloadWithRedirectCheck` enforcing single GET flow, per-hop HTTPS validation, and host pinning.
- **Canonical Root Enforcement**: Prohibited execution with `-Install` under temporary or test directories.
- **Behavioral Test Harness**: Developed 45-test suite in `scripts/test-provision-rust-windows.ps1` with junction isolation and marker tokens.

### Phase 0 — Foundation & Security Invariants

#### Added
- **Initial Cargo Workspace**: Established four-crate architecture (`ulvo-core`, `ulvo-adapters`, `ulvo-discovery`, `ulvo-cli`).
- **Bounded Discovery**: Enforced strict input limits (128 PATH entries, 1 MiB config reads, 64 KiB capture buffers, 3-second timeouts).
- **Reparse Point Rejection**: Enforced rejection of symlinks and junctions during directory walking.
- **CLI Foundation**: Implemented read-only `ulvo inventory` and `ulvo inspect` commands.
