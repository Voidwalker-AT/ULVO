# ULVO Stage 3.4 — External Toolchain Retirement Matrix & Consolidation Governance

**Document Version:** 2.0.0  
**Verification Date:** 2026-10-20  
**Status:** [VERIFIED - ACCURATE PHYSICAL AUDIT & SAFE CONSOLIDATION BLUEPRINT]  
**Safety Invariant:** **STRICTLY ZERO DELETIONS OR UNINSTALLATIONS OF EXTERNAL TOOLCHAINS ON DRIVE C: DURING THIS MISSION.**  
All host development tools, interpreters, compilers, and SDKs remain 100% untouched and functional.

---

## 1. Executive Summary & Reconciliation Findings

In Stage 3.3, preliminary inventories estimated external toolchain footprints on drive `C:` with some historical inaccuracies (such as referencing an obsolete `C:\Ruby34-x64` and listing 3.2 GB for host `C:\Users\hp\.cargo`). 

In Stage 3.4, rigorous physical filesystem probing and reconciliation established ground truth:
1. **Ruby Reconciliation**:
   - `C:\Ruby34-x64` **does not exist** on the filesystem.
   - The active host Ruby installation is **`C:\Ruby40-x64`** (`1.22 GB` / `1,224.45 MB`), running `ruby 4.0.1 (2026-01-13 revision e04267a14b) +PRISM [x64-mingw-ucrt]`.
   - Action: Obsolete Ruby 3.4 entries removed; Ruby 4.0.1 retained with zero modifications.
2. **Cargo / Rustup Reconciliation**:
   - `C:\Users\hp\.cargo\bin` contains **0 executable files** (0 bytes).
   - `C:\Users\hp\.rustup` contains only `settings.toml` (28 bytes).
   - The ~3.2 GB Rust toolchain was physically migrated to `E:\Workspace\Active\ULVO\vault\runtimes\rust` during Stage 3.2.
   - Action: Host Cargo entry corrected from 3.2 GB to 0 MB (marked as `ALREADY_MIGRATED`).
3. **Standalone uv Independence**:
   - In Stage 3.3, ULVO relied on `C:\Users\hp\.local\bin\uv.exe`.
   - In Stage 3.4, ULVO vendored pinned `uv.exe` (`v0.13.1`, SHA-256: `4075A09490327D2BEDFACC807FA48252FB93647C5BD7EF28F00E579160EA3FB5`) directly into `E:\Workspace\Active\ULVO\vault\bin\uv.exe`.
   - Adapter lookup priorities (`vault_uv.rs` and `vault_python.rs`) now explicitly favor `vault/bin/uv.exe`, achieving runtime independence from host uv while leaving host uv undisturbed.
4. **Standalone Vault Python Interpreter**:
   - Complete CPython 3.11.16 standalone distribution staged into `vault\runtimes\python\cpython-3.11.16-windows-x86_64-none` (with alias `cpython-3.11-windows-x86_64-none`).
   - Host `C:\Program Files\Python311` and `Python313` remain completely intact.

---

## 2. Reconciled Machine-Wide Toolchain Inventory

| Toolchain & Location on `C:` | Measured Footprint | ULVO Management Status | Active External Consumers | Retirement Readiness | Prerequisites for Eventual Retirement | Rollback Procedure | Human Sign-Off Gate |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Rustup / Cargo**<br>`C:\Users\hp\.rustup`<br>`C:\Users\hp\.cargo` | **~28 bytes**<br>(0 MB on `C:`) | **FULLY MIGRATED**<br>(Active toolchain resides in `vault\runtimes\rust\cargo` on `E:`) | Rust Analyzer, global CLI cargo tools | **N/A** (Already Migrated) | None remaining. Host directory contains no compiler binaries. | Re-run `rustup-init.exe` if standalone host Rust is ever requested. | **MIGRATED & VERIFIED** |
| **MSYS2 / MinGW64**<br>`C:\msys64` | **~4.20 GB** | **FULLY VAULTED**<br>(Vault hosts GCC 14.2.0 in `vault\toolchains\msys2`) | Standalone CMake builds, native Windows C/C++ projects, MSYS2 bash | **Phase 2 Candidate** (Medium Risk) | 1. Verification that all external builds use ULVO GCC gateway.<br>2. Explicit user approval. | Restore from backup or re-extract official MSYS2 archive to `C:\msys64`. | **MANDATORY EXPLICIT APPROVAL** |
| **Ruby 4.0.1**<br>`C:\Ruby40-x64` | **~1.22 GB** | **MANAGED VIA GATEWAY**<br>(Detected by ULVO Discovery Gateway) | Jekyll, CocoaPods, Ruby gems, web services | **Phase 3 Candidate** (Medium Risk) | 1. Implement vault-managed Ruby runtime pilot.<br>2. Explicit user authorization. | Re-run Ruby+Devkit 4.0.1 installer. | **MANDATORY EXPLICIT APPROVAL** |
| **Go Toolchain**<br>`C:\Program Files\Go` | **~650 MB** | **MANAGED VIA GATEWAY**<br>(Go 1.26.5 resolved by Gateway) | Go LSP (`gopls`), standalone Go microservices | **Phase 3 Candidate** (Low Risk) | Ingest Go toolchain physically into `vault\toolchains\go\1.26.5`. | Reinstall official Go MSI from golang.org. | **MANDATORY EXPLICIT APPROVAL** |
| **Java JDK (Adoptium)**<br>`C:\Program Files\Eclipse Adoptium\jdk-21.0.4.7-hotspot` | **~340 MB** | **MANAGED VIA GATEWAY**<br>(Java 21.0.4 resolved by Gateway) | Android Studio, Gradle, Maven, Language Server for Java | **Phase 3 Candidate** (Low Risk) | Ingest OpenJDK into `vault\toolchains\java\21.0.4`. | Reinstall Temurin JDK 21 MSI. | **MANDATORY EXPLICIT APPROVAL** |
| **Node.js LTS**<br>`C:\Program Files\nodejs` | **~80 MB** | **MANAGED VIA GATEWAY**<br>(Node 24.15.0 resolved by Gateway) | Tauri CLI, Vite, ESLint, Antigravity webviews, npm/pnpm | **BLOCKED** (High Risk) | Ingest Node LTS into `vault\toolchains\node\24.15.0` with npm/pnpm shims. | Reinstall Node.js LTS MSI. | **RETAINED INDEFINITELY** |
| **Python 3.11**<br>`C:\Program Files\Python311` | **~120 MB** | **INDEPENDENT**<br>(ULVO uses vault CPython 3.11.16) | System tools, legacy background tasks | **BLOCKED** (High Risk) | None — Retained indefinitely for host compatibility. | Repair Python 3.11 via Windows Settings. | **RETAINED INDEFINITELY** |
| **Python 3.13**<br>`C:\Users\hp\AppData\Local\Programs\Python\Python313` | **~140 MB** | **EXTERNAL RUNTIME** | Standalone AI experiments, host Jupyter | **BLOCKED** (High Risk) | None — Retained indefinitely for host compatibility. | Repair Python 3.13 via Windows Settings. | **RETAINED INDEFINITELY** |
| **uv Standalone**<br>`C:\Users\hp\.local\bin\uv.exe` | **~45 MB** | **INDEPENDENT**<br>(ULVO uses `vault\bin\uv.exe`) | Standalone user shell scripts | **BLOCKED** (Retained) | None — Retained for non-ULVO shell scripts. | Re-download standalone uv binary. | **RETAINED INDEFINITELY** |

---

## 3. Potential Storage Recovery Opportunity (Future Phases)

If and when the user chooses to retire verified duplicate toolchains in future operational phases (e.g. Stage 4), the potential physical space recovery on drive `C:` is:

| Toolchain Category | Path on `C:` | Potential Recovery | Safety Level |
| :--- | :--- | :--- | :--- |
| MSYS2 MinGW64 Toolchain | `C:\msys64` | ~4.20 GB | Medium (Vault already contains GCC 14.2.0) |
| Ruby 4.0.1 Toolchain | `C:\Ruby40-x64` | ~1.22 GB | Medium (Requires vault adoption pilot) |
| Go 1.26.5 Toolchain | `C:\Program Files\Go` | ~0.65 GB | Low (Requires vault adoption pilot) |
| Java Adoptium JDK 21 | `C:\Program Files\Eclipse Adoptium\...` | ~0.34 GB | Low (Requires vault adoption pilot) |
| **Total Potential Recovery on `C:`** | | **~6.41 GB** | **Subject to Explicit Human Approval** |

---

## 4. Mandatory Quarantine Protocol (Zero In-Place Deletion)

When future retirement is authorized by the user, **no directories may be deleted directly**. The 4-step quarantine protocol must be applied:

1. **Phase 1: Registry Snapshot**:
   Take an immutable snapshot of System and User `PATH` and relevant environment variables (`JAVA_HOME`, `GOROOT`, etc.).
2. **Phase 2: Quarantine Renaming**:
   Rename target directory to `<name>.quarantine.<timestamp>` (e.g. `C:\msys64.quarantine.20261020`).
3. **Phase 3: 30-Day Shadow Verification**:
   Execute standard development tasks across all active projects. If any toolchain fails to resolve, immediately rename back to the original path.
4. **Phase 4: Formal Uninstallation**:
   Execute the official Windows uninstaller via `winget uninstall` or Windows Settings.

---

## 5. Current Gate Status

- **Host Deletions Executed in Stage 3.4:** **0**
- **Host Modifications Executed in Stage 3.4:** **0**
- **Safety Verdict:** **[VERIFIED - 100% INTACT]**
- **Human Acceptance Gate:** **ACTIVE & ENFORCED**
