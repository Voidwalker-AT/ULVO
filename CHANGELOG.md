# Changelog

All notable changes to the ULVO project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

- Add shared native Windows configuration resolution and a resumable, reviewed metadata-only setup flow in CLI and desktop.
- Bound optional tool discovery and retain external ownership when consumers or relocatability are unknown.
- Add connector-only migration batch review and selection; resolution alone can no longer authorize retirement.
- Add bounded local operation logs, explicit supervised native-tool capture, redacted export, crash events and exact-plan registry repair with backups.
- Make agent startup opt-in, request an available port on managed startup, and package qualification artifacts without deploying or overwriting earlier output.
- Document WebView2 requirements, privacy, recovery, performance targets and blocked independent Windows acceptance.

- Resolve legacy relative managed executables against the configured vault rather than the shell working directory.
- Normalize extended Windows paths during toolchain ownership checks and inventory deduplication.
- Support long Windows paths in atomic writes, flush staged contents before replacement, and preserve the original Win32 failure code.
- Query ordinary-file allocation with `FileStandardInfo`; retain compressed/sparse accounting and NTFS file-identity deduplication.
- Honor the explicit `storage report` directory and include CAS integrity failure in the doctor verdict.
- Bound agent IPC request time, reassemble fragmented requests, serialize agent startup per vault, and use system randomness for newly generated authentication tokens.
- Make agent status inspect lease metadata without a recursive storage audit. Run heavy desktop reads on background workers, render the actual environment audit schema, deduplicate vault file identities, and query live volume capacity.
- Harden in-progress project registry and watcher work: reject corrupt/future registries, reject stale writes under an OS lock, avoid watching vault/cache output, and decode native notification buffers without unaligned pointer casts.

## [1.2.0] - 2026-10-20

### Stage 3.4 — Production Reliability Blockers, Runtime Independence & Performance Hardening

#### Package Store Safety & Inode Isolation (`scripts/test-package-store-safety.ps1`)
- **NTFS Atomic Inode Severing Verification**:
  - Proved that atomic cache file replacement using `MoveFileEx(..., MOVEFILE_REPLACE_EXISTING)` allocates a brand new NTFS inode to the target directory entry and decrements the shared-store inode's reference count, preventing any corruption or modification of immutable package store files.
  - Repaired test suite precondition in `scripts/test-package-store-safety.ps1` to cleanly handle Win32 `ERROR_ALREADY_EXISTS` (183).
  - Verified immutable store behavior across 8 safety dimensions: Inode reference counts, cache prune unlink resilience, atomic replacement safety, in-place write blocking, 5-thread concurrent reader isolation, interrupted transaction resilience, read-only attribute hardening (17,714 files), and PyTorch 2.6.0+cu124 tensor matmul on physical NVIDIA GeForce RTX 2050 Laptop GPU.
  - Generated audit evidence at `docs/evidence/stage3-4/package-store-safety-audit.json`.

#### Physical Runtime Independence & Hermetic Packaging
- **Vendored Standalone uv & Standalone Vault Python**:
  - Vendored pinned `uv.exe` (`v0.13.1`, SHA-256: `4075A09490327D2BEDFACC807FA48252FB93647C5BD7EF28F00E579160EA3FB5`) directly into `vault/bin/uv.exe` with cryptographic checksum verification (`vault/bin/uv.checksum.txt`).
  - Staged complete standalone CPython 3.11.16 distribution into `vault/runtimes/python/cpython-3.11.16-windows-x86_64-none` (with alias `cpython-3.11-windows-x86_64-none`) containing full standard library, C headers (`include/Python.h`), and link libraries (`libs/python311.lib`).
  - Updated `ulvo-adapters` (`vault_uv.rs` and `vault_python.rs`) to prioritize `vault_root/bin/uv.exe`, `ULVO_VAULT/bin/uv.exe`, and executable-relative paths before checking host PATH.
  - Configured `UvManager` across all subcommands with `UV_PYTHON_INSTALL_DIR = vault/runtimes/python` and `UV_CACHE_DIR = vault/packages/python/uv-cache`.
  - Documented audit evidence at `docs/evidence/stage3-4/runtime-independence-audit.json` and provenance at `docs/evidence/stage3-4/uv-provenance.json`.

#### Fast Execution Profiling & Warm Launch Optimization (`scripts/generate-fast-execution-profile.ps1`)
- **10.28x Speedup on Warm Execution Path**:
  - Diagnosed warm launch bottleneck (previously 1614 ms): eliminated recursive abandoned-environment directory walks (`scan_and_reclaim_abandoned_environments`) from the warm execution path in `project_lifecycle.rs`.
  - Fast-pathed Python runtime discovery by checking the vault runtimes directory directly, avoiding redundant `uv.exe python find` subprocess invocations on warm runs.
  - Reduced warm `ulvo run` launch latency from **1614 ms to 157 ms** (**10.28x speedup**).
  - Reduced `ulvo project status` inspection latency from **1475 ms to 84 ms** (**17.56x speedup**).
  - Reduced `ulvo doctor` health check latency from **1530 ms to 860 ms**.
  - Documented benchmark profile at `docs/evidence/stage3-4/fast-execution-profile.json`.

#### IDE Integration & Developer Ergonomics Corrections (`scripts/test-ide-integration.ps1`)
- **Rigorous IDE Configuration Validation**:
  - Replaced non-existent `${workspaceFolder}/.ulvo/runtime/...` paths in `.vscode/settings.json` with deterministic vault environment paths (`E:/Workspace/Active/ULVO/vault/environments/python/.../Scripts/python.exe`).
  - Corrected Windows Terminal GUID to valid RFC 4122 hexadecimal syntax (`{a63d95b0-13f5-4c09-9069-000000000001}`).
  - Replaced fragile `cd` shell alias overrides with non-destructive PowerShell prompt window-title hooks (`$host.UI.RawUI.WindowTitle`).
  - Added Windows-native `.cmd` pre-commit hook alongside POSIX shell fallback.
  - Verified 100% pass across all 4 IDE checks in `docs/evidence/stage3-4/ide-integration-validation.json`.

#### Storage Accounting & Retirement Matrix Reconciliation
- **Mathematical Unit Reconciliation & Ground Truth Audit**:
  - Reconciled decimal GB (`10^9`) and binary GiB (`2^30`) accounting: proved that user-observed 22.5 GiB and agent-reported 24.17 GB are the exact same measurement (`24.17 * 10^9 / 1073741824 = 22.509 GiB`).
  - Recorded exact volume statistics: Drive `E:` has **30.19 GB** free (**28.11 GiB**); Drive `C:` has **20.86 GB** free (**19.43 GiB**).
  - Disproved phantom `C:\Ruby34-x64` (confirmed host has `C:\Ruby40-x64` at 1.22 GB) and corrected host Cargo record from 3.2 GB to 0 MB (Rust toolchain already vaulted on `E:`).
  - Preserved all host tools with **strictly zero deletions on C:**.
  - Documented in `docs/evidence/stage3-4/storage-reconciliation-and-retirement.json` and `docs/evidence/stage3-4/external-toolchain-retirement-matrix.md`.

#### Full Polyglot & Workspace Regression Gates
- **Comprehensive Verification Suite (`scripts/test-real-project-regression.ps1` & `scripts/run-workspace-tests.ps1`)**:
  - **11/11 Polyglot & Hardware Verifications Passed**: CrossModalLab PyTorch CUDA on RTX 2050, zero `.venv` bloat, NexusNode unittests, socket binding, AstraRun Python 3.13, Go 1.26.5 gateway execution, Ruby 4.0.1 gateway execution, MSYS2 GCC 14.2.0 C++ compilation and run, Node.js 24.15.0 gateway execution, Java Adoptium JDK 21 compilation and run, and NexusNode HTTP lifecycle shutdown.
  - **128/128 Cargo Workspace Unit Tests Passed Cleanly** across `ulvo-adapters`, `ulvo-core`, `ulvo-discovery`, and `ulvo-services`.
  - Evidence recorded at `docs/evidence/stage3-4/project-regression-suite.json`.

### Stage 3.3 — Universal Multi-Ecosystem Onboarding, Global Gateway & Machine-Wide Consolidation

#### Runtime Independence & PATH Isolation Audit (`scripts/test-runtime-independence.ps1`)
- **Hermetic Toolchain Verification**:
  - Audited all managed runtimes under strict, hermetic Windows PATH isolation (`C:\Windows\System32;C:\Windows;C:\Windows\System32\Wbem`).
  - Rust 1.96.0 (`vault/runtimes/rust/cargo`) and MSYS2 GCC 14.2.0 (`vault/toolchains/msys2/ucrt64`) verified as `FULLY_MANAGED` inside the vault, compiling and executing independently of host tools.
  - Python categorized as `MANAGED_WITH_EXTERNAL_DEPENDENCIES` (package assets hosted in vault, orchestrated via standalone `uv` binary).
  - Go 1.26.5, Ruby 4.0.1, Node.js 24.15.0, and Java 21.0.4 verified as `EXTERNAL_REFERENCE` resolved deterministically through the ULVO Unified Gateway.
  - Generated complete audit evidence at `docs/evidence/stage3-3/runtime-independence-audit.json`.

#### Package Store Safety & Immutability Enforcement (`scripts/test-package-store-safety.ps1`)
- **Hardlink Severing & Read-Only Protection**:
  - Verified NTFS hardlink persistence under cache pruning: deleting cache entries severs the link without affecting shared store files (hardlink count decrements cleanly from 2 to 1).
  - Enforced `ReadOnly` attributes across all 17,714 files in `vault/packages/python/shared` (4.512 GB logical), preventing in-place mutation or corruption by downstream package managers or editable installs.
  - Re-verified PyTorch CUDA tensor matmul on physical NVIDIA GeForce RTX 2050 Laptop GPU under strict read-only store protection.
  - Generated complete audit evidence at `docs/evidence/stage3-3/package-store-safety-audit.json`.

#### Whole-Vault Physical Storage Accounting (`scripts/measure-whole-vault-accounting.ps1`)
- **Global Inode Deduplication Across 15 Categories**:
  - Engineered whole-vault dual-accounting scanner tracking Win32 `(VolumeSerialNumber, FileIndexHigh, FileIndexLow)` identities globally.
  - Audited 198,164 files across 15 vault and workspace categories.
  - Reconciled **24.818 GB** (23.114 GiB) logical allocation against **19.850 GB** (18.487 GiB) unique physical disk consumption.
  - Verified that 13,590 files (4.39 GB) in `vault/packages/python/shared` share physical inodes with `vault/packages/python/uv-cache`, adding only ~122 MB of unique physical bytes on disk.
  - Documented complete inventory breakdown at `docs/evidence/stage3-3/whole-vault-physical-accounting.json`.

#### Universal Multi-Ecosystem Project Onboarding Engine (`ulvo project onboard`)
- **Automated Ecosystem Detection & Thin Connector Generation**:
  - Implemented `OnboardingPolicy` (`MANUAL`, `PROMPT`, `AUTO_FOR_TRUSTED_WORKSPACES`) and `EcosystemKind` in `ulvo-core`.
  - Added recursive project scanner in `ulvo-discovery` detecting Python (`pyproject.toml`, `requirements.txt`), Node.js (`package.json`), Rust (`Cargo.toml`), Go (`go.mod`), Ruby (`Gemfile`), Java (`pom.xml`, `build.gradle`), and C/C++ (`CMakeLists.txt`, `Makefile`).
  - Added monorepo container traversal (scanning `apps/`, `services/`, `packages/`, `crates/`, `modules/` up to 2 levels deep).
  - Added CLI command `ulvo project onboard [PATH] [--policy <policy>] [--force] [-y]`.
  - Automated 12-test onboarding verification suite (`scripts/test-universal-onboarding.ps1`): 12/12 passed (`docs/evidence/stage3-3/universal-onboarding-verification.json`).

#### Global Gateway Registry & User PATH Hygiene (`scripts/verify-global-gateway.ps1`)
- **Zero Language Pollution Invariant**:
  - Registered `E:\Workspace\Active\ULVO\bin` in Windows User PATH.
  - Detected and permanently purged 8 leaked ephemeral environment script paths from User PATH in Windows Registry.
  - Verified global `ulvo --version` invocation from `C:\`, `E:\`, and temporary folders.
  - Verified tool missing diagnostics: missing tools fail gracefully with clear actionable registry errors.
  - Verified that underlying language binaries (`cargo.exe`, `gcc.exe`, `g++.exe`) remain hidden from global PATH, preventing machine-wide toolchain collisions.
  - Evidence documented at `docs/evidence/stage3-3/global-gateway-verification.json`.

#### IDE Integration Assessment & Toolchain Retirement Matrix
- **IDE Integration Blueprint (`docs/evidence/stage3-3/ide-integration-assessment.md`)**:
  - Formulated comprehensive configuration patterns for VS Code, Cursor, Windsurf, Google Antigravity, PowerShell profiles, Windows Terminal, and Git pre-commit hooks to eliminate unmanaged `.venv` spawning.
- **Machine-Wide External Toolchain Retirement Matrix (`docs/evidence/stage3-3/external-toolchain-retirement-matrix.md`)**:
  - Cataloged 11.75 GB of external toolchains on `C:` (Rustup, MSYS2, Python 3.11/3.13, Go, Ruby, Node, Java).
  - Defined prerequisite verification, transition phases, and rollback mechanisms.
  - Enforced strict Human Acceptance Gate: **Zero external files or toolchains deleted on C:**.

#### Real Project Regression Suite (`scripts/test-real-project-regression.ps1`)
- **11/11 Passing Real-World Verifications**:
  - `CrossModalLab`: PyTorch CUDA 4096x4096 matmul completed on NVIDIA GeForce RTX 2050 GPU (470 ms, 200 MB VRAM) with zero `.venv` project tree bloat.
  - `NexusNode`: 20 unit tests passed; network socket binding and clean lifecycle release verified on port 58945.
  - `NexusNode`: HTTP server startup, `GET /health` request handling (HTTP 200 `{"status": "healthy", "engine": "NexusNode", "orchestrator": "ULVO"}`), and clean socket shutdown verified on port 58948.
  - `AstraRun`: Python 3.13.0 resolved and executed inside vault environment.
  - Polyglot execution: Go 1.26.5 compiled and ran, Ruby 4.0.1 executed, MSYS2 GCC 14.2.0 compiled and ran native C++ binary, Node.js v24.15.0 executed via gateway.
  - Polyglot Java: Eclipse Adoptium Temurin OpenJDK 21 compiled native Java class with `javac` and executed with `java` through the gateway.
  - Evidence documented at `docs/evidence/stage3-3/project-regression-suite.json`.

#### Resource Governance & Latency Benchmarks (`scripts/benchmark-resource-governance.ps1`)
- Direct Native Python: 31 ms median startup latency.
- ULVO Cold Start: 1,576 ms (complete lifecycle: connector contract verification, specification hash, environment resolution, execution, and lease cleanup).
- ULVO Warm Start: 1,608 ms median repeated execution.
- Memory Working Set: Native Win32 CLI parent working set bounded at 6.8 MB (< 15 MB budget, zero garbage collector pauses).
- Evidence documented at `docs/evidence/stage3-3/resource-governance-benchmark.json`.

#### Full Test Suite Verification
- **182 unit and integration tests passing** across all workspace crates with zero failures (`cargo test`).

---

### Stage 3.2 — Storage Optimization, Package Store Deduplication & Toolchain Consolidation

#### Storage Footprint Audit & Verified Physical Accounting
- **High-Performance NTFS Metadata Scanner (`scripts/measure-fast-storage-baseline.ps1`)**:
  - Engineered queue-based directory traversal with reparse point exclusion (0 loop cycles).
  - Queried logical sizes and cluster allocations via Win32 `GetCompressedFileSizeW` with zero file locking or full stream reads.
  - Processed 186,851 files at 2,800+ files/second across 15 vault and workspace categories in ~53 seconds.
  - Initial baseline: 23.25 GB logical, 22.88 GB unique physical; Drive E: 24.17 GB free, Drive C: 21.27 GB free.

#### Package Store Deduplication (`scripts/deduplicate-package-store.ps1`)
- **NTFS Hardlink Deduplication Engine**:
  - Audited exact byte-level duplication between `vault/packages/python/shared` (PyTorch, torchvision, numpy, pillow, sympy) and `vault/packages/python/uv-cache/archive-v0`.
  - Verified 13,590 identical files across 21 package distributions via cryptographic SHA-256 matching.
  - Replaced duplicate allocations with atomic NTFS hardlinks (`CreateHardLink` + `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`).
  - Both paths remain 100% intact and functional with zero project mutation; reclaimed **4.412 GB** (4,737,593,344 bytes) of physical storage on drive E:.

#### Build Artifact Governance & Disposable Intermediates Pruning
- **`ulvo storage build-prune --all`**:
  - Audited `target/release` compiler intermediates and release build dependencies (`build/`, `deps/`, `.fingerprint/`).
  - Pruned 3,496 disposable compiler intermediates (1,588.94 MB) while strictly preserving all essential release binaries (`ulvo.exe`, `ulvo-agent.exe`, `ulvo-desktop.exe`, `WebView2Loader.dll`).
  - Reclaimed **1.329 GB** (1,394,008,064 bytes) of physical storage.
  - Drive E: free capacity expanded from 24.17 GB to **32.84 GB free** (**+8.67 GB** net physical gain).

#### Machine-Wide External Toolchain Inventory & Adoption Assessment
- **Discovery & Measurement on `C:`**:
  - Audited `C:\msys64` (5.097 GB, 69,699 files), `C:\Program Files\nodejs` (3.854 GB, 168,542 files), `C:\Ruby40-x64` (1.196 GB, 43,544 files), `Python313` (1.063 GB, 40,419 files), `Eclipse Adoptium JDK` (0.319 GB, 487 files), and `Go` (0.220 GB, 15,026 files).
  - Total external duplicate footprint identified on `C:`: **11.749 GB** across 337,717 files.
  - Formulated comprehensive External Duplicate Toolchain Retirement Report (`docs/evidence/stage3-2/external-duplicate-retirement-report.md`).
  - Enforced strict Human Approval Gate: Zero files deleted or moved on `C:` without explicit user sign-off.

#### Unified Execution Gateway & Multi-Language Verification
- **Multi-Ecosystem Resolution**:
  - Verified gateway dispatch for MSYS2 GCC/G++ 14.2.0, Go go1.26.5, Ruby 4.0.1, Node.js v24.15.0, and Java 21.0.4.
  - Compiled and executed native C++ program with vault-managed MSYS2 G++ 14.2.0 (`Sum = 55`).
  - Built and executed Go verification program with go1.26.5.
  - Executed Ruby script with Ruby 4.0.1.
  - Compiled and executed Java class with OpenJDK 21.0.4.

#### Project Compatibility Verification
- **Real-World Project Execution**:
  - Verified `CrossModalLab` under `ulvo run`: Loaded PyTorch 2.6.0+cu124 from shared package store, executed CUDA tensor operations on physical NVIDIA GeForce RTX 2050 GPU (`cuda:0`), and cleanly reclaimed the ephemeral environment.
  - Verified `NexusNode` under `ulvo run`: Executed on Python 3.11.16 with zero local virtualenv pollution.
  - Verified `AstraRun` under `ulvo run`: Executed on Python 3.13.0 with zero local virtualenv pollution.
  - Zero lingering environments in `vault/environments`.

#### Full Test Suite Verification
- 177 unit and integration tests passing across all workspace crates with zero regressions (`cargo test`).

---

### Stage 3.1.3 — Final Production Reliability, Real-World Dogfooding, Performance Profiling & Release Preparation

#### Transactional Installer & Uninstaller Security Hardening
- **Atomic Directory Swapping & Journaling**:
  - Rewrote `scripts/install-ulvo.ps1` with sibling staging (`.ulvo_stage_<txnId>`) and atomic `[System.IO.Directory]::Move` directory swaps.
  - Implemented state journaling (`.ulvo_txn_<txnId>.journal.json`) and automated interrupted transaction crash recovery.
  - Automated migration and preservation of user custom settings (`config\preferences.json`) across upgrades.
- **Two-Phase Uninstaller Security & Cryptographic Validation**:
  - Rewrote `scripts/uninstall-ulvo.ps1` with mandatory `.ulvo-install-manifest.json` ownership validation.
  - Phase 1 security invariants check: Rejects directory traversal (`..`), alternate data streams (`:`), rooted paths, and sibling-prefix escapes before touching disk.
  - Per-file SHA-256 validation preserves user-modified files; vault data and user projects are strictly preserved.
- **7-Suite Installer Lifecycle Verification (`scripts/test-installer-lifecycle.ps1`)**:
  - Verified clean install, execution, transactional upgrade with separate staging, crash recovery at `STAGED` and `BACKUP_CREATED`, 4 manifest attack vectors blocked, hash mismatch preservation, and clean uninstallation.

#### Real Isolated Provisioning Lifecycle Engine (`scripts/test-real-provisioning.ps1`)
- **10-Step Transaction Validation**:
  - Catalog lookup -> version selection -> download -> SHA-256 validation -> staging -> validation -> registry commit -> launch -> rollback -> removal.
  - Eliminated fake `.exe` files containing batch text in favor of properly named `.cmd` wrappers and bounded execution timeouts.
  - Verified atomic registry update and automatic rollback on SHA-256 corruption.

#### Deep Performance & Full Process-Tree Profiling (`scripts/benchmark-deep-performance.ps1`)
- **Multi-Run Latency Baselines**:
  - Direct Native Python: Min 1009 ms, Median 1011 ms, P95 1054 ms
  - ULVO Cold Start: Min 2015 ms, Median 2016 ms, P95 2017 ms
  - ULVO Warm & Repeated: Min 2020 ms, Median 2027 ms, P95 2032 ms
  - Environment Reclamation: Min 1009 ms, Median 1016 ms, P95 1057 ms
- **Full Process-Tree Memory Profiling**:
  - CLI Execution: Peak parent `ulvo.exe` WorkingSet = 6.80 MB, child `python.exe` = 5.05 MB (Total: 11.85 MB).
  - Desktop GUI: Main window WorkingSet = 37.16 MB, WebView2 child = 120.21 MB (Total: 157.46 MB).
  - GPU Telemetry: NVIDIA GeForce RTX 2050 (4096 MiB VRAM, 518 MiB used, Driver 596.49).
  - Stored machine-readable evidence to `docs/evidence/stage3-1-3/deep-performance-benchmark.json`.

#### Multi-Language Regression Suite (`scripts/test-multilang-regression.ps1`)
- Verified 6/6 test suites across:
  - Python standard library & math calculations
  - `NexusNode/server` (Python 3.11.16)
  - `CrossModalLab` (PIL 12.3.0)
  - `Academic/sem5/DL/exp1` (PyTorch 2.6.0+cu124 with NVIDIA GeForce RTX 2050 CUDA acceleration)
  - Universal Gateway multi-language execution (Go 1.26.5, Ruby 4.0.1, MSYS2 GCC 14.2.0)
  - Invariant assertion: Zero project-local `.venv` folders and zero lingering active environments.

#### Real Installed Desktop GUI Dogfooding (`scripts/test-real-installed-gui.ps1`)
- Verified isolated installation into `$env:TEMP\ulvo_gui_installed`.
- Launched real installed `bin\ulvo-desktop.exe`, located Win32 HWND, verified `ULVO Control Center` title, window geometry (1214x858), and responsive state.
- Verified all 8 desktop backend data views (Doctor, Inventory, Catalog, Project Connector, Lifecycle Leases, Package Cache, Storage Intelligence, Background Agent).
- Verified graceful exit upon `WM_CLOSE` and clean uninstallation without vault data deletion.

#### Human Acceptance Gate & Release Artifacts
- **Human Acceptance Protocol (`USER_ACCEPTANCE_TEST.md`)**:
  - Detailed 15-step manual testing protocol with exact PowerShell actions, expected outcomes, red-flag indicators, and human sign-off template.
- **Deterministic Distribution Package**:
  - `dist/ulvo-v1.2.0-windows-x64-portable.zip` (11.72 MB, SHA-256: `63062d0a42c24fde12fddb62728c9b66c67ee3573f878bf011637c398d35c618`)
  - `dist/SHA256SUMS.txt`

### Stage 3.1.2 — Real Desktop GUI Dogfooding, Safety Hardening, Gateway Benchmarks & Release Candidate Validation

#### Critical Safety Repairs
- **Transactional Staging & Rollback**:
  - Hardened `scripts/install-ulvo.ps1` with timestamped rollback snapshots (`.rollback_<timestamp>`) of existing installations prior to commit. Automatically restores the exact previous version upon staging or validation errors.
  - Added `-NoRegistry`, `-NoShortcuts`, and `-NoPath` flags to installer and uninstaller for fully isolated integration testing without mutating user configuration.
- **Unsafe Vault Purge Eliminated**:
  - Completely removed `PurgeVault` and all recursive deletion options from `scripts/uninstall-ulvo.ps1`. The vault and user project data are permanently and unconditionally preserved during uninstallation.
- **Manifest-Only Uninstaller Deletion**:
  - Replaced background `cmd.exe /c ... rmdir /s /q` with strict manifest-only file unlinking driven by `.ulvo-install-manifest.json`.
  - Preserves custom user files, unknown configurations, and user data in the program directory.
- **Non-Following Build Cleanup**:
  - Implemented `safe_clean_path_non_following` in `crates/ulvo-services/src/build_governance.rs`.
  - Enforces canonical containment within target directories and unlinks directory junctions/symlinks without traversing into target contents.
- **Cryptographic SHA-256 Download Integrity**:
  - Hardened `ProvisioningEngine::execute_transaction` in `crates/ulvo-services/src/provisioning.rs` to detect checksum mismatches and corrupt archives.
  - Automatically purges corrupted staging directories and logs to `rollback.log` before returning fail-closed errors.

#### Independent Desktop GUI & Gateway Dogfooding
- **Real Packaged Desktop Application**:
  - Launched `target/release/ulvo-desktop.exe` in release mode. Located visible interactive window titled `ULVO Control Center` (`1214x858`, `Responding=True`).
  - Verified main process working set (`21.58 MB`) and WebView2 helper working set (`10.71 MB`).
  - Verified clean window termination via `WM_CLOSE` and immediate relaunch without port or file lock collisions.
  - Verified 8 backend service endpoints serving Catalog, Universal Search, Installed Resources, Projects, Storage Governance, Cache Audit, Background Agent, and System Settings.
- **Real Multi-Language Project Execution & Gateways**:
  - `NexusNode/server`: Executed Python 3.11.16 through ULVO gateway with zero project-local `.venv` and zero residual environments.
  - `CrossModalLab`: Executed PIL 12.3.0 through ULVO gateway with zero residual disk churn.
  - `Go`: Executed `ulvo exec --tool go -- version` -> `go1.26.5 windows/amd64`.
  - `Ruby`: Executed `ulvo exec --tool ruby -- -v` -> `ruby 4.0.1 [x64-mingw-ucrt]`.
  - `MSYS2 C/C++`: Verified native C and C++ compilation and execution via `ulvo toolchain msys2 verify`.
  - `GPU Deep Learning (exp1)`: Executed PyTorch 2.6.0+cu124 via `ulvo run`. Verified `CUDA Available: True`, Device: `NVIDIA GeForce RTX 2050`, zero residual disk churn, and zero project-local `.venv`.
- **Thin Project Connector Audit**:
  - Initialized thin connectors across projects. Verified connector directory `.ulvo/manifest.toml` is only 130–462 bytes (well within the <100 KiB budget).
- **Performance Benchmarking & Concurrency**:
  - `ulvo --version`: 21.7 ms
  - `ulvo catalog list`: 12.3 ms
  - `ulvo project connector`: 19.3 ms
  - `ulvo run` (Cold start): 1,390 ms
  - `ulvo run` (Warm start): 1,239 ms
  - Desktop GUI startup to interactive window: 1,030 ms
  - CLI Peak Working Set: 6.86 MB; Private Bytes: 1.41 MB
  - Concurrency: Executed simultaneous workloads in `NexusNode/server` and `CrossModalLab` with zero lock collisions, zero manifest corruption, and exit code 0 for both processes.
- **Failure Injection Test Suite (6 / 6 Passed)**:
  - Insufficient disk space: 500 MB hard reserve preflight actively enforced.
  - Corrupt download: Bit-rot and SHA-256 mismatch detected, staging purged, rollback logged.
  - Active lease protection: Pruning during running workload reported `Blocked Active (Protected): 1` and preserved running environment until process exited cleanly.
  - Process crash recovery: Abandoned staging directories detected and purged cleanly without residual locks.
  - Offline catalog resilience: Served 10 catalog entries locally in 19 ms with zero network calls.
  - Offline agent fallback: Seamless in-process direct CLI execution when background agent service is offline.
- **Physical Storage Baseline Reconciliation**:
  - Reclaimed 384.12 MB of disposable compiler intermediates via `ulvo storage build-prune`.
  - Drive E: physical available capacity verified at **24,164,048,896 bytes (24.16 GB decimal / 22.504 GiB)** with zero lingering virtual environments.

### Stage 3.1.1 — Build Storage Governance, Installer Security Hardening & Storage Recovery

#### Added & Improved
- **Physical Storage Recovery & Regression Diagnosis**:
  - Investigated 7.1 GB post-desktop compilation disk space regression using Win32 `GetCompressedFileSizeW` cluster accounting:
    - `CONFIRMED_CAUSE`: Cargo debug objects (`target\debug`: 6.67 GB across 124 debug executables, `.rlib`s, and incremental compiler states) + duplicate GNU target tree (`target\x86_64-pc-windows-gnu`: 0.22 GB) + intermediate release objects (`target\release\deps` and `build`: 1.48 GB).
    - `LIKELY_CONTRIBUTOR`: Crates.io package downloads for Tauri 2 (`0.33 GB`) + desktop UI `node_modules` (`0.08 GB`) + release staging archives (`0.05 GB`).
  - Executed safe, non-destructive reclamation: removed stale duplicate `target\x86_64-pc-windows-gnu` (+211.31 MB) and compiler debug intermediate objects (`target\debug`: +5.51 GB), recovering **+5.72 GB** of physical capacity and elevating Drive E: available space to **24,197,586,944 bytes (22.533 GB)**.
- **Durable Build Storage Governance (`BuildGovernanceService`)**:
  - Implemented `crates/ulvo-services/src/build_governance.rs`: classifies build artifacts into `Essential` (release binaries, sources, manifests), `Reusable` (package caches), and `Disposable` (compiler intermediates, debug objects, incremental caches).
  - Enforced fail-closed compiler liveness protection: checks active `rustc`, `gcc`, `g++`, `link`, and `ld` processes and `target/.cargo-lock` file locks before any pruning operation.
  - Added CLI commands `ulvo storage build-audit` and `ulvo storage build-prune [--all] [--dry-run]` to `ulvo-cli`.
- **Installer & Uninstaller Security Hardening**:
  - Hardened `scripts/install-ulvo.ps1`:
    - Made uninstaller self-contained (`$InstallDir\uninstall.ps1`), eliminating dangling uninstaller paths when source or release ZIP is deleted.
    - Switched vault path template from developer paths to `$env:LOCALAPPDATA\ULVO\vault`.
    - Added `-NoPath` switch for headless / CI deployments.
    - Added active process detection and graceful agent shutdown before replacing running binaries.
    - Implemented transactional staging (`.staging_<timestamp>`) and atomic rollback.
    - Generates `.ulvo-install-manifest.json` recording version, timestamp, and SHA-256 hashes for all installed files.
  - Hardened `scripts/uninstall-ulvo.ps1`:
    - Added boundary and root validation: prevents deleting drive roots, ProgramFiles, LocalAppData, or UserProfile directories.
    - Added sentinel verification: requires `.ulvo-install-manifest.json` or `bin\ulvo.exe` before proceeding with deletion.
    - Gracefully terminates running ULVO processes.
    - Cleans shortcuts, HKCU registry entry, and user PATH safely.
    - Strictly preserves vault data and user projects by default.
- **Comprehensive Lifecycle Verification**:
  - Verified installer and uninstaller lifecycle in isolated `$env:TEMP\ulvo_installer_validation` target (`scripts/test-installer-lifecycle.ps1`).
  - Verified real desktop GUI execution: launched `ulvo-desktop.exe`, confirmed WebView2 runtime initialization (`msedgewebview2.exe`), verified window responsiveness (`Responding=True`), and executed clean termination (`scripts/test-desktop-gui.ps1`).
  - Verified 173 workspace unit and integration tests passing (`cargo test --workspace`).

### Stage 3.1 — Windows Desktop Control Center, Online Catalog, Provisioning Engine, Background Agent & Production Installer

#### Added & Improved
- **Native Windows Desktop Control Center (`ulvo-desktop.exe`)**:
  - High-density visual cockpit constructed with **Tauri 2.0**, React 19, TypeScript, and a dark-first visual language (`#08090C` void black, `#11141A` surface, `#00E5FF` cyan glow, `#00E676` emerald, `#76B900` NVIDIA/CUDA green) following `docs/GUI_AND_BRANDING_SPECIFICATION.md`.
  - Native build compatibility on Windows using `x86_64-pc-windows-gnu` and MSYS2 UCRT64 GCC 14.2.0 linking with Microsoft Edge WebView2 Evergreen Runtime.
  - Eight modular views:
    1. **Dashboard**: Live system health, Drive E: free space with 500 MB safety floor, single-root vault physical allocation, active process leases, and background agent status.
    2. **Discover**: Online catalog grid with categories, version browsing, publisher metadata, and 10-step transactional installation.
    3. **Installed**: List of vault-managed resources with durable verification badges and SHA-256 integrity confirmation.
    4. **Projects**: Real-time project connector tracking (`.ulvo/manifest.toml`), active OS process leases, and one-click safe reclamation of inactive ephemeral environments.
    5. **Storage**: Side-by-side dual-accounting display (logical footprint vs single-instance physical allocation), hardlink sharing ratio, and cache controls (prune unreferenced wheels, enforce budget).
    6. **GPU Copilot**: Real-time NVIDIA GeForce RTX 2050 telemetry (4096 MiB VRAM, driver 596.49, compute capability sm_86) affirming system-managed display driver boundaries.
    7. **Adoption**: 6-state host discovery (`Managed`, `External`, `Adoptable`, `SystemManaged`, `Blocked`, `Unknown`) with one-click adoption into Universal Vault Registry.
    8. **Activity & Settings**: Audit trail, transaction journal, staging crash recovery, and customizable preferences (`vault/config/preferences.json`).
  - **First-Run Onboarding Flow**: Honest empty state ("Welcome to ULVO", no assumed runtimes, guided host discovery and initial provisioning recommendations).

- **Unified Application Services Layer (`ulvo-services`)**:
  - Created crate `crates/ulvo-services` as the single authoritative application backend serving CLI, GUI, and background agent with zero duplicate execution or lifecycle logic.
  - **Online Runtime & Tool Catalog (`CatalogService`)**: Curated verified catalog for Python, Rust, Go, Ruby, Node.js, OpenJDK Java, GCC/MSYS2, CMake, Ninja, and uv.
  - **Universal Search Engine**: Query parser resolving ecosystem ambiguity into explicit intents (`RuntimeInstallation`, `ToolchainInstallation`, `GlobalToolInstallation`, `ProjectDependency`, `Ambiguous`).
  - **10-Step Provisioning Engine (`ProvisioningEngine`)**: Implemented transactional state machine (`Discover` -> `Resolve` -> `Plan` -> `Download` -> `Verify` -> `Stage` -> `Install` -> `Validate` -> `Register` -> `Commit`) with disk safety preflights (500 MB floor), destination containment checks, streaming SHA-256 CAS verification, atomic rollback on failure, and orphaned transaction crash recovery.
  - **Storage Intelligence (`StorageService`)**: Dual-accounting (logical vs cluster allocation via `GetCompressedFileSizeW`), hardlink sharing ratio, and package cache pruning/LRU GC.
  - **Adoption Scanner (`AdoptionService`)**: Host tool discovery across Windows PATH and hardware telemetry via `nvidia-smi`.

- **Configurable Per-User Background Agent (`ulvo-agent.exe`)**:
  - Created crate `crates/ulvo-agent` providing a lightweight background daemon communicating over loopback TCP IPC on `127.0.0.1:48590`.
  - Authenticated via per-user secret bearer token (`vault/agent.token`).
  - Handlers for `Ping`, `Status`, `ReconcileLeases` (dead process cleanup and abandoned environment recovery), `EnforceCacheBudget`, `CheckUpdates`, and graceful `Shutdown`.

- **CLI Parity (`ulvo-cli`)**:
  - `ulvo catalog [list]`: Lists verified catalog entries with installation status.
  - `ulvo catalog search <QUERY>`: Resolves universal search queries into actionable intents.
  - `ulvo catalog install <ENTRY_ID> [--version <VER>]`: Executes the 10-step provisioning transaction directly from the command line.
  - `ulvo agent status`: Queries running daemon status over loopback IPC.
  - `ulvo agent reconcile`: Triggers immediate lease reconciliation (via daemon or in-process fallback).
  - `ulvo agent stop` & `ulvo agent run`: Daemon lifecycle control.

- **Production Packaging & Distribution**:
  - `scripts/package-release.ps1`: Automates staging, portable zip generation (`dist/ulvo-v1.2.0-windows-x64-portable.zip`), and cryptographic `SHA256SUMS.txt` generation.
  - `scripts/install-ulvo.ps1`: Per-user Windows installer configuring User PATH, Start Menu, Desktop shortcuts, and Add/Remove Programs registry without administrator privileges.
  - `scripts/uninstall-ulvo.ps1`: Clean uninstaller safely preserving user vault data.

---

## [1.1.1] - 2026-10-19

### Stage 3.0.1 — Physical Storage Recovery, Exact Accounting & Lifecycle Hardening

#### Added & Improved
- **Physical Free Space Recovery on E:**:
  - Baseline Drive E: Free Space: `6,001,418,240` bytes (5.589 GiB / 6.001 GB).
  - Initial Recovery Target (15 GB free): **PASSED** (reached 22.774 GB).
  - Stretch Recovery Target (25 GB free): **PASSED** (reached `25,732,075,520` bytes = **25.732 GB decimal / 23.965 GiB**).
  - Total Physical Disk Space Recovered on `E:`: **`19,730,657,280` bytes (+19.731 GB / +18.375 GiB)** across 11 safe, audited operations:
    - OP-1: Removed ULVO `target/debug` and GNU debug intermediates (+4.512 GB).
    - OP-2: Reclaimed superseded external cache `E:\cache\uv` (+5.035 GB).
    - OP-3: Reclaimed disposable `pdfcraft` Next.js `.next` and `out` build outputs (+2.127 GB).
    - OP-4: Reclaimed `Filee` .NET `bin`/`obj` build outputs (+4.935 GB).
    - OP-5: Reclaimed `local_ai_workflow` gitignored Electron `dist` output (+554.89 MB).
    - OP-6: Reclaimed `AstraRun` PyInstaller intermediate `build` directory (+247.36 MB).
    - OP-7: Reclaimed `OmenCore` .NET `bin`/`obj` build outputs (+221.58 MB).
    - OP-8: Reclaimed `AegisNFC` Android Gradle compiler intermediate `build` cache (+190.85 MB).
    - OP-9: Reclaimed abandoned legacy `Academic/sem2/.conda` environment (18,559 files, +476.36 MB).
    - OP-10 & OP-11: Reclaimed unreferenced PySide6 and CPU torch archives from `vault/packages/python/uv-cache/archive-v0` (+1.189 GB).
  - Strictly preserved all source repositories, git trees, datasets, active SmolVLM2 model weights (`E:\cache\huggingface`), active Jupyter kernels (`E:\Workspace\Shared\envs\ds-base`), and Docker WSL disks.

- **18.21 GB Discrepancy Investigation & Storage Accounting Hardening**:
  - Thoroughly investigated and documented the 4 root causes of why Stage 3.0 environment deletion didn't yield physical disk space: NTFS hardlink inode sharing with `uv-cache/archive-v0`, unreferenced dead torch wheel archives, unlinked duplication between shared store and uv cache, and unoptimized Cargo debug build trees.
  - Hardened CAS GC accounting in `crates/ulvo-adapters/src/cas_storage.rs`: verified `link_count == 1` before attributing physical disk space recovery and added `volume_free_bytes_delta` Win32 tracking.
  - Added Hardlink Sharing Ratio (`total_logical / total_physical`) reporting to `ulvo env audit`.

- **Package Cache Governance Engine & CLI (`ulvo-cli`, `ulvo-discovery`)**:
  - `ulvo cache status [--json]`: Granular storage reporting for vault packages broken down by ecosystem (uv-cache, shared wheels, universal CAS, Cargo registry) with storage classifications.
  - `ulvo cache prune [--dry-run] [--json]`: Safe pruning of unreferenced wheel archives with `nlink == 1` while protecting actively linked packages.
  - `ulvo cache gc [--max-gb <GB>] [--dry-run] [--json]`: Budget-capped LRU garbage collector enforcing physical package cache quotas.

- **Lifecycle Hardening & Structured Execution**:
  - Added `EnvironmentLifecycleState::FailedReclamation` to `crates/ulvo-core/src/lifecycle.rs` to prevent masked deletion errors.
  - Added pre-materialization crash recovery (`scan_and_reclaim_abandoned_environments()`) to clean abandoned directories and dead process leases before launching new runs.
  - Implemented `ulvo run --json` returning structured `RunExecutionResult` (exit code, duration ms, cleanup verdict, residual environment path, active lease state, physical storage delta).

- **Python Lazy Imports & Benchmark**:
  - Implemented `__getattr__` module-level lazy import dispatcher in `Academic/sem5/DL/exp1/main.py` for PyTorch, Torchvision, and Matplotlib.
  - Benchmark result: `--version` reduced from >35,000 ms to **252 ms**, and `--help` reduced to **285 ms**, achieving sub-500ms target.

- **Workspace Verification & Testing**:
  - Verified 159 tests passing across all 4 workspace crates (`ulvo-core`, `ulvo-adapters`, `ulvo-discovery`, `ulvo-cli`).
  - Cross-project execution verified on `Academic/sem5/DL/exp1` (CUDA True, RTX 2050, 0 lingering envs) and `Active/CrossModalLab` (PIL 12.3.0, 0 lingering envs).
  - `ulvo doctor` confirmed READY across all 7 language ecosystems and host GPU.

---

## [1.1.0] - 2026-10-19

### Stage 3.0 — Zero Persistent Project Environments, Automatic Reclamation & Storage Cache Governance

#### Added
- **Vault Environment Audit & Garbage Collection Engine (`ulvo-discovery::project_lifecycle`, `ulvo-core::lifecycle`)**:
  - Implemented `ProjectOrchestrator::audit_vault_environments()` providing comprehensive inspection and categorization of all materialized environments under `vault/environments/python` into `ACTIVE`, `RECLAIMABLE`, `ABORTED`, and `UNKNOWN`.
  - Implemented `ProjectOrchestrator::reclaim_all_inactive_environments(dry_run)`: automated, safe vault-wide garbage collector.
  - Implemented `ProjectOrchestrator::reclaim_aborted_materialization()`: recovers orphaned or interrupted environment directories (e.g. from aborted builds) while strictly guarding against active processes.
  - Added new CLI subcommands:
    - `ulvo env audit [--json]`: formatted tabular/JSON audit of vault environments, active leases, logical/physical storage, and owner projects.
    - `ulvo env prune [--dry-run] [--json]`: automated safe environment reclamation, freeing inactive and aborted environments while strictly preserving active and foreign directories.
- **Package Cache & Store Governance (`ulvo-cli`)**:
  - `ulvo storage cache-audit`: inspects package manager caches and shared package stores under the vault, displaying disk usage and cache locations.
  - `ulvo storage cache-prune [--dry-run]`: invokes `uv cache prune` to safely prune unreferenced wheel archives while preserving immutable shared packages.
- **Default Ephemeral Lifecycle & Safe Auto-Reclamation**:
  - Updated `ulvo run` to enforce `EPHEMERAL` lifecycle as default: automatically reclaims materialized environments upon process completion and quiescence unless `--keep-env` is passed or manifest explicitly sets persistent policy.
  - Guaranteed try-finally lease release: child process termination and lease cleanup run unconditionally even if the user command fails or errors.
  - Added pre-materialization cleanup of aborted/corrupted directories and transactional rollback on materialization failure.
- **Adversarial & Regression Test Suite**:
  - Added 4 comprehensive test scenarios to `crates/ulvo-discovery/src/project_lifecycle.rs`:
    - `test_vault_environments_audit_empty`: verifies audit against clean vault.
    - `test_vault_environments_audit_and_prune_classification`: validates 4-way classification (`ACTIVE`, `RECLAIMABLE`, `ABORTED`, `UNKNOWN`), dry-run safety, and selective reclamation preserving active/unknown.
    - `test_dead_process_lease_cleanup_allows_reclaim`: verifies dead process lease recovery and state reconciliation.
    - `test_active_lease_blocks_direct_reclaim`: verifies fail-closed blocking of active leased environments.

#### Verified & Hardened
- Reclaimed 11 lingering environments totaling **18.21 GB (18,613.11 MB)** of redundant logical storage from `vault/environments/python`.
- Verified end-to-end execution and immediate zero-residual auto-reclamation across:
  - `Academic\sem5\DL\exp1`: PyTorch 2.6.0+cu124 CUDA execution on NVIDIA RTX 2050 (9/9 gates passed, 0 lingering environments).
  - `Academic\sem5\DL\exp2`: PyTorch CNN baseline and active model checkpoint verification (0 lingering environments).
  - `Active\CrossModalLab`: PyTorch, Transformers, Accelerate, and CUDA bounded smoke test (0 lingering environments).
  - `Active\termux-ai-workstation`: pytest tool tests passing under ULVO (0 lingering environments).
  - `Active\AstraRun`: pytest tests passing under ULVO (0 lingering environments).
- Proved 159 workspace tests passing with 0 failures across all 4 crates.

---

## [1.0.0] - 2026-10-19

### ULVO v1.0.0 — Release Readiness & System-Wide Development Resource Consolidation

#### Added
- **Universal CLI Surface Reconciliation (`ulvo-cli`)**:
  - `ulvo doctor`: Unified system-wide health and readiness doctor verifying host platform, vault durability (registry, lock, journal), single execution gateway, GPU compute & hardware telemetry (NVIDIA RTX 2050, driver 596.49, sm_86), all 7 language ecosystems (Python, Rust, C/C++, Ruby, Go, Node.js, Java), storage reserves, and CAS integrity.
  - `ulvo catalog`: Universal, beautifully formatted table or JSON catalog of all 55 development resources across Windows, MSYS2, and the ULVO Vault, sorted and grouped by ecosystem.
  - `ulvo storage [report]`: Consolidated workspace storage, physical allocation, deduplication, and CAS intelligence reporting.
  - `ulvo toolchain list`: Catalog of all discovered and managed development toolchains with ownership classification and exact binary paths.
  - `ulvo toolchain verify [<tool>]`: Automated execution verification across all managed and adopted toolchains (MSYS2 C/C++, Rust, Python, Ruby, Go, Node.js, Java) returning granular pass/fail telemetry.
  - `ulvo project init [<path>]`: Minimal, compliant `.ulvo/manifest.toml` generator adhering strictly to the Thin Connector Contract (< 100 KiB budget, no prohibited artifacts).
  - Single Execution Gateway Directory: Deployed `ulvo.exe` into dedicated `bin/ulvo.exe` for single-PATH global integration.
  - Release Packaging: Created `scripts/package-release.ps1` generating portable release bundles (`dist/ulvo-v1.0.0-windows-x86_64.zip`) with SHA-256 verification hashes.

#### Hardened & Fixed
- **CAS Active Consumer Protection & Integrity (`ulvo-adapters::cas_storage`)**:
  - Closed hardlink invalidation vulnerability: guarded against in-place overwriting of corrupted CAS objects when active consumers reference them (`CAS_CORRUPTED_OBJECT_HELD_BY_ACTIVE_CONSUMERS`), preventing silent crash or corruption of running processes.
  - Enforced NTFS read-only attributes on materialized target files in project environments.
  - Added failure journaling: unpersisted materialization transactions are journaled to `cas_journal.jsonl` if manifest persistence fails.
  - Added adversarial test `test_22_active_consumers_block_corrupted_object_repair` to `crates/ulvo-adapters/tests/cas_adversarial.rs` (22/22 adversarial tests pass).
- **Dynamic Gateway Discovery & Portability (`ulvo-adapters::gateway`, `ulvo-cli`)**:
  - Dynamic Vault Root Discovery via `with_vault_root`, `ULVO_VAULT_ROOT`, and registry introspection, eliminating hardcoded drive or workspace paths.
  - Replaced hardcoded `E:\Workspace` CLI defaults across `MigrateCommands` and `WorkspaceCommands` with portable `"."`.
  - Added full environment injection for Node.js (`APPDATA`, `LOCALAPPDATA`, `PNPM_HOME`) and Java (`JAVA_HOME`).
- **Comprehensive Workspace Verification**:
  - Verified 155 workspace unit and integration tests passing with 0 failures across all 4 crates (`ulvo-core`, `ulvo-adapters`, `ulvo-discovery`, `ulvo-cli`).
  - Verified end-to-end PyTorch CUDA execution and transactional environment reclamation on `E:\Workspace\Academic\sem5\DL\exp1`.

---

## [0.2.8] - 2026-10-19

### Stage 2.9.4a — CAS Immutability, Transactional Materialization & Safe Garbage Collection Hardening

#### Hardened & Fixed
- **CAS Immutability Guarantee (`ulvo-adapters::cas_storage`)**:
  - Eliminated hardlinks from mutable source files: implemented secure stream copy into private `.staging` directory followed by post-copy cryptographic SHA-256 verification, preventing concurrent in-flight source modification from poisoning published CAS objects.
  - Added pre-existing object content verification: before reusing an existing CAS object, its content hash is verified against expected SHA-256 digest; corrupt objects are automatically repaired.
  - Enforced NTFS read-only file attribute (`readonly = true`) on all published CAS objects, preventing accidental in-place mutation or tampering.
- **Fail-Closed Compatibility Validation (`ulvo-core::cas`)**:
  - `CasArtifactDescriptor::is_compatible()` explicitly rejects `Platform::Unknown`, `Architecture::Unknown`, and `Abi::Unknown`.
  - Enforced strict hardware/CUDA capability matching: artifacts marked with `requires_cuda` must match target project environment CUDA capability string (e.g. `cuda_sm_86`), preventing silent CPU-wheel fallback in GPU environments.
- **Transactional Target Materialization (`ulvo-adapters::cas_storage`)**:
  - Implemented 4-phase transactional target materialization: private staging directory in target folder -> rename existing target to backup -> atomic move into final destination -> set read-only attribute -> clean up backup.
  - Guaranteed zero-loss rollback: if hardlinking or staging fails, existing target files/directories are preserved untouched and restored from backup.
- **Idempotent Byte Accounting (`ulvo-core::cas`)**:
  - Decoupled physical `unique_cas_bytes` from logical `total_logical_bytes`.
  - Repeated insertion of identical artifacts increments logical references without inflating physical CAS payload bytes.
- **Safe Garbage Collection Planning & Execution (`ulvo-adapters::cas_storage`, `ulvo vault cas gc`)**:
  - Default read-only planning mode (`--plan`); destructive unlinking strictly requires explicit `--execute`.
  - Fails closed if active environment leases, running processes, incomplete staging directories, or unowned manifests exist.
  - Native package caches (`uv-cache`, `shared wheels`, `cargo registry`, `pnpm-store`, `GOMODCACHE`, `gems`, `pacman pkg`) permanently immune from CAS unlinking.
  - Audit logging of all reclamation actions to append-only `gc_journal.jsonl`.
- **Adversarial Verification Suite (`ulvo-adapters/tests/cas_adversarial.rs`)**:
  - 21-case adversarial test suite covering source mutation, concurrent writers, crash recovery, cross-volume fallback, junction escapes, and corrupted object repair.
  - Test suite passing at 154 tests across all crates (0 failures).

### Stage 2.9.4 — Universal Content-Addressable Dependency Store & Machine-Wide Language Adoption

#### Added
- **Universal Content-Addressable Dependency Store (`ulvo-core::cas`, `ulvo-adapters::cas_storage`)**:
  - Implemented immutable, cryptographic SHA-256 CAS object storage under `vault/packages/cas/objects/<prefix>/<sha256>`.
  - Created pure-Rust, zero-dependency streaming cryptographic SHA-256 implementation (FIPS 180-4) with `Sha256` and `Sha256Digest` supporting arbitrary streaming chunks.
  - Implemented `CasArtifactDescriptor`, `CasArtifactKind` (Python wheels, Node packages, Cargo crates, Go modules, Java archives, Ruby gems, pacman packages, native binaries, generic payloads), and `CasReferenceTracker`.
  - Implemented atomic staging via `vault/packages/cas/.staging` with fail-safe transactions, mutual exclusion file locking (`.cas.lock`), and checksum verification.
  - Implemented CAS garbage collection (`CasStorageManager::garbage_collect`) safely pruning unreferenced artifacts (0 active consumers) and reclaiming physical bytes.
- **Coordinated Native Package Cache Coordination (`NativeCacheCoordinator`)**:
  - Unified telemetry and discovery across 7 native ecosystem package managers:
    - Python: `uv-cache` (`vault/packages/python/uv-cache`) & `shared wheels` (`vault/packages/python/shared`)
    - Rust: `cargo registry` (`vault/runtimes/rust/cargo/registry`)
    - Node.js: `pnpm store` (`vault/packages/node/pnpm-store`)
    - Go: `GOMODCACHE` (`vault/packages/go/mod`)
    - Ruby: `gem/bundler` (`vault/packages/ruby/gems`)
    - C/C++: `pacman pkg` (`vault/toolchains/msys2/var/cache/pacman/pkg`)
  - Added CLI command `ulvo vault cas caches` reporting per-cache allocation, item counts, and relative vault paths.
- **Robust Machine-Wide Toolchain Discovery (`ulvo-discovery::toolchain_inventory`)**:
  - Discovered 41 development tools across Windows PATH, known filesystem roots, MSYS2, and WSL.
  - Formulated typed 9-stage adoption plans (`ulvo toolchain plan <tool>`) and staged adoption (`ulvo toolchain adopt <tool>`) for external toolchains without moving or breaking original installations.
  - Full first-class adoption for **Ruby 4.0.1** (`C:\Ruby40-x64\bin\ruby.exe`) and **Go 1.26.5** (`C:\Program Files\Go\bin\go.exe`).
- **Unified Execution Gateway Multi-Language Hardening (`ulvo-adapters::gateway`, `ulvo exec`)**:
  - Added `--tool <name>` option to `ulvo exec` supporting `python`, `ruby`, `go`, `gcc`, `g++`, `java`, etc.
  - Implemented Windows verbatim extended path (`\\?\`) normalization via `ulvo_core::strip_unc_prefix`, resolving GCC 14.2.0 `cannot execute 'cc1'` CreateProcess errors.
  - Injected process-local isolated environment variables:
    - Ruby: MSYS2 UCRT64 toolchain injection for building native C gems
    - Go: `GOROOT`, `GOMODCACHE`, `GOCACHE` pointing into the vault
    - Java: `JAVA_HOME` pointing to discovered Adoptium OpenJDK
- **Verified Transactional Python/GPU Package Sharing (`ulvo-discovery::project_lifecycle`)**:
  - Enforced strict distribution metadata verification: `.dist-info/WHEEL` matching target Python ABI tag (e.g. `cp311`), `.dist-info/METADATA` validating CUDA capability.
  - Supported single-file modules (`typing_extensions.py`, `six.py`) alongside directory packages.
  - Verified real GPU execution (`Academic\sem5\DL\exp1`) through ULVO with `TORCH_CUDA_AVAILABLE: True` on NVIDIA GeForce RTX 2050, both in persistent and `--reclaim` ephemeral modes.
- **Zero Global Mutation Guarantee**:
  - Preserved external `C:\msys64` and global PATH / registry completely untouched.
  - Reclaimed over 4.62 GB during environment lifecycle tests; maintained drive E: free space at **~11.40 GB**.
- **CLI Commands Added**:
  - `ulvo vault cas list`: Lists all immutable CAS artifacts.
  - `ulvo vault cas caches`: Reports status of all coordinated native ecosystem caches.
  - `ulvo vault cas verify`: Verifies cryptographic SHA-256 integrity of all CAS artifacts.
  - `ulvo vault cas gc`: Performs garbage collection of unreferenced CAS artifacts.
  - `ulvo toolchain adopt <tool>`: Adopts external toolchain into the Vault Registry.
  - `ulvo exec --tool <name> <args...>`: Direct execution gateway by tool name.
- **Test Suite**:
  - 127 native tests passing across `ulvo-core`, `ulvo-adapters`, `ulvo-discovery`, and `ulvo-cli`.

## [0.2.7] - 2026-10-19

### Stage 2.9.3 — Lightweight Project Integration Contract & Universal Connector Governance

#### Added
- **Mandatory Architectural Invariant: Thin Project Connector Contract (`ulvo-core::manifest`, `ulvo-discovery::connector`)**:
  - Enforced formal metadata budget for `<project>/.ulvo`: normal target size **under 100 KiB** (102,400 bytes), hard upper limit **10 MiB** (10,485,760 bytes).
  - Established rule that `.ulvo` is strictly a connector, not an environment; prohibited directory contents include `.venv`, `venv`, `node_modules`, `site-packages`, `bin`, `Scripts`, `lib`, `target`, `build`, `dist`, `cache`, and binary executables (`.exe`, `.dll`, `.so`, etc.).
  - Added `audit_project_connector` and `enforce_connector_contract` fail-closed validators returning typed `ConnectorComplianceVerdict`.
- **Multi-Ecosystem Manifest Contract (`ProjectManifest`)**:
  - Extended manifest schema to declare typed ecosystem configurations referencing native manifests without duplicating dependency payloads:
    - `python`: `uv`, `requirements.txt`, `pyproject.toml`, `uv.lock`
    - `node`: `pnpm`, `package.json`, `pnpm-lock.yaml`
    - `rust`: `rustup`/`cargo`, `Cargo.toml`, `Cargo.lock`
    - `cpp`: MSYS2, `CMakeLists.txt`, `Makefile`
    - `java`: JDK, `pom.xml`, `build.gradle`
    - `go`: Go toolchain, `go.mod`, `go.sum`
    - `ruby`: Ruby, `Gemfile`, `Gemfile.lock`
  - Integrated `referenced_native_manifests()` into environment specification fingerprinting: any mutation in referenced native files (`package.json`, `requirements.txt`, etc.) automatically invalidates the environment hash.
- **No Project-Local Managed Environments**:
  - All project execution environments materialize exclusively under `vault/environments/<ecosystem>/` and never inside `<project>/`.
  - Added automated post-execution verification to `ulvo run` asserting that no project-local `.venv` was created during runtime.
  - Verified across real projects (`NexusNode/server`, `termux-ai-workstation`, `AstraRun-Anti`, `AstraRun-MiMo`, `Academic\sem5\DL\exp1`) that execution succeeds with zero project-local `.venv` present.
- **CLI Subcommands**:
  - Added `ulvo project connector <path>` for full connector compliance auditing with human-readable and `--json` outputs.
  - Added `ulvo project status <path>` reporting environment ID, lifecycle state, active leases, python executable, and connector compliance.
  - Integrated Thin Connector Contract check as Check #0 in `ulvo project doctor`.
- **Expanded Test Suite (121 Native Tests)**:
  - Added 12 new unit and integration tests across `ulvo-core` and `ulvo-discovery`, achieving 100% pass rate on `x86_64-pc-windows-gnu`.

## [0.2.6] - 2026-10-19

### Stage 2.9.2a — Emergency Storage Recovery, Toolchain Consolidation & Gateway Hardening

#### Added
- **Emergency Storage Investigation & Space Reclamation**: Investigated root cause of drive E: free space collapse to 3.68 GB. Safely reclaimed abandoned test fixtures (`E:\ULVO_TEST`, 259.36 MB), obsolete build targets (`target-local`, 580.14 MB), and Cargo incremental cache (`debug/incremental`, 2,038.52 MB).
- **Automated Retirement of Verified Legacy `.venv` Directories**:
  - `Active\termux-ai-workstation\.venv`: Reclaimed **3,457.72 MB (3.46 GB)** across 31,560 files; confirmed 301/302 tests collect and pass cleanly before and after retirement.
  - `Agents\AstraRun-Anti\.venv`: Reclaimed **886.37 MB** across 11,705 files; verified execution via `ulvo run`.
  - `Agents\AstraRun-MiMo\.venv`: Reclaimed **694.71 MB** across 6,816 files; verified execution via `ulvo run`.
  - Net physical storage reclaimed from legacy environments: **5,038.80 MB (~5.04 GB)**, raising drive E: free space to **6.75 GB - 7.13 GB** (a +3.45 GB net gain while preserving all test/release builds).
- **Durability & Read-Only Registry Access**:
  - Implemented `load_vault_registry_readonly` to ensure non-mutating commands (`vault stats`, `vault list`, `vault inspect`, `exec`) never perform disk mutations.
  - Implemented atomic transaction engine `with_vault_registry_transaction` with automatic rollback, fail-closed journaling, and corrupt registry backup (`vault_registry.corrupt.<timestamp>.json`).
- **Unified Execution Gateway Hardening (`ulvo exec`)**:
  - Hardened `GatewayResolver` with strict canonical matching against approved Windows system directories (`System32`, `Wbem`, `WindowsPowerShell`), eliminating inheritance of dirty user PATH entries while guaranteeing core OS DLL resolution.
  - Deterministic collision resolution prioritized: `Managed` > `External` > `Unknown`, verified verification status, and numeric semver version ranking.
  - End-to-end verified execution of external toolchains via `ulvo exec gcc --version` and `ulvo exec rustc --version`.
- **Windows File System Robustness**:
  - Added UTF-8 BOM, UTF-16 LE, and UTF-16 BE decoding in `bounded_read` to prevent file encoding crashes on Windows filesystems.
  - Upgraded canonical path comparison in `ulvo-core::lifecycle` and `ulvo-discovery::vault` to support relative suffix matching.
- **Deep MSYS2 & Universal Toolchain Audit**:
  - Fully verified MSYS2 UCRT64 toolchain: GCC 14.2.0, G++ 14.2.0, GNU Make 4.4.1, Pacman 6.1.0 across 192 packages, with package database consistency checked via `pacman -D --check`.
  - Cataloged 29 discovered toolchains across Windows, MSYS2, and Scoop/shims without modifying any global PATH or user registry entries.
- **AI/ML Workload Preservation**:
  - Successfully preserved `Academic\sem5\DL\exp5` (CUDA 12.1 mismatch and direct git dependency `clip @ git+https://github.com/ultralytics/CLIP.git`), safely recording explicit blocker `BLOCKER: DIRECT_GIT_DEPENDENCY_AND_DIFFERING_CUDA_ABI`.

## [0.2.5] - 2026-10-19

### Stage 2.9.2 — Universal Vault Adoption, Resource Lifecycle & Storage Recovery

#### Added
- **First-Class Managed MSYS2 Adapter (`ulvo-adapters::vault_msys2`)**: Modeled `Msys2Subsystem` (`Ucrt64`, `Mingw64`, `Clang64`, `Msys`), implementing staged managed toolchain provisioning into `vault/toolchains/msys2` (`ucrt64`, `usr`, `etc`, `var/lib/pacman`) anchored by `.ulvo_managed.json` sentinel and verified provenance.
- **Native C and C++ Toolchain Verification**: Built automated compilation, linking, and execution tests (`verify_native_c_compilation` and `verify_native_cpp_compilation`) proving complete GCC/G++ 14.2.0 functionality under process-local isolation with zero runtime dependency on external `C:\msys64`.
- **Unified Execution Gateway (`ulvo exec`)**: Added `ulvo exec [--toolchain <name>] -- <cmd...>` CLI command backed by `GatewayResolver`. Constructs backend-isolated process environments with deterministic collision resolution (`Managed` > `External` > `Unknown`, verified status, and version ranking) while rejecting dirty user PATH entries.
- **Durable Vault Registry Schema v2**: Implemented `PersistentRegistryEnvelope` with cryptographic SHA-256 payload checksums, cross-process file locking (`vault_registry.lock`), atomic persistence, append-only journal (`journal.jsonl`), and fail-closed corruption detection.
- **Mandatory Workspace-Wide .venv Elimination**: Safely retired multi-gigabyte project-local virtual environments for `Academic\sem5\DL\exp1` (4,620.67 MB) and `Academic\sem5\DL\exp2` (4,621.00 MB) after passing 100% of all 10 verification gates (Gates A–J).
- **Physical Storage Recovery Milestone**: Reclaimed **9,690,602,659 bytes (~9.69 GB / 9,241.67 MB)** across `exp1` and `exp2`, expanding drive E: free space from 6.88 GB to **11.44 GB**, bringing total workspace physical reclamation across 5 retired environments to **9,791,592,159 bytes (~9.79 GB / 9,337.99 MB)**.
- **Zero-Regression Post-Retirement Verification**: Verified that both `exp1` and `exp2` deep learning GPU workloads (PyTorch 2.6.0+cu124 on CUDA 12.4 with physical NVIDIA RTX 2050) continue to execute identically through ULVO without regenerating project-local `.venv` directories.
- **Strict Managed Provenance Enforcement**: Required non-null cryptographic or manifest `Provenance` metadata for all `Ownership::Managed` resources, ensuring path containment alone never confers managed ownership.
- **Runtime-Verified GPU Telemetry**: Integrated direct `nvidia-smi` banner parsing for runtime CUDA support with documented driver boundary mapping and fail-closed `Unknown` fallbacks.

## [0.2.4] - 2026-10-19

### Stage 2.9.1 — GPU-Aware Python Environment Orchestration & Universal Toolchain Adoption

#### Added
- **GPU Environment Compatibility Model (`ulvo-core::gpu`)**: Implemented typed metadata structures for GPU devices, compute capability (`sm_86`), NVIDIA driver version (`596.49`), deep-learning frameworks (PyTorch), framework build variants (`+cu124` vs `+cpu`), CUDA runtime dependencies (`torch_cuda.dll`), native wheel platform tags (`cp311-cp311-win_amd64`), and storage capacity verdicts (`CapacityVerdict::SufficientSpace` vs `BlockedCapacity`).
- **Strict Kernel Driver Boundary Enforcement**: Prohibited copying or relocating system NVIDIA kernel drivers into the vault (`SystemGpuDriver::kernel_driver_relocation_prohibited`), explicitly distinguishing system display drivers from ULVO user-space runtimes.
- **Typed 9-Stage Toolchain Adoption State Machine (`ulvo-core::adoption`)**: Established formal adoption lifecycle (`EXTERNAL_DISCOVERED` -> `COMPATIBILITY_ASSESSED` -> `ADOPTION_PLANNED` -> `STAGING` -> `VERIFIED` -> `MANAGED`, with branches to `EXTERNAL_RETAINED` and `BLOCKED`), enforcing verification command requirements and rejecting direct toolchain relocation without proven relocatability.
- **Universal Multi-Ecosystem Toolchain Inventory (`ulvo-discovery::toolchain_inventory`)**: Implemented read-only metadata scanning across Windows PATH, known install roots, MSYS2 (`C:\msys64`), configured VS Code profiles, and WSL metadata without booting WSL, discovering 29 toolchains across Python, uv, Rust, Cargo, Node.js, npm, pnpm, Go, Java, javac, Ruby, gem, bundler, .NET SDK, GCC, G++, Clang, CMake, and Ninja.
- **Unified Execution Gateway Foundation (`ulvo-adapters::gateway`)**: Built `GatewayResolver` for resolving tools via Vault Registry identities and constructing process-local PATHs and ecosystem environments without altering system PATH, user registry, or deploying global shims.
- **Shared GPU Dependency Storage Engine (`ulvo-adapters::fs_identity`)**: Implemented `hardlink_or_copy_tree` to establish single-instance immutable package stores (`vault/packages/python/shared`), enabling zero-copy hardlink materialization into ephemeral environments on NTFS volumes and avoiding multi-gigabyte wheel re-downloads.
- **Real GPU Pilot Verification (`Academic\sem5\DL\exp1`)**: Executed real GPU deep learning workload (`SimpleNeuralNetwork` MNIST MLP) under ULVO on physical NVIDIA GeForce RTX 2050 (4 GB VRAM) running PyTorch 2.6.0+cu124 on CUDA 12.4; verified all 9 GPU gates including tensor allocation, CUDA forward pass, loss calculation, backprop, numerical parity with CPU (max diff `4.47e-08`), and zero imports from legacy `.venv`.
- **GPU Pilot Retirement Readiness Report**: Evaluated `exp1` across reproducibility, execution equivalence, ownership, and inactive consumers, classifying it as `READY_FOR_RETIREMENT` while preserving the multi-gigabyte environment until separate operator approval.
- **GUI Architecture and Visual Branding Specification (`docs/GUI_AND_BRANDING_SPECIFICATION.md`)**: Authored comprehensive design specification detailing the hexagonal vault symbol, dark-first visual system, cockpit dashboard modules, Rust core IPC bindings, and Tauri 2.0 Windows build feasibility.
- **Expanded Test Suite (104 Native Tests)**: Added 15 new unit and integration tests across `ulvo-core`, `ulvo-adapters`, and `ulvo-discovery`, achieving 100% pass rate on `x86_64-pc-windows-gnu`.

---

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
