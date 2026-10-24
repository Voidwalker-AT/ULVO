# ULVO v1.2.0-beta.1 — First Public Beta

**Windows 11 x64 Early Access · GitHub Pre-release**

ULVO coordinates development runtimes, toolchains, native dependency managers and
isolated project environments through a shared local vault. Thin project
connectors retain native manifests and lockfiles alongside project source.

The application consists of a Rust CLI, an optional background agent and a Tauri
desktop with embedded React assets. Seven Rust workspace crates separate types,
platform/storage adapters, discovery, shared services and these three interfaces.
The production desktop starts without a development server.

First-run CLI/GUI setup reviews the vault location, capacity, ecosystem preferences
and optional bounded metadata discovery before applying metadata. No prerequisite
download occurs during bootstrap. Ecosystem selections do not install runtimes.
Gateway/provisioning support does not certify every native package-manager or
dependency combination; preserve existing working environments and pinned locks.

Migration uses explicit review/apply plans and creates eligible thin connectors.
It preserves legacy environments, project source and native locks. Unsupported or
ML-sensitive projects may remain blocked. Connector creation does not prove
functional equivalence or authorize deleting an old toolchain/environment.

Local structured logging, bounded diagnostics, reviewed export and narrow registry
repair support diagnosis and recovery. Redaction cannot recognize every secret;
review exports before sharing. Repair requires a valid reviewed registry backup,
rechecks hashes and preserves original bytes. It does not repair arbitrary data.
The agent is opt-in; standalone CLI/desktop operation is supported without it.

## Installation and limitations

Extract the complete portable ZIP to a writable directory. Run `bin\ulvo.exe
setup status`, or launch `bin\ulvo-desktop.exe` / `ULVO Control Center.bat`.
The desktop requires Microsoft's separately installed WebView2 Evergreen Runtime.
The bundled x64 loader is not the browser runtime. CLI bootstrap does not require
WebView2, Python, Node, Rust, a GPU, an agent or a development server.

The root installer/uninstaller is **experimental** and has not passed final
packaged-entrypoint acceptance. Earlier isolated script fixtures do not qualify
these root entrypoints. Use portable startup for this beta. Independent clean
Windows scenarios, other operating systems and universal clean-machine
compatibility remain unverified. Executables are unsigned.

The executables retain their development-qualified **1.2.0** version and exact
Stage 4 bytes from `d112aac3c13c54022ba54e78488de49e019147ff`. The public
distribution name/tag is **v1.2.0-beta.1**. This remediation changes packaging,
licensing materials and documentation; it does not introduce application features
or rebuild/strip executables. Harmless compiler/source path strings remain.

## Verification and licensing

The accompanying verification matrix separates rerun portable CRC/extraction,
hashes, safe paths, documentation, secret-pattern review, CLI/GUI/agent checks and
the existing-project regression from inherited Stage 4 service/build receipts.
Host-only checks do not establish independent Windows acceptance. No new full
model/GPU research campaign is claimed.

The corrected ZIP includes ULVO's standard MIT LICENSE, third-party attribution,
the complete lockfile inventory, native notices and original checksum-verified MPL
source archives. Microsoft's exact loader was matched to its upstream NuGet
package. Third-party terms remain separate from ULVO's MIT license.

**Redistribution review remains qualified:** exact native linker provenance and
human legal sign-off are outstanding. This is an engineering review, not complete
license clearance. See `docs/BETA_REDISTRIBUTION_REVIEW.md` and
`docs/BETA_VERIFICATION.md` in the archive. Publication requires explicit human
approval after resolving the recorded blockers. No tag or release is created by
this preparation work.
