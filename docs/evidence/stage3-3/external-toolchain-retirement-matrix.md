# ULVO Stage 3.3 — Machine-Wide External Toolchain Retirement Matrix & Safe Consolidation Plan

**Document Version:** 1.0.0  
**Status:** [VERIFIED - ARCHITECTURAL BLUEPRINT & SIGN-OFF GATE]  
**Safety Invariant:** **DO NOT UNINSTALL OR DELETE ANY EXTERNAL TOOL DURING THIS MISSION.**  
All migrations and uninstalls require explicit user approval, prerequisite verification, and verified rollback checkpoints.

---

## 1. Executive Summary

During Stage 3.2 and Stage 3.3 investigations, **11.75 GB** of development toolchains, runtimes, and compiler toolchains were cataloged on system drive `C:`. While ULVO now hosts fully-managed toolchains for Rust and MSYS2 GCC on `E:\Workspace\Active\ULVO\vault`, several runtimes on `C:` remain active references or external dependencies for machine-wide developer tasks, system services, and background agents.

This document establishes the official machine-wide retirement matrix, tracking:
- What toolchains exist on `C:`.
- How they are currently referenced by ULVO and external consumers.
- The technical criteria required before retirement can even be considered.
- The step-by-step rollback procedures if retirement causes breaks.
- The mandatory human acceptance sign-off gates.

---

## 2. Machine-Wide Toolchain Inventory & Retirement Matrix

| Toolchain & Location on `C:` | Physical Footprint | ULVO Management Status | Active External Consumers | Retirement Readiness | Prerequisites for Retirement | Rollback Procedure | Human Sign-Off Gate |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Rustup / Cargo**<br>`C:\Users\hp\.rustup`<br>`C:\Users\hp\.cargo` | ~4.80 GB | **FULLY MANAGED**<br>(Vault has Rust 1.96.0 + cargo) | Rust Analyzer (VS Code), global cargo tools, PowerShell sessions | **Phase 2 Candidate** (Medium Risk) | 1. Move global cargo tools into vault.<br>2. Point VS Code Rust Analyzer to vault toolchain.<br>3. Verify all external rust crates compile with vault cargo. | Re-run rustup installer or restore backup archive to `%USERPROFILE%\.rustup`. | **MANDATORY EXPLICIT APPROVAL** |
| **MSYS2 / MinGW64**<br>`C:\msys64` | ~3.20 GB | **FULLY MANAGED**<br>(Vault has MinGW64 GCC 14.2.0) | CMake, Make, native C/C++ builds, MSYS2 bash tools | **Phase 2 Candidate** (Low Risk) | 1. Verify CMake finds vault GCC/G++.<br>2. Ensure no system scripts require `C:\msys64\usr\bin\bash.exe`. | Re-extract MSYS2 installer or restore `C:\msys64` snapshot. | **MANDATORY EXPLICIT APPROVAL** |
| **Python 3.11**<br>`C:\Program Files\Python311` | ~120 MB | **MANAGED VIA UV** | System utilities, legacy scripts, Antigravity background tools | **BLOCKED** (High Risk) | None — Required for host-level tools and legacy scripts not yet containerized. | Repair Python 3.11 from Windows Settings > Installed Apps. | **RETAINED INDEFINITELY** |
| **Python 3.13**<br>`C:\Users\hp\AppData\Local\Programs\Python\Python313` | ~140 MB | **MANAGED VIA UV** | `AstraRun`, Jupyter, standalone AI tools | **BLOCKED** (High Risk) | None — Active primary host interpreter for recent AI experiments. | Repair Python 3.13 installer. | **RETAINED INDEFINITELY** |
| **uv Standalone**<br>`C:\Users\hp\.local\bin\uv.exe`<br>`%LOCALAPPDATA%\uv` | ~45 MB binary<br>~2.1 GB cache | **MANAGED DEPENDENCY**<br>(Used by ULVO Python engine) | ULVO Python package resolution engine, standalone uv scripts | **BLOCKED** (Essential Core Engine) | Never retire unless ULVO vendors uv binary directly inside `vault/bin`. | Download fresh `uv.exe` from astral.sh. | **CRITICAL CORE COMPONENT** |
| **Go Toolchain**<br>`C:\Program Files\Go` | ~650 MB | **EXTERNAL REFERENCE**<br>(Go 1.26.5 resolved by Gateway) | Go LSP (`gopls`), Go projects, Docker/Kubernetes tooling | **Phase 3 Candidate** | Ingest Go toolchain physically into `vault/toolchains/go/1.26.5`. | Reinstall official Go MSI. | **MANDATORY EXPLICIT APPROVAL** |
| **Ruby Toolchain**<br>`C:\Ruby34-x64` | ~480 MB | **EXTERNAL REFERENCE**<br>(Ruby 4.0.1 resolved by Gateway) | Jekyll, CocoaPods, Ruby gems | **Phase 3 Candidate** | Ingest Ruby toolchain physically into `vault/toolchains/ruby/4.0.1`. | Reinstall Ruby+Devkit installer. | **MANDATORY EXPLICIT APPROVAL** |
| **Node.js**<br>`C:\Program Files\nodejs` | ~120 MB | **EXTERNAL REFERENCE**<br>(Node 24.15.0 resolved by Gateway) | Tauri CLI, Vite, ESLint, Antigravity UI webviews, VS Code extensions | **BLOCKED** (High Risk) | Ingest Node LTS into `vault/toolchains/node/24.15.0` with npm/pnpm shims. | Reinstall Node.js LTS MSI. | **RETAINED** |
| **Java JDK (Adoptium)**<br>`C:\Program Files\Eclipse Adoptium\jdk-21.0.4.7-hotspot` | ~320 MB | **EXTERNAL REFERENCE**<br>(Java 21.0.4 resolved by Gateway) | Android Studio, Gradle, Maven, Language Server for Java | **Phase 3 Candidate** | Ingest OpenJDK into `vault/toolchains/java/21.0.4`. | Reinstall Temurin JDK MSI. | **MANDATORY EXPLICIT APPROVAL** |

---

## 3. Toolchain Lifecycle & Transition Protocol

For any toolchain scheduled for future retirement (Phase 2 or Phase 3), the following 5-stage transition protocol must be strictly observed:

```
[Phase 0: Catalog & Fingerprint]
               │
               ▼
[Phase 1: Ingest into ULVO Vault & Verify Hermetic Execution]
               │
               ▼
[Phase 2: Shadow Period (30 Days)]
  - System PATH references removed
  - External tool renamed to *.disabled (never deleted)
  - All builds monitored for missing dependency errors
               │
               ▼
[Phase 3: Rollback Verification & Smoke Testing]
  - If any failure occurs, immediately rename back to original
               │
               ▼
[Phase 4: Explicit User Authorization & Final Archival]
  - User signs off on permanent removal
  - Tool uninstalled cleanly via Windows Settings / uninstaller
```

---

## 4. Disaster Recovery & Rollback Procedures

### 4.1 Registry PATH Restoration
Before any external tool directory is disabled, an immutable registry snapshot is recorded:
```powershell
# Create pre-migration backup of Environment PATH
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$sysPath  = [Environment]::GetEnvironmentVariable("Path", "Machine")
[IO.File]::WriteAllText("E:\Workspace\Active\ULVO\docs\evidence\backup-user-path.txt", $userPath)
[IO.File]::WriteAllText("E:\Workspace\Active\ULVO\docs\evidence\backup-sys-path.txt", $sysPath)
```
**Restoration Command:**
```powershell
[Environment]::SetEnvironmentVariable("Path", (Get-Content "E:\Workspace\Active\ULVO\docs\evidence\backup-user-path.txt"), "User")
```

### 4.2 Safe Disabling Pattern (Zero Deletion)
Never call `Remove-Item -Recurse` on toolchains on `C:`. Instead, apply the quarantine rename pattern:
```powershell
Rename-Item -Path "C:\msys64" -NewName "C:\msys64.quarantine"
```
If an unpredicted dependency error appears:
```powershell
Rename-Item -Path "C:\msys64.quarantine" -NewName "C:\msys64"
```

---

## 5. Formal Human Acceptance Gate

Under `/gravitas` and Stage 3.3 invariants:
- **Current Mission Actions:** Zero deletions, zero modifications to `C:\Program Files`, `C:\msys64`, `C:\Users\hp\.rustup`, or `C:\Users\hp\.cargo`.
- **Status:** **PASS / FROZEN**. All external toolchains are fully mapped, documented, and protected against accidental removal.
