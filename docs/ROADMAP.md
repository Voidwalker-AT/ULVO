# ULVO Development Roadmap

This document outlines the architectural progression, completed engineering milestones, active development, and future roadmap for the ULVO platform.

---

## Roadmap Overview

```mermaid
flowchart LR
    P0["Phase 0<br/>Foundation & Security<br/>(COMPLETED)"]
    S24["Stage 2.4 - 2.5<br/>Windows Toolchain Provisioning<br/>(COMPLETED)"]
    S26["Stage 2.6<br/>Vault Registry Foundation<br/>(COMPLETED)"]
    S27["Stage 2.7 - 2.7.1<br/>Workspace Intelligence & NTFS Identity<br/>(COMPLETED)"]
    PUB["GitHub Publication<br/>Public Checkpoint<br/>(COMPLETED)"]
    S28["Stage 2.8 - 2.8.2<br/>Ephemeral Python Environment Pilot & Retirement<br/>(COMPLETED)"]
    S29["Stage 2.9 - 2.9.2<br/>Workspace Migration, GPU & Managed MSYS2<br/>(COMPLETED)"]
    S30["Stage 2.9.3+<br/>Multi-Language Adapters & Tauri GUI<br/>(UP NEXT)"]

    P0 --> S24
    S24 --> S26
    S26 --> S27
    S27 --> PUB
    PUB --> S28
    S28 --> S29
    S29 --> S30
```

---

## Milestone Status

### Phase 0: Security & Model Foundation
- **Status**: `COMPLETED`
- **Deliverables**:
  - Four-crate Cargo workspace structure (`ulvo-core`, `ulvo-adapters`, `ulvo-discovery`, `ulvo-cli`).
  - Read-only toolchain inventory and project inspection model.
  - Strict input bounds (128 PATH entries, 1 MiB config reads, 64 KiB probe streams).
  - Reparse point traversal rejection and non-mutating `--verify` flags.

### Stage 2.2 – 2.4.2: Windows Rust Provisioning Security Hardening
- **Status**: `COMPLETED`
- **Deliverables**:
  - Fully offline, reproducible toolchain manifest parsing (`config/toolchain-artifacts.toml`).
  - Single-flow redirect validation with per-hop HTTPS and host checking (`Invoke-DownloadWithRedirectCheck`).
  - Production canonical root enforcement (rejects `-Install` under disposable test directories).
  - 45-test behavioral security test suite (`scripts/test-provision-rust-windows.ps1`) covering junction isolation and ownership token validation.

### Stage 2.5: Windows-Native Rust Toolchain Provisioning
- **Status**: `COMPLETED`
- **Deliverables**:
  - Supervised installation of Rust 1.96.0 (`x86_64-pc-windows-gnu`) inside single-root `vault/runtimes/rust`.
  - Process-local redirection via `CARGO_HOME` and `RUSTUP_HOME`.
  - Linking verified using external MSYS2 UCRT64 GCC linker.
  - Zero modification to global PATH, Windows registry, or system permissions.

### Stage 2.6: Universal Vault Registry Foundation
- **Status**: `COMPLETED`
- **Deliverables**:
  - Deterministic `ResourceId` generation with case-normalized path hashing.
  - In-memory registry with conflict detection and idempotency guarantees.
  - `ulvo vault list`, `inspect`, `verify`, and `stats` CLI commands.
  - Automatic reconciliation between on-disk toolchains and registry state.

### Stage 2.7: Python/uv Workspace Intelligence & Storage Optimization
- **Status**: `COMPLETED`
- **Deliverables**:
  - Python virtual environment discovery adapter inspecting `pyvenv.cfg`, `conda-meta`, and `site-packages`.
  - Detection of native C-extension wheels (`.pyd`, `.so`) and editable installs (`direct_url.json`).
  - Multi-tier storage savings estimation (`Conservative`, `Realistic`, `Theoretical`).

### Stage 2.7.1: Storage-Accounting Hardening & NTFS File Identity
- **Status**: `COMPLETED`
- **Deliverables**:
  - Replaced path hashes with native NTFS 64-bit Master File Table indices (`(dwVolumeSerialNumber, nFileIndexHigh:Low)`) via `CreateFileW` and `GetFileInformationByHandle`.
  - Implemented cluster allocation measurement via `GetCompressedFileSizeW`.
  - Package duplication discrimination: classified distributions into `[verified identical]` vs `[compatible candidate - differing ABI/RECORD]`, attributing zero wasted bytes to differing ABIs.
  - Removed arbitrary heuristic multipliers from the storage savings model.
  - Surfaced traversal errors and incomplete scans via `inaccessible_count`.
  - Process-level ownership sentinels (`.ulvo_test_sentinel`) for test fixtures.
  - 60/60 passing automated unit and integration tests.

### Stage 2.8: Project-to-ULVO Ephemeral Python Environment Integration
- **Status**: `COMPLETED`
- **Pilot Target**: `Active\NexusNode\server`
- **Deliverables**:
  - 9-state environment lifecycle engine (`DEFINED` → `RESOLVED` → `MATERIALIZING` → `READY` → `EXECUTING` → `RELEASED` → `RECLAIMABLE` → `RECLAIMED` / `FAILED`) in `ulvo-core`.
  - `UvManager` in `ulvo-adapters` for isolated package caching (`vault/packages/python/uv-cache`) and environment materialization (`vault/environments/python/env_<hash>`).
  - Fail-closed Win32 process liveness checking preventing deletion while active processes exist.
  - Subprocess `PATH` sanitization preventing local `.venv` leakage.
  - Full CLI integration: `ulvo project inspect`, `resolve`, `doctor`, `ulvo run [--reclaim]`, `ulvo env status`, and `ulvo env reclaim`.
  - NexusNode pilot verification with 40/40 tests passing.
  - Safe reclamation telemetry recording 29.25 MB freed upon quiescence.
  - 67/67 automated tests passing across the workspace.

### Stage 2.8.1: Ephemeral Lifecycle Safety, Concurrency Hardening & .venv Independence
- **Status**: `COMPLETED`
- **Pilot Target**: `Active\NexusNode\server`
- **Deliverables**:
  - Leaf-first safe directory deletion engine rejecting reparse points/junctions and clearing read-only attributes with retry backoff.
  - Multi-field ownership record deserialization and canonical path validation.
  - Process liveness engine tracking creation timestamps via `GetProcessTimes` to protect against PID recycling, with `ERROR_ACCESS_DENIED` mapped to fail-closed `ProcessLiveness::Unknown`.
  - Cross-process concurrency control with Win32 `LockFileEx` exclusive locking and atomic `MoveFileExW` temporary file updates.
  - Win32 Job Object supervision (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) guaranteeing clean child and worker process termination.
  - Cryptographic dependency specification fingerprinting across manifests and requirement files.
  - Read-only safety verification ensuring inspection/doctor/resolve/status commands never mutate disk.
  - Dual physical allocation (`GetCompressedFileSizeW`) and volume free delta (`GetDiskFreeSpaceExW`) telemetry.
  - 78/78 passing automated regression tests across the workspace.

---

### Stage 2.9: Workspace-Wide Python Migration & Storage Reclamation
- **Status**: `COMPLETED`
- **Deliverables**:
  - `ulvo migrate` CLI suite (`scan`, `plan`, `verify`, `apply`, `retire`, `status`, `report`).
  - 10 formal verification gates (Gates A–J) enforcing fail-closed safety before retirement.
  - Prioritized migration planning with 500 MB volume safety reserve.
  - Safe retirement of legacy environments for `Tools\whichllm` and `Active\ProofStack`, recovering 100.98 MB physical disk space.
  - 89 passing automated native tests across the workspace.

### Stage 2.9.1: GPU-Aware Python Environment Orchestration & Universal Toolchain Adoption
- **Status**: `COMPLETED`
- **Pilot Target**: `Academic\sem5\DL\exp1` (NVIDIA GeForce RTX 2050 GPU, PyTorch 2.6.0+cu124 on CUDA 12.4)
- **Deliverables**:
  - GPU compatibility domain model (`ulvo-core::gpu`) with typed compute capabilities (`sm_86`), framework build variants, and capacity gates.
  - System GPU driver boundary enforcement strictly prohibiting kernel driver relocation into vault.
  - Typed 9-stage toolchain adoption state machine (`ulvo-core::adoption`) with transition verification and relocation rejection.
  - Universal multi-ecosystem toolchain inventory (`ulvo-discovery::toolchain_inventory`) discovering 29 toolchains across Windows, MSYS2, VS Code, and WSL.
  - Unified Execution Gateway foundation (`ulvo-adapters::gateway`) constructing process-local PATHs without global state modification.
  - Shared GPU package-store hardlink engine (`vault/packages/python/shared`) achieving zero-copy materialization on NTFS volumes.
  - End-to-end verified real GPU execution of `Academic\sem5\DL\exp1` without legacy `.venv` dependencies, passing all 9 GPU verification gates.
  - Project-specific retirement readiness report classifying `exp1` as `READY_FOR_RETIREMENT`.
  - Non-executable GUI architecture and visual branding specification (`docs/GUI_AND_BRANDING_SPECIFICATION.md`).
  - 104/104 passing automated regression tests across the workspace.

### Stage 2.9.2: Universal Vault Adoption, Resource Lifecycle & Storage Recovery
- **Status**: `COMPLETED`
- **Deliverables**:
  - **Mandatory Workspace-Wide Legacy `.venv` Elimination**:
    - Safely retired heavy AI/ML virtual environments for `Academic\sem5\DL\exp1` and `exp2`.
    - Recovered **~9.69 GB** physical storage across the two deep learning projects (**~9.79 GB total recovered** across 5 projects in `E:\Workspace`).
    - Expanded volume free space on `E:` from 6.88 GB to **11.44 GB** (12,283,654,144 bytes).
    - Post-deletion verification confirmed identical execution (PyTorch 2.6.0+cu124 on physical RTX 2050 GPU; 81.09% test accuracy on CNN baseline) with zero `.venv` presence or re-creation.
  - **First-Class Managed MSYS2 C/C++ Toolchain**:
    - Implemented `vault_msys2` manager supporting `Ucrt64`, `Mingw64`, `Clang64`, and `Msys` subsystems.
    - Adopted MSYS2 into `vault/toolchains/msys2` anchored by `.ulvo_managed.json` sentinel and verified provenance.
    - Verified native C (`gcc`) and C++ (`g++`) compilation and execution harnesses via `ulvo toolchain msys2 verify`.
    - Zero mutation or dependencies on external `C:\msys64`.
  - **Unified Execution Gateway (`ulvo exec`)**:
    - Gateway dispatcher routing commands directly to vault-managed toolchains (`ulvo exec [--toolchain <name>] -- <cmd...>`).
    - Isolated process environment constructed with local PATH prepend, preserving system stability without global PATH or Windows registry pollution.
  - **Durable Vault Registry v2**:
    - Persistent registry envelope with SHA-256 payload integrity check.
    - Concurrency protection via `vault_registry.lock` file locking.
    - Append-only transaction journaling via `journal.jsonl`.
    - Auto-discovery and registration of managed MSYS2 toolchains alongside Rust and Python resources.
  - **Regression Test Suite**:
    - 109/109 passing automated unit and integration tests across the workspace.

### Stage 2.9.2a: Emergency Storage Recovery, Toolchain Consolidation & Gateway Hardening
- **Status**: `COMPLETED`
- **Deliverables**:
  - **Emergency Storage Investigation & Space Reclamation**:
    - Identified consumers causing drive E: free space collapse to 3.68 GB (Cargo `incremental` cache, `target-local`, and test fixtures).
    - Safely retired eligible legacy `.venv` directories: `Active\termux-ai-workstation` (3,457.72 MB), `Agents\AstraRun-Anti` (886.37 MB), and `Agents\AstraRun-MiMo` (694.71 MB).
    - Net physical storage reclaimed from legacy environments: **5,038.80 MB (~5.04 GB)** across 50,081 files.
    - Recovered volume free space on drive `E:` back to safe operating margins (**6.75 GB - 7.13 GB**), maintaining strict 500 MB NTFS reserves.
  - **Durability & Read-Only Registry Access**:
    - Added `load_vault_registry_readonly` for non-mutating subcommands (`vault stats`, `vault list`, `vault inspect`, `exec`).
    - Added atomic transaction wrapper `with_vault_registry_transaction` with automatic rollback, fail-closed journaling, and corrupt registry backup.
  - **Unified Execution Gateway Hardening (`ulvo exec`)**:
    - Hardened `GatewayResolver` to strictly preserve only approved Windows system directories (`System32`, `Wbem`, etc.), stripping dirty user PATH entries.
    - Deterministic collision resolution prioritizing `Managed` > `External` > `Unknown`, verified verification status, and numeric semver versions.
    - Validated external execution with `ulvo exec gcc --version` and `ulvo exec rustc --version`.
  - **Deep MSYS2 & Toolchain Consolidation**:
    - Audited MSYS2 UCRT64 toolchain (GCC 14.2.0, G++ 14.2.0, GNU Make 4.4.1, Pacman 6.1.0 across 192 packages; database verified consistent).
    - Preserved external `C:\msys64` untouched and unmutated.
    - Preserved AI/ML project `Academic\sem5\DL\exp5` due to direct git dependency and CUDA 12.1 mismatch.
  - **Full Regression Verification**:
    - 109/109 passing automated native tests across all four workspace crates.

### Stage 2.9.3: Lightweight Project Integration Contract & Thin Connector Governance
- **Status**: `COMPLETED`
- **Deliverables**:
  - **Thin Project Connector Contract**:
    - Mandated `<project>/.ulvo` metadata budget: target size under 100 KiB, max budget 10 MiB.
    - Prohibited all runtime environments, packages, toolchains, build caches, and binaries in `.ulvo/`.
    - Implemented `ulvo-discovery::connector` with recursive scanning, budget enforcement, and reparse-point rejection.
  - **Multi-Ecosystem Manifest Contract**:
    - Extended `ProjectManifest` with typed configurations referencing native manifests (`requirements.txt`, `package.json`, `Cargo.toml`, etc.) without duplicating dependency payloads.
    - Automated environment fingerprint invalidation upon native manifest changes.
  - **Zero Project-Local Managed Environments**:
    - Materialized execution environments strictly inside `vault/environments/<ecosystem>/`.
    - Added post-execution verification to `ulvo run` asserting no project-local `.venv` was created.
    - Verified against real projects (`NexusNode/server`, `termux-ai-workstation`, `AstraRun-Anti`, `AstraRun-MiMo`, `Academic\sem5\DL\exp1`).
  - **CLI Commands**:
    - `ulvo project connector <path>` for full connector compliance auditing.
    - `ulvo project status <path>` for comprehensive lifecycle and connector status.
    - Thin Connector Contract check integrated as Check #0 in `ulvo project doctor`.
  - **Regression Verification**:
    - 121/121 passing automated native tests across all four workspace crates.

### Stage 2.9.4: Universal Content-Addressable Dependency Store & Language Adoption
- **Status**: `COMPLETED`
- **Deliverables**:
  - Pure-Rust FIPS 180-4 streaming SHA-256 CAS engine (`vault/packages/cas`).
  - Coordinated native package caches across 7 ecosystems (`uv-cache`, `shared wheels`, `cargo registry`, `pnpm store`, `GOMODCACHE`, `gems`, `pacman pkg`).
  - Machine-wide toolchain discovery (41 tools) and typed 9-stage adoption plans.
  - Multi-language execution gateway (`ulvo exec --tool <name>`) with Windows verbatim path normalization.

### Stage 2.9.4a: CAS Immutability, Transactional Materialization & Adversarial Hardening
- **Status**: `COMPLETED`
- **Deliverables**:
  - Stream copy to private staging with post-copy cryptographic validation.
  - Active consumer guard (`CAS_CORRUPTED_OBJECT_HELD_BY_ACTIVE_CONSUMERS`) preventing in-place corruption.
  - 4-phase transactional target materialization with NTFS read-only attributes and rollback.
  - Fail-closed compatibility validation with CUDA capability matching (`cuda_sm_86`).
  - Safe garbage collection planning (`--plan` default, journaled unlinking).
  - 22-case adversarial test suite passing across all scenarios.

### Stage 1.0.0: Release Readiness & System-Wide Development Resource Consolidation
- **Status**: `COMPLETED`
- **Deliverables**:
  - Single Execution Gateway Directory (`bin/ulvo.exe`) for single-PATH global integration.
  - Universal CLI surface: `ulvo doctor`, `ulvo catalog`, `ulvo storage`, `ulvo toolchain list/install/verify`, `ulvo project init`.
  - Automated multi-toolchain verification across all 7 ecosystems (MSYS2 C/C++, Rust, Python, Ruby, Go, Node.js, Java) returning `ALL_VERIFIED`.
  - Minimal Thin Connector compliance generator (< 100 KiB budget, zero runtime bloat).
  - Dynamic Vault Root and portable CLI defaults (`"."`).
  - Release packaging script (`scripts/package-release.ps1`) generating `dist/ulvo-v1.0.0-windows-x86_64.zip` with SHA-256 checksums.
  - 155 workspace tests passing with 0 failures.

---

### Stage 3.0: Zero Persistent Project Environments, Automatic Reclamation & Storage Cache Governance
- **Status**: `COMPLETED`
- **Deliverables**:
  - Vault environment inspection engine auditing all environments under `vault/environments/python` into `ACTIVE`, `RECLAIMABLE`, `ABORTED`, and `UNKNOWN`.
  - Automated safe vault garbage collection (`ulvo env prune`) recovering inactive and aborted materializations while strictly protecting active leases and foreign directories.
  - Package manager cache governance (`ulvo storage cache-audit` and `ulvo storage cache-prune`) invoking `uv cache prune` to safely reclaim unreferenced archives while preserving shared packages.
  - Default `EPHEMERAL` lifecycle enforcement in `ulvo run`: automatic reclamation upon quiescence unless `--keep-env` is specified.
  - Try-finally lease release guaranteeing lease cleanup even on application crash or failure.
  - Pre-materialization cleanup of aborted/corrupt directories and transactional rollback on materialization failure.
  - Reclaimed 11 lingering environments totaling 18.21 GB (18,613.11 MB) from `vault/environments/python`.
  - Verified end-to-end execution and immediate post-run zero residual environments on `Academic\sem5\DL\exp1` (CUDA PyTorch), `DL\exp2` (CNN baseline), `CrossModalLab` (hardware preflight), `termux-ai-workstation` (pytest), and `AstraRun` (pytest).
  - 159 workspace tests passing with 0 failures across all 4 crates.

---

## Future Roadmap

### Stage 3.1: Desktop Control Center (Tauri 2.0 GUI)
- **Status**: `BLOCKED` (Pending host Tauri 2.0 / web frontend build prerequisites)
- **Objectives**:
  - Native Windows desktop application consuming the Rust core via Tauri 2.0.
  - Interactive cockpit for GPU telemetry, active leases, storage deduplication, and staged toolchain adoption.
  - Architectural and visual design fully specified in [docs/GUI_AND_BRANDING_SPECIFICATION.md](GUI_AND_BRANDING_SPECIFICATION.md).
