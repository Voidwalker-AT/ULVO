# ULVO Phase 0

A Windows-first, read-only toolchain inventory and project-inspection foundation.

**Status: partial Phase 0 implementation. Linux/WSL build and functional tests pass; native Windows validation and multi-distribution WSL discovery are incomplete.**

## Build in the existing Ubuntu distribution

From `/mnt/e/Workspace/Active/ULVO`:

```sh
export CARGO_HOME="$PWD/.cargo-local"
export CARGO_TARGET_DIR="$PWD/target-local"
export RUSTC=/home/anmol/.cargo/bin/rustc
export RUSTDOC=/home/anmol/.cargo/bin/rustdoc
/home/anmol/.cargo/bin/cargo test --workspace --locked
/home/anmol/.cargo/bin/cargo build --workspace --locked
/usr/bin/python3 -B tests/validate.py
```

Before writes, confirm the physical repository path and that output ancestors contain no symlinks or Windows reparse points. The local Cargo home and target directory are Git-ignored. Existing Rust/Cargo/rustup installations are not modified. Dependencies are locked from official crates.io.

## Commands

```sh
target-local/debug/ulvo --help
target-local/debug/ulvo --version
target-local/debug/ulvo inventory --all-backends
 target-local/debug/ulvo inventory --all-backends --json
 target-local/debug/ulvo inventory --all-backends --verify --json
 target-local/debug/ulvo inspect /absolute/project/path --json
```

Without `--verify`, inventory performs no child-process execution. With `--verify`, only recognized native binaries in conservative installation roots receive fixed version arguments. Windows and MSYS2 candidates are never executed by the Linux build. `.NET` probes are disabled because first-run behavior could write global state.

## Initial ULVO manifest

`.ulvo/manifest.toml`:

```toml
schema_version = 1
name = "example"
[runtimes]
python = "3.12"
node = "22"
```

These are requests, not resolved dependencies. Unknown fields and unsupported schema versions are errors. No scripts run, environments are created, or native lockfiles changed.

## Coverage

Four crates separate models, adapters, discovery, and CLI. Discovery covers a bounded PATH/known-location subset, current WSL native and rustup toolchains, Windows installation candidates, MSYS2 environment candidates, and selected strict-JSON VS Code configuration references. Architecture is derived from ELF/PE headers when recognized; vendor and ownership remain unknown unless evidence supports them.

See `docs/PHASE0_IMPLEMENTATION_REPORT.md`, `docs/PHASE0_VALIDATION.md`, and `docs/SECURITY.md`. This binary is Linux ELF, **not ulvo.exe**. `--all-backends` does not imply complete enumeration or usable foreign executables.

## Windows Native Provisioning

ULVO uses a single-root architecture. All managed runtimes, toolchains, and resources reside under `E:\Workspace\Active\ULVO\`:

```
E:\Workspace\Active\ULVO\
├── bin/                      # ULVO orchestrator
├── vault/                    # Managed resources
│   ├── runtimes/             # Language runtimes (Rust, Python, etc.)
│   │   └── rust/            # Windows Rust toolchain
│   │       ├── rustup/       # RUSTUP_HOME
│   │       └── cargo/        # CARGO_HOME
│   ├── toolchains/           # External toolchain references
│   ├── packages/            # Package stores
│   ├── environments/         # Temporary project environments
│   ├── registry/            # Resource metadata
│   └── tmp/                 # Disposable work files
├── crates/                   # Rust implementation
├── scripts/                  # Provisioning and validation
└── docs/                     # Architecture and evidence
```

**Provisioning Script**: `scripts/provision-rust-windows.ps1`

- **Dry-run mode**: `-DryRun` — Validates paths and free space without downloads or installations
- **Usage**: See script help (`Get-Help scripts\provision-rust-windows.ps1 -Detailed`)
- **Safety**: No global PATH, registry, or ACL modifications

**Note**: Existing `C:\msys64` and WSL Rust installations are preserved and treated as external bootstrap dependencies until ULVO-managed alternatives are validated.
