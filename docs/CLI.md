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

#### `ulvo vault cas <list|caches|verify|gc>`
Commands for interacting with the Universal Content-Addressable Dependency Store:
- `ulvo vault cas list`: Enumerate stored CAS artifacts, package kinds, ecosystems, unique physical CAS payload bytes, and total logical reference bytes.
- `ulvo vault cas caches`: Report allocations and status across 7 coordinated native ecosystem package stores (uv, pip, cargo, pnpm, go, gem, pacman).
- `ulvo vault cas verify`: Verify cryptographic SHA-256 integrity of all CAS artifacts against their CAS object digests.
- `ulvo vault cas gc [--plan] [--execute]`: Garbage collection of unreferenced artifacts (0 consumers). Defaults to read-only planning mode (`--plan`). Requires explicit `--execute` to perform unlinking. Fails closed if active environment leases, running processes, incomplete staging directories, or unowned manifests exist. Writes all reclaim actions to `gc_journal.jsonl`.

```sh
ulvo vault cas list
ulvo vault cas caches
ulvo vault cas verify
ulvo vault cas gc          # Read-only planning mode (dry-run, default)
ulvo vault cas gc --plan   # Explicit read-only planning mode
ulvo vault cas gc --execute # Explicit execution mode with journaled unlinking
```

### Example
```sh
ulvo vault list --json
ulvo vault stats --json
ulvo vault cas caches
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
Executes preflight health and dependency checks, verifying manifest syntax, Thin Connector Contract compliance (target size < 100 KiB, max budget 10 MiB, no prohibited artifacts), `uv` package manager availability, runtime resolution, and dependency manifest presence. Purely read-only; does not create directories.

```sh
ulvo project doctor E:\Workspace\Active\NexusNode\server [--json]
```

### 5.4 `ulvo project connector <path>`
Audits and verifies the lightweight `.ulvo` connector contract for a project:
- Verifies manifest presence and TOML schema validity.
- Measures total allocated size across all files in `.ulvo`.
- Enforces recommended target budget (under 100 KiB) and maximum budget limit (10 MiB).
- Scans for prohibited environment artifacts (`.venv`, `node_modules`, `site-packages`, `bin`, `lib`, `target`, `cache`, binary executables).
- Checks all referenced native manifests (`requirements.txt`, `package.json`, `Cargo.toml`, etc.).

```sh
ulvo project connector E:\Workspace\Active\NexusNode\server [--json]
```

### 5.5 `ulvo project status <path>`
Reports comprehensive project integration status, including environment ID, lifecycle state, active process leases, python executable path, specification hash, and connector compliance verdict.

```sh
ulvo project status E:\Workspace\Active\NexusNode\server [--json]
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

## 8. `ulvo migrate`

Automated workspace-wide Python virtual-environment migration and storage reclamation system. Scans projects, verifies dependency specifications, enforces 10 safety gates, synthesizes `.ulvo/manifest.toml`, and safely retires legacy environments.

### 8.1 `ulvo migrate scan [PATH]`
Performs a read-only scan of the workspace, discovering all Python virtual environments, resolving project roots, detecting Python versions, and analyzing dependency reproducibility.

```sh
ulvo migrate scan E:\Workspace [--json]
```

### 8.2 `ulvo migrate plan [PATH] [--safety-reserve <BYTES>]`
Formulates a capacity-aware, prioritized migration plan. Enforces a safety reserve (default: 500 MB) and calculates the available capacity budget. Blocks multi-gigabyte or high-risk ML environments whose sizes exceed the capacity budget (`BLOCKED_CAPACITY`), and sorts eligible candidates in ascending size order.

```sh
ulvo migrate plan E:\Workspace [--safety-reserve 524288000] [--json]
```

### 8.3 `ulvo migrate verify <PROJECT_PATH>`
Executes all 10 formal safety gates (Gates A–J) against a candidate project and its legacy environment. Returns `Retirement Status: ELIGIBLE FOR RETIREMENT` if and only if all gates pass.

```sh
ulvo migrate verify E:\Workspace\Tools\whichllm [--json]
```

### 8.4 `ulvo migrate apply <PROJECT_PATH>`
Atomically creates `.ulvo/manifest.toml` for the project using `write_atomic` without altering, touching, or invalidating the legacy environment.

```sh
ulvo migrate apply E:\Workspace\Tools\whichllm [--json]
```

### 8.5 `ulvo migrate retire <PROJECT_PATH> [--yes]`
Safely retires the project's legacy virtual environment leaf-first within the project boundary. Requires explicit `--yes` confirmation and enforces all 10 gates before unlinking files and directories.

```sh
ulvo migrate retire E:\Workspace\Tools\whichllm --yes [--json]
```

### 8.6 `ulvo migrate status [PATH]`
Displays the live migration status, risk level, and integration state across all workspace projects.

```sh
ulvo migrate status E:\Workspace [--json]
```

### 8.7 `ulvo migrate report [PATH]`
Generates a comprehensive migration and storage reclamation report with a full Markdown/ASCII summary table detailing discovered environments, integration status, functional verification, retirement results, and physical storage recovered.

```sh
ulvo migrate report E:\Workspace [--json]
```

---

## 9. `ulvo toolchain`

Universal multi-ecosystem toolchain discovery, inventory, and staged adoption.

### 9.1 `ulvo toolchain inventory`
Performs a comprehensive read-only scan of installed language toolchains across Windows PATH, known installation roots, MSYS2 (`C:\msys64`), configured VS Code profiles, and WSL metadata without launching WSL. Detects 29 development tools across Python, uv, Rust, Cargo, Node.js, npm, pnpm, Go, Java, javac, Ruby, gem, bundler, .NET SDK, GCC, G++, Clang, CMake, and Ninja.

```sh
ulvo toolchain inventory [--json]
```

### 9.2 `ulvo toolchain plan <TOOL_NAME>`
Formulates a typed 9-stage adoption plan (`EXTERNAL_DISCOVERED` -> `COMPATIBILITY_ASSESSED` -> `ADOPTION_PLANNED` -> `STAGING` -> `VERIFIED` -> `MANAGED`) for bringing an external toolchain under ULVO. Displays original location, intended vault path, strategy, verification commands, and rollback provisions.

```sh
ulvo toolchain plan go [--json]
```

### 9.3 `ulvo toolchain adopt <TOOL_NAME>`
Adopts a discovered external toolchain into the ULVO Vault Registry under `MANAGED` status using the non-destructive `ReferenceExternal` strategy. Once adopted, the tool is governed by the registry and executable via the Unified Gateway without altering global `PATH`.

```sh
ulvo toolchain adopt ruby
ulvo toolchain adopt go
```

### 9.4 `ulvo toolchain msys2 <adopt|verify|status>`
Provides first-class management of the native MSYS2 / C / C++ toolchain under the ULVO vault (`vault/toolchains/msys2`).

- `ulvo toolchain msys2 adopt [--source <PATH>]`: Adopts external MSYS2 into the vault with `.ulvo_managed.json` sentinel and verified provenance. Default source: `C:\msys64`.
- `ulvo toolchain msys2 verify`: Compiles and executes native C (`hello.c`) and C++ (`hello.cpp`) programs using the vault-managed GCC/G++ 14.2.0 compilers with zero runtime dependency on external `C:\msys64`.
- `ulvo toolchain msys2 status`: Reports the live provisioning state, subsystem status, and binary paths.

```sh
ulvo toolchain msys2 adopt --source C:\msys64
ulvo toolchain msys2 verify
ulvo toolchain msys2 status
```

---

## 10. `ulvo exec`

Executes commands through the ULVO Unified Execution Gateway. Resolves requested tools deterministically against Vault Registry identities and builds clean, isolated process environments without polluting global `PATH` or user profile scripts. Extended path normalization (`strip_unc_prefix`) automatically prevents MinGW/GCC `cannot execute 'cc1'` errors.

### Syntax
```sh
ulvo exec [--tool <NAME>] [--toolchain <NAME>] -- <COMMAND...>
```

### Options
- `--tool <NAME>`: Optional explicit tool identifier (e.g. `python`, `ruby`, `go`, `gcc`, `g++`, `java`).
- `--toolchain <NAME>`: Optional explicit toolchain selection (e.g. `msys2-ucrt64`, `msys2`, `rust`).
- `<COMMAND...>`: Command binary and trailing arguments.

### Examples
```sh
# Execute Python via unified gateway
ulvo exec --tool python -- -V

# Execute Ruby with MSYS2 UCRT64 toolchain environment
ulvo exec --tool ruby -- -v

# Execute Go with vault-managed GOROOT/GOMODCACHE
ulvo exec --tool go -- version

# Execute GCC via managed MSYS2 toolchain
ulvo exec --tool gcc -- --version

# Execute G++ via automatic deterministic gateway resolution
ulvo exec -- g++ --version

# Execute GNU Make inside clean process environment
ulvo exec -- make --version
```

---

## 11. `ulvo gpu`

GPU hardware telemetry, display driver inspection, and framework compatibility evaluation.

### 11.1 `ulvo gpu detect`
Detects host NVIDIA GPU hardware and display driver telemetry, reporting device model, total VRAM, driver version, compute capability (e.g. `sm_86`), and enforcing system-managed driver boundaries where kernel driver relocation is prohibited.

```sh
ulvo gpu detect [--json]
```

### 11.2 `ulvo gpu inspect <PATH>`
Evaluates a project's GPU requirements against host hardware, driver version, compute architecture, framework variants, and volume storage capacity. Distinguishes CPU-only wheels from CUDA builds and verifies available disk space against safety reserves.

```sh
ulvo gpu inspect E:\Workspace\Academic\sem5\DL\exp1 [--json]
```

---

## 12. `ulvo doctor`

Performs an exhaustive system and control plane health check across all release-critical subsystems:
- Platform and architecture integrity
- Vault durability (registry, lock, journal validation)
- Execution gateway availability (`bin/ulvo.exe`)
- Host GPU hardware and driver telemetry (NVIDIA RTX, driver version, compute capability)
- Managed and adopted language ecosystems (Python, Rust, MSYS2 C/C++, Ruby, Go, Node.js, Java)
- Storage reserves against safety reserve thresholds (500 MB minimum)
- CAS store cryptographic integrity

### Syntax
```sh
ulvo doctor [--json]
```

---

## 13. `ulvo catalog`

Authoritative online language runtime, compiler, build tool, and package catalog provider.

### Subcommands

#### `ulvo catalog list`
Lists all verified catalog runtimes, compilers, package managers, and build tools with their recommended version, category, and live host installation status.
```sh
ulvo catalog list [--json]
```

#### `ulvo catalog search`
Universal search query parser resolving ecosystem ambiguity into typed intents (`RuntimeInstallation`, `ToolchainInstallation`, `GlobalToolInstallation`, `ProjectDependency`, or `Ambiguous`).
```sh
ulvo catalog search <QUERY> [--json]
```
*Examples:*
- `ulvo catalog search python@3.12` -> RuntimeInstallation (Python 3 Runtime)
- `ulvo catalog search "torch==2.6.0+cu124"` -> ProjectDependency (torch)
- `ulvo catalog search cmake` -> ToolchainInstallation (CMake Build System)

#### `ulvo catalog install`
Provisions a verified runtime or toolchain from the online catalog using the 10-step transactional state machine (`Discover` -> `Resolve` -> `Plan` -> `Download` -> `Verify` -> `Stage` -> `Install` -> `Validate` -> `Register` -> `Commit`) with streaming SHA-256 CAS verification and atomic rollback.
```sh
ulvo catalog install <ENTRY_ID> [--version <VERSION>] [--json]
```

---

## 14. `ulvo storage`

Provides integrated storage and deduplication intelligence reporting across a project or workspace directory tree, reporting logical storage, physical disk allocation, hardlink deduplication count, CAS unique payload bytes, and discovered virtual environments.

### Syntax
```sh
ulvo storage [report] [<PATH>] [--json]
```

---

## 15. `ulvo toolchain list` & `ulvo toolchain verify`

### 15.1 `ulvo toolchain list`
Lists all discovered and managed compilers, runtimes, package managers, and SDKs with backend and ownership indicators.
```sh
ulvo toolchain list [--json]
```

### 15.2 `ulvo toolchain verify`
Runs automated compilation, linking, and execution verification across managed and adopted toolchains (MSYS2 C/C++, Rust, Python, Ruby, Go, Node.js, Java).
```sh
ulvo toolchain verify [<TOOL>] [--json]
```

---

## 16. `ulvo project init`

Initializes a minimal, compliant `.ulvo/manifest.toml` thin connector inside a target project directory (< 100 KiB budget, zero heavy runtime binaries or virtual environments).

### Syntax
```sh
ulvo project init [<PROJECT_PATH>] [--ecosystem <ECOSYSTEM>] [--runtime-version <VERSION>] [--json]
```

---

## 17. `ulvo env audit` & `ulvo env prune`

Manages materialized project environments within the ULVO vault (`vault/environments/<ecosystem>`). Enforces the Stage 3.0 invariant: zero persistent project-specific environments.

### 17.1 `ulvo env audit`
Audits all materialized environments in the vault, reporting their state (`ACTIVE`, `RECLAIMABLE`, `ABORTED`, `UNKNOWN`), active PIDs, logical/physical storage consumption, and owner project.
```sh
ulvo env audit [--json]
```

### 17.2 `ulvo env prune`
Safely garbage-collects inactive environments and aborted materializations. Active environments (referenced by running processes) and unrecognized directories are strictly protected and preserved.
```sh
ulvo env prune [--dry-run] [--json]
```

---

## 18. `ulvo storage cache-audit` & `ulvo storage cache-prune`

Provides governance and safe pruning over persistent package caches (`uv-cache`, shared wheels).

### 18.1 `ulvo storage cache-audit`
Audits package manager caches and shared package stores under the vault, displaying disk usage and cache paths.
```sh
ulvo storage cache-audit [--json]
```

### 18.2 `ulvo storage cache-prune`
Prunes stale package-manager caches using underlying ecosystem tooling (`uv cache prune`), reclaiming unreferenced wheel archives while preserving active shared packages.
```sh
ulvo storage cache-prune [--dry-run] [--json]
```

### 18.3 `ulvo storage build-audit`
Audits workspace Cargo build artifacts and classifies them into `Essential` (verified release binaries, manifests, and sources), `Reusable` (package manager and local Cargo registry caches), and `Disposable` (intermediate compiler objects, debug objects, incremental caches, and build scripts).
```sh
ulvo storage build-audit [<PATH>] [--json]
```

### 18.4 `ulvo storage build-prune`
Safely reclaims disposable compiler intermediates with strict safety gates (compiler liveness checking, reparse point rejection). By default preserves release dependency caches (`target/release/deps`) for fast incremental builds; pass `--all` to reclaim all disposable intermediates down to essential release binaries.
```sh
ulvo storage build-prune [<PATH>] [--dry-run] [--all] [--json]
```

---

## 19. `ulvo run [--keep-env]`

Executes commands inside an on-demand, ephemeral project environment materialized in the vault. Default lifecycle policy is **ephemeral**: upon command completion and quiescence (all child processes exited), the environment is automatically reclaimed unless `--keep-env` is passed or manifest policy is explicitly persistent.

### Syntax
```sh
ulvo run [--keep-env] <PROJECT_PATH> -- <COMMAND...>
```

---

## 20. `ulvo agent`

Manages the configurable per-user ULVO background reconciliation daemon communicating over loopback TCP IPC (`127.0.0.1:48590`) with bearer token authentication (`vault/agent.token`).

### Subcommands

#### `ulvo agent status`
Inspects background agent status and IPC connectivity.
```sh
ulvo agent status [--json]
```

#### `ulvo agent reconcile`
Triggers an immediate background lease and storage cache reconciliation cycle.
```sh
ulvo agent reconcile [--json]
```

#### `ulvo agent stop`
Sends a graceful termination signal to the running background agent daemon.
```sh
ulvo agent stop [--json]
```

#### `ulvo agent run`
Runs the background reconciliation agent daemon in the current process.
```sh
ulvo agent run [--port <PORT>]
```

---

## 21. `ulvo cache`

Unified multi-ecosystem package cache governance, cluster allocation reporting, and storage budget enforcement across uv, cargo, pnpm, go, gem, and pacman.

### Subcommands

#### `ulvo cache status`
Displays exact physical cluster allocation and cache directory paths for each coordinated package manager.
```sh
ulvo cache status [--json]
```

#### `ulvo cache prune`
Prunes stale, unreferenced package cache artifacts and temporary downloads while preserving active shared packages.
```sh
ulvo cache prune [--dry-run] [--json]
```

#### `ulvo cache gc`
Executes deep garbage collection subject to a configurable storage ceiling (e.g. `--max-gb 10`).
```sh
ulvo cache gc [--max-gb <GB>] [--dry-run] [--json]
```

---

## 22. Non-Mutating Execution Guarantee

- Global environment variables (`PATH`, `PYTHONPATH`) and Windows registry keys are never altered.
- All temporary environments and package caches reside exclusively inside `vault/`.
- Safe reclamation fails closed if active processes are detected holding leases.
- All temporary test fixtures use process-level ownership sentinels.
- Legacy environment retirement requires explicit authorization and passes 10 safety gates.
- Toolchain discovery is metadata-only and never launches or moves external installations.


