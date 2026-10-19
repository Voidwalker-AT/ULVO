# ULVO Section 6: Deep Performance, Latency & Process-Tree Profiling
$ErrorActionPreference = "Stop"

$ulvo = "E:\Workspace\Active\ULVO\target\release\ulvo.exe"
$desktopExe = "E:\Workspace\Active\ULVO\target\release\ulvo-desktop.exe"
$nexusProj = "E:\Workspace\Active\NexusNode\server"
$nativePython = "C:\Users\hp\AppData\Local\Programs\Python\Python313\python.exe"

$evidenceDir = "E:\Workspace\Active\ULVO\docs\evidence\stage3-1-3"
if (-not (Test-Path $evidenceDir)) {
    New-Item -ItemType Directory -Path $evidenceDir -Force | Out-Null
}
$jsonReportPath = "$evidenceDir\deep-performance-benchmark.json"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "     ULVO Section 6: Deep Performance & Process Tree        " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host ""

function Get-Stats {
    param([double[]]$Values)
    $sorted = $Values | Sort-Object
    $count = $sorted.Count
    $min = $sorted[0]
    $max = $sorted[-1]
    $avg = [math]::Round(($sorted | Measure-Object -Average).Average, 2)
    
    # Median
    $mid = [math]::Floor($count / 2)
    $median = if ($count % 2 -eq 0) {
        [math]::Round(($sorted[$mid - 1] + $sorted[$mid]) / 2, 2)
    } else {
        [math]::Round($sorted[$mid], 2)
    }

    # P95 (nearest rank)
    $p95Index = [math]::Min([math]::Ceiling(0.95 * $count) - 1, $count - 1)
    $p95 = [math]::Round($sorted[$p95Index], 2)

    return @{
        min = $min
        median = $median
        p95 = $p95
        max = $max
        mean = $avg
        samples = $sorted
    }
}

# --------------------------------------------------------------------------
# 1. Direct Native Python Execution Latency (5 runs)
# --------------------------------------------------------------------------
Write-Host ">>> 1. Benchmarking Direct Native Python Latency (5 iterations) <<<" -ForegroundColor Yellow
$nativeTimes = @()
for ($i = 1; $i -le 5; $i++) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $nativePython -ArgumentList "-c ""print(1)""" -NoNewWindow -Wait -PassThru
    $sw.Stop()
    $nativeTimes += [double]$sw.ElapsedMilliseconds
}
$nativeStats = Get-Stats -Values $nativeTimes
Write-Host "  Native Python: Min=$($nativeStats.min)ms, Median=$($nativeStats.median)ms, P95=$($nativeStats.p95)ms, Max=$($nativeStats.max)ms" -ForegroundColor DarkGray

# --------------------------------------------------------------------------
# 2. ULVO Cold Start Latency (3 iterations, cold after prune)
# --------------------------------------------------------------------------
Write-Host "`n>>> 2. Benchmarking ULVO Cold Execution Latency (3 iterations) <<<" -ForegroundColor Yellow
$coldTimes = @()
for ($i = 1; $i -le 3; $i++) {
    & $ulvo env prune | Out-Null
    Start-Sleep -Milliseconds 200
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $ulvo -ArgumentList "run $nexusProj -- python -c ""print(1)""" -NoNewWindow -Wait -PassThru
    $sw.Stop()
    if ($p.ExitCode -eq 0) {
        $coldTimes += [double]$sw.ElapsedMilliseconds
    }
}
$coldStats = Get-Stats -Values $coldTimes
Write-Host "  ULVO Cold:     Min=$($coldStats.min)ms, Median=$($coldStats.median)ms, P95=$($coldStats.p95)ms, Max=$($coldStats.max)ms" -ForegroundColor DarkGray

# --------------------------------------------------------------------------
# 3. ULVO Warm & Repeated Execution Latency (5 iterations)
# --------------------------------------------------------------------------
Write-Host "`n>>> 3. Benchmarking ULVO Warm & Repeated Latency (5 iterations) <<<" -ForegroundColor Yellow
$warmTimes = @()
for ($i = 1; $i -le 5; $i++) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $ulvo -ArgumentList "run $nexusProj -- python -c ""print(1)""" -NoNewWindow -Wait -PassThru
    $sw.Stop()
    if ($p.ExitCode -eq 0) {
        $warmTimes += [double]$sw.ElapsedMilliseconds
    }
}
$warmStats = Get-Stats -Values $warmTimes
Write-Host "  ULVO Warm:     Min=$($warmStats.min)ms, Median=$($warmStats.median)ms, P95=$($warmStats.p95)ms, Max=$($warmStats.max)ms" -ForegroundColor DarkGray

# --------------------------------------------------------------------------
# 4. Environment Reclamation Latency (ulvo env prune)
# --------------------------------------------------------------------------
Write-Host "`n>>> 4. Benchmarking Environment Reclamation Latency <<<" -ForegroundColor Yellow
$pruneTimes = @()
for ($i = 1; $i -le 3; $i++) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $ulvo -ArgumentList "env prune" -NoNewWindow -Wait -PassThru
    $sw.Stop()
    $pruneTimes += [double]$sw.ElapsedMilliseconds
}
$pruneStats = Get-Stats -Values $pruneTimes
Write-Host "  Env Prune:     Min=$($pruneStats.min)ms, Median=$($pruneStats.median)ms, P95=$($pruneStats.p95)ms, Max=$($pruneStats.max)ms" -ForegroundColor DarkGray

# --------------------------------------------------------------------------
# 5. Full Process-Tree Profiling during ULVO Run
# --------------------------------------------------------------------------
Write-Host "`n>>> 5. Profiling Full Process Tree (ULVO Parent + Child Python) <<<" -ForegroundColor Yellow
$parentJob = Start-Process -FilePath $ulvo -ArgumentList "run $nexusProj -- python -c ""import time; time.sleep(2.5)""" -NoNewWindow -PassThru
Start-Sleep -Milliseconds 600

$parentWS = 0
$parentPB = 0
$childWS = 0
$childPB = 0
$childPid = $null

$swSample = [System.Diagnostics.Stopwatch]::StartNew()
while (-not $parentJob.HasExited -and $swSample.ElapsedMilliseconds -lt 4000) {
    try {
        $parentJob.Refresh()
        $curParentWS = [math]::Round($parentJob.WorkingSet64 / 1MB, 2)
        $curParentPB = [math]::Round($parentJob.PrivateMemorySize64 / 1MB, 2)
        if ($curParentWS -gt $parentWS) { $parentWS = $curParentWS }
        if ($curParentPB -gt $parentPB) { $parentPB = $curParentPB }

        # Locate child python process
        $children = Get-CimInstance Win32_Process -Filter "ParentProcessId = $($parentJob.Id)"
        foreach ($c in $children) {
            $childProc = Get-Process -Id $c.ProcessId -ErrorAction SilentlyContinue
            if ($childProc) {
                $childPid = $c.ProcessId
                $cWS = [math]::Round($childProc.WorkingSet64 / 1MB, 2)
                $cPB = [math]::Round($childProc.PrivateMemorySize64 / 1MB, 2)
                if ($cWS -gt $childWS) { $childWS = $cWS }
                if ($cPB -gt $childPB) { $childPB = $cPB }
            }
        }
    } catch { }
    Start-Sleep -Milliseconds 100
}
$parentJob.WaitForExit()

Write-Host "  Parent ulvo.exe (PID $($parentJob.Id)): Peak WorkingSet = $parentWS MB, Peak PrivateBytes = $parentPB MB" -ForegroundColor DarkGray
Write-Host "  Child python.exe (PID $childPid): Peak WorkingSet = $childWS MB, Peak PrivateBytes = $childPB MB" -ForegroundColor DarkGray
Write-Host "  Total Process-Tree Peak WorkingSet: $([math]::Round($parentWS + $childWS, 2)) MB" -ForegroundColor DarkGray

# --------------------------------------------------------------------------
# 6. Desktop GUI Startup Latency & Process Tree (ulvo-desktop + WebView2)
# --------------------------------------------------------------------------
Write-Host "`n>>> 6. Profiling Desktop GUI & WebView2 Process Tree <<<" -ForegroundColor Yellow
$swGui = [System.Diagnostics.Stopwatch]::StartNew()
$guiProc = Start-Process -FilePath $desktopExe -PassThru
$guiPid = $guiProc.Id

# Wait up to 5 seconds for main window to appear
$windowReady = $false
for ($i = 0; $i -lt 50; $i++) {
    $guiProc.Refresh()
    if ($guiProc.MainWindowHandle -ne 0 -or $guiProc.HasExited) {
        $windowReady = $true
        break
    }
    Start-Sleep -Milliseconds 100
}
$swGui.Stop()
$guiStartupMs = $swGui.ElapsedMilliseconds

Write-Host "  GUI Main Window Created in: $guiStartupMs ms (WindowHandle: $($guiProc.MainWindowHandle))" -ForegroundColor DarkGray

# Let WebView2 children initialize
Start-Sleep -Milliseconds 1500
$guiProc.Refresh()
$guiWS = [math]::Round($guiProc.WorkingSet64 / 1MB, 2)
$guiPB = [math]::Round($guiProc.PrivateMemorySize64 / 1MB, 2)

# Find child WebView2 processes
$webChildren = Get-CimInstance Win32_Process -Filter "ParentProcessId = $guiPid"
$totalChildWS = 0.0
$totalChildPB = 0.0
$childCount = 0

foreach ($wc in $webChildren) {
    $cProc = Get-Process -Id $wc.ProcessId -ErrorAction SilentlyContinue
    if ($cProc) {
        $childCount++
        $cWS = [math]::Round($cProc.WorkingSet64 / 1MB, 2)
        $cPB = [math]::Round($cProc.PrivateMemorySize64 / 1MB, 2)
        $totalChildWS += $cWS
        $totalChildPB += $cPB
    }
}

Write-Host "  ulvo-desktop.exe (Main Window): WorkingSet = $guiWS MB, PrivateBytes = $guiPB MB" -ForegroundColor DarkGray
Write-Host "  WebView2 Child Subprocesses ($childCount): Total WS = $([math]::Round($totalChildWS, 2)) MB, Total PB = $([math]::Round($totalChildPB, 2)) MB" -ForegroundColor DarkGray
Write-Host "  Combined Desktop GUI Memory: WorkingSet = $([math]::Round($guiWS + $totalChildWS, 2)) MB" -ForegroundColor DarkGray

# Gracefully close GUI window
$guiProc.CloseMainWindow() | Out-Null
$exitedClean = $guiProc.WaitForExit(4000)
if (-not $exitedClean) {
    $guiProc.Kill()
}
Write-Host "  Desktop GUI Process exited cleanly: $exitedClean (ExitCode: $($guiProc.ExitCode))" -ForegroundColor DarkGray

# --------------------------------------------------------------------------
# 7. GPU Telemetry via nvidia-smi
# --------------------------------------------------------------------------
Write-Host "`n>>> 7. GPU Telemetry & VRAM Allocation <<<" -ForegroundColor Yellow
$gpuInfo = (& nvidia-smi --query-gpu=name,memory.total,memory.used,memory.free,driver_version,utilization.gpu --format=csv,noheader 2>&1 | Out-String).Trim()
$gpuParts = $gpuInfo -split ", "
$gpuReport = @{
    device_name = $gpuParts[0]
    total_vram = $gpuParts[1]
    used_vram = $gpuParts[2]
    free_vram = $gpuParts[3]
    driver_version = $gpuParts[4]
    gpu_utilization = if ($gpuParts.Count -ge 6) { $gpuParts[5] } else { "N/A" }
}
Write-Host "  GPU: $($gpuReport.device_name) | Driver: $($gpuReport.driver_version) | VRAM Used/Total: $($gpuReport.used_vram) / $($gpuReport.total_vram)" -ForegroundColor DarkGray

# --------------------------------------------------------------------------
# 8. Package Machine-Readable Benchmark Evidence JSON
# --------------------------------------------------------------------------
$benchmarkEvidence = @{
    schema_version = 1
    timestamp_utc = [DateTimeOffset]::UtcNow.ToString("o")
    host_os = "Windows 11 x64"
    hardware_profile = @{
        gpu = $gpuReport
    }
    execution_latencies_ms = @{
        native_python = $nativeStats
        ulvo_cold = $coldStats
        ulvo_warm = $warmStats
        environment_reclamation = $pruneStats
    }
    process_tree_memory_mb = @{
        cli_execution = @{
            parent_ulvo_ws = $parentWS
            parent_ulvo_pb = $parentPB
            child_python_ws = $childWS
            child_python_pb = $childPB
            total_tree_ws = [math]::Round($parentWS + $childWS, 2)
        }
        desktop_gui = @{
            startup_latency_ms = $guiStartupMs
            main_window_ws = $guiWS
            main_window_pb = $guiPB
            webview2_children_count = $childCount
            webview2_children_ws = [math]::Round($totalChildWS, 2)
            webview2_children_pb = [math]::Round($totalChildPB, 2)
            combined_gui_ws = [math]::Round($guiWS + $totalChildWS, 2)
            clean_exit = $exitedClean
        }
    }
}

$benchmarkEvidence | ConvertTo-Json -Depth 6 | Set-Content -Path $jsonReportPath -Encoding utf8
Write-Host "`n  Machine-readable benchmark report saved to: $jsonReportPath" -ForegroundColor Green

Write-Host "`n============================================================" -ForegroundColor Cyan
Write-Host "         DEEP PERFORMANCE BENCHMARK COMPLETED               " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
