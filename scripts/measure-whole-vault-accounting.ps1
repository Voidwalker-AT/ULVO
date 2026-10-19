# ULVO Stage 3.3: Whole-Vault Physical Accounting Auditor
# Global Inode Deduplication across all 15 categories simultaneously

$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.IO;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

public class FastStorageScanner {
    [StructLayout(LayoutKind.Sequential)]
    public struct BY_HANDLE_FILE_INFORMATION {
        public uint FileAttributes;
        public System.Runtime.InteropServices.ComTypes.FILETIME CreationTime;
        public System.Runtime.InteropServices.ComTypes.FILETIME LastAccessTime;
        public System.Runtime.InteropServices.ComTypes.FILETIME LastWriteTime;
        public uint VolumeSerialNumber;
        public uint FileSizeHigh;
        public uint FileSizeLow;
        public uint NumberOfLinks;
        public uint FileIndexHigh;
        public uint FileIndexLow;
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool GetFileInformationByHandle(
        SafeFileHandle hFile,
        out BY_HANDLE_FILE_INFORMATION lpFileInformation
    );

    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Auto)]
    public static extern SafeFileHandle CreateFile(
        string lpFileName,
        uint dwDesiredAccess,
        uint dwShareMode,
        IntPtr lpSecurityAttributes,
        uint dwCreationDisposition,
        uint dwFlagsAndAttributes,
        IntPtr hTemplateFile
    );

    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Auto)]
    public static extern uint GetCompressedFileSize(string lpFileName, out uint lpFileSizeHigh);

    private const uint FILE_READ_ATTRIBUTES = 0x0080;
    private const uint FILE_SHARE_READ = 0x00000001;
    private const uint FILE_SHARE_WRITE = 0x00000002;
    private const uint FILE_SHARE_DELETE = 0x00000004;
    private const uint OPEN_EXISTING = 3;
    private const uint FILE_FLAG_BACKUP_SEMANTICS = 0x02000000;

    public static ulong GetPhysicalSize(string path, long fallbackLogical) {
        uint high = 0;
        uint low = GetCompressedFileSize(path, out high);
        if (low == 0xFFFFFFFF && Marshal.GetLastWin32Error() != 0) {
            return (ulong)fallbackLogical;
        }
        return ((ulong)high << 32) | (ulong)low;
    }

    public static bool GetFileIndexInfo(string path, out string identity, out uint linkCount) {
        identity = null;
        linkCount = 1;
        try {
            using (SafeFileHandle handle = CreateFile(
                path,
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                IntPtr.Zero,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                IntPtr.Zero))
            {
                if (handle != null && !handle.IsInvalid) {
                    BY_HANDLE_FILE_INFORMATION info;
                    if (GetFileInformationByHandle(handle, out info)) {
                        linkCount = info.NumberOfLinks;
                        if (linkCount > 1) {
                            ulong id = ((ulong)info.FileIndexHigh << 32) | (ulong)info.FileIndexLow;
                            identity = info.VolumeSerialNumber.ToString("X") + ":" + id.ToString("X");
                        }
                        return true;
                    }
                }
            }
        } catch { }
        return false;
    }
}
"@

$ulvoRoot = "E:\Workspace\Active\ULVO"
$vaultRoot = "E:\Workspace\Active\ULVO\vault"
$evidenceDir = "$ulvoRoot\docs\evidence\stage3-3"
if (-not (Test-Path $evidenceDir)) {
    New-Item -ItemType Directory -Path $evidenceDir -Force | Out-Null
}
$finalJsonPath = "$evidenceDir\whole-vault-physical-accounting.json"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host " ULVO Stage 3.3: Whole-Vault Global Accounting Auditor      " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host ""

# 1. Drive Baselines
$drives = Get-PSDrive C, E
$driveReport = @()
foreach ($d in $drives) {
    $driveReport += [PSCustomObject]@{
        DriveLetter = $d.Name
        FreeBytes   = $d.Free
        FreeGB      = [math]::Round($d.Free / 1GB, 3)
        UsedGB      = [math]::Round($d.Used / 1GB, 3)
        TotalGB     = [math]::Round(($d.Free + $d.Used) / 1GB, 3)
    }
}
Write-Host ">>> 1. INITIAL DRIVE CAPACITY & FREE SPACE <<<" -ForegroundColor Yellow
$driveReport | Format-Table -AutoSize | Out-String | Write-Host

# 15 Categories to audit in logical topological order (e.g. shared packages before environments)
$categories = [ordered]@{
    "ULVO Source Code" = @(
        "$ulvoRoot\crates",
        "$ulvoRoot\apps\ulvo-desktop\src",
        "$ulvoRoot\apps\ulvo-desktop\src-tauri\src",
        "$ulvoRoot\docs",
        "$ulvoRoot\scripts"
    )
    "Rust Cargo Target" = @("$ulvoRoot\target")
    "Tauri Desktop Build Target" = @("$ulvoRoot\apps\ulvo-desktop\src-tauri\target")
    "Frontend node_modules" = @("$ulvoRoot\apps\ulvo-desktop\node_modules")
    "Portable Release Artifacts" = @("$ulvoRoot\dist")
    "Vault Cargo Registry & Crates" = @("$vaultRoot\runtimes\rust\cargo")
    "Vault Rust Toolchain" = @(
        "$vaultRoot\runtimes\rust\rustup",
        "$vaultRoot\runtimes\rust\bin",
        "$vaultRoot\runtimes\rust\lib"
    )
    "Vault Python Runtimes" = @("$vaultRoot\runtimes\python")
    "Vault uv Cache" = @("$vaultRoot\packages\python\uv-cache")
    "Vault Shared Packages" = @("$vaultRoot\packages\python\shared")
    "Vault Content-Addressable Store (CAS)" = @("$vaultRoot\packages\cas")
    "Vault Managed MSYS2" = @("$vaultRoot\toolchains\msys2")
    "Vault Ephemeral Environments" = @("$vaultRoot\environments")
    "Vault Staging Transactions" = @("$vaultRoot\staging")
    "Vault Registry & Metadata" = @("$vaultRoot\registry")
}

$categoryResults = @()
$globalSeenInodes = New-Object 'System.Collections.Generic.HashSet[string]'

$grandLogical = [long]0
$grandAllocated = [long]0
$grandUnique = [long]0
$grandFiles = [int]0
$grandIntraHardlinks = [int]0
$grandCrossHardlinks = [int]0

$catIdx = 0
$totalCats = $categories.Count

foreach ($catName in $categories.Keys) {
    $catIdx++
    Write-Host "`n[$catIdx/$totalCats] Scanning Category: $catName ..." -ForegroundColor Yellow
    $paths = $categories[$catName]

    $swCat = [System.Diagnostics.Stopwatch]::StartNew()
    $catFiles = 0
    $catInaccessible = 0
    $catReparseSkipped = 0
    $catLogical = [long]0
    $catAllocated = [long]0
    $catUnique = [long]0
    $catIntraHardlinks = 0
    $catCrossHardlinks = 0

    $catSeenInodes = New-Object 'System.Collections.Generic.HashSet[string]'

    foreach ($rootPath in $paths) {
        if (-not (Test-Path $rootPath)) { continue }

        $dirQueue = New-Object 'System.Collections.Generic.Queue[string]'
        $dirQueue.Enqueue($rootPath)

        $lastProgressCount = 0

        while ($dirQueue.Count -gt 0) {
            $currentDir = $dirQueue.Dequeue()

            try {
                $dirInfo = New-Object System.IO.DirectoryInfo($currentDir)
                if (($dirInfo.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
                    $catReparseSkipped++
                    continue
                }

                foreach ($sub in $dirInfo.EnumerateDirectories()) {
                    if (($sub.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -eq 0) {
                        $dirQueue.Enqueue($sub.FullName)
                    } else {
                        $catReparseSkipped++
                    }
                }

                foreach ($f in $dirInfo.EnumerateFiles()) {
                    $catFiles++
                    $len = $f.Length
                    $catLogical += $len

                    $phys = [long][FastStorageScanner]::GetPhysicalSize($f.FullName, $len)
                    $catAllocated += $phys

                    $identity = $null
                    $linkCount = [uint32]1
                    $gotInfo = [FastStorageScanner]::GetFileIndexInfo($f.FullName, [ref]$identity, [ref]$linkCount)

                    if ($gotInfo -and $linkCount -gt 1 -and $identity) {
                        if ($globalSeenInodes.Contains($identity)) {
                            # Inode has already been charged as unique physical bytes
                            if ($catSeenInodes.Contains($identity)) {
                                $catIntraHardlinks++
                            } else {
                                $catCrossHardlinks++
                                $catSeenInodes.Add($identity) | Out-Null
                            }
                        } else {
                            # First time encountering this inode globally
                            $globalSeenInodes.Add($identity) | Out-Null
                            $catSeenInodes.Add($identity) | Out-Null
                            $catUnique += $phys
                        }
                    } else {
                        # Single-link file or no inode returned
                        $catUnique += $phys
                    }

                    if ($catFiles - $lastProgressCount -ge 5000) {
                        $rate = [math]::Round($catFiles / [math]::Max($swCat.Elapsed.TotalSeconds, 0.001), 0)
                        $currLogicalMB = [math]::Round($catLogical / 1MB, 1)
                        $currUniqueMB = [math]::Round($catUnique / 1MB, 1)
                        Write-Host "  ... processed $catFiles files ($rate files/sec | $currLogicalMB MB logical | $currUniqueMB MB unique) in $catName" -ForegroundColor DarkGray
                        $lastProgressCount = $catFiles
                    }
                }
            } catch {
                $catInaccessible++
            }
        }
    }

    $swCat.Stop()
    $catSec = [math]::Round($swCat.Elapsed.TotalSeconds, 2)
    $catRate = if ($catSec -gt 0) { [math]::Round($catFiles / $catSec, 0) } else { 0 }
    $catShared = $catAllocated - $catUnique

    Write-Host "  Category Finished in ${catSec}s ($catFiles files @ $catRate files/sec, skipped $catReparseSkipped reparse points, $catInaccessible inaccessible)" -ForegroundColor Green
    Write-Host "  Logical: $([math]::Round($catLogical/1MB, 2)) MB | Allocated: $([math]::Round($catAllocated/1MB, 2)) MB | Unique: $([math]::Round($catUnique/1MB, 2)) MB" -ForegroundColor DarkGray
    Write-Host "  Intra-category hardlinks: $catIntraHardlinks | Cross-category hardlinks: $catCrossHardlinks | Shared: $([math]::Round($catShared/1MB, 2)) MB" -ForegroundColor DarkGray

    $resObj = [PSCustomObject]@{
        Name = $catName
        FileCount = $catFiles
        InaccessibleCount = $catInaccessible
        ReparsePointsSkipped = $catReparseSkipped
        LogicalBytes = $catLogical
        AllocatedBytes = $catAllocated
        UniquePhysicalBytes = $catUnique
        SharedBytes = $catShared
        IntraCategoryHardlinks = $catIntraHardlinks
        CrossCategoryHardlinks = $catCrossHardlinks
        ElapsedSeconds = $catSec
    }
    $categoryResults += $resObj

    $grandLogical += $catLogical
    $grandAllocated += $catAllocated
    $grandUnique += $catUnique
    $grandFiles += $catFiles
    $grandIntraHardlinks += $catIntraHardlinks
    $grandCrossHardlinks += $catCrossHardlinks
}

Write-Host "`n============================================================" -ForegroundColor Cyan
Write-Host "   ULVO WHOLE-VAULT GLOBAL PHYSICAL ACCOUNTING COMPLETE     " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

$tableReport = @()
foreach ($c in $categoryResults) {
    $tableReport += [PSCustomObject]@{
        Category         = $c.Name
        Files            = $c.FileCount
        LogicalGB        = [math]::Round($c.LogicalBytes / 1GB, 3)
        AllocatedGB      = [math]::Round($c.AllocatedBytes / 1GB, 3)
        UniquePhysGB     = [math]::Round($c.UniquePhysicalBytes / 1GB, 3)
        SharedGB         = [math]::Round($c.SharedBytes / 1GB, 3)
        IntraLinks       = $c.IntraCategoryHardlinks
        CrossLinks       = $c.CrossCategoryHardlinks
        TimeSec          = $c.ElapsedSeconds
    }
}
$tableReport | Format-Table -AutoSize | Out-String | Write-Host

$grandShared = $grandAllocated - $grandUnique

Write-Host "=== WHOLE-VAULT GRAND TOTALS ===" -ForegroundColor Green
Write-Host "  Total Files Audited:             $grandFiles"
Write-Host "  Total Logical Size:              $([math]::Round($grandLogical / 1GB, 3)) GB ($grandLogical bytes)"
Write-Host "  Total Allocated Size (NTFS):     $([math]::Round($grandAllocated / 1GB, 3)) GB ($grandAllocated bytes)"
Write-Host "  Total Unique Physical Size:      $([math]::Round($grandUnique / 1GB, 3)) GB ($grandUnique bytes)"
Write-Host "  Total Shared / Deduplicated:     $([math]::Round($grandShared / 1GB, 3)) GB ($grandShared bytes)"
Write-Host "  Total Intra-Category Hardlinks:  $grandIntraHardlinks"
Write-Host "  Total Cross-Category Hardlinks:  $grandCrossHardlinks"
Write-Host "  Unique Inodes Discovered:        $($globalSeenInodes.Count)"

$finalData = @{
    schema_version = 1
    timestamp_utc = [DateTimeOffset]::UtcNow.ToString("o")
    volumes = $driveReport
    categories = $categoryResults
    summary = @{
        total_files = $grandFiles
        total_logical_bytes = $grandLogical
        total_allocated_bytes = $grandAllocated
        total_unique_physical_bytes = $grandUnique
        total_shared_bytes = $grandShared
        total_intra_category_hardlinks = $grandIntraHardlinks
        total_cross_category_hardlinks = $grandCrossHardlinks
        unique_inodes_tracked = $globalSeenInodes.Count
    }
}
$finalData | ConvertTo-Json -Depth 5 | Set-Content -Path $finalJsonPath -Encoding utf8
Write-Host "`nSaved machine-readable report to $finalJsonPath" -ForegroundColor Green
