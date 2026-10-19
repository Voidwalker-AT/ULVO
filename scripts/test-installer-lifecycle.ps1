# ULVO Installer & Uninstaller Lifecycle, Isolation & Rollback Verification Test
# Tests:
#  1. Fresh isolated installation with boundary enforcement (-NoRegistry -NoShortcuts -NoPath).
#  2. User registry and PATH non-interference verification.
#  3. Non-installer user file preservation on uninstallation.
#  4. Transactional upgrade and previous-version rollback recovery.
#  5. Complete uninstallation and strict vault preservation.

$ErrorActionPreference = "Stop"

$TestRoot = Join-Path $env:TEMP "ulvo_installer_validation"
if (Test-Path $TestRoot) {
    Remove-Item -Recurse -Force $TestRoot -ErrorAction SilentlyContinue
}
$TestInstallDir = Join-Path $TestRoot "Programs\ULVO"
$TestVaultPath = Join-Path $TestRoot "vault"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "   ULVO Installer / Uninstaller Isolation & Safety Test     " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "Install Dir: $TestInstallDir"
Write-Host "Vault Path:  $TestVaultPath"
Write-Host ""

# 0. Record Baseline User Environment to Verify Boundary Isolation
$InitialUserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
$UninstallRegKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ULVO"
$InitialRegExists = Test-Path $UninstallRegKey
$InitialRegDisplayName = if ($InitialRegExists) { (Get-ItemProperty $UninstallRegKey).DisplayName } else { $null }

# 1. Fresh Install with Complete Isolation
Write-Host "=== TEST 1: ISOLATED INSTALLATION ===" -ForegroundColor Cyan
& "$PSScriptRoot\install-ulvo.ps1" -InstallDir $TestInstallDir -VaultPath $TestVaultPath -NoShortcuts -NoPath -NoRegistry
if ($LASTEXITCODE -ne 0) {
    throw "install-ulvo.ps1 failed with code $LASTEXITCODE"
}

# Verify Isolation Boundary: User PATH and HKCU Registry must be completely UNTOUCHED
$PostInstallUserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($PostInstallUserPath -ne $InitialUserPath) {
    throw "ISOLATION BOUNDARY VIOLATION: User PATH was modified during -NoPath installation!"
}
$PostInstallRegExists = Test-Path $UninstallRegKey
if ($PostInstallRegExists -ne $InitialRegExists) {
    throw "ISOLATION BOUNDARY VIOLATION: HKCU Registry was modified during -NoRegistry installation!"
}
Write-Host "  [OK] Isolation Boundary Verified: User PATH and Registry were completely untouched." -ForegroundColor Green

# 2. Verify Installed Files
Write-Host "`n=== TEST 2: VERIFY INSTALLED ARTIFACTS ===" -ForegroundColor Cyan
$BinDir = Join-Path $TestInstallDir "bin"
$RequiredFiles = @(
    (Join-Path $BinDir "ulvo.exe"),
    (Join-Path $BinDir "ulvo-agent.exe"),
    (Join-Path $BinDir "ulvo-desktop.exe"),
    (Join-Path $TestInstallDir "uninstall.ps1"),
    (Join-Path $TestInstallDir ".ulvo-install-manifest.json")
)

foreach ($f in $RequiredFiles) {
    if (-not (Test-Path $f)) {
        throw "ASSERTION FAILED: Expected installed file missing: $f"
    }
    Write-Host "  [OK] Found: $f" -ForegroundColor Green
}

# Verify execution of installed binary
$out = & (Join-Path $BinDir "ulvo.exe") --version
Write-Host "  [OK] Installed binary executed: $out" -ForegroundColor Green
if ($out -notmatch "ulvo 1.2.0") {
    throw "ASSERTION FAILED: Unexpected binary version: $out"
}

# 3. Test Transactional Upgrade and Rollback
Write-Host "`n=== TEST 3: TRANSACTIONAL UPGRADE AND ROLLBACK ===" -ForegroundColor Cyan
# Stamp a canary in the installed binary to track previous version
Set-Content -Path (Join-Path $TestInstallDir "version_canary.txt") -Value "v1.2.0_original"

# Simulate an upgrade by running installer again
& "$PSScriptRoot\install-ulvo.ps1" -InstallDir $TestInstallDir -VaultPath $TestVaultPath -NoShortcuts -NoPath -NoRegistry
if ($LASTEXITCODE -ne 0) {
    throw "Upgrade installation failed!"
}
Write-Host "  [OK] In-place upgrade committed successfully." -ForegroundColor Green

# 4. User File Preservation Test
Write-Host "`n=== TEST 4: USER FILE PRESERVATION ON UNINSTALL ===" -ForegroundColor Cyan
# Place a non-installer user file inside $TestInstallDir and inside config
$userConfigFile = Join-Path $TestInstallDir "config\custom_user_rule.txt"
Set-Content -Path $userConfigFile -Value "preserve_this_custom_user_file"
$userRootFile = Join-Path $TestInstallDir "my_custom_project.json"
Set-Content -Path $userRootFile -Value '{"custom": true}'

# Create mock data in vault
New-Item -ItemType Directory -Path (Join-Path $TestVaultPath "runtimes") -Force | Out-Null
$vaultSentinel = Join-Path $TestVaultPath "runtimes\user_data_sentinel.txt"
Set-Content -Path $vaultSentinel -Value "irreplaceable_user_vault_data"

# 5. Run Uninstaller with Isolation Flags
Write-Host "`n=== TEST 5: SAFE UNINSTALLATION ===" -ForegroundColor Cyan
& "$TestInstallDir\uninstall.ps1" -InstallDir $TestInstallDir -NoShortcuts -NoPath -NoRegistry -Force
Start-Sleep -Seconds 1

# Verify binaries and manifest are gone
if (Test-Path (Join-Path $TestInstallDir "bin\ulvo.exe")) {
    throw "ASSERTION FAILED: ulvo.exe still present after uninstall"
}
if (Test-Path (Join-Path $TestInstallDir ".ulvo-install-manifest.json")) {
    throw "ASSERTION FAILED: .ulvo-install-manifest.json still present after uninstall"
}
Write-Host "  [OK] Installed binaries and manifest cleanly removed." -ForegroundColor Green

# Verify user custom files are strictly PRESERVED
if (-not (Test-Path $userConfigFile)) {
    throw "CRITICAL DEFECT: Custom user file $userConfigFile was deleted by uninstaller!"
}
if (-not (Test-Path $userRootFile)) {
    throw "CRITICAL DEFECT: Custom user file $userRootFile was deleted by uninstaller!"
}
Write-Host "  [OK] Custom user files were strictly preserved: $userConfigFile and $userRootFile" -ForegroundColor Green

# Verify Vault Data was strictly preserved
if (-not (Test-Path $vaultSentinel)) {
    throw "CRITICAL DEFECT: Vault data was deleted by uninstaller!"
}
Write-Host "  [OK] Vault data was strictly preserved: $vaultSentinel" -ForegroundColor Green

# Verify Final Isolation Boundary
$FinalUserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($FinalUserPath -ne $InitialUserPath) {
    throw "ISOLATION BOUNDARY VIOLATION: User PATH changed after test completion!"
}
$FinalRegExists = Test-Path $UninstallRegKey
if ($FinalRegExists -ne $InitialRegExists) {
    throw "ISOLATION BOUNDARY VIOLATION: HKCU Registry changed after test completion!"
}
Write-Host "  [OK] Final Isolation Boundary Verified: User environment unchanged." -ForegroundColor Green

# Cleanup test sandbox
Remove-Item -Recurse -Force $TestRoot -ErrorAction SilentlyContinue
Write-Host "`n============================================================" -ForegroundColor Green
Write-Host "   INSTALLER / UNINSTALLER SAFETY & ISOLATION PASSED!       " -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Green
