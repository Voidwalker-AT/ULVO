use std::path::{Component, Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedPaths {
    pub config_root: PathBuf,
    pub vault_root: PathBuf,
    pub source: String,
    pub workspace_roots: Vec<PathBuf>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BootstrapPreferences {
    #[serde(default)]
    pub vault_path: Option<PathBuf>,
    #[serde(default)]
    pub workspace_roots: Vec<PathBuf>,
    #[serde(default)]
    pub ecosystems: Vec<String>,
    #[serde(default)]
    pub enable_background_agent: bool,
}

pub fn validate_absolute_directory(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() || path.parent().is_none()
        || path.components().any(|c| matches!(c, Component::ParentDir))
        || path.as_os_str().to_string_lossy().contains('\0') {
        return Err("configuration paths must be absolute, normalized non-root directories".into());
    }
    crate::reject_links(path)?;
    if path.exists() && !path.is_dir() { return Err("configuration path is not a directory".into()); }
    Ok(path.to_path_buf())
}

#[cfg(windows)]
pub fn windows_root() -> Result<PathBuf, String> {
    use std::os::windows::ffi::OsStringExt;
    extern "system" { fn GetWindowsDirectoryW(buffer: *mut u16, size: u32) -> u32; }
    let mut buffer = vec![0u16; 32768];
    let n = unsafe { GetWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if n == 0 || n >= buffer.len() { return Err("Windows directory query failed".into()); }
    Ok(PathBuf::from(std::ffi::OsString::from_wide(&buffer[..n])))
}
#[cfg(not(windows))]
pub fn windows_root() -> Result<PathBuf, String> { Err("Windows platform required".into()) }

pub fn trusted_nvidia_smi() -> Option<PathBuf> {
    let path = windows_root().ok()?.join("System32/nvidia-smi.exe");
    if path.is_file() && crate::reject_links(&path).is_ok() { Some(path) } else { None }
}

#[cfg(windows)]
pub fn user_data_root() -> Result<PathBuf, String> {
    use std::os::windows::ffi::OsStringExt;
    #[repr(C)]
    struct Guid { a: u32, b: u16, c: u16, d: [u8; 8] }
    #[link(name = "shell32")]
    extern "system" { fn SHGetKnownFolderPath(id: *const Guid, flags: u32, token: *mut std::ffi::c_void, path: *mut *mut u16) -> i32; }
    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(reserved: *mut std::ffi::c_void, mode: u32) -> i32;
        fn CoUninitialize();
        fn CoTaskMemFree(memory: *mut std::ffi::c_void);
    }
    let local = Guid { a: 0xf1b32785, b: 0x6fba, c: 0x4fcf, d: [0x9d,0x55,0x7b,0x8e,0x7f,0x15,0x70,0x91] };
    let initialized = unsafe { CoInitializeEx(std::ptr::null_mut(), 2) };
    let mut ptr = std::ptr::null_mut();
    let result = unsafe { SHGetKnownFolderPath(&local, 0x4000, std::ptr::null_mut(), &mut ptr) };
    let path = if result >= 0 && !ptr.is_null() {
        let mut length = 0;
        while length < 32768 && unsafe { *ptr.add(length) } != 0 { length += 1; }
        if length == 32768 { Err("known-folder path exceeds Windows path limit".into()) }
        else { Ok(PathBuf::from(std::ffi::OsString::from_wide(unsafe { std::slice::from_raw_parts(ptr, length) })).join("ULVO")) }
    } else { Err(format!("cannot resolve Windows LocalAppData known folder: HRESULT {result:#x}")) };
    if !ptr.is_null() { unsafe { CoTaskMemFree(ptr as *mut _) }; }
    if initialized >= 0 { unsafe { CoUninitialize() }; }
    path
}

#[cfg(not(windows))]
pub fn user_data_root() -> Result<PathBuf, String> {
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
        .map(|p| p.join("ulvo")).ok_or("user data directory unavailable".into())
}

pub fn resolve_paths(explicit_vault: Option<&Path>, explicit_config: Option<&Path>) -> Result<ResolvedPaths, String> {
    let config = explicit_config.map(Path::to_path_buf)
        .or_else(|| std::env::var_os("ULVO_CONFIG_ROOT").map(PathBuf::from))
        .map(Ok).unwrap_or_else(user_data_root)?;
    let process_vault = std::env::var_os("ULVO_VAULT_ROOT").map(PathBuf::from);
    resolve_paths_in(explicit_vault, process_vault.as_deref(), &config, std::env::current_exe().ok().as_deref())
}

/// Explicit inputs make precedence testable without changing process environment.
pub fn resolve_paths_in(explicit: Option<&Path>, environment: Option<&Path>, config: &Path, exe: Option<&Path>) -> Result<ResolvedPaths, String> {
    let config = validate_absolute_directory(config)?;
    let settings = config.join("preferences.json");
    let mut prefs: BootstrapPreferences = if settings.exists() {
        serde_json::from_str(&crate::bounded_read(&settings)?).map_err(|e| format!("invalid user preferences: {e}"))?
    } else { BootstrapPreferences::default() };
    let mut source = "user_default";
    // Per-install preferences and a proven existing adjacent vault preserve
    // older installations. Never inspect arbitrary CWD ancestors.
    if !settings.exists() {
        if let Some(install) = exe.and_then(Path::parent).and_then(Path::parent) {
            let installed = install.join("config/preferences.json");
            let legacy = install.join("vault");
            if installed.is_file() {
                prefs = serde_json::from_str(&crate::bounded_read(&installed)?).map_err(|e| format!("invalid installation preferences: {e}"))?;
                source = "installation_preferences";
            } else if legacy.join("registry/vault_registry.json").is_file() {
                prefs.vault_path = Some(legacy);
                source = "existing_installation_vault";
            }
        }
    } else { source = "user_preferences"; }
    let root = if let Some(path) = explicit { source = "command_line"; path.to_path_buf() }
        else if let Some(path) = environment { source = "process_environment"; path.to_path_buf() }
        else { prefs.vault_path.unwrap_or_else(|| config.join("vault")) };
    let root = validate_absolute_directory(&root)?;
    for workspace in &prefs.workspace_roots { validate_absolute_directory(workspace)?; }
    Ok(ResolvedPaths { config_root: config, vault_root: root, source: source.into(), workspace_roots: prefs.workspace_roots })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root(tag: &str) -> PathBuf { std::env::temp_dir().join(format!("ulvo_paths_{tag}_{}", std::process::id())) }
    #[test]
    fn precedence_and_unicode_without_creating_vault() {
        let base = root("precedence");
        std::fs::create_dir_all(&base).unwrap();
        let persisted = base.join("persisted vault Δ");
        std::fs::write(base.join("preferences.json"), serde_json::to_vec(&BootstrapPreferences { vault_path: Some(persisted.clone()), ..Default::default() }).unwrap()).unwrap();
        let environment = base.join("environment");
        let explicit = base.join("explicit");
        assert_eq!(resolve_paths_in(Some(&explicit), Some(&environment), &base, None).unwrap().vault_root, explicit);
        assert_eq!(resolve_paths_in(None, Some(&environment), &base, None).unwrap().vault_root, environment);
        assert_eq!(resolve_paths_in(None, None, &base, None).unwrap().vault_root, persisted);
        assert!(!persisted.exists());
        assert!(resolve_paths_in(None, Some(Path::new("relative")), &base, None).is_err());
    }
    #[test]
    fn safe_default_and_corrupt_preferences_fail_closed() {
        let base = root("default");
        assert_eq!(resolve_paths_in(None, None, &base, None).unwrap().vault_root, base.join("vault"));
        assert!(!base.exists());
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("preferences.json"), "{bad").unwrap();
        assert!(resolve_paths_in(None, None, &base, None).is_err());
    }
    #[test]
    fn legacy_installation_is_preserved_without_moving_data() {
        let base = root("legacy");
        let install = base.join("app");
        let vault = install.join("vault");
        std::fs::create_dir_all(vault.join("registry")).unwrap();
        std::fs::write(vault.join("registry/vault_registry.json"), "{}").unwrap();
        let result = resolve_paths_in(None, None, &base.join("user"), Some(&install.join("bin/ulvo.exe"))).unwrap();
        assert_eq!(result.vault_root, vault);
        assert_eq!(result.source, "existing_installation_vault");
    }
}
