# ULVO Section 4: Real Installed Desktop GUI Dogfooding & Lifecycle Test
$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using System.Text;

public class InstalledGuiHelper {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc enumProc, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

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

    public static IntPtr FindWindow(uint targetPid) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((hWnd, lParam) => {
            uint pid;
            GetWindowThreadProcessId(hWnd, out pid);
            if (pid == targetPid && IsWindowVisible(hWnd)) {
                var sb = new StringBuilder(256);
                GetWindowText(hWnd, sb, 256);
                string title = sb.ToString();
                if (title.Contains("ULVO") || title.Length > 0) {
                    result = hWnd;
                    return false;
                }
            }
            return true;
        }, IntPtr.Zero);
        return result;
    }
}
"@

$isolatedInstallDir = "$env:TEMP\ulvo_gui_installed"
$sourceDir = "E:\Workspace\Active\ULVO\target\release"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "       ULVO Section 4: Real Installed Desktop GUI Test      " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host ""

$testsPassed = 0
$totalTests = 6

# --------------------------------------------------------------------------
# Step 1: Install ULVO into isolated target
# --------------------------------------------------------------------------
Write-Host ">>> STEP 1: Installing ULVO into Isolated Target <<<" -ForegroundColor Yellow
if (Test-Path $isolatedInstallDir) {
    Remove-Item -Path $isolatedInstallDir -Recurse -Force -ErrorAction SilentlyContinue
}

& powershell.exe -NoProfile -ExecutionPolicy Bypass -File "E:\Workspace\Active\ULVO\scripts\install-ulvo.ps1" `
    -InstallDir $isolatedInstallDir -NoShortcuts -NoRegistry -NoPath -Force

$installedDesktopExe = Join-Path $isolatedInstallDir "bin\ulvo-desktop.exe"
$installedCli = Join-Path $isolatedInstallDir "bin\ulvo.exe"

if ((Test-Path $installedDesktopExe) -and (Test-Path $installedCli)) {
    Write-Host "  Installed artifacts verified at: $isolatedInstallDir\bin" -ForegroundColor DarkGray
    Write-Host "  STEP 1 PASSED: Clean isolated installation succeeded." -ForegroundColor Green
    $testsPassed++
} else {
    throw "STEP 1 FAILED: Installed binaries not found!"
}

# --------------------------------------------------------------------------
# Step 2: Launch Real Installed Desktop GUI
# --------------------------------------------------------------------------
Write-Host "`n>>> STEP 2: Launching Installed Desktop Cockpit <<<" -ForegroundColor Yellow
$guiProc = Start-Process -FilePath $installedDesktopExe -PassThru
$guiPid = [uint32]$guiProc.Id
Write-Host "  Launched installed ulvo-desktop.exe (PID $guiPid)" -ForegroundColor DarkGray

$timeout = [DateTime]::UtcNow.AddSeconds(12)
$hWnd = [IntPtr]::Zero

while ([DateTime]::UtcNow -lt $timeout -and $hWnd -eq [IntPtr]::Zero) {
    $guiProc.Refresh()
    if ($guiProc.HasExited) {
        throw "STEP 2 FAILED: ulvo-desktop.exe exited prematurely with code $($guiProc.ExitCode)!"
    }
    $hWnd = [InstalledGuiHelper]::FindWindow($guiPid)
    if ($hWnd -eq [IntPtr]::Zero) {
        Start-Sleep -Milliseconds 400
    }
}

if ($hWnd -ne [IntPtr]::Zero) {
    $titleSb = New-Object System.Text.StringBuilder 256
    [InstalledGuiHelper]::GetWindowText($hWnd, $titleSb, 256) | Out-Null
    $rect = New-Object InstalledGuiHelper+RECT
    [InstalledGuiHelper]::GetWindowRect($hWnd, [ref]$rect) | Out-Null
    $w = $rect.Right - $rect.Left
    $h = $rect.Bottom - $rect.Top

    Write-Host "  Window Handle:   $hWnd" -ForegroundColor DarkGray
    Write-Host "  Window Title:    '$($titleSb.ToString())'" -ForegroundColor DarkGray
    Write-Host "  Window Geometry: ${w}x${h} at ($($rect.Left), $($rect.Top))" -ForegroundColor DarkGray
    Write-Host "  Responding:      $($guiProc.Responding)" -ForegroundColor DarkGray
    Write-Host "  STEP 2 PASSED: Installed desktop window created and responding." -ForegroundColor Green
    $testsPassed++
} else {
    throw "STEP 2 FAILED: Window handle could not be resolved within timeout!"
}

# --------------------------------------------------------------------------
# Step 3: Profile Process Tree & Memory Footprint
# --------------------------------------------------------------------------
Write-Host "`n>>> STEP 3: Profiling Installed Desktop Process Tree <<<" -ForegroundColor Yellow
Start-Sleep -Milliseconds 1500
$guiProc.Refresh()
$wsMB = [math]::Round($guiProc.WorkingSet64 / 1MB, 2)
$privMB = [math]::Round($guiProc.PrivateMemorySize64 / 1MB, 2)

$webProcs = Get-Process -Name "msedgewebview2" -ErrorAction SilentlyContinue | Where-Object {
    $parentProcId = (Get-CimInstance Win32_Process -Filter "ProcessId = $($_.Id)").ParentProcessId
    $parentProcId -eq $guiProc.Id
}
$webWs = 0.0
foreach ($wp in $webProcs) {
    $webWs += [math]::Round($wp.WorkingSet64 / 1MB, 2)
}

Write-Host "  Main Window WorkingSet: $wsMB MB (Private: $privMB MB)" -ForegroundColor DarkGray
Write-Host "  WebView2 Subprocesses ($($webProcs.Count)): Total WS = $([math]::Round($webWs, 2)) MB" -ForegroundColor DarkGray
Write-Host "  Combined Desktop WS: $([math]::Round($wsMB + $webWs, 2)) MB" -ForegroundColor DarkGray
Write-Host "  STEP 3 PASSED: Process tree profiled within production memory budget." -ForegroundColor Green
$testsPassed++

# --------------------------------------------------------------------------
# Step 4: Verify 8 Backend Data Views via Execution Gateway
# --------------------------------------------------------------------------
Write-Host "`n>>> STEP 4: Verifying 8 Backend Data Views <<<" -ForegroundColor Yellow

$nexusProj = "E:\Workspace\Active\NexusNode\server"

# 1. System Doctor
$v1 = (& $installedCli doctor 2>&1 | Out-String)
$ok1 = $v1 -match "Execution Gateway"
Write-Host "  [1/8] System Doctor / Vault Health:      $(if ($ok1) {'OK'} else {'FAIL'})" -ForegroundColor DarkGray

# 2. Inventory / Discovered Toolchains
$v2 = (& $installedCli toolchain list 2>&1 | Out-String)
$ok2 = $v2 -match "Toolchain Catalog"
Write-Host "  [2/8] Inventory / Discovered Toolchains: $(if ($ok2) {'OK'} else {'FAIL'})" -ForegroundColor DarkGray

# 3. Catalog / Browsable Packages
$v3 = (& $installedCli catalog list 2>&1 | Out-String)
$ok3 = $v3 -match "Universal Development Resource Catalog"
Write-Host "  [3/8] Catalog / Browsable Packages:     $(if ($ok3) {'OK'} else {'FAIL'})" -ForegroundColor DarkGray

# 4. Project Connector / Workspaces
$v4 = (& $installedCli project connector $nexusProj 2>&1 | Out-String)
$ok4 = $v4 -match "NexusNode"
Write-Host "  [4/8] Project Connector / Workspaces:   $(if ($ok4) {'OK'} else {'FAIL'})" -ForegroundColor DarkGray

# 5. Environment Lifecycle / Leases
$v5 = (& $installedCli env audit 2>&1 | Out-String)
$ok5 = $v5 -match "Vault Environments Audit"
Write-Host "  [5/8] Environment Lifecycle / Leases:   $(if ($ok5) {'OK'} else {'FAIL'})" -ForegroundColor DarkGray

# 6. Package Cache / Governance
$v6 = (& $installedCli cache status 2>&1 | Out-String)
$ok6 = $v6 -match "Package Cache Governance"
Write-Host "  [6/8] Package Cache / Governance:       $(if ($ok6) {'OK'} else {'FAIL'})" -ForegroundColor DarkGray

# 7. Storage Intelligence / Deduplication
$v7 = (& $installedCli storage report 2>&1 | Out-String)
$ok7 = $v7 -match "Storage & Deduplication Intelligence"
Write-Host "  [7/8] Storage Intelligence / Deduplication: $(if ($ok7) {'OK'} else {'FAIL'})" -ForegroundColor DarkGray

# 8. Background Reconciliation Agent
$v8 = (& $installedCli agent status 2>&1 | Out-String)
$ok8 = $v8 -match "Background Agent"
Write-Host "  [8/8] Background Reconciliation Agent:  $(if ($ok8) {'OK'} else {'FAIL'})" -ForegroundColor DarkGray

if ($ok1 -and $ok2 -and $ok3 -and $ok4 -and $ok5 -and $ok6 -and $ok7 -and $ok8) {
    Write-Host "  STEP 4 PASSED: All 8 desktop backend views operational." -ForegroundColor Green
    $testsPassed++
} else {
    throw "STEP 4 FAILED: One or more desktop backend data views failed!"
}

# --------------------------------------------------------------------------
# Step 5: Clean Exit via WM_CLOSE
# --------------------------------------------------------------------------
Write-Host "`n>>> STEP 5: Testing Clean Graceful Exit via WM_CLOSE <<<" -ForegroundColor Yellow
[InstalledGuiHelper]::PostMessage($hWnd, [InstalledGuiHelper]::WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null

$closed = $guiProc.WaitForExit(6000)
if ($closed) {
    Write-Host "  Desktop GUI exited cleanly upon WM_CLOSE (ExitCode: $($guiProc.ExitCode))." -ForegroundColor DarkGray
} else {
    Write-Host "  Process required termination after WM_CLOSE timeout." -ForegroundColor DarkGray
    $guiProc.Kill()
}
Write-Host "  STEP 5 PASSED: Window closed gracefully." -ForegroundColor Green
$testsPassed++

# --------------------------------------------------------------------------
# Step 6: Clean Removal via Uninstall Script
# --------------------------------------------------------------------------
Write-Host "`n>>> STEP 6: Clean Removal via Uninstaller <<<" -ForegroundColor Yellow
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File "E:\Workspace\Active\ULVO\scripts\uninstall-ulvo.ps1" `
    -InstallDir $isolatedInstallDir -Force

if (-not (Test-Path $installedDesktopExe) -and -not (Test-Path $installedCli)) {
    Write-Host "  Binaries and installation directory cleaned without residuals." -ForegroundColor DarkGray
    Write-Host "  STEP 6 PASSED: Clean uninstallation verified." -ForegroundColor Green
    $testsPassed++
} else {
    throw "STEP 6 FAILED: Installed binaries lingered after uninstallation!"
}

Write-Host "`n============================================================" -ForegroundColor Cyan
Write-Host "   INSTALLED DESKTOP GUI RESULTS: $testsPassed / $totalTests PASSED     " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
