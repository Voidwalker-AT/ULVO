# Universal Vault Registry Architecture

The **Universal Vault Registry** is the central resource ledger of ULVO, implemented in the [`ulvo-core`](../crates/ulvo-core/src/registry.rs) and [`ulvo-discovery`](../crates/ulvo-discovery/src/vault.rs) crates.

It tracks all managed, external, discovered, and experimental development resources—including language runtimes, compilers, package caches, and virtual environments—under a unified metadata and lifecycle contract.

---

## 1. Resource Identity & Deterministic `ResourceId`

Resources in ULVO are identified by a deterministic `ResourceId` URI:

```text
<ecosystem>:<backend>:<category>:<name>:<version>
```

### Format Specification
- **`ecosystem`**: The target language or platform ecosystem (e.g. `rust`, `python`, `node`, `go`, `system`).
- **`backend`**: The host execution environment (`windows`, `wsl`, `msys2`).
- **`category`**: The resource type:
  - `runtime`: Base interpreter or execution engine (e.g. `python.exe`, `node.exe`).
  - `toolchain`: Compiler and build driver (e.g. `rustc.exe`, `gcc.exe`).
  - `package`: Distributed library or wheel archive.
  - `environment`: Project virtual environment.
  - `sdk`: Platform SDK or native headers.
- **`name`**: Normalized canonical tool or package name (e.g. `rustc`, `cffi`, `pyside6`).
- **`version`**: Semantic or distribution version string (e.g. `1.96.0`, `3.11.9`, `2.3.3`).

### Path Normalization & Collision Prevention
- **Path Case Sensitivity**: On Windows hosts, all paths are canonicalized and converted to lowercase for equality comparisons to prevent duplicate entries caused by casing variations. On POSIX/Linux hosts, exact path casing is preserved.
- **Architectural Separation**: Resources differing in target architecture (e.g. `x86_64` vs `aarch64`) or ABI (e.g. `gnu` vs `msvc`, `cp311` vs `cp312`) generate distinct resource records and never collide.
- **Backend Separation**: A toolchain discovered in WSL (e.g. `/usr/bin/python3`) and a toolchain on Windows (e.g. `C:\Python312\python.exe`) are partitioned by their backend identifier.

---

## 2. Core Domain Models

### `ManagedResource`
Represents an individual tracked resource:
```rust
pub struct ManagedResource {
    pub id: ResourceId,
    pub path: PathBuf,
    pub ownership: OwnershipCategory,
    pub verification: VerificationState,
    pub capabilities: Vec<ResourceCapability>,
    pub metadata: HashMap<String, String>,
    pub last_verified: Option<SystemTime>,
}
```

### `OwnershipCategory`
Defines who manages the lifecycle of the resource:
- `Managed`: Exclusively owned and orchestrated by ULVO within the single-root `vault/`. Can be installed, updated, or safely deleted by ULVO.
- `Configured`: Declared in user or project configuration files (e.g. `.ulvo/manifest.toml`, VS Code settings). Read-only; never deleted by ULVO.
- `System`: Installed at standard OS system locations (e.g. `C:\Windows`, `C:\Program Files`). Read-only.
- `Discovered`: Identified during ambient filesystem scans of PATH or development directories.
- `Experimental`: Resources created during testing or under active development.

### `VerificationState`
Tracks the operational readiness of the resource:
- `Verified`: The backing executable/files exist on disk, match expected cryptographic hashes or PE/ELF headers, and passed non-mutating validation.
- `Present`: The backing file exists at the recorded path, but deep verification has not yet run.
- `Configured`: Referenced in configuration, but unverified on disk.
- `Stale`: The resource was previously verified, but on-disk file size, timestamp, or identity has changed since last check.
- `Failed`: The resource file is missing, corrupted, or returned an unexpected exit code during validation.
- `Unsupported`: The resource exists for a foreign backend that cannot be executed on the current host.

### `ResourceCapability`
Capabilities declared by the resource:
- `Compiler`: Able to compile source code into binaries (e.g. `rustc`, `gcc`, `javac`).
- `Runtime`: Able to execute programs directly (e.g. `python`, `node`, `java`).
- `PackageManager`: Manages packages or dependencies (e.g. `cargo`, `uv`, `pnpm`).
- `LanguageServer`: Provides editor intelligence (e.g. `clangd`, `rust-analyzer`).
- `Linter`: Code analysis and formatting (e.g. `clippy`, `ruff`).
- `Debugger`: Debug engine (e.g. `gdb`, `lldb`).

---

## 3. Registry Reconciliation Engine

The reconciliation engine ([`ulvo-core/src/registry.rs`](../crates/ulvo-core/src/registry.rs)) synchronizes in-memory registry states with physical on-disk realities:

1. **Idempotent Registration**: Registering an identical resource is a no-op that returns `Ok(false)` without state duplication.
2. **Conflict Rejection**: Attempting to register two conflicting versions or distinct ecosystems at the exact same physical path is rejected with `Err`.
3. **Staleness Transition**: When `verify_resource()` detects that a file's timestamp or size has diverged from its registry record, the resource state transitions automatically to `VerificationState::Stale`.
4. **Non-Mutating Verification**: Resource verification never executes arbitrary code or scripts; it verifies file existence, PE/ELF magic bytes, and optionally runs pinned `--version` probes.

---

## 4. On-Disk Persistence Schema

Registry state is persisted as strict JSON in `vault/registry/`:

```json
{
  "schema_version": 1,
  "resources": [
    {
      "id": "rust:windows:managed:rustc:1.96.0",
      "path": "vault/runtimes/rust/cargo/bin/rustc.exe",
      "ownership": "managed",
      "verification": "verified",
      "capabilities": [
        "compiler"
      ],
      "metadata": {
        "target": "x86_64-pc-windows-gnu",
        "channel": "stable"
      },
      "last_verified": 1729296000
    }
  ]
}
```
