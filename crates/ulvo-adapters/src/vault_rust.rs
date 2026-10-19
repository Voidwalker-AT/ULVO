use std::path::{Path, PathBuf};
use std::time::Duration;
use ulvo_core::{
    current_time_rfc3339, Abi, Architecture, Backend, Ecosystem, IssueSeverity,
    Ownership, Platform, Provenance, ReconciliationIssue, ReconciliationIssueType,
    Resource, ResourceKind, VerificationStatus,
};
use crate::{binary_architecture, bounded_read, reject_links, run_bounded_with_env};

pub fn parse_settings_toolchain(content: &str) -> Option<(String, String)> {
    let mut default_toolchain = None;
    let mut default_host_triple = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("default_toolchain") {
            let val = rest.trim_start_matches(|c: char| c == '=' || c.is_whitespace())
                .trim_matches('"')
                .trim_matches('\'');
            default_toolchain = Some(val.to_string());
        } else if let Some(rest) = trimmed.strip_prefix("default_host_triple") {
            let val = rest.trim_start_matches(|c: char| c == '=' || c.is_whitespace())
                .trim_matches('"')
                .trim_matches('\'');
            default_host_triple = Some(val.to_string());
        }
    }
    let tc = default_toolchain?;
    let triple = default_host_triple.unwrap_or_else(|| "x86_64-pc-windows-gnu".to_string());
    Some((tc, triple))
}

pub fn parse_version_from_toolchain_string(tc: &str) -> Option<String> {
    // e.g. "1.96.0-x86_64-pc-windows-gnu" -> "1.96.0"
    let ver_part = tc.split('-').next()?;
    if ver_part.chars().all(|c| c.is_ascii_digit() || c == '.') && ver_part.contains('.') {
        Some(ver_part.to_string())
    } else {
        None
    }
}

pub fn discover_vault_rust(
    rust_root: &Path,
    manifest_provenance: Option<&Provenance>,
) -> Result<Vec<Resource>, String> {
    reject_links(rust_root)?;
    let rustup_dir = rust_root.join("rustup");
    let cargo_dir = rust_root.join("cargo");

    if !rustup_dir.exists() {
        return Ok(Vec::new());
    }

    let settings_file = rustup_dir.join("settings.toml");
    if !settings_file.exists() {
        return Ok(Vec::new());
    }

    let settings_content = bounded_read(&settings_file)?;
    let (toolchain_name, host_triple) = parse_settings_toolchain(&settings_content)
        .ok_or_else(|| "Failed to parse default_toolchain from settings.toml".to_string())?;

    let version = parse_version_from_toolchain_string(&toolchain_name);
    let toolchain_dir = rustup_dir.join("toolchains").join(&toolchain_name);

    let mut resources = Vec::new();

    // Check components file
    let components_file = toolchain_dir.join("lib/rustlib/components");
    let mut component_names = Vec::new();
    if components_file.is_file() {
        if let Ok(comp_text) = bounded_read(&components_file) {
            for line in comp_text.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    component_names.push(trimmed.to_string());
                }
            }
        }
    }

    let (platform, arch, abi) = if host_triple.contains("windows") {
        let abi = if host_triple.contains("gnu") { Abi::Gnu } else { Abi::Msvc };
        (Platform::Windows, Architecture::X86_64, abi)
    } else {
        (Platform::Linux, Architecture::X86_64, Abi::Gnu)
    };

    let provenance = manifest_provenance.cloned().or_else(|| {
        Some(Provenance {
            source_type: "vault_rust_toolchain".into(),
            canonical_url: None,
            expected_sha256: None,
            actual_sha256: None,
            manifest_path: Some(settings_file.to_string_lossy().to_string()),
            notes: Some("Rustup managed toolchain settings discovered in vault".into()),
        })
    });

    // 1. Rustc Compiler
    let rustc_exe = toolchain_dir.join("bin").join("rustc.exe");
    let rustc_status = evaluate_file_status(&rustc_exe, "x86_64");
    resources.push(Resource::new(
        ResourceKind::Compiler,
        Ecosystem::Rust,
        "rustc",
        version.clone(),
        platform.clone(),
        arch.clone(),
        Backend::Windows,
        abi.clone(),
        Some(rustc_exe.to_string_lossy().to_string()),
        Some("rustup".into()),
        Ownership::Managed,
        rustc_status,
        provenance.clone(),
        vec!["compiler".into(), "codegen".into(), host_triple.clone()],
    ));

    // 2. Cargo Package Manager
    let cargo_exe = toolchain_dir.join("bin").join("cargo.exe");
    let cargo_status = evaluate_file_status(&cargo_exe, "x86_64");
    resources.push(Resource::new(
        ResourceKind::PackageManager,
        Ecosystem::Rust,
        "cargo",
        version.clone(),
        platform.clone(),
        arch.clone(),
        Backend::Windows,
        abi.clone(),
        Some(cargo_exe.to_string_lossy().to_string()),
        Some("rustup".into()),
        Ownership::Managed,
        cargo_status,
        provenance.clone(),
        vec!["package_manager".into(), "build_tool".into()],
    ));

    // 3. Rust-std Library
    let std_dir = toolchain_dir.join("lib/rustlib").join(&host_triple).join("lib");
    let std_status = if std_dir.is_dir() {
        VerificationStatus::Discovered
    } else {
        VerificationStatus::Missing
    };
    resources.push(Resource::new(
        ResourceKind::SharedLibrary,
        Ecosystem::Rust,
        "rust-std",
        version.clone(),
        platform.clone(),
        arch.clone(),
        Backend::Windows,
        abi.clone(),
        Some(std_dir.to_string_lossy().to_string()),
        Some("rustup".into()),
        Ownership::Managed,
        std_status,
        provenance.clone(),
        vec!["standard_library".into(), "sysroot".into()],
    ));

    // 4. Rust-mingw Linker Support (if GNU target)
    if abi == Abi::Gnu {
        let mingw_dir = toolchain_dir.join("lib/rustlib").join(&host_triple).join("bin");
        let mingw_status = if mingw_dir.is_dir() {
            VerificationStatus::Discovered
        } else {
            VerificationStatus::Missing
        };
        resources.push(Resource::new(
            ResourceKind::Sdk,
            Ecosystem::Rust,
            "rust-mingw",
            version.clone(),
            platform.clone(),
            arch.clone(),
            Backend::Windows,
            abi.clone(),
            Some(mingw_dir.to_string_lossy().to_string()),
            Some("rustup".into()),
            Ownership::Managed,
            mingw_status,
            provenance.clone(),
            vec!["c_runtime".into(), "linker_support".into()],
        ));
    }

    // 5. Rustup Manager in cargo/bin
    let rustup_exe = cargo_dir.join("bin").join("rustup.exe");
    let rustup_status = evaluate_file_status(&rustup_exe, "x86_64");
    resources.push(Resource::new(
        ResourceKind::Toolchain,
        Ecosystem::Rust,
        "rustup",
        Some("1.29.0".into()),
        platform.clone(),
        arch.clone(),
        Backend::Windows,
        abi.clone(),
        Some(rustup_exe.to_string_lossy().to_string()),
        Some("rustup".into()),
        Ownership::Managed,
        rustup_status,
        provenance.clone(),
        vec!["toolchain_manager".into(), "installer".into()],
    ));

    // 6. Aggregate Rust Toolchain
    let tc_status = if toolchain_dir.is_dir() {
        VerificationStatus::Discovered
    } else {
        VerificationStatus::Missing
    };
    resources.push(Resource::new(
        ResourceKind::Toolchain,
        Ecosystem::Rust,
        "rust",
        version,
        platform,
        arch,
        Backend::Windows,
        abi,
        Some(toolchain_dir.to_string_lossy().to_string()),
        Some("rustup".into()),
        Ownership::Managed,
        tc_status,
        provenance,
        vec!["toolchain".into(), toolchain_name],
    ));

    Ok(resources)
}

fn evaluate_file_status(path: &Path, expected_arch: &str) -> VerificationStatus {
    if !path.exists() {
        VerificationStatus::Missing
    } else if path.is_file() {
        match binary_architecture(path) {
            Ok(arch) if arch == expected_arch => VerificationStatus::Discovered,
            _ => VerificationStatus::Modified,
        }
    } else {
        VerificationStatus::Modified
    }
}

pub fn verify_rust_resource(
    resource: &mut Resource,
    rust_root: &Path,
) -> Result<String, String> {
    reject_links(rust_root)?;
    let rustup_home = rust_root.join("rustup");
    let cargo_home = rust_root.join("cargo");

    let Some(ref path_str) = resource.installation_path else {
        return Err("resource has no installation path".into());
    };
    let path = PathBuf::from(path_str);
    reject_links(&path)?;

    if !path.exists() {
        resource.verification_status = VerificationStatus::Missing;
        return Err(format!("path does not exist: {path_str}"));
    }

    if path.is_file() {
        let arch = binary_architecture(&path)?;
        if arch != "x86_64" {
            resource.verification_status = VerificationStatus::Modified;
            return Err(format!("PE architecture mismatch: observed {arch}, expected x86_64"));
        }
    }

    let output = match resource.name.as_str() {
        "rustc" | "cargo" | "rustup" => {
            let envs = [
                ("RUSTUP_HOME", rustup_home.to_str().unwrap_or_default()),
                ("CARGO_HOME", cargo_home.to_str().unwrap_or_default()),
            ];
            run_bounded_with_env(&path, &["--version"], &envs, Duration::from_secs(3))?
        }
        "rust" => {
            let rustc_bin = path.join("bin").join("rustc.exe");
            let envs = [
                ("RUSTUP_HOME", rustup_home.to_str().unwrap_or_default()),
                ("CARGO_HOME", cargo_home.to_str().unwrap_or_default()),
            ];
            run_bounded_with_env(&rustc_bin, &["--version"], &envs, Duration::from_secs(3))?
        }
        _ => {
            format!("filesystem resource validated at {path_str}")
        }
    };

    if let Some(ref expected_ver) = resource.version {
        if !output.contains(expected_ver) && resource.name != "rust-std" && resource.name != "rust-mingw" {
            resource.verification_status = VerificationStatus::Conflicted;
            return Err(format!("version probe mismatch: expected '{expected_ver}', got '{output}'"));
        }
    }

    resource.verification_status = VerificationStatus::Verified;
    resource.last_verified_at = Some(current_time_rfc3339());
    Ok(output.trim().to_string())
}

pub fn check_rust_filesystem_integrity(
    resource: &Resource,
    _rust_root: &Path,
) -> Vec<ReconciliationIssue> {
    let mut issues = Vec::new();
    let Some(ref path_str) = resource.installation_path else {
        issues.push(ReconciliationIssue {
            resource_id: resource.resource_id.clone(),
            issue_type: ReconciliationIssueType::MissingPath,
            severity: IssueSeverity::Error,
            description: format!("Resource '{}' has no installation path", resource.name),
        });
        return issues;
    };

    let path = Path::new(path_str);
    if let Err(e) = reject_links(path) {
        issues.push(ReconciliationIssue {
            resource_id: resource.resource_id.clone(),
            issue_type: ReconciliationIssueType::ReparseTraversalRejected,
            severity: IssueSeverity::Error,
            description: format!("Security check failed for path '{path_str}': {e}"),
        });
        return issues;
    }

    if !path.exists() {
        let issue_type = if path_str.ends_with(".exe") {
            ReconciliationIssueType::MissingExecutable
        } else {
            ReconciliationIssueType::MissingPath
        };
        issues.push(ReconciliationIssue {
            resource_id: resource.resource_id.clone(),
            issue_type,
            severity: IssueSeverity::Error,
            description: format!("Path does not exist on filesystem: '{path_str}'"),
        });
    } else if path.is_file() && path_str.ends_with(".exe") {
        match binary_architecture(path) {
            Ok(arch) => {
                let expected = format!("{:?}", resource.architecture).to_lowercase();
                if arch != expected {
                    issues.push(ReconciliationIssue {
                        resource_id: resource.resource_id.clone(),
                        issue_type: ReconciliationIssueType::DifferentBackendOrArchitecture,
                        severity: IssueSeverity::Error,
                        description: format!("Executable architecture mismatch: observed '{arch}', expected '{expected}'"),
                    });
                }
            }
            Err(e) => {
                issues.push(ReconciliationIssue {
                    resource_id: resource.resource_id.clone(),
                    issue_type: ReconciliationIssueType::ModifiedExecutable,
                    severity: IssueSeverity::Error,
                    description: format!("Executable header verification failed: {e}"),
                });
            }
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_settings() {
        let content = r#"
version = "12"
default_host_triple = "x86_64-pc-windows-gnu"
default_toolchain = "1.96.0-x86_64-pc-windows-gnu"
profile = "minimal"
"#;
        let (tc, triple) = parse_settings_toolchain(content).unwrap();
        assert_eq!(tc, "1.96.0-x86_64-pc-windows-gnu");
        assert_eq!(triple, "x86_64-pc-windows-gnu");
        assert_eq!(parse_version_from_toolchain_string(&tc).as_deref(), Some("1.96.0"));
    }

    #[test]
    fn synthetic_fixture_discovery_missing_files() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_vault_missing");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let rustup_dir = temp_dir.join("rustup");
        std::fs::create_dir_all(&rustup_dir).unwrap();

        std::fs::write(
            rustup_dir.join("settings.toml"),
            "default_toolchain = \"1.96.0-x86_64-pc-windows-gnu\"\ndefault_host_triple = \"x86_64-pc-windows-gnu\"\n",
        ).unwrap();

        let resources = discover_vault_rust(&temp_dir, None).unwrap();
        assert_eq!(resources.len(), 6);
        let rustc = resources.iter().find(|r| r.name == "rustc").unwrap();
        assert_eq!(rustc.verification_status, VerificationStatus::Missing);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn synthetic_fixture_discovery_corrupted_pe() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_vault_corrupt_pe");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let bin_dir = temp_dir.join("rustup/toolchains/1.96.0-x86_64-pc-windows-gnu/bin");
        std::fs::create_dir_all(&bin_dir).unwrap();

        std::fs::write(
            temp_dir.join("rustup/settings.toml"),
            "default_toolchain = \"1.96.0-x86_64-pc-windows-gnu\"\ndefault_host_triple = \"x86_64-pc-windows-gnu\"\n",
        ).unwrap();

        // Write a fake non-PE file to rustc.exe
        std::fs::write(bin_dir.join("rustc.exe"), "NOT_A_VALID_PE_HEADER").unwrap();

        let resources = discover_vault_rust(&temp_dir, None).unwrap();
        let rustc = resources.iter().find(|r| r.name == "rustc").unwrap();
        assert_eq!(rustc.verification_status, VerificationStatus::Modified);

        // Check integrity check also reports ModifiedExecutable
        let issues = check_rust_filesystem_integrity(rustc, &temp_dir);
        assert!(issues.iter().any(|i| i.issue_type == ReconciliationIssueType::ModifiedExecutable));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
