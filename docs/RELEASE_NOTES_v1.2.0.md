# ULVO Release Notes — Version 1.2.0 (Release Candidate)

**Target Version:** `v1.2.0`  
**Target Architecture:** Windows 11 / Windows 10 (x86_64)  
**Release Git Commit:** `fe6186eff4317c90da0603c036d25e7570afa75e`  
**Repository:** `https://github.com/Voidwalker-AT/ULVO`  
**Release Readiness:** **CANDIDATE FROZEN — AWAITING EXPLICIT HUMAN ACCEPTANCE SIGN-OFF**

---

## 1. Executive Summary

ULVO (**Universal Language & Virtual Environment Orchestrator**) v1.2.0 transforms Windows multi-language software development from fragmented, disk-duplicating environments into an integrated, deterministic, high-performance control plane.

By enforcing the five foundational invariants:
```
       STORE ONCE.
       ISOLATE PER PROJECT.
       MATERIALIZE ON DEMAND.
       EXECUTE THROUGH ULVO.
       RECLAIM SAFELY.
```
ULVO eliminates the disk explosion of redundant `.venv` directories, coordinates 7 native package caching layers, provides process-isolated language toolchains without Windows PATH pollution, and delivers an accessible native Windows Desktop Cockpit.

---

## 2. Key Capabilities & Architectural Advancements

### 2.1 Physical Runtime Independence & Vault Vendoring
- **Vendored Standalone uv**: Integrated pinned `uv.exe` (`v0.13.1`, SHA-256: `4075A09490327D2BEDFACC807FA48252FB93647C5BD7EF28F00E579160EA3FB5`) directly inside `vault/bin/uv.exe`.
- **Standalone CPython 3.11.16 Distribution**: Staged complete portable CPython distribution into `vault/runtimes/python/cpython-3.11.16-windows-x86_64-none` with C headers (`include/Python.h`), link libraries (`libs/python311.lib`), and standard library.
- **Prioritized Lookup Chain**: Adapters (`vault_uv.rs` and `vault_python.rs`) resolve runtimes from the vault root first, completely eliminating reliance on host `C:` package managers while leaving external tools intact.

### 2.2 Package Store Safety & Inode Isolation
- **NTFS Atomic Inode Severing**: Proved that atomic replacement (`MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`) assigns a newly staged file's inode to the target cache entry while decrementing the original inode's reference count, guaranteeing zero corruption to the shared package store.
- **Immutable Shared Store Protection**: Applied `ReadOnly` NTFS attributes across all 17,714 files (4.512 GB) in `vault/packages/python/shared`, preventing accidental in-place mutations from editable installs.

### 2.3 10.28x Fast Warm Execution Path
- **Warm Launch Latency**: Reduced warm `ulvo run` launch latency from **1,614 ms to 157 ms** (**10.28x speedup**) and `ulvo project status` to **84 ms** (**17.56x speedup**).
- **Optimization Strategy**: Bypassed recursive abandoned-environment traversals during active warm runs and cached direct runtime paths, eliminating redundant subprocess discovery.

### 2.4 Unified Polyglot Execution Gateway
- **Process-Isolated Execution**: Single entrypoint `ulvo exec [--tool <name>]` isolating `PATH` to the specific runtime without machine-wide registry or profile pollution.
- **Supported Ecosystems**:
  - **Python**: Vault CPython 3.11.16 & Host Python 3.13.
  - **Rust**: Vault-managed Rust 1.96.0 + Cargo.
  - **C/C++**: Vault-managed MSYS2 GCC 14.2.0 + G++ 14.2.0 (UCRT64).
  - **Go**: Go 1.26.5 via Gateway.
  - **Ruby**: Ruby 4.0.1 (+PRISM [x64-mingw-ucrt]) via Gateway.
  - **Node.js**: Node.js 24.15.0 via Gateway.
  - **Java**: Eclipse Adoptium OpenJDK 21.0.4 via Gateway.

### 2.5 Real Hardware GPU Acceleration
- **Physical NVIDIA GeForce RTX 2050 Laptop GPU**: First-class CUDA compute capability (`sm_86`) verification.
- **End-to-End Validation**: PyTorch 2.6.0+cu124 tensor matrix multiplication (4096x4096 in ~190 ms) executing with zero project `.venv` bloat.

### 2.6 Native Windows Desktop Control Center
- **Tauri 2.0 / React 19 / TypeScript**: Accessible dark-first desktop cockpit with 8 views: Dashboard, Discover, Installed, Projects, Storage, GPU Copilot, Adoption, and Settings.
- **Loopback IPC Background Agent**: Authenticated background daemon (`ulvo-agent.exe`) on `127.0.0.1:48590` providing real-time lease monitoring and non-destructive reconciliation.

### 2.7 Universal Project Onboarding Engine
- **Automated Ecosystem Scanning**: `ulvo project onboard [PATH]` recursively detecting Python, Node.js, Rust, Go, Ruby, Java, C/C++, and monorepos, provisioning deterministic thin connectors (`.ulvo/manifest.toml` < 10 KB).

---

## 3. Storage Accounting Reconciliation

- **Physical Drive E (Workspaces & Vault)**: **30.15 GB** free (**28.08 GiB**).
- **Physical Drive C (System OS)**: **20.66 GB** free (**19.24 GiB**).
- **Decimal vs Binary Reconciliation**: Mathematically proven that $24.17\text{ GB} = 22.509\text{ GiB}$.
- **Zero Host Mutation**: Zero deletions or renames executed against external toolchains on drive `C:`.

---

## 4. Verification Checkpoint Summary

| Verification Suite | Result | Evidence File |
| :--- | :--- | :--- |
| **Cargo Workspace Unit Tests** | **128 / 128 Passed** | `target/debug/deps/` |
| **Real Project Polyglot Acceptance** | **6 / 6 Projects Passed** | `docs/evidence/stage3-4/real-project-acceptance.json` |
| **Language Gateway Acceptance** | **7 / 7 Languages Passed** | `docs/evidence/stage3-4/language-gateway-acceptance.json` |
| **Complete Desktop GUI Acceptance** | **7 / 7 Checks Passed** | `docs/evidence/stage3-4/gui-acceptance-verification.json` |
| **Package Store Safety Audit** | **8 / 8 Checks Passed** | `docs/evidence/stage3-4/package-store-safety-audit.json` |
| **Universal Project Onboarding** | **12 / 12 Tests Passed** | `docs/evidence/stage3-4/universal-onboarding-verification.json` |
| **Installer / Uninstaller Security** | **7 / 7 Suites Passed** | `docs/evidence/stage3-4/installer-acceptance-verification.json` |
| **IDE Integration Validation** | **4 / 4 Checks Passed** | `docs/evidence/stage3-4/ide-integration-validation.json` |

---

## 5. Artifact Checksums

```
bin/ulvo.exe (7,440,442 bytes)
SHA-256: aaec8149121809b329fce0249e1a9bba4e479496c6ba4f3f22d9f802769c7d2d

target/release/ulvo-desktop.exe (23,984,395 bytes)
SHA-256: 3b9c6fca675fa88cfdfa1a89c381cbb4c1a2dbd5a0cbb0b2cae8668351745749

target/release/ulvo-agent.exe (6,237,080 bytes)
SHA-256: b5c27a3c7a1b7b8da8ec59d162e137c58b1e3ba2b3bfe9992c44ac9a62152b27

dist/ulvo-v1.2.0-windows-x64-portable.zip (12,285,548 bytes)
SHA-256: 63062d0a42c24fde12fddb62728c9b66c67ee3573f878bf011637c398d35c618
```

---

## 6. Publication Gate Status

**GATE STATUS:** **HELD FOR HUMAN ACCEPTANCE SIGN-OFF.**  
No git release tags have been created. No GitHub release assets have been published.
