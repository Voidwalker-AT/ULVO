# Qualification packaging only; never deploys or deletes an existing output.
[CmdletBinding()]
param(
    [ValidatePattern('^[0-9A-Za-z][0-9A-Za-z.-]{0,63}$')][string]$Version = '1.2.0-stage4-dev',
    [string]$DistDir = 'dist\stage4-dev',
    [switch]$ForceRebuild
)
$ErrorActionPreference = 'Stop'
$RepoRoot = (Get-Item -LiteralPath $PSScriptRoot).Parent.FullName
$DistPath = [IO.Path]::GetFullPath((Join-Path $RepoRoot $DistDir))
if (-not $DistPath.StartsWith($RepoRoot.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Output must be inside this repository.' }
if (Test-Path -LiteralPath $DistPath) { throw 'Output already exists. Choose a new versioned directory; packaging never deletes previous output.' }
if ($ForceRebuild) {
    Push-Location $RepoRoot
    try {
        & cargo build --locked --release -p ulvo-cli -p ulvo-agent
        if ($LASTEXITCODE) { throw 'CLI/agent build failed' }
        Push-Location (Join-Path $RepoRoot 'apps\ulvo-desktop')
        try { & npm run build; if ($LASTEXITCODE) { throw 'Frontend build failed' } } finally { Pop-Location }
        & cargo build --locked --release -p ulvo-desktop --features tauri/custom-protocol
        if ($LASTEXITCODE) { throw 'Desktop build failed' }
    } finally { Pop-Location }
}
$ReleaseBin = Join-Path $RepoRoot 'target\release'
$Names = @('ulvo.exe', 'ulvo-agent.exe', 'ulvo-desktop.exe', 'WebView2Loader.dll')
foreach ($name in $Names) { if (-not (Test-Path -LiteralPath (Join-Path $ReleaseBin $name) -PathType Leaf)) { throw "Missing release artifact: $name" } }
$StageDir = Join-Path $DistPath "ulvo-$Version-windows-x64-portable"
foreach ($dir in @('bin', 'docs', 'scripts')) { New-Item -ItemType Directory -Path (Join-Path $StageDir $dir) -Force | Out-Null }
foreach ($name in $Names) { Copy-Item -LiteralPath (Join-Path $ReleaseBin $name) -Destination (Join-Path $StageDir "bin\$name") }
Copy-Item -LiteralPath (Join-Path $RepoRoot 'README.md') -Destination $StageDir
foreach ($name in @('INSTALLATION.md', 'PRIVACY.md', 'TROUBLESHOOTING.md', 'STAGE4_RELEASE_MATRIX.md')) { Copy-Item -LiteralPath (Join-Path $RepoRoot "docs\$name") -Destination (Join-Path $StageDir 'docs') }
foreach ($kind in @('install', 'uninstall')) {
    Copy-Item -LiteralPath (Join-Path $RepoRoot "scripts\$kind-ulvo.ps1") -Destination (Join-Path $StageDir "$kind.ps1")
    Copy-Item -LiteralPath (Join-Path $RepoRoot "scripts\$kind-ulvo.ps1") -Destination (Join-Path $StageDir "scripts\$kind-ulvo.ps1")
}
@('@echo off', 'start "" "%~dp0bin\ulvo-desktop.exe"') | Set-Content -LiteralPath (Join-Path $StageDir 'ULVO Control Center.bat') -Encoding ascii
$Hashes = [ordered]@{}
foreach ($name in $Names) { $Hashes[$name] = (Get-FileHash -LiteralPath (Join-Path $ReleaseBin $name) -Algorithm SHA256).Hash.ToLowerInvariant() }
$Commit = & git -C $RepoRoot rev-parse HEAD
if ($LASTEXITCODE) { throw 'Cannot identify source commit' }
[ordered]@{ qualification_version = $Version; source_commit = $Commit; stable_release = $false; required_os = 'Windows 11 x64'; desktop_runtime = 'Microsoft WebView2 Evergreen'; binaries = $Hashes } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $StageDir 'build-manifest.json') -Encoding utf8
$ZipPath = Join-Path $DistPath "ulvo-$Version-windows-x64-portable.zip"
Compress-Archive -Path (Join-Path $StageDir '*') -DestinationPath $ZipPath
$Checksums = @(Get-ChildItem -LiteralPath $StageDir -Recurse -File | Sort-Object FullName | ForEach-Object { (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() + '  ' + $_.FullName.Substring($StageDir.Length + 1).Replace('\', '/') })
$Checksums += (Get-FileHash -LiteralPath $ZipPath -Algorithm SHA256).Hash.ToLowerInvariant() + '  ' + [IO.Path]::GetFileName($ZipPath)
$Checksums | Set-Content -LiteralPath (Join-Path $DistPath 'SHA256SUMS.txt') -Encoding utf8
Write-Output "Qualification package: $ZipPath"
