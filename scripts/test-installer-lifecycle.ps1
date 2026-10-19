$ErrorActionPreference = "Stop"

$TestRoot = Join-Path $env:TEMP "ulvo_installer_validation"
if (Test-Path $TestRoot) {
    Remove-Item -Recurse -Force $TestRoot -ErrorAction SilentlyContinue
}
$TestInstallDir = Join-Path $TestRoot "Programs\ULVO"
$TestVaultPath = Join-Path $TestRoot "vault"

Write-Host "=== TEST: INSTALLING ULVO TO ISOLATED TARGET ==="
Write-Host "Install Dir: $TestInstallDir"
Write-Host "Vault Path:  $TestVaultPath"

# 1. Run installer
& "$PSScriptRoot\install-ulvo.ps1" -InstallDir $TestInstallDir -VaultPath $TestVaultPath -NoShortcuts -NoPath
if ($LASTEXITCODE -ne 0) {
    throw "install-ulvo.ps1 failed with code $LASTEXITCODE"
}

# 2. Verify files installed
Write-Host "`n=== VERIFYING INSTALLED ARTIFACTS ==="
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

# 3. Test execution of installed binary
$out = & (Join-Path $BinDir "ulvo.exe") --version
Write-Host "  [OK] Installed binary output: $out" -ForegroundColor Green
if ($out -notmatch "ulvo 1.2.0") {
    throw "ASSERTION FAILED: Unexpected binary version: $out"
}

# 4. Verify manifest content
$manifestJson = Get-Content (Join-Path $TestInstallDir ".ulvo-install-manifest.json") -Raw | ConvertFrom-Json
if ($manifestJson.version -ne "1.2.0") {
    throw "ASSERTION FAILED: Manifest version mismatch"
}
Write-Host "  [OK] Installation manifest verified: Version $($manifestJson.version), $($manifestJson.files.PSObject.Properties.Count) files tracked" -ForegroundColor Green

# 5. Create a mock project in the test vault to prove vault preservation
New-Item -ItemType Directory -Path (Join-Path $TestVaultPath "runtimes") -Force | Out-Null
Set-Content -Path (Join-Path $TestVaultPath "runtimes\user_data_sentinel.txt") -Value "irreplaceable_user_vault_data"

# 6. Run uninstaller
Write-Host "`n=== TEST: UNINSTALLING FROM ISOLATED TARGET ==="
& "$TestInstallDir\uninstall.ps1" -InstallDir $TestInstallDir -Force
Start-Sleep -Seconds 2

# 7. Verify uninstallation result
if (Test-Path (Join-Path $TestInstallDir "bin\ulvo.exe")) {
    throw "ASSERTION FAILED: Binaries still present after uninstall"
}
Write-Host "  [OK] Installed binaries removed successfully." -ForegroundColor Green

# 8. Verify vault data was strictly preserved!
$sentinel = Join-Path $TestVaultPath "runtimes\user_data_sentinel.txt"
if (-not (Test-Path $sentinel)) {
    throw "CRITICAL FAILURE: Vault data was deleted during uninstall!"
}
Write-Host "  [OK] Vault preservation verified: $sentinel is intact!" -ForegroundColor Green

# Cleanup test root
Remove-Item -Recurse -Force $TestRoot -ErrorAction SilentlyContinue
Write-Host "`n=== INSTALLER/UNINSTALLER LIFECYCLE TEST PASSED! ===" -ForegroundColor Green
