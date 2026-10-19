# ULVO v1.2.0 Release Packaging Script (Windows x86_64)
# Automates deterministic packaging of release artifacts, binaries, and SHA-256 checksums.

[CmdletBinding()]
param(
    [string]$Version = "1.2.0",
    [string]$DistDir = "dist"
)

$ErrorActionPreference = "Stop"

$RepoRoot = (Get-Item $PSScriptRoot).Parent.FullName
Set-Location $RepoRoot

Write-Host "==> Packaging ULVO v$Version for Windows x86_64..." -ForegroundColor Cyan

# Ensure release binaries exist
$ReleaseCli = Join-Path $RepoRoot "target\release\ulvo.exe"
$ReleaseAgent = Join-Path $RepoRoot "target\release\ulvo-agent.exe"
$ReleaseDesktop = Join-Path $RepoRoot "target\release\ulvo-desktop.exe"

$MissingBinaries = @()
if (-not (Test-Path $ReleaseCli)) { $MissingBinaries += "ulvo.exe" }
if (-not (Test-Path $ReleaseAgent)) { $MissingBinaries += "ulvo-agent.exe" }
if (-not (Test-Path $ReleaseDesktop)) { $MissingBinaries += "ulvo-desktop.exe" }

if ($MissingBinaries.Count -gt 0) {
    Write-Host "Missing release binaries ($($MissingBinaries -join ', ')). Building..." -ForegroundColor Yellow
    $env:CARGO_HOME = Join-Path $RepoRoot "vault\runtimes\rust\cargo"
    $env:RUSTUP_HOME = Join-Path $RepoRoot "vault\runtimes\rust\rustup"
    $env:PATH = "$env:CARGO_HOME\bin;C:\msys64\ucrt64\bin;C:\msys64\usr\bin;$env:PATH"
    cargo build --release -p ulvo-cli -p ulvo-agent -p ulvo-desktop
}

# Verify CLI version
& $ReleaseCli --version
if ($LASTEXITCODE -ne 0) {
    throw "Release CLI verification failed with exit code $LASTEXITCODE"
}

$DistPath = Join-Path $RepoRoot $DistDir
if (Test-Path $DistPath) {
    Remove-Item -Recurse -Force $DistPath
}
$StageDir = Join-Path $DistPath "ulvo-v$Version-windows-x64-portable"
New-Item -ItemType Directory -Path (Join-Path $StageDir "bin") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $StageDir "docs") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $StageDir "config") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $StageDir "scripts") -Force | Out-Null

Write-Host "==> Staging release files..." -ForegroundColor Cyan
Copy-Item $ReleaseCli -Destination (Join-Path $StageDir "bin\ulvo.exe")
Copy-Item $ReleaseAgent -Destination (Join-Path $StageDir "bin\ulvo-agent.exe")
Copy-Item $ReleaseDesktop -Destination (Join-Path $StageDir "bin\ulvo-desktop.exe")

Copy-Item (Join-Path $RepoRoot "README.md") -Destination (Join-Path $StageDir "README.md") -ErrorAction SilentlyContinue
Copy-Item (Join-Path $RepoRoot "CHANGELOG.md") -Destination (Join-Path $StageDir "CHANGELOG.md") -ErrorAction SilentlyContinue
Copy-Item (Join-Path $RepoRoot "LICENSE") -Destination (Join-Path $StageDir "LICENSE") -ErrorAction SilentlyContinue

# Default config template (dynamic fallback on user systems)
$DefaultPrefs = @{
    vault_path = ""
    safety_reserve_mb = 500
    cache_budget_gb = 20.0
    auto_update = $false
    enable_gateway_path = $true
    enable_background_agent = $true
} | ConvertTo-Json -Depth 4
$DefaultPrefs | Set-Content (Join-Path $StageDir "config\preferences.default.json") -Encoding utf8

# Copy docs
Copy-Item (Join-Path $RepoRoot "docs\CLI.md") -Destination (Join-Path $StageDir "docs\CLI.md") -ErrorAction SilentlyContinue
Copy-Item (Join-Path $RepoRoot "docs\ARCHITECTURE.md") -Destination (Join-Path $StageDir "docs\ARCHITECTURE.md") -ErrorAction SilentlyContinue
Copy-Item (Join-Path $RepoRoot "docs\GUI_AND_BRANDING_SPECIFICATION.md") -Destination (Join-Path $StageDir "docs\GUI_AND_BRANDING_SPECIFICATION.md") -ErrorAction SilentlyContinue

# Copy installer and uninstaller scripts to both root and scripts/
Copy-Item (Join-Path $RepoRoot "scripts\install-ulvo.ps1") -Destination (Join-Path $StageDir "install.ps1") -ErrorAction SilentlyContinue
Copy-Item (Join-Path $RepoRoot "scripts\install-ulvo.ps1") -Destination (Join-Path $StageDir "scripts\install-ulvo.ps1") -ErrorAction SilentlyContinue
Copy-Item (Join-Path $RepoRoot "scripts\uninstall-ulvo.ps1") -Destination (Join-Path $StageDir "uninstall.ps1") -ErrorAction SilentlyContinue
Copy-Item (Join-Path $RepoRoot "scripts\uninstall-ulvo.ps1") -Destination (Join-Path $StageDir "scripts\uninstall-ulvo.ps1") -ErrorAction SilentlyContinue

# Create root launcher script in portable root
@"
@echo off
start "" "%~dp0bin\ulvo-desktop.exe"
"@ | Set-Content (Join-Path $StageDir "ULVO Control Center.bat")

# Compress portable zip
$ZipPath = Join-Path $DistPath "ulvo-v$Version-windows-x64-portable.zip"
Write-Host "==> Compressing portable archive to $ZipPath..." -ForegroundColor Cyan
Compress-Archive -Path "$StageDir\*" -DestinationPath $ZipPath -Force

# Generate SHA256SUMS.txt
Write-Host "==> Generating cryptographic SHA-256 checksums..." -ForegroundColor Cyan
$ChecksumLines = @()
Get-ChildItem -Path $StageDir -Recurse -File | ForEach-Object {
    $RelPath = $_.FullName.Substring($StageDir.Length + 1).Replace("\", "/")
    $Hash = (Get-FileHash -Path $_.FullName -Algorithm SHA256).Hash.ToLower()
    $ChecksumLines += "$Hash  $RelPath"
}
$ZipHash = (Get-FileHash -Path $ZipPath -Algorithm SHA256).Hash.ToLower()
$ChecksumLines += "$ZipHash  ulvo-v$Version-windows-x64-portable.zip"

$ChecksumFile = Join-Path $DistPath "SHA256SUMS.txt"
$ChecksumLines | Set-Content -Path $ChecksumFile -Encoding utf8

Write-Host "==> Release Distribution Package Created Successfully:" -ForegroundColor Green
Write-Host "  ZIP:       $ZipPath ($( [math]::Round((Get-Item $ZipPath).Length / 1MB, 2) ) MB)"
Write-Host "  ZIP SHA:   $ZipHash"
Write-Host "  CHECKSUMS: $ChecksumFile"
