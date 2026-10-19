$ulvo = "E:\Workspace\Active\ULVO\bin\ulvo.exe"
$project = "E:\Workspace\Active\CrossModalLab"

Write-Host "=== BASELINE LATENCY BENCHMARK ===" -ForegroundColor Cyan

# Native Python
$sw = [System.Diagnostics.Stopwatch]::StartNew()
& "E:\Workspace\Active\ULVO\vault\runtimes\python\cpython-3.11.16-windows-x86_64-none\python.exe" -c "pass"
$sw.Stop()
Write-Host "Native Vault Python: $($sw.ElapsedMilliseconds) ms"

# ulvo run warm
$times = @()
for ($i = 0; $i -lt 3; $i++) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    & $ulvo run $project python -c "pass" 2>$null
    $sw.Stop()
    $times += $sw.ElapsedMilliseconds
}
Write-Host "ulvo run Warm: $($times -join ', ') ms (Avg: $([math]::Round(($times | Measure-Object -Average).Average)) ms)"

# ulvo doctor
$sw = [System.Diagnostics.Stopwatch]::StartNew()
& $ulvo doctor 2>$null | Out-Null
$sw.Stop()
Write-Host "ulvo doctor: $($sw.ElapsedMilliseconds) ms"

# ulvo project status
$sw = [System.Diagnostics.Stopwatch]::StartNew()
& $ulvo project status $project 2>$null | Out-Null
$sw.Stop()
Write-Host "ulvo project status: $($sw.ElapsedMilliseconds) ms"
