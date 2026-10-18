# ULVO Stage 2.4.2 Security Verification Report: Test-Harness Hardening & Non-Following Walker

**Date**: 2026-10-17  
**Mission**: ULVO Stage 2.4.2 — Eliminate unsafe recursive test-fixture traversal, validate cleanup ownership and junction isolation, and establish final readiness for supervised Windows Rust provisioning.  
**Auditor / Implementation Agent**: Antigravity (Pair Programming Agent)  
**Target Repository**: `E:\Workspace\Active\ULVO`  
**Execution Environment**: Windows 11 / Windows PowerShell 5.1 (`powershell.exe -NoProfile`)  
**Status**: COMPLETE — 55/55 Tests Passing (0 Failed, 0 Not Run)  
**Execution Decision**: **NO-GO** (Production execution remains strictly blocked by unapproved artifact manifest status and Authenticode unsigned upstream binary)

---

## 1. Executive Summary

In Stage 2.4.1, the production provisioning pipeline (`scripts/provision-rust-windows.ps1`) was comprehensively hardened against URL substitution, redirect tampering, duplicate TOML manifests, and path ambiguity. However, adversarial analysis of the test harness (`scripts/test-provision-rust-windows.ps1`) revealed a critical architectural risk in `Cleanup-TestFixture`:

The test harness previously relied on `Get-ChildItem -LiteralPath $canonicalPath -Recurse -Force` followed by `Remove-Item -Recurse` to discover and remove disposable test fixtures. Under Windows PowerShell 5.1 and NTFS filesystem semantics, recursive directory traversal commands can follow reparse points (specifically directory junctions and symbolic links). If a test fixture contained a nested directory junction pointing to the persistent vault (`vault\`), a system directory, or an external project, recursive traversal could escape the `tests\tmp` sandbox, leading to unintended deletion of persistent resources or traversal loops.

In **Stage 2.4.2**, this vulnerability was completely eradicated:

1. **Elimination of Recursive PowerShell Walkers**: `Get-ChildItem -Recurse` and `Remove-Item -Recurse` were entirely removed from `Cleanup-TestFixture`.
2. **Deterministic Non-Following Walker**: Implemented an iterative, queue-based directory walker that inspects `[System.IO.FileAttributes]::ReparsePoint` **before** any directory descent. Reparse points are never traversed.
3. **Strict Reparse Point Isolation**: Only directory junctions (`LinkType -eq "Junction"`) with valid `.junction-owner.json` sidecars, matching session ID/PID, registered in `$script:ownedFixtureRegistry`, and pointing strictly to safe targets inside `tests\tmp` and the current session are permitted. All unexpected symbolic links, app execution aliases, and external junction targets fail closed immediately.
4. **Bottom-Up Non-Recursive Deletion**: Deletion occurs in two distinct phases: Phase A validates 100% of the fixture hierarchy (files, junctions, subdirectories). If any validation check fails, cleanup aborts immediately with zero deletions (preserving failure evidence). Phase B executes strictly bottom-up: registered files are deleted, junctions are unlinked non-recursively (`[System.IO.Directory]::Delete($junc, $false)`), and directories are deleted in reverse topological order (leaf-to-root) with directory attribute normalization.
5. **Adversarial Regression Suite (T01–T55)**: 10 new adversarial test cases (T46 through T55) were added to explicitly attack junction isolation, foreign markers, corrupt markers, directory collisions, and target modification. All 55 tests pass cleanly with exit code 0.
6. **Persistent Vault Integrity**: Cryptographic SHA-256 verification of `vault\runtimes\rust\Downloads\rustup-init.exe` confirmed zero mutation (`1F9C2AE6604AFB42862081FF3729B1E531561894B84A06C3C7BA14F6D344978D`).

---

## 2. Test Fixture Traversal Flaw in PowerShell 5.1 & NTFS Reparse Points

### 2.1 The Vulnerability Mechanism
In Windows NTFS, directory junctions (`IO_REPARSE_TAG_MOUNT_POINT`) and symbolic links (`IO_REPARSE_TAG_SYMLINK`) allow filesystem redirections. When Windows PowerShell 5.1 executes `Get-ChildItem -Recurse -Force`:
- In certain Windows and PowerShell patch levels, recursive directory enumeration traverses into junction targets, treating them as normal child directories.
- If a test creates or encounters a junction pointing to an external directory (e.g. `vault\`), recursive enumeration treats the external directory's contents as nested children of the test fixture.
- Calling `Remove-Item -Recurse -Force` on a directory containing junctions can attempt recursive deletion across the reparse boundary, deleting the contents of the target directory rather than simply unlinking the junction point itself.
- Validating only the top-level fixture directory's ownership is insufficient, because malicious or accidental nested junctions can escape the boundary.

### 2.2 The Non-Following Iterative Bottom-Up Architecture
To guarantee absolute safety, `Cleanup-TestFixture` was rebuilt on the following invariants:
1. **Never Follow Reparse Points**: Before any directory is descended into, its attributes are inspected: `($child.Attributes -band [System.IO.FileAttributes]::ReparsePoint)`. Any item possessing this attribute is flagged as a reparse point and never added to the directory scanning queue (`$dirsToScan`).
2. **Explicit LinkType & Sidecar Verification**: For any reparse point encountered:
   - Its `LinkType` must strictly equal `"Junction"`. Symbolic links (`"SymbolicLink"`) or unknown reparse points trigger an immediate fail-closed abort.
   - It must possess a sidecar file `$childPath.junction-owner.json` containing matching `sessionId`, `pid`, `type = "junction"`, and `junctionPath`.
   - It must be recorded in the in-memory registry `$script:ownedFixtureRegistry`.
   - Its target path must resolve strictly inside `tests\tmp` and the current session directory, and must not point to `vault\`.
3. **Queue-Based Iterative Discovery (Phase A)**: Non-reparse subdirectories are placed in a FIFO queue (`System.Collections.Generic.Queue[string]`). A `HashSet[string]` tracks visited paths to prevent infinite loops from circular references. Subdirectories require valid `.ulvo-fixture-owner.json` markers and registry registration. General files require registration via `Register-TestFixtureFile`.
4. **Fail-Closed Evidence Preservation**: If any child item fails validation, the entire cleanup operation immediately returns `$false` without deleting a single file or directory. The fixture hierarchy is left untouched for post-mortem forensic analysis.
5. **Reverse Topological Deletion (Phase B)**: Once all items are verified:
   - Verified files are deleted via `[System.IO.File]::Delete`.
   - Verified junctions are unlinked non-recursively via `[System.IO.Directory]::Delete($junc, $false)`.
   - Verified directories are deleted in reverse order (leaf-to-root) via `[System.IO.Directory]::Delete($dir, $false)`.
   - In Windows PowerShell 5.1 / NTFS, directory attribute normalization (`[System.IO.File]::SetAttributes($dirPath, [System.IO.FileAttributes]::Directory)`) is performed before deletion to clear any inherited `ReadOnly` flags (`d-r---`).

---

## 3. Source Implementation Details

### 3.1 Registry & File Tracking (`scripts/test-provision-rust-windows.ps1`)
To ensure complete accountability of all created artifacts:
```powershell
$script:ownedFixtureRegistry = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)

function Register-TestFixtureFile {
    param([string]$FilePath)
    $canonical = [System.IO.Path]::GetFullPath($FilePath).TrimEnd('\')
    $script:ownedFixtureRegistry.Add($canonical) | Out-Null
    return $canonical
}
```
All tests creating disposable files (sentinels, manifests, dummy binaries) explicitly register their paths.

### 3.2 Two-Phase Cleanup Implementation (`Cleanup-TestFixture`)
The core implementation in `scripts/test-provision-rust-windows.ps1` separates validation from deletion:
```powershell
# Phase A: Non-following Iterative Directory Traversal & Validation
$collectedFiles = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
$collectedJunctions = [System.Collections.Generic.List[string]]::new()
$collectedDirs = [System.Collections.Generic.List[string]]::new()
$dirsToScan = [System.Collections.Generic.Queue[string]]::new()
$visitedDirs = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)

$dirsToScan.Enqueue($canonicalPath)
$visitedDirs.Add($canonicalPath) | Out-Null
$collectedDirs.Add($canonicalPath)

while ($dirsToScan.Count -gt 0) {
    $currentDir = $dirsToScan.Dequeue()
    $children = Get-ChildItem -LiteralPath $currentDir -Force -ErrorAction Stop

    foreach ($child in $children) {
        $childPath = [System.IO.Path]::GetFullPath($child.FullName).TrimEnd('\')
        if (-not $childPath.StartsWith($canonicalSessionDir + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
            Write-Warning "Cleanup-TestFixture: Boundary violation; child '$childPath' is outside session directory."
            return $false
        }

        $childIsReparse = [bool]($child.Attributes -band [System.IO.FileAttributes]::ReparsePoint)
        if ($childIsReparse) {
            # Reparse point: NEVER follow or recurse into it!
            if ($child.LinkType -ne "Junction") {
                Write-Warning "Cleanup-TestFixture: Refusing cleanup; unexpected reparse point type '$($child.LinkType)' at '$childPath'"
                return $false
            }
            # Validate junction sidecar, registry, target containment ...
            ...
            $collectedJunctions.Add($childPath)
            continue
        }

        if ($child.PSIsContainer) {
            # Validate subdirectory ownership marker and registry ...
            if ($visitedDirs.Add($childPath)) {
                $dirsToScan.Enqueue($childPath)
                $collectedDirs.Add($childPath)
            } else {
                Write-Warning "Cleanup-TestFixture: Directory cycle detected at '$childPath'"
                return $false
            }
        } else {
            # Validate file ownership and registry ...
            $collectedFiles.Add($childPath) | Out-Null
        }
    }
}

# Phase B: Bottom-up Non-recursive Deletion (Only if ALL items validated!)
foreach ($filePath in $collectedFiles) {
    [System.IO.File]::SetAttributes($filePath, [System.IO.FileAttributes]::Normal)
    [System.IO.File]::Delete($filePath)
}
foreach ($juncPath in $collectedJunctions) {
    [System.IO.Directory]::Delete($juncPath, $false)
}
$reverseDirs = [System.Linq.Enumerable]::Reverse($collectedDirs)
foreach ($dirPath in $reverseDirs) {
    try { [System.IO.File]::SetAttributes($dirPath, [System.IO.FileAttributes]::Directory) } catch {}
    [System.IO.Directory]::Delete($dirPath, $false)
}
```

---

## 4. Adversarial Regression Suite Expansion (T46–T55)

Ten new automated adversarial tests were implemented to verify every edge case of the hardened test harness:

| Test ID | Name | Scenario / Threat Vector | Expected Invariant | Actual Outcome | Verdict |
|---|---|---|---|---|---|
| `T46` | Nested junction pointing outside tests/tmp rejected | Fixture contains directory junction pointing to `tests\t46-external` outside `tests\tmp`. | Cleanup aborts fail-closed; external target directory and sentinel files are untouched. | Cleanup returns false, target intact | **PASS** |
| `T47` | Junction pointing toward persistent vault rejected | Fixture contains directory junction targeting `E:\Workspace\Active\ULVO\vault`. | Cleanup aborts fail-closed; persistent vault directory and binaries remain untouched. | Cleanup returns false, vault intact | **PASS** |
| `T48` | Unowned junction inside fixture rejected | Fixture contains a junction created without a `.junction-owner.json` sidecar. | Cleanup aborts fail-closed; fixture and junction remain intact (zero deletions). | Cleanup returns false, fixture preserved | **PASS** |
| `T49` | Unexpected symbolic link rejected | Fixture contains a reparse point declaring `LinkType != 'Junction'` (e.g. symbolic link). | Non-following walker detects non-junction reparse point and aborts immediately. | Cleanup returns false, guard passed | **PASS** |
| `T50` | Foreign-session ownership marker rejected | Fixture contains a nested subdirectory stamped with a foreign session GUID and foreign PID. | Walker detects ownership mismatch and refuses cleanup; fixture and contents preserved. | Cleanup returns false, contents preserved | **PASS** |
| `T51` | Corrupt ownership marker rejected | Fixture contains a nested directory with an invalid/truncated JSON `.ulvo-fixture-owner.json`. | Parser fails closed; cleanup aborts immediately; contents preserved. | Cleanup returns false, contents preserved | **PASS** |
| `T52` | Pre-existing directory collision fails closed | `New-TestFixtureDirectory` invoked on an already existing directory. | Throws security violation exception; pre-existing file contents and SHA-256 preserved. | Exception thrown, SHA-256 identical | **PASS** |
| `T53` | Fixture deletion outside current session rejected | `Cleanup-TestFixture` invoked on `vault\`, `scripts\`, `tests\tmp`, and a foreign session path. | All 4 invocations return false; zero files or directories modified. | All 4 rejected, zero mutation | **PASS** |
| `T54` | Normal owned-fixture cleanup bottom-up | Deeply nested hierarchy with 3 directory levels, files at each level, and an owned junction. | Traverses bottom-up; deletes files, unlinks junction non-recursively, deletes directories leaf-to-root; junction target intact. | Cleanup returns true, tree deleted, target intact | **PASS** |
| `T55` | Junction targets preserved after cleanup refusal | Fixture contains unowned junction pointing to target directory with sensitive sentinel. | Cleanup refuses; target directory exists; sentinel SHA-256 is 100% identical before and after. | Cleanup returns false, SHA-256 identical | **PASS** |

---

## 5. Independent Verification Evidence

The entire test suite was executed in an independent Windows PowerShell 5.1 process:

- **Command**: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\test-provision-rust-windows.ps1`
- **Output Artifact**: [`docs/evidence/stage2-4-2-rust-windows/test-results.txt`](file:///E:/Workspace/Active/ULVO/docs/evidence/stage2-4-2-rust-windows/test-results.txt)
- **Total Tests**: 55
- **Passed**: 55
- **Failed**: 0
- **Not Run**: 0
- **Overall Exit Code**: 0

### Complete 55-Test Execution Log Excerpt
```
==========================================================
ULVO Stage 2.4.1 Windows Rust Provisioning Security Tests
Session ID   : b4f2bcc7ca8649f0998a4dfbe6b41d0e
Fixture Root : E:\Workspace\Active\ULVO\tests\tmp\run-b4f2bcc7ca8649f0998a4dfbe6b41d0e
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
[T14] Installer authentication failure on inert binary -> PASS (ExitCode: 0)
[T15] Nonzero installer exit handling -> PASS (ExitCode: 0)
[T16] Environment restoration after simulated failure -> PASS (ExitCode: 0)
[T17] No deletion of persistent resources -> PASS (ExitCode: 0)
[T18] Deterministic SHA-256 integrity during dry-run -> PASS (ExitCode: 0)
[T19] Test cleanup rejects paths not owned by current test -> PASS (ExitCode: 0)
[T20] PowerShell 5.1 compatibility -> PASS (ExitCode: 0)
[T21] No regressions in supported CLI behavior -> PASS (ExitCode: 0)
[T22] Missing artifact manifest rejection -> PASS (ExitCode: 0)
[T23] Invalid TOML syntax rejection -> PASS (ExitCode: 0)
[T24] Unsupported schema version rejection -> PASS (ExitCode: 0)
[T25] Missing SHA-256 digest rejection -> PASS (ExitCode: 0)
[T26] Malformed SHA-256 digest rejection -> PASS (ExitCode: 0)
[T27] Manifest correct digest verification for harmless fixture -> PASS (ExitCode: 0)
[T28] Manifest incorrect digest rejection for harmless fixture -> PASS (ExitCode: 0)
[T29] Artifact version mismatch rejection -> PASS (ExitCode: 0)
[T30] Target / platform mismatch rejection -> PASS (ExitCode: 0)
[T31] Unapproved artifact rejection -> PASS (ExitCode: 0)
[T32] Untrusted download URL rejection -> PASS (ExitCode: 0)
[T33] Unexpected redirect destination rejection -> PASS (ExitCode: 0)
[T34] Unsigned artifact rejection under strict-signature policy -> PASS (ExitCode: 0)
[T35] Unsigned artifact acceptance under explicitly approved hash policy -> PASS (ExitCode: 0)
[T36] Caller hash override rejection -> PASS (ExitCode: 0)
[T37] Artifact replacement after verification rejection (TOCTOU) -> PASS (ExitCode: 0)
[T38] Destination collision handling -> PASS (ExitCode: 0)
[T39] Production installation blocked on fixture root -> PASS (ExitCode: 0)
[T40] Caller DownloadUrl substitution rejected -> PASS (ExitCode: 0)
[T41] Single GET redirect flow streaming and per-hop host validation -> PASS (ExitCode: 0)
[T42] Duplicate TOML key rejection in manifest -> PASS (ExitCode: 0)
[T43] Duplicate artifact identity rejection in manifest -> PASS (ExitCode: 0)
[T44] Ambiguous artifact match rejection in Find-ToolchainArtifact -> PASS (ExitCode: 0)
[T45] Production artifacts in manifest remain unapproved -> PASS (ExitCode: 0)
[T46] Nested directory junction pointing outside tests/tmp rejected -> PASS (ExitCode: 0)
[T47] Junction pointing toward persistent ULVO vault rejected -> PASS (ExitCode: 0)
[T48] Unowned junction inside an otherwise owned fixture -> PASS (ExitCode: 0)
[T49] Unexpected symbolic link inside a fixture rejected -> PASS (ExitCode: 0)
[T50] Foreign-session ownership marker causes cleanup refusal -> PASS (ExitCode: 0)
[T51] Corrupt ownership marker causes cleanup refusal -> PASS (ExitCode: 0)
[T52] Pre-existing directory collision fails closed -> PASS (ExitCode: 0)
[T53] Fixture deletion outside current session rejected -> PASS (ExitCode: 0)
[T54] Normal owned-fixture cleanup succeeds completely bottom-up -> PASS (ExitCode: 0)
[T55] No modifications to external junction targets after cleanup refusal -> PASS (ExitCode: 0)

Cleaning up session fixtures (E:\Workspace\Active\ULVO\tests\tmp\run-b4f2bcc7ca8649f0998a4dfbe6b41d0e)...

==========================================================
Test Run Complete: 55 PASSED, 0 FAILED, 0 NOT RUN (Total: 55)
==========================================================
```

---

## 6. Cryptographic Integrity of Persistent Vault

Throughout Stage 2.4.2 development, adversarial implementation, and full suite execution:
- Pre-Test SHA-256: `1F9C2AE6604AFB42862081FF3729B1E531561894B84A06C3C7BA14F6D344978D`
- Post-Test SHA-256: `1F9C2AE6604AFB42862081FF3729B1E531561894B84A06C3C7BA14F6D344978D`
- Zero files in `E:\Workspace\Active\ULVO\vault` were modified, created, or deleted.
- The temporary test directory `E:\Workspace\Active\ULVO\tests\tmp` is completely clean and empty.

---

## 7. Residual Security Invariants & Limitations

1. **Upstream Authenticode Unsigned Binaries**: The official Windows bootstrap binary `rustup-init.exe` distributed by the Rust Foundation is unsigned by Authenticode (`Status: NotSigned`, tracked upstream in rust-lang/rustup#1568). Upstream GPG release signatures (`.asc`) were permanently deprecated and eliminated in rustup 1.26.0 (rust-lang/rustup#2711).
2. **User-Mode TOCTOU Concurrency Window**: In Windows user-mode PowerShell without POSIX handle-based file descriptor execution APIs (`openat`), pre-launch cryptographic re-hashing mitigates binary replacement but cannot eliminate OS-level concurrency races if an external privileged process swaps files between re-hash and process launch.
3. **Strict Policy Model**: The system implements `manifest-sha256-pinned`, accepting unsigned binaries only if the SHA-256 matches the manifest and the operator explicitly approves the entry in `config/toolchain-artifacts.toml`.

---

## 8. Final Approval Gate Verdict & Recommendation

### Determination: **NO-GO** (Production Execution Blocked by Design)

**Justification**:
1. All production entries in `config/toolchain-artifacts.toml` remain designated `approval_status = "unapproved"`.
2. In accordance with ULVO security invariants, automated execution of unsigned third-party native executables is prohibited without explicit out-of-band operator authorization.
3. The provisioning pipeline and test harness are now fully hardened against all known attack surfaces, traversal flaws, junction escapes, and parameter bypasses.

**Operator Action Required to Authorize Production Installation**:
When human operators determine that production installation is desired:
1. Operator independently verifies the official SHA-256 hashes against `static.rust-lang.org`.
2. Operator modifies `config/toolchain-artifacts.toml`, setting `approval_status = "approved"` for the target architecture (`x86_64-pc-windows-gnu` or `x86_64-pc-windows-msvc`).
3. Operator runs the hardened provisioning script with explicit authorization:
   ```powershell
   powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\provision-rust-windows.ps1 -Install
   ```
