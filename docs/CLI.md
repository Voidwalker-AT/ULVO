# ULVO Command-Line Interface Reference

The `ulvo` binary is a Windows-native command-line tool (`x86_64-pc-windows-gnu`) providing read-only toolchain discovery, project manifest inspection, vault registry management, and workspace storage intelligence.

---

## Global Options

```text
Usage: ulvo.exe [OPTIONS] <COMMAND>

Commands:
  inventory  Discover installed compilers, runtimes, and development tools
  inspect    Inspect a project directory and its manifest configuration
  vault      Manage the Universal Vault Registry and inspect managed resources
  workspace  Analyze workspace storage, Python environments, and deduplication
  help       Print this message or the help of the given subcommand(s)

Options:
      --json     Serialize command output as standard JSON
  -h, --help     Print help
  -V, --version  Print version
```

---

## 1. `ulvo inventory`

Discovers toolchains, language runtimes, compilers, and development tools installed across the host system.

### Syntax
```sh
ulvo inventory [OPTIONS]
```

### Options
- `--all-backends`: Enumerate toolchains across all detected backends (Windows, WSL, MSYS2).
- `--verify`: Execute opt-in bounded version checks on recognized native binaries in conservative locations.
- `--json`: Output full inventory records as JSON.

### Execution Policy
- **Without `--verify`**: Performs strictly zero subprocess execution. Detection is based on filesystem paths, PE/ELF binary headers, and configuration files.
- **With `--verify`**: Only recognized binaries at conservative known locations receive fixed, non-mutating version flags (e.g. `--version`). Stdin is closed, output is bounded at 64 KiB, and execution is killed after 3 seconds.

### Example
```sh
ulvo inventory --json
```

---

## 2. `ulvo inspect`

Inspects a project directory for recognized native manifests and `.ulvo/manifest.toml` declarations.

### Syntax
```sh
ulvo inspect [OPTIONS] <PATH>
```

### Options
- `--json`: Output inspection findings as JSON.

### Execution Policy
- Traversal checks reject paths containing symlinks or reparse points.
- Reads manifest files up to 1 MiB.
- Never runs build scripts, dependency resolvers, or shell commands.

### Example
```sh
ulvo inspect ./my-project --json
```

---

## 3. `ulvo vault`

Interacts with the Universal Vault Registry and inspects managed runtimes.

### Subcommands

#### `ulvo vault list`
Lists all managed and registered resources in the vault.
```sh
ulvo vault list [OPTIONS]
```
- `--backend <BACKEND>`: Filter by host backend (`windows`, `wsl`, `msys2`).
- `--ecosystem <ECOSYSTEM>`: Filter by ecosystem (`rust`, `python`, etc.).
- `--ownership <OWNERSHIP>`: Filter by ownership (`managed`, `configured`, `system`, `discovered`, `experimental`).
- `--json`: Output as JSON array.

#### `ulvo vault inspect`
Displays full metadata, capabilities, and verification history for a specific resource ID.
```sh
ulvo vault inspect <RESOURCE_ID> [--json]
```

#### `ulvo vault verify`
Verifies that the files backing a registered resource exist and retain their expected identity.
```sh
ulvo vault verify <RESOURCE_ID> [--json]
```

#### `ulvo vault stats`
Displays registry aggregate statistics (total resources, ecosystems, backends, verification states).
```sh
ulvo vault stats [--json]
```

### Example
```sh
ulvo vault list --json
ulvo vault stats --json
ulvo vault inspect "rust:windows:managed:rustc:1.96.0" --json
```

---

## 4. `ulvo workspace`

Performs read-only workspace storage intelligence, NTFS physical cluster accounting, Python virtual environment discovery, and package deduplication analysis.

### Subcommands

#### `ulvo workspace scan`
Recursively traverses a workspace root, computing logical storage, unique physical allocations, hardlink deduplications, and category breakdowns.
```sh
ulvo workspace scan [OPTIONS] [PATH]
```
- `[PATH]`: Workspace root directory (defaults to current directory if omitted).
- `--json`: Output comprehensive scan report as JSON.

#### `ulvo workspace report`
Produces a comprehensive audit report including categories, all discovered Python environments, package duplication lists, and storage savings models.
```sh
ulvo workspace report [OPTIONS] [PATH]
```

#### `ulvo workspace python-envs`
Discovers and lists all Python virtual environments, inspecting Python versions, editable installs, and native C-extension wheels.
```sh
ulvo workspace python-envs [OPTIONS] [PATH]
```

#### `ulvo workspace duplicates`
Identifies duplicated package distributions across virtual environments, discriminating between byte-identical distributions and differing ABI/RECORD wheels.
```sh
ulvo workspace duplicates [OPTIONS] [PATH]
```

#### `ulvo workspace savings`
Calculates conservative, realistic, and theoretical storage reclamation projections without modifying any files.
```sh
ulvo workspace savings [OPTIONS] [PATH]
```

### Verified Examples

#### Scanning a Workspace
```sh
ulvo workspace scan ./workspace
```
*Sample Output:*
```text
ULVO Workspace Intelligence Audit (./workspace)
  Scan Status:           COMPLETE (0 inaccessible entries)
  Logical Storage:       12.45 GB (152000 files, 150000 unique physical allocations)
  Physical Allocation:   12.10 GB
  Hardlink Deduplication: 2000 files

Top-Level Categories:
  Active                 8.20 GB logical    8.00 GB physical
  Tools                  4.25 GB logical    4.10 GB physical
```

#### Calculating Storage Savings
```sh
ulvo workspace savings ./workspace --json
```
*Sample Output (JSON):*
```json
{
  "conservative": {
    "recoverable_bytes": 2222981120,
    "recoverable_gb": 2.07,
    "percentage_of_workspace": 7.1,
    "method": "Package-store deduplication of verified byte-identical packages across environments",
    "risk": "Low",
    "confidence": "Moderate (verified via package content hash comparison)",
    "assumptions": [
      "Deduplication requires byte-identical distributions verified by package content hashes",
      "Target environments support hardlink-based or symlink-based package store sharing",
      "Assumes independent copies; if packages are pre-hardlinked to a shared cache (e.g. uv), physical savings will be lower than logical duplicate estimates"
    ]
  },
  "realistic": {
    "recoverable_bytes": 7484323840,
    "recoverable_gb": 6.97,
    "percentage_of_workspace": 23.8,
    "method": "uv-managed shared cache for reproducible project environments plus package dedup for persistent environments",
    "risk": "Moderate",
    "confidence": "Moderate (projected based on non-editable project environments and reproducible manifests)",
    "assumptions": [
      "Projects with reproducible manifests and no editable installs transition to on-demand ephemeral environments",
      "Environments with local editable packages or base shared runtimes remain persistent",
      "Cross-environment package deduplication is applied strictly to persistent environments to prevent double-counting",
      "Eliminated arbitrary heuristic percentage multipliers"
    ]
  },
  "theoretical": {
    "recoverable_bytes": 31457280000,
    "recoverable_gb": 29.30,
    "percentage_of_workspace": 100.0,
    "method": "Complete reclamation of all project-level virtual environments into a centralized package store",
    "risk": "High (assumes all projects run purely on-demand with zero persistent venvs)",
    "confidence": "Low (theoretical upper bound; requires complete tooling adaptation)",
    "assumptions": [
      "All project dependencies are 100% resolvable and reconstructible on-demand via ULVO package store",
      "Zero persistent virtual environments maintained on disk",
      "Assumes zero developer disruption for unpinned or offline dependencies"
    ]
  }
}
```

---

## 5. `ulvo project`

Commands for inspecting, resolving, and diagnosing project configuration and runtime dependencies.

### 5.1 `ulvo project inspect <path>`
Inspects a project directory for recognized native manifests and `.ulvo/manifest.toml` declarations.

```sh
ulvo project inspect E:\Workspace\Active\NexusNode\server [--json]
```

### 5.2 `ulvo project resolve <path>`
Resolves runtime requirements and computes the deterministic specification fingerprint hash and vault target directory (`env_<slug>_<hash>`).

```sh
ulvo project resolve E:\Workspace\Active\NexusNode\server [--json]
```

### 5.3 `ulvo project doctor <path>`
Executes preflight health and dependency checks, verifying manifest syntax, `uv` package manager availability, runtime resolution, and dependency manifest presence. Purely read-only; does not create directories.

```sh
ulvo project doctor E:\Workspace\Active\NexusNode\server [--json]
```

---

## 6. `ulvo run`

Executes a command inside an isolated, ULVO-managed project environment. If the environment is not yet materialized, ULVO provisions it on demand inside the vault with PATH sanitization (preventing leakage from local `.venv`). Automatically attaches processes to a Win32 Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` to supervise child and worker processes.

### Syntax
```sh
ulvo run <PATH> [--reclaim] -- <COMMAND>...
```

### Options
- `--reclaim`: Automatically reclaim and unlink the environment after command execution quiesces.

### Example
```sh
ulvo run E:\Workspace\Active\NexusNode\server -- python -m unittest tests/test_package.py
```

---

## 7. `ulvo env`

Manages the lifecycle and reclamation of ULVO project environments.

### 7.1 `ulvo env status <path>`
Queries the current 9-state lifecycle (`DEFINED`, `RESOLVED`, `MATERIALIZING`, `READY`, `EXECUTING`, `RELEASED`, `RECLAIMABLE`, `RECLAIMED`, `FAILED`), active process leases with creation timestamps, specification hash, and quiescence.

```sh
ulvo env status E:\Workspace\Active\NexusNode\server [--json]
```

### 7.2 `ulvo env reclaim <path>`
Safely reclaims an environment if quiescent. Enforces fail-closed safety gates (Win32 process liveness checking, PID-reuse protection, ownership record validation, reparse point rejection). Employs leaf-first bottom-up traversal with retry backoff and reports physical bytes freed and volume free-space delta.

```sh
ulvo env reclaim E:\Workspace\Active\NexusNode\server [--json]
```

---

## 8. Non-Mutating Execution Guarantee

- Global environment variables (`PATH`, `PYTHONPATH`) and Windows registry keys are never altered.
- All temporary environments and package caches reside exclusively inside `vault/`.
- Safe reclamation fails closed if active processes are detected holding leases.
- All temporary test fixtures use process-level ownership sentinels.
