# Test Thin Project Connector Contract Initialization and Audit
$ErrorActionPreference = "Stop"

$ulvo = "E:\Workspace\Active\ULVO\target\release\ulvo.exe"
$testProj = Join-Path $env:TEMP "ulvo_test_project_dogfood"

if (Test-Path $testProj) {
    Remove-Item -Recurse -Force $testProj
}
New-Item -ItemType Directory -Path $testProj -Force | Out-Null

try {
    Write-Host "==> Initializing thin project connector at $testProj..." -ForegroundColor Cyan
    & $ulvo project init --ecosystem python --runtime-version 3.12.8 $testProj

    Write-Host "`n==> Inspecting project connector contract..." -ForegroundColor Cyan
    & $ulvo project connector $testProj

    Write-Host "`n==> Inspecting project status..." -ForegroundColor Cyan
    & $ulvo project status $testProj

    Write-Host "`n==> Inspecting generated .ulvo/manifest.toml..." -ForegroundColor Cyan
    $manifestPath = Join-Path $testProj ".ulvo\manifest.toml"
    Get-Content $manifestPath

    Write-Host "`n[OK] Thin project connector initialized and audited successfully!" -ForegroundColor Green
} finally {
    if (Test-Path $testProj) {
        Remove-Item -Recurse -Force $testProj -ErrorAction SilentlyContinue
    }
}
