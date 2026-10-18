pub mod vault_rust;
pub use vault_rust::*;
pub mod vault_python;
pub use vault_python::*;
pub mod fs_identity;
pub use fs_identity::*;
pub mod vault_uv;
pub use vault_uv::*;

use std::{io::Read, path::{Path, PathBuf}, process::{Command, Stdio},
    sync::mpsc, thread, time::{Duration, Instant}};
use ulvo_core::Backend;

pub const TOOLS: &[(&str, &[&str], &str)] = &[
    ("python", &["--version"], "interpreter"),
    ("python3", &["--version"], "interpreter"),
    ("node", &["--version"], "interpreter"),
    ("java", &["-version"], "runtime"), ("javac", &["-version"], "compiler"),
    ("go", &["version"], "compiler"), ("ruby", &["--version"], "interpreter"),
    ("gcc", &["--version"], "compiler"), ("g++", &["--version"], "compiler"),
    ("rustc", &["--version"], "compiler"), ("cargo", &["--version"], "build_tool"),
    ("dotnet", &["--version"], "sdk"), ("cmake", &["--version"], "build_tool"),
    ("ninja", &["--version"], "build_tool"),
    ("clangd", &["--version"], "language_server"),
    ("nvcc", &["--version"], "compiler"),
];

pub fn classify_path(path: &str, host: Backend) -> Backend {
    let p = path.replace('\\', "/").to_lowercase();
    if ["/ucrt64/", "/mingw64/", "/mingw32/", "/clang64/", "/clangarm64/", "/msys64/"].iter().any(|s| p.contains(s)) {
        Backend::Msys2
    } else if p.starts_with("/mnt/") && p.ends_with(".exe") || p.as_bytes().get(1) == Some(&b':') {
        Backend::Windows
    } else { host }
}

pub fn host_backend() -> Backend {
    if cfg!(windows) { Backend::Windows } else { Backend::Wsl }
}

pub fn bounded_read(path: &Path) -> Result<String, String> {
    reject_links(path)?;
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut b = Vec::new();
    f.by_ref().take(1_048_577).read_to_end(&mut b).map_err(|e| e.to_string())?;
    if b.len() > 1_048_576 { return Err("input exceeds 1 MiB".into()); }
    String::from_utf8(b).map_err(|e| e.to_string())
}

pub fn reject_links(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        if let Ok(meta) = std::fs::symlink_metadata(&current) {
            if meta.file_type().is_symlink() { return Err("symlink traversal rejected".into()); }
            #[cfg(windows)] {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 { return Err("reparse traversal rejected".into()); }
            }
        }
    }
    Ok(())
}

fn capture<R: Read + Send + 'static>(mut pipe: R) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut kept = Vec::new(); let mut b = [0; 4096];
        while let Ok(n) = pipe.read(&mut b) {
            if n == 0 { break; }
            let count = n.min(65536usize.saturating_sub(kept.len()));
            kept.extend_from_slice(&b[..count]);
        }
        let _ = tx.send(kept);
    });
    rx
}

pub fn probe(path: &Path, tool: &str, backend: &Backend) -> Result<String, String> {
    let spec = TOOLS.iter().find(|s| s.0 == tool).ok_or("tool not allowlisted")?;
    if backend != &host_backend() { return Err("foreign backend execution unavailable".into()); }
    let canonical = path.canonicalize().map_err(|e| e.to_string())?;
    let text = canonical.to_string_lossy().replace('\\', "/").to_lowercase();
    // Only fixed installation roots may be probed; PATH does not confer trust.
    let trusted = if cfg!(windows) {
        text.starts_with("c:/program files/") || text.starts_with("c:/windows/")
    } else {
        text.starts_with("/usr/bin/") || text.starts_with("/usr/lib/") ||
        std::env::var("HOME").ok().is_some_and(|h| text.starts_with(&format!("{h}/.rustup/toolchains/")))
    };
    if !trusted { return Err("outside approved probe roots; present only".into()); }
    if tool == "dotnet" { return Err("dotnet probe disabled: first-run global-state risk".into()); }
    let stem = canonical.file_stem().unwrap_or_default().to_string_lossy();
    let name_matches = stem == tool || (tool.starts_with("python") && stem.starts_with("python"));
    if !name_matches { return Err("canonical executable name does not match tool".into()); }
    binary_architecture(&canonical)?;
    run_bounded(&canonical, spec.1, Duration::from_secs(3))
}

pub fn binary_architecture(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = [0u8; 64];
    file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    if bytes[..4] == [0x7f, b'E', b'L', b'F'] {
        let machine = if bytes[5] == 1 { u16::from_le_bytes([bytes[18], bytes[19]]) }
            else { u16::from_be_bytes([bytes[18], bytes[19]]) };
        return Ok(match machine { 62 => "x86_64", 3 => "x86", 183 => "aarch64", _ => "unknown_elf" }.into());
    }
    if &bytes[..2] == b"MZ" {
        use std::io::{Seek, SeekFrom};
        let offset = u32::from_le_bytes(bytes[60..64].try_into().unwrap()) as u64;
        if offset > 1_048_576 { return Err("PE header offset exceeds bound".into()); }
        file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let mut pe = [0; 6]; file.read_exact(&mut pe).map_err(|e| e.to_string())?;
        if &pe[..4] != b"PE\0\0" { return Err("invalid PE signature".into()); }
        return Ok(match u16::from_le_bytes([pe[4], pe[5]]) {
            0x8664 => "x86_64", 0x14c => "x86", 0xaa64 => "aarch64", _ => "unknown_pe" }.into());
    }
    Err("not a recognized native binary; scripts never probed".into())
}

pub fn run_bounded_with_env(
    path: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
    timeout: Duration,
) -> Result<String, String> {
    let mut cmd = Command::new(path);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let out = capture(child.stdout.take().unwrap());
    let err = capture(child.stderr.take().unwrap());
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(20)),
            _ => { let _ = child.kill(); let _ = child.wait(); return Err("probe timeout or wait failure".into()); }
        }
    };
    if !status.success() { return Err(format!("probe exit {status}")); }
    let stdout = out.recv_timeout(Duration::from_millis(200)).map_err(|_| "stdout capture timeout")?;
    let stderr = err.recv_timeout(Duration::from_millis(200)).map_err(|_| "stderr capture timeout")?;
    Ok(format!("{}\n{}", String::from_utf8_lossy(&stdout), String::from_utf8_lossy(&stderr)))
}

pub fn run_bounded(path: &Path, args: &[&str], timeout: Duration) -> Result<String, String> {
    run_bounded_with_env(path, args, &[], timeout)
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn paths() {
        assert_eq!(classify_path("C:\\Python312\\python.exe", Backend::Wsl), Backend::Windows);
        assert_eq!(classify_path("/mnt/c/msys64/ucrt64/bin/gcc.exe", Backend::Wsl), Backend::Msys2);
        assert_eq!(classify_path("/usr/bin/python3", Backend::Wsl), Backend::Wsl);
        assert_eq!(classify_path("/mnt/c/Windows/python.exe", Backend::Wsl), Backend::Windows);
    }
    #[test] fn missing_and_untrusted() {
        assert!(probe(Path::new("/missing/ulvo"), "python", &host_backend()).is_err());
        assert!(probe(Path::new("/usr/bin/sh"), "sh", &host_backend()).is_err());
        assert!(probe(Path::new("C:/python.exe"), "python", &Backend::Windows).is_err());
    }
    #[test] fn clangd_not_compiler() {
        assert_eq!(TOOLS.iter().find(|s| s.0 == "clangd").unwrap().2, "language_server");
    }
    #[test] fn capture_limit() {
        let rx = capture(std::io::Cursor::new(vec![b'x'; 100_000]));
        assert_eq!(rx.recv_timeout(Duration::from_secs(1)).unwrap().len(), 65536);
    }
    #[cfg(unix)] #[test] fn timeout() {
        let start = Instant::now();
        assert!(run_bounded(Path::new("/usr/bin/sleep"), &["1"], Duration::from_millis(50)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[cfg(unix)] #[test] fn native_probe_and_header() {
        assert!(probe(Path::new("/usr/bin/python3"), "python3", &Backend::Wsl).unwrap().contains("Python"));
        assert_eq!(binary_architecture(Path::new("/usr/bin/python3")).unwrap(), "x86_64");
        assert!(probe(Path::new("/usr/bin/sh"), "python3", &Backend::Wsl).is_err());
    }
}