$ErrorActionPreference = "Stop"

Add-Type -ReferencedAssemblies "System.Drawing", "System.Windows.Forms" -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using System.Drawing;
using System.Text;

public class PackagedGuiCap {
    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBmp, uint nFlags);
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")]
    public static extern bool PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
    public const uint WM_CLOSE = 0x0010;
    public const uint PW_RENDERFULLCONTENT = 2;

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
}
"@

$packagedExe = "E:\Workspace\Active\ULVO\dist\ulvo-v1.2.0-windows-x64-portable\bin\ulvo-desktop.exe"
if (-not (Test-Path $packagedExe)) {
    throw "Packaged binary not found at $packagedExe"
}

Write-Host "Target: $packagedExe"
Write-Host "Last Modified: $((Get-Item $packagedExe).LastWriteTime)"
Write-Host "Size: $((Get-Item $packagedExe).Length) bytes"

# Verify no node/vite running on port 1420
$tcp = Get-NetTCPConnection -LocalPort 1420 -ErrorAction SilentlyContinue
if ($tcp) {
    Write-Warning "Port 1420 is active! PID: $($tcp.OwningProcess)"
} else {
    Write-Host "[VERIFIED] Port 1420 is CLOSED. No dev server active." -ForegroundColor Green
}

$proc = Start-Process -FilePath $packagedExe -PassThru
try {
    Write-Host "Waiting 8 seconds for packaged GUI to launch and render embedded assets..."
    Start-Sleep -Seconds 8
    $hWnd = [PackagedGuiCap]::Find([uint32]$proc.Id)
    Write-Host "Found hWnd: $hWnd"
    if ($hWnd -ne [IntPtr]::Zero) {
        $bmp = [PackagedGuiCap]::Capture($hWnd)
        if ($bmp) {
            $evidencePath = "E:\Workspace\Active\ULVO\docs\evidence\stage3-4\packaged_gui_dogfood_capture.png"
            $artifactPath = "C:\Users\hp\.gemini\antigravity\brain\cbe8994d-2583-4467-b78f-42b2dc16d158\packaged_gui_dogfood_capture.png"
            $bmp.Save($evidencePath, [System.Drawing.Imaging.ImageFormat]::Png)
            $bmp.Save($artifactPath, [System.Drawing.Imaging.ImageFormat]::Png)
            $bmp.Dispose()
            Write-Host "[PASS] Saved dogfood screenshot to: $evidencePath ($((Get-Item $evidencePath).Length) bytes)" -ForegroundColor Green
            Write-Host "[PASS] Copied to artifact path: $artifactPath" -ForegroundColor Green
        } else {
            throw "Capture returned null bitmap"
        }
    } else {
        throw "Could not locate window for packaged GUI"
    }
    Start-Sleep -Seconds 2
    [PackagedGuiCap]::PostMessage($hWnd, [PackagedGuiCap]::WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    $proc.WaitForExit(4000)
    Write-Host "Packaged GUI cleanly exited with code: $($proc.ExitCode)" -ForegroundColor Green
} finally {
    if (-not $proc.HasExited) { $proc.Kill() }
}
