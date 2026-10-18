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

## 5. Non-Mutating Execution Guarantee

All current `ulvo` CLI commands operate in **read-only mode**:
- No files are created, modified, or deleted within inspected project directories.
- No global environment variables (e.g. `PATH`, `PYTHONPATH`) or Windows registry keys are altered.
- All temporary test fixtures are created in isolated directories with explicit process-level ownership sentinels.
