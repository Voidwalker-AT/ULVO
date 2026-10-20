$ErrorActionPreference = "Stop"

$agentExe = "E:\Workspace\Active\ULVO\target\release\ulvo-agent.exe"
$ulvoCli = "E:\Workspace\Active\ULVO\bin\ulvo.exe"

Write-Host "Starting agent daemon..." -ForegroundColor Cyan
$proc = Start-Process -FilePath $agentExe -ArgumentList "run" -PassThru -WindowStyle Hidden
try {
    Start-Sleep -Milliseconds 1200
    Write-Host "Started Agent PID: $($proc.Id)"
    
    Write-Host "Querying agent status..." -ForegroundColor Cyan
    & $ulvoCli agent status
    
    Write-Host "Triggering reconciliation cycle..." -ForegroundColor Cyan
    & $ulvoCli agent reconcile
    
    Write-Host "Stopping agent via IPC..." -ForegroundColor Cyan
    & $ulvoCli agent stop
    
    $proc.WaitForExit(4000)
    Write-Host "Agent exited cleanly with code: $($proc.ExitCode)" -ForegroundColor Green
} finally {
    if (-not $proc.HasExited) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    }
}
