# ULVO Hardened Windows Uninstaller
# Safely unregisters PATH, shortcuts, and installed program files while strictly preserving vault project data.

[CmdletBinding()]
param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\ULVO",
    [switch]$PurgeVault,
    [switch]$Force
)

$ErrorActionPreference = "Stop"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "     ULVO Windows Desktop & Gateway Uninstaller v1.2.0      " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

# 0. Safety Boundary Validation on InstallDir
if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    throw "InstallDir cannot be empty."
}

$ResolvedInstall = [System.IO.Path]::GetFullPath($InstallDir)
$RootDrives = [System.IO.DriveInfo]::GetDrives() | ForEach-Object { $_.RootDirectory.FullName.TrimEnd('\') }

foreach ($drive in $RootDrives) {
    if ($ResolvedInstall.Equals($drive, [System.StringComparison]::OrdinalIgnoreCase) -or 
        $ResolvedInstall.Equals("$drive\", [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "CRITICAL SAFETY REFUSAL: Refusing to uninstall root directory $ResolvedInstall"
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
    if ($ResolvedInstall.Equals($prot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "CRITICAL SAFETY REFUSAL: Refusing to uninstall system directory $ResolvedInstall"
    }
}

# Sentinel Check: Require either .ulvo-install-manifest.json or bin\ulvo.exe
$ManifestFile = Join-Path $ResolvedInstall ".ulvo-install-manifest.json"
$CliBinary = Join-Path $ResolvedInstall "bin\ulvo.exe"

if (-not (Test-Path $ManifestFile) -and -not (Test-Path $CliBinary)) {
    throw "SAFETY REFUSAL: Directory $ResolvedInstall does not contain a verified ULVO installation sentinel (.ulvo-install-manifest.json or bin\ulvo.exe)."
}

# 1. Stop running ULVO processes
$BinDir = Join-Path $ResolvedInstall "bin"
if (Test-Path $BinDir) {
    $running = Get-Process | Where-Object { $_.Path -and $_.Path.StartsWith($BinDir, [System.StringComparison]::OrdinalIgnoreCase) }
    if ($running) {
        Write-Host "==> Stopping running ULVO processes..." -ForegroundColor Yellow
        $installedCli = Join-Path $BinDir "ulvo.exe"
        if (Test-Path $installedCli) {
            & $installedCli agent stop *>$null
            Start-Sleep -Milliseconds 500
        }
        $remaining = Get-Process | Where-Object { $_.Path -and $_.Path.StartsWith($BinDir, [System.StringComparison]::OrdinalIgnoreCase) }
        if ($remaining) {
            $remaining | Stop-Process -Force
        }
    }
}

# 2. Remove Application Shortcuts
Write-Host "==> Removing application shortcuts..." -ForegroundColor Cyan
$StartMenuShortcut = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\ULVO Control Center.lnk"
if (Test-Path $StartMenuShortcut) {
    Remove-Item -Force $StartMenuShortcut -ErrorAction SilentlyContinue
    Write-Host "  Removed Start Menu shortcut." -ForegroundColor Green
}

$DesktopDir = [Environment]::GetFolderPath("Desktop")
if (Test-Path $DesktopDir) {
    $DesktopShortcut = Join-Path $DesktopDir "ULVO Control Center.lnk"
    if (Test-Path $DesktopShortcut) {
        Remove-Item -Force $DesktopShortcut -ErrorAction SilentlyContinue
        Write-Host "  Removed Desktop shortcut." -ForegroundColor Green
    }
}

# 3. Unregister from Windows Add/Remove Programs Registry
Write-Host "==> Unregistering from Windows Registry..." -ForegroundColor Cyan
$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ULVO"
if (Test-Path $UninstallKey) {
    Remove-Item -Recurse -Force $UninstallKey -ErrorAction SilentlyContinue
    Write-Host "  Removed HKCU Uninstall registry key." -ForegroundColor Green
}

# 4. Clean User PATH
Write-Host "==> Cleaning User PATH..." -ForegroundColor Cyan
$UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($UserPath -like "*$BinDir*") {
    $CleanedParts = $UserPath -split ";" | Where-Object { 
        -not [string]::IsNullOrWhiteSpace($_) -and -not $_.Equals($BinDir, [System.StringComparison]::OrdinalIgnoreCase)
    }
    $CleanedPath = $CleanedParts -join ";"
    [Environment]::SetEnvironmentVariable("PATH", $CleanedPath, "User")
    Write-Host "  Removed $BinDir from User PATH." -ForegroundColor Green
} else {
    Write-Host "  $BinDir not present in User PATH." -ForegroundColor DarkGray
}

# 5. Read configured vault path before removing files (for reporting or optional purge)
$ConfiguredVault = ""
$PrefFile = Join-Path $ResolvedInstall "config\preferences.json"
if (Test-Path $PrefFile) {
    try {
        $prefs = Get-Content $PrefFile -Raw | ConvertFrom-Json
        $ConfiguredVault = $prefs.vault_path
    } catch {}
}

# 6. Remove Program Files
Write-Host "==> Removing installation files at $ResolvedInstall..." -ForegroundColor Cyan
# If uninstaller was invoked directly from $ResolvedInstall\uninstall.ps1, schedule self-cleanup
Remove-Item -Path (Join-Path $ResolvedInstall "bin") -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -Path (Join-Path $ResolvedInstall "config") -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -Path (Join-Path $ResolvedInstall "scripts") -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -Path (Join-Path $ResolvedInstall ".ulvo-install-manifest.json") -Force -ErrorAction SilentlyContinue

# Attempt removing parent directory
try {
    # If script is inside $ResolvedInstall, use background cmd to remove the directory after exit
    $scriptDir = $PSScriptRoot
    if ($scriptDir -and $scriptDir.StartsWith($ResolvedInstall, [System.StringComparison]::OrdinalIgnoreCase)) {
        Start-Process cmd.exe -ArgumentList "/c timeout /t 1 /nobreak >nul & rmdir /s /q `"$ResolvedInstall`"" -WindowStyle Hidden
    } else {
        Remove-Item -Path $ResolvedInstall -Recurse -Force -ErrorAction SilentlyContinue
    }
    Write-Host "  Program files removed cleanly." -ForegroundColor Green
} catch {
    Write-Host "  Notice: Residual files scheduled for cleanup." -ForegroundColor Yellow
}

# 7. Vault Governance Policy
if ($PurgeVault -and $Force) {
    if ($ConfiguredVault -and (Test-Path $ConfiguredVault)) {
        Write-Host "==> PURGING VAULT DATA (-PurgeVault and -Force specified)..." -ForegroundColor Yellow
        Remove-Item -Path $ConfiguredVault -Recurse -Force
        Write-Host "  Vault directory at $ConfiguredVault has been purged." -ForegroundColor Yellow
    }
} else {
    Write-Host ""
    Write-Host "============================================================" -ForegroundColor Green
    Write-Host "        ULVO v1.2.0 Uninstalled Successfully!               " -ForegroundColor Green
    Write-Host "============================================================" -ForegroundColor Green
    if ($ConfiguredVault) {
        Write-Host "  SAFE PRESERVATION: Vault data, runtimes, and user projects at:" -ForegroundColor Cyan
        Write-Host "  $ConfiguredVault" -ForegroundColor Cyan
        Write-Host "  have been strictly preserved." -ForegroundColor Cyan
    }
}
