# ULVO Stage 3.4 — IDE Integration Assessment & Hardened Blueprint

**Document Version:** 1.2.0  
**Status:** [VERIFIED]  
**Evaluated IDEs & Environments:** Visual Studio Code, Cursor, Windsurf, PyCharm, Google Antigravity, PowerShell 7 / Windows PowerShell 5.1, Windows Terminal, Git Hooks.

---

## 1. Executive Summary & Objective

In modern Windows developer setups, IDEs (specifically VS Code, Cursor, Windsurf, and PyCharm) and background language servers attempt to automatically detect, create, and activate virtual environments or locate toolchains on the system `PATH`. Without intentional configuration, IDEs exhibit behaviors that conflict with ULVO's storage optimization and single-entry gateway design:
1. **Unmanaged Local Virtual Environments:** The Python extension detects `requirements.txt` or `pyproject.toml` and prompts or silently creates an unmanaged `.venv` in the project root, duplicating gigabytes of wheel dependencies already deduplicated in the ULVO vault.
2. **Thin Connector Invariants:** Pointing an IDE to `${workspaceFolder}/.ulvo/runtime/...` is invalid because `.ulvo` is strictly a lightweight connector directory (target budget <= 100 KiB, hard limit <= 10 MiB) storing only project metadata and manifests.
3. **Interpreter Auto-Activation Conflicts:** IDE terminal sessions attempt to run `.venv\Scripts\Activate.ps1`, which fails or pollutes the session PATH with transient directory references.
4. **Toolchain Discovery Bypasses:** Build tools (Rust Analyzer, Go gopls, C/C++ clangd) look for compilers directly on `PATH`, failing when tools are sequestered in the vault or picking up unmanaged, outdated system tools from `C:`.

This blueprint provides tested, validated configuration templates, PowerShell hooks, and Git hook patterns ensuring zero unmanaged disk bloat, transparent interpreter resolution, and deterministic toolchain routing through the ULVO Gateway.

---

## 2. Risk Matrix & IDE Default Behavior

| Component | Default Behavior | ULVO Risk | Mitigation Strategy |
| :--- | :--- | :--- | :--- |
| **VS Code Python Extension** | Scans workspace for `.venv`; prompts "Create Environment" | Spawns unmanaged 2-5 GB duplicate environment | Set `"python.terminal.activateEnvironment": false`, `"python.defaultInterpreterPath"` to the deterministic ULVO vault environment interpreter. |
| **Pylance / Pyright** | Resolves site-packages from local `.venv` | Fails to resolve modules hardlinked into vault shared store | Set `"python.analysis.extraPaths"` pointing to the project's vault `site-packages` and the vault shared packages store. |
| **Rust Analyzer** | Invokes `cargo` and `rustc` directly from system `PATH` | Fails if vault Rust is not globally exposed | Configure `rust-analyzer.server.extraEnv` or `rust-analyzer.cargo.buildScripts.overrideCommand` via ULVO vault cargo. |
| **Node / npm / ESLint** | Looks for local `node_modules` | Standard behavior preserved; hardlinked store managed by ULVO | Standard node resolution; ensure node runner points to `ulvo exec node`. |
| **PowerShell Profiles** | Default shell inherits raw User PATH | Cannot run project tools if unmanaged venv is absent | Add lightweight non-destructive prompt hook in `$PROFILE` to auto-export environment context. |
| **Git Pre-commit Hooks** | Invokes `python -m pre_commit` or system `pytest` | Breaks if tools only exist in managed vault environment or if `sh.exe` is missing on Windows | Polyglot / batch pre-commit hook delegating directly to `ulvo.exe run`. |

---

## 3. Recommended IDE Configuration

### 3.1 Python Interpreter Resolution Architecture

ULVO environments are stored centrally in the vault on drive `E:` (`E:\Workspace\Active\ULVO\vault\environments\python\<env_id>`). To provide seamless IntelliSense, code completion, and debugging in IDEs:

1. **Persistent Projects (Recommended for Active Development):**
   When a project is onboarded with persistent lifecycle policy (`[lifecycle] policy = "persistent"` in `.ulvo/manifest.toml`), its environment path is deterministic. Query the path via:
   ```powershell
   ulvo project status <project-path>
   ```
   Example output:
   `Python Executable: E:\Workspace\Active\ULVO\vault\environments\python\env_crossmodallab_ff2f88afbb62\Scripts\python.exe`

2. **Workspace Default Interpreter:**
   For general exploration or polyglot workspaces before environment materialization, IDEs can point to ULVO's vault-managed CPython distribution:
   `E:/Workspace/Active/ULVO/vault/runtimes/python/cpython-3.11-windows-x86_64-none/python.exe`
   with `python.analysis.extraPaths` pointing to the shared package store `E:/Workspace/Active/ULVO/vault/packages/python/shared`.

### 3.2 Standard `.vscode/settings.json` Template

Place this file in `.vscode/settings.json` within any project onboarded to ULVO (e.g., `CrossModalLab`, `NexusNode`):

```json
{
  "python.terminal.activateEnvironment": false,
  "python.locateExternalPython": false,
  "python.defaultInterpreterPath": "E:/Workspace/Active/ULVO/vault/environments/python/env_crossmodallab_ff2f88afbb62/Scripts/python.exe",
  "python.analysis.autoSearchPaths": true,
  "python.analysis.extraPaths": [
    "E:/Workspace/Active/ULVO/vault/packages/python/shared"
  ],
  "terminal.integrated.env.windows": {
    "ULVO_MANAGED_PROJECT": "${workspaceFolder}"
  },
  "files.watcherExclude": {
    "**/.git/objects/**": true,
    "**/.git/subtree-cache/**": true,
    "**/node_modules/**": true,
    "**/target/**": true,
    "**/dist/**": true
  },
  "search.exclude": {
    "**/.git": true,
    "**/node_modules": true,
    "**/target": true
  }
}
```

> [!IMPORTANT]
> Never set `python.defaultInterpreterPath` to `${workspaceFolder}/.ulvo/...`. The `.ulvo` directory is a metadata-only connector and must never contain runtimes, virtual environments, or binaries.

---

## 4. Shell & Terminal Integration

### 4.1 PowerShell Profile Integration (`Microsoft.PowerShell_profile.ps1`)

Developers using PowerShell 7 (`pwsh`) or Windows PowerShell 5.1 can append the following non-destructive hook to their `$PROFILE`. This implementation preserves standard `cd` behavior, tab completion, and parameters:

```powershell
# ULVO Terminal Title Hook: Non-destructive project context indicator
if (Test-Path Function:\prompt) {
    $script:ulvo_old_prompt = (Get-Command prompt).Definition
} else {
    $script:ulvo_old_prompt = '"PS $($executionContext.SessionState.Path.CurrentLocation)$(''>'' * ($nestedPromptLevel + 1)) "'
}

function prompt {
    if (Test-Path -Path ".ulvo\manifest.toml" -PathType Leaf) {
        $host.UI.RawUI.WindowTitle = "[ULVO] $(Split-Path $PWD.Path -Leaf)"
    }
    & ([scriptblock]::Create($script:ulvo_old_prompt))
}
```

### 4.2 Windows Terminal Profile

For Windows Terminal, developers can configure a dedicated "ULVO Developer Shell". Note that the `guid` must strictly follow RFC 4122 hexadecimal syntax:

```json
{
  "guid": "{a63d95b0-13f5-4c09-9069-000000000001}",
  "name": "ULVO Developer Shell",
  "commandline": "pwsh.exe -NoExit -Command \"$env:PATH = 'E:\\Workspace\\Active\\ULVO\\bin;' + $env:PATH; Write-Host 'ULVO Developer Gateway Initialized' -ForegroundColor Cyan\"",
  "startingDirectory": "E:\\Workspace",
  "icon": "E:\\Workspace\\Active\\ULVO\\app\\desktop\\src\\assets\\icon.png"
}
```

---

## 5. Git Pre-Commit Hook Integration

To ensure pre-commit checks run deterministically on Windows systems regardless of whether Git Bash or `sh.exe` is installed:

### 5.1 Windows Native Pre-Commit Hook (`.git/hooks/pre-commit.cmd`)

```cmd
@echo off
setlocal
set "ULVO_EXE=E:\Workspace\Active\ULVO\bin\ulvo.exe"
if not exist "%ULVO_EXE%" (
    where ulvo.exe >nul 2>&1
    if %ERRORLEVEL% equ 0 (
        set "ULVO_EXE=ulvo.exe"
    ) else (
        echo [ERROR] ULVO gateway ulvo.exe was not found.
        exit /b 1
    )
)

echo [ULVO] Running pre-commit validation...
"%ULVO_EXE%" run "%~dp0..\.." python -m unittest discover -s tests -p "test_*.py"
exit /b %ERRORLEVEL%
```

### 5.2 POSIX Shell Fallback (`.git/hooks/pre-commit`)

```bash
#!/bin/sh
# ULVO Pre-commit hook for POSIX/Git Bash environments
if command -v ulvo.exe >/dev/null 2>&1; then
    ULVO_BIN="ulvo.exe"
elif [ -f "E:/Workspace/Active/ULVO/bin/ulvo.exe" ]; then
    ULVO_BIN="E:/Workspace/Active/ULVO/bin/ulvo.exe"
else
    echo "[ERROR] ULVO gateway executable not found."
    exit 1
fi

"$ULVO_BIN" run "$(dirname "$0")/../.." python -m unittest discover -s tests -p "test_*.py"
exit $?
```

---

## 6. Verification Evidence

Validation results generated by `scripts/test-ide-integration.ps1` and stored in `docs/evidence/stage3-4/ide-integration-validation.json`:
- [x] Windows Terminal RFC 4122 GUID format verified: PASS
- [x] VS Code `settings.json` clean JSON syntax verified: PASS
- [x] Python interpreter path verified and functional: PASS
- [x] Windows Native Pre-Commit Hook executed cleanly under cmd.exe: PASS
- [x] Zero unmanaged virtual environments created in project workspace: PASS
