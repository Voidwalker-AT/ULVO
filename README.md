# ULVO — Universal Language & Virtual Environment Orchestrator

ULVO is an early-stage, actively developed universal development-resource control plane. It provides unified governance, inspection, deterministic resource registry tracking, and non-destructive storage intelligence over established language ecosystem managers.

```
       STORE ONCE.
       ISOLATE PER PROJECT.
       MATERIALIZE ON DEMAND.
       EXECUTE THROUGH ULVO.
       RECLAIM SAFELY.
```

> **Current Development Status**: ULVO is an active engineering foundation. It is **not yet** a complete universal package manager or automatic ephemeral environment provisioner. It currently provides a native Windows CLI, a deterministic Universal Vault Registry, a Windows-native Rust toolchain inside a single-root vault, and a read-only Python/uv workspace storage intelligence and deduplication engine.

---

## 1. The Problem ULVO Solves

Modern multi-language software development suffers from resource fragmentation, environment sprawl, and storage waste:

1. **Massive Disk Duplication**: Each project often creates its own isolated virtual environment (`.venv`, `node_modules`, `target`), copying identical large binaries, native wheels (e.g. PySide6, PyTorch, NumPy), and dependencies repeatedly across a single workstation.
2. **Ecosystem Fragmentation**: Developers juggle fragmented CLI tools (`rustup`, `cargo`, `uv`, `poetry`, `pnpm`, `conda`, `nvm`) that store caches, configuration, and runtimes in scattered global directories across the host operating system.
3. **Global State Pollution**: Language tools routinely modify global `PATH`, user profile scripts, or registry entries, making multi-toolchain reproducibility fragile and causing environment collision.
4. **Uncertain Reclamation**: Deleting unused environments is manual and hazardous; developers fear breaking dependencies in co-dependent projects.

---

## 2. Design Philosophy

ULVO operates under five core architectural principles:

- **STORE ONCE**: Download and cache development runtimes, compilers, and external packages exactly once in a centralized content-addressable vault.
- **ISOLATE PER PROJECT**: Ensure every project retains clean dependency isolation without sharing mutable runtime state.
- **MATERIALIZE ON DEMAND**: Project environments are ephemeral projections (via NTFS hardlinks or directory junctions) instantiated when work begins.
- **EXECUTE THROUGH ULVO**: Tools and runtimes execute through the ULVO control plane with process-local environment variables, never polluting global host state.
- **RECLAIM SAFELY**: Inactive ephemeral projections can be unlinked at any time without data loss—project source code remains untouched, and shared assets remain preserved in the persistent vault.

---

## 3. Architecture Overview

ULVO does not reinvent low-level package managers or language compilers. Instead, it serves as a **unified control plane** orchestrating proven, high-performance ecosystem tools:

- **Python**: Leverages `uv` and standard wheels for rapid package caching and resolution.
- **Rust**: Leverages `rustup` and `cargo` inside an isolated single-root runtime vault.
- **JavaScript / TypeScript (Planned)**: Leverages `pnpm` content-addressable storage.
- **Native Windows & Linkers**: Coordinates with MSYS2/UCRT64 and native Win32 toolchains.

```mermaid
graph TD
    CLI["ULVO Native CLI (ulvo.exe)"]
    CORE["Universal Vault Registry (ulvo-core)"]
    ADAPT["Ecosystem Adapters (ulvo-adapters)<br/>(Python/uv, Rust, Win32 NTFS)"]
    DISC["Workspace Discovery (ulvo-discovery)<br/>(FS Traversal, Reparse Checks, Audit Engine)"]
    VAULT["Single-Root Vault (vault/)<br/>(runtimes/ | toolchains/ | registry/ | packages/)"]

    CLI --> DISC
    CLI --> ADAPT
    CLI --> CORE
    DISC --> ADAPT
    DISC --> CORE
    ADAPT --> CORE
    ADAPT --> VAULT
```

For detailed architectural specifications, see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

---

## 4. Current Implemented Functionality

The following capabilities are fully implemented, verified natively on Windows, and covered by automated regression tests:

- **Single-Root Vault**: Directory architecture isolating all managed assets under `vault/`.
- **Windows Rust Provisioning**: Hardened provisioning script (`scripts/provision-rust-windows.ps1`) with pinned artifact hashes, redirect validation, and junction isolation.
- **Native Windows Rust Compilation**: Working native toolchain (`x86_64-pc-windows-gnu`) building all crates and passing all tests without WSL dependence.
- **Universal Vault Registry**: Deterministic resource models (`ResourceId`), conflict rejection, idempotent registration, and lifecycle verification.
- **NTFS Filesystem Identity**: True 64-bit Master File Table file indexing via Win32 `GetFileInformationByHandle`.
- **Physical Disk Allocation**: Cluster allocation measurement using Win32 `GetCompressedFileSizeW`.
- **Python Virtual Environment Discovery**: Detection of virtualenvs, Python versions, editable installs (`direct_url.json`, `.pth`), and native C-extension wheels.
- **Cryptographic Manifest Discrimination**: Evaluation of `.dist-info/WHEEL` ABI tags and `.dist-info/RECORD` SHA-256 digests.
- **Bounded Storage Savings Model**: Three-tier set-theoretic savings model (`Conservative`, `Realistic`, `Theoretical`) without heuristic percentage multipliers.
- **Read-Only Non-Mutating CLI**: Full CLI suite with `--json` output (`inventory`, `inspect`, `vault`, `workspace`).

---

## 5. Planned Functionality

The following capabilities are in active development or planned for subsequent releases:

- **Stage 2.8 Ephemeral Python Environment Pilot**: On-demand materialization of ephemeral virtual environments for selected projects via junction projections.
- **Shared Package-Store Hardlinking**: Hardlinked package caching across isolated environments.
- **Multi-Ecosystem Expansion**: Adapters for Node.js (`pnpm`), Go (`GOMODCACHE`), and Java/JVM.
- **Desktop Graphical Interface**: Native Windows desktop GUI for workspace visualization and environment management.

---

## 6. Supported Platforms and Limitations

### Supported Platforms
- **Windows 11 / Windows 10 (x86_64)**: Primary supported platform. Full support for NTFS file identity, cluster measurement, and native CLI execution.
- **Ubuntu WSL**: Secondary build and test host. Supports Linux ELF compilation and POSIX device/inode identity.

### Current Limitations
- **Not a Standalone Package Manager**: ULVO orchestrates external toolchains; it does not replace Cargo or PyPI resolvers.
- **No GUI**: All current interaction occurs through the native `ulvo` CLI.
- **Manual Ephemeral Materialization**: Automatic lifecycle management of project virtual environments is planned for Stage 2.8+.
- **In-Situ Payload Verification**: Python package verification hashes distribution manifests (`RECORD`), not whole-disk package payloads, during discovery.
- **Hardlinked Store Caveat**: Package duplication reports logical byte counts; if an upstream tool (such as `uv`) already hardlinked files to a central cache, physical disk savings will be lower.

---

## 7. Repository Structure

```
ULVO/
├── Cargo.toml                    # Workspace definition (4 crates)
├── Cargo.lock                    # Cryptographically locked dependencies
├── config/                       # Pinned toolchain artifact manifests
│   └── toolchain-artifacts.toml
├── crates/
│   ├── ulvo-core/                # Pure domain models, registry, resource IDs (No I/O)
│   ├── ulvo-adapters/            # NTFS identity, Win32 APIs, Python/Rust adapters
│   ├── ulvo-discovery/           # Filesystem traversal, workspace audit engine
│   └── ulvo-cli/                 # Command-line driver and formatters
├── docs/                         # Architecture, CLI, security, and developer guides
├── scripts/                      # Toolchain provisioning and audit scripts
├── tests/                        # Validation harnesses and test fixtures
└── vault/                        # Single-root runtime and registry storage (Git-ignored)
```

---

## 8. Windows-Native Build Instructions

### Prerequisites
- Windows 10 or 11 (NTFS volume)
- Rust 1.96.0+ (or ULVO-managed Rust toolchain)
- MSYS2 UCRT64 GCC linker (`C:\msys64\ucrt64\bin\gcc.exe`)

### Setup and Build

```powershell
# 1. Configure process-local toolchain environment (optional if Rust is on system PATH)
$env:CARGO_HOME = "$PWD\vault\runtimes\rust\cargo"
$env:RUSTUP_HOME = "$PWD\vault\runtimes\rust\rustup"
$env:PATH = "$PWD\vault\runtimes\rust\cargo\bin;C:\msys64\ucrt64\bin;C:\msys64\usr\bin;" + $env:PATH

# 2. Check compilation
cargo check --workspace --locked --target x86_64-pc-windows-gnu

# 3. Run automated tests (60 unit and integration tests)
cargo test --workspace --locked --target x86_64-pc-windows-gnu

# 4. Build native executable
cargo build --workspace --locked --target x86_64-pc-windows-gnu
```

The compiled binary will be located at:
`target\x86_64-pc-windows-gnu\debug\ulvo.exe`

For detailed developer instructions, see [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

---

## 9. CLI Usage with Verified Examples

### Workspace Intelligence Scan
Scan a workspace directory to inspect storage, file counts, and categories:
```powershell
.\target\x86_64-pc-windows-gnu\debug\ulvo.exe workspace scan <PATH>
```
*Example Output:*
```text
ULVO Workspace Intelligence Audit (E:\Workspace)
  Scan Status:           INCOMPLETE (41 inaccessible entries)
  Logical Storage:       29.30 GB (316543 files, 314634 unique physical allocations)
  Physical Allocation:   28.85 GB
  Hardlink Deduplication: 1909 files

Top-Level Categories:
  Active                 9.35 GB logical    8.97 GB physical  (8 inaccessible)
  Academic               3.70 GB logical    3.70 GB physical
  Tools                  5.45 GB logical    5.45 GB physical
```

### Storage Savings Projections
Calculate conservative, realistic, and theoretical storage reclamation opportunities:
```powershell
.\target\x86_64-pc-windows-gnu\debug\ulvo.exe workspace savings <PATH> --json
```

### Vault Registry Listing
List all tracked development resources in the Universal Vault Registry:
```powershell
.\target\x86_64-pc-windows-gnu\debug\ulvo.exe vault list --json
```

For complete command-line documentation, see [docs/CLI.md](docs/CLI.md).

---

## 10. Universal Vault Registry

The Universal Vault Registry organizes development assets under a deterministic URI schema:
```text
<ecosystem>:<backend>:<category>:<name>:<version>
```
- Tracks ownership (`Managed`, `Configured`, `System`, `Discovered`, `Experimental`).
- Manages verification lifecycle (`Verified`, `Present`, `Configured`, `Stale`, `Failed`).
- Prevents cross-architecture and cross-ABI collisions.

For full technical specifications, see [docs/VAULT_REGISTRY.md](docs/VAULT_REGISTRY.md).

---

## 11. Python/uv Workspace Auditing

The Python adapter discovers and analyzes virtual environments:
- Identifies virtual environment roots via `pyvenv.cfg` or `conda-meta`.
- Discovers installed packages by parsing `.dist-info` directories in `site-packages`.
- Flags native C-extension wheels (`.pyd`, `.so`) and inspects `.dist-info/WHEEL` ABI tags.
- Detects local editable packages (`direct_url.json` and `.pth` files) to protect active projects from premature environment unlinking.

---

## 12. Storage Accounting & Limitations

ULVO measures storage strictly using native filesystem queries:
- **Logical Storage**: Total uncompressed bytes across all file entries.
- **Physical Allocation**: Actual cluster-allocated bytes on disk via `GetCompressedFileSizeW`.
- **Package Duplication**:
  - `[verified identical]`: Packages matching name, version, ABI tag, and RECORD content hash.
  - `[compatible candidate - differing ABI/RECORD]`: Packages matching name and version but differing in binary architecture or build digests. Wasted bytes are attributed strictly to verified identical copies.
- **Savings Hierarchy**:
  $$\text{Conservative} \le \text{Realistic} \le \text{Theoretical} \le \text{Total Workspace}$$

---

## 13. Security & Ownership Model

ULVO is built around a non-mutating, zero-trust execution model:
- **No Global Writes**: Zero writes to system `PATH` or Windows registry.
- **Reparse Point Rejection**: Filesystem traversals reject symlinks and directory junctions to prevent traversal loops and junction hijacking.
- **Process Sentinels**: Test fixtures require `.ulvo_test_sentinel` validation verifying process PID before cleanup.
- **Sandboxed Probes**: Toolchain version checks use null stdin, bounded 64 KiB capture buffers, and 3-second hard timeouts.

For complete security specifications, see [docs/SECURITY.md](docs/SECURITY.md).

---

## 14. Development Roadmap

- **Phase 0**: Security and model foundation [COMPLETED]
- **Stage 2.4 – 2.5**: Windows Rust provisioning and native compilation [COMPLETED]
- **Stage 2.6**: Universal Vault Registry foundation [COMPLETED]
- **Stage 2.7 – 2.7.1**: Workspace intelligence and NTFS file identity [COMPLETED]
- **Stage 2.8**: Controlled ephemeral Python virtual environment pilot [PLANNED]
- **Stage 2.9+**: Multi-language orchestration (pnpm, Go, Java) and native GUI [PLANNED]

For roadmap details, see [docs/ROADMAP.md](docs/ROADMAP.md).

---

## 15. Contributing & Development Status

ULVO is currently in pre-release development. Contributions, feedback, and issue reports are welcome.

Before submitting contributions:
1. Ensure all changes compile with `cargo check --workspace --locked --target x86_64-pc-windows-gnu`.
2. Ensure all tests pass with `cargo test --workspace --locked --target x86_64-pc-windows-gnu`.
3. Adhere to the core invariants in [docs/SECURITY.md](docs/SECURITY.md) and [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

---

## 16. Documentation Index

- [Architecture Specification](docs/ARCHITECTURE.md)
- [CLI Reference Manual](docs/CLI.md)
- [Universal Vault Registry Guide](docs/VAULT_REGISTRY.md)
- [Security & Safety Model](docs/SECURITY.md)
- [Development & Build Guide](docs/DEVELOPMENT.md)
- [Development Roadmap](docs/ROADMAP.md)
- [Project Changelog](CHANGELOG.md)
- [Stage 2.7.1 Workspace Audit Report](docs/STAGE2_7_WORKSPACE_AUDIT_REPORT.md)
