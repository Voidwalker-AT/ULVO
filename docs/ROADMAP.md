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
    S28["Stage 2.8<br/>Ephemeral Python Environment Pilot<br/>(COMPLETED)"]
    S29["Stage 2.9+<br/>Multi-Language Orchestration & GUI<br/>(UP NEXT)"]

    P0 --> S24
    S24 --> S26
    S26 --> S27
    S27 --> PUB
    PUB --> S28
    S28 --> S29
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

---

## Future Roadmap

### Stage 2.9: Multi-Ecosystem Adapters
- **Status**: `PLANNED` (Recommended Next Milestone)
- **Ecosystems**:
  - **Node.js / JavaScript**: Integration with `pnpm` content-addressable store and `package.json` manifest parsing.
  - **Go**: Integration with `GOPATH`/`GOMODCACHE` and `go.mod` module management.
  - **Java / JVM**: Integration with Maven/Gradle caches and JDK toolchain management.

### Stage 3.0: Desktop Control Plane (GUI)
- **Status**: `PLANNED`
- **Objectives**:
  - Native Windows desktop application providing interactive workspace storage visualization.
  - Visual dependency graph explorer.
  - One-click ephemeral environment lifecycle controls.
