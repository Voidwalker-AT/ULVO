# ULVO Hardened Windows Installer (Per-User, Zero Admin Required)
# Implements atomic directory-level transactional upgrade, separate staging, journaling, and recovery.

[CmdletBinding()]
param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\ULVO",
    [string]$VaultPath = "",
    [switch]$NoShortcuts,
    [switch]$NoRegistry,
    [switch]$NoPath,
    [switch]$Force
)

$ErrorActionPreference = "Stop"

# 0. Resolve canonical paths and validate destination
if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    throw "InstallDir cannot be empty."
}

$ResolvedInstall = [System.IO.Path]::GetFullPath($InstallDir).TrimEnd('\')
$InstallParent = [System.IO.Path]::GetDirectoryName($ResolvedInstall)

if ([string]::IsNullOrWhiteSpace($InstallParent)) {
    throw "CRITICAL SAFETY REFUSAL: Invalid installation parent directory for $ResolvedInstall"
}

# Protected Roots Validation
$RootDrives = [System.IO.DriveInfo]::GetDrives() | ForEach-Object { $_.RootDirectory.FullName.TrimEnd('\') }
foreach ($drive in $RootDrives) {
    if ($ResolvedInstall.Equals($drive, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "CRITICAL SAFETY REFUSAL: Refusing to install into root directory $ResolvedInstall"
    }
}

$ProtectedRoots = @(
    [Environment]::GetFolderPath("ProgramFiles"),
    [Environment]::GetFolderPath("ProgramFilesX86"),
    [Environment]::GetFolderPath("Windows"),
    [Environment]::GetFolderPath("System"),
    $env:LOCALAPPDATA,
    $env:APPDATA,
    $env:USERPROFILE
)

foreach ($prot in $ProtectedRoots) {
    if ([string]::IsNullOrWhiteSpace($prot)) { continue }
    if ($ResolvedInstall.Equals([System.IO.Path]::GetFullPath($prot).TrimEnd('\'), [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "CRITICAL SAFETY REFUSAL: Refusing to install directly into protected system directory $ResolvedInstall"
    }
}

# Resolve VaultPath cleanly
if ([string]::IsNullOrWhiteSpace($VaultPath)) {
    if (Test-Path "E:\Workspace\Active\ULVO\vault") {
        $VaultPath = "E:\Workspace\Active\ULVO\vault"
    } else {
        $VaultPath = Join-Path $env:LOCALAPPDATA "ULVO\vault"
    }
}

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "     ULVO Windows Desktop & Gateway Installer v1.2.0        " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "Install Target: $ResolvedInstall"
Write-Host "Vault Target:   $VaultPath"
Write-Host ""

# Ensure parent directory exists
if (-not (Test-Path $InstallParent)) {
    New-Item -ItemType Directory -Path $InstallParent -Force | Out-Null
}

# 1. Recover any interrupted transactions from previous runs
function Recover-InterruptedTransactions {
    param([string]$ParentDir, [string]$TargetDir)
    
    $journalFiles = Get-ChildItem -Path $ParentDir -Filter ".ulvo_txn_*.journal.json" -File -ErrorAction SilentlyContinue
    foreach ($jFile in $journalFiles) {
        Write-Host "==> Found transaction journal from previous run: $($jFile.Name)" -ForegroundColor Yellow
        try {
            $jData = Get-Content $jFile.FullName -Raw | ConvertFrom-Json
            $stageDir = $jData.staging_dir
            $backupDir = $jData.backup_dir
            $state = $jData.state

            Write-Host "  Transaction State: $state" -ForegroundColor DarkGray
            if ($state -eq "BACKUP_CREATED" -or $state -eq "SWAPPING") {
                if ((Test-Path $backupDir) -and (-not (Test-Path $TargetDir))) {
                    Write-Host "  Restoring previous installation from backup: $backupDir -> $TargetDir" -ForegroundColor Yellow
                    [System.IO.Directory]::Move($backupDir, $TargetDir)
                }
            }

            if (Test-Path $stageDir) {
                Remove-Item -Path $stageDir -Recurse -Force -ErrorAction SilentlyContinue
            }
            if (Test-Path $backupDir) {
                Remove-Item -Path $backupDir -Recurse -Force -ErrorAction SilentlyContinue
            }
            Remove-Item -Path $jFile.FullName -Force -ErrorAction SilentlyContinue
            Write-Host "  Interrupted transaction recovered cleanly." -ForegroundColor Green
        } catch {
            Write-Host "  Warning during journal recovery: $_" -ForegroundColor Yellow
        }
    }

    # Clean orphaned staging or backup directories without journals older than 10 minutes
    $orphans = Get-ChildItem -Path $ParentDir -Directory -Filter ".ulvo_*" -ErrorAction SilentlyContinue
    foreach ($orph in $orphans) {
        if ($orph.Name -match "^\.ulvo_(stage|backup)_") {
            Write-Host "  Purging orphaned temporary directory: $($orph.Name)" -ForegroundColor DarkGray
            Remove-Item -Path $orph.FullName -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

Recover-InterruptedTransactions -ParentDir $InstallParent -TargetDir $ResolvedInstall

# 2. Verify Ownership of Existing Target
$HasExistingInstall = Test-Path $ResolvedInstall
if ($HasExistingInstall) {
    $existingManifest = Join-Path $ResolvedInstall ".ulvo-install-manifest.json"
    if (-not (Test-Path $existingManifest)) {
        $existingItems = Get-ChildItem -Path $ResolvedInstall -Force
        if ($existingItems.Count -gt 0 -and (-not $Force)) {
            throw "SAFETY REFUSAL: Target directory $ResolvedInstall exists, is non-empty, but lacks an official ULVO installation manifest (.ulvo-install-manifest.json). Refusing to overwrite foreign directory without -Force."
        }
    } else {
        try {
            $mJson = Get-Content $existingManifest -Raw | ConvertFrom-Json
            if ($mJson.product -ne "ULVO") {
                throw "SAFETY REFUSAL: Target directory manifest product is '$($mJson.product)', expected 'ULVO'."
            }
        } catch {
            if (-not $Force) {
                throw "SAFETY REFUSAL: Target directory manifest is corrupted. Re-run with -Force to proceed with recovery upgrade."
            }
        }
    }
}

# 3. Locate Source Binaries
$SourceBinDir = Join-Path (Get-Item $PSScriptRoot).Parent.FullName "target\release"
if (-not (Test-Path (Join-Path $SourceBinDir "ulvo.exe"))) {
    $SourceBinDir = Join-Path $PSScriptRoot "..\bin"
}
if (-not (Test-Path (Join-Path $SourceBinDir "ulvo.exe"))) {
    $SourceBinDir = Join-Path $PSScriptRoot "bin"
}

if (-not (Test-Path (Join-Path $SourceBinDir "ulvo.exe"))) {
    throw "Source binaries not found. Please build release binaries or run from release archive."
}

$RequiredBinaries = @("ulvo.exe", "ulvo-agent.exe", "ulvo-desktop.exe")
foreach ($bin in $RequiredBinaries) {
    $binPath = Join-Path $SourceBinDir $bin
    if (-not (Test-Path $binPath)) {
        throw "Required binary missing: $binPath"
    }
}

# 4. Check for running processes before upgrading
$BinDir = Join-Path $ResolvedInstall "bin"
if (Test-Path $BinDir) {
    $running = Get-Process | Where-Object { $_.Path -and $_.Path.StartsWith($BinDir, [System.StringComparison]::OrdinalIgnoreCase) }
    if ($running) {
        Write-Host "==> Detected running ULVO processes. Attempting graceful shutdown..." -ForegroundColor Yellow
        $installedCli = Join-Path $BinDir "ulvo.exe"
        if (Test-Path $installedCli) {
            & $installedCli agent stop *>$null
            Start-Sleep -Milliseconds 500
        }
        $remaining = Get-Process | Where-Object { $_.Path -and $_.Path.StartsWith($BinDir, [System.StringComparison]::OrdinalIgnoreCase) }
        if ($remaining) {
            if ($Force) {
                Write-Host "  Force-stopping remaining processes..." -ForegroundColor Yellow
                $remaining | Stop-Process -Force
            } else {
                throw "Active ULVO processes are running from $ResolvedInstall. Please close ULVO Control Center or re-run with -Force."
            }
        }
    }
}

# 5. Transaction Setup: Separate Sibling Staging
$TxnId = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds().ToString() + "_" + [System.Guid]::NewGuid().ToString("N").Substring(0, 8)
$StagingDir = Join-Path $InstallParent (".ulvo_stage_" + $TxnId)
$BackupDir = Join-Path $InstallParent (".ulvo_backup_" + $TxnId)
$JournalPath = Join-Path $InstallParent (".ulvo_txn_" + $TxnId + ".journal.json")

$Journal = @{
    transaction_id = $TxnId
    timestamp = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    state = "INIT"
    install_dir = $ResolvedInstall
    staging_dir = $StagingDir
    backup_dir = $BackupDir
}

function Save-Journal {
    param([hashtable]$j)
    $j | ConvertTo-Json -Depth 4 | Set-Content -Path $JournalPath -Encoding utf8
}

Save-Journal $Journal

try {
    # 6. Stage Complete Application Version Separately
    Write-Host "==> Staging complete new version in separate directory: $StagingDir..." -ForegroundColor Cyan
    New-Item -ItemType Directory -Path (Join-Path $StagingDir "bin") -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $StagingDir "config") -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $StagingDir "scripts") -Force | Out-Null

    # Copy binaries to staging
    foreach ($bin in $RequiredBinaries) {
        Copy-Item (Join-Path $SourceBinDir $bin) -Destination (Join-Path $StagingDir "bin\$bin") -Force
    }
    # Optional WebView2Loader.dll
    $webLoader = Join-Path $SourceBinDir "WebView2Loader.dll"
    if (Test-Path $webLoader) {
        Copy-Item $webLoader -Destination (Join-Path $StagingDir "bin\WebView2Loader.dll") -Force
    }

    # Copy uninstaller script to staging
    $SourceUninstall = Join-Path $PSScriptRoot "uninstall-ulvo.ps1"
    if (Test-Path $SourceUninstall) {
        Copy-Item $SourceUninstall -Destination (Join-Path $StagingDir "uninstall.ps1") -Force
        Copy-Item $SourceUninstall -Destination (Join-Path $StagingDir "scripts\uninstall-ulvo.ps1") -Force
    }

    # Launcher batch script
    @"
@echo off
start "" "%~dp0bin\ulvo-desktop.exe"
"@ | Set-Content (Join-Path $StagingDir "ULVO Control Center.bat") -Encoding ascii

    # Default Preferences template
    $Prefs = @{
        vault_path = $VaultPath
        safety_reserve_mb = 500
        cache_budget_gb = 20.0
        auto_update = $false
        enable_gateway_path = (-not $NoPath)
        enable_background_agent = $true
    } | ConvertTo-Json -Depth 4
    $Prefs | Set-Content -Path (Join-Path $StagingDir "config\preferences.json") -Encoding utf8

    # Build Installation Manifest with cryptographic SHA-256 hashes
    $ManifestFiles = @{}
    Get-ChildItem -Path $StagingDir -Recurse -File | ForEach-Object {
        $rel = $_.FullName.Substring($StagingDir.Length + 1).Replace("\", "/")
        $hash = (Get-FileHash -Path $_.FullName -Algorithm SHA256).Hash.ToLower()
        $ManifestFiles[$rel] = $hash
    }

    $InstallManifest = @{
        schema_version = 1
        product = "ULVO"
        version = "1.2.0"
        install_timestamp = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
        install_dir = $ResolvedInstall
        vault_path = $VaultPath
        files = $ManifestFiles
    } | ConvertTo-Json -Depth 4
    $InstallManifest | Set-Content -Path (Join-Path $StagingDir ".ulvo-install-manifest.json") -Encoding utf8

    # Verify all staged binaries exist and are non-empty
    foreach ($bin in $RequiredBinaries) {
        $stagedBin = Join-Path $StagingDir "bin\$bin"
        if (-not (Test-Path $stagedBin)) {
            throw "Staging verification failed: missing $bin"
        }
        $sz = (Get-Item $stagedBin).Length
        if ($sz -le 1024) {
            throw "Staging verification failed: $bin size is suspiciously small ($sz bytes)"
        }
    }

    $Journal.state = "STAGED"
    Save-Journal $Journal
    Write-Host "  Staged version verified cleanly." -ForegroundColor Green

    # 7. Atomic Commit Protocol using Same-Volume Directory Moves
    Write-Host "==> Committing new version to $ResolvedInstall..." -ForegroundColor Cyan

    if ($HasExistingInstall) {
        # Preserve user custom files and preferences from old install
        $oldUserPrefs = Join-Path $ResolvedInstall "config\preferences.json"
        $savedUserPrefs = $null
        if (Test-Path $oldUserPrefs) {
            $savedUserPrefs = Get-Content $oldUserPrefs -Raw
        }

        # Step 7a: Move existing install directory to backup
        $Journal.state = "SWAPPING"
        Save-Journal $Journal
        [System.IO.Directory]::Move($ResolvedInstall, $BackupDir)
        $Journal.state = "BACKUP_CREATED"
        Save-Journal $Journal

        # Step 7b: Move staging directory into place
        [System.IO.Directory]::Move($StagingDir, $ResolvedInstall)
        $Journal.state = "COMMITTED"
        Save-Journal $Journal

        # Restore custom user preferences if present
        if ($savedUserPrefs) {
            $newPrefsPath = Join-Path $ResolvedInstall "config\preferences.json"
            $savedUserPrefs | Set-Content -Path $newPrefsPath -Encoding utf8
        }

        # Migrate any user-created non-installer files from backup to new install
        $backupItems = Get-ChildItem -Path $BackupDir -Recurse -File -ErrorAction SilentlyContinue
        $oldManifestPath = Join-Path $BackupDir ".ulvo-install-manifest.json"
        $oldManifest = $null
        if (Test-Path $oldManifestPath) {
            try { $oldManifest = Get-Content $oldManifestPath -Raw | ConvertFrom-Json } catch {}
        }
        if ($oldManifest -and $oldManifest.files) {
            $knownOldFiles = @($oldManifest.files.PSObject.Properties | ForEach-Object { $_.Name.Replace("/", "\") })
            $knownOldFiles += @("uninstall.ps1", "scripts\uninstall-ulvo.ps1", ".ulvo-install-manifest.json", "ULVO Control Center.bat")
            foreach ($bFile in $backupItems) {
                $rel = $bFile.FullName.Substring($BackupDir.Length + 1)
                if ($knownOldFiles -notcontains $rel -and -not $rel.StartsWith(".rollback_") -and -not $rel.StartsWith(".staging_")) {
                    $destUserFile = Join-Path $ResolvedInstall $rel
                    $destParent = [System.IO.Path]::GetDirectoryName($destUserFile)
                    if (-not (Test-Path $destParent)) { New-Item -ItemType Directory -Path $destParent -Force | Out-Null }
                    Copy-Item $bFile.FullName -Destination $destUserFile -Force
                    Write-Host "  Migrated custom user file: $rel" -ForegroundColor DarkGray
                }
            }
        }

        # Remove backup directory on successful commit
        Remove-Item -Path $BackupDir -Recurse -Force -ErrorAction SilentlyContinue

    } else {
        # First-time installation: simple directory move
        [System.IO.Directory]::Move($StagingDir, $ResolvedInstall)
        $Journal.state = "COMMITTED"
        Save-Journal $Journal
    }

    # Step 7c: Post-Commit Self-Test
    $installedCli = Join-Path $ResolvedInstall "bin\ulvo.exe"
    $testRun = & $installedCli --version
    if ($LASTEXITCODE -ne 0) {
        throw "Installed binary execution verification failed with exit code $LASTEXITCODE"
    }
    Write-Host "  Commit verified: $testRun" -ForegroundColor Green

    # Step 7d: Clean Journal
    $Journal.state = "COMPLETED"
    Remove-Item -Path $JournalPath -Force -ErrorAction SilentlyContinue

} catch {
    Write-Host "ERROR during installation transaction: $_" -ForegroundColor Red
    Write-Host "==> Executing rollback..." -ForegroundColor Yellow

    # If staging directory still exists, clean it
    if (Test-Path $StagingDir) {
        Remove-Item -Path $StagingDir -Recurse -Force -ErrorAction SilentlyContinue
    }

    # If old install was moved to backup, restore it
    if ((Test-Path $BackupDir) -and (-not (Test-Path $ResolvedInstall))) {
        Write-Host "  Restoring original installation from $BackupDir..." -ForegroundColor Yellow
        [System.IO.Directory]::Move($BackupDir, $ResolvedInstall)
        Write-Host "  Restoration completed: original installation restored." -ForegroundColor Green
    } elseif ((Test-Path $BackupDir) -and (Test-Path $ResolvedInstall)) {
        # Swap failed midway: remove broken target, restore backup
        Remove-Item -Path $ResolvedInstall -Recurse -Force -ErrorAction SilentlyContinue
        [System.IO.Directory]::Move($BackupDir, $ResolvedInstall)
        Write-Host "  Restoration completed: reverted to original installation." -ForegroundColor Green
    }

    Remove-Item -Path $JournalPath -Force -ErrorAction SilentlyContinue
    throw "Transactional installation failed and was rolled back cleanly: $_"
}

# 8. User PATH Registration (Conditional)
$InstalledBinDir = Join-Path $ResolvedInstall "bin"
if (-not $NoPath) {
    Write-Host "==> Registering $InstalledBinDir in User PATH..." -ForegroundColor Cyan
    $UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    if ($UserPath -notlike "*$InstalledBinDir*") {
        $NewPath = if ([string]::IsNullOrWhiteSpace($UserPath)) { $InstalledBinDir } else { "$UserPath;$InstalledBinDir" }
        [Environment]::SetEnvironmentVariable("PATH", $NewPath, "User")
        $env:PATH = "$InstalledBinDir;$env:PATH"
        Write-Host "  Added to User PATH successfully." -ForegroundColor Green
    } else {
        Write-Host "  Already present in User PATH." -ForegroundColor DarkGray
    }
} else {
    Write-Host "==> Skipping PATH modification (-NoPath specified)." -ForegroundColor DarkGray
}

# 9. Shortcuts (Desktop and Start Menu)
if (-not $NoShortcuts) {
    Write-Host "==> Creating application shortcuts..." -ForegroundColor Cyan
    $WshShell = New-Object -ComObject WScript.Shell

    # Start Menu Shortcut
    $StartMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
    if (Test-Path $StartMenuDir) {
        $StartMenuShortcut = Join-Path $StartMenuDir "ULVO Control Center.lnk"
        $Shortcut = $WshShell.CreateShortcut($StartMenuShortcut)
        $Shortcut.TargetPath = Join-Path $InstalledBinDir "ulvo-desktop.exe"
        $Shortcut.WorkingDirectory = $InstalledBinDir
        $Shortcut.Description = "ULVO Control Center & Universal Execution Gateway"
        $Shortcut.Save()
    }

    # Desktop Shortcut
    $DesktopDir = [Environment]::GetFolderPath("Desktop")
    if (Test-Path $DesktopDir) {
        $DesktopShortcut = Join-Path $DesktopDir "ULVO Control Center.lnk"
        $DShortcut = $WshShell.CreateShortcut($DesktopShortcut)
        $DShortcut.TargetPath = Join-Path $InstalledBinDir "ulvo-desktop.exe"
        $DShortcut.WorkingDirectory = $InstalledBinDir
        $DShortcut.Description = "ULVO Control Center & Universal Execution Gateway"
        $DShortcut.Save()
    }
} else {
    Write-Host "==> Skipping shortcut creation (-NoShortcuts specified)." -ForegroundColor DarkGray
}

# 10. Windows Add/Remove Programs Registration
if (-not $NoRegistry) {
    Write-Host "==> Registering in Windows Add/Remove Programs (HKCU)..." -ForegroundColor Cyan
    $UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ULVO"
    if (-not (Test-Path $UninstallKey)) {
        New-Item -Path $UninstallKey -Force | Out-Null
    }

    Set-ItemProperty -Path $UninstallKey -Name "DisplayName" -Value "ULVO Control Center" -Type String
    Set-ItemProperty -Path $UninstallKey -Name "DisplayVersion" -Value "1.2.0" -Type String
    Set-ItemProperty -Path $UninstallKey -Name "Publisher" -Value "ULVO Contributors" -Type String
    Set-ItemProperty -Path $UninstallKey -Name "DisplayIcon" -Value (Join-Path $InstalledBinDir "ulvo-desktop.exe") -Type String
    Set-ItemProperty -Path $UninstallKey -Name "InstallLocation" -Value $ResolvedInstall -Type String
    $UninstallCmd = "powershell.exe -ExecutionPolicy Bypass -File `"$ResolvedInstall\uninstall.ps1`""
    Set-ItemProperty -Path $UninstallKey -Name "UninstallString" -Value $UninstallCmd -Type String
    Set-ItemProperty -Path $UninstallKey -Name "QuietUninstallString" -Value "$UninstallCmd -Force" -Type String
    Set-ItemProperty -Path $UninstallKey -Name "NoModify" -Value 1 -Type DWord
    Set-ItemProperty -Path $UninstallKey -Name "NoRepair" -Value 1 -Type DWord
    Set-ItemProperty -Path $UninstallKey -Name "EstimatedSize" -Value 45000 -Type DWord
    Set-ItemProperty -Path $UninstallKey -Name "HelpLink" -Value "https://github.com/Voidwalker-AT/ULVO" -Type String
    Set-ItemProperty -Path $UninstallKey -Name "URLInfoAbout" -Value "https://github.com/Voidwalker-AT/ULVO" -Type String
} else {
    Write-Host "==> Skipping Registry modification (-NoRegistry specified)." -ForegroundColor DarkGray
}

Write-Host ""
Write-Host "============================================================" -ForegroundColor Green
Write-Host "       ULVO v1.2.0 Installation Completed Successfully!    " -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Green
Write-Host "Launch ULVO:"
Write-Host "    GUI: Launch 'ULVO Control Center' from Desktop or Start Menu"
Write-Host "    CLI: Open terminal and run 'ulvo doctor' or 'ulvo catalog'"
Write-Host "Vault Location:"
Write-Host "    $VaultPath"
Write-Host ""
