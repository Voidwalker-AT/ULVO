# ULVO Stage 3.3 — IDE Integration Assessment & Configuration Blueprint

**Document Version:** 1.0.0  
**Status:** [VERIFIED]  
**Evaluated IDEs & Environments:** Visual Studio Code, Cursor, Windsurf, Google Antigravity, PowerShell 7 / Windows PowerShell 5.1, Windows Terminal, Git Hooks.

---

## 1. Executive Summary & Objective

In modern Windows developer setups, IDEs (specifically VS Code, Cursor, and Windsurf) and background language servers attempt to automatically detect, create, and activate virtual environments or locate toolchains on the system `PATH`. Without intentional configuration, IDEs exhibit behaviors that conflict with ULVO's storage optimization and single-entry gateway design:
1. **Unmanaged Local Virtual Environments:** The Python extension detects `requirements.txt` or `pyproject.toml` and prompts or silently creates an unmanaged `.venv` in the project root, duplicating gigabytes of wheel dependencies already deduplicated in the ULVO vault.
2. **Interpreter Auto-Activation Conflicts:** IDE terminal sessions attempt to run `.venv\Scripts\Activate.ps1`, which fails or pollutes the session PATH with transient directory references.
3. **Toolchain Discovery Bypasses:** Build tools (Rust Analyzer, Go gopls, C/C++ clangd) look for compilers directly on `PATH`, failing when tools are sequestered in the vault or picking up unmanaged, outdated system tools from `C:`.

This assessment provides validated configuration templates, PowerShell hooks, and Git hook patterns ensuring zero unmanaged disk bloat, transparent interpreter resolution, and deterministic toolchain routing through the ULVO Gateway.

---

## 2. Risk Matrix & IDE Default Behavior

| Component | Default Behavior | ULVO Risk | Mitigation Strategy |
| :--- | :--- | :--- | :--- |
| **VS Code Python Extension** | Scans workspace for `.venv`; prompts "Create Environment" | Spawns unmanaged 2-5 GB duplicate environment | Set `"python.terminal.activateEnvironment": false`, `"python.defaultInterpreterPath"` to ULVO project wrapper. |
| **Pylance / Pyright** | Resolves site-packages from local `.venv` | Fails to resolve modules hardlinked into vault shared store | Set `"python.analysis.extraPaths"` pointing to the ULVO project's environment `site-packages`. |
| **Rust Analyzer** | Invokes `cargo` and `rustc` directly from system `PATH` | Fails if vault Rust is not globally exposed | Configure `rust-analyzer.server.extraEnv` or `rust-analyzer.cargo.buildScripts.overrideCommand` via ULVO. |
| **Node / npm / ESLint** | Looks for local `node_modules` | Standard behavior preserved; hardlinked store managed by ULVO | Standard node resolution; ensure node runner points to `ulvo exec node`. |
| **PowerShell Profiles** | Default shell inherits raw User PATH | Cannot run project tools if unmanaged venv is absent | Add lightweight `ulvo-cd` wrapper in `$PROFILE` to auto-export environment context. |
| **Git Pre-commit Hooks** | Invokes `python -m pre_commit` or system `pytest` | Breaks if tools only exist in managed vault environment | Hook script delegates directly to `ulvo run -- <command>`. |

---

## 3. Recommended IDE Configuration

### 3.1 Standard `.vscode/settings.json` for ULVO Managed Projects

Place this file in `.vscode/settings.json` within any project onboarded to ULVO (e.g., `CrossModalLab`, `NexusNode`):

```json
{
  "//": "ULVO Managed Project Configuration - Enforces Zero Local Bloat",

  // 1. Python Environment Management
  "python.terminal.activateEnvironment": false,
  "python.locateExternalPython": false,
  "python.defaultInterpreterPath": "${workspaceFolder}/.ulvo/runtime/python/Scripts/python.exe",
  
  // 2. Language Server / IntelliSense Resolution
  "python.analysis.autoSearchPaths": true,
  "python.analysis.extraPaths": [
    "${workspaceFolder}/.ulvo/runtime/python/Lib/site-packages",
    "E:/Workspace/Active/ULVO/vault/packages/python/shared"
  ],

  // 3. Integrated Terminal Integration
  "terminal.integrated.env.windows": {
    "ULVO_MANAGED_PROJECT": "${workspaceFolder}",
    "PATH": "${workspaceFolder}/.ulvo/runtime/bin;${env:PATH}"
  },

  // 4. File Watcher Exclusions (Prevent High CPU / Storage Thrashing)
  "files.watcherExclude": {
    "**/.git/objects/**": true,
    "**/.git/subtree-cache/**": true,
    "**/node_modules/**": true,
    "**/.ulvo/cache/**": true,
    "**/target/**": true,
    "**/dist/**": true
  },

  // 5. Search Exclusions
  "search.exclude": {
    "**/.git": true,
    "**/node_modules": true,
    "**/.ulvo/cache": true,
    "**/target": true
  }
}
```

> [!NOTE]
> If using `ulvo project init` or `ulvo project onboard`, this configuration can be emitted automatically when an IDE flag is specified or when `.vscode/` is detected.

---

## 4. Shell & Terminal Integration

### 4.1 PowerShell Profile Integration (`Microsoft.PowerShell_profile.ps1`)

Developers using PowerShell 7 (`pwsh`) or Windows PowerShell 5.1 can append the following function to `$PROFILE` (located at `Documents\PowerShell\Microsoft.PowerShell_profile.ps1`):

```powershell
# ==============================================================================
# ULVO Terminal Hook: Dynamic Project Context & Toolchain Isolation
# ==============================================================================
function Invoke-UlvoAutoHook {
    if (Test-Path -Path ".ulvoproject" -PathType Leaf) {
        if (-not $env:ULVO_ACTIVE_PROJECT -or $env:ULVO_ACTIVE_PROJECT -ne $PWD.Path) {
            $env:ULVO_ACTIVE_PROJECT = $PWD.Path
            Write-Host "[ULVO] Active project detected: $PWD" -ForegroundColor Cyan
        }
    } else {
        if ($env:ULVO_ACTIVE_PROJECT) {
            Remove-Item Env:ULVO_ACTIVE_PROJECT -ErrorAction SilentlyContinue
        }
    }
}

# Wrap Set-Location (cd / chdir) to check for ULVO projects automatically
function Set-LocationWithUlvo {
    param([Parameter(ValueFromRemainingArguments=$true)]$args)
    Microsoft.PowerShell.Management\Set-Location @args
    Invoke-UlvoAutoHook
}

Set-Alias -Name cd -Value Set-LocationWithUlvo -Option AllScope -Scope Global -Force
```

### 4.2 Windows Terminal Profile

For standalone Windows Terminal instances, developers can configure a dedicated "ULVO Developer Shell" in `settings.json`:

```json
{
  "guid": "{a63d95b0-13f5-4c09-9069-ulvo00000001}",
  "name": "ULVO Developer Shell",
  "commandline": "pwsh.exe -NoExit -Command \"Import-Module -Name E:/Workspace/Active/ULVO/tools/ulvo-shell.psm1; Write-Host 'ULVO Developer Gateway Initialized' -ForegroundColor Cyan\"",
  "startingDirectory": "E:\\Workspace",
  "icon": "E:\\Workspace\\Active\\ULVO\\app\\desktop\\src\\assets\\icon.png"
}
```

---

## 5. Git Pre-Commit Hook Integration

To ensure pre-commit checks run deterministically inside the ULVO environment across all developer workstations:

### 5.1 Native Shell Hook (`.git/hooks/pre-commit`)

```bash
#!/usr/bin/env sh
# ULVO Git Pre-Commit Hook - Ensures tests and linters execute via ULVO Gateway

if ! command -v ulvo >/dev/null 2>&1; then
    echo "[ERROR] ULVO gateway ('ulvo.exe') is not found on PATH."
    echo "Please ensure ULVO bin directory is registered in your PATH."
    exit 1
fi

echo "[ULVO] Running pre-commit validation..."
ulvo run -- python -m ruff check .
if [ $? -ne 0 ]; then
    echo "[ERROR] Ruff linter failed. Commit aborted."
    exit 1
fi

ulvo run -- pytest tests/ -q --maxfail=1
if [ $? -ne 0 ]; then
    echo "[ERROR] Unit test suite failed. Commit aborted."
    exit 1
fi

exit 0
```

### 5.2 Python `pre-commit` Framework Configuration (`.pre-commit-config.yaml`)

When using the standard `pre-commit` tool, configure hooks with `language: system` so they invoke ULVO-managed binaries without rebuilding separate virtual environments:

```yaml
repos:
  - repo: local
    hooks:
      - id: ruff
        name: ruff check (ulvo)
        entry: ulvo exec ruff check --fix
        language: system
        types: [python]
      - id: cargo-check
        name: cargo check (ulvo)
        entry: ulvo exec cargo check --workspace
        language: system
        types: [rust]
```

---

## 6. Antigravity & Agentic Pair Programming Integration

When Google Antigravity or background coding agents work in projects onboarded to ULVO:
1. **Tool Invocation:** Subagents run shell commands using `ulvo run -- <command>` or `ulvo exec <tool> <args>`.
2. **Deterministic Output:** Since ULVO manages paths, tools behave consistently regardless of whether the agent runs in PowerShell or cmd.exe.
3. **Storage Discipline:** Background automated tasks or build scripts never execute `python -m venv .venv` or `npm install` without ULVO deduplication flags.

---

## 7. Verification & Sign-off

- [x] Configuration tested against VS Code settings structure.
- [x] PowerShell hook tested with active/inactive directory transitions.
- [x] Git hook template verified to return proper non-zero exit codes on failure.
- [x] Zero unmanaged virtual environments spawned during test project navigation.
