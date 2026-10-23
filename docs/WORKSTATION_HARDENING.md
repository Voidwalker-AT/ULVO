# Workstation hardening and verification limits

ULVO remains a release candidate. Acceptance must distinguish source tests, staged production binaries, installed gateways, and observed desktop behavior. Passing one does not establish the others.

## Verification

Use the installed gateway from fresh PowerShell 5.1 and PowerShell 7 sessions in different directories. Confirm `doctor`, actual tool execution, project diagnostics, and required GPU computation. A desktop check must inspect rendered production content without a development server.

Run Rust tests with the existing lockfile and bound the entire process tree with an external timeout. Run watcher tests serially. Capture exact arguments, working directory, duration, exit code, and both output streams. A timeout is a failure. Keep machine inventories and raw logs in ignored local storage; do not publish them.

`cargo test --offline --locked --workspace -- --test-threads=1` exercises the Rust workspace. The frontend's existing `npm run build` checks TypeScript and generates production assets. A release desktop build requires `tauri/custom-protocol` so it embeds those assets.

## Persistence and recovery

Project registry loading is read-only. Invalid JSON, unreadable files, and unsupported schema versions are errors; preserve the original file. Saving acquires `registry/projects.lock`, compares the on-disk snapshot with the loaded snapshot, and atomically replaces only an unchanged snapshot. A stale caller must reload and replay its operation. This locking guarantee applies to Windows; the existing non-Windows lock adapter does not provide OS locking.

For recovery, stop registry writers gracefully, retain the damaged bytes, validate a known-good backup including its schema, and restore it atomically. Retry discovery explicitly. Do not silently replace an unreadable registry with an empty one. Atomic replacement is protection against partial documents; it is not a claim that every filesystem or power-loss failure has been tested.

The agent holds a vault-specific OS lock for its lifetime. Idle and malformed clients cannot hold the synchronous IPC reader indefinitely. Newly created authentication tokens use system randomness. Existing tokens are preserved; rotating a legacy token requires stopping the agent and its clients first. Token confidentiality also depends on the vault's filesystem permissions.

Agent status counts recorded live or conservatively unknown process leases from metadata, without scanning package contents. It is not an enumeration of every unleased process on the machine. Heavy desktop storage and project reads run on blocking workers after releasing the shared service lock. Initial full-vault storage collection can still take substantial time; validate navigation during collection rather than assuming a loading indicator proves responsiveness.

## Storage measurement

For ordinary Windows files, `GetCompressedFileSizeW` returns the logical file length, so it cannot supply allocation-unit accounting. ULVO uses `GetFileInformationByHandleEx(FileStandardInfo)` for ordinary files and compressed-size queries for compressed/sparse files. Count each NTFS volume/file identity once when aggregating hardlinks. See Microsoft's [file allocation structure](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_standard_info) and [compressed size behavior](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getcompressedfilesizew).

These are file-data allocation measurements. They exclude filesystem metadata and do not establish recoverable space. Record volume free bytes before and after an approved cleanup; concurrent activity can affect that delta. Never infer reclaimed space from logical directory sizes or historical reports.

The lightweight directory-size helper deduplicates file identities and omits unavailable allocation values rather than treating logical length as physical allocation. Use the detailed CLI audit's inaccessible/incomplete indicators for acceptance evidence. Category figures can overlap through hardlinks and must not be summed to estimate unique vault storage. Volume capacity is queried from the operating system instead of a machine-specific constant.

The workspace audit applies its standard generated-directory exclusions (including nested `target`, `node_modules`, and cache directories) even when its root is the vault. Its totals describe that filtered scope. The desktop's whole-directory allocation helper uses a different scope; these totals are not directly comparable. The environment allocation ratio is logical environment bytes divided by environment allocation, not a measured global deduplication savings ratio.

CAS cleanup treats plans as untrusted input: candidates must match current registered, unreferenced CAS objects and canonical content-address paths. Reparse traversal is rejected, and a journal intent must be flushed before unlinking. The legacy `physical_bytes_reclaimed` report field represents last-link data allocation unlinked; use `volume_free_bytes_delta` for the observed volume change. Unknown allocation contributes zero to that estimate rather than falling back to logical length.

## Adoption and retirement

Discovery is read-only with respect to projects. Registration and onboarding are explicit operations. Classify fixtures and templates separately from normal managed projects. A gateway-resolvable external installation remains external.

Retain native manifests, lockfiles, shared environments with editable packages, IDE references, model caches, and active leases. Before retiring an external installation or legacy environment, verify its replacement, consumers, actual project execution, rollback, liveness, and physical benefit, then obtain approval for the exact resource. No machine PATH changes or stable release publication are implied by passing tests.

## Remaining acceptance boundaries

The default scanner is bounded by depth; nested projects may require a configured scan depth. Watcher rename handling currently retains a missing old-path entry and discovers the new path; it does not establish stable cross-volume project identity. Instruction files guide cooperating agents and do not enforce OS-wide execution policy. Cold GPU materialization, full research campaigns, destructive cleanup recovery, and external IDE consumer compatibility require separate evidence.
