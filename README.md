# ULVO v1.2.0-beta.1

**Windows 11 x64 Early Access · GitHub Pre-release candidate**

ULVO coordinates development runtimes, toolchains, native package managers and
project environments through a shared local vault. Projects retain their source,
native manifests and lockfiles; a thin `.ulvo/manifest.toml` connector records their
relationship to ULVO. The aim is to reuse resources while preserving project
isolation and existing working environments.

This is an early beta with host-qualified portable binaries. Independent clean
Windows acceptance remains outstanding. Windows 10, Linux, macOS, universal
clean-machine compatibility and every package-manager workflow are not qualified
by this release. The root installer is **experimental**.

## Start with the portable package

1. Extract the entire ZIP into a writable directory. Keep its files together.
2. Run `bin\ulvo.exe --version`, then `bin\ulvo.exe setup status`.
3. Open `ULVO Control Center.bat` or `bin\ulvo-desktop.exe` for the desktop wizard.

The GUI requires **Microsoft Edge WebView2 Evergreen Runtime** already installed.
The supplied x64 `WebView2Loader.dll` locates that runtime; it does not contain or
install it. Obtain the runtime from [Microsoft](https://developer.microsoft.com/en-us/microsoft-edge/webview2/).
CLI setup and diagnostics do not require WebView2, Python, Node, Rust, an agent,
a GPU or a development server. Bootstrap does not download prerequisites.

Use [installation instructions](docs/INSTALLATION.md),
[privacy and diagnostics](docs/PRIVACY.md), and
[troubleshooting and recovery](docs/TROUBLESHOOTING.md).
Review the [release notes](docs/BETA_RELEASE_NOTES.md),
[verification matrix](docs/BETA_VERIFICATION.md), and
[redistribution review](docs/BETA_REDISTRIBUTION_REVIEW.md) before distributing.

## First run and migration

The GUI and CLI share path resolution, setup planning, consent and metadata
services. Setup reviews the vault location, optional bounded discovery, capacity,
ecosystem preferences and migration limitations before apply. Discovery is opt-in
and metadata-only. Selecting an ecosystem does not install its runtime.

```powershell
.\bin\ulvo.exe --help
.\bin\ulvo.exe setup --help
.\bin\ulvo.exe doctor --deep --json
```

For an isolated installation, use absolute `--config-root` and `--vault-root`
options, or process-local `ULVO_CONFIG_ROOT` and `ULVO_VAULT_ROOT`. Configuration
precedence is described in the installation guide. Moving the portable application
directory does not move an explicitly configured vault.

Migration review is a plan/apply operation. Eligible projects receive a connector;
their legacy environments, source, native manifests and locks are retained.
Unsupported or ML-sensitive projects can remain blocked. A connector receipt is
not functional equivalence and does not authorize removing an old environment.
Review real project execution, consumers, ABI/GPU compatibility and rollback
before considering any retirement.

## Runtime management and architecture

The Rust workspace contains seven crates: `ulvo-core` (types and contracts),
`ulvo-adapters` (storage and platform access), `ulvo-discovery` (local inventory),
`ulvo-services` (orchestration and safety checks), `ulvo-cli`, `ulvo-agent`, and
`ulvo-desktop`. The desktop uses Tauri with embedded React frontend assets and
Windows WebView2. It starts without Vite or another development server.

ULVO coordinates supported Python, Node, Rust, C/C++, Go, JVM and Ruby workflows
using native ecosystem tools where available. Tool discovery, gateway execution,
managed provisioning and dependency compatibility are different checks: the
presence of a tool does not qualify every workflow. Preserve pinned versions and
existing environments. The portable ZIP contains the ULVO applications and loader,
not these language runtimes, package caches, GPU drivers or user environments.

The background agent is optional and opt-in. The CLI and desktop can use local
services without a running agent. Use `agent --help` before managing an agent;
select the intended configuration/vault so an existing installation is preserved.

## Diagnostics, recovery and limits

Local structured events support listing, inspection and reviewed export.
Credential-marker redaction is bounded and cannot identify every secret; inspect
diagnostics before sharing. No automatic telemetry upload is implemented.
Registry repair accepts a reviewed valid backup with stale-plan protection; it
does not repair arbitrary project data or automatically reinstall runtimes.

The installation scripts expose optional per-user PATH, shortcuts and registry
integration. Their packaged root entrypoints remain experimental pending acceptance.
Use the portable entrypoints for this beta. Keep backups and the previous package;
never delete a vault to resolve an installation problem.

Executables report application version **1.2.0**; the proposed public tag is
**v1.2.0-beta.1**. The desktop was rebuilt for the narrowly scoped Adoption repair.
CLI, agent and WebView2 loader bytes remain from Stage 4 source
`d112aac3c13c54022ba54e78488de49e019147ff`. The build manifest records per-binary
source provenance. Executables remain unsigned. Harmless compiler paths remain.

Adoption always displays a disabled, loading, empty, results or error state.
Discovery remains opt-in. Its explicit permission control persists separately
from the approved setup plan and does not overwrite other preferences. Previously
registered tools remain available through Installed. Late replies are ignored
after navigation or permission changes; unavailable IPC offers an explicit retry.

## License

ULVO's own source is distributed under the standard [MIT License](LICENSE),
Copyright (c) 2026 Voidwalker-AT. Third-party components retain their own terms;
MIT does not replace those terms. See [third-party notices](legal/THIRD-PARTY-NOTICES.txt),
the [dependency inventory](legal/DEPENDENCIES.json) and the redistribution review.
MPL source archives are included with the distribution. This evidence review is
not a legal opinion or a claim of complete license clearance.
