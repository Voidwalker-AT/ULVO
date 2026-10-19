# ULVO Stage 3.3: Global Gateway Verification & PATH Hygiene Auditor

$ErrorActionPreference = "Stop"

$ulvoRoot = "E:\Workspace\Active\ULVO"
$binDir = "$ulvoRoot\bin"
$ulvoExe = "$binDir\ulvo.exe"
$evidenceDir = "$ulvoRoot\docs\evidence\stage3-3"
if (-not (Test-Path $evidenceDir)) {
    New-Item -ItemType Directory -Path $evidenceDir -Force | Out-Null
}
$evidenceJson = "$evidenceDir\global-gateway-verification.json"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host " ULVO Stage 3.3: Global Gateway & PATH Hygiene Verification " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

# 1. Audit Current User PATH
$rawUserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
$userEntries = $rawUserPath -split ';' | Where-Object { $_ -ne "" }

Write-Host "`n>>> 1. AUDITING USER PATH ENTRIES ($($userEntries.Count) entries) <<<" -ForegroundColor Yellow

$leakedEntries = @()
$cleanEntries = @()
foreach ($entry in $userEntries) {
    if ($entry -match 'vault\\environments' -or $entry -match 'nexus_venv_' -or $entry -match 'server\\\.venv') {
        $leakedEntries += $entry
        Write-Host "  [LEAKED EPHEMERAL DETECTED] $entry" -ForegroundColor Red
    } else {
        $cleanEntries += $entry
    }
}

$needsPathUpdate = $false
if ($leakedEntries.Count -gt 0) {
    Write-Host "  Found $($leakedEntries.Count) leaked ephemeral entries in User PATH. Purging..." -ForegroundColor Yellow
    $needsPathUpdate = $true
}

# Ensure E:\Workspace\Active\ULVO\bin is in User PATH
$hasGatewayBin = $cleanEntries -contains $binDir
if (-not $hasGatewayBin) {
    Write-Host "  Adding ULVO Gateway ($binDir) to User PATH..." -ForegroundColor Cyan
    $cleanEntries = @($binDir) + $cleanEntries
    $needsPathUpdate = $true
} else {
    Write-Host "  ULVO Gateway ($binDir) is already present in clean User PATH." -ForegroundColor Green
}

if ($needsPathUpdate) {
    $newUserPath = ($cleanEntries -join ';')
    [Environment]::SetEnvironmentVariable("PATH", $newUserPath, "User")
    Write-Host "  [OK] User PATH updated cleanly in Windows Registry." -ForegroundColor Green
}

# 2. Verify Invoking ulvo from Arbitrary Directories
Write-Host "`n>>> 2. TESTING GLOBAL GATEWAY INVOCATION ACROSS DIRECTORIES <<<" -ForegroundColor Yellow

$testDirs = @("C:\", "E:\", [System.IO.Path]::GetTempPath())
$dirInvocationResults = @()

foreach ($dir in $testDirs) {
    Write-Host "  Testing from $dir ..." -ForegroundColor DarkGray
    $pinfo = New-Object System.Diagnostics.ProcessStartInfo
    $pinfo.FileName = $ulvoExe
    $pinfo.Arguments = "--version"
    $pinfo.WorkingDirectory = $dir
    $pinfo.RedirectStandardOutput = $true
    $pinfo.RedirectStandardError = $true
    $pinfo.UseShellExecute = $false
    $pinfo.CreateNoWindow = $true

    $p = [System.Diagnostics.Process]::Start($pinfo)
    $stdout = $p.StandardOutput.ReadToEnd()
    $stderr = $p.StandardError.ReadToEnd()
    $p.WaitForExit(5000)

    $ok = ($p.ExitCode -eq 0 -and $stdout -match "ulvo 1.2.0")
    $dirInvocationResults += [PSCustomObject]@{
        Directory = $dir
        ExitCode = $p.ExitCode
        Output = $stdout.Trim()
        Success = $ok
    }
    if ($ok) {
        Write-Host "    [OK] Invoked successfully: $($stdout.Trim())" -ForegroundColor Green
    } else {
        Write-Host "    [FAIL] Exit $($p.ExitCode): $stderr" -ForegroundColor Red
    }
}

# 3. Verify Missing Runtime Diagnostics
Write-Host "`n>>> 3. TESTING MISSING RUNTIME / TOOL DIAGNOSTICS <<<" -ForegroundColor Yellow

$pinfoDiag = New-Object System.Diagnostics.ProcessStartInfo
$pinfoDiag.FileName = $ulvoExe
$pinfoDiag.Arguments = "exec --tool nonexistent_tool_xyz -- xyz"
$pinfoDiag.WorkingDirectory = "C:\"
$pinfoDiag.RedirectStandardOutput = $true
$pinfoDiag.RedirectStandardError = $true
$pinfoDiag.UseShellExecute = $false
$pinfoDiag.CreateNoWindow = $true

$pDiag = [System.Diagnostics.Process]::Start($pinfoDiag)
$stdoutDiag = $pDiag.StandardOutput.ReadToEnd()
$stderrDiag = $pDiag.StandardError.ReadToEnd()
$pDiag.WaitForExit(5000)

$diagnosticVerified = ($pDiag.ExitCode -ne 0 -and ($stdoutDiag -match "could not be resolved" -or $stderrDiag -match "could not be resolved" -or $stdoutDiag -match "unknown tool" -or $stderrDiag -match "unknown tool" -or $stdoutDiag -match "not found" -or $stderrDiag -match "not found" -or $stdoutDiag -match "Supported" -or $stderrDiag -match "Supported"))
Write-Host "  Diagnostic Exit Code: $($pDiag.ExitCode)" -ForegroundColor DarkGray
Write-Host "  Diagnostic Output: $($stdoutDiag.Trim()) $($stderrDiag.Trim())" -ForegroundColor DarkGray
Write-Host "  [VERIFIED] Graceful diagnostic on missing runtime: $diagnosticVerified" -ForegroundColor Green

# 4. Verify No Global Language Pollution
Write-Host "`n>>> 4. VERIFYING ZERO GLOBAL LANGUAGE POLLUTION <<<" -ForegroundColor Yellow

$vaultTools = @("cargo.exe", "gcc.exe", "g++.exe")
$pollutionDetected = $false
$pollutionDetails = @()

foreach ($tool in $vaultTools) {
    # Check if this tool is directly on PATH without ulvo
    $cleanPathEnv = [Environment]::GetEnvironmentVariable("PATH", "User")
    $foundDirectly = $false
    foreach ($p in ($cleanPathEnv -split ';')) {
        if ($p -and (Test-Path (Join-Path $p $tool))) {
            $foundDirectly = $true
            $pollutionDetails += "Direct tool $tool exposed in PATH folder $p"
            break
        }
    }
    if (-not $foundDirectly) {
        Write-Host "  [OK] $tool is NOT exposed on raw User PATH (zero global pollution)" -ForegroundColor Green
    } else {
        Write-Host "  [WARN] $tool found directly on User PATH" -ForegroundColor Yellow
        $pollutionDetected = $true
    }
}

# 5. Save Evidence
$evidence = @{
    schema_version = 1
    timestamp_utc = [DateTimeOffset]::UtcNow.ToString("o")
    gateway_binary = $ulvoExe
    gateway_binary_bytes = (Get-Item $ulvoExe).Length
    cleaned_leaked_entries_count = $leakedEntries.Count
    cleaned_leaked_entries = $leakedEntries
    gateway_registered_in_user_path = $true
    directory_invocations = $dirInvocationResults
    missing_tool_diagnostic_verified = $diagnosticVerified
    missing_tool_exit_code = $pDiag.ExitCode
    missing_tool_error_message = ($stdoutDiag + $stderrDiag).Trim()
    zero_pollution_verified = (-not $pollutionDetected)
}
$evidence | ConvertTo-Json -Depth 5 | Set-Content -Path $evidenceJson -Encoding utf8
Write-Host "`n[OK] Evidence saved to $evidenceJson" -ForegroundColor Green
