# Phase 0 architecture

## Layers

- `ulvo-core`: serializable deterministic models, version-token parser, backend/distribution/file-identity deduplication. No I/O or process execution.
- `ulvo-adapters`: native-header inspection, bounded input reads, traversal rejection, explicit Windows/WSL/MSYS2 classification, fixed executable/argument allowlist and bounded native-host probes.
- `ulvo-discovery`: bounded PATH/known-location inventory, current-distribution rustup discovery, VS Code references, project native-manifest detection, strict initial ULVO TOML parser.
- `ulvo-cli`: clap arguments, human/JSON output, diagnostics and exit codes.

## Domain model

Backend: windows, wsl, msys2. Ownership: external, experimental, managed, unknown. Verification: configured, present, verified, failed, unsupported. Records also carry tool, version, executable, file identity, distribution, architecture, vendor, sources, capabilities and diagnostics. Optional values remain null when unknown.

Filesystem candidates initially receive `present`, not `verified`. Fixed successful probes with recognized version output can become `verified`; this is a version-check result, not an assurance of build capability or trust. VS Code references remain configured and are never executed. Ownership discovery currently remains unknown; no E: experiment paths are scanned or assigned a managed label.

On Unix, device/inode identify underlying files, with canonical-path consolidation as fallback. Native Windows file-ID deduplication is not implemented; canonical paths alone do not consolidate NTFS hardlink aliases. Deduplication merges discovery sources; it does not currently merge every capability/verification field from differing aliases.

## Backends

Windows is the primary intended product target. Current build host is Ubuntu WSL, producing Linux ELF. The implementation distinguishes backend identities and rejects foreign execution. MSYS2 discovery is path-based; executing within an MSYS2 environment is not implemented. Windows-to-WSL launcher and cross-distribution inventory remain pending. Non-Windows hosts currently classify as WSL; generic standalone Linux is outside this prototype's supported host contract.

## Bounded discovery

At most 128 PATH entries, 128 children per explicitly selected installation/profile root, fixed candidate filenames, no recursive drive scans. Files read as configuration are limited to 1 MiB. Probe output retention is capped at 64 KiB per stream; streams are drained. Probe process wait is limited to three seconds plus capture allowance. There is no total inventory timeout or filesystem-operation timeout. Directory enumeration truncation is conservative and may omit entries; the selected subset is filesystem-order dependent.

## Inspection

Top-level known native manifest names are detected without parsing/resolving their dependencies. Only `.ulvo/manifest.toml` is parsed. Parent traversal and symlink/reparse traversal are rejected. JSON schema version is 1; semantic runtime constraint validation is not implemented.

No database, background service, environment provisioning, installation, garbage collection, or MCP implementation is included.
