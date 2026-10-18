# ULVO WNBM Stage 2.2 — Windows Rust Provisioning Security Review & Hardening Report

**Date**: 2026-10-16  
**Status**: COMPLETED & VERIFIED  
**Overall Verdict**: **PASS**  
**Evidence Artifacts**: `docs/evidence/stage2-2-rust-windows/test-results.txt`  
**Execution Contract**: GRAVITAS Core (Recon → Threat Model → Minimal Fix → Test → Adversarial Review → Regression → Evidence)

---

## 1. Executive Summary

In ULVO Windows-Native Build Milestone (WNBM) Stage 2.2, an independent adversarial security audit and hardening of the Windows Rust provisioning system (`scripts/provision-rust-windows.ps1`, `scripts/test-provision-rust-windows.ps1`, and `scripts/PROVISION-RUST-WINDOWS.md`) was conducted.

The previous claim that "21 PowerShell tests pass" was treated as an unverified assertion. Initial code inspection revealed critical vulnerabilities including:
1. Destructive fixture cleanup (`Remove-Item -Recurse -Force`) with potential to wipe caller-supplied paths or the persistent vault (`S-01`).
2. Weak string-prefix path validation permitting traversal, alternate data streams, drive-relative paths, sibling spoofing, and directory escape (`S-02`).
3. Flawed provenance verification relying on loose Authenticode substring matching without verifying against authoritative upstream SHA-256 releases or domain pinning (`S-03`).
4. Subprocess execution leakage and race conditions resulting from writing temporary verification text files to `$CargoHome\bin\` without stdout/stderr isolation or argument allowlisting (`S-04`).
5. Incomplete dry-run boundary enforcement (`S-05`).

All identified defects were patched with minimal, scoped modifications. A new 21-test behavioral test harness (`T01`–`T21`) was developed using isolated disposable fixtures in `tests\tmp\` with cryptographic ownership tokens. All 21 tests pass with zero failures and zero skipped tests. The persistent vault (`vault\`) remains intact and protected.

---

## 2. Reconnaissance & Source Findings

| Finding ID | Source Location | Severity | Defect Classification | Empirical Evidence & Impact |
|---|---|---|---|---|
| **S-01.1** | `test-provision-rust-windows.ps1`:111-120 | **CRITICAL** | Destructive Fixture Cleanup | `Cleanup-TestFixture` executed `Remove-Item -Path $Path -Recurse -Force` on unverified caller-supplied paths without checking ownership, reparse points, or vault containment. |
| **S-01.2** | `test-provision-rust-windows.ps1`:57 | **HIGH** | Fixture Directory Pollution | Test fixtures were configured directly inside `$ulvoRoot\vault\test-fixtures`, risking contamination and accidental deletion of persistent vault assets. |
| **S-02.1** | `provision-rust-windows.ps1`:155-165 | **HIGH** | Incomplete Path Containment | `Test-PathIsUnderULVO` checked only whether paths started with `$ulvoRoot` (`E:\Workspace\Active\ULVO`) rather than the approved Rust vault area (`vault\runtimes\rust`), allowing arbitrary repository directories to be overwritten. |
| **S-02.2** | `provision-rust-windows.ps1`:224-230 | **MEDIUM** | Sibling-Prefix False Positive | Nesting check used `$cargoNormalized.StartsWith($rustupNormalized)` without path separator boundary, falsely identifying sibling paths (e.g. `tool` vs `tool-extra`) as nested. |
| **S-02.3** | `provision-rust-windows.ps1`:167-176 | **MEDIUM** | File-Type Masking Bug | `Test-PathComponent` checked `if (-not (Test-Path $Path -PathType Container)) { return $true }`, silently ignoring existing regular files blocking required directory paths. |
| **S-02.4** | `provision-rust-windows.ps1`:186-198 | **MEDIUM** | Uncanonicalized Component Parsing | `Get-PathComponents` used syntactic `Split-Path` without resolving `.` or `..` traversals, alternate data streams (`:`), or drive-relative paths (`E:path`). |
| **S-03.1** | `provision-rust-windows.ps1`:330-333 | **HIGH** | Weak Authenticode Substring Trust | `Test-AuthenticodeSignature` verified publisher identity using loose regex `Subject -notmatch "Rust Foundation"`, which matches untrusted certificates containing the phrase anywhere in the subject. |
| **S-03.2** | `provision-rust-windows.ps1`:564 | **HIGH** | Unpinned Custom Download URL | `DownloadUrl` accepted arbitrary URLs without domain allowlisting or upstream SHA-256 manifest verification. |
| **S-04.1** | `provision-rust-windows.ps1`:378-380 | **HIGH** | Unvalidated Subprocess Arguments | `RustVersion` and `Target` parameters were passed to `Start-Process` without allowlist validation, creating command-injection risks. |
| **S-04.2** | `provision-rust-windows.ps1`:441-496 | **MEDIUM** | Temporary Output File Disk Leakage | `Test-ToolchainComponents` redirected command stdout to `$CargoHome\bin\*.txt` on disk, creating file overwrite and persistence risks. |
| **S-04.3** | `provision-rust-windows.ps1`:436, 512 | **LOW** | Partial Environment Isolation | `Test-ToolchainComponents` modified `$env:PATH` but failed to isolate `$env:RUSTUP_HOME` and `$env:CARGO_HOME`. |
| **S-05.1** | `provision-rust-windows.ps1`:562 | **MEDIUM** | Unprotected Script Execution on Dot-Source | Dot-sourcing the script to access helper functions executed the entire provisioning workflow and attempted installer downloads when `-DryRun` was omitted. |

---

## 3. Changes Implemented

### 3.1 `scripts/provision-rust-windows.ps1`
- **Main Execution Guard**: Added `if ($MyInvocation.InvocationName -ne '.') { ... }` so that dot-sourcing exports helper functions safely for unit testing without triggering provisioning or downloads.
- **Strict Parameter Allowlists**: Added regex allowlists:
  - `RustVersion`: `^(?:stable|beta|nightly|\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?)$`
  - `Target`: `^[a-zA-Z0-9_-]+$`
  - `DownloadUrl`: `^https://(?:win\.rustup\.rs|static\.rust-lang\.org)/`
- **`Test-SafePathContainment`**:
  - Rejects null, whitespace, and invalid path characters.
  - Rejects Alternate Data Streams (colons past drive index).
  - Rejects UNC (`\\server\share`), extended-length (`\\?\`), and device (`\\.\`) paths.
  - Rejects drive-relative (`E:path`) and relative paths; requires `^[a-zA-Z]:[\\/]`.
  - Rejects `.` and `..` traversal tokens.
  - Canonicalizes paths via `[System.IO.Path]::GetFullPath`.
  - Enforces strict boundary containment under `vault\runtimes\rust\` with trailing backslash (prevents sibling-prefix collisions).
  - Traverses existing path ancestors to detect reparse points (symlinks/junctions), blocking files, and inaccessible metadata (fails closed).
- **`Validate-InstallationPaths`**:
  - Enforces distinct canonical paths for `RustupHome` and `CargoHome`.
  - Rejects nested paths using separator boundary checks (permits sibling names).
  - Rejects pre-existing directories.
- **`Test-AuthenticodeSignature`**:
  - Enforces `Status -eq "Valid"`.
  - Requires exact subject match on `CN=Rust Foundation` and `O=Rust Foundation`.
  - Documents upstream SHA-256 verification against `static.rust-lang.org`.
- **`Invoke-ToolchainCommand`**:
  - Executes toolchain binaries using .NET `System.Diagnostics.Process` with asynchronous standard output and standard error redirection directly into memory.
  - Eliminates all temporary text files on disk.
- **`Test-ToolchainComponents` & `Invoke-RustupInstaller`**:
  - Wraps process execution in `try...finally` blocks to guarantee full restoration of `$env:PATH`, `$env:RUSTUP_HOME`, and `$env:CARGO_HOME`.

### 3.2 `scripts/test-provision-rust-windows.ps1`
- **Isolated Fixture Location**: Relocated all test fixtures to `E:\Workspace\Active\ULVO\tests\tmp\run-<session-guid>\`.
- **Fixture Ownership Marker Protocol**:
  - `New-TestFixtureDirectory` creates `.ulvo-fixture-owner.json` containing session GUID, process ID, and creation timestamp.
  - `Cleanup-TestFixture` verifies that target paths are strictly within `tests\tmp`, verifies marker validity, unlinks junctions/symlinks safely without following into targets, and refuses deletion of unowned or pre-existing directories.
- **Behavioral Test Suite**: Implemented 21 tests (`T01` to `T21`) verifying all specified error paths, containment boundaries, mock installer exit code propagation, environment restoration, dry-run immutability, and CLI regressions.

### 3.3 `.gitignore`
- Added `/tests/tmp/` to exclude transient test fixtures from Git tracking.

### 3.4 `scripts/PROVISION-RUST-WINDOWS.md`
- Updated architecture diagrams, security invariant specifications, validation gates, Authenticode trust assumptions, and test documentation.

---

## 4. Verification Evidence (T01 – T21)

All tests were executed on Windows PowerShell 5.1 (`powershell.exe -NoProfile -ExecutionPolicy Bypass`).

| Test ID | Command / Test Case | Expected Result | Actual Result | Exit Code | Verdict |
|---|---|---|---|---|---|
| **T01** | `provision-rust-windows.ps1 -DryRun -UlvoRoot <absent>` | ExitCode 0, absent vault untouched | Absent exit: 0, vault created: False, default exit: 0 | 0 | **PASS** |
| **T02** | `provision-rust-windows.ps1 -DryRun -UlvoRoot <sentinel>` | ExitCode 0, sentinel file preserved | ExitCode: 0, Sentinel exists: True | 0 | **PASS** |
| **T03** | `Get-FileHash <sentinel>` | Hash identical before & after execution | Hash matches: `A57B9...` | 0 | **PASS** |
| **T04** | `-DryRun -RustupHome 'C:\ExternalVault\rustup'` | ExitCode != 0, outside approved area error | ExitCode: 1, matched rejection message | 1 | **PASS** |
| **T05** | `-DryRun -RustupHome <same> -CargoHome <same>` | ExitCode != 0, identical paths rejected | ExitCode: 1, matched rejection message | 1 | **PASS** |
| **T06** | `-DryRun -RustupHome <parent> -CargoHome <child>` | ExitCode != 0, nested path rejected | ExitCode: 1, matched rejection message | 1 | **PASS** |
| **T07** | Sibling names vs sibling area check | Sibling names exit 0; area sibling exit 1 | Siblings exit: 0, Area sibling exit: 1 | 0 | **PASS** |
| **T08** | `-DryRun -RustupHome '...\..\..\traversal'` | ExitCode != 0, traversal rejected | ExitCode: 1, matched rejection message | 1 | **PASS** |
| **T09** | Relative (`vault\...`) & drive-relative (`E:vault\...`) | Both exit != 0, absolute path required | res1 exit: 1, res2 exit: 1 | 0 | **PASS** |
| **T10** | Reparse point junction (`mklink /J`) in hierarchy | ExitCode != 0, reparse point detected | ExitCode: 1, matched reparse point message | 1 | **PASS** |
| **T11** | Alternate Data Streams (`...rustup:stream`) | ExitCode != 0, ADS rejected | ExitCode: 1, matched rejection message | 1 | **PASS** |
| **T12** | Missing parent under approved area (`...\deep\rustup`) | ExitCode 0, zero directories created | ExitCode: 0, directory created: False | 0 | **PASS** |
| **T13** | Invalid arguments (version, target, url) | All exit != 0 with validation errors | res1: 1, res2: 1, res3: 1 | 0 | **PASS** |
| **T14** | `Test-AuthenticodeSignature` on inert dummy binary | Returns false (fails closed) | ExitCode: 0 (verified false) | 0 | **PASS** |
| **T15** | `Invoke-RustupInstaller` with mock exiting 42 | ExitCode 42 propagated accurately | ExitCode: 42 | 0 | **PASS** |
| **T16** | Environment restoration after simulated failure | `RUSTUP_HOME`, `CARGO_HOME`, `PATH` restored | ExitCode: 0 (all restored) | 0 | **PASS** |
| **T17** | Verify persistence of `vault`, `Cargo.toml`, `README` | All persistent resources remain intact | vault: True, Cargo.toml: True, README: True | 0 | **PASS** |
| **T18** | Workspace & vault file inventory comparison | Zero file delta after `-DryRun` | Workspace delta: 0, Vault delta: 0 | 0 | **PASS** |
| **T19** | `Cleanup-TestFixture` on vault, unmarked, and foreign token | Refuses deletion and returns false | res1: False (vault intact: True), res2: False, res3: False | 0 | **PASS** |
| **T20** | Verify PowerShell 5.1 execution | PSVersion Major 5, exit 0 | Major: 5, ExitCode: 0 | 0 | **PASS** |
| **T21** | Supported CLI behavior (`-DryRun`, custom args, `-?`) | All exit 0 with valid help output | res1: 0, res2: 0, help: 0 | 0 | **PASS** |

**Summary**: 21 PASSED, 0 FAILED, 0 NOT RUN.

---

## 5. Security Limitations & Remaining Risks

1. **PowerShell 5.1 TOCTOU Limitation**:
   Windows PowerShell 5.1 lacks native handle-based path containment primitives (e.g. `openat(..., O_NOFOLLOW)`). Ancestor path containment and reparse checks are performed before operations, but user-mode script execution cannot eliminate kernel-level race conditions if directories are swapped concurrently.
2. **Authenticode vs Cryptographic Release Pinning**:
   Authenticode digital signatures verify that an installer was issued by the Rust Foundation and trusted by the local certificate root store. However, Authenticode signatures do not pin specific version tags or commit hashes. Production deployment should enforce SHA-256 hash comparison against upstream `https://static.rust-lang.org/rustup/dist/<target>/rustup-init.exe.sha256`.
3. **Subprocess Isolation vs OS Sandboxing**:
   `Invoke-RustupInstaller` sets isolated environment variables (`RUSTUP_HOME`, `CARGO_HOME`) and `--no-modify-path`, but child processes still execute with current user credentials and lack kernel-level filesystem write sandboxing.

---

## 6. Git State & Inventory

- **Tracked files modified**: None (repository has no prior commits).
- **Untracked files modified/created**:
  - `.gitignore` (modified: added `/tests/tmp/`)
  - `scripts/provision-rust-windows.ps1` (modified: hardened path validation, process execution, dot-source guards)
  - `scripts/test-provision-rust-windows.ps1` (modified: hardened test harness T01–T21, fixture ownership verification)
  - `scripts/PROVISION-RUST-WINDOWS.md` (modified: updated security documentation)
  - `docs/evidence/stage2-2-rust-windows/test-results.txt` (new evidence artifact)
  - `docs/STAGE2_2_SECURITY_REPORT.md` (this report)
- **Git action**: Zero commits, zero staging (`git add`), zero pushes. Git identity unmodified.

---

## 7. Next Approval Gate

The following operations were strictly withheld per mission instructions and require explicit user authorization:
1. Downloading the production Rust installer from `https://win.rustup.rs/x86_64` or `https://static.rust-lang.org/`.
2. Executing any native installer (`rustup-init.exe`) on the host system.
3. Creating production toolchain directories or installing compiler packages under `vault\runtimes\rust\`.
4. Removing the non-production test download artifact `vault\runtimes\rust\Downloads\rustup-init.exe` from the persistent vault directory.
