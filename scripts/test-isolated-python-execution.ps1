$ErrorActionPreference = "Stop"

$workspaceRoot = "E:\Workspace\Active\ULVO"
$ulvoExe = "$workspaceRoot\bin\ulvo.exe"
$projectRoot = "E:\Workspace\Active\CrossModalLab"

Write-Host "Testing ULVO Python Execution Independence..." -ForegroundColor Cyan

# Test running Python via ULVO gateway
$out = & $ulvoExe run $projectRoot python -c "import sys; print(f'Python Interpreter: {sys.executable}'); print(f'Python Version: {sys.version}')" 2>&1
Write-Host "Gateway Output:"
$out | ForEach-Object { Write-Host "  $_" }

$matched = $out -match "Python Interpreter:"
if (-not $matched) {
    throw "Failed to execute Python through ULVO gateway"
}

Write-Host "ULVO Gateway Python Execution: PASS" -ForegroundColor Green
