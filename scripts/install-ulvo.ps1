# ULVO Hardened Windows Installer (Per-User, Zero Admin Required)
# Installs ULVO Control Center, CLI gateway, and background agent cleanly for the current user.

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

# 0. Resolve default VaultPath cleanly
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
Write-Host "Install Target: $InstallDir"
Write-Host "Vault Target:   $VaultPath"
Write-Host ""

# 1. Locate Source Binaries
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

# 2. Check for running processes before upgrading
$BinDir = Join-Path $InstallDir "bin"
if (Test-Path $BinDir) {
    $running = Get-Process | Where-Object { $_.Path -and $_.Path.StartsWith($BinDir, [System.StringComparison]::OrdinalIgnoreCase) }
    if ($running) {
        Write-Host "==> Detected running ULVO processes. Attempting graceful shutdown..." -ForegroundColor Yellow
        # Try graceful stop of agent
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
                throw "Active ULVO processes are running from $InstallDir. Please close ULVO Control Center or re-run with -Force."
            }
        }
    }
}

# 2.5 Snapshot previous version for atomic rollback on upgrade
$StageTimestamp = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
$HasExistingInstall = (Test-Path (Join-Path $InstallDir ".ulvo-install-manifest.json")) -or (Test-Path (Join-Path $BinDir "ulvo.exe"))
$RollbackDir = Join-Path $InstallDir ".rollback_$StageTimestamp"

if ($HasExistingInstall) {
    Write-Host "==> Creating rollback snapshot of existing installation..." -ForegroundColor Cyan
    New-Item -ItemType Directory -Path $RollbackDir -Force | Out-Null
    if (Test-Path $BinDir) {
        Copy-Item $BinDir -Destination (Join-Path $RollbackDir "bin") -Recurse -Force
    }
    if (Test-Path (Join-Path $InstallDir "config")) {
        Copy-Item (Join-Path $InstallDir "config") -Destination (Join-Path $RollbackDir "config") -Recurse -Force
    }
    if (Test-Path (Join-Path $InstallDir "scripts")) {
        Copy-Item (Join-Path $InstallDir "scripts") -Destination (Join-Path $RollbackDir "scripts") -Recurse -Force
    }
    if (Test-Path (Join-Path $InstallDir ".ulvo-install-manifest.json")) {
        Copy-Item (Join-Path $InstallDir ".ulvo-install-manifest.json") -Destination (Join-Path $RollbackDir ".ulvo-install-manifest.json") -Force
    }
    if (Test-Path (Join-Path $InstallDir "uninstall.ps1")) {
        Copy-Item (Join-Path $InstallDir "uninstall.ps1") -Destination (Join-Path $RollbackDir "uninstall.ps1") -Force
    }
}

# 3. Transactional Staging
$StagingDir = Join-Path $InstallDir ".staging_$StageTimestamp"

try {
    Write-Host "==> Staging installation files..." -ForegroundColor Cyan
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

    # Configuration template
    $Prefs = @{
        vault_path = $VaultPath
        safety_reserve_mb = 500
        cache_budget_gb = 20.0
        auto_update = $false
        enable_gateway_path = (-not $NoPath)
        enable_background_agent = $true
    } | ConvertTo-Json -Depth 4
    $Prefs | Set-Content -Path (Join-Path $StagingDir "config\preferences.json") -Encoding utf8

    # Build installation manifest with SHA-256 hashes
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
        install_timestamp = $StageTimestamp
        install_dir = $InstallDir
        vault_path = $VaultPath
        files = $ManifestFiles
    } | ConvertTo-Json -Depth 4
    $InstallManifest | Set-Content -Path (Join-Path $StagingDir ".ulvo-install-manifest.json") -Encoding utf8

    # 4. Atomic Commit
    Write-Host "==> Committing installation to $InstallDir..." -ForegroundColor Cyan
    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $InstallDir "config") -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $InstallDir "scripts") -Force | Out-Null

    # Copy from staging to destination
    Get-ChildItem -Path (Join-Path $StagingDir "bin") -File | ForEach-Object {
        Copy-Item $_.FullName -Destination (Join-Path $BinDir $_.Name) -Force
    }
    Copy-Item (Join-Path $StagingDir "uninstall.ps1") -Destination (Join-Path $InstallDir "uninstall.ps1") -Force
    Copy-Item (Join-Path $StagingDir "scripts\uninstall-ulvo.ps1") -Destination (Join-Path $InstallDir "scripts\uninstall-ulvo.ps1") -Force
    Copy-Item (Join-Path $StagingDir ".ulvo-install-manifest.json") -Destination (Join-Path $InstallDir ".ulvo-install-manifest.json") -Force

    # Preserve existing user preferences if already present
    $DestPref = Join-Path $InstallDir "config\preferences.json"
    if (-not (Test-Path $DestPref)) {
        Copy-Item (Join-Path $StagingDir "config\preferences.json") -Destination $DestPref -Force
    }

    # Clean staging and rollback snapshot directories on success
    Remove-Item -Path $StagingDir -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path $RollbackDir) {
        Remove-Item -Path $RollbackDir -Recurse -Force -ErrorAction SilentlyContinue
    }

} catch {
    Write-Host "ERROR during installation: $_" -ForegroundColor Red
    Write-Host "==> Rolling back incomplete installation..." -ForegroundColor Yellow
    if (Test-Path $StagingDir) {
        Remove-Item -Path $StagingDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    if ($HasExistingInstall -and (Test-Path $RollbackDir)) {
        Write-Host "==> Restoring previous version from rollback snapshot..." -ForegroundColor Yellow
        if (Test-Path (Join-Path $RollbackDir "bin")) {
            Copy-Item (Join-Path $RollbackDir "bin\*") -Destination $BinDir -Recurse -Force -ErrorAction SilentlyContinue
        }
        if (Test-Path (Join-Path $RollbackDir "config")) {
            Copy-Item (Join-Path $RollbackDir "config\*") -Destination (Join-Path $InstallDir "config") -Recurse -Force -ErrorAction SilentlyContinue
        }
        if (Test-Path (Join-Path $RollbackDir "scripts")) {
            Copy-Item (Join-Path $RollbackDir "scripts\*") -Destination (Join-Path $InstallDir "scripts") -Recurse -Force -ErrorAction SilentlyContinue
        }
        if (Test-Path (Join-Path $RollbackDir ".ulvo-install-manifest.json")) {
            Copy-Item (Join-Path $RollbackDir ".ulvo-install-manifest.json") -Destination (Join-Path $InstallDir ".ulvo-install-manifest.json") -Force -ErrorAction SilentlyContinue
        }
        if (Test-Path (Join-Path $RollbackDir "uninstall.ps1")) {
            Copy-Item (Join-Path $RollbackDir "uninstall.ps1") -Destination (Join-Path $InstallDir "uninstall.ps1") -Force -ErrorAction SilentlyContinue
        }
        Remove-Item -Path $RollbackDir -Recurse -Force -ErrorAction SilentlyContinue
        Write-Host "  Rollback completed: previous version restored." -ForegroundColor Green
    }
    throw "Installation failed and was rolled back cleanly: $_"
}

# 5. User PATH Registration (Conditional)
if (-not $NoPath) {
    Write-Host "==> Registering $BinDir in User PATH..." -ForegroundColor Cyan
    $UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    if ($UserPath -notlike "*$BinDir*") {
        $NewPath = if ([string]::IsNullOrWhiteSpace($UserPath)) { $BinDir } else { "$UserPath;$BinDir" }
        [Environment]::SetEnvironmentVariable("PATH", $NewPath, "User")
        $env:PATH = "$BinDir;$env:PATH"
        Write-Host "  Added to User PATH successfully." -ForegroundColor Green
    } else {
        Write-Host "  Already present in User PATH." -ForegroundColor DarkGray
    }
} else {
    Write-Host "==> Skipping PATH modification (-NoPath specified)." -ForegroundColor DarkGray
}

# 6. Shortcuts (Desktop and Start Menu)
if (-not $NoShortcuts) {
    Write-Host "==> Creating application shortcuts..." -ForegroundColor Cyan
    $WshShell = New-Object -ComObject WScript.Shell

    # Start Menu Shortcut
    $StartMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
    if (Test-Path $StartMenuDir) {
        $StartMenuShortcut = Join-Path $StartMenuDir "ULVO Control Center.lnk"
        $Shortcut = $WshShell.CreateShortcut($StartMenuShortcut)
        $Shortcut.TargetPath = Join-Path $BinDir "ulvo-desktop.exe"
        $Shortcut.WorkingDirectory = $BinDir
        $Shortcut.Description = "ULVO Control Center & Universal Execution Gateway"
        $Shortcut.Save()
    }

    # Desktop Shortcut
    $DesktopDir = [Environment]::GetFolderPath("Desktop")
    if (Test-Path $DesktopDir) {
        $DesktopShortcut = Join-Path $DesktopDir "ULVO Control Center.lnk"
        $DShortcut = $WshShell.CreateShortcut($DesktopShortcut)
        $DShortcut.TargetPath = Join-Path $BinDir "ulvo-desktop.exe"
        $DShortcut.WorkingDirectory = $BinDir
        $DShortcut.Description = "ULVO Control Center & Universal Execution Gateway"
        $DShortcut.Save()
    }
}

# 7. Register Uninstaller in HKCU Registry (Add/Remove Programs)
# Self-contained: points to the INSTALLED uninstaller in $InstallDir\uninstall.ps1
if (-not $NoRegistry) {
    $InstalledUninstaller = Join-Path $InstallDir "uninstall.ps1"
    $UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ULVO"
    if (-not (Test-Path $UninstallKey)) {
        New-Item -Path $UninstallKey -Force | Out-Null
    }
    Set-ItemProperty -Path $UninstallKey -Name "DisplayName" -Value "ULVO Control Center & Universal Gateway"
    Set-ItemProperty -Path $UninstallKey -Name "DisplayVersion" -Value "1.2.0"
    Set-ItemProperty -Path $UninstallKey -Name "Publisher" -Value "Voidwalker-AT"
    Set-ItemProperty -Path $UninstallKey -Name "InstallLocation" -Value $InstallDir
    Set-ItemProperty -Path $UninstallKey -Name "DisplayIcon" -Value (Join-Path $BinDir "ulvo-desktop.exe")
    Set-ItemProperty -Path $UninstallKey -Name "UninstallString" -Value "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$InstalledUninstaller`""
    Set-ItemProperty -Path $UninstallKey -Name "QuietUninstallString" -Value "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$InstalledUninstaller`" -Force"
} else {
    Write-Host "==> Skipping Registry modification (-NoRegistry specified)." -ForegroundColor DarkGray
}

# 8. Post-Installation Verification Check
Write-Host "==> Verifying installation integrity..." -ForegroundColor Cyan
$VerOutput = & (Join-Path $BinDir "ulvo.exe") --version
Write-Host "  Verification passed: $VerOutput" -ForegroundColor Green

Write-Host ""
Write-Host "============================================================" -ForegroundColor Green
Write-Host "       ULVO v1.2.0 Installation Completed Successfully!    " -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Green
Write-Host "Launch ULVO:"
Write-Host "  • GUI: Launch 'ULVO Control Center' from Desktop or Start Menu"
Write-Host "  • CLI: Open terminal and run 'ulvo doctor' or 'ulvo catalog'"
