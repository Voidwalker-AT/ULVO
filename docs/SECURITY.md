# ULVO Security & Safety Specification

This document details the security boundaries, execution invariants, filesystem safety rules, and threat model governing ULVO.

---

## 1. Core Invariants

ULVO operates under strict safety and isolation guarantees:

1. **Read-Only by Default**: Discovery, inspection, registry queries, and workspace auditing never mutate project files, delete environments, or alter system state.
2. **Zero Global Mutation**: ULVO never modifies global system `PATH`, Windows registry keys, system file permissions, or shared user profiles.
3. **Process-Local Redirection**: Runtimes, environment variables (`CARGO_HOME`, `RUSTUP_HOME`, `VIRTUAL_ENV`), and execution contexts are scoped strictly to the child process.
4. **Reparse Point & Symlink Containment**: Filesystem traversal routines explicitly reject symlinks, NTFS directory junctions, and reparse points to prevent path traversal and infinite recursion loops.
5. **Ownership-Gated Cleanup**: Any cleanup or removal routine is restricted to explicitly owned resources bearing verified ownership sentinels.

---

## 2. Windows-Native Filesystem Safety

### Lock-Free NTFS Handle Queries
When inspecting files across development workspaces, ULVO avoids taking locks that could cause file contention with running editors or compilers:
- Windows API calls use `CreateFileW` with `FILE_READ_ATTRIBUTES` access only.
- Handles are shared with `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`.
- Flags include `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT` to safely query directories and reparse points without traversing them.
- File identities are extracted using `GetFileInformationByHandle`, capturing `(dwVolumeSerialNumber, nFileIndexHigh:Low)` to track hardlinks and physical allocations accurately.
- Physical cluster allocations are measured via `GetCompressedFileSizeW`.

### Junction and Reparse Point Isolation
- During recursive directory walking (`workspace_audit.rs`, `vault_python.rs`), every entry is tested for reparse point attributes (`FILE_ATTRIBUTE_REPARSE_POINT`).
- Directories with reparse point attributes are skipped immediately.
- If a workspace root or fixture itself is a reparse point, the operation is aborted before walking.

### Process-Level Fixture Ownership Guards
All synthetic test fixtures implement strict ownership guards:
- Test directories are generated under the system temp path using process ID and nanosecond timestamps: `ulvo_test_<name>_<pid>_<nanos>`.
- Prior to running tests, an ownership sentinel file `.ulvo_test_sentinel` is written containing the current process PID and timestamp.
- On `Drop`, the fixture verifies that `.ulvo_test_sentinel` exists and contains the matching process PID before executing recursive deletion. If the sentinel is missing or altered, cleanup is aborted.

---

## 3. Subprocess Execution & Probe Containment

Toolchain discovery includes an opt-in `--verify` mode to validate compiler and runtime binaries.

### Probe Sandbox Limits
1. **Null Stdin**: Subprocess stdin is redirected to `std::process::Stdio::null()`, preventing interactive hangs or prompt exploitation.
2. **Concurrent Stream Draining**: Subprocess `stdout` and `stderr` are drained asynchronously with a strict 64 KiB capture limit per stream to avoid memory exhaustion from runaway logging.
3. **Hard 3-Second Timeout**: Every probe is monitored with a bounded timer. If the child process does not exit within 3 seconds, it is forcibly terminated.
4. **Conservative Argument Allowlist**: Binaries only receive fixed, hardcoded version inspection flags (e.g. `--version`). User-supplied arguments or project-controlled scripts are never passed.
5. **Conservative Directory Trust**: Only binaries situated in recognized installation roots (e.g. `vault/`, `C:\Program Files`, active toolchain directories) are eligible for probing.
6. **Cross-Platform Rejection**: Windows binaries are never executed from Linux/WSL builds, and ELF binaries are never executed on Windows.

---

## 4. Threat Model & Residual Limitations

ULVO adheres to the GRAVITAS engineering standard of disclosing residual risks honestly:

### Time-of-Check to Time-of-Use (TOCTOU)
Filesystem checks for reparse points and permissions mitigate but do not completely eliminate race conditions if another process concurrently replaces a directory with a junction. True kernel-level race prevention requires transaction-locked handles or Windows Filter Drivers, which are outside the scope of user-space CLI tooling.

### Distribution Catalog Digest vs. Full In-Situ Payload Hashing
When inspecting Python virtual environments:
- ULVO hashes `.dist-info/RECORD` or `METADATA` to verify package distribution catalog identity.
- This verifies that two packages originate from identical distribution manifests without incurring hundreds of megabytes of disk I/O per virtual environment during discovery.
- **Limitation**: If a developer manually edits a `.py` file inside `site-packages` without updating `RECORD`, the distribution manifest hash will remain unchanged. Full in-situ payload re-verification is reserved for deep verification passes.

### Hardlink Deduplication Accounting
If an existing environment manager (e.g. `uv`) installed packages using hardlinks from a central cache, physical storage clusters on disk are already shared (`link_count > 1`). ULVO identifies shared filesystem identities during workspace scans; however, cross-environment package duplication estimates compute logical duplicated bytes assuming independent copies. Reclaimable physical storage will be lower if hardlinking is already present.

### Inaccessible Directory Reporting
If directory entries are inaccessible due to Windows file permissions (e.g. `Administrator`-only directories or locked system files), ULVO increments an `inaccessible_count` accumulator and marks the scan status as `INCOMPLETE (<N> inaccessible entries)` rather than silently ignoring errors.
