# ULVO Stage 2.5 Security & Toolchain Verification Report

**Date**: 2026-10-18  
**Mission**: ULVO Stage 2.5 — Establish a verified Windows-native Rust toolchain inside the single ULVO root, validate native compilation and linking readiness, and prepare the existing ULVO Rust workspace for its first Windows build.  
**Auditor / Implementation Agent**: Antigravity (Pair Programming Agent)  
**Target Repository**: `E:\Workspace\Active\ULVO`  
**Execution Environment**: Windows 11 Home Single Language (64-bit AMD64, Build 26200), Windows PowerShell 5.1 (`powershell.exe -NoProfile`)  
**Approval Gate Status**: **AUTHORIZED BY OPERATOR & EXECUTED**  
**Final Status**: COMPLETE — Toolchain Provisioned, Smoke Test Passed, Full Workspace Compiled & Tested (20/20 Unit Tests Passed, 55/55 Security Tests Passed)

---

## 1. Executive Summary

In Stage 2.5, the Windows-native Rust toolchain was successfully and verifiably established inside the single ULVO root (`E:\Workspace\Active\ULVO\vault\runtimes\rust\`) following strict GRAVITAS principles:

1. **Read-Only Preflight**: Verified host environment, NTFS filesystem, 14.34 GB free space, UCRT64 GCC 14.2.0 MinGW linker readiness, and confirmed zero host Rust executables.
2. **Existing Installer Collision Analysis**: Discovered that `vault\runtimes\rust\Downloads\rustup-init.exe` (SHA-256 `1F9C2AE6...`, 6.8 MB) was a truncated artifact from an aborted download in an earlier session (cut off at ~53% of its 12.7 MB total, missing `.rdata` strings and relocation tables).
3. **Explicit Operator Approval Gate**: Produced preflight report [`docs/STAGE2_5_PREFLIGHT_REPORT.md`](file:///E:/Workspace/Active/ULVO/docs/STAGE2_5_PREFLIGHT_REPORT.md) with verdict `READY FOR APPROVAL`. Halted execution and prompted the operator. The operator explicitly granted authorization to archive the truncated artifact, approve the GNU manifest entry, and execute installation.
4. **Non-Destructive Archiving**: Preserved the historical truncated artifact to `vault\runtimes\rust\Downloads\rustup-init.exe.truncated-1f9c2ae6.bak` (verified hash `1F9C2AE6...`).
5. **Manifest Gating**: Updated [`config/toolchain-artifacts.toml`](file:///E:/Workspace/Active/ULVO/config/toolchain-artifacts.toml) to approve `x86_64-pc-windows-gnu` while keeping `x86_64-pc-windows-msvc` strictly unapproved.
6. **Hardened Provisioning Execution**: Executed `provision-rust-windows.ps1 -Install`. The official 1.29.0 GNU installer was downloaded over HTTPS, verified against pinned SHA-256 `03dbaae1...`, re-hashed at Gate 5 immediately prior to launch, and installed `1.96.0-x86_64-pc-windows-gnu` into the vault with exit code 0.
7. **Native Smoke Test**: Compiled a disposable Rust test program for `x86_64-pc-windows-gnu` and executed it directly in PowerShell, achieving exit code 0 and verifying UCRT DLL imports (`api-ms-win-crt-*.dll`).
8. **Full Workspace Build & Test Acceptance**:
   - `cargo check --workspace --target x86_64-pc-windows-gnu`: Exit code 0.
   - `cargo test --workspace --target x86_64-pc-windows-gnu`: 20/20 tests passed across all 4 crates (`ulvo-core`, `ulvo-adapters`, `ulvo-discovery`, `ulvo-cli`).
   - `cargo build --workspace --target x86_64-pc-windows-gnu`: Built native `ulvo.exe` (`AMD64 / x64`).
   - `ulvo.exe inventory`: Executed natively, outputting discovered system tools.
9. **Regression Suite**: All 55 security tests in `test-provision-rust-windows.ps1` pass cleanly (55/55).

---

## 2. Toolchain Inventory & Technical Verification

All toolchain components were installed strictly inside the ULVO vault and verified via process-local environment variables (`RUSTUP_HOME`, `CARGO_HOME`):

| Component | Path | Version / Digest | Verification Method |
|---|---|---|---|
| `rustc.exe` | `vault\runtimes\rust\cargo\bin\rustc.exe` | `1.96.0 (ac68faa20 2026-05-25)` | `rustc --version`, `rustc -vV` |
| `cargo.exe` | `vault\runtimes\rust\cargo\bin\cargo.exe` | `1.96.0 (30a34c682 2026-05-25)` | `cargo --version` |
| `rustup.exe` | `vault\runtimes\rust\cargo\bin\rustup.exe` | `1.29.0` (Active: `1.96.0-x86_64-pc-windows-gnu`) | `rustup show` |
| `libstd` | `vault\runtimes\rust\rustup\toolchains\1.96.0-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\lib\` | `libstd-b0558c7fd7f3aef7.rlib` (8,896,094 bytes) | `rustc --print target-libdir` |
| PE Machine Type | All `.exe` binaries | `0x8664` (`IMAGE_FILE_MACHINE_AMD64`) | Direct binary PE header reader |
| GNU C Linker | `C:\msys64\ucrt64\bin\gcc.exe` | `14.2.0 (Rev3)` / GNU ld `2.42` | `gcc -v`, `ld --version` |
| Installer Binary | `vault\runtimes\rust\Downloads\rustup-init.exe` | `03dbaae1d33a4d220bd0d202e5092955dae859c119074192ca513f8c4713fff7` | `Get-FileHash -Algorithm SHA256` |
| Preserved Truncated | `vault\runtimes\rust\Downloads\rustup-init.exe.truncated-1f9c2ae6.bak` | `1F9C2AE6604AFB42862081FF3729B1E531561894B84A06C3C7BA14F6D344978D` | `Get-FileHash -Algorithm SHA256` |

---

## 3. Host Isolation & Zero Host Mutation

- **Global PATH**: Verified unmodified. In a fresh PowerShell process without local environment variables, `Get-Command rustc, cargo, rustup` returns nothing.
- **Registry & ACLs**: Zero modifications to registry hives or filesystem ACLs.
- **MSYS2 / WSL Isolation**: MSYS2 and WSL Ubuntu installations were untouched. No Linux binaries were copied into the Windows vault.

---

## 4. Minimal Native Smoke Test Evidence

A minimal disposable program was compiled directly with `rustc`:
```rust
fn main() {
    let msg = format!("ULVO-NATIVE-RUST-OK: target={}, arch={}", "x86_64-pc-windows-gnu", std::env::consts::ARCH);
    println!("{}", msg);
}
```
**Compilation Command**:
```powershell
rustc.exe --target x86_64-pc-windows-gnu -O main.rs -o smoke.exe
```
**Execution Output**:
```
ULVO-NATIVE-RUST-OK: target=x86_64-pc-windows-gnu, arch=x86_64
ExitCode: 0
```
**PE DLL Dependencies (`objdump -p smoke.exe`)**:
- `KERNEL32.dll`
- `api-ms-win-crt-environment-l1-1-0.dll`
- `api-ms-win-crt-heap-l1-1-0.dll`
- `api-ms-win-crt-math-l1-1-0.dll`
- `api-ms-win-crt-private-l1-1-0.dll`
- `api-ms-win-crt-runtime-l1-1-0.dll`
- `api-ms-win-crt-stdio-l1-1-0.dll`
- `api-ms-win-crt-string-l1-1-0.dll`
- `api-ms-win-crt-time-l1-1-0.dll`
- `ntdll.dll`

---

## 5. ULVO Workspace Windows Build & Test Acceptance

The entire ULVO Cargo workspace was compiled and verified using process-isolated `CARGO_HOME = E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo` and `CARGO_TARGET_DIR = E:\Workspace\Active\ULVO\target`:

### 5.1 Cargo Check
```
cargo check --workspace --target x86_64-pc-windows-gnu
Finished `dev` profile [unoptimized + debuginfo] target(s) in 44.84s
ExitCode: 0
```

### 5.2 Cargo Test (20/20 Passed)
```
running 4 tests
test tests::clangd_not_compiler ... ok
test tests::paths ... ok
test tests::capture_limit ... ok
test tests::missing_and_untrusted ... ok
test result: ok. 4 passed; 0 failed (ulvo_adapters)

running 2 tests
test tests::json_model ... ok
test tests::arguments ... ok
test result: ok. 2 passed; 0 failed (ulvo-cli)

running 5 tests
test tests::ownership_categories ... ok
test tests::transitive_alias_bridge ... ok
test tests::backend_distribution_and_configured_stay_separate ... ok
test tests::merge_retains_versions_status_and_capabilities ... ok
test tests::versions ... ok
test result: ok. 5 passed; 0 failed (ulvo_core)

running 9 tests
test tests::missing_project ... ok
test tests::selection_sorts_before_limit_and_deduplicates ... ok
test tests::vscode_unverified ... ok
test tests::underlying_identity_dedup ... ok
test tests::duplicate_candidates ... ok
test tests::manifest_valid ... ok
test tests::manifest_invalid ... ok
test tests::synthetic_project ... ok
test tests::invalid_project ... ok
test result: ok. 9 passed; 0 failed (ulvo_discovery)

Total: 20 passed; 0 failed; 0 ignored; 0 filtered out
ExitCode: 0
```

### 5.3 Cargo Build & CLI Native Execution
```powershell
cargo build --workspace --target x86_64-pc-windows-gnu
.\target\x86_64-pc-windows-gnu\debug\ulvo.exe --version
# Output: ulvo 0.1.0 (ExitCode: 0)

.\target\x86_64-pc-windows-gnu\debug\ulvo.exe inventory
# Output: Full machine toolchain inventory in JSON (ExitCode: 0)
```

---

## 6. Regression Security Suite Evidence (55/55 Passed)

The hardened security test harness was executed:
- **Command**: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\test-provision-rust-windows.ps1`
- **Results**: **55 PASSED, 0 FAILED, 0 NOT RUN (Total: 55)**
- **Evidence Log**: [`docs/evidence/stage2-5-rust-windows/test-results.txt`](file:///E:/Workspace/Active/ULVO/docs/evidence/stage2-5-rust-windows/test-results.txt)
- **Exit Code**: `0`

---

## 7. Epistemic Classification Matrix

| Finding / Claim | Epistemic Status | Empirical Evidence |
|---|---|---|
| Single-root filesystem containment | `[VERIFIED]` | All toolchain files located strictly in `E:\Workspace\Active\ULVO\vault\runtimes\rust\` |
| Pinned cryptographic hash match | `[VERIFIED]` | `03dbaae1...` matches official upstream companion `.sha256` and file on disk |
| Truncated artifact preserved | `[VERIFIED]` | `rustup-init.exe.truncated-1f9c2ae6.bak` preserved with exact SHA-256 `1F9C2AE6...` |
| Native Windows compilation & linking | `[VERIFIED]` | Minimal smoke test and full 4-crate workspace compiled with UCRT64 GCC |
| Full workspace test suite pass | `[VERIFIED]` | `cargo test --workspace` passed (20 passed, 0 failed, exit code 0) |
| CLI execution on Windows | `[VERIFIED]` | `ulvo.exe --version` and `ulvo.exe inventory` executed natively with exit code 0 |
| Zero host environment pollution | `[VERIFIED]` | Global PATH, registry, and ACLs confirmed untouched |
| Upstream Authenticode status | `[VERIFIED]` | Upstream binary is unsigned (`NotSigned`, rust-lang/rustup#1568); accepted under operator-approved policy |
| User-mode TOCTOU limitation | `[VERIFIED]` | Pre-launch re-hashing mitigates binary replacement; user-mode Windows concurrency window documented |

---

## 8. Conclusion

ULVO Stage 2.5 is **100% COMPLETE**. The Windows-native Rust toolchain is provisioned, verified, and operational within the ULVO single-root vault. The ULVO workspace has achieved its first successful native Windows build, test run, and CLI execution.
