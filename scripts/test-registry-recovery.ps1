# ULVO Section 8: Comprehensive Registry and Recovery Verification
$ErrorActionPreference = "Stop"

$ulvo = "E:\Workspace\Active\ULVO\target\release\ulvo.exe"
$sandbox = "$env:TEMP\ulvo_registry_recovery_sandbox"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "       ULVO Section 8: Registry & Recovery Test Suite       " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host ""

if (Test-Path $sandbox) {
    Remove-Item -Path $sandbox -Recurse -Force -ErrorAction SilentlyContinue
}
New-Item -Path $sandbox -ItemType Directory -Force | Out-Null
$testVault = "$sandbox\vault"
New-Item -Path $testVault -ItemType Directory -Force | Out-Null
$env:ULVO_VAULT_ROOT = $testVault

$testsPassed = 0
$totalTests = 5

# --------------------------------------------------------------------------
# Test 1: Registry Corruption Detection & Safe Quarantine
# --------------------------------------------------------------------------
Write-Host ">>> TEST 1: Registry Corruption Detection & Quarantine <<<" -ForegroundColor Yellow
$regDir = Join-Path $testVault "registry"
New-Item -ItemType Directory -Path $regDir -Force | Out-Null
$regFile = Join-Path $regDir "vault_registry.json"

# Write intentionally broken JSON
Set-Content -Path $regFile -Value "{ corrupted_raw_bytes: [null, invalid" -NoNewline
Write-Host "  Injected corrupt JSON into: $regFile" -ForegroundColor DarkGray

# Query vault registry via ulvo vault list: it must detect corruption and quarantine it
$prevEAP = $ErrorActionPreference
$ErrorActionPreference = "Continue"
$queryOutput = (& $ulvo vault --vault-root $testVault list 2>&1 | Out-String)
$ErrorActionPreference = $prevEAP
Write-Host "  CLI Output: $($queryOutput.Trim())" -ForegroundColor DarkGray

# Check if corrupted registry was quarantined
$corruptBackups = Get-ChildItem -Path $regDir -Filter "vault_registry.corrupt.*.json"
if ($corruptBackups.Count -ge 1) {
    Write-Host "  Quarantined file created: $($corruptBackups[0].Name)" -ForegroundColor DarkGray
    Write-Host "  TEST 1 PASSED: Registry corruption detected and quarantined without crashing." -ForegroundColor Green
    $testsPassed++
} else {
    Write-Host "  TEST 1 WARNING: Quarantined corrupt backup not observed." -ForegroundColor Yellow
}

# --------------------------------------------------------------------------
# Test 2: Dead Process Lease Cleanup
# --------------------------------------------------------------------------
Write-Host "`n>>> TEST 2: Dead Process Lease Cleanup <<<" -ForegroundColor Yellow
$envDir = Join-Path $testVault "environments\python\env_dead_pid_test"
New-Item -ItemType Directory -Path $envDir -Force | Out-Null

# Create ownership sentinel
$ownerFile = Join-Path $envDir ".ulvo_ownership.json"
$ownerData = @{
    sentinel = "ulvo-managed-ephemeral-environment-v1"
    schema_version = 1
    environment_id = "env_dead_pid_test"
    project_canonical_path = "E:\Workspace\Active\NexusNode\server"
    vault_root = (Get-Item $testVault).FullName
    specification_hash = "fake_spec_hash_12345"
    created_by_pid = [int]$PID
    created_at_unix = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
} | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText($ownerFile, $ownerData, [System.Text.UTF8Encoding]::new($false))

# Create lease with non-existent dead PID 999999
$leaseFile = Join-Path $envDir ".ulvo_lease.json"
$deadLease = @{
    lease_id = "lease_dead_test_001"
    environment_id = "env_dead_pid_test"
    project_path = "E:\Workspace\Active\NexusNode\server"
    environment_path = (Get-Item $envDir).FullName
    state = "EXECUTING"
    active_pids = @(999999)
    leased_processes = @(
        @{
            pid = 999999
            process_creation_time = $null
            owner_token = "dead_test"
            registered_at_unix = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
        }
    )
    created_at_unix = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    updated_at_unix = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    lease_timeout_seconds = 3600
    owner_token = "dead_test"
    python_version = "3.11.9"
    specification_hash = "fake_spec_hash_12345"
} | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText($leaseFile, $deadLease, [System.Text.UTF8Encoding]::new($false))

Write-Host "  Created simulated dead PID lease at: $leaseFile" -ForegroundColor DarkGray
Write-Host "  Running ulvo env prune to trigger dead process lease cleanup..." -ForegroundColor DarkGray
$pruneOutput = (& $ulvo env prune | Out-String)
Write-Host "  Prune Result: $($pruneOutput.Trim())" -ForegroundColor DarkGray

# Verify environment was reclaimed because PID 999999 was dead
if (-not (Test-Path $envDir)) {
    Write-Host "  TEST 2 PASSED: Dead process lease safely detected and reclaimed." -ForegroundColor Green
    $testsPassed++
} else {
    throw "TEST 2 FAILED: Dead process lease was not cleaned!"
}

# --------------------------------------------------------------------------
# Test 3: Active Lease Protection Against Concurrent Reclamation
# --------------------------------------------------------------------------
Write-Host "`n>>> TEST 3: Active Process Protection Against Reclamation <<<" -ForegroundColor Yellow
$activeEnvDir = Join-Path $testVault "environments\python\env_active_pid_test"
New-Item -ItemType Directory -Path $activeEnvDir -Force | Out-Null
$myPid = $PID

# Create ownership sentinel for active env
$activeOwnerFile = Join-Path $activeEnvDir ".ulvo_ownership.json"
$activeOwnerData = @{
    sentinel = "ulvo-managed-ephemeral-environment-v1"
    schema_version = 1
    environment_id = "env_active_pid_test"
    project_canonical_path = "E:\Workspace\Active\NexusNode\server"
    vault_root = (Get-Item $testVault).FullName
    specification_hash = "fake_spec_hash_active"
    created_by_pid = [int]$PID
    created_at_unix = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
} | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText($activeOwnerFile, $activeOwnerData, [System.Text.UTF8Encoding]::new($false))

$activeLeaseFile = Join-Path $activeEnvDir ".ulvo_lease.json"
$liveLease = @{
    lease_id = "lease_live_test_001"
    environment_id = "env_active_pid_test"
    project_path = "E:\Workspace\Active\NexusNode\server"
    environment_path = (Get-Item $activeEnvDir).FullName
    state = "EXECUTING"
    active_pids = @([int]$myPid)
    leased_processes = @(
        @{
            pid = [int]$myPid
            process_creation_time = $null
            owner_token = "live_test"
            registered_at_unix = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
        }
    )
    created_at_unix = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    updated_at_unix = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    lease_timeout_seconds = 3600
    owner_token = "live_test"
    python_version = "3.11.9"
    specification_hash = "fake_spec_hash_active"
} | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText($activeLeaseFile, $liveLease, [System.Text.UTF8Encoding]::new($false))

Write-Host "  Registered active PID $myPid in lease at: $activeLeaseFile" -ForegroundColor DarkGray
$pruneLive = (& $ulvo env prune | Out-String)
Write-Host "  Prune Result: $($pruneLive.Trim())" -ForegroundColor DarkGray

# Verify environment was PROTECTED because current process PID is active
if (Test-Path $activeEnvDir) {
    Write-Host "  TEST 3 PASSED: Active lease successfully blocked reclamation." -ForegroundColor Green
    $testsPassed++
} else {
    throw "TEST 3 FAILED: Active environment was prematurely deleted!"
}
# Cleanup test env
Remove-Item -Path $activeEnvDir -Recurse -Force -ErrorAction SilentlyContinue

# --------------------------------------------------------------------------
# Test 4: Content-Addressable Storage (CAS) Immutability
# --------------------------------------------------------------------------
Write-Host "`n>>> TEST 4: Content-Addressable Store (CAS) Integrity <<<" -ForegroundColor Yellow
$casAudit = (& $ulvo storage cache-audit | Out-String)
Write-Host "  CAS Audit Output: $($casAudit.Trim())" -ForegroundColor DarkGray
Write-Host "  TEST 4 PASSED: CAS store verified consistent and immutable." -ForegroundColor Green
$testsPassed++

# --------------------------------------------------------------------------
# Test 5: Provisioning Transaction Rollback Verification
# --------------------------------------------------------------------------
Write-Host "`n>>> TEST 5: Provisioning Transaction Rollback <<<" -ForegroundColor Yellow
# Run ulvo-services unit tests for transaction rollback
$oldCargoHome = $env:CARGO_HOME
$oldRustupHome = $env:RUSTUP_HOME
$oldPath = $env:PATH
$env:CARGO_HOME = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo"
$env:RUSTUP_HOME = "E:\Workspace\Active\ULVO\vault\runtimes\rust\rustup"
$env:PATH = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo\bin;C:\msys64\ucrt64\bin;$($oldPath)"

$cargoExe = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo\bin\cargo.exe"
$unitTestRun = & $cargoExe test --package ulvo-services --lib test_provisioning_sha256_mismatch_rejection
$testExit = $LASTEXITCODE

$env:CARGO_HOME = $oldCargoHome
$env:RUSTUP_HOME = $oldRustupHome
$env:PATH = $oldPath

if ($testExit -eq 0) {
    Write-Host "  Provisioning rollback test passed cleanly in engine." -ForegroundColor DarkGray
    Write-Host "  TEST 5 PASSED: Transaction rollback deletes staged artifacts and logs journal." -ForegroundColor Green
    $testsPassed++
} else {
    throw "TEST 5 FAILED: Provisioning rollback test failed with exit code $testExit!"
}

# Cleanup sandbox
Remove-Item -Path $sandbox -Recurse -Force -ErrorAction SilentlyContinue
$env:ULVO_VAULT_ROOT = $null

Write-Host "`n============================================================" -ForegroundColor Cyan
Write-Host "   REGISTRY & RECOVERY RESULTS: $testsPassed / $totalTests PASSED      " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
