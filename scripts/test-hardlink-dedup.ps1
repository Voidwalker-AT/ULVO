Add-Type -TypeDefinition @"
using System;
using System.IO;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

public class HardlinkDedup {
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

    public static bool DeduplicateFile(string sourceTarget, string destToReplace) {
        string id1, id2;
        uint links1, links2;
        if (!GetFileIdentity(sourceTarget, out id1, out links1)) return false;
        if (!GetFileIdentity(destToReplace, out id2, out links2)) return false;

        // Already same physical file
        if (id1 == id2) return true;

        string tmpLink = destToReplace + ".ulvo_hl_tmp";
        if (File.Exists(tmpLink)) {
            try { File.Delete(tmpLink); } catch { }
        }

        if (!CreateHardLink(tmpLink, sourceTarget, IntPtr.Zero)) {
            return false;
        }

        if (!MoveFileEx(tmpLink, destToReplace, MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)) {
            try { File.Delete(tmpLink); } catch { }
            return false;
        }

        return true;
    }
}
"@

$scratch = "E:\Workspace\Active\ULVO\vault\scratch"
$src = "$scratch\dedup_src.bin"
$dst = "$scratch\dedup_dst.bin"
Set-Content -Path $src -Value "Deduplication Test Content 1234567890"
Set-Content -Path $dst -Value "Deduplication Test Content 1234567890"

$id1 = ""; $links1 = 0
$id2 = ""; $links2 = 0
[HardlinkDedup]::GetFileIdentity($src, [ref]$id1, [ref]$links1) | Out-Null
[HardlinkDedup]::GetFileIdentity($dst, [ref]$id2, [ref]$links2) | Out-Null
Write-Host "Before Dedup: Src ID: $id1 (Links: $links1) | Dst ID: $id2 (Links: $links2)"

$ok = [HardlinkDedup]::DeduplicateFile($src, $dst)
Write-Host "Dedup call result: $ok"

$id1After = ""; $links1After = 0
$id2After = ""; $links2After = 0
[HardlinkDedup]::GetFileIdentity($src, [ref]$id1After, [ref]$links1After) | Out-Null
[HardlinkDedup]::GetFileIdentity($dst, [ref]$id2After, [ref]$links2After) | Out-Null
Write-Host "After Dedup:  Src ID: $id1After (Links: $links1After) | Dst ID: $id2After (Links: $links2After)"

if ($id1After -eq $id2After -and $links1After -eq 2) {
    Write-Host "DEDUPLICATION TEST PASSED: Files successfully converted to hardlinks sharing the same physical inode!" -ForegroundColor Green
} else {
    Write-Host "DEDUPLICATION TEST FAILED" -ForegroundColor Red
}

Remove-Item -Path $src, $dst -Force -ErrorAction SilentlyContinue
