# ULVO v1.0.0 Release Packaging Script (Windows x86_64)
# Automates deterministic packaging of release artifacts and checksum generation.

[CmdletBinding()]
param(
    [string]$Version = "1.0.0",
    [string]$DistDir = "dist"
)

$ErrorActionPreference = "Stop"

$RepoRoot = (Get-Item $PSScriptRoot).Parent.FullName
Set-Location $RepoRoot

Write-Host "==> Packaging ULVO v$Version for Windows x86_64..." -ForegroundColor Cyan

$ReleaseExe = Join-Path $RepoRoot "target\release\ulvo.exe"
if (-not (Test-Path $ReleaseExe)) {
    Write-Host "Release binary target\release\ulvo.exe not found. Building with cargo..." -ForegroundColor Yellow
    $env:CARGO_HOME = Join-Path $RepoRoot "vault\runtimes\rust\cargo"
    $env:RUSTUP_HOME = Join-Path $RepoRoot "vault\runtimes\rust\rustup"
    $env:PATH = "$env:CARGO_HOME\bin;C:\msys64\ucrt64\bin;C:\msys64\usr\bin;$env:PATH"
    cargo build --release --bin ulvo
}

# Verify binary executes
& $ReleaseExe --version
if ($LASTEXITCODE -ne 0) {
    throw "Release binary verification failed with exit code $LASTEXITCODE"
}

$DistPath = Join-Path $RepoRoot $DistDir
if (Test-Path $DistPath) {
    Remove-Item -Recurse -Force $DistPath
}
$StageDir = Join-Path $DistPath "ulvo-v$Version-windows-x86_64"
New-Item -ItemType Directory -Path $StageDir -Force | Out-Null

Write-Host "==> Staging release files..." -ForegroundColor Cyan
Copy-Item $ReleaseExe -Destination (Join-Path $StageDir "ulvo.exe")
Copy-Item (Join-Path $RepoRoot "README.md") -Destination (Join-Path $StageDir "README.md")
Copy-Item (Join-Path $RepoRoot "CHANGELOG.md") -Destination (Join-Path $StageDir "CHANGELOG.md")

$DocsTarget = Join-Path $StageDir "docs"
New-Item -ItemType Directory -Path $DocsTarget -Force | Out-Null
Copy-Item (Join-Path $RepoRoot "docs\CLI.md") -Destination (Join-Path $DocsTarget "CLI.md") -ErrorAction SilentlyContinue
Copy-Item (Join-Path $RepoRoot "docs\ARCHITECTURE.md") -Destination (Join-Path $DocsTarget "ARCHITECTURE.md") -ErrorAction SilentlyContinue
Copy-Item (Join-Path $RepoRoot "docs\DECISION_RECORD_V1_0_0_READINESS.md") -Destination (Join-Path $DocsTarget "DECISION_RECORD_V1_0_0_READINESS.md") -ErrorAction SilentlyContinue

$ZipPath = Join-Path $DistPath "ulvo-v$Version-windows-x86_64.zip"
Write-Host "==> Compressing to $ZipPath..." -ForegroundColor Cyan
Compress-Archive -Path "$StageDir\*" -DestinationPath $ZipPath -Force

$Hash = (Get-FileHash -Path $ZipPath -Algorithm SHA256).Hash.ToLower()
$ShaPath = "$ZipPath.sha256"
"$Hash  ulvo-v$Version-windows-x86_64.zip" | Set-Content -Path $ShaPath -Encoding utf8

Write-Host "==> Release Package Created Successfully:" -ForegroundColor Green
Write-Host "  ZIP:     $ZipPath ($( (Get-Item $ZipPath).Length ) bytes)"
Write-Host "  SHA256:  $Hash"
Write-Host "  CHECKSUM FILE: $ShaPath"
