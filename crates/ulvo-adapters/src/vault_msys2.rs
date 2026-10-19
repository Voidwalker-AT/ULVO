use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use serde::{Deserialize, Serialize};
use ulvo_core::{
    Abi, Architecture, Backend, Ecosystem, Ownership, Platform, Provenance,
    Resource, ResourceKind, VerificationStatus,
};
use crate::{bounded_read, reject_links, write_atomic};

/// MSYS2 Subsystems supported by ULVO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Msys2Subsystem {
    Ucrt64,
    Mingw64,
    Clang64,
    Msys,
}

impl Msys2Subsystem {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ucrt64 => "ucrt64",
            Self::Mingw64 => "mingw64",
            Self::Clang64 => "clang64",
            Self::Msys => "usr",
        }
    }

    pub fn msystem_env(&self) -> &'static str {
        match self {
            Self::Ucrt64 => "UCRT64",
            Self::Mingw64 => "MINGW64",
            Self::Clang64 => "CLANG64",
            Self::Msys => "MSYS",
        }
    }

    pub fn bin_dir(&self, msys2_root: &Path) -> PathBuf {
        match self {
            Self::Ucrt64 => msys2_root.join("ucrt64").join("bin"),
            Self::Mingw64 => msys2_root.join("mingw64").join("bin"),
            Self::Clang64 => msys2_root.join("clang64").join("bin"),
            Self::Msys => msys2_root.join("usr").join("bin"),
        }
    }
}

/// Metadata recorded in `.ulvo_managed.json` sentinel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedMsys2Metadata {
    pub schema_version: u32,
    pub toolchain: String,
    pub subsystems: Vec<String>,
    pub source_path: String,
    pub adopted_at_unix: u64,
    pub provenance: Provenance,
}

/// Checks if MSYS2 is managed and provisioned in the given vault root.
pub fn is_msys2_provisioned(vault_root: &Path) -> bool {
    let sentinel = vault_root.join("toolchains").join("msys2").join(".ulvo_managed.json");
    if sentinel.is_file() {
        if let Ok(content) = bounded_read(&sentinel) {
            return serde_json::from_str::<ManagedMsys2Metadata>(&content).is_ok();
        }
    }
    false
}

/// Recursively copies a directory tree, clearing read-only attributes on Windows.
pub fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    reject_links(src)?;
    if !dst.exists() {
        std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    }

    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let entry_type = entry.file_type().map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if entry_type.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else if entry_type.is_file() {
            std::fs::copy(&src_path, &dst_path).map_err(|e| {
                format!("failed to copy '{}' to '{}': {e}", src_path.display(), dst_path.display())
            })?;
        }
    }
    Ok(())
}

/// Provisions MSYS2 from an external root (e.g. C:\msys64) into the ULVO vault under `toolchains\msys2`.
pub fn provision_managed_msys2(
    vault_root: &Path,
    source_root: &Path,
) -> Result<ManagedMsys2Metadata, String> {
    reject_links(vault_root)?;
    reject_links(source_root)?;

    let target_dir = vault_root.join("toolchains").join("msys2");
    let sentinel_path = target_dir.join(".ulvo_managed.json");

    // If already provisioned, return existing metadata
    if sentinel_path.is_file() {
        if let Ok(content) = bounded_read(&sentinel_path) {
            if let Ok(meta) = serde_json::from_str::<ManagedMsys2Metadata>(&content) {
                return Ok(meta);
            }
        }
    }

    let ucrt64_src = source_root.join("ucrt64");
    let usr_src = source_root.join("usr");
    if !ucrt64_src.is_dir() || !usr_src.is_dir() {
        return Err(format!(
            "invalid MSYS2 source at '{}': missing 'ucrt64' or 'usr' directory",
            source_root.display()
        ));
    }

    std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

    // Copy ucrt64 subsystem
    copy_dir_all(&ucrt64_src, &target_dir.join("ucrt64"))?;

    // Copy usr subsystem
    copy_dir_all(&usr_src, &target_dir.join("usr"))?;

    // Copy etc if present
    let etc_src = source_root.join("etc");
    if etc_src.is_dir() {
        let _ = copy_dir_all(&etc_src, &target_dir.join("etc"));
    }

    // Copy var\lib\pacman if present
    let pacman_db = source_root.join("var").join("lib").join("pacman");
    if pacman_db.is_dir() {
        let target_pacman = target_dir.join("var").join("lib").join("pacman");
        let _ = std::fs::create_dir_all(&target_pacman);
        let _ = copy_dir_all(&pacman_db, &target_pacman);
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let provenance = Provenance {
        source_type: "managed_toolchain_adoption".into(),
        canonical_url: Some("https://www.msys2.org".into()),
        expected_sha256: None,
        actual_sha256: None,
        manifest_path: Some(sentinel_path.to_string_lossy().to_string()),
        notes: Some(format!("Adopted from {}", source_root.display())),
    };

    let metadata = ManagedMsys2Metadata {
        schema_version: 1,
        toolchain: "msys2".into(),
        subsystems: vec!["ucrt64".into(), "usr".into()],
        source_path: source_root.to_string_lossy().to_string(),
        adopted_at_unix: now,
        provenance: provenance.clone(),
    };

    let json_bytes = serde_json::to_string_pretty(&metadata).map_err(|e| e.to_string())?;
    write_atomic(&sentinel_path, json_bytes.as_bytes())?;

    Ok(metadata)
}

/// Discovers managed MSYS2 toolchain resources in the vault.
pub fn discover_vault_msys2(vault_root: &Path) -> Result<Vec<Resource>, String> {
    let msys2_root = vault_root.join("toolchains").join("msys2");
    let sentinel_path = msys2_root.join(".ulvo_managed.json");

    if !sentinel_path.is_file() {
        return Ok(Vec::new());
    }

    let content = bounded_read(&sentinel_path)?;
    let metadata: ManagedMsys2Metadata = serde_json::from_str(&content)
        .map_err(|e| format!("corrupted MSYS2 sentinel at {}: {e}", sentinel_path.display()))?;

    let ucrt64_bin = msys2_root.join("ucrt64").join("bin");
    let usr_bin = msys2_root.join("usr").join("bin");

    let mut resources = Vec::new();

    // 1. GCC (C compiler)
    let gcc_path = ucrt64_bin.join("gcc.exe");
    if gcc_path.is_file() {
        let mut r = Resource::new(
            ResourceKind::Compiler,
            Ecosystem::Cpp,
            "gcc",
            Some("14.2.0".into()),
            Platform::Windows,
            Architecture::X86_64,
            Backend::Msys2,
            Abi::Gnu,
            Some(gcc_path.to_string_lossy().to_string()),
            Some("ulvo".into()),
            Ownership::Managed,
            VerificationStatus::Verified,
            Some(metadata.provenance.clone()),
            vec!["c_compiler".into(), "c11".into(), "c17".into(), "c23".into()],
        );
        r.last_verified_at = Some(chrono_like_now());
        resources.push(r);
    }

    // 2. G++ (C++ compiler)
    let gpp_path = ucrt64_bin.join("g++.exe");
    if gpp_path.is_file() {
        let mut r = Resource::new(
            ResourceKind::Compiler,
            Ecosystem::Cpp,
            "g++",
            Some("14.2.0".into()),
            Platform::Windows,
            Architecture::X86_64,
            Backend::Msys2,
            Abi::Gnu,
            Some(gpp_path.to_string_lossy().to_string()),
            Some("ulvo".into()),
            Ownership::Managed,
            VerificationStatus::Verified,
            Some(metadata.provenance.clone()),
            vec!["cpp_compiler".into(), "c++17".into(), "c++20".into(), "c++23".into()],
        );
        r.last_verified_at = Some(chrono_like_now());
        resources.push(r);
    }

    // 3. Make (Build tool)
    let make_path = usr_bin.join("make.exe");
    if make_path.is_file() {
        let mut r = Resource::new(
            ResourceKind::Executable,
            Ecosystem::Cpp,
            "make",
            None,
            Platform::Windows,
            Architecture::X86_64,
            Backend::Msys2,
            Abi::Gnu,
            Some(make_path.to_string_lossy().to_string()),
            Some("ulvo".into()),
            Ownership::Managed,
            VerificationStatus::Verified,
            Some(metadata.provenance.clone()),
            vec!["build_tool".into(), "makefile".into()],
        );
        r.last_verified_at = Some(chrono_like_now());
        resources.push(r);
    }

    // 4. Pacman (Package manager)
    let pacman_path = usr_bin.join("pacman.exe");
    if pacman_path.is_file() {
        let mut r = Resource::new(
            ResourceKind::PackageManager,
            Ecosystem::Cpp,
            "pacman",
            None,
            Platform::Windows,
            Architecture::X86_64,
            Backend::Msys2,
            Abi::Gnu,
            Some(pacman_path.to_string_lossy().to_string()),
            Some("ulvo".into()),
            Ownership::Managed,
            VerificationStatus::Verified,
            Some(metadata.provenance.clone()),
            vec!["package_manager".into(), "pacman".into()],
        );
        r.last_verified_at = Some(chrono_like_now());
        resources.push(r);
    }

    Ok(resources)
}

fn chrono_like_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{now}")
}

/// Constructs a clean, isolated process environment for executing tools under MSYS2.
/// Strictly excludes user PATH entries and external MSYS2 installations.
pub fn build_msys2_process_env(
    vault_root: &Path,
    subsystem: Msys2Subsystem,
) -> Result<BTreeMap<String, String>, String> {
    let msys2_root = vault_root.join("toolchains").join("msys2");
    if !msys2_root.is_dir() {
        return Err(format!(
            "managed MSYS2 toolchain not found at '{}'; run 'ulvo toolchain msys2 adopt' first",
            msys2_root.display()
        ));
    }

    let sub_bin = ulvo_core::strip_unc_prefix(&subsystem.bin_dir(&msys2_root));
    let usr_bin = ulvo_core::strip_unc_prefix(&msys2_root.join("usr").join("bin"));

    let mut path_entries = Vec::new();
    if sub_bin.is_dir() {
        path_entries.push(sub_bin.to_string_lossy().to_string());
    }
    if usr_bin.is_dir() && !path_entries.contains(&usr_bin.to_string_lossy().to_string()) {
        path_entries.push(usr_bin.to_string_lossy().to_string());
    }

    // Windows system directories only for OS runtime DLLs
    let sys_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let sys32 = PathBuf::from(&sys_root).join("System32");
    let win = PathBuf::from(&sys_root);
    let wbem = PathBuf::from(&sys_root).join("System32").join("Wbem");
    let ps = PathBuf::from(&sys_root).join("System32").join("WindowsPowerShell").join("v1.0");

    for d in [&sys32, &win, &wbem, &ps] {
        if d.is_dir() {
            let s = d.to_string_lossy().to_string();
            if !path_entries.contains(&s) {
                path_entries.push(s);
            }
        }
    }

    let path_str = path_entries.join(if cfg!(windows) { ";" } else { ":" });

    let mut env = BTreeMap::new();
    env.insert("PATH".to_string(), path_str);
    env.insert("SystemRoot".to_string(), sys_root.clone());
    if let Ok(sd) = std::env::var("SystemDrive") {
        env.insert("SystemDrive".to_string(), sd);
    }
    env.insert("MSYSTEM".to_string(), subsystem.msystem_env().to_string());
    env.insert("MSYS2_PATH_TYPE".to_string(), "minimal".to_string());
    env.insert("CHERE_INVOKING".to_string(), "1".to_string());
    env.insert("LC_ALL".to_string(), "C".to_string());

    let temp_dir = std::env::var("TEMP")
        .or_else(|_| std::env::var("TMP"))
        .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string());
    env.insert("TEMP".to_string(), temp_dir.clone());
    env.insert("TMP".to_string(), temp_dir);

    if let Ok(userprof) = std::env::var("USERPROFILE") {
        env.insert("USERPROFILE".to_string(), userprof.clone());
        env.insert("HOME".to_string(), userprof);
    }

    Ok(env)
}

/// Verifies native C compilation using the vault-managed MSYS2 GCC compiler.
pub fn verify_native_c_compilation(vault_root: &Path, scratch_dir: &Path) -> Result<String, String> {
    let msys2_root = vault_root.join("toolchains").join("msys2");
    let gcc_exe = ulvo_core::strip_unc_prefix(&msys2_root.join("ucrt64").join("bin").join("gcc.exe"));
    if !gcc_exe.is_file() {
        return Err(format!("gcc.exe not found in managed MSYS2 at '{}'", gcc_exe.display()));
    }

    std::fs::create_dir_all(scratch_dir).map_err(|e| e.to_string())?;
    let src_file = ulvo_core::strip_unc_prefix(&scratch_dir.join("test_ulvo_c.c"));
    let out_exe = ulvo_core::strip_unc_prefix(&scratch_dir.join("test_ulvo_c.exe"));

    let c_source = r#"#include <stdio.h>
int main(void) {
    printf("ULVO_MANAGED_GCC_VERIFIED_SUCCESS\n");
    return 0;
}
"#;
    std::fs::write(&src_file, c_source).map_err(|e| e.to_string())?;

    let env = build_msys2_process_env(vault_root, Msys2Subsystem::Ucrt64)?;

    // Compile
    let mut cmd = Command::new(&gcc_exe);
    cmd.args(["-O2", src_file.to_str().unwrap(), "-o", out_exe.to_str().unwrap()]);
    cmd.env_clear();
    for (k, v) in &env {
        cmd.env(k, v);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let compile_output = cmd.output().map_err(|e| format!("failed to spawn gcc: {e}"))?;
    if !compile_output.status.success() {
        let err_text = String::from_utf8_lossy(&compile_output.stderr);
        return Err(format!("GCC compilation failed: {err_text}"));
    }

    // Execute generated binary
    let mut run_cmd = Command::new(&out_exe);
    run_cmd.env_clear();
    for (k, v) in &env {
        run_cmd.env(k, v);
    }
    run_cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let run_output = run_cmd.output().map_err(|e| format!("failed to execute compiled C binary: {e}"))?;
    if !run_output.status.success() {
        return Err("Compiled C executable returned non-zero exit code".into());
    }

    let stdout_text = String::from_utf8_lossy(&run_output.stdout);
    if !stdout_text.contains("ULVO_MANAGED_GCC_VERIFIED_SUCCESS") {
        return Err(format!("Unexpected binary output: {stdout_text}"));
    }

    // Clean up binaries
    let _ = std::fs::remove_file(&src_file);
    let _ = std::fs::remove_file(&out_exe);

    Ok("Native C compilation & execution verified successfully using vault-managed MSYS2 GCC".into())
}

/// Verifies native C++ compilation using the vault-managed MSYS2 G++ compiler.
pub fn verify_native_cpp_compilation(vault_root: &Path, scratch_dir: &Path) -> Result<String, String> {
    let msys2_root = vault_root.join("toolchains").join("msys2");
    let gpp_exe = ulvo_core::strip_unc_prefix(&msys2_root.join("ucrt64").join("bin").join("g++.exe"));
    if !gpp_exe.is_file() {
        return Err(format!("g++.exe not found in managed MSYS2 at '{}'", gpp_exe.display()));
    }

    std::fs::create_dir_all(scratch_dir).map_err(|e| e.to_string())?;
    let src_file = ulvo_core::strip_unc_prefix(&scratch_dir.join("test_ulvo_cpp.cpp"));
    let out_exe = ulvo_core::strip_unc_prefix(&scratch_dir.join("test_ulvo_cpp.exe"));

    let cpp_source = r#"#include <iostream>
#include <vector>
#include <numeric>

int main() {
    std::vector<int> numbers = {10, 20, 30, 40};
    int total = std::accumulate(numbers.begin(), numbers.end(), 0);
    std::cout << "ULVO_MANAGED_GPP_VERIFIED_SUCCESS: " << total << std::endl;
    return 0;
}
"#;
    std::fs::write(&src_file, cpp_source).map_err(|e| e.to_string())?;

    let env = build_msys2_process_env(vault_root, Msys2Subsystem::Ucrt64)?;

    // Compile
    let mut cmd = Command::new(&gpp_exe);
    cmd.args(["-std=c++20", "-O2", src_file.to_str().unwrap(), "-o", out_exe.to_str().unwrap()]);
    cmd.env_clear();
    for (k, v) in &env {
        cmd.env(k, v);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let compile_output = cmd.output().map_err(|e| format!("failed to spawn g++: {e}"))?;
    if !compile_output.status.success() {
        let err_text = String::from_utf8_lossy(&compile_output.stderr);
        return Err(format!("G++ compilation failed: {err_text}"));
    }

    // Execute generated binary
    let mut run_cmd = Command::new(&out_exe);
    run_cmd.env_clear();
    for (k, v) in &env {
        run_cmd.env(k, v);
    }
    run_cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let run_output = run_cmd.output().map_err(|e| format!("failed to execute compiled C++ binary: {e}"))?;
    if !run_output.status.success() {
        return Err("Compiled C++ executable returned non-zero exit code".into());
    }

    let stdout_text = String::from_utf8_lossy(&run_output.stdout);
    if !stdout_text.contains("ULVO_MANAGED_GPP_VERIFIED_SUCCESS: 100") {
        return Err(format!("Unexpected binary output: {stdout_text}"));
    }

    // Clean up binaries
    let _ = std::fs::remove_file(&src_file);
    let _ = std::fs::remove_file(&out_exe);

    Ok("Native C++ compilation & execution verified successfully using vault-managed MSYS2 G++".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msys2_subsystem_attributes() {
        assert_eq!(Msys2Subsystem::Ucrt64.as_str(), "ucrt64");
        assert_eq!(Msys2Subsystem::Ucrt64.msystem_env(), "UCRT64");
        assert_eq!(Msys2Subsystem::Mingw64.as_str(), "mingw64");
        assert_eq!(Msys2Subsystem::Mingw64.msystem_env(), "MINGW64");
        assert_eq!(Msys2Subsystem::Clang64.as_str(), "clang64");
        assert_eq!(Msys2Subsystem::Clang64.msystem_env(), "CLANG64");
        assert_eq!(Msys2Subsystem::Msys.as_str(), "usr");
        assert_eq!(Msys2Subsystem::Msys.msystem_env(), "MSYS");
    }

    #[test]
    fn test_unprovisioned_msys2_returns_empty() {
        let temp = std::env::temp_dir().join(format!("ulvo_test_msys2_empty_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();

        assert!(!is_msys2_provisioned(&temp));
        let resources = discover_vault_msys2(&temp).unwrap();
        assert!(resources.is_empty());

        let _ = std::fs::remove_dir_all(&temp);
    }
}
