# ULVO Production Windows Installer (Per-User, Zero Admin Required)
# Installs ULVO Control Center, CLI gateway, and background agent cleanly for the current user.

[CmdletBinding()]
param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\ULVO",
    [string]$VaultPath = "E:\Workspace\Active\ULVO\vault",
    [switch]$NoShortcuts
)

$ErrorActionPreference = "Stop"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "     ULVO Windows Desktop & Gateway Installer v1.2.0        " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "Install Target: $InstallDir"
Write-Host "Vault Target:   $VaultPath"
Write-Host ""

$SourceBinDir = Join-Path (Get-Item $PSScriptRoot).Parent.FullName "target\release"
if (-not (Test-Path (Join-Path $SourceBinDir "ulvo.exe"))) {
    $SourceBinDir = Join-Path $PSScriptRoot "..\bin"
}

if (-not (Test-Path (Join-Path $SourceBinDir "ulvo.exe"))) {
    throw "Source binaries not found. Please build release binaries or run from release archive."
}

# 1. Create target directories
$BinDir = Join-Path $InstallDir "bin"
$ConfigDir = Join-Path $InstallDir "config"
New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
New-Item -ItemType Directory -Path $ConfigDir -Force | Out-Null

# 2. Copy binaries
Write-Host "==> Installing binaries to $BinDir..." -ForegroundColor Cyan
Copy-Item (Join-Path $SourceBinDir "ulvo.exe") -Destination (Join-Path $BinDir "ulvo.exe") -Force
Copy-Item (Join-Path $SourceBinDir "ulvo-agent.exe") -Destination (Join-Path $BinDir "ulvo-agent.exe") -Force
Copy-Item (Join-Path $SourceBinDir "ulvo-desktop.exe") -Destination (Join-Path $BinDir "ulvo-desktop.exe") -Force

# 3. Write initial user configuration
$PrefFile = Join-Path $ConfigDir "preferences.json"
if (-not (Test-Path $PrefFile)) {
    $Prefs = @{
        vault_path = $VaultPath
        safety_reserve_mb = 500
        cache_budget_gb = 20.0
        auto_update = $false
        enable_gateway_path = $true
        enable_background_agent = $true
    } | ConvertTo-Json -Depth 4
    $Prefs | Set-Content -Path $PrefFile -Encoding utf8
}

# 4. Add bin directory to User PATH environment variable
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

# 5. Create Desktop and Start Menu Shortcuts
if (-not $NoShortcuts) {
    Write-Host "==> Creating application shortcuts..." -ForegroundColor Cyan
    $WshShell = New-Object -ComObject WScript.Shell

    # Start Menu Shortcut
    $StartMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
    $StartMenuShortcut = Join-Path $StartMenuDir "ULVO Control Center.lnk"
    $Shortcut = $WshShell.CreateShortcut($StartMenuShortcut)
    $Shortcut.TargetPath = Join-Path $BinDir "ulvo-desktop.exe"
    $Shortcut.WorkingDirectory = $BinDir
    $Shortcut.Description = "ULVO Control Center & Universal Execution Gateway"
    $Shortcut.Save()

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

# 6. Register Uninstaller in HKCU Registry (Add/Remove Programs)
$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ULVO"
if (-not (Test-Path $UninstallKey)) {
    New-Item -Path $UninstallKey -Force | Out-Null
}
Set-ItemProperty -Path $UninstallKey -Name "DisplayName" -Value "ULVO Control Center & Universal Gateway"
Set-ItemProperty -Path $UninstallKey -Name "DisplayVersion" -Value "1.2.0"
Set-ItemProperty -Path $UninstallKey -Name "Publisher" -Value "Voidwalker-AT"
Set-ItemProperty -Path $UninstallKey -Name "InstallLocation" -Value $InstallDir
Set-ItemProperty -Path $UninstallKey -Name "DisplayIcon" -Value (Join-Path $BinDir "ulvo-desktop.exe")
Set-ItemProperty -Path $UninstallKey -Name "UninstallString" -Value "powershell.exe -ExecutionPolicy Bypass -File `"$PSScriptRoot\uninstall-ulvo.ps1`""

# 7. Verification check
Write-Host "==> Verifying installation..." -ForegroundColor Cyan
$VerOutput = & (Join-Path $BinDir "ulvo.exe") --version
Write-Host "  Verification passed: $VerOutput" -ForegroundColor Green

Write-Host ""
Write-Host "============================================================" -ForegroundColor Green
Write-Host "       ULVO v1.2.0 Installation Completed Successfully!    " -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Green
Write-Host "Launch ULVO:"
Write-Host "  • GUI: Launch 'ULVO Control Center' from Desktop or Start Menu"
Write-Host "  • CLI: Open terminal and run 'ulvo doctor' or 'ulvo catalog'"
