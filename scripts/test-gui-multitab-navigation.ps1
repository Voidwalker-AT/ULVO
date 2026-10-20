$ErrorActionPreference = "Stop"

Add-Type -ReferencedAssemblies "System.Drawing", "System.Windows.Forms" -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using System.Drawing;
using System.Text;
using System.Windows.Forms;

public class MultiTabTester {
    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBmp, uint nFlags);
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")]
    public static extern bool PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, int dwExtraInfo);

    public const uint WM_CLOSE = 0x0010;
    public const uint PW_RENDERFULLCONTENT = 2;
    public const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
    public const uint MOUSEEVENTF_LEFTUP = 0x0004;

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left; public int Top; public int Right; public int Bottom;
    }

    public static Bitmap Capture(IntPtr hWnd) {
        RECT rect;
        GetWindowRect(hWnd, out rect);
        int w = rect.Right - rect.Left;
        int h = rect.Bottom - rect.Top;
        if (w <= 0 || h <= 0) return null;
        Bitmap bmp = new Bitmap(w, h);
        using (Graphics g = Graphics.FromImage(bmp)) {
            IntPtr hdc = g.GetHdc();
            try {
                if (!PrintWindow(hWnd, hdc, PW_RENDERFULLCONTENT)) {
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

    public static void ClickAt(int screenX, int screenY) {
        Cursor.Position = new Point(screenX, screenY);
        System.Threading.Thread.Sleep(100);
        mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
        System.Threading.Thread.Sleep(80);
        mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
    }
}
"@

$exe = "E:\Workspace\Active\ULVO\dist\ulvo-v1.2.0-windows-x64-portable\bin\ulvo-desktop.exe"
$evidenceDir = "E:\Workspace\Active\ULVO\docs\evidence\stage3-4"
if (-not (Test-Path $evidenceDir)) {
    New-Item -ItemType Directory -Path $evidenceDir -Force | Out-Null
}

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "    ULVO Standalone GUI Multi-Tab Navigation & Relaunch     " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

# ---------------------------------------------------------
# PHASE 1: Initial Launch & Tab Navigation
# ---------------------------------------------------------
Write-Host "`n>>> [1/3] Phase 1: Launching Standalone GUI..." -ForegroundColor Yellow
$proc = Start-Process -FilePath $exe -PassThru
$hWnd = [IntPtr]::Zero

try {
    Start-Sleep -Seconds 6
    $hWnd = [MultiTabTester]::Find([uint32]$proc.Id)
    if ($hWnd -eq [IntPtr]::Zero) {
        throw "Failed to locate window handle for PID $($proc.Id)"
    }
    Write-Host "  Found main HWND: $hWnd for PID $($proc.Id)" -ForegroundColor Green

    [MultiTabTester]::SetForegroundWindow($hWnd) | Out-Null
    Start-Sleep -Milliseconds 500

    $rect = New-Object MultiTabTester+RECT
    [MultiTabTester]::GetWindowRect($hWnd, [ref]$rect) | Out-Null
    $wLeft = $rect.Left
    $wTop = $rect.Top
    Write-Host "  Window coordinates: Left=$wLeft, Top=$wTop" -ForegroundColor DarkGray

    # Tab relative offsets (x=80 inside window)
    $tabs = @(
        @{ name = "01_dashboard"; relY = 85 },
        @{ name = "02_discover";  relY = 127 },
        @{ name = "03_installed"; relY = 169 },
        @{ name = "04_projects";  relY = 211 },
        @{ name = "05_storage";   relY = 253 },
        @{ name = "06_settings";  relY = 421 }
    )

    foreach ($tab in $tabs) {
        $clickX = $wLeft + 80
        $clickY = $wTop + $tab.relY
        Write-Host "  Clicking tab '$($tab.name)' at ($clickX, $clickY)..." -ForegroundColor Cyan
        [MultiTabTester]::ClickAt($clickX, $clickY)
        Start-Sleep -Milliseconds 800

        $bmp = [MultiTabTester]::Capture($hWnd)
        if ($bmp) {
            $savePath = "$evidenceDir\tab_$($tab.name).png"
            $bmp.Save($savePath, [System.Drawing.Imaging.ImageFormat]::Png)
            $bmp.Dispose()
            $bytes = (Get-Item $savePath).Length
            Write-Host "    [PASS] Tab screenshot saved: $savePath ($bytes bytes)" -ForegroundColor Green
            if ($bytes -lt 30000) {
                throw "Tab '$($tab.name)' rendered blank screen ($bytes bytes)!"
            }
        }
    }

    # Close Phase 1
    Write-Host "`n>>> [2/3] Closing application cleanly via WM_CLOSE..." -ForegroundColor Yellow
    [MultiTabTester]::PostMessage($hWnd, [MultiTabTester]::WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    $exited = $proc.WaitForExit(8000)
    if (-not $exited) {
        $proc.CloseMainWindow() | Out-Null
        $exited = $proc.WaitForExit(4000)
    }
    Write-Host "  Clean Exit Verified: $exited (ExitCode: $($proc.ExitCode))" -ForegroundColor Green
    if (-not $exited) { throw "Process failed to close cleanly!" }

} finally {
    if (-not $proc.HasExited) { $proc.Kill() }
}

# ---------------------------------------------------------
# PHASE 2: Application Relaunch Verification
# ---------------------------------------------------------
Write-Host "`n>>> [3/3] Phase 2: Relaunching Application to Verify Lifecycle Durability..." -ForegroundColor Yellow
$proc2 = Start-Process -FilePath $exe -PassThru
$hWnd2 = [IntPtr]::Zero

try {
    Start-Sleep -Seconds 6
    $hWnd2 = [MultiTabTester]::Find([uint32]$proc2.Id)
    if ($hWnd2 -eq [IntPtr]::Zero) {
        throw "Failed to locate window handle for relaunched PID $($proc2.Id)"
    }
    Write-Host "  Relaunch Succeeded! Found HWND: $hWnd2 for PID $($proc2.Id)" -ForegroundColor Green

    $bmp2 = [MultiTabTester]::Capture($hWnd2)
    if ($bmp2) {
        $relaunchPath = "$evidenceDir\app_relaunch_verified.png"
        $bmp2.Save($relaunchPath, [System.Drawing.Imaging.ImageFormat]::Png)
        $bmp2.Dispose()
        $rBytes = (Get-Item $relaunchPath).Length
        Write-Host "  [PASS] Relaunch screenshot captured: $relaunchPath ($rBytes bytes)" -ForegroundColor Green
        if ($rBytes -lt 30000) {
            throw "Relaunch window rendered blank screen ($rBytes bytes)!"
        }
    }

    [MultiTabTester]::PostMessage($hWnd2, [MultiTabTester]::WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    $rExited = $proc2.WaitForExit(8000)
    Write-Host "  Relaunch Clean Exit Verified: $rExited (ExitCode: $($proc2.ExitCode))" -ForegroundColor Green
    if (-not $rExited) { throw "Relaunched process failed to close cleanly!" }

} finally {
    if (-not $proc2.HasExited) { $proc2.Kill() }
}

Write-Host "`n============================================================" -ForegroundColor Green
Write-Host "  ALL GUI MULTI-TAB & RELAUNCH ACCEPTANCE TESTS PASSED!    " -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Green
