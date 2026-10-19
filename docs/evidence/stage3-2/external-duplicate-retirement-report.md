# ULVO Stage 3.2 — Machine-Wide External Toolchain Retirement Report

**Status:** AWAITING EXPLICIT HUMAN AUTHORIZATION  
**Governing Rule:** Under ULVO Safety Invariants (Section 11), external toolchains and development installations on `C:` MUST NEVER be deleted, moved, or uninstalled automatically without explicit, interactive human review and authorization.

---

## 1. Executive Summary

During Stage 3.2, ULVO completed a machine-wide inventory of development installations on `C:`. Six external toolchains were audited, evaluated for compatibility, and verified against ULVO's Unified Execution Gateway and Vault Managed environments.

A total of **11.749 GB** across **337,717 files** on `C:` is currently consumed by external toolchains whose capabilities are now fully operational through ULVO.

| External Installation Path | Ecosystem | Version | Disk Size (MB) | Disk Size (GB) | Files | Managed Replacement | Verification Status |
| :--- | :--- | :--- | :---: | :---: | :---: | :--- | :---: |
| `C:\msys64` | C / C++ / MinGW | GCC 14.2.0 | 5,218.82 | 5.097 | 69,699 | `vault/toolchains/msys2` | **VERIFIED** |
| `C:\Program Files\nodejs` | Node.js / npm | v24.15.0 | 3,946.54 | 3.854 | 168,542 | ULVO Gateway / Catalog | **VERIFIED** |
| `C:\Ruby40-x64` | Ruby / Gems | 4.0.1 | 1,224.45 | 1.196 | 43,544 | ULVO Gateway `ruby` | **VERIFIED** |
| `C:\Users\hp\AppData\Local\Programs\Python\Python313` | Python / pip | 3.13.0 | 1,088.28 | 1.063 | 40,419 | Vault Python / uv | **VERIFIED** |
| `C:\Program Files\Eclipse Adoptium\jdk-21.0.4.7-hotspot` | Java / JDK | 21.0.4 | 326.19 | 0.319 | 487 | ULVO Gateway `java`/`javac` | **VERIFIED** |
| `C:\Program Files\Go` | Go Runtime | go1.26.5 | 224.89 | 0.220 | 15,026 | ULVO Gateway `go` | **VERIFIED** |
| **TOTALS** | | | **12,029.17** | **11.749** | **337,717** | | |

---

## 2. Detailed Toolchain Profiles & Retirement Assessment

### Candidate 1: `C:\msys64` (Priority High)
- **Path:** `C:\msys64`
- **Installed Components:** MinGW UCRT64 toolchain, GCC 14.2.0, G++ 14.2.0, binutils, make, pacman database.
- **Physical Footprint:** 5.097 GB (69,699 files)
- **Managed Replacement:** `E:\Workspace\Active\ULVO\vault\toolchains\msys2` (1.350 GB, 52,562 files).
- **Verification Evidence:**
  - `ulvo toolchain msys2 verify`: PASS (both native C and C++ compilation and execution passed with zero dependency on `C:\msys64`).
  - `ulvo exec --tool g++ -- -o test_cpp.exe test_cpp.cpp`: PASS (Sum = 55).
- **Known Consumers:** Workspace Cargo GNU builds, native C/C++ extensions, CLI tools.
- **Estimated Recoverable Storage:** **5.097 GB** on `C:`.
- **Rollback Plan:** Supported installer `msys2-x86_64-latest.exe` or official tarball restoration.
- **Residual Risk:** Low. If external non-UCRT environments (e.g. `mingw32`, `clang64`) were used, they should be exported first.
- **Recommendation:** Approve retirement via official uninstaller or standard folder retirement once user confirms no independent MinGW32 dependencies exist.

### Candidate 2: `C:\Program Files\nodejs` (Priority High)
- **Path:** `C:\Program Files\nodejs`
- **Installed Components:** Node.js v24.15.0, npm 11.x, pnpm, corepack.
- **Physical Footprint:** 3.854 GB (168,542 files)
- **Managed Replacement:** ULVO Gateway resolution and catalog-managed Node.js runtimes.
- **Verification Evidence:** `ulvo exec --tool node -- -v` outputs `v24.15.0`.
- **Known Consumers:** Tauri desktop frontend build, local web development.
- **Estimated Recoverable Storage:** **3.854 GB** on `C:`.
- **Rollback Plan:** Standard Windows MSI installer from nodejs.org.
- **Residual Risk:** Global npm packages installed directly in `AppData\Roaming\npm`.
- **Recommendation:** Retain or adopt into managed store; do not uninstall until user verifies all global npm scripts.

### Candidate 3: `C:\Ruby40-x64` (Priority Medium)
- **Path:** `C:\Ruby40-x64`
- **Installed Components:** Ruby 4.0.1, RubyGems, Bundler.
- **Physical Footprint:** 1.196 GB (43,544 files)
- **Managed Replacement:** ULVO Gateway `ruby` adapter.
- **Verification Evidence:** `ulvo exec --tool ruby -- test_ruby.rb` executed successfully (Array sum = 15).
- **Known Consumers:** Ruby CLI scripts.
- **Estimated Recoverable Storage:** **1.196 GB** on `C:`.
- **Rollback Plan:** RubyInstaller 4.0.1-1.
- **Residual Risk:** Low.

### Candidate 4: `C:\Users\hp\AppData\Local\Programs\Python\Python313` (Priority Medium)
- **Path:** `C:\Users\hp\AppData\Local\Programs\Python\Python313`
- **Installed Components:** Python 3.13.0, pip, standard library.
- **Physical Footprint:** 1.063 GB (40,419 files)
- **Managed Replacement:** ULVO Vault Python runtimes and `uv` package manager with thin connectors.
- **Verification Evidence:** `ulvo run AstraRun` executed on Python 3.13.0 with zero local `.venv` duplication.
- **Known Consumers:** Global Python 3.13 scripts.
- **Estimated Recoverable Storage:** **1.063 GB** on `C:`.
- **Rollback Plan:** Python 3.13 installer.
- **Residual Risk:** Windows shell file association for `.py` files.

### Candidate 5: `C:\Program Files\Eclipse Adoptium\jdk-21.0.4.7-hotspot` (Priority Low)
- **Path:** `C:\Program Files\Eclipse Adoptium\jdk-21.0.4.7-hotspot`
- **Installed Components:** Temurin OpenJDK 21.0.4 LTS, javac, jar, hotspot runtime.
- **Physical Footprint:** 0.319 GB (487 files)
- **Managed Replacement:** ULVO Gateway `java` and `javac`.
- **Verification Evidence:** `ulvo exec --tool javac` and `ulvo exec --tool java` compiled and executed `TestJava.java` successfully.
- **Known Consumers:** Java builds, Android SDK tools.
- **Estimated Recoverable Storage:** **0.319 GB** on `C:`.
- **Rollback Plan:** Eclipse Adoptium Temurin MSI installer.
- **Residual Risk:** Low.

### Candidate 6: `C:\Program Files\Go` (Priority Low)
- **Path:** `C:\Program Files\Go`
- **Installed Components:** Go compiler and standard library (go1.26.5).
- **Physical Footprint:** 0.220 GB (15,026 files)
- **Managed Replacement:** ULVO Gateway `go`.
- **Verification Evidence:** `ulvo exec --tool go -- run test_go.go` built and executed successfully.
- **Known Consumers:** Go modules.
- **Estimated Recoverable Storage:** **0.220 GB** on `C:`.
- **Rollback Plan:** Official Go MSI installer.
- **Residual Risk:** Low.

---

## 3. Human Approval Gate

> [!IMPORTANT]
> **ACTION REQUIRED FROM USER:**
> Pursuant to ULVO Safety Invariants, none of the external toolchains above have been deleted, moved, or modified on `C:`.
>
> All toolchains remain intact at their original paths on `C:` and are independently operational.
>
> To retire any of these candidates and reclaim up to **11.75 GB** on `C:`, please review this report and grant explicit authorization for the specific candidates you wish to retire.
