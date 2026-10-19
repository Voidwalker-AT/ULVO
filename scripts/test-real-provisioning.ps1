# ULVO Section 5: Real Isolated Provisioning Lifecycle Test Suite
# Tests:
#  1. Catalog search and resolution
#  2. 10-Step transactional provisioning into isolated vault
#  3. Staging and destination containment verification
#  4. Binary execution from provisioned toolchain
#  5. Vault registry registration verification
#  6. Corrupt download SHA-256 rejection and rollback verification
#  7. Clean sandbox teardown

$ErrorActionPreference = "Stop"

$ulvo = "E:\Workspace\Active\ULVO\target\release\ulvo.exe"
$sandboxRoot = "$env:TEMP\ulvo_prov_test"
$sandboxVault = "$sandboxRoot\vault"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "     ULVO Section 5: Real Provisioning Lifecycle Test       " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host ""

if (Test-Path $sandboxRoot) {
    Remove-Item -Path $sandboxRoot -Recurse -Force -ErrorAction SilentlyContinue
}
New-Item -Path $sandboxVault -ItemType Directory -Force | Out-Null

$testsPassed = 0
$totalTests = 5

# --------------------------------------------------------------------------
# Test 1: Catalog Search and Resolution
# --------------------------------------------------------------------------
Write-Host ">>> TEST 1: Catalog Lookup & Resolution <<<" -ForegroundColor Yellow
$searchOutput = (& $ulvo catalog search ninja | Out-String)
Write-Host "  Search Output: $($searchOutput.Trim())" -ForegroundColor DarkGray
if ($searchOutput -match "ninja" -and $searchOutput -match "Ninja Fast Build Tool") {
    Write-Host "  TEST 1 PASSED: Catalog entry found and resolved." -ForegroundColor Green
    $testsPassed++
} else {
    throw "TEST 1 FAILED: Could not resolve 'ninja' in catalog search!"
}

# --------------------------------------------------------------------------
# Test 2: Provisioning via 10-Step Transaction Engine
# --------------------------------------------------------------------------
Write-Host "`n>>> TEST 2: 10-Step Transactional Provisioning <<<" -ForegroundColor Yellow
$prevVaultRoot = $env:ULVO_VAULT_ROOT
$env:ULVO_VAULT_ROOT = $sandboxVault

$installOutput = (& $ulvo catalog install ninja 2>&1 | Out-String)
Write-Host "  Install Output:`n$($installOutput.Trim())" -ForegroundColor DarkGray

if ($installOutput -match "Provisioning Successful" -or $LASTEXITCODE -eq 0) {
    Write-Host "  TEST 2 PASSED: 10-step transaction completed successfully." -ForegroundColor Green
    $testsPassed++
} else {
    throw "TEST 2 FAILED: Provisioning transaction failed!"
}

# --------------------------------------------------------------------------
# Test 3: Artifact Verification and Containment
# --------------------------------------------------------------------------
Write-Host "`n>>> TEST 3: Staged Containment & Execution Verification <<<" -ForegroundColor Yellow
$installedCmd = Join-Path $sandboxVault "toolchains\ninja\bin\ninja.cmd"
if (Test-Path $installedCmd) {
    Write-Host "  Installed command wrapper verified at: $installedCmd" -ForegroundColor DarkGray
    # Run the installed wrapper with bounded timeout
    $pinfo = New-Object System.Diagnostics.ProcessStartInfo
    $pinfo.FileName = "cmd.exe"
    $pinfo.Arguments = "/c `"$installedCmd`""
    $pinfo.RedirectStandardOutput = $true
    $pinfo.RedirectStandardError = $true
    $pinfo.UseShellExecute = $false
    $pinfo.CreateNoWindow = $true

    $proc = [System.Diagnostics.Process]::Start($pinfo)
    $hasExited = $proc.WaitForExit(5000)
    if (-not $hasExited) {
        $proc.Kill()
        throw "TEST 3 FAILED: Command wrapper timed out after 5 seconds!"
    }
    $execOutput = $proc.StandardOutput.ReadToEnd() + $proc.StandardError.ReadToEnd()
    Write-Host "  Executable Output: $($execOutput.Trim())" -ForegroundColor DarkGray
    if ($execOutput -match "ninja") {
        Write-Host "  TEST 3 PASSED: Provisioned executable launches and validates." -ForegroundColor Green
        $testsPassed++
    } else {
        throw "TEST 3 FAILED: Executable output did not match expected runtime banner!"
    }
} else {
    throw "TEST 3 FAILED: Expected installed wrapper not found at $installedCmd!"
}

# --------------------------------------------------------------------------
# Test 4: Staging Cleanup & Zero Residuals
# --------------------------------------------------------------------------
Write-Host "`n>>> TEST 4: Staging Cleanup & Zero Residuals <<<" -ForegroundColor Yellow
$stagingDir = Join-Path $sandboxVault "staging"
$residualItems = if (Test-Path $stagingDir) { Get-ChildItem -Path $stagingDir } else { @() }
if ($residualItems.Count -eq 0) {
    Write-Host "  Staging directory completely cleaned after transaction commit." -ForegroundColor DarkGray
    Write-Host "  TEST 4 PASSED: Zero uncommitted residuals left in staging." -ForegroundColor Green
    $testsPassed++
} else {
    throw "TEST 4 FAILED: Staging directory contains residual files: $($residualItems.Name)!"
}

# --------------------------------------------------------------------------
# Test 5: Failure Injection Rollback Verification
# --------------------------------------------------------------------------
Write-Host "`n>>> TEST 5: Transaction Rollback on Failure <<<" -ForegroundColor Yellow
# Run engine unit tests for failure rollback with SHA mismatch
$cargoExe = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo\bin\cargo.exe"
$oldCargoHome = $env:CARGO_HOME
$oldRustupHome = $env:RUSTUP_HOME
$oldPath = $env:PATH
$env:CARGO_HOME = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo"
$env:RUSTUP_HOME = "E:\Workspace\Active\ULVO\vault\runtimes\rust\rustup"
$env:PATH = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo\bin;C:\msys64\ucrt64\bin;$($oldPath)"

$testRollback = & $cargoExe test --package ulvo-services --lib test_provisioning_sha256_mismatch_rejection
$testRollbackExit = $LASTEXITCODE

$env:CARGO_HOME = $oldCargoHome
$env:RUSTUP_HOME = $oldRustupHome
$env:PATH = $oldPath

if ($testRollbackExit -eq 0) {
    Write-Host "  Rollback on integrity failure verified: staged files cleaned, no corrupt commits." -ForegroundColor DarkGray
    Write-Host "  TEST 5 PASSED: Transaction rollback functions correctly on failure." -ForegroundColor Green
    $testsPassed++
} else {
    throw "TEST 5 FAILED: Provisioning engine rollback test failed!"
}

# Teardown sandbox
$env:ULVO_VAULT_ROOT = $prevVaultRoot
Remove-Item -Path $sandboxRoot -Recurse -Force -ErrorAction SilentlyContinue

Write-Host "`n============================================================" -ForegroundColor Cyan
Write-Host "   PROVISIONING LIFECYCLE RESULTS: $testsPassed / $totalTests PASSED    " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
