$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using System.Text;

public class Win32Gui {
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Auto)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);

    public const uint WM_CLOSE = 0x0010;

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }
}
"@

$desktopExe = "E:\Workspace\Active\ULVO\target\release\ulvo-desktop.exe"
if (-not (Test-Path $desktopExe)) {
    throw "ulvo-desktop.exe not found at $desktopExe"
}

Write-Host "=== LAUNCHING REAL DESKTOP COCKPIT: $desktopExe ==="
$proc = Start-Process -FilePath $desktopExe -PassThru

try {
    Write-Host "Waiting up to 10 seconds for window creation and WebView2 initialization..."
    $timeout = [DateTime]::Now.AddSeconds(10)
    $hWnd = [IntPtr]::Zero
    while ([DateTime]::Now -lt $timeout -and $hWnd -eq [IntPtr]::Zero) {
        $proc.Refresh()
        if ($proc.HasExited) {
            throw "ulvo-desktop.exe exited prematurely with code $($proc.ExitCode)"
        }
        $hWnd = $proc.MainWindowHandle
        if ($hWnd -eq [IntPtr]::Zero) {
            Start-Sleep -Milliseconds 500
        }
    }

    if ($hWnd -eq [IntPtr]::Zero) {
        # Check child processes (WebView2 runtime)
        $children = Get-CimInstance Win32_Process | Where-Object { $_.ParentProcessId -eq $proc.Id }
        Write-Host "Process ID: $($proc.Id), Child Processes: $($children.Count)"
        foreach ($c in $children) {
            Write-Host "  Child PID: $($c.ProcessId), Name: $($c.Name)"
        }
    }

    $title = New-Object System.Text.StringBuilder 256
    [Win32Gui]::GetWindowText($hWnd, $title, 256) | Out-Null
    $rect = New-Object Win32Gui+RECT
    [Win32Gui]::GetWindowRect($hWnd, [ref]$rect) | Out-Null
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top

    Write-Host "`n=== DESKTOP COCKPIT TELEMETRY ===" -ForegroundColor Green
    Write-Host "  PID:               $($proc.Id)" -ForegroundColor Green
    Write-Host "  Window Handle:     $hWnd" -ForegroundColor Green
    Write-Host "  Window Title:      '$($title.ToString())'" -ForegroundColor Green
    Write-Host "  Window Dimensions: ${width}x${height} (Left=$($rect.Left), Top=$($rect.Top))" -ForegroundColor Green
    Write-Host "  Process Status:    Responding=$($proc.Responding)" -ForegroundColor Green

    # Verify WebView2 processes spawned by Tauri
    $edgeProcs = Get-Process -Name "msedgewebview2" -ErrorAction SilentlyContinue | Where-Object {
        $parent = (Get-CimInstance Win32_Process -Filter "ProcessId = $($_.Id)").ParentProcessId
        $parent -eq $proc.Id
    }
    Write-Host "  WebView2 Engines:  $($edgeProcs.Count) active renderer/utility process(es)" -ForegroundColor Green

    Start-Sleep -Seconds 3

    # Clean Exit via WM_CLOSE
    Write-Host "`n==> Testing clean graceful exit via WM_CLOSE..." -ForegroundColor Cyan
    if ($hWnd -ne [IntPtr]::Zero) {
        [Win32Gui]::PostMessage($hWnd, [Win32Gui]::WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    } else {
        $proc.CloseMainWindow() | Out-Null
    }

    $closed = $proc.WaitForExit(5000)
    if ($closed) {
        Write-Host "  [OK] ulvo-desktop.exe exited cleanly with code $($proc.ExitCode)." -ForegroundColor Green
    } else {
        Write-Host "  Force terminating..." -ForegroundColor Yellow
        $proc.Kill()
        Write-Host "  Process terminated."
    }

} catch {
    if (-not $proc.HasExited) {
        $proc.Kill()
    }
    throw $_
}
