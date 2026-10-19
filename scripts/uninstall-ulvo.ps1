# ULVO Production Windows Uninstaller
# Safely unregisters PATH, shortcuts, and installed program files without deleting vault project data.

[CmdletBinding()]
param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\ULVO",
    [switch]$PurgeVault,
    [switch]$Force
)

$ErrorActionPreference = "Stop"

Write-Host "==> Uninstalling ULVO Control Center & Gateway..." -ForegroundColor Cyan

# 1. Remove Shortcuts
$StartMenuShortcut = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\ULVO Control Center.lnk"
if (Test-Path $StartMenuShortcut) {
    Remove-Item -Force $StartMenuShortcut
    Write-Host "  Removed Start Menu shortcut." -ForegroundColor Green
}

$DesktopDir = [Environment]::GetFolderPath("Desktop")
$DesktopShortcut = Join-Path $DesktopDir "ULVO Control Center.lnk"
if (Test-Path $DesktopShortcut) {
    Remove-Item -Force $DesktopShortcut
    Write-Host "  Removed Desktop shortcut." -ForegroundColor Green
}

# 2. Unregister from HKCU Uninstall registry
$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ULVO"
if (Test-Path $UninstallKey) {
    Remove-Item -Recurse -Force $UninstallKey
    Write-Host "  Unregistered from Windows installed applications." -ForegroundColor Green
}

# 3. Remove bin from User PATH
$BinDir = Join-Path $InstallDir "bin"
$UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($UserPath -like "*$BinDir*") {
    $CleanedPath = ($UserPath -split ";" | Where-Object { $_ -ne $BinDir -and -not [string]::IsNullOrWhiteSpace($_) }) -join ";"
    [Environment]::SetEnvironmentVariable("PATH", $CleanedPath, "User")
    Write-Host "  Removed $BinDir from User PATH." -ForegroundColor Green
}

# 4. Remove installation files
if (Test-Path $InstallDir) {
    Remove-Item -Recurse -Force $InstallDir
    Write-Host "  Removed program files at $InstallDir." -ForegroundColor Green
}

Write-Host "==> ULVO uninstalled successfully." -ForegroundColor Green
Write-Host "  Note: Vault data and runtimes at E:\Workspace\Active\ULVO\vault were preserved." -ForegroundColor Cyan
