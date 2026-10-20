$ErrorActionPreference = "Stop"

Add-Type -ReferencedAssemblies "System.Drawing", "System.Windows.Forms" -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using System.Drawing;
using System.Text;

public class GuiAcceptanceHelper {
    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBmp, uint nFlags);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);

    public const uint WM_CLOSE = 0x0010;
    public const uint PW_RENDERFULLCONTENT = 2;

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    public static Bitmap CaptureWindow(IntPtr hWnd) {
        RECT rect;
        GetWindowRect(hWnd, out rect);
        int width = rect.Right - rect.Left;
        int height = rect.Bottom - rect.Top;
        if (width <= 0 || height <= 0) return null;

        Bitmap bmp = new Bitmap(width, height);
        using (Graphics g = Graphics.FromImage(bmp)) {
            IntPtr hdc = g.GetHdc();
            try {
                bool success = PrintWindow(hWnd, hdc, PW_RENDERFULLCONTENT);
                if (!success) {
                    PrintWindow(hWnd, hdc, 0);
                }
            } finally {
                g.ReleaseHdc(hdc);
            }
        }
        return bmp;
    }

    public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumProc proc, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder sb, int max);

    public static IntPtr Find(uint targetPid) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((hWnd, lp) => {
            uint pid;
            GetWindowThreadProcessId(hWnd, out pid);
            if (pid == targetPid && IsWindowVisible(hWnd)) {
                StringBuilder sb = new StringBuilder(256);
                GetWindowText(hWnd, sb, 256);
                if (sb.ToString().Contains("ULVO")) {
                    found = hWnd;
                    return false;
                }
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
}
"@

$desktopExe = "E:\Workspace\Active\ULVO\target\release\ulvo-desktop.exe"
$agentExe = "E:\Workspace\Active\ULVO\target\release\ulvo-agent.exe"
$ulvoCli = "E:\Workspace\Active\ULVO\bin\ulvo.exe"
$evidenceDir = "E:\Workspace\Active\ULVO\docs\evidence\stage3-4"
if (-not (Test-Path $evidenceDir)) {
    New-Item -ItemType Directory -Path $evidenceDir -Force | Out-Null
}

$evidenceJson = "$evidenceDir\gui-acceptance-verification.json"
$screenshotPath = "$evidenceDir\desktop_cockpit_acceptance.png"
$artifactScreenshot = "C:\Users\hp\.gemini\antigravity\brain\cbe8994d-2583-4467-b78f-42b2dc16d158\desktop_cockpit_acceptance.png"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "    ULVO Stage 3.4 / v1.2.0 Complete GUI Acceptance Test   " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

$testResults = [ordered]@{
    timestamp = [DateTimeOffset]::UtcNow.ToString("o")
    checks = @{}
    telemetry = @{}
    verdict = "FAILED"
}

# 1. Start background agent
Write-Host "`n>>> [1/7] Starting Background Agent Daemon..." -ForegroundColor Yellow
$agentProc = Start-Process -FilePath $agentExe -ArgumentList "run" -PassThru -WindowStyle Hidden
Start-Sleep -Milliseconds 1200
$agentStatus = (& $ulvoCli agent status 2>&1 | Out-String)
$agentOnline = $agentStatus -match "ONLINE"
$testResults.checks["agent_online"] = $agentOnline
Write-Host "  Agent Status: $(if ($agentOnline) {'ONLINE (PASS)'} else {'OFFLINE (FAIL)'})" -ForegroundColor $(if ($agentOnline) {'Green'} else {'Red'})

# 2. Launch Desktop GUI Cockpit
Write-Host "`n>>> [2/7] Launching ULVO Control Center..." -ForegroundColor Yellow
$guiProc = Start-Process -FilePath $desktopExe -PassThru
$hWnd = [IntPtr]::Zero

try {
    $timeout = [DateTime]::UtcNow.AddSeconds(12)
    while ([DateTime]::UtcNow -lt $timeout -and $hWnd -eq [IntPtr]::Zero) {
        $guiProc.Refresh()
        if ($guiProc.HasExited) {
            throw "ulvo-desktop.exe exited prematurely with code $($guiProc.ExitCode)"
        }
        $hWnd = [GuiAcceptanceHelper]::Find([uint32]$guiProc.Id)
        if ($hWnd -eq [IntPtr]::Zero) {
            Start-Sleep -Milliseconds 400
        }
    }

    if ($hWnd -eq [IntPtr]::Zero) {
        throw "Failed to locate main visible window for ulvo-desktop.exe"
    }

    $titleSb = New-Object System.Text.StringBuilder 256
    [GuiAcceptanceHelper]::GetWindowText($hWnd, $titleSb, 256) | Out-Null
    $rect = New-Object GuiAcceptanceHelper+RECT
    [GuiAcceptanceHelper]::GetWindowRect($hWnd, [ref]$rect) | Out-Null
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top

    $guiProc.Refresh()
    $wsMB = [math]::Round($guiProc.WorkingSet64 / 1MB, 2)
    $privMB = [math]::Round($guiProc.PrivateMemorySize64 / 1MB, 2)

    $edgeProcs = Get-Process -Name "msedgewebview2" -ErrorAction SilentlyContinue | Where-Object {
        $parent = (Get-CimInstance Win32_Process -Filter "ProcessId = $($_.Id)").ParentProcessId
        $parent -eq $guiProc.Id
    }
    $totalEdgeWs = 0.0
    foreach ($ep in $edgeProcs) {
        $totalEdgeWs += [math]::Round($ep.WorkingSet64 / 1MB, 2)
    }

    $testResults.telemetry = [ordered]@{
        pid = $guiProc.Id
        hwnd = $hWnd.ToInt64()
        title = $titleSb.ToString()
        geometry = "${width}x${height}"
        responding = $guiProc.Responding
        main_ws_mb = $wsMB
        private_mb = $privMB
        webview2_count = $edgeProcs.Count
        webview2_total_ws_mb = [math]::Round($totalEdgeWs, 2)
        combined_ws_mb = [math]::Round($wsMB + $totalEdgeWs, 2)
    }

    Write-Host "  Window Title:      '$($titleSb.ToString())'" -ForegroundColor DarkGray
    Write-Host "  Window Geometry:   ${width}x${height} at ($($rect.Left), $($rect.Top))" -ForegroundColor DarkGray
    Write-Host "  Responding:        $($guiProc.Responding)" -ForegroundColor DarkGray
    Write-Host "  Combined WS:       $([math]::Round($wsMB + $totalEdgeWs, 2)) MB ($($edgeProcs.Count) WebView2 engines)" -ForegroundColor DarkGray
    $testResults.checks["window_created"] = $true
    $testResults.checks["window_responding"] = $guiProc.Responding

    # 3. Capture Real Screenshot via PrintWindow after first paint
    Write-Host "`n>>> [3/7] Capturing Real Window Screenshot..." -ForegroundColor Yellow
    Write-Host "  Waiting 6 seconds for WebView2 engine first paint and React mount..." -ForegroundColor DarkGray
    Start-Sleep -Seconds 6

    $bmp = [GuiAcceptanceHelper]::CaptureWindow($hWnd)
    if ($bmp -ne $null) {
        $bmp.Save($screenshotPath, [System.Drawing.Imaging.ImageFormat]::Png)
        $bmp.Save($artifactScreenshot, [System.Drawing.Imaging.ImageFormat]::Png)
        $bmp.Dispose()
        $imgBytes = (Get-Item $screenshotPath).Length
        Write-Host "  [PASS] Screenshot captured: $imgBytes bytes saved to $screenshotPath" -ForegroundColor Green
        
        # Verify rendered content: blank white window is ~8 KB; fully rendered UI is > 40 KB
        if ($imgBytes -lt 30000) {
            throw "UI content validation failed: Captured screenshot is only $imgBytes bytes (blank or unpainted window)"
        }
        $testResults.checks["screenshot_captured"] = $true
        $testResults.telemetry["screenshot_bytes"] = $imgBytes
    } else {
        throw "Failed to capture window bitmap"
    }

    # 4. Verify Backend Views & Catalog Search
    Write-Host "`n>>> [4/7] Verifying Backend Views & Catalog Search..." -ForegroundColor Yellow
    $catSearch = (& $ulvoCli catalog search python 2>&1 | Out-String)
    $hasPythonCatalog = $catSearch -match "python"
    $testResults.checks["catalog_search"] = $hasPythonCatalog
    Write-Host "  Catalog Search ('python'): $(if ($hasPythonCatalog) {'PASS'} else {'FAIL'})" -ForegroundColor $(if ($hasPythonCatalog) {'Green'} else {'Red'})

    $storageRep = (& $ulvoCli storage report 2>&1 | Out-String)
    $hasStorageRep = $storageRep -match "Storage & Deduplication Intelligence"
    $testResults.checks["storage_report"] = $hasStorageRep
    Write-Host "  Storage Intelligence View: $(if ($hasStorageRep) {'PASS'} else {'FAIL'})" -ForegroundColor $(if ($hasStorageRep) {'Green'} else {'Red'})

    $projConn = (& $ulvoCli project connector E:\Workspace\Active\NexusNode\server 2>&1 | Out-String)
    $hasProjConn = $projConn -match "NexusNode"
    $testResults.checks["project_connector"] = $hasProjConn
    Write-Host "  Project Connector View:    $(if ($hasProjConn) {'PASS'} else {'FAIL'})" -ForegroundColor $(if ($hasProjConn) {'Green'} else {'Red'})

    $projList = (& $ulvoCli project list 2>&1 | Out-String)
    $hasProjects = ($projList -match "ULVO") -and ($projList -match "CrossModalLab")
    $testResults.checks["project_registry"] = $hasProjects
    Write-Host "  Project Registry View:     $(if ($hasProjects) {'PASS'} else {'FAIL'})" -ForegroundColor $(if ($hasProjects) {'Green'} else {'Red'})

    $envAudit = (& $ulvoCli env audit 2>&1 | Out-String)
    $hasEnvAudit = $envAudit -match "Vault Environments Audit"
    $testResults.checks["environment_audit"] = $hasEnvAudit
    Write-Host "  Environment Lifecycle:     $(if ($hasEnvAudit) {'PASS'} else {'FAIL'})" -ForegroundColor $(if ($hasEnvAudit) {'Green'} else {'Red'})

    # 5. User Settings Persistence
    Write-Host "`n>>> [5/7] Verifying User Settings Persistence..." -ForegroundColor Yellow
    $prefFile = "E:\Workspace\Active\ULVO\vault\config\preferences.json"
    $prefExists = Test-Path $prefFile
    $validJson = $false
    if ($prefExists) {
        try {
            $parsed = Get-Content $prefFile -Raw | ConvertFrom-Json
            if ($parsed.vault_path -and $parsed.safety_reserve_mb) {
                $validJson = $true
            }
        } catch {}
    }
    $testResults.checks["preferences_file"] = ($prefExists -and $validJson)
    Write-Host "  Preferences File:          $(if ($prefExists -and $validJson) {'PASS (Exists & Valid)'} else {'FAIL'})" -ForegroundColor $(if ($prefExists -and $validJson) {'Green'} else {'Red'})

    # 6. Clean Exit via WM_CLOSE
    Write-Host "`n>>> [6/7] Testing Clean Graceful Exit via WM_CLOSE..." -ForegroundColor Yellow
    [GuiAcceptanceHelper]::PostMessage($hWnd, [GuiAcceptanceHelper]::WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    $closed = $guiProc.WaitForExit(8000)
    if (-not $closed) {
        try { $guiProc.CloseMainWindow() | Out-Null } catch {}
        $closed = $guiProc.WaitForExit(4000)
    }
    $testResults.checks["clean_exit"] = $closed
    Write-Host "  Graceful WM_CLOSE Exit:    $(if ($closed) {'PASS'} else {'FAIL'}) (ExitCode: $($guiProc.ExitCode))" -ForegroundColor $(if ($closed) {'Green'} else {'Red'})

    # 7. Stop background agent
    Write-Host "`n>>> [7/7] Stopping Background Agent..." -ForegroundColor Yellow
    & $ulvoCli agent stop | Out-Null
    $agentProc.WaitForExit(3000)
    $testResults.checks["agent_stopped"] = $agentProc.HasExited
    Write-Host "  Agent Clean Shutdown:      $(if ($agentProc.HasExited) {'PASS'} else {'FAIL'})" -ForegroundColor $(if ($agentProc.HasExited) {'Green'} else {'Red'})

    $allPassed = ($testResults.checks.Values -notcontains $false)
    $testResults.verdict = if ($allPassed) { "PASS" } else { "FAILED" }

} finally {
    if (-not $guiProc.HasExited) {
        $guiProc.Kill()
    }
    if (-not $agentProc.HasExited) {
        $agentProc.Kill()
    }
}

$testResults | ConvertTo-Json -Depth 5 | Set-Content -Path $evidenceJson -Encoding utf8
Write-Host "`n============================================================" -ForegroundColor Cyan
Write-Host "  GUI Acceptance Result: $($testResults.verdict)" -ForegroundColor $(if ($testResults.verdict -eq "PASS") {'Green'} else {'Red'})
Write-Host "  Evidence saved to: $evidenceJson" -ForegroundColor DarkGray
Write-Host "============================================================" -ForegroundColor Cyan

if ($testResults.verdict -ne "PASS") {
    exit 1
}
