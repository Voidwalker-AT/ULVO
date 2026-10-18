# ULVO Stage 2.5 — Preflight Security & Readiness Report

**Date**: 2026-10-18  
**Mission**: ULVO Stage 2.5 — Establish a verified Windows-native Rust toolchain inside the single ULVO root, validate native compilation and linking readiness, and prepare the existing ULVO Rust workspace for its first Windows build.  
**Auditor / Implementation Agent**: Antigravity (Pair Programming Agent)  
**Target Repository**: `E:\Workspace\Active\ULVO`  
**Execution Environment**: Windows 11 Home Single Language (64-bit, Build 26200), Windows PowerShell 5.1 (`powershell.exe -NoProfile`)  
**Preflight Verdict**: **READY FOR APPROVAL** (Read-Only Preflight Complete; Execution Halted at Approval Gate)

---

## 1. Epistemic Verification Taxonomy

Every finding in this report is classified according to GRAVITAS epistemic rules:
- **`[VERIFIED]`**: Directly observed from current source or execution output in this session.
- **`[INFERRED]`**: Supported by direct evidence but not yet exercised end-to-end.
- **`[UNVERIFIED]`**: Not tested or empirical evidence currently unavailable.
- **`[BLOCKED]`**: Cannot proceed without operator authorization or missing prerequisites.

---

## 2. Preflight Findings (P01 – P06)

### P01 — Repository Identity & Ancestor Integrity
- **`[VERIFIED]` Repository Root**: Physical path confirmed at `E:\Workspace\Active\ULVO`.
- **`[VERIFIED]` Git State**: Branch `master`. No commits yet. Zero files staged.
- **`[VERIFIED]` Untracked Files**: `.gitignore`, `Cargo.lock`, `Cargo.toml`, `README.md`, `config/`, `crates/`, `docs/`, `scripts/`, `tests/`.
- **`[VERIFIED]` Persistent Vault Git-Ignored**: `git check-ignore vault/` and `git check-ignore vault/runtimes/rust/Downloads/rustup-init.exe` confirmed ignored by `.gitignore`.
- **`[VERIFIED]` Cargo Workspace**: `Cargo.toml` declares 4 workspace members:
  1. `crates/ulvo-core`
  2. `crates/ulvo-adapters`
  3. `crates/ulvo-discovery`
  4. `crates/ulvo-cli`
  All four directories exist under `crates/`.
- **`[VERIFIED]` Ancestor Reparse Points**: Inspected `E:\Workspace\Active\ULVO`, `E:\Workspace\Active`, `E:\Workspace`, and `E:\`. All report `IsReparsePoint = False`. No unexpected junctions or symlinks exist in repository ancestry.

### P02 — Windows Environment Inspection
- **`[VERIFIED]` Operating System**: Microsoft Windows 11 Home Single Language, Version 10.0.26200, 64-bit AMD64 architecture.
- **`[VERIFIED]` PowerShell Engine**: Windows PowerShell 5.1.26100.9444 (Desktop Edition, CLR 4.0.30319.42000).
- **`[VERIFIED]` Filesystem**: Drive `E:` is NTFS, Fixed Drive, Healthy (`OperationalStatus: OK`).
- **`[VERIFIED]` Storage Space**: Drive `E:` has 15,400,820,736 bytes (~14.34 GB) free, well exceeding the 2.0 GB gate threshold.
- **`[VERIFIED]` Host Environment Variables**: No global `RUSTUP_HOME` or `CARGO_HOME` set in current environment.
- **`[VERIFIED]` Existing Host Rust Installations**: Zero Rust executables on PATH. `Get-Command rustc, cargo, rustup` returns nothing. User profile directory `C:\Users\hp\.cargo\bin` exists but is completely empty (0 files). `C:\Users\hp\.rustup` does not exist.
- **`[VERIFIED]` Windows SDK / MSVC Status**: `vswhere.exe -all -format json` returns `[]`. No Visual Studio C++ toolchain / MSVC compiler is installed on this host. MSVC target cannot link natively without external tools.

### P03 — Existing Installer Analysis & Collision Discovery
- **`[VERIFIED]` Target File**: `E:\Workspace\Active\ULVO\vault\runtimes\rust\Downloads\rustup-init.exe`
- **`[VERIFIED]` File Size on Disk**: 6,815,109 bytes (~6.50 MB).
- **`[VERIFIED]` Computed SHA-256**: `1F9C2AE6604AFB42862081FF3729B1E531561894B84A06C3C7BA14F6D344978D`
- **`[VERIFIED]` Authenticode Status**: `NotSigned` (Unsigned PE).
- **`[VERIFIED]` PE Machine Architecture**: `0x8664` (`IMAGE_FILE_MACHINE_AMD64`).
- **`[VERIFIED]` PE Header Anomaly (Truncation Discovery)**:
  Direct parsing of PE section headers reveals:
  - `.text` section: Raw Address `0x00000400`, Raw Size `0x008C8800` (ends at `0x008C8C00`, 9,210,880 bytes).
  - `.rdata` section: Starts at Raw Address `0x008C8C00`, Raw Size `0x002F1800` (3,086,336 bytes).
  - `.data`, `.pdata`, `.fptable`, `.reloc` sections extend to offset `0x00C21E00` (12,721,664 bytes).
  - **Conclusion**: The file is truncated at 6,815,109 bytes (approx. 53% of its 12.7 MB total). It was severed midway through `.text`. The entire `.rdata` section (containing all strings and constants), data, and certificate tables were never written. It is an incomplete historical test artifact from an earlier session, incapable of execution, and does not match the official pinned GNU digest.
- **`[BLOCKED]` Destination Collision Disposition**: The destination path `vault\runtimes\rust\Downloads\rustup-init.exe` is occupied by this truncated artifact. Under current script behavior, downloading would overwrite it. To preserve historical evidence non-destructively, the proposed plan requires operator approval to archive the truncated file to `vault\runtimes\rust\Downloads\rustup-init.exe.truncated-1f9c2ae6.bak` before downloading the official binary.

### P04 — Manifest Provenance Assessment
- **`[VERIFIED]` Manifest Location**: `E:\Workspace\Active\ULVO\config\toolchain-artifacts.toml` (Schema version `1.0.0`).
- **`[VERIFIED]` Target Artifact**: `rustup-init` for `x86_64-pc-windows-gnu`.
- **`[VERIFIED]` Canonical URL**: `https://static.rust-lang.org/rustup/archive/1.29.0/x86_64-pc-windows-gnu/rustup-init.exe`
- **`[VERIFIED]` Pinned SHA-256**: `03dbaae1d33a4d220bd0d202e5092955dae859c119074192ca513f8c4713fff7` (Matches official companion `.sha256` published on `static.rust-lang.org`).
- **`[VERIFIED]` Verification Policy**: `manifest-sha256-pinned`.
- **`[VERIFIED]` Current Approval Status**: `unapproved` (Gate 3.5 will reject execution until authorized).

### P05 — MSYS2 Toolchain & Linker Readiness
- **`[VERIFIED]` Compiler**: `C:\msys64\ucrt64\bin\gcc.exe` (Version 14.2.0, Rev3, Built by MSYS2 project).
- **`[VERIFIED]` Target Architecture**: `x86_64-w64-mingw32` (Thread model: posix).
- **`[VERIFIED]` GNU Linker**: `C:\msys64\ucrt64\bin\ld.exe` (GNU ld Binutils 2.42).
- **`[VERIFIED]` Runtime Environment**: UCRT (Universal C Runtime, `ucrtbase.dll`).
- **`[VERIFIED]` Shared Libraries**: `libgcc_s_seh-1.dll`, `libwinpthread-1.dll`, `libstdc++-6.dll` are present in `C:\msys64\ucrt64\bin`.
- **`[INFERRED]` Linker Compatibility**: Rust's `x86_64-pc-windows-gnu` target utilizes MinGW GCC as its default linker driver. `C:\msys64\ucrt64\bin` is already present on the current process `PATH`.

### P06 — WSL Rust Separation
- **`[VERIFIED]` Isolation**: Ubuntu WSL was not started and was not accessed. No Linux binaries will be copied into the Windows vault.

---

## 3. Production Script Execution-Path Review

A non-mutating dry-run was executed:
```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\provision-rust-windows.ps1 -DryRun
```
Output:
- UlvoRoot validated: `E:\Workspace\Active\ULVO` (Canonical production root).
- Manifest loaded and validated: `config/toolchain-artifacts.toml`.
- Artifact selected: `rustup-init v1.29.0 (x86_64-pc-windows-gnu)`.
- Expected digest: `03dbaae1d33a4d220bd0d202e5092955dae859c119074192ca513f8c4713fff7`.
- Path containment passed: `RustupHome` and `CargoHome` inside `vault\runtimes\rust\`.
- Free space check passed: 14.34 GB available.
- Dry-run exited 0 with zero mutations.

---

## 4. Expected Changes & Risk Analysis

If explicit human authorization is granted:
1. **Manifest Mutation**:
   In `config/toolchain-artifacts.toml`, update entry 1 (`x86_64-pc-windows-gnu`):
   Change `approval_status = "unapproved"` to `approval_status = "approved"`.
   Entry 2 (`x86_64-pc-windows-msvc`) remains strictly `approval_status = "unapproved"`.
2. **Existing Truncated File Preservation**:
   Safely rename existing truncated installer:
   `vault\runtimes\rust\Downloads\rustup-init.exe` -> `vault\runtimes\rust\Downloads\rustup-init.exe.truncated-1f9c2ae6.bak`.
3. **Network Download**:
   Download official `rustup-init.exe` (1.29.0 GNU) from `https://static.rust-lang.org/rustup/archive/1.29.0/x86_64-pc-windows-gnu/rustup-init.exe`.
   Size: approx. 12.7 MB.
4. **Integrity Gates**:
   - Gate 4.5: Asserts computed SHA-256 matches `03dbaae1d33a4d220bd0d202e5092955dae859c119074192ca513f8c4713fff7`.
   - Gate 5: Re-hashes binary immediately prior to launch (`Invoke-RustupInstaller`).
5. **Toolchain Installation**:
   Executes `rustup-init.exe` with arguments:
   `-y --default-host x86_64-pc-windows-gnu --default-toolchain 1.96.0 --profile minimal --no-modify-path -q`
   Installs `rustc.exe`, `cargo.exe`, `rustup.exe`, and `x86_64-pc-windows-gnu` standard library strictly under:
   `E:\Workspace\Active\ULVO\vault\runtimes\rust\rustup`
   `E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo`
6. **Zero Host Mutation**:
   - Zero modifications to Windows system PATH.
   - Zero registry modifications.
   - Zero ACL changes.
   - Zero modifications to MSYS2 or WSL.

---

## 5. Proposed Installation Command Requiring Approval

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\provision-rust-windows.ps1 -Install -RustVersion 1.96.0 -InstallerVersion 1.29.0 -Target x86_64-pc-windows-gnu
```

---

## 6. Preflight Determination

### Verdict: **READY FOR APPROVAL**
Execution is stopped at the approval gate. Awaiting explicit operator authorization before any mutation.
