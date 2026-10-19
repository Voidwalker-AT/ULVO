# ULVO Stage 3.3 — IDE Integration Assessment & Configuration Blueprint (Superseded & Corrected in Stage 3.4)

> [!NOTE]
> This document was audited and corrected during Stage 3.4. For the hardened, production-verified blueprint, see [docs/evidence/stage3-4/ide-integration-assessment.md](file:///E:/Workspace/Active/ULVO/docs/evidence/stage3-4/ide-integration-assessment.md).
> Key corrections:
> 1. Corrected `python.defaultInterpreterPath` from non-existent `${workspaceFolder}/.ulvo/runtime/...` to deterministic vault environments.
> 2. Fixed Windows Terminal GUID to valid RFC 4122 hexadecimal string `{a63d95b0-13f5-4c09-9069-000000000001}`.
> 3. Replaced fragile `cd` alias override with non-destructive PowerShell prompt title hook.
> 4. Added Windows native batch pre-commit hook alongside POSIX shell fallback.

**Document Version:** 1.1.0  
**Status:** [VERIFIED]  
**Evaluated IDEs & Environments:** Visual Studio Code, Cursor, Windsurf, PyCharm, Google Antigravity, PowerShell 7 / Windows PowerShell 5.1, Windows Terminal, Git Hooks.

---

## 1. Executive Summary & Objective

In modern Windows developer setups, IDEs (specifically VS Code, Cursor, Windsurf, and PyCharm) and background language servers attempt to automatically detect, create, and activate virtual environments or locate toolchains on the system `PATH`. Without intentional configuration, IDEs exhibit behaviors that conflict with ULVO's storage optimization and single-entry gateway design:
1. **Unmanaged Local Virtual Environments:** The Python extension detects `requirements.txt` or `pyproject.toml` and prompts or silently creates an unmanaged `.venv` in the project root, duplicating gigabytes of wheel dependencies already deduplicated in the ULVO vault.
2. **Thin Connector Invariants:** Pointing an IDE to `${workspaceFolder}/.ulvo/runtime/...` is invalid because `.ulvo` is strictly a lightweight connector directory (target budget <= 100 KiB, hard limit <= 10 MiB) storing only project metadata and manifests.
3. **Interpreter Auto-Activation Conflicts:** IDE terminal sessions attempt to run `.venv\Scripts\Activate.ps1`, which fails or pollutes the session PATH with transient directory references.
4. **Toolchain Discovery Bypasses:** Build tools (Rust Analyzer, Go gopls, C/C++ clangd) look for compilers directly on `PATH`, failing when tools are sequestered in the vault or picking up unmanaged, outdated system tools from `C:`.

---

## 2. Hardened VS Code / Cursor / Windsurf Settings Template

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

---

## 3. Windows Terminal Profile

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

## 4. Git Pre-Commit Hook Integration

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
"%ULVO_EXE%" run "%~dp0..\.." python -m unittest discover -s tests -p "test_*.py"
exit /b %ERRORLEVEL%
```
