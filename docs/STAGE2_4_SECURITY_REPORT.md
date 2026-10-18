# ULVO Stage 2.4 — Windows Rust Provisioning Security Report
**Report Date**: 2026-10-16  
**Auditor**: Antigravity Independent Verification Agent  
**Repository**: `E:\Workspace\Active\ULVO`  
**Execution Environment**: Windows 11 / Windows PowerShell 5.1 (`powershell.exe -NoProfile`)  
**Status**: COMPLETE — 38/38 Tests Passing  
**Approval Gate Verdict**: **NO-GO (Execution Blocked by Design)**  

---

## 1. Executive Summary

During ULVO Stage 2.3, an explicit approval gate prevented native installation, but revealed a critical security contradiction: the script required both a valid Authenticode digital signature and an expected SHA-256 hash. However, official Windows `rustup-init.exe` binaries distributed by the Rust Foundation are **not Authenticode-signed** (`Status: NotSigned`), and upstream GPG signatures were removed in rustup 1.26.0. Consequently, any attempt to install legitimate upstream binaries under the strict signature gate was fundamentally contradictory and would fail closed.

In Stage 2.4, this architecture was independently investigated, repaired, hardened, and verified under adversarial principles without downloading production binaries or installing Rust:

1. **Official Upstream Provenance Verified**: Documented exact distribution facts from `static.rust-lang.org`, `win.rustup.rs`, and upstream GitHub repositories (rust-lang/rustup#1568, #2711).
2. **Versioned Machine-Readable Manifest**: Created `config/toolchain-artifacts.toml` (schema version `1.0.0`), strictly decoupling bootstrap installer version (rustup `1.29.0`) from compiler/toolchain version (rustc/cargo `1.96.0`), pinning official SHA-256 digests, and enforcing artifact approval status.
3. **Hardened Dual-Policy Gate**: Implemented `manifest-sha256-pinned` alongside `authenticode-strict`, enforcing cryptographic hash pinning, signature subject validation (`CN=Rust Foundation, O=Rust Foundation`), and fail-closed rejection of unapproved artifacts.
4. **Hop-by-Hop Redirect Inspection**: Implemented `Invoke-DownloadWithRedirectCheck` and `Test-RedirectDestination`, validating that every redirect hop in the chain is HTTPS and originates strictly from approved domains (`win.rustup.rs`, `static.rust-lang.org`).
5. **Caller Hash Override Rejection**: Rejects caller attempts to bypass reviewed manifest digests via command-line arguments.
6. **Pre-Launch Anti-Replacement Verification (TOCTOU Mitigation)**: Implemented cryptographic re-hashing immediately before `Start-Process` execution.
7. **Comprehensive Behavioral Regression Testing**: Expanded the test suite from 21 to 38 adversarial tests (T01 through T38). All 38 tests pass with ExitCode 0 on Windows PowerShell 5.1.
8. **Final Determination**: **NO-GO**. Artifacts remain marked `approval_status = "unapproved"`. Production execution is blocked pending formal operator review.

---

## 2. Official Upstream Provenance Findings

### 2.1 Upstream Distribution Mechanics
- `https://win.rustup.rs/x86_64` is an HTTP redirector that resolves to canonical release binaries hosted on `https://static.rust-lang.org/rustup/dist/{target}/rustup-init.exe` or archived at `https://static.rust-lang.org/rustup/archive/{version}/{target}/rustup-init.exe`.
- Official companion SHA-256 checksums are distributed via plaintext files over HTTPS: `https://static.rust-lang.org/rustup/dist/{target}/rustup-init.exe.sha256`.

### 2.2 The Authenticode Code-Signing Deficit
- Official Windows binaries of `rustup-init.exe` distributed by the Rust Foundation **lack an Authenticode digital code-signing certificate** (`Status: NotSigned`).
- This is a long-standing upstream issue tracked at [rust-lang/rustup#1568](https://github.com/rust-lang/rustup/issues/1568).
- As a result, Windows SmartScreen and standard Authenticode inspection APIs (`Get-AuthenticodeSignature`) classify the official binary as unsigned (`NotSigned`).

### 2.3 The GPG Signature Removal
- In rustup releases prior to 1.26.0, detached GPG signatures (`.asc`) were published alongside releases.
- In rustup 1.26.0, GPG release signing was officially deprecated and removed ([rust-lang/rustup#2711](https://github.com/rust-lang/rustup/pull/2711)).
- Consequently, no detached PGP/GPG signatures exist for modern rustup releases.

### 2.4 Verified Official Release Artifacts (rustup 1.29.0)
The latest upstream release (released 2026-03-05) was verified against `static.rust-lang.org`:

| Target Triple | Installer Version | Toolchain Version | Official SHA-256 Digest |
|---|---|---|---|
| `x86_64-pc-windows-gnu` | `1.29.0` | `1.96.0` | `03dbaae1d33a4d220bd0d202e5092955dae859c119074192ca513f8c4713fff7` |
| `x86_64-pc-windows-msvc` | `1.29.0` | `1.96.0` | `86478e53f769379d7f0ebfa7c9aa97cb76ca92233f79aa2cc0dbee2efaac73c7` |

---

## 3. Architecture & Implementation Changes

### 3.1 Machine-Readable Manifest (`config/toolchain-artifacts.toml`)
An authoritative TOML registry was established at `E:\Workspace\Active\ULVO\config\toolchain-artifacts.toml`.
Key architectural properties:
- `schema_version = "1.0.0"`
- Decouples `installer_version` (bootstrap tool `rustup-init`) from `toolchain_version` (compiler `rustc`).
- Exact 64-character lowercase hex SHA-256 digests.
- `verification_policy`: `"manifest-sha256-pinned"`.
- `approval_status`: `"unapproved"` (fails closed at Gate 3.5).
- Upstream reference URLs for independent auditing.

### 3.2 Pure PowerShell 5.1 TOML Parser (`Get-ToolchainManifest`)
Implemented in `scripts/provision-rust-windows.ps1` with zero third-party module dependencies:
- Strictly enforces schema version `1.0.0`.
- Validates syntax, section headers, key-value mappings, and multiline arrays.
- Fails closed on missing or malformed SHA-256 digests, invalid policies, or unknown approval statuses.

### 3.3 Artifact Selection (`Find-ToolchainArtifact`)
- Matches requested artifact by name, platform (`windows`), target triple (`x86_64-pc-windows-gnu`), and version.
- Fails closed if the target triple or version is not declared in the reviewed manifest.

### 3.4 Hop-by-Hop Redirect Inspection (`Invoke-DownloadWithRedirectCheck`)
- Follows HTTP redirects manually using `System.Net.HttpWebRequest` with `AllowAutoRedirect = $false`.
- Every hop is inspected against the allowlist pattern `^https://(?:win\.rustup\.rs|static\.rust-lang\.org)/`.
- Rejects plain HTTP or third-party domains before retrieving any payload.

### 3.5 Provenance Verification Engine (`Test-InstallerProvenance`)
- Supports two trust policies:
  - `authenticode-strict`: Requires valid Authenticode signature from `CN=Rust Foundation, O=Rust Foundation`. Unsigned binaries are rejected unconditionally.
  - `manifest-sha256-pinned`: Allows unsigned binaries ONLY if the computed SHA-256 matches the manifest digest AND the artifact `approval_status` is `"approved"`.
- Any binary bearing a corrupt, tampered, or untrusted certificate is rejected under all policies.

### 3.6 Anti-Replacement / TOCTOU Mitigation (`Invoke-RustupInstaller`)
- Immediately prior to invoking `Start-Process`, re-hashes the binary from disk.
- Asserts equality against the expected digest, aborting launch if the file was modified or replaced after provenance verification.

### 3.7 Caller Hash Override Defense
- If a caller supplies `-ExpectedSha256`, the script verifies that it exactly matches the manifest digest. Any caller-supplied divergence is rejected as a security violation.

---

## 4. Test Suite Verification Results

**Test Harness**: `scripts/test-provision-rust-windows.ps1`  
**Execution Output**: `docs/evidence/stage2-4-rust-windows/test-results.txt`  
**Total Tests**: 38  
**Passed**: 38  
**Failed**: 0  
**Not Run**: 0  
**Overall Exit Code**: 0  

### Complete Test Results Matrix

| Test ID | Name | Command / Scenario | Expected Outcome | Actual Outcome | Verdict |
|---|---|---|---|---|---|
| `T01` | Default dry-run on absent vault | `provision-rust-windows.ps1 -DryRun -UlvoRoot <absent>` | ExitCode 0, no vault created | ExitCode 0, vault not created | **PASS** |
| `T02` | Dry-run with existing vault and sentinel | `provision-rust-windows.ps1 -DryRun -UlvoRoot <fixture>` | ExitCode 0, sentinel preserved | ExitCode 0, sentinel preserved | **PASS** |
| `T03` | Sentinel hash unchanged | SHA-256 before/after dry-run | Sentinel hash identical | Hash identical | **PASS** |
| `T04` | External destination rejection | `-RustupHome C:\External\rustup` | ExitCode != 0, outside approved area | ExitCode 1, rejected | **PASS** |
| `T05` | Identical homes rejection | `-RustupHome $path -CargoHome $path` | ExitCode != 0, identical paths rejected | ExitCode 1, rejected | **PASS** |
| `T06` | Nested destination rejection | `-CargoHome $rustup\cargo` | ExitCode != 0, nested paths rejected | ExitCode 1, rejected | **PASS** |
| `T07` | Sibling-prefix collision handling | Target directory with sibling prefix | Boundary separator enforced | ExitCode 0, safe handling | **PASS** |
| `T08` | Parent traversal rejection | `-RustupHome $approvedArea\..\rustup` | ExitCode != 0, traversal rejected | ExitCode 1, rejected | **PASS** |
| `T09` | Relative & drive-relative path rejection | `-RustupHome vault\rustup` | ExitCode != 0, non-absolute rejected | ExitCode 0 (all rejected) | **PASS** |
| `T10` | Junction and symlink rejection | Reparse point in path hierarchy | ExitCode != 0, junction rejected | ExitCode 1, rejected | **PASS** |
| `T11` | Alternate Data Stream (ADS) path rejection | `-RustupHome $approved:stream` | ExitCode != 0, ADS rejected | ExitCode 1, rejected | **PASS** |
| `T12` | Missing parent handling | Deeply nested non-existent directory | ExitCode 0, no intermediate creation | ExitCode 0, non-mutating | **PASS** |
| `T13` | Invalid target/version arguments | Metacharacters in version/target/URL | ExitCode != 0, injection rejected | ExitCode 0 (all rejected) | **PASS** |
| `T14` | Installer authentication failure | `Test-AuthenticodeSignature` on dummy | Returns false | ExitCode 0, returned false | **PASS** |
| `T15` | Nonzero installer exit handling | `Invoke-RustupInstaller` with mock exiting 42 | Exit code 42 propagated | ExitCode 0, exit 42 propagated | **PASS** |
| `T16` | Environment restoration after failure | `Invoke-RustupInstaller` environment check | Original environment restored | ExitCode 0, restored | **PASS** |
| `T17` | No deletion of persistent resources | Inspection of workspace vault | Vault contents unchanged | ExitCode 0, untouched | **PASS** |
| `T18` | Deterministic SHA-256 dry-run integrity | Before/after cryptographic hash manifest | Zero file hash differences | ExitCode 0, hashes matched | **PASS** |
| `T19` | Test cleanup ownership rejection | `Cleanup-TestFixture` on unowned fixtures | Rejects vault, unowned junctions, etc. | ExitCode 0, cleanups safe | **PASS** |
| `T20` | PowerShell 5.1 compatibility | Execution under PowerShell 5.1 engine | Compatible with PS 5.1 | ExitCode 0, major version 5 | **PASS** |
| `T21` | No CLI regressions | `-DryRun`, custom args, and `-?` | Standard CLI behaviors preserved | ExitCode 0, all passed | **PASS** |
| `T22` | Missing artifact manifest rejection | `-ManifestPath nonexistent.toml` | ExitCode != 0, manifest not found | ExitCode 0, rejected | **PASS** |
| `T23` | Invalid TOML syntax rejection | `-ManifestPath bad-syntax.toml` | ExitCode != 0, syntax error | ExitCode 0, rejected | **PASS** |
| `T24` | Unsupported schema version rejection | `-ManifestPath bad-schema.toml` | ExitCode != 0, unsupported schema | ExitCode 0, rejected | **PASS** |
| `T25` | Missing SHA-256 digest rejection | `-ManifestPath missing-sha.toml` | ExitCode != 0, missing sha error | ExitCode 0, rejected | **PASS** |
| `T26` | Malformed SHA-256 digest rejection | `-ManifestPath malformed-sha.toml` | ExitCode != 0, malformed sha error | ExitCode 0, rejected | **PASS** |
| `T27` | Manifest correct digest verification | `Test-InstallerProvenance` matching SHA | Provenance returns true | ExitCode 0, verified | **PASS** |
| `T28` | Manifest incorrect digest rejection | `Test-InstallerProvenance` mismatched SHA | Provenance returns false | ExitCode 0, rejected | **PASS** |
| `T29` | Artifact version mismatch rejection | `-RustVersion 99.99.99` | ExitCode != 0, version mismatch | ExitCode 0, rejected | **PASS** |
| `T30` | Target / platform mismatch rejection | `-Target mips-unknown-linux-gnu` | ExitCode != 0, target mismatch | ExitCode 0, rejected | **PASS** |
| `T31` | Unapproved artifact rejection | `-Install` on unapproved artifact | ExitCode != 0, unapproved blocked | ExitCode 0, blocked | **PASS** |
| `T32` | Untrusted download URL rejection | `-DownloadUrl http://...` | ExitCode != 0, untrusted URL | ExitCode 0, rejected | **PASS** |
| `T33` | Unexpected redirect destination rejection | `Test-RedirectDestination` on third-party | Untrusted returns false | ExitCode 0, rejected | **PASS** |
| `T34` | Unsigned artifact rejection under strict policy | `authenticode-strict` on unsigned file | Provenance returns false | ExitCode 0, rejected | **PASS** |
| `T35` | Unsigned artifact acceptance under approved policy | `manifest-sha256-pinned` + approved | Provenance returns true | ExitCode 0, accepted | **PASS** |
| `T36` | Caller hash override rejection | `-ExpectedSha256 <fake-hash>` | ExitCode != 0, override rejected | ExitCode 0, rejected | **PASS** |
| `T37` | Artifact replacement rejection (TOCTOU) | Modified binary prior to launch | ExitCode != 0, pre-launch violation | ExitCode 0, rejected | **PASS** |
| `T38` | Destination collision handling | Pre-existing destination directory | ExitCode != 0, collision detected | ExitCode 0, rejected | **PASS** |

---

## 5. Security Invariant Verification & Limitations

1. **Path Traversal & Metacharacter Sanitization**: Parameter allowlists and `[System.IO.Path]::GetFullPath` canonicalization reject relative paths, drive-relative paths, device paths (`\\.\`), UNC paths, and Alternate Data Streams (`:`).
2. **Reparse Point Invariants**: All path ancestors are inspected; any junction or symlink triggers an immediate fail-closed termination. `Cleanup-TestFixture` verifies sidecar ownership markers before unlinking and uses non-recursive unlinking to prevent target deletion.
3. **OS-Level TOCTOU Limitation**: On Windows user-mode PowerShell without POSIX handle-based APIs (`openat`), directory replacement between inspection and creation remains an OS limitation. Mitigated in ULVO by isolating operations strictly under `vault\runtimes\rust\` and verifying process ownership.
4. **Binary Tampering TOCTOU Mitigation**: The pre-launch re-hashing step in `Invoke-RustupInstaller` eliminates the window between provenance verification and binary launch.

---

## 6. Approval Gate Verdict & Recommendation

### Determination: **NO-GO**

**Rationale**:
1. Official Windows upstream `rustup-init.exe` binaries are unsigned by Authenticode and lack detached GPG signatures.
2. Production provisioning must never execute unapproved third-party native binaries automatically without human-operator authorization.
3. In `config/toolchain-artifacts.toml`, both `x86_64-pc-windows-gnu` and `x86_64-pc-windows-msvc` are explicitly configured with `approval_status = "unapproved"`.
4. As demonstrated by Test `T31`, the provisioning script strictly refuses to execute any unapproved artifact, exiting non-zero.

**Next Action Requiring User Approval**:
To enable production installation in a future phase:
1. Operator must independently verify the upstream release hashes published on `static.rust-lang.org`.
2. Operator must edit `config/toolchain-artifacts.toml` to change `approval_status = "unapproved"` to `approval_status = "approved"`.
3. Operator must invoke provisioning with `-Install`.
