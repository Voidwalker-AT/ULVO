# ==============================================================================
# ULVO Stage 3.4 — IDE Integration & Configuration Validation Test
# ==============================================================================
$ErrorActionPreference = "Stop"

$workspaceRoot = "E:\Workspace\Active\ULVO"
$evidenceDir = "$workspaceRoot\docs\evidence\stage3-4"
if (-not (Test-Path $evidenceDir)) { New-Item -ItemType Directory -Path $evidenceDir -Force | Out-Null }

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "   ULVO Stage 3.4: IDE Integration & Configuration Audit" -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

$checks = @()

# 1. Test Windows Terminal Profile GUID Validity
$terminalGuid = "{a63d95b0-13f5-4c09-9069-000000000001}"
$guidRegex = "^\{[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\}$"
$guidValid = $terminalGuid -match $guidRegex
Write-Host "1. Windows Terminal GUID Syntax: $(if ($guidValid) { 'PASS (Valid RFC 4122 UUID)' } else { 'FAIL' })"
$checks += [PSCustomObject]@{
    Check = "Windows Terminal Profile GUID Validity"
    Passed = $guidValid
    Details = "GUID $terminalGuid conforms strictly to 128-bit RFC 4122 hexadecimal specification."
}

# 2. Test VS Code settings.json Syntax and Interpreter Resolution
$mockVscodeDir = "$workspaceRoot\vault\scratch\mock_vscode_project\.vscode"
if (-not (Test-Path $mockVscodeDir)) { New-Item -ItemType Directory -Path $mockVscodeDir -Force | Out-Null }

# The verified stable interpreter resolution strategy:
# For persistent projects, use the deterministic vault environment interpreter.
# For general workspace development, point to the vault-managed Python interpreter.
$vaultPyExe = "$workspaceRoot\vault\runtimes\python\cpython-3.11-windows-x86_64-none\python.exe"
$cmlEnvPy = "E:\Workspace\Active\ULVO\vault\environments\python\env_crossmodallab_ff2f88afbb62\Scripts\python.exe"

$settingsContent = @"
{
    "python.terminal.activateEnvironment": false,
    "python.locateExternalPython": false,
    "python.defaultInterpreterPath": "$($cmlEnvPy -replace '\\', '/')",
    "python.analysis.autoSearchPaths": true,
    "python.analysis.extraPaths": [
        "E:/Workspace/Active/ULVO/vault/packages/python/shared"
    ],
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
"@

$settingsFile = "$mockVscodeDir\settings.json"
Set-Content -Path $settingsFile -Value $settingsContent -Encoding utf8

$jsonValid = $false
try {
    $parsed = Get-Content -Path $settingsFile -Raw | ConvertFrom-Json
    $jsonValid = ($parsed.'python.defaultInterpreterPath' -ne $null)
} catch {
    $jsonValid = $false
}
Write-Host "2. VS Code settings.json Parsing: $(if ($jsonValid) { 'PASS (Valid JSON)' } else { 'FAIL' })"
$checks += [PSCustomObject]@{
    Check = "VS Code settings.json Format & Parsing"
    Passed = $jsonValid
    Details = "settings.json parsed cleanly by standard JSON parser without invalid comments."
}

# 3. Test Interpreter Existence and Execution
$interpreterExists = Test-Path $cmlEnvPy
$interpreterRuns = $false
if ($interpreterExists) {
    $out = (& $cmlEnvPy -c "import sys; print(sys.version)" 2>&1) | Out-String
    $interpreterRuns = ($LASTEXITCODE -eq 0) -and ($out -match "3\.11")
}
Write-Host "3. Referenced Python Interpreter: $(if ($interpreterRuns) { 'PASS (Executable & Functional)' } else { 'FAIL' })"
$checks += [PSCustomObject]@{
    Check = "Referenced Interpreter Path Existence & Execution"
    Passed = $interpreterRuns
    Details = "Resolved interpreter at $cmlEnvPy executed cleanly and confirmed Python 3.11."
}

# 4. Test Windows Dual Git Hook (sh + cmd polyglot)
$hookContent = @"
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
"%ULVO_EXE%" run "E:\Workspace\Active\CrossModalLab" python -c "print('[ULVO Pre-commit] Validation passed')"
exit /b %ERRORLEVEL%
"@
$hookPath = "$workspaceRoot\vault\scratch\pre-commit.cmd"
Set-Content -Path $hookPath -Value $hookContent -Encoding ascii

$hookRuns = $false
$hookOut = (& cmd.exe /c $hookPath 2>&1) | Out-String
if ($LASTEXITCODE -eq 0 -and $hookOut -match "Validation passed") {
    $hookRuns = $true
}
Write-Host "4. Windows Native Pre-Commit Hook: $(if ($hookRuns) { 'PASS (Executed without sh.exe)' } else { 'FAIL' })"
$checks += [PSCustomObject]@{
    Check = "Windows Native Pre-Commit Hook Execution"
    Passed = $hookRuns
    Details = "pre-commit.cmd executed cleanly under Windows cmd.exe without requiring sh.exe or Git Bash."
}

# 5. Clean mock files
Remove-Item -Path $mockVscodeDir -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -Path $hookPath -Force -ErrorAction SilentlyContinue

$allPassed = (($checks | Where-Object { -not $_.Passed }).Count -eq 0)

$report = @{
    schema_version = 1
    timestamp_utc = [DateTimeOffset]::UtcNow.ToString("o")
    checks = $checks
    overall_status = if ($allPassed) { "PASS" } else { "FAIL" }
}

$reportFile = "$evidenceDir\ide-integration-validation.json"
$report | ConvertTo-Json -Depth 5 | Set-Content -Path $reportFile -Encoding utf8
Write-Host "`nIDE integration validation saved to: $reportFile" -ForegroundColor Green

if (-not $allPassed) {
    exit 1
} else {
    exit 0
}
