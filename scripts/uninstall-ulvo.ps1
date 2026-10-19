# ULVO Hardened Windows Uninstaller
# Safely unregisters PATH, shortcuts, and installed program files while strictly preserving vault project data.

[CmdletBinding()]
param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\ULVO",
    [switch]$NoShortcuts,
    [switch]$NoRegistry,
    [switch]$NoPath,
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
if (-not $NoShortcuts) {
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
} else {
    Write-Host "==> Skipping shortcut removal (-NoShortcuts specified)." -ForegroundColor DarkGray
}

# 3. Unregister from Windows Add/Remove Programs Registry
if (-not $NoRegistry) {
    Write-Host "==> Unregistering from Windows Registry..." -ForegroundColor Cyan
    $UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ULVO"
    if (Test-Path $UninstallKey) {
        Remove-Item -Recurse -Force $UninstallKey -ErrorAction SilentlyContinue
        Write-Host "  Removed HKCU Uninstall registry key." -ForegroundColor Green
    }
} else {
    Write-Host "==> Skipping Registry modification (-NoRegistry specified)." -ForegroundColor DarkGray
}

# 4. Clean User PATH
if (-not $NoPath) {
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
} else {
    Write-Host "==> Skipping PATH modification (-NoPath specified)." -ForegroundColor DarkGray
}

# 5. Read configured vault path before removing files (for preservation reporting)
$ConfiguredVault = ""
$PrefFile = Join-Path $ResolvedInstall "config\preferences.json"
if (Test-Path $PrefFile) {
    try {
        $prefs = Get-Content $PrefFile -Raw | ConvertFrom-Json
        $ConfiguredVault = $prefs.vault_path
    } catch {}
}

# 6. Remove ONLY Independently Verified Installer-Owned Files
Write-Host "==> Removing verified installer-owned files at $ResolvedInstall..." -ForegroundColor Cyan

$filesRemoved = 0
$filesPreserved = 0

if (Test-Path $ManifestFile) {
    try {
        $manifest = Get-Content $ManifestFile -Raw | ConvertFrom-Json
        if ($manifest.files) {
            foreach ($prop in $manifest.files.PSObject.Properties) {
                $relPath = $prop.Name.Replace("/", "\")
                $targetFile = Join-Path $ResolvedInstall $relPath
                $fullTarget = [System.IO.Path]::GetFullPath($targetFile)
                # Ensure target is strictly inside $ResolvedInstall
                if ($fullTarget.StartsWith($ResolvedInstall, [System.StringComparison]::OrdinalIgnoreCase) -and (Test-Path $fullTarget)) {
                    Remove-Item -Path $fullTarget -Force -ErrorAction SilentlyContinue
                    $filesRemoved++
                }
            }
        }
    } catch {
        Write-Host "  Warning: Failed parsing manifest, falling back to known binary cleanup: $_" -ForegroundColor Yellow
    }
}

# Cleanup known auxiliary files
$KnownFiles = @(
    (Join-Path $ResolvedInstall "uninstall.ps1"),
    (Join-Path $ResolvedInstall "scripts\uninstall-ulvo.ps1"),
    (Join-Path $ResolvedInstall ".ulvo-install-manifest.json"),
    (Join-Path $ResolvedInstall "bin\ulvo.exe"),
    (Join-Path $ResolvedInstall "bin\ulvo-agent.exe"),
    (Join-Path $ResolvedInstall "bin\ulvo-desktop.exe"),
    (Join-Path $ResolvedInstall "bin\WebView2Loader.dll")
)

foreach ($kf in $KnownFiles) {
    if (Test-Path $kf) {
        Remove-Item -Path $kf -Force -ErrorAction SilentlyContinue
        $filesRemoved++
    }
}

# Safely remove empty subdirectories, leaving user-created/custom files intact
$Subdirs = @("bin", "scripts", "config")
foreach ($sub in $Subdirs) {
    $subPath = Join-Path $ResolvedInstall $sub
    if (Test-Path $subPath) {
        $remainingItems = Get-ChildItem -Path $subPath -Force -ErrorAction SilentlyContinue
        if ($remainingItems.Count -eq 0) {
            Remove-Item -Path $subPath -Force -ErrorAction SilentlyContinue
        } else {
            Write-Host "  Preserved non-installer items in $subPath ($($remainingItems.Count) item(s))." -ForegroundColor DarkGray
            $filesPreserved += $remainingItems.Count
        }
    }
}

# Check if root installation directory is empty
$remainingRoot = Get-ChildItem -Path $ResolvedInstall -Force -ErrorAction SilentlyContinue
if ($remainingRoot.Count -eq 0) {
    Remove-Item -Path $ResolvedInstall -Force -ErrorAction SilentlyContinue
    Write-Host "  Removed installation directory $ResolvedInstall." -ForegroundColor Green
} else {
    Write-Host "  Preserved user files in $ResolvedInstall ($($remainingRoot.Count) non-installer item(s) preserved)." -ForegroundColor Yellow
}

Write-Host ""
Write-Host "============================================================" -ForegroundColor Green
Write-Host "        ULVO v1.2.0 Uninstalled Successfully!               " -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Green
Write-Host "  Summary: $filesRemoved installer file(s) removed, $filesPreserved user file(s) preserved."
if ($ConfiguredVault) {
    Write-Host "  VAULT PRESERVATION: Vault data, runtimes, and user projects at:" -ForegroundColor Cyan
    Write-Host "  $ConfiguredVault" -ForegroundColor Cyan
    Write-Host "  have been strictly preserved." -ForegroundColor Cyan
}
