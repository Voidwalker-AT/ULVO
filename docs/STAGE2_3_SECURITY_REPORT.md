# ULVO WNBM Stage 2.3 — Residual Security Defects Resolution & Approval Gate Report

**Date**: 2026-10-16  
**Status**: DEFECTS REPAIRED & INDEPENDENTLY VERIFIED  
**Overall Provisioning Approval Gate Recommendation**: **NO-GO** (Production Execution Blocked)  
**Regression Test Results**: 21 PASSED, 0 FAILED, 0 NOT RUN (Exit Code: 0)  
**Raw Test Evidence**: `docs/evidence/stage2-3-rust-windows/test-results.txt`  
**Execution Standard**: GRAVITAS Adversarial Verification & Fail-Closed Boundary Testing  

---

## 1. Executive Summary

In ULVO Windows-Native Build Milestone (WNBM) Stage 2.3, an adversarial code audit was conducted on the residual security defects identified in the Windows Rust provisioning system. All 5 confirmed defects were repaired, regression tested, and verified using safe, disposable test fixtures strictly isolated under `E:\Workspace\Active\ULVO\tests\tmp\`.

The 21-test automated behavioral test suite (`scripts/test-provision-rust-windows.ps1`) executed cleanly with exit code `0`, confirming:
1. **Zero Cleanup Bypasses**: All direct filesystem modifications and deletions outside `Cleanup-TestFixture` were eliminated from test code.
2. **Pre-Unlinking Reparse Point Ownership Verification**: Reparse points (junctions and symbolic links) are never unlinked or deleted without verifying sidecar ownership markers and in-memory session registration. Unowned junctions are rejected fail-closed.
3. **Cryptographic Integrity Verification in T18**: Naive file count checks were replaced with deterministic before/after SHA-256 hash maps across all bounded areas (`$ulvoRoot` root, `vault`, `scripts`, `crates`) and a randomized nonce sentinel file, proving bit-for-bit filesystem preservation during dry-run execution.
4. **Accurate T11 Test Specification**: The test was aligned and renamed to `Alternate Data Stream (ADS) path containment rejection`, eliminating inaccurate coverage claims.
5. **Empirical Upstream Provenance Discovery & Gate Verdict**: Inspection of upstream distribution binaries (`rustup-init.exe`) revealed that official binaries distributed by `static.rust-lang.org` are **not Authenticode-signed** (`Status: NotSigned`), upstream GPG signatures were removed in rustup 1.26.0, and SHA-256 hashes are served as unauthenticated plaintext over HTTPS. Consequently, unattended production provisioning cannot guarantee binary authenticity and is rated **NO-GO**.

---

## 2. Confirmed Findings & Resolution Matrix

| Finding ID | Component | Defect Description | Resolution & Hardening | Adversarial Verification |
|---|---|---|---|---|
| **F-01** | `test-provision-rust-windows.ps1` (T19) | Direct `Remove-Item -Recurse -Force` calls bypassed ownership-aware cleanup on unmarked and foreign-session fixtures. | Eliminated all direct `Remove-Item` and manual deletion calls in tests. In T19, fixtures are adopted by writing valid current session markers and registering them before calling `Cleanup-TestFixture`. | T19 passes with zero direct deletion calls; all cleanups route through `Cleanup-TestFixture`. |
| **F-02** | `test-provision-rust-windows.ps1` (`Cleanup-TestFixture`) | Reparse points were deleted prior to validating fixture ownership, permitting deletion of unowned junctions in `tests\tmp`. | Reparse points now require a sidecar ownership token (`<path>.junction-owner.json`) and in-memory registry membership before unlinking. Standalone directories require `.ulvo-fixture-owner.json` and registry validation. | Adversarial check in T19 attempts to clean an unowned junction; `Cleanup-TestFixture` logs a warning, returns `$false`, and leaves the junction untouched. |
| **F-03** | `test-provision-rust-windows.ps1` (T18) | Dry-run integrity tested only coarse file counts, which could not detect in-place file modifications or replacement. | Replaced file count checks with cryptographic SHA-256 snapshot comparisons across all files in `$ulvoRoot` root, `vault`, `scripts`, and `crates`, plus an isolated GUID-nonce sentinel file. | T18 computed baseline hashes across 33 bounded files, ran `-DryRun`, and verified 0 added, 0 removed, 0 modified files, and intact sentinel hash. |
| **F-04** | `test-provision-rust-windows.ps1` (T11) | T11 claimed "inaccessible metadata" coverage but actually tested colon-delimited Alternate Data Streams. | Renamed test and result entries to `Alternate Data Stream (ADS) path containment rejection`; updated documentation and comments to accurately reflect ADS containment testing. | T11 verified ADS path rejection with exit code 1 and matching error message. |
| **F-05** | `provision-rust-windows.ps1` (Gates 3.5 & 5) | Installer authenticity was unpinned and relied on assumptions of Authenticode validity. | Added Gate 3.5 requiring explicit `-Install` switch and 64-hex `-ExpectedSha256` parameter. Documented empirical finding that upstream binary is unsigned. | Provisioning script strictly fails closed without `-ExpectedSha256` and fails closed when Authenticode signature is absent or unverified. |

---

## 3. Upstream Installer Authenticity & Provenance Investigation

### 3.1 Upstream Distribution Mechanics
- **Redirection Endpoint**: `https://win.rustup.rs/{target}` is an HTTP 302 redirector pointing to `https://static.rust-lang.org/rustup/dist/{target}/rustup-init.exe`.
- **GPG/PGP Deprecation**: Upstream rustup removed PGP/GPG signature verification support in rustup version 1.26.0 (GitHub issue rust-lang/rustup#2711). Detached `.asc` signature files are no longer published for `rustup-init.exe`.
- **Authenticode Signature Status**: Local PowerShell inspection (`Get-AuthenticodeSignature`) of official `rustup-init.exe` binaries downloaded from `static.rust-lang.org` demonstrates:
  ```powershell
  SignerCertificate : 
  Status            : NotSigned
  StatusMessage     : The file ...\rustup-init.exe is not digitally signed.
  SignatureType     : None
  IsOSBinary        : False
  ```
  The official `rustup-init.exe` release binaries are **not digitally signed** with an Authenticode code-signing certificate from the Rust Foundation or any trusted CA.
- **SHA-256 Checksum Distribution**: Upstream publishes plain `.sha256` text files alongside binaries (e.g., `https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-gnu/rustup-init.exe.sha256`). These files are served over standard TLS without detached signatures or transparency logs.

### 3.2 Security Risk Evaluation
1. **Lack of Authenticode Defense**: Because the upstream binary is unsigned, `Test-AuthenticodeSignature` in `provision-rust-windows.ps1` correctly returns `$false` and blocks execution unless intentionally bypassed.
2. **Transit & Mirror Poisoning**: In the absence of signed release manifests or Authenticode certificates, reliance on downloading both the binary and the `.sha256` file over the same channel provides no protection against DNS hijacking, CDN compromise, or malicious upstream mirror substitutions.
3. **Requirement for Out-of-Band Pinned Hashes**: Production execution must enforce an operator-vetted, out-of-band cryptographic manifest specifying exact SHA-256 hashes for toolchain releases.

---

## 4. Implementation Inventory

### 4.1 Changed Files

| File Path | Description of Changes |
|---|---|
| [`scripts/provision-rust-windows.ps1`](file:///E:/Workspace/Active/ULVO/scripts/provision-rust-windows.ps1) | Added `-Install` switch and `-ExpectedSha256` parameter; implemented Gate 3.5 (hash pinning & production execution guard); enforced dot-sourcing guard; hardened path containment and parameter allowlists. |
| [`scripts/test-provision-rust-windows.ps1`](file:///E:/Workspace/Active/ULVO/scripts/test-provision-rust-windows.ps1) | Implemented `$script:ownedFixtureRegistry` in-memory tracking; implemented `New-TestJunctionFixture` with sidecar ownership marker; hardened `New-TestFixtureDirectory` against pre-existing overwrites; hardened `Cleanup-TestFixture` to require ownership validation before reparse-point unlinking; updated T10 to clean junctions via `Cleanup-TestFixture`; updated T11 name and claims to ADS rejection; strengthened T18 with deterministic SHA-256 snapshot comparisons; updated T19 to test unowned junction rejection and eliminated all direct `Remove-Item` bypasses. |
| [`.gitignore`](file:///E:/Workspace/Active/ULVO/.gitignore) | Added `/tests/tmp/` to exclude all disposable test fixtures from Git tracking. |
| [`scripts/PROVISION-RUST-WINDOWS.md`](file:///E:/Workspace/Active/ULVO/scripts/PROVISION-RUST-WINDOWS.md) | Documented Stage 2.3 parameters (`-Install`, `-ExpectedSha256`), execution gates, updated test suite mappings, upstream provenance findings, and NO-GO approval gate recommendation. |
| [`docs/evidence/stage2-3-rust-windows/test-results.txt`](file:///E:/Workspace/Active/ULVO/docs/evidence/stage2-3-rust-windows/test-results.txt) | Captured raw regression test execution evidence showing 21 passing tests under Windows PowerShell 5.1. |

---

## 5. Verification Evidence

### 5.1 Regression Suite Execution
- **Command**: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts\test-provision-rust-windows.ps1`
- **Exit Code**: `0`
- **Summary**: `21 PASSED, 0 FAILED, 0 NOT RUN (Total: 21)`

```text
==========================================================
ULVO Stage 2.3 Windows Rust Provisioning Security Tests
Session ID   : e69725c5ebea406ebcf4d52f4911d1e0
Fixture Root : E:\Workspace\Active\ULVO\tests\tmp\run-e69725c5ebea406ebcf4d52f4911d1e0
==========================================================
[T01] Default dry-run on an absent vault -> PASS (ExitCode: 0)
[T02] Dry-run with an existing vault and sentinel file -> PASS (ExitCode: 0)
[T03] Sentinel hash unchanged after execution -> PASS (ExitCode: 0)
[T04] External destination rejection -> PASS (ExitCode: 1)
[T05] Identical RustupHome/CargoHome rejection -> PASS (ExitCode: 1)
[T06] Nested destination rejection -> PASS (ExitCode: 1)
[T07] Sibling-prefix collision handling -> PASS (ExitCode: 0)
[T08] Parent traversal rejection -> PASS (ExitCode: 1)
[T09] Relative and drive-relative path rejection -> PASS (ExitCode: 0)
[T10] Junction and symlink rejection -> PASS (ExitCode: 1)
[T11] Alternate Data Stream (ADS) path containment rejection -> PASS (ExitCode: 1)
[T12] Missing parent handling -> PASS (ExitCode: 0)
[T13] Invalid target/version arguments -> PASS (ExitCode: 0)
[DIAG] Authenticode signature status: UnknownError
[DIAG] Status message: The form specified for the subject is not one supported or known by the specified trust provider
[T14] Installer authentication failure on inert binary -> PASS (ExitCode: 0)
[DIAG] Installing Rust 1.96.0 with target x86_64-pc-windows-gnu
[DIAG]   RUSTUP_HOME: E:\Workspace\Active\ULVO\vault\runtimes\rust\rustup
[DIAG]   CARGO_HOME : E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo
[DIAG]   Arguments  : -y --default-host x86_64-pc-windows-gnu --default-toolchain 1.96.0 --profile minimal --no-modify-path -q
[T15] Nonzero installer exit handling -> PASS (ExitCode: 0)
[DIAG] Installing Rust 1.96.0 with target x86_64-pc-windows-gnu
[DIAG]   RUSTUP_HOME: E:\Workspace\Active\ULVO\vault\runtimes\rust\rustup
[DIAG]   CARGO_HOME : E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo
[DIAG]   Arguments  : -y --default-host x86_64-pc-windows-gnu --default-toolchain 1.96.0 --profile minimal --no-modify-path -q
[T16] Environment restoration after simulated failure -> PASS (ExitCode: 0)
[T17] No deletion of persistent resources -> PASS (ExitCode: 0)
[T18] Deterministic SHA-256 integrity during dry-run -> PASS (ExitCode: 0)
WARNING: Cleanup-TestFixture: Rejected deletion attempt on persistent vault: 'E:\Workspace\Active\ULVO\vault'
WARNING: Cleanup-TestFixture: Refusing deletion; missing ownership marker in 'E:\Workspace\Active\ULVO\tests\tmp\run-e69725c5ebea406ebcf4d52f4911d1e0\unmarked-test-dir'
WARNING: Cleanup-TestFixture: Refusing deletion; session ID mismatch (owner: FOREIGN-SESSION-9999, current: e69725c5ebea406ebcf4d52f4911d1e0)
WARNING: Cleanup-TestFixture: Refusing unlinking; missing junction ownership sidecar for 'E:\Workspace\Active\ULVO\tests\tmp\run-e69725c5ebea406ebcf4d52f4911d1e0\unowned-junction'
[T19] Test cleanup rejects paths not owned by current test -> PASS (ExitCode: 0)
True
[T20] PowerShell 5.1 compatibility -> PASS (ExitCode: 0)
[T21] No regressions in supported CLI behavior -> PASS (ExitCode: 0)

Cleaning up session fixtures (E:\Workspace\Active\ULVO\tests\tmp\run-e69725c5ebea406ebcf4d52f4911d1e0)...

==========================================================
Test Run Complete: 21 PASSED, 0 FAILED, 0 NOT RUN (Total: 21)
==========================================================
```

### 5.2 Verification Highlights
- **Ownership Rejection Proof**: `Cleanup-TestFixture` explicitly rejected and logged warnings for:
  - Persistent vault (`E:\Workspace\Active\ULVO\vault`)
  - Unmarked directory fixture
  - Foreign session directory fixture
  - Unowned directory junction
- **Cryptographic File Preservation Proof (T18)**: Bit-for-bit SHA-256 comparison across 33 files before and after `-DryRun` confirmed 0 added, 0 removed, and 0 modified files, with sentinel hash identical.
- **Zero Cleanup Bypasses in Source**: Code inspection of `test-provision-rust-windows.ps1` confirmed that `Remove-Item` and `[System.IO.Directory]::Delete` are called only inside `Cleanup-TestFixture` after ownership validation.

---

## 6. Formal Approval Gate Recommendation

### Verdict: **NO-GO** (Production Execution Gate)

#### Justification:
While the local script architecture, dry-run safety, path containment, and test harness ownership boundaries have been hardened and verified:
1. **Unsigned Official Binaries**: The upstream official installer (`rustup-init.exe`) is distributed without an Authenticode digital signature.
2. **Missing Cryptographic Manifest in ULVO**: ULVO does not currently maintain an out-of-band pinned digest manifest for target toolchains.
3. **Execution Block Active**: Gate 3.5 in `provision-rust-windows.ps1` correctly blocks execution without `-ExpectedSha256`, and Gate 5 fails closed on unverified binaries.

#### Prerequisites to Reach GO:
1. Define a version-pinned JSON/TOML manifest under `E:\Workspace\Active\ULVO` recording authoritative SHA-256 digests for supported targets and versions.
2. Update the provisioning workflow to pull expected digests exclusively from the local verified manifest.
3. Implement an explicit policy for handling unsigned upstream binaries with operator attestation before production installation is approved.
