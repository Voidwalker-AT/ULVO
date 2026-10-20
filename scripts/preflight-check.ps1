$ErrorActionPreference = "Stop"

Write-Host "=== PREFLIGHT INVENTORY ===" -ForegroundColor Cyan
$ulvoCli = "E:\Workspace\Active\ULVO\bin\ulvo.exe"
Write-Host "CLI Path: $ulvoCli"
if (Test-Path $ulvoCli) {
    & $ulvoCli --version
} else {
    Write-Host "CLI not found in bin!" -ForegroundColor Red
}

$desktopExe = "E:\Workspace\Active\ULVO\target\release\ulvo-desktop.exe"
$desktopTauriExe = "E:\Workspace\Active\ULVO\apps\ulvo-desktop\src-tauri\target\release\ulvo-desktop.exe"
Write-Host "Desktop Exe (workspace release): $(Test-Path $desktopExe) ($desktopExe)"
Write-Host "Desktop Exe (tauri release): $(Test-Path $desktopTauriExe) ($desktopTauriExe)"

$agentExe = "E:\Workspace\Active\ULVO\target\release\ulvo-agent.exe"
Write-Host "Agent Exe: $(Test-Path $agentExe) ($agentExe)"

$vaultPath = "E:\Workspace\Active\ULVO\vault"
Write-Host "Vault Path: $vaultPath (Exists: $(Test-Path $vaultPath))"

# Check broken transactions
$brokenTxns = Get-ChildItem -Path "E:\Workspace\Active\ULVO" -Filter ".ulvo_txn_*.journal.json" -Recurse -ErrorAction SilentlyContinue
Write-Host "Active Broken Install Transactions: $(@($brokenTxns).Count)"

# Volume free space
$driveC = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='C:'"
$driveE = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='E:'"
Write-Host ("Drive C Free: {0:N0} bytes ({1:N2} GB, {2:N2} GiB)" -f $driveC.FreeSpace, ($driveC.FreeSpace / 1e9), ($driveC.FreeSpace / 1073741824))
Write-Host ("Drive E Free: {0:N0} bytes ({1:N2} GB, {2:N2} GiB)" -f $driveE.FreeSpace, ($driveE.FreeSpace / 1e9), ($driveE.FreeSpace / 1073741824))

# Doctor check
Write-Host "`n--- ULVO DOCTOR ---" -ForegroundColor Cyan
& $ulvoCli doctor

Write-Host "`n--- MANAGED & EXTERNAL RUNTIMES (INVENTORY) ---" -ForegroundColor Cyan
& $ulvoCli inventory
