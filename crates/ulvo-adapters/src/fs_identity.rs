use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileSystemIdentity {
    pub volume_serial_number: u64,
    pub file_index: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileSizeInfo {
    pub logical_bytes: u64,
    pub allocated_bytes: Option<u64>,
    pub link_count: u32,
    pub is_sparse: bool,
    pub is_compressed: bool,
    pub is_reparse: bool,
}

#[cfg(windows)]
pub fn query_file_identity_and_size(path: &Path) -> Result<(FileSystemIdentity, FileSizeInfo), String> {
    use std::os::windows::ffi::OsStrExt;

    type HANDLE = *mut std::ffi::c_void;
    type DWORD = u32;
    type BOOL = i32;
    type LPCWSTR = *const u16;
    type LPVOID = *mut std::ffi::c_void;

    const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
    const FILE_READ_ATTRIBUTES: DWORD = 0x0080;
    const FILE_SHARE_READ: DWORD = 1;
    const FILE_SHARE_WRITE: DWORD = 2;
    const FILE_SHARE_DELETE: DWORD = 4;
    const OPEN_EXISTING: DWORD = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: DWORD = 0x02000000;
    const FILE_FLAG_OPEN_REPARSE_POINT: DWORD = 0x00200000;
    const FILE_ATTRIBUTE_SPARSE_FILE: DWORD = 0x00000200;
    const FILE_ATTRIBUTE_REPARSE_POINT: DWORD = 0x00000400;
    const FILE_ATTRIBUTE_COMPRESSED: DWORD = 0x00000800;
    const INVALID_FILE_SIZE: DWORD = 0xFFFFFFFF;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct FILETIME {
        dwLowDateTime: DWORD,
        dwHighDateTime: DWORD,
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    struct BY_HANDLE_FILE_INFORMATION {
        dwFileAttributes: DWORD,
        ftCreationTime: FILETIME,
        ftLastAccessTime: FILETIME,
        ftLastWriteTime: FILETIME,
        dwVolumeSerialNumber: DWORD,
        nFileSizeHigh: DWORD,
        nFileSizeLow: DWORD,
        nNumberOfLinks: DWORD,
        nFileIndexHigh: DWORD,
        nFileIndexLow: DWORD,
    }

    extern "system" {
        fn CreateFileW(
            lpFileName: LPCWSTR,
            dwDesiredAccess: DWORD,
            dwShareMode: DWORD,
            lpSecurityAttributes: LPVOID,
            dwCreationDisposition: DWORD,
            dwFlagsAndAttributes: DWORD,
            hTemplateFile: HANDLE,
        ) -> HANDLE;

        fn GetFileInformationByHandle(
            hFile: HANDLE,
            lpFileInformation: *mut BY_HANDLE_FILE_INFORMATION,
        ) -> BOOL;

        fn GetCompressedFileSizeW(
            lpFileName: LPCWSTR,
            lpFileSizeHigh: *mut DWORD,
        ) -> DWORD;

        fn CloseHandle(hObject: HANDLE) -> BOOL;
        fn GetLastError() -> DWORD;
    }

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();

    // Open handle with FILE_READ_ATTRIBUTES without locking the file for other processes
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        let err = unsafe { GetLastError() };
        return Err(format!("CreateFileW failed for '{}' with error code {}", path.display(), err));
    }

    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(handle, &mut info) };
    unsafe { CloseHandle(handle); }

    if ok == 0 {
        let err = unsafe { GetLastError() };
        return Err(format!("GetFileInformationByHandle failed for '{}' with error code {}", path.display(), err));
    }

    let is_sparse = (info.dwFileAttributes & FILE_ATTRIBUTE_SPARSE_FILE) != 0;
    let is_compressed = (info.dwFileAttributes & FILE_ATTRIBUTE_COMPRESSED) != 0;
    let is_reparse = (info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT) != 0;

    let logical_bytes = ((info.nFileSizeHigh as u64) << 32) | (info.nFileSizeLow as u64);
    let link_count = info.nNumberOfLinks;

    let identity = FileSystemIdentity {
        volume_serial_number: info.dwVolumeSerialNumber as u64,
        file_index: ((info.nFileIndexHigh as u64) << 32) | (info.nFileIndexLow as u64),
    };

    // Query physical allocation on disk (accounts for cluster rounding, compression, and sparse holes)
    let mut high: DWORD = 0;
    let low = unsafe { GetCompressedFileSizeW(wide.as_ptr(), &mut high) };
    let allocated_bytes = if low == INVALID_FILE_SIZE && unsafe { GetLastError() } != 0 {
        None
    } else {
        Some(((high as u64) << 32) | (low as u64))
    };

    Ok((identity, FileSizeInfo {
        logical_bytes,
        allocated_bytes,
        link_count,
        is_sparse,
        is_compressed,
        is_reparse,
    }))
}

#[cfg(not(windows))]
pub fn query_file_identity_and_size(path: &Path) -> Result<(FileSystemIdentity, FileSizeInfo), String> {
    use std::os::unix::fs::MetadataExt;

    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    let identity = FileSystemIdentity {
        volume_serial_number: meta.dev(),
        file_index: meta.ino(),
    };
    let logical_bytes = meta.len();
    let allocated_bytes = Some(meta.blocks() * 512);
    let link_count = meta.nlink() as u32;
    let is_reparse = meta.file_type().is_symlink();

    Ok((identity, FileSizeInfo {
        logical_bytes,
        allocated_bytes,
        link_count,
        is_sparse: false,
        is_compressed: false,
        is_reparse,
    }))
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, part) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes(part.try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    format!(
        "{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}",
        h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct SafeTestFixture {
        path: PathBuf,
    }

    impl SafeTestFixture {
        fn new(name: &str) -> Self {
            let pid = std::process::id();
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("ulvo_test_fs_{name}_{pid}_{ts}"));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join(".ulvo_test_sentinel"), format!("pid={pid}\nts={ts}\n")).unwrap();
            Self { path }
        }
    }

    impl Drop for SafeTestFixture {
        fn drop(&mut self) {
            let sentinel = self.path.join(".ulvo_test_sentinel");
            if sentinel.is_file() {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }

    #[test]
    fn test_sha256_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_hardlinks_share_filesystem_identity() {
        let fixture = SafeTestFixture::new("hardlinks");
        let file1 = fixture.path.join("original.txt");
        let file2 = fixture.path.join("hardlinked.txt");

        std::fs::write(&file1, b"Hello hardlink identity testing").unwrap();
        // Create hardlink
        if std::fs::hard_link(&file1, &file2).is_ok() {
            let (id1, info1) = query_file_identity_and_size(&file1).unwrap();
            let (id2, info2) = query_file_identity_and_size(&file2).unwrap();

            assert_eq!(id1, id2, "Hardlinked files must share the exact same volume and file_index");
            assert_eq!(info1.link_count, 2);
            assert_eq!(info2.link_count, 2);
            assert_eq!(info1.logical_bytes, info2.logical_bytes);
            assert!(info1.allocated_bytes.is_some());
        }
    }

    #[test]
    fn test_independent_files_with_identical_content_have_different_identities() {
        let fixture = SafeTestFixture::new("independent");
        let file_a = fixture.path.join("file_a.txt");
        let file_b = fixture.path.join("file_b.txt");

        let content = b"Identical file content in two different files";
        std::fs::write(&file_a, content).unwrap();
        std::fs::write(&file_b, content).unwrap();

        let (id_a, info_a) = query_file_identity_and_size(&file_a).unwrap();
        let (id_b, info_b) = query_file_identity_and_size(&file_b).unwrap();

        assert_ne!(id_a, id_b, "Independent files must have distinct filesystem identities even with identical content");
        assert_eq!(info_a.link_count, 1);
        assert_eq!(info_b.link_count, 1);
        assert_eq!(info_a.logical_bytes, info_b.logical_bytes);
    }

    #[test]
    fn test_different_files_with_identical_names_and_sizes() {
        let fixture = SafeTestFixture::new("same_name");
        let dir1 = fixture.path.join("dir1");
        let dir2 = fixture.path.join("dir2");
        std::fs::create_dir_all(&dir1).unwrap();
        std::fs::create_dir_all(&dir2).unwrap();

        let file1 = dir1.join("package.json");
        let file2 = dir2.join("package.json");

        // Same size (10 bytes) but different content
        std::fs::write(&file1, b"1234567890").unwrap();
        std::fs::write(&file2, b"0987654321").unwrap();

        let (id1, info1) = query_file_identity_and_size(&file1).unwrap();
        let (id2, info2) = query_file_identity_and_size(&file2).unwrap();

        assert_ne!(id1, id2);
        assert_eq!(info1.logical_bytes, 10);
        assert_eq!(info2.logical_bytes, 10);
    }

    #[test]
    fn test_query_missing_file_fails() {
        let fixture = SafeTestFixture::new("missing");
        let missing = fixture.path.join("does_not_exist.bin");
        assert!(query_file_identity_and_size(&missing).is_err());
    }

    #[test]
    fn test_ntfs_identity_stability() {
        let fixture = SafeTestFixture::new("stability");
        let file = fixture.path.join("stable.txt");
        std::fs::write(&file, b"NTFS Identity Stability Verification").unwrap();

        let (id1, info1) = query_file_identity_and_size(&file).unwrap();
        let (id2, info2) = query_file_identity_and_size(&file).unwrap();

        assert_eq!(id1, id2, "Querying the same file must yield identical filesystem identity");
        assert_eq!(info1, info2, "Querying the same file must yield identical FileSizeInfo");
    }

    #[test]
    fn test_fixture_cleanup_ownership_safety() {
        let path = {
            let fixture = SafeTestFixture::new("ownership_guard");
            let p = fixture.path.clone();
            assert!(p.join(".ulvo_test_sentinel").is_file());
            // Intentionally remove sentinel to simulate an unowned or external directory
            std::fs::remove_file(p.join(".ulvo_test_sentinel")).unwrap();
            p
            // fixture dropped here
        };

        // Because sentinel was removed, SafeTestFixture::drop must refuse to delete the directory
        assert!(path.is_dir(), "Unowned directory without sentinel must NOT be deleted by fixture cleanup");
        let _ = std::fs::remove_dir_all(&path);
    }
}

