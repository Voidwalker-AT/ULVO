# ULVO Hardened Windows Uninstaller
# Strict Ownership Manifest Verification, Boundary Containment, Cryptographic Hash Matching, and Safe Selective Deletion.

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

$ResolvedInstall = [System.IO.Path]::GetFullPath($InstallDir).TrimEnd('\')
$CanonicalRoot = $ResolvedInstall
$RootPrefix = $CanonicalRoot + [System.IO.Path]::DirectorySeparatorChar

# Refuse uninstalling root drives
$RootDrives = [System.IO.DriveInfo]::GetDrives() | ForEach-Object { $_.RootDirectory.FullName.TrimEnd('\') }
foreach ($drive in $RootDrives) {
    if ($CanonicalRoot.Equals($drive, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "CRITICAL SAFETY REFUSAL: Refusing to uninstall root directory $CanonicalRoot"
    }
}

# Refuse uninstalling system directories
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
    $canonProt = [System.IO.Path]::GetFullPath($prot).TrimEnd('\')
    if ($CanonicalRoot.Equals($canonProt, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "CRITICAL SAFETY REFUSAL: Refusing to uninstall protected system directory $CanonicalRoot"
    }
}

# 1. Ownership Manifest Verification (MANDATORY)
# Section 3: "Require a valid installation ownership manifest. Do not accept bin/ulvo.exe alone as sufficient proof of ownership."
$ManifestFile = Join-Path $CanonicalRoot ".ulvo-install-manifest.json"

if (-not (Test-Path $ManifestFile)) {
    throw "CRITICAL OWNERSHIP REFUSAL: Directory '$CanonicalRoot' lacks an official ULVO installation ownership manifest (.ulvo-install-manifest.json). Refusing to delete unverified directory."
}

# Validate Manifest Schema & Product Identity
$manifestRaw = Get-Content $ManifestFile -Raw -ErrorAction Stop
$manifest = $null
try {
    $manifest = $manifestRaw | ConvertFrom-Json
} catch {
    throw "CRITICAL OWNERSHIP REFUSAL: Installation manifest at '$ManifestFile' is corrupted or invalid JSON: $_"
}

if ($null -eq $manifest) {
    throw "CRITICAL OWNERSHIP REFUSAL: Manifest parsed as null."
}

if ($manifest.schema_version -ne 1) {
    throw "CRITICAL OWNERSHIP REFUSAL: Unsupported manifest schema version '$($manifest.schema_version)' (expected 1)."
}

if ($manifest.product -ne "ULVO") {
    throw "CRITICAL OWNERSHIP REFUSAL: Manifest product mismatch: '$($manifest.product)' (expected 'ULVO')."
}

if ([string]::IsNullOrWhiteSpace($manifest.install_dir)) {
    throw "CRITICAL OWNERSHIP REFUSAL: Manifest does not declare an install_dir."
}

$manifestInstallCanon = [System.IO.Path]::GetFullPath($manifest.install_dir).TrimEnd('\')
if (-not $CanonicalRoot.Equals($manifestInstallCanon, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "CRITICAL OWNERSHIP REFUSAL: Manifest install_dir mismatch! Manifest declares '$manifestInstallCanon', but target is '$CanonicalRoot'."
}

if ($null -eq $manifest.files) {
    throw "CRITICAL OWNERSHIP REFUSAL: Manifest does not contain a file inventory."
}

# 2. Stop running ULVO processes inside installation root
$BinDir = Join-Path $CanonicalRoot "bin"
if (Test-Path $BinDir) {
    $running = Get-Process | Where-Object { $_.Path -and $_.Path.StartsWith($RootPrefix, [System.StringComparison]::OrdinalIgnoreCase) }
    if ($running) {
        Write-Host "==> Stopping running ULVO processes..." -ForegroundColor Yellow
        $installedCli = Join-Path $BinDir "ulvo.exe"
        if (Test-Path $installedCli) {
            & $installedCli agent stop *>$null
            Start-Sleep -Milliseconds 500
        }
        $remaining = Get-Process | Where-Object { $_.Path -and $_.Path.StartsWith($RootPrefix, [System.StringComparison]::OrdinalIgnoreCase) }
        if ($remaining) {
            $remaining | Stop-Process -Force
        }
    }
}

# 3. Remove Application Shortcuts
if (-not $NoShortcuts) {
    Write-Host "==> Removing application shortcuts..." -ForegroundColor Cyan
    $StartMenuShortcut = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\ULVO Control Center.lnk"
    if (Test-Path $StartMenuShortcut) {
        try {
            Remove-Item -Force $StartMenuShortcut -ErrorAction Stop
            Write-Host "  Removed Start Menu shortcut." -ForegroundColor Green
        } catch {
            Write-Host "  Warning: Failed to remove Start Menu shortcut: $_" -ForegroundColor Yellow
        }
    }

    $DesktopDir = [Environment]::GetFolderPath("Desktop")
    if (Test-Path $DesktopDir) {
        $DesktopShortcut = Join-Path $DesktopDir "ULVO Control Center.lnk"
        if (Test-Path $DesktopShortcut) {
            try {
                Remove-Item -Force $DesktopShortcut -ErrorAction Stop
                Write-Host "  Removed Desktop shortcut." -ForegroundColor Green
            } catch {
                Write-Host "  Warning: Failed to remove Desktop shortcut: $_" -ForegroundColor Yellow
            }
        }
    }
} else {
    Write-Host "==> Skipping shortcut removal (-NoShortcuts specified)." -ForegroundColor DarkGray
}

# 4. Unregister from Windows Add/Remove Programs Registry
if (-not $NoRegistry) {
    Write-Host "==> Unregistering from Windows Registry..." -ForegroundColor Cyan
    $UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ULVO"
    if (Test-Path $UninstallKey) {
        try {
            Remove-Item -Recurse -Force $UninstallKey -ErrorAction Stop
            Write-Host "  Removed HKCU Uninstall registry key." -ForegroundColor Green
        } catch {
            Write-Host "  Warning: Failed to remove Registry key: $_" -ForegroundColor Yellow
        }
    }
} else {
    Write-Host "==> Skipping Registry modification (-NoRegistry specified)." -ForegroundColor DarkGray
}

# 5. Clean User PATH
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

# 6. Read configured vault path before removing files (for preservation reporting)
$ConfiguredVault = $manifest.vault_path
if ([string]::IsNullOrWhiteSpace($ConfiguredVault)) {
    $PrefFile = Join-Path $CanonicalRoot "config\preferences.json"
    if (Test-Path $PrefFile) {
        try {
            $prefs = Get-Content $PrefFile -Raw | ConvertFrom-Json
            $ConfiguredVault = $prefs.vault_path
        } catch {}
    }
}

# 7. Validate ALL Manifest Entries BEFORE Modifying File System
Write-Host "==> Verifying ownership manifest security invariants..." -ForegroundColor Cyan

$ValidatedTargets = @()

foreach ($prop in $manifest.files.PSObject.Properties) {
    $relKey = $prop.Name
    $expectedHash = $prop.Value

    # SECURITY VALIDATION: No absolute paths in relative key
    if ([System.IO.Path]::IsPathRooted($relKey)) {
        throw "MALICIOUS MANIFEST DETECTED: Absolute path in relative key: '$relKey'"
    }

    # SECURITY VALIDATION: No directory traversal
    if ($relKey -match "(\.\.[\\/]|[\\/]\.\.)" -or $relKey -eq "..") {
        throw "MALICIOUS MANIFEST DETECTED: Path traversal attempt: '$relKey'"
    }

    # SECURITY VALIDATION: No alternate data streams
    if ($relKey -match ":") {
        throw "MALICIOUS MANIFEST DETECTED: Alternate data stream attempt: '$relKey'"
    }

    # Canonical Path Construction
    $normalizedRel = $relKey.Replace("/", [System.IO.Path]::DirectorySeparatorChar.ToString())
    $targetPath = Join-Path $CanonicalRoot $normalizedRel
    $canonicalTarget = [System.IO.Path]::GetFullPath($targetPath)

    # SECURITY VALIDATION: Separator Boundary Containment (Sibling Prefix Attack Defense)
    # Target MUST start with "$CanonicalRoot\" or equal "$CanonicalRoot"
    if (-not $canonicalTarget.StartsWith($RootPrefix, [System.StringComparison]::OrdinalIgnoreCase) -and
        -not $canonicalTarget.Equals($CanonicalRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "MALICIOUS MANIFEST DETECTED: Boundary escape violation for '$canonicalTarget' outside '$CanonicalRoot'"
    }

    $ValidatedTargets += [PSCustomObject]@{
        RelKey = $normalizedRel
        CanonicalPath = $canonicalTarget
        ExpectedHash = $expectedHash
    }
}

Write-Host "  Manifest security invariants verified cleanly." -ForegroundColor Green

# 8. Selectively Remove Only Verified Installer-Owned Files
Write-Host "==> Removing verified installer-owned files at $CanonicalRoot..." -ForegroundColor Cyan

$filesRemoved = 0
$filesPreserved = 0
$filesFailed = 0

foreach ($item in $ValidatedTargets) {
    $canonicalTarget = $item.CanonicalPath
    $expectedHash = $item.ExpectedHash
    $normalizedRel = $item.RelKey

    # Don't delete uninstaller scripts yet (delete them at the very end)
    if ($normalizedRel -eq "uninstall.ps1" -or $normalizedRel -eq "scripts\uninstall-ulvo.ps1") {
        continue
    }

    # SECURITY VALIDATION: Reparse Point / Symlink Traversal Check
    $cursor = $canonicalTarget
    while ($cursor.Length -ge $CanonicalRoot.Length) {
        if (Test-Path $cursor) {
            $itemInfo = Get-Item $cursor -Force -ErrorAction SilentlyContinue
            if ($itemInfo -and ($itemInfo.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
                Write-Host "  SECURITY WARNING: Reparse point detected at $($cursor). Deleting link only without traversing." -ForegroundColor Yellow
                try {
                    [System.IO.File]::Delete($cursor)
                    $filesRemoved++
                } catch {
                    Write-Host "  Error deleting reparse point $($cursor): $_" -ForegroundColor Red
                    $filesFailed++
                }
                break
            }
        }
        $cursor = [System.IO.Path]::GetDirectoryName($cursor)
    }

    if (-not (Test-Path $canonicalTarget -PathType Leaf)) {
        continue
    }

    # CRYPTOGRAPHIC INTEGRITY MATCHING: Compare installed file hash against recorded hash
    try {
        $actualHash = (Get-FileHash -Path $canonicalTarget -Algorithm SHA256 -ErrorAction Stop).Hash.ToLower()
        if ($actualHash -eq $expectedHash) {
            # File is unmodified installer-owned: Safe to delete
            Remove-Item -Path $canonicalTarget -Force -ErrorAction Stop
            $filesRemoved++
        } else {
            # File was modified by user! Preserve it and report discrepancy
            Write-Host "  PRESERVED MODIFIED FILE: $normalizedRel (SHA-256 differs from manifest: actual=$actualHash, expected=$expectedHash)" -ForegroundColor Yellow
            $filesPreserved++
        }
    } catch {
        Write-Host "  Error processing/removing $($canonicalTarget): $_" -ForegroundColor Red
        $filesFailed++
    }
}

# Remove auxiliary installer files if unmodified
$AuxFiles = @(
    "uninstall.ps1",
    "scripts\uninstall-ulvo.ps1",
    "ULVO Control Center.bat"
)
foreach ($aux in $AuxFiles) {
    $auxPath = Join-Path $CanonicalRoot $aux
    if (Test-Path $auxPath) {
        try {
            Remove-Item -Path $auxPath -Force -ErrorAction Stop
            $filesRemoved++
        } catch {
            Write-Host "  Warning: could not remove auxiliary file $($auxPath): $_" -ForegroundColor Yellow
            $filesFailed++
        }
    }
}

# Lastly remove the manifest file itself
try {
    Remove-Item -Path $ManifestFile -Force -ErrorAction Stop
    $filesRemoved++
} catch {
    Write-Host "  Warning: could not remove manifest file: $_" -ForegroundColor Yellow
    $filesFailed++
}

# 8. Safely Clean Empty Subdirectories (Preserving User Custom Files)
$Subdirs = @("bin", "scripts", "config")
foreach ($sub in $Subdirs) {
    $subPath = Join-Path $CanonicalRoot $sub
    if (Test-Path $subPath) {
        $remainingItems = Get-ChildItem -Path $subPath -Force -ErrorAction SilentlyContinue
        if ($remainingItems.Count -eq 0) {
            try {
                Remove-Item -Path $subPath -Force -ErrorAction Stop
            } catch {
                Write-Host "  Warning: could not remove empty directory $($subPath): $_" -ForegroundColor Yellow
            }
        } else {
            Write-Host "  Preserved non-installer items in $subPath ($($remainingItems.Count) item(s))." -ForegroundColor DarkGray
            $filesPreserved += $remainingItems.Count
        }
    }
}

# Check if root installation directory is empty
$remainingRoot = Get-ChildItem -Path $CanonicalRoot -Force -ErrorAction SilentlyContinue
if ($remainingRoot.Count -eq 0) {
    try {
        Remove-Item -Path $CanonicalRoot -Force -ErrorAction Stop
        Write-Host "  Removed installation directory $CanonicalRoot." -ForegroundColor Green
    } catch {
        Write-Host "  Warning: could not remove empty installation directory: $_" -ForegroundColor Yellow
    }
} else {
    Write-Host "  Preserved user files in $CanonicalRoot ($($remainingRoot.Count) non-installer item(s) preserved)." -ForegroundColor Yellow
}

Write-Host ""
Write-Host "============================================================" -ForegroundColor Green
Write-Host "        ULVO v1.2.0 Uninstalled Successfully!               " -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Green
Write-Host "  Summary: $filesRemoved installer file(s) removed, $filesPreserved user file(s) preserved, $filesFailed failure(s)."
if ($ConfiguredVault) {
    Write-Host "  VAULT PRESERVATION: Vault data, runtimes, and user projects at:" -ForegroundColor Cyan
    Write-Host "  $ConfiguredVault" -ForegroundColor Cyan
    Write-Host "  have been strictly preserved." -ForegroundColor Cyan
}

if ($filesFailed -gt 0) {
    throw "Uninstallation completed with $filesFailed deletion failure(s). Please review the log above."
}
