# ULVO Stage 3.2: Package Store NTFS Hardlink Deduplicator
# Safely eliminates physical duplicate files between vault\packages\python\shared and vault\packages\python\uv-cache\archive-v0
# Both paths remain fully valid and intact, sharing the same physical NTFS clusters.

$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.IO;
using System.Security.Cryptography;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

public class PackageDeduplicator {
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Auto)]
    public static extern bool CreateHardLink(string lpFileName, string lpExistingFileName, IntPtr lpSecurityAttributes);

    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Auto)]
    public static extern bool MoveFileEx(string lpExistingFileName, string lpNewFileName, uint dwFlags);

    public const uint MOVEFILE_REPLACE_EXISTING = 0x00000001;
    public const uint MOVEFILE_WRITE_THROUGH = 0x00000008;

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

    private const uint FILE_READ_ATTRIBUTES = 0x0080;
    private const uint FILE_SHARE_READ = 0x00000001;
    private const uint FILE_SHARE_WRITE = 0x00000002;
    private const uint FILE_SHARE_DELETE = 0x00000004;
    private const uint OPEN_EXISTING = 3;
    private const uint FILE_FLAG_BACKUP_SEMANTICS = 0x02000000;

    public static bool GetFileIdentity(string path, out string id, out uint linkCount) {
        id = null;
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
                        ulong fileIdx = ((ulong)info.FileIndexHigh << 32) | (ulong)info.FileIndexLow;
                        id = info.VolumeSerialNumber.ToString("X") + ":" + fileIdx.ToString("X");
                        return true;
                    }
                }
            }
        } catch { }
        return false;
    }

    public static bool ComputeFastSha256(string path, out string hashHex) {
        hashHex = null;
        try {
            using (FileStream fs = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite)) {
                using (SHA256 sha = SHA256.Create()) {
                    byte[] hash = sha.ComputeHash(fs);
                    hashHex = BitConverter.ToString(hash).Replace("-", "");
                    return true;
                }
            }
        } catch {
            return false;
        }
    }

    public static bool Deduplicate(string canonicalSource, string targetToReplace) {
        string id1, id2;
        uint links1, links2;
        if (!GetFileIdentity(canonicalSource, out id1, out links1)) return false;
        if (!GetFileIdentity(targetToReplace, out id2, out links2)) return false;

        // Already same physical inode
        if (id1 == id2) return true;

        string tmpLink = targetToReplace + ".ulvo_hl_tmp";
        if (File.Exists(tmpLink)) {
            try { File.Delete(tmpLink); } catch { }
        }

        if (!CreateHardLink(tmpLink, canonicalSource, IntPtr.Zero)) {
            return false;
        }

        if (!MoveFileEx(tmpLink, targetToReplace, MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)) {
            try { File.Delete(tmpLink); } catch { }
            return false;
        }

        return true;
    }
}
"@

$vaultRoot = "E:\Workspace\Active\ULVO\vault"
$sharedRoot = "$vaultRoot\packages\python\shared"
$uvCacheRoot = "$vaultRoot\packages\python\uv-cache\archive-v0"
$evidenceDir = "E:\Workspace\Active\ULVO\docs\evidence\stage3-2"
if (-not (Test-Path $evidenceDir)) { New-Item -ItemType Directory -Path $evidenceDir -Force | Out-Null }
$reportFile = "$evidenceDir\package-dedup-report.json"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "     ULVO Stage 3.2: Package Store Deduplication            " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

$freeBeforeE = (Get-PSDrive E).Free
Write-Host "Initial Drive E: Free Space: $([math]::Round($freeBeforeE/1GB, 3)) GB ($freeBeforeE bytes)" -ForegroundColor Yellow

$sharedDistInfos = Get-ChildItem -Path $sharedRoot -Filter "*.dist-info" -Directory
Write-Host "Found $($sharedDistInfos.Count) dist-info packages in shared store.`n"

$totalEvaluated = 0
$totalDeduped = 0
$totalAlreadyLinked = 0
$totalBytesReclaimed = [long]0
$packageDetails = @()

$swTotal = [System.Diagnostics.Stopwatch]::StartNew()

foreach ($sdi in $sharedDistInfos) {
    $pkgName = $sdi.Name -replace "\.dist-info$", ""
    # Locate archive in uv cache
    $matchArchive = Get-ChildItem -Path $uvCacheRoot -Directory | Where-Object { Test-Path "$($_.FullName)\$($sdi.Name)" } | Select-Object -First 1
    if (-not $matchArchive) {
        continue
    }

    $topLevelFile = "$($sdi.FullName)\top_level.txt"
    $pkgDirs = @()
    if (Test-Path $topLevelFile) {
        $pkgDirs = Get-Content $topLevelFile | Where-Object { $_ -and (Test-Path "$sharedRoot\$_") }
    }

    $filesToCheck = @(Get-ChildItem -Path $sdi.FullName -Recurse -File)
    foreach ($pd in $pkgDirs) {
        $filesToCheck += Get-ChildItem -Path "$sharedRoot\$pd" -Recurse -File -ErrorAction SilentlyContinue
    }

    $pkgDeduped = 0
    $pkgAlready = 0
    $pkgBytes = [long]0
    $pkgSw = [System.Diagnostics.Stopwatch]::StartNew()

    foreach ($sf in $filesToCheck) {
        $totalEvaluated++
        $rel = $sf.FullName.Substring($sharedRoot.Length + 1)
        $uvFile = "$($matchArchive.FullName)\$rel"
        if (-not (Test-Path $uvFile)) { continue }

        $uf = Get-Item $uvFile
        if ($uf.Length -ne $sf.Length) { continue }

        $id1 = ""; $l1 = 0
        $id2 = ""; $l2 = 0
        if (-not [PackageDeduplicator]::GetFileIdentity($uvFile, [ref]$id1, [ref]$l1)) { continue }
        if (-not [PackageDeduplicator]::GetFileIdentity($sf.FullName, [ref]$id2, [ref]$l2)) { continue }

        if ($id1 -eq $id2) {
            $totalAlreadyLinked++
            $pkgAlready++
            continue
        }

        # Verify hash match before mutating
        $h1 = ""; $h2 = ""
        if (-not [PackageDeduplicator]::ComputeFastSha256($uvFile, [ref]$h1)) { continue }
        if (-not [PackageDeduplicator]::ComputeFastSha256($sf.FullName, [ref]$h2)) { continue }

        if ($h1 -ne $h2) { continue }

        # Deduplicate
        if ([PackageDeduplicator]::Deduplicate($uvFile, $sf.FullName)) {
            $pkgDeduped++
            $pkgBytes += $sf.Length
            $totalDeduped++
            $totalBytesReclaimed += $sf.Length
        }
    }
    $pkgSw.Stop()

    $mb = [math]::Round($pkgBytes / 1MB, 2)
    Write-Host "  [DEDUP] $pkgName : Deduped $pkgDeduped files ($mb MB) | Already Linked: $pkgAlready in $($pkgSw.Elapsed.TotalSeconds.ToString('F2'))s" -ForegroundColor Green

    $packageDetails += [PSCustomObject]@{
        Package = $pkgName
        FilesDeduped = $pkgDeduped
        AlreadyLinked = $pkgAlready
        BytesReclaimed = $pkgBytes
        MBReclaimed = $mb
        ElapsedSeconds = [math]::Round($pkgSw.Elapsed.TotalSeconds, 2)
    }
}

$swTotal.Stop()
$freeAfterE = (Get-PSDrive E).Free
$deltaE = $freeAfterE - $freeBeforeE

Write-Host "`n============================================================" -ForegroundColor Cyan
Write-Host "     PACKAGE DEDUPLICATION COMPLETE                         " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "  Total Files Evaluated:    $totalEvaluated"
Write-Host "  Total Files Deduplicated: $totalDeduped"
Write-Host "  Already Hardlinked:       $totalAlreadyLinked"
Write-Host "  Calculated Bytes Freed:   $totalBytesReclaimed ($([math]::Round($totalBytesReclaimed/1MB, 2)) MB, $([math]::Round($totalBytesReclaimed/1GB, 3)) GB)"
Write-Host "  Drive E: Free Space Now:  $([math]::Round($freeAfterE/1GB, 3)) GB ($freeAfterE bytes)"
Write-Host "  Drive E: Delta Observed:  $([math]::Round($deltaE/1MB, 2)) MB ($deltaE bytes)"
Write-Host "  Total Elapsed Time:       $([math]::Round($swTotal.Elapsed.TotalSeconds, 2)) seconds"

$finalReport = @{
    schema_version = 1
    timestamp_utc = [DateTimeOffset]::UtcNow.ToString("o")
    total_evaluated = $totalEvaluated
    total_deduplicated_files = $totalDeduped
    already_hardlinked = $totalAlreadyLinked
    calculated_bytes_freed = $totalBytesReclaimed
    drive_e_before_free_bytes = $freeBeforeE
    drive_e_after_free_bytes = $freeAfterE
    drive_e_delta_bytes = $deltaE
    package_details = $packageDetails
}
$finalReport | ConvertTo-Json -Depth 5 | Set-Content -Path $reportFile -Encoding utf8
Write-Host "`nMachine-readable report saved to: $reportFile" -ForegroundColor Green
