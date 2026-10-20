$ErrorActionPreference = "Stop"

Add-Type -ReferencedAssemblies "System.Drawing", "System.Windows.Forms" -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using System.Drawing;
using System.Text;

public class WindowCapture {
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
$proc = Start-Process -FilePath $desktopExe -PassThru

try {
    Write-Host "Waiting for window creation and WebView2 render..."
    Start-Sleep -Seconds 5
    
    $hWnd = [WindowCapture]::Find([uint32]$proc.Id)
    Write-Host "Found hWnd: $hWnd"
    
    if ($hWnd -ne [IntPtr]::Zero) {
        $bmp = [WindowCapture]::CaptureWindow($hWnd)
        if ($bmp -ne $null) {
            $savePath = "E:\Workspace\Active\ULVO\docs\evidence\desktop_cockpit_captured.png"
            $artifactPath = "C:\Users\hp\.gemini\antigravity\brain\cbe8994d-2583-4467-b78f-42b2dc16d158\desktop_cockpit_captured.png"
            $bmp.Save($savePath, [System.Drawing.Imaging.ImageFormat]::Png)
            $bmp.Save($artifactPath, [System.Drawing.Imaging.ImageFormat]::Png)
            $bmp.Dispose()
            Write-Host "Screenshot saved to $savePath and $artifactPath" -ForegroundColor Green
            Write-Host "Image file size: $((Get-Item $savePath).Length) bytes"
        }
    }

    [WindowCapture]::PostMessage($hWnd, [WindowCapture]::WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    $proc.WaitForExit(4000)
    Write-Host "Process closed with code $($proc.ExitCode)"
} finally {
    if (-not $proc.HasExited) {
        $proc.Kill()
    }
}
