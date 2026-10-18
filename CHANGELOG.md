# Changelog

All notable changes to the ULVO project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Planned
- **Stage 2.8 Ephemeral Python Environment Pilot**: Controlled ephemeral virtual environment orchestration for `Active\NexusNode\server`.
- **Content-Addressable Package Store**: Shared hardlinked package caching across isolated environments.
- **Node.js / pnpm Adapter**: JavaScript/TypeScript manifest detection and package store orchestration.

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
