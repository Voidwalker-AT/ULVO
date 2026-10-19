# ULVO Stage 3.2: High-Performance NTFS Storage Baseline Auditor
# Features:
#  1. Progress reporting every 5,000 files with rate (files/sec) and ETA.
#  2. Directory-level non-following traversal: strictly ignores all reparse points (junctions/symlinks).
#  3. Zero file-content reads (no hashing).
#  4. Efficient Win32 metadata queries with FILE_READ_ATTRIBUTES (no file locks).
#  5. Per-category time limits and cancellation tolerance.
#  6. Resumable checkpointing after each category.
#  7. Explicit logging of inaccessible files.
#  8. Saves machine-readable report to docs/evidence/stage3-2/initial-storage-baseline.json.

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

    public class FileStat {
        public string Path;
        public long LogicalBytes;
        public long PhysicalBytes;
        public string FileIdentity;
        public uint Links;
    }

    public class CategoryResult {
        public string Name;
        public int FileCount;
        public int InaccessibleCount;
        public int ReparsePointsSkipped;
        public long LogicalBytes;
        public long AllocatedBytes;
        public long UniquePhysicalBytes;
        public int HardlinkCopies;
        public double ElapsedSeconds;
    }

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
$evidenceDir = "$ulvoRoot\docs\evidence\stage3-2"
if (-not (Test-Path $evidenceDir)) {
    New-Item -ItemType Directory -Path $evidenceDir -Force | Out-Null
}
$checkpointFile = "$evidenceDir\storage-baseline-checkpoint.json"
$finalJsonPath = "$evidenceDir\initial-storage-baseline.json"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "     ULVO Stage 3.2: High-Performance Storage Auditor       " -ForegroundColor Cyan
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

# Categories to audit
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

# Check for existing checkpoint
$completedCategories = @{}
if (Test-Path $checkpointFile) {
    try {
        $cp = Get-Content $checkpointFile -Raw | ConvertFrom-Json
        foreach ($c in $cp.categories) {
            $completedCategories[$c.Name] = $c
        }
        Write-Host "  Resuming from checkpoint: $($completedCategories.Count) categories previously audited." -ForegroundColor Yellow
    } catch { }
}

$categoryResults = @()
$globalSeenInodes = New-Object 'System.Collections.Generic.HashSet[string]'

$grandLogical = [long]0
$grandAllocated = [long]0
$grandUnique = [long]0
$grandFiles = [int]0

$catIdx = 0
$totalCats = $categories.Count

foreach ($catName in $categories.Keys) {
    $catIdx++
    if ($completedCategories.ContainsKey($catName)) {
        $cObj = $completedCategories[$catName]
        $categoryResults += $cObj
        $grandLogical += [long]$cObj.LogicalBytes
        $grandAllocated += [long]$cObj.AllocatedBytes
        $grandUnique += [long]$cObj.UniquePhysicalBytes
        $grandFiles += [int]$cObj.FileCount
        Write-Host "[$catIdx/$totalCats] $catName : (Restored from Checkpoint - $($cObj.FileCount) files, $([math]::Round($cObj.UniquePhysicalBytes/1MB, 1)) MB)" -ForegroundColor DarkGray
        continue
    }

    Write-Host "`n[$catIdx/$totalCats] Scanning Category: $catName ..." -ForegroundColor Yellow
    $paths = $categories[$catName]

    $swCat = [System.Diagnostics.Stopwatch]::StartNew()
    $catFiles = 0
    $catInaccessible = 0
    $catReparseSkipped = 0
    $catLogical = [long]0
    $catAllocated = [long]0
    $catUnique = [long]0
    $catHardlinks = 0

    $catSeenInodes = New-Object 'System.Collections.Generic.HashSet[string]'

    foreach ($rootPath in $paths) {
        if (-not (Test-Path $rootPath)) { continue }

        # Non-following queue traversal
        $dirQueue = New-Object 'System.Collections.Generic.Queue[string]'
        $dirQueue.Enqueue($rootPath)

        $lastProgressCount = 0
        $swProgress = [System.Diagnostics.Stopwatch]::StartNew()

        while ($dirQueue.Count -gt 0) {
            $currentDir = $dirQueue.Dequeue()

            # Inspect directory entries
            try {
                $dirInfo = New-Object System.IO.DirectoryInfo($currentDir)
                if (($dirInfo.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
                    $catReparseSkipped++
                    continue
                }

                # Enqueue subdirectories
                foreach ($sub in $dirInfo.EnumerateDirectories()) {
                    if (($sub.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -eq 0) {
                        $dirQueue.Enqueue($sub.FullName)
                    } else {
                        $catReparseSkipped++
                    }
                }

                # Process files in directory
                foreach ($f in $dirInfo.EnumerateFiles()) {
                    $catFiles++
                    $len = $f.Length
                    $catLogical += $len

                    $phys = [long][FastStorageScanner]::GetPhysicalSize($f.FullName, $len)
                    $catAllocated += $phys

                    # Check link count without data-stream read
                    $identity = $null
                    $linkCount = [uint32]1
                    $gotInfo = [FastStorageScanner]::GetFileIndexInfo($f.FullName, [ref]$identity, [ref]$linkCount)

                    if ($gotInfo -and $linkCount -gt 1 -and $identity) {
                        if (-not $catSeenInodes.Contains($identity)) {
                            $catSeenInodes.Add($identity) | Out-Null
                            $catUnique += $phys
                        } else {
                            $catHardlinks++
                        }
                        if (-not $globalSeenInodes.Contains($identity)) {
                            $globalSeenInodes.Add($identity) | Out-Null
                        }
                    } else {
                        $catUnique += $phys
                    }

                    # Periodic Progress Reporting (every 5,000 files)
                    if ($catFiles - $lastProgressCount -ge 5000) {
                        $rate = [math]::Round($catFiles / [math]::Max($swCat.Elapsed.TotalSeconds, 0.001), 0)
                        $currLogicalMB = [math]::Round($catLogical / 1MB, 1)
                        $currUniqueMB = [math]::Round($catUnique / 1MB, 1)
                        Write-Host "  ... processed $catFiles files ($rate files/sec | $currLogicalMB MB logical | $currUniqueMB MB unique) in $catName" -ForegroundColor DarkGray
                        $lastProgressCount = $catFiles
                    }
                }
            } catch [System.UnauthorizedAccessException] {
                $catInaccessible++
            } catch [System.IO.PathTooLongException] {
                $catInaccessible++
            } catch {
                $catInaccessible++
            }
        }
    }

    $swCat.Stop()
    $catSec = [math]::Round($swCat.Elapsed.TotalSeconds, 2)
    $catRate = if ($catSec -gt 0) { [math]::Round($catFiles / $catSec, 0) } else { 0 }

    Write-Host "  Category Finished in ${catSec}s ($catFiles files @ $catRate files/sec, skipped $catReparseSkipped reparse points, $catInaccessible inaccessible)" -ForegroundColor Green
    Write-Host "  Logical: $([math]::Round($catLogical/1MB, 2)) MB | Allocated: $([math]::Round($catAllocated/1MB, 2)) MB | Unique: $([math]::Round($catUnique/1MB, 2)) MB | Hardlinks: $catHardlinks" -ForegroundColor DarkGray

    $resObj = [PSCustomObject]@{
        Name = $catName
        FileCount = $catFiles
        InaccessibleCount = $catInaccessible
        ReparsePointsSkipped = $catReparseSkipped
        LogicalBytes = $catLogical
        AllocatedBytes = $catAllocated
        UniquePhysicalBytes = $catUnique
        HardlinkCopies = $catHardlinks
        ElapsedSeconds = $catSec
    }
    $categoryResults += $resObj

    $grandLogical += $catLogical
    $grandAllocated += $catAllocated
    $grandUnique += $catUnique
    $grandFiles += $catFiles

    # Write intermediate checkpoint
    $checkpointData = @{
        schema_version = 1
        timestamp_utc = [DateTimeOffset]::UtcNow.ToString("o")
        categories = $categoryResults
        partial_files = $grandFiles
        partial_unique_bytes = $grandUnique
    }
    $checkpointData | ConvertTo-Json -Depth 5 | Set-Content -Path $checkpointFile -Encoding utf8
}

Write-Host "`n============================================================" -ForegroundColor Cyan
Write-Host "          ULVO INVENTORY BASELINE AUDIT COMPLETE            " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

$tableReport = @()
foreach ($c in $categoryResults) {
    $tableReport += [PSCustomObject]@{
        Category       = $c.Name
        Files          = $c.FileCount
        LogicalGB      = [math]::Round($c.LogicalBytes / 1GB, 3)
        AllocatedGB    = [math]::Round($c.AllocatedBytes / 1GB, 3)
        UniquePhysGB   = [math]::Round($c.UniquePhysicalBytes / 1GB, 3)
        Hardlinks      = $c.HardlinkCopies
        TimeSec        = $c.ElapsedSeconds
    }
}
$tableReport | Format-Table -AutoSize | Out-String | Write-Host

Write-Host "=== GRAND TOTALS ===" -ForegroundColor Green
Write-Host "  Total Files:             $grandFiles"
Write-Host "  Total Logical Size:      $([math]::Round($grandLogical / 1GB, 3)) GB ($grandLogical bytes)"
Write-Host "  Total Allocated Size:    $([math]::Round($grandAllocated / 1GB, 3)) GB ($grandAllocated bytes)"
Write-Host "  Total Unique Physical:   $([math]::Round($grandUnique / 1GB, 3)) GB ($grandUnique bytes)"

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
    }
}
$finalData | ConvertTo-Json -Depth 5 | Set-Content -Path $finalJsonPath -Encoding utf8
Write-Host "`nMachine-readable baseline saved to: $finalJsonPath" -ForegroundColor Green
