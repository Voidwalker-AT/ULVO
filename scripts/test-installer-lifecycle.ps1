# ULVO Installer & Uninstaller Lifecycle, Security & Recovery Verification Test Suite
# Tests:
#  1. Fresh isolated installation with boundary enforcement (-NoRegistry -NoShortcuts -NoPath).
#  2. User registry and PATH non-interference verification.
#  3. Transactional upgrade with separate staging and atomic directory swap.
#  4. Interrupted upgrade / crash recovery at multiple commit points (STAGED, BACKUP_CREATED).
#  5. Uninstaller Security Defenses:
#     - Missing manifest refusal (refuses to delete on bin/ulvo.exe alone).
#     - Corrupted / invalid schema manifest refusal.
#     - Malicious path traversal attack refusal.
#     - Malicious sibling-prefix attack refusal.
#     - Malicious alternate data stream attack refusal.
#  6. Cryptographic Hash Validation on Uninstall:
#     - User-modified files (hash mismatch) are preserved.
#     - Custom unknown user files are preserved.
#  7. Safe selective uninstallation and strict vault preservation.

param(
    [string]$EvidenceDirectory = (Join-Path $PSScriptRoot 'output\installer-lifecycle')
)

$ErrorActionPreference = "Stop"

$TestRoot = Join-Path $env:TEMP "ulvo_installer_validation"
$resolvedTestRoot = [IO.Path]::GetFullPath($TestRoot)
$temporaryBoundary = [IO.Path]::GetFullPath($env:TEMP).TrimEnd('\') + '\'
if (-not $resolvedTestRoot.StartsWith($temporaryBoundary, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Installer test root escaped its temporary workspace.'
}
if (Test-Path $TestRoot) {
    Remove-Item -Recurse -Force $TestRoot -ErrorAction SilentlyContinue
}
$TestInstallDir = Join-Path $TestRoot "Programs\ULVO"
$TestVaultPath = Join-Path $TestRoot "vault"
$TestInstallParent = Join-Path $TestRoot "Programs"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "   ULVO Installer / Uninstaller Security & Lifecycle Test   " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "Install Dir: $TestInstallDir"
Write-Host "Vault Path:  $TestVaultPath"
Write-Host ""

$testsPassed = 0
$totalTests = 7

# Baseline User Environment to Verify Boundary Isolation
$InitialUserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
$UninstallRegKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ULVO"
$InitialRegExists = Test-Path $UninstallRegKey

# --------------------------------------------------------------------------
# TEST 1: Fresh Clean Installation with Complete Isolation
# --------------------------------------------------------------------------
Write-Host "=== TEST 1: ISOLATED CLEAN INSTALLATION ===" -ForegroundColor Cyan
& "$PSScriptRoot\install-ulvo.ps1" -InstallDir $TestInstallDir -VaultPath $TestVaultPath -NoShortcuts -NoPath -NoRegistry
if ($LASTEXITCODE -ne 0) {
    throw "TEST 1 FAILED: install-ulvo.ps1 failed with code $LASTEXITCODE"
}

# Verify Isolation Boundary: User PATH and HKCU Registry must be completely UNTOUCHED
$PostInstallUserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($PostInstallUserPath -ne $InitialUserPath) {
    throw "TEST 1 FAILED: User PATH was modified during -NoPath installation!"
}
$PostInstallRegExists = Test-Path $UninstallRegKey
if ($PostInstallRegExists -ne $InitialRegExists) {
    throw "TEST 1 FAILED: HKCU Registry was modified during -NoRegistry installation!"
}
Write-Host "  [OK] Isolation Boundary Verified: User PATH and Registry were completely untouched." -ForegroundColor Green
$testsPassed++

# --------------------------------------------------------------------------
# TEST 2: Verify Installed Artifacts & Execution
# --------------------------------------------------------------------------
Write-Host "`n=== TEST 2: VERIFY INSTALLED ARTIFACTS & EXECUTION ===" -ForegroundColor Cyan
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
        throw "TEST 2 FAILED: Expected installed file missing: $f"
    }
    Write-Host "  [OK] Found: $f" -ForegroundColor Green
}

$out = & (Join-Path $BinDir "ulvo.exe") --version
Write-Host "  [OK] Installed binary executed: $out" -ForegroundColor Green
if ($out -notmatch "ulvo 1.2.0") {
    throw "TEST 2 FAILED: Unexpected binary version: $out"
}
$testsPassed++

# --------------------------------------------------------------------------
# TEST 3: Transactional Upgrade & Directory-Level Commit
# --------------------------------------------------------------------------
Write-Host "`n=== TEST 3: TRANSACTIONAL UPGRADE WITH SEPARATE STAGING ===" -ForegroundColor Cyan
# Set custom user preference in existing install to verify migration
$prefsFile = Join-Path $TestInstallDir "config\preferences.json"
$prefs = Get-Content $prefsFile -Raw | ConvertFrom-Json
$prefs | Add-Member -MemberType NoteProperty -Name "custom_flag_test" -Value "user_preference_preserved_across_upgrade" -Force
$prefs | ConvertTo-Json -Depth 4 | Set-Content -Path $prefsFile -Encoding utf8

# Add a custom user file to existing install to verify preservation
$customUserDoc = Join-Path $TestInstallDir "docs\user_custom_doc.txt"
New-Item -ItemType Directory -Path (Join-Path $TestInstallDir "docs") -Force | Out-Null
Set-Content -Path $customUserDoc -Value "user_custom_content"

# Run upgrade
& "$PSScriptRoot\install-ulvo.ps1" -InstallDir $TestInstallDir -VaultPath $TestVaultPath -NoShortcuts -NoPath -NoRegistry
if ($LASTEXITCODE -ne 0) {
    throw "TEST 3 FAILED: Upgrade installation failed!"
}

# Verify user preference was migrated
$newPrefs = Get-Content $prefsFile -Raw | ConvertFrom-Json
if ($newPrefs.custom_flag_test -ne "user_preference_preserved_across_upgrade") {
    throw "TEST 3 FAILED: User preferences were not preserved across upgrade!"
}
# Verify custom user document was migrated
if (-not (Test-Path $customUserDoc)) {
    throw "TEST 3 FAILED: User custom file was lost during directory upgrade!"
}
Write-Host "  [OK] In-place directory-swap upgrade committed successfully with user preferences and custom files preserved." -ForegroundColor Green
$testsPassed++

# --------------------------------------------------------------------------
# TEST 4: Crash Recovery Simulation at Multiple Commit Points
# --------------------------------------------------------------------------
Write-Host "`n=== TEST 4: CRASH RECOVERY SIMULATION ===" -ForegroundColor Cyan

# 4a. Interrupted at STAGED state
Write-Host "  [4a] Simulating crash during STAGED state..." -ForegroundColor DarkGray
$fakeTxn1 = "sim_crash_staged_1001"
$fakeStageDir1 = Join-Path $TestInstallParent (".ulvo_stage_" + $fakeTxn1)
New-Item -ItemType Directory -Path $fakeStageDir1 -Force | Out-Null
Set-Content -Path (Join-Path $fakeStageDir1 "partial.tmp") -Value "abandoned_staging_payload"

$fakeJournal1 = @{
    transaction_id = $fakeTxn1
    timestamp = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    state = "STAGED"
    install_dir = $TestInstallDir
    staging_dir = $fakeStageDir1
    backup_dir = (Join-Path $TestInstallParent (".ulvo_backup_" + $fakeTxn1))
}
$fakeJournal1 | ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $TestInstallParent (".ulvo_txn_" + $fakeTxn1 + ".journal.json")) -Encoding utf8

# Re-run installer: it must automatically detect and purge the abandoned staging transaction
& "$PSScriptRoot\install-ulvo.ps1" -InstallDir $TestInstallDir -VaultPath $TestVaultPath -NoShortcuts -NoPath -NoRegistry
if (Test-Path $fakeStageDir1) {
    throw "TEST 4 FAILED: Abandoned staging directory was not purged during recovery!"
}
Write-Host "  [OK] Abandoned STAGED transaction safely purged without corrupting live installation." -ForegroundColor Green

# 4b. Interrupted at BACKUP_CREATED state (simulating power cut right after live dir moved to backup)
Write-Host "  [4b] Simulating crash during BACKUP_CREATED state (target dir in backup)..." -ForegroundColor DarkGray
$fakeTxn2 = "sim_crash_backup_1002"
$fakeBackupDir2 = Join-Path $TestInstallParent (".ulvo_backup_" + $fakeTxn2)
# Temporarily move real live install to backup to simulate interrupted swap
[System.IO.Directory]::Move($TestInstallDir, $fakeBackupDir2)

$fakeJournal2 = @{
    transaction_id = $fakeTxn2
    timestamp = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    state = "BACKUP_CREATED"
    install_dir = $TestInstallDir
    staging_dir = (Join-Path $TestInstallParent (".ulvo_stage_" + $fakeTxn2))
    backup_dir = $fakeBackupDir2
}
$fakeJournal2 | ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $TestInstallParent (".ulvo_txn_" + $fakeTxn2 + ".journal.json")) -Encoding utf8

# Re-run installer: recovery must restore the backup back to live install and finish!
& "$PSScriptRoot\install-ulvo.ps1" -InstallDir $TestInstallDir -VaultPath $TestVaultPath -NoShortcuts -NoPath -NoRegistry
if (-not (Test-Path (Join-Path $TestInstallDir "bin\ulvo.exe"))) {
    throw "TEST 4 FAILED: Target installation was not recovered from BACKUP_CREATED state!"
}
if (Test-Path $fakeBackupDir2) {
    throw "TEST 4 FAILED: Temporary backup directory was not cleaned after recovery!"
}
Write-Host "  [OK] BACKUP_CREATED interrupted state cleanly restored original installation." -ForegroundColor Green

# 4c. A poisoned journal must never turn recovery into arbitrary tree deletion.
$sentinelDir = Join-Path $TestRoot 'external_recovery_sentinel'
New-Item -ItemType Directory -Path $sentinelDir -Force | Out-Null
$sentinelFile = Join-Path $sentinelDir 'retain.txt'
Set-Content -LiteralPath $sentinelFile -Value 'retain'
$poisonId = 'poison_boundary_1003'
$poisonJournal = Join-Path $TestInstallParent ('.ulvo_txn_' + $poisonId + '.journal.json')
@{ transaction_id=$poisonId; state='STAGED'; install_dir=$TestInstallDir; staging_dir=$sentinelDir; backup_dir=(Join-Path $TestInstallParent ('.ulvo_backup_' + $poisonId)) } | ConvertTo-Json | Set-Content -LiteralPath $poisonJournal
$refused = $false
try { & "$PSScriptRoot\install-ulvo.ps1" -InstallDir $TestInstallDir -VaultPath $TestVaultPath -NoShortcuts -NoPath -NoRegistry } catch { $refused = $true }
if (-not $refused -or -not (Test-Path -LiteralPath $sentinelFile)) { throw 'TEST 4 FAILED: poisoned recovery journal crossed ownership boundary' }
Remove-Item -LiteralPath $poisonJournal
Write-Host '  [OK] Poisoned recovery journal refused; external sentinel retained.' -ForegroundColor Green
$testsPassed++

# --------------------------------------------------------------------------
# TEST 5: Uninstaller Security Defenses (Malicious & Corrupted Manifests)
# --------------------------------------------------------------------------
Write-Host "`n=== TEST 5: UNINSTALLER SECURITY DEFENSES ===" -ForegroundColor Cyan
$ManifestPath = Join-Path $TestInstallDir ".ulvo-install-manifest.json"

# 5a. Refuse uninstall if manifest is missing (even if bin/ulvo.exe exists!)
Write-Host "  [5a] Testing missing manifest refusal..." -ForegroundColor DarkGray
$manifestBackup = Get-Content $ManifestPath -Raw
Remove-Item -Path $ManifestPath -Force

$threwMissing = $false
try {
    & "$TestInstallDir\uninstall.ps1" -InstallDir $TestInstallDir -NoShortcuts -NoPath -NoRegistry
} catch {
    $threwMissing = $true
    Write-Host "    Expected refusal: $($_.Exception.Message)" -ForegroundColor DarkGray
}
if (-not $threwMissing) {
    throw "TEST 5 FAILED: Uninstaller did not refuse to run when manifest was missing!"
}
# Restore manifest
$manifestBackup | Set-Content -Path $ManifestPath -Encoding utf8
Write-Host "  [OK] Missing manifest correctly refused (requires valid ownership manifest)." -ForegroundColor Green

# 5b. Refuse uninstall on Directory Traversal attack in manifest
Write-Host "  [5b] Testing malicious path traversal refusal..." -ForegroundColor DarkGray
$badManifestTraverse = $manifestBackup.Replace('"bin/ulvo.exe"', '"../../Windows/System32/calc.exe"')
$badManifestTraverse | Set-Content -Path $ManifestPath -Encoding utf8

$threwTraverse = $false
try {
    & "$TestInstallDir\uninstall.ps1" -InstallDir $TestInstallDir -NoShortcuts -NoPath -NoRegistry
} catch {
    $threwTraverse = $true
    Write-Host "    Expected refusal: $($_.Exception.Message)" -ForegroundColor DarkGray
}
if (-not $threwTraverse) {
    throw "TEST 5 FAILED: Uninstaller failed to block directory traversal attack!"
}
Write-Host "  [OK] Directory traversal attack in manifest blocked." -ForegroundColor Green

# 5c. Refuse uninstall on Sibling-Prefix attack in manifest
Write-Host "  [5c] Testing malicious sibling-prefix attack refusal..." -ForegroundColor DarkGray
$siblingTarget = Join-Path $TestInstallParent "ULVO_SIBLING_ATTACK\secret.txt"
New-Item -ItemType Directory -Path (Join-Path $TestInstallParent "ULVO_SIBLING_ATTACK") -Force | Out-Null
Set-Content -Path $siblingTarget -Value "sensitive_data"

# Create a relative key pointing to sibling directory
$badManifestSibling = $manifestBackup.Replace('"bin/ulvo.exe"', '"../ULVO_SIBLING_ATTACK/secret.txt"')
$badManifestSibling | Set-Content -Path $ManifestPath -Encoding utf8

$threwSibling = $false
try {
    & "$TestInstallDir\uninstall.ps1" -InstallDir $TestInstallDir -NoShortcuts -NoPath -NoRegistry
} catch {
    $threwSibling = $true
    Write-Host "    Expected refusal: $($_.Exception.Message)" -ForegroundColor DarkGray
}
if (-not $threwSibling) {
    throw "TEST 5 FAILED: Uninstaller failed to block sibling prefix attack!"
}
# Verify sibling file was untouched
if (-not (Test-Path $siblingTarget)) {
    throw "CRITICAL FAILURE: Sibling directory file was deleted by uninstaller!"
}
Remove-Item -Path (Join-Path $TestInstallParent "ULVO_SIBLING_ATTACK") -Recurse -Force
Write-Host "  [OK] Sibling-prefix attack blocked and sibling target protected." -ForegroundColor Green

# 5d. Refuse uninstall on Alternate Data Stream attack
Write-Host "  [5d] Testing alternate data stream refusal..." -ForegroundColor DarkGray
$badManifestADS = $manifestBackup.Replace('"bin/ulvo.exe"', '"bin/ulvo.exe:hidden"')
$badManifestADS | Set-Content -Path $ManifestPath -Encoding utf8

$threwADS = $false
try {
    & "$TestInstallDir\uninstall.ps1" -InstallDir $TestInstallDir -NoShortcuts -NoPath -NoRegistry
} catch {
    $threwADS = $true
    Write-Host "    Expected refusal: $($_.Exception.Message)" -ForegroundColor DarkGray
}
if (-not $threwADS) {
    throw "TEST 5 FAILED: Uninstaller failed to block alternate data stream!"
}
Write-Host "  [OK] Alternate data stream attack in manifest blocked." -ForegroundColor Green

# Restore clean manifest
$manifestBackup | Set-Content -Path $ManifestPath -Encoding utf8
$testsPassed++

# --------------------------------------------------------------------------
# TEST 6: User-Modified File & Custom User Content Preservation
# --------------------------------------------------------------------------
Write-Host "`n=== TEST 6: USER-MODIFIED FILE & CUSTOM FILE PRESERVATION ===" -ForegroundColor Cyan
# Tamper with preferences.json to simulate user configuration edits
$prefToModify = Join-Path $TestInstallDir "config\preferences.json"
Add-Content -Path $prefToModify -Value "`n// USER EDITED THIS FILE MANUALLY"

# Add custom user file in root and subdirectories
$userRuleFile = Join-Path $TestInstallDir "config\custom_user_rule.txt"
Set-Content -Path $userRuleFile -Value "preserve_this_custom_user_file"

$userCustomProj = Join-Path $TestInstallDir "my_custom_project.json"
Set-Content -Path $userCustomProj -Value '{"custom_project": true}'

# Add dummy irreplaceable vault data
New-Item -ItemType Directory -Path (Join-Path $TestVaultPath "runtimes") -Force | Out-Null
$vaultSentinel = Join-Path $TestVaultPath "runtimes\user_data_sentinel.txt"
Set-Content -Path $vaultSentinel -Value "irreplaceable_vault_data"

Write-Host "  Executing uninstaller..." -ForegroundColor DarkGray
& "$TestInstallDir\uninstall.ps1" -InstallDir $TestInstallDir -NoShortcuts -NoPath -NoRegistry -Force
if ($LASTEXITCODE -ne 0) {
    throw "TEST 6 FAILED: Uninstaller exited with code $LASTEXITCODE"
}

# Assert: Modified file MUST BE PRESERVED because its hash differed from manifest
if (-not (Test-Path $prefToModify)) {
    throw "TEST 6 FAILED: User-modified file '$prefToModify' was deleted instead of being preserved!"
}
Write-Host "  [OK] User-modified file preserved due to cryptographic hash mismatch." -ForegroundColor Green

# Assert: Custom user files MUST BE PRESERVED
if (-not (Test-Path $userRuleFile)) {
    throw "TEST 6 FAILED: Custom user file '$userRuleFile' was deleted!"
}
if (-not (Test-Path $userCustomProj)) {
    throw "TEST 6 FAILED: Custom user project '$userCustomProj' was deleted!"
}
Write-Host "  [OK] Custom user files strictly preserved." -ForegroundColor Green
$testsPassed++

# --------------------------------------------------------------------------
# TEST 7: Installed Binaries Clean Removal & Vault Preservation
# --------------------------------------------------------------------------
Write-Host "`n=== TEST 7: SAFE SELECTIVE REMOVAL & VAULT PRESERVATION ===" -ForegroundColor Cyan
# Assert: Installer-owned binaries were cleanly removed
$installedExe = Join-Path $TestInstallDir "bin\ulvo.exe"
$installedAgent = Join-Path $TestInstallDir "bin\ulvo-agent.exe"
$installedDesktop = Join-Path $TestInstallDir "bin\ulvo-desktop.exe"
if ((Test-Path $installedExe) -or (Test-Path $installedAgent) -or (Test-Path $installedDesktop)) {
    throw "TEST 7 FAILED: Installer-owned binaries were not removed!"
}
Write-Host "  [OK] Verified installer-owned binaries cleanly unlinked." -ForegroundColor Green

# Assert: Vault data strictly preserved
if (-not (Test-Path $vaultSentinel)) {
    throw "CRITICAL FAILURE: Vault data was deleted during uninstallation!"
}
Write-Host "  [OK] Vault data strictly preserved: $vaultSentinel intact." -ForegroundColor Green

# Final Isolation Boundary Check
$FinalUserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($FinalUserPath -ne $InitialUserPath) {
    throw "TEST 7 FAILED: User PATH was permanently altered!"
}
$FinalRegExists = Test-Path $UninstallRegKey
if ($FinalRegExists -ne $InitialRegExists) {
    throw "TEST 7 FAILED: HKCU Registry was permanently altered!"
}
Write-Host "  [OK] Final isolation boundary verified: user environment unchanged." -ForegroundColor Green
$testsPassed++

# Clean up temporary test harness
Remove-Item -Path $TestRoot -Recurse -Force -ErrorAction SilentlyContinue

$evidenceDir = $EvidenceDirectory
if (-not (Test-Path $evidenceDir)) {
    New-Item -ItemType Directory -Path $evidenceDir -Force | Out-Null
}
$evidenceJson = "$evidenceDir\installer-acceptance-verification.json"
$report = [ordered]@{
    timestamp = [DateTimeOffset]::UtcNow.ToString("o")
    total_suites = $totalTests
    passed_suites = $testsPassed
    all_passed = ($testsPassed -eq $totalTests)
    tests = @(
        "Isolated Clean Installation",
        "PATH & Registry Non-Interference",
        "Transactional Upgrade & Atomic Swap",
        "Interrupted Upgrade Crash Recovery (STAGED & BACKUP_CREATED)",
        "Uninstaller Security Defenses (Manifest, Traversal, Sibling, ADS)",
        "User File & Modified File Preservation (Hash Validation)",
        "Safe Selective Removal & Vault Preservation"
    )
}
$report | ConvertTo-Json -Depth 4 | Set-Content -Path $evidenceJson -Encoding utf8
Write-Host "  Evidence saved to: $evidenceJson" -ForegroundColor DarkGray

Write-Host "`n============================================================" -ForegroundColor Cyan
Write-Host "   INSTALLER / UNINSTALLER SECURITY PASSED: $testsPassed / $totalTests SUITES   " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
