# ULVO Windows Rust Provisioning — Hardened Architecture (Stage 2.4.2)

## 1. Overview

This document describes the hardened provisioning system for a Windows-native Rust toolchain under ULVO's single-root architecture.

- **Provisioning Script**: `scripts/provision-rust-windows.ps1`  
- **Test Suite**: `scripts/test-provision-rust-windows.ps1`  
- **Artifact Manifest**: `config/toolchain-artifacts.toml`  
- **Evidence Log**: `docs/evidence/stage2-4-2-rust-windows/test-results.txt`  

All ULVO-managed Rust resources reside strictly within the approved Rust vault area:

```
E:\Workspace\Active\ULVO\
├── config\
│   └── toolchain-artifacts.toml   # Authoritative manifest of reviewed toolchain artifacts
└── vault\
    └── runtimes\
        └── rust\
            ├── rustup\            # RUSTUP_HOME (toolchains, update manifests)
            ├── cargo\             # CARGO_HOME (binaries: rustc.exe, cargo.exe, rustup.exe)
            └── Downloads\         # Temporary installer storage (production only)
```

## 2. Hardened Security Invariants

The provisioning system enforces the following non-negotiable security boundaries:

- **Genuinely Non-Mutating Dry-Run**: In `-DryRun` mode, the script performs ZERO filesystem writes, directory creations, file deletions, network requests, or subprocess executions.
- **Machine-Readable Artifact Manifest**: Toolchain bootstrap binaries are declared in `config/toolchain-artifacts.toml` (schema version `1.0.0`). The manifest distinguishes the bootstrap installer version (e.g. rustup `1.29.0`) from the compiler/toolchain version (e.g. rustc `1.96.0`).
- **Cryptographic Digest Pinning & Policy-Aware Provenance**:
  - Every artifact defines a lowercase 64-character hexadecimal SHA-256 digest.
  - The verification policy specifies either `manifest-sha256-pinned` or `authenticode-strict`.
  - Under `manifest-sha256-pinned`, an unsigned binary is accepted ONLY if its computed SHA-256 matches the manifest digest AND its `approval_status` is explicitly set to `approved`.
  - Under `authenticode-strict`, an unsigned binary is rejected unconditionally even if marked approved.
  - Any binary bearing an attached Authenticode signature that is corrupt, invalid, or issued by an entity other than `CN=Rust Foundation, O=Rust Foundation` is rejected immediately.
- **Caller Hash & URL Substitution Rejection**: Callers cannot bypass or override reviewed manifest digests or canonical download URLs via the command line. If `-ExpectedSha256` or `-DownloadUrl` is provided by the caller, it must match the manifest entry exactly or fail closed immediately.
- **Single Controlled Streaming GET Flow**: All download requests execute a single controlled `GET` streaming flow (`AllowAutoRedirect = $false`) with per-hop HTTPS and host validation (`win.rustup.rs`, `static.rust-lang.org`). The final HTTP 200 payload is streamed directly to disk, eliminating separate HEAD/GET divergence.
- **Production-Only Execution Gate**: The `-Install` switch is strictly prohibited when `UlvoRoot` points to a disposable test fixture under `tests\tmp`. Production installation requires the canonical production root `E:\Workspace\Active\ULVO`.
- **Manifest Ambiguity & Duplicate Rejection**: `Get-ToolchainManifest` detects and rejects duplicate root keys, duplicate artifact keys, and duplicate artifact identities `(artifact_name, platform, target, installer_version)`. `Find-ToolchainArtifact` fails closed on ambiguous duplicate matches.
- **Pre-Launch Anti-Replacement Verification (TOCTOU Mitigation)**: Immediately before `Start-Process` launches the installer executable, `Invoke-RustupInstaller` recomputes the SHA-256 digest of the binary on disk and asserts exact equality against the verified expected digest. This mitigates pre-launch file modification, though in user-mode Windows without handle-based execution APIs, it reduces but cannot eliminate OS-level concurrency races.
- **Destination Collision Defense**: Existing `RUSTUP_HOME` or `CARGO_HOME` directories are detected during path validation, preventing accidental clobbering or unauthorized modification of existing installations.
- **Strict Vault Area Containment**: Custom `RUSTUP_HOME` and `CARGO_HOME` must reside strictly inside `vault\runtimes\rust\`. Sibling directory prefixes (`rust-evil`), parent traversals (`..`), UNC paths (`\\`), device paths (`\\.\`), and Alternate Data Streams (`:`) are strictly prohibited.
- **Subprocess Isolation & Environment Restoration**: All child process executions preserve and restore `RUSTUP_HOME`, `CARGO_HOME`, and `PATH` in `try...finally` blocks.
- **Deterministic Non-Following Bottom-Up Test Fixture Cleanup**:
  - `Cleanup-TestFixture` completely eliminates `Get-ChildItem -Recurse` and `Remove-Item -Recurse` to prevent following malicious junctions or unowned symlinks outside the test sandbox.
  - Phase A (Validation & Discovery): Non-following iterative queue-based traversal detects `[System.IO.FileAttributes]::ReparsePoint` before any descent. Junctions require matching `.junction-owner.json` sidecars, registry presence, and validated targets (strictly inside `tests\tmp` and session directory, never targeting the vault). Subdirectories require `.ulvo-fixture-owner.json` markers with valid session ID/PID; cycles are caught with `HashSet[string]`. General files require registration via `Register-TestFixtureFile`.
  - Fail-Closed Abort: Any unowned, foreign-session, corrupt, unexpected reparse point, or boundary-escaping item causes cleanup to abort immediately with zero deletions, preserving failure evidence.
  - Phase B (Bottom-Up Deletion): Executes only after 100% hierarchy validation succeeds. Deletes registered files, unlinks junctions non-recursively (`[System.IO.Directory]::Delete($junc, $false)`), and removes directories in reverse topological order (leaf-to-root) with directory attribute normalization.
  - The persistent vault (`vault\`) and paths outside `tests\tmp` are categorically rejected from deletion.

## 3. Parameters

| Parameter | Type | Default | Description | Security Constraints |
|-----------|------|---------|-------------|----------------------|
| `DryRun` | Switch | `False` | Run validation only without mutation | Guaranteed non-mutating |
| `RustVersion` | String | `1.96.0` | Rust toolchain compiler version | Must match SemVer (`\d+\.\d+\.\d+`) or channel |
| `InstallerVersion` | String | `1.29.0` | Bootstrap installer version | Must match manifest installer version |
| `Target` | String | `x86_64-pc-windows-gnu` | Target triple | Must match alphanumeric triple pattern and manifest entry |
| `DownloadUrl` | String | `$null` | Installer download URL | Bound to manifest canonical_url; caller substitution rejected |
| `ManifestPath` | String | `$canonicalUlvoRoot\config\toolchain-artifacts.toml` | Manifest location | Must be valid TOML conforming to schema 1.0.0 without duplicates |
| `RustupHome` | String | `...vault\runtimes\rust\rustup` | RUSTUP_HOME directory | Must be strictly under `vault\runtimes\rust\` |
| `CargoHome` | String | `...vault\runtimes\rust\cargo` | CARGO_HOME directory | Must be strictly under `vault\runtimes\rust\` |
| `UlvoRoot` | String | `E:\Workspace\Active\ULVO` | Repository root | Restricted to workspace or `tests\tmp` fixtures |
| `Install` | Switch | `False` | Explicit authorization for production install | Required to proceed beyond DryRun; prohibited on test fixtures |
| `ExpectedSha256` | String | `$null` | 64-hex SHA-256 cryptographic digest | Must match manifest; caller overrides strictly rejected |

## 4. Execution Flow & Validation Gates

1. **Root & Argument Validation**: Validates `UlvoRoot` containment, validates parameter syntax allowlists against injection (`RustVersion`, `Target`, `DownloadUrl`), and resolves canonical paths.
2. **Manifest Loading & Selection**:
   - Parses `config/toolchain-artifacts.toml` in pure PowerShell 5.1 (`Get-ToolchainManifest`).
   - Asserts schema version `1.0.0`, rejects duplicate root keys, duplicate artifact keys, and duplicate artifact identities.
   - Selects approved artifact matching target, platform, installer version, and toolchain version (`Find-ToolchainArtifact`), rejecting ambiguous matches.
   - Binds download target URL to reviewed manifest `canonical_url` and rejects caller substitution.
   - Asserts caller-supplied `-ExpectedSha256` matches the reviewed manifest hash; rejects overrides.
3. **Gate 1 — Free Space Check**: Verifies minimum 2.0 GB free disk space on the target drive via CIM/WMI.
4. **Gate 2 — Path Validation & Containment**:
   - Canonicalizes `RustupHome` and `CargoHome`.
   - Validates absolute drive-rooted format without traversal, device paths, or alternate data streams.
   - Enforces containment within `vault\runtimes\rust\`.
   - Checks existing ancestor path components for reparse points and file/directory type conflicts, failing closed on inaccessible metadata.
   - Enforces that `RustupHome` and `CargoHome` are non-identical and non-nested.
   - Verifies destination directories do not pre-exist.
5. **Gate 3 — Dry-Run Check**: If `-DryRun` is specified, prints validation summary and exits `0`. No downloads, writes, or process executions occur.
6. **Gate 3.5 — Production Execution Guard & Artifact Approval Check**:
   - Requires explicit `-Install` switch to proceed beyond Gate 3.
   - Strictly prohibits `-Install` when `UlvoRoot` is under `tests\tmp` (fixture root).
   - Evaluates artifact `approval_status`. If `approval_status != "approved"` (e.g. `"unapproved"`), execution aborts immediately.
7. **Gate 4 — Single Controlled Streaming GET Download**:
   - Executes single GET stream with `AllowAutoRedirect = $false`.
   - Validates each hop against the HTTPS domain allowlist.
   - Streams payload directly to temporary download location.
   - Checks minimum binary size (1 MB).
8. **Gate 4.5 — Policy-Aware Provenance Verification**:
   - Computes SHA-256 of downloaded executable and asserts exact match against manifest digest.
   - Inspects Authenticode signature status and publisher subject (`CN=Rust Foundation, O=Rust Foundation`).
   - Applies policy rule: rejects unsigned binaries under `authenticode-strict`; permits unsigned binaries under `manifest-sha256-pinned` ONLY if explicitly approved; rejects invalid/corrupt signatures under all policies.
9. **Gate 5 — Pre-Launch Integrity Check & Installation**:
   - Immediately prior to launch, re-hashes the binary on disk and asserts match against expected digest.
   - Executes installer with `--no-modify-path`, `minimal` profile, and isolated environment variables.
10. **Gate 6 — Toolchain Verification**: Validates `rustc.exe`, `cargo.exe`, `rustup.exe`, version strings, default host, and target `libstd` in-memory. Restores original environment.

## 5. Automated Behavioral Test Suite

The test suite (`scripts/test-provision-rust-windows.ps1`) executes 55 independent behavioral tests:

| Test ID | Name | Focus Area | Verification Method |
|---------|------|------------|---------------------|
| `T01` | Default dry-run on absent vault | S-05 Dry-run | Executes on absent vault fixture; verifies zero directory creation |
| `T02` | Dry-run with existing vault and sentinel | S-05 Dry-run | Executes with sentinel file present; verifies sentinel preserved |
| `T03` | Sentinel hash unchanged | S-05 Dry-run | SHA-256 hash comparison before and after dry-run |
| `T04` | External destination rejection | S-02 Containment | Rejects paths outside approved `vault\runtimes\rust\` area |
| `T05` | Identical homes rejection | S-02 Containment | Rejects identical `RustupHome` and `CargoHome` |
| `T06` | Nested destination rejection | S-02 Containment | Rejects `CargoHome` nested inside `RustupHome` |
| `T07` | Sibling-prefix collision handling | S-02 Containment | Sibling directories allowed while sibling area spoofing rejected |
| `T08` | Parent traversal rejection | S-02 Containment | Rejects `..` path traversal attempts |
| `T09` | Relative & drive-relative path rejection | S-02 Containment | Rejects `vault\...` and `E:vault\...` paths |
| `T10` | Junction and symlink rejection | S-02 Reparse Points | Rejects directory junctions; unlinks strictly through `Cleanup-TestFixture` |
| `T11` | Alternate Data Stream (ADS) path rejection | S-02 Containment | Rejects Alternate Data Streams (`:`) and enforces fail-closed path validation |
| `T12` | Missing parent handling | S-02 Containment | Handles deeply nested non-existent paths without intermediate creation |
| `T13` | Invalid target/version arguments | S-04 Process Exec | Rejects metacharacters and semicolons in arguments |
| `T14` | Installer authentication failure | S-03 Authenticity | Fails closed on unsigned dummy/inert executable |
| `T15` | Nonzero installer exit handling | S-04 Process Exec | Verifies propagation of non-zero exit code (42) from mock installer |
| `T16` | Environment restoration after failure | S-04 Subprocess | Verifies `RUSTUP_HOME`, `CARGO_HOME`, and `PATH` restored after failure |
| `T17` | No deletion of persistent resources | S-01 Cleanup | Verifies workspace vault, `Cargo.toml`, and README remain intact |
| `T18` | Deterministic SHA-256 dry-run integrity | S-05 Dry-run | Cryptographic SHA-256 snapshot comparisons across bounded workspace areas |
| `T19` | Test cleanup ownership rejection | S-01 Cleanup | `Cleanup-TestFixture` rejects vault, unmarked paths, foreign sessions, and unowned junctions |
| `T20` | PowerShell 5.1 compatibility | Compatibility | Verifies execution under Windows PowerShell 5.1 runtime |
| `T21` | No CLI regressions | Compatibility | Verifies `-DryRun`, custom arguments, and `-?` help display |
| `T22` | Missing artifact manifest rejection | Manifest Security | Rejects non-existent manifest path |
| `T23` | Invalid TOML syntax rejection | Manifest Security | Fails closed on syntax-corrupted TOML manifest |
| `T24` | Unsupported schema version rejection | Manifest Security | Rejects manifest schema versions other than 1.0.0 |
| `T25` | Missing SHA-256 digest rejection | Manifest Security | Rejects artifact entries omitting `expected_sha256` |
| `T26` | Malformed SHA-256 digest rejection | Manifest Security | Rejects non-hex or invalid-length SHA-256 digests |
| `T27` | Manifest correct digest verification | Provenance | Harmless fixture with matching SHA-256 passes provenance |
| `T28` | Manifest incorrect digest rejection | Provenance | Harmless fixture with mismatched SHA-256 is rejected |
| `T29` | Artifact version mismatch rejection | Manifest Selection | Rejects versions not present in reviewed manifest |
| `T30` | Target / platform mismatch rejection | Manifest Selection | Rejects target triples not present in reviewed manifest |
| `T31` | Unapproved artifact rejection | Approval Gate | Execution blocked when artifact `approval_status = "unapproved"` |
| `T32` | Untrusted download URL rejection | Network Security | Rejects HTTP and non-whitelisted domain download URLs |
| `T33` | Unexpected redirect destination rejection | Network Security | Rejects redirects to HTTP or external domains |
| `T34` | Unsigned artifact rejection under strict policy | Policy Enforcement | `authenticode-strict` rejects unsigned binaries unconditionally |
| `T35` | Unsigned artifact acceptance under approved policy | Policy Enforcement | `manifest-sha256-pinned` accepts unsigned binary when hash matches and approved |
| `T36` | Caller hash override rejection | Anti-Bypass | Caller `-ExpectedSha256` differing from manifest is rejected |
| `T37` | Artifact replacement rejection (TOCTOU) | Pre-Launch Check | Pre-launch check in `Invoke-RustupInstaller` detects modified binary |
| `T38` | Destination collision handling | Collision Defense | Detects and aborts if `RUSTUP_HOME` already exists |
| `T39` | Production install blocked on fixture root | Execution Gate | Rejects `-Install` when `UlvoRoot` is under `tests\tmp` |
| `T40` | Caller DownloadUrl substitution rejected | URL Provenance | Fails closed if caller `-DownloadUrl` differs from manifest canonical_url |
| `T41` | Single GET redirect streaming validation | Network Security | Validates per-hop redirects and direct body streaming in single GET flow |
| `T42` | Duplicate TOML key rejection | Manifest Security | Fails closed on duplicate root or artifact keys in manifest |
| `T43` | Duplicate artifact identity rejection | Manifest Security | Fails closed on duplicate `(name, platform, target, version)` tuples |
| `T44` | Ambiguous artifact match rejection | Manifest Selection | Fails closed if multiple conflicting artifacts match criteria |
| `T45` | Production artifacts remain unapproved | Security Gate | Asserts all entries in `config/toolchain-artifacts.toml` are unapproved |
| `T46` | Nested junction pointing outside tests/tmp rejected | Cleanup Hardening | Non-following walker detects escaping junction target, aborts cleanup fail-closed, external target preserved |
| `T47` | Junction pointing toward persistent vault rejected | Vault Preservation | Fails closed on junction pointing to `vault\`, prevents recursive or non-recursive vault damage |
| `T48` | Unowned junction inside fixture rejected | Fixture Ownership | Missing sidecar token causes cleanup refusal, zero deletions execute |
| `T49` | Unexpected symbolic link rejected | Reparse Safety | Reparse points with `LinkType != 'Junction'` rejected immediately fail-closed |
| `T50` | Foreign-session ownership marker rejected | Session Boundary | Mismatched `sessionId` or `pid` causes cleanup refusal, fixture tree preserved |
| `T51` | Corrupt ownership marker rejected | Session Boundary | Invalid JSON in `.ulvo-fixture-owner.json` causes cleanup refusal |
| `T52` | Pre-existing directory collision fails closed | Collision Defense | `New-TestFixtureDirectory` throws security violation on pre-existing directory, preserving contents |
| `T53` | Fixture deletion outside current session rejected | Containment | Rejects cleanup attempts on vault, scripts, fixture root, and other session directories |
| `T54` | Normal owned-fixture cleanup bottom-up | Cleanup Execution | Deeply nested owned hierarchy with files, subdirs, and junctions cleanly unlinked and deleted leaf-to-root |
| `T55` | Junction targets preserved after cleanup refusal | Target Isolation | External junction target file SHA-256 remains 100% identical before and after cleanup abort |

### Running the Test Suite

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\test-provision-rust-windows.ps1
```

## 6. Official Provenance Findings & Stage 2.4.2 Verdict

1. **Official Windows Installer is Unsigned**: Empirical testing and upstream documentation confirm that official Windows `rustup-init.exe` binaries distributed by the Rust Foundation are **not Authenticode code-signed** (`Status: NotSigned`, tracked upstream in rust-lang/rustup#1568).
2. **GPG Signatures Deprecated Upstream**: Detached GPG signatures (`.asc`) were permanently deprecated and removed from official release distribution in rustup 1.26.0 (rust-lang/rustup#2711).
3. **Transport-Only Integrity Upstream**: Upstream publishes plaintext `.sha256` companion files over HTTPS (`static.rust-lang.org`). Without out-of-band digest pinning, authenticity relies solely on TLS transport security.
4. **ULVO Solution**: ULVO resolves this contradiction through `config/toolchain-artifacts.toml`, pinning exact release digests (`03dbaae1...` for GNU, `86478e53...` for MSVC) under the `manifest-sha256-pinned` policy.
5. **Hardened Bottom-Up Test Harness**: Test fixture cleanup is completely guarded against traversal vulnerabilities, unowned junctions, symlinks, and foreign sessions through an iterative non-following bottom-up walker and fail-closed validation.
6. **Approval Gate Recommendation**: **NO-GO**. In accordance with security-first engineering invariants, all production artifacts in `toolchain-artifacts.toml` remain designated `approval_status = "unapproved"`. Automated download and execution of native binaries remains strictly disabled until human operators explicitly review and approve production provisioning.
