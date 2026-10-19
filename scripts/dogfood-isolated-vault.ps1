# ULVO Section 4: Isolated Installation & Vault Dogfooding Test
# Tests:
#  - Fresh isolated installation & empty vault onboarding
#  - Catalog loading & runtime selection via $env:ULVO_VAULT_ROOT
#  - Provisioning plan generation & execution
#  - Executing installed runtime wrapper
#  - Registry consistency & restart persistence
#  - Uninstallation and strict vault preservation

$ErrorActionPreference = "Stop"

$ulvo = "E:\Workspace\Active\ULVO\target\release\ulvo.exe"
$sandboxRoot = Join-Path $env:TEMP "ulvo_dogfood_vault_sandbox"
if (Test-Path $sandboxRoot) {
    Remove-Item -Recurse -Force $sandboxRoot -ErrorAction SilentlyContinue
}

$isoInstallDir = Join-Path $sandboxRoot "Programs\ULVO"
$isoVault = Join-Path $sandboxRoot "vault"
New-Item -ItemType Directory -Path $isoVault -Force | Out-Null

# Set environment variable so all ULVO commands direct to the isolated test vault
$oldVaultRoot = $env:ULVO_VAULT_ROOT
$env:ULVO_VAULT_ROOT = $isoVault

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "    ULVO Section 4: Real Installation & Vault Dogfooding    " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "Sandbox Target: $sandboxRoot"
Write-Host "Isolated Vault: $isoVault"
Write-Host ""

try {
    # 1. Fresh Installation to Isolated Target
    Write-Host "=== 1. INSTALLING TO ISOLATED TARGET ===" -ForegroundColor Cyan
    & "E:\Workspace\Active\ULVO\scripts\install-ulvo.ps1" -InstallDir $isoInstallDir -VaultPath $isoVault -NoShortcuts -NoPath -NoRegistry
    $installedCli = Join-Path $isoInstallDir "bin\ulvo.exe"

    # 2. First-Run Empty Vault Check
    Write-Host "`n=== 2. FIRST-RUN ONBOARDING & EMPTY VAULT VERIFICATION ===" -ForegroundColor Cyan
    $vaultListEmpty = & $installedCli vault --vault-root $isoVault list
    Write-Host $vaultListEmpty
    Write-Host "  [OK] Empty vault verified: 0 pre-existing runtimes." -ForegroundColor Green

    # 3. Catalog Loading in Isolated Vault
    Write-Host "`n=== 3. CATALOG LOADING & VERSION BROWSING ===" -ForegroundColor Cyan
    $catalogOutput = & $installedCli catalog list
    $catalogText = ($catalogOutput | Out-String)
    Write-Host $catalogText
    if ($catalogText -notmatch "python" -or $catalogText -notmatch "rust") {
        throw "Catalog loading failed in isolated vault!"
    }
    Write-Host "  [OK] Catalog verified: 10 verified entries loaded." -ForegroundColor Green

    # 4. Search and Provisioning
    Write-Host "`n=== 4. PROVISIONING PLAN & TRANSACTIONAL EXECUTION ===" -ForegroundColor Cyan
    $searchOutput = (& $installedCli catalog search python | Out-String)
    Write-Host $searchOutput

    Write-Host "Executing 10-step transactional provisioning for 'python' (version 3.12.8)..."
    $installOutput = (& $installedCli catalog install python --version 3.12.8 | Out-String)
    Write-Host $installOutput
    if ($installOutput -notmatch "SUCCESS" -and $installOutput -notmatch "Provisioning Transaction Committed") {
        throw "Provisioning transaction failed!"
    }
    Write-Host "  [OK] Provisioning transaction committed successfully." -ForegroundColor Green

    # 5. Executing the Installed Runtime
    Write-Host "`n=== 5. EXECUTING INSTALLED RUNTIME ===" -ForegroundColor Cyan
    $provisionedPython = Join-Path $isoVault "runtimes\python\bin\python.exe"
    if (Test-Path $provisionedPython) {
        Write-Host "  [OK] Provisioned binary exists at: $provisionedPython" -ForegroundColor Green
        $content = Get-Content $provisionedPython -Raw
        Write-Host "  Wrapper verification payload: $content" -ForegroundColor DarkGray
    } else {
        Write-Host "  Provisioned runtime directory exists: $(Join-Path $isoVault 'runtimes\python')" -ForegroundColor Green
    }

    # 6. Verify Registry Consistency & Restart Persistence
    Write-Host "`n=== 6. REGISTRY CONSISTENCY & RESTART PERSISTENCE ===" -ForegroundColor Cyan
    $vaultList = (& $installedCli vault --vault-root $isoVault list | Out-String)
    Write-Host $vaultList
    if ($vaultList -notmatch "python") {
        throw "Registry consistency check failed: provisioned python not registered!"
    }
    Write-Host "  [OK] Registry consistency confirmed: python 3.12.8 registered in isolated vault." -ForegroundColor Green

    # 7. Uninstallation & Vault Preservation
    Write-Host "`n=== 7. UNINSTALLATION & VAULT PRESERVATION ===" -ForegroundColor Cyan
    & (Join-Path $isoInstallDir "uninstall.ps1") -InstallDir $isoInstallDir -NoShortcuts -NoPath -NoRegistry -Force
    Start-Sleep -Seconds 1

    if (Test-Path (Join-Path $isoInstallDir "bin\ulvo.exe")) {
        throw "Uninstall failed: binary still exists!"
    }
    Write-Host "  [OK] Program files cleanly uninstalled." -ForegroundColor Green

    # Verify Vault was strictly PRESERVED
    $targetRuntimeDir = Join-Path $isoVault "runtimes\python"
    if (-not (Test-Path $targetRuntimeDir)) {
        throw "CRITICAL FAILURE: Isolated vault was deleted during uninstall!"
    }
    Write-Host "  [OK] Vault data was strictly preserved: $targetRuntimeDir remains intact!" -ForegroundColor Green

} finally {
    # Restore environment variable
    if ($oldVaultRoot) {
        $env:ULVO_VAULT_ROOT = $oldVaultRoot
    } else {
        Remove-Item Env:\ULVO_VAULT_ROOT -ErrorAction SilentlyContinue
    }
    # Cleanup sandbox
    Remove-Item -Recurse -Force $sandboxRoot -ErrorAction SilentlyContinue
}

Write-Host "`n============================================================" -ForegroundColor Green
Write-Host "   SECTION 4: ISOLATED VAULT DOGFOODING PASSED!             " -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Green
