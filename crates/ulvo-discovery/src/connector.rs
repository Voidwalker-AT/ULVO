use std::path::Path;
use ulvo_core::{
    ConnectorComplianceVerdict, ConnectorContract, ProjectManifest,
    CONNECTOR_MAX_BUDGET_BYTES, CONNECTOR_TARGET_BUDGET_BYTES, PROHIBITED_CONNECTOR_ARTIFACTS,
};
use ulvo_adapters::{bounded_read, is_reparse_point};

/// Prohibited binary file extensions that should never be stored in the lightweight connector folder.
const PROHIBITED_EXTENSIONS: &[&str] = &[
    "exe", "dll", "so", "dylib", "pyd", "node", "whl", "bin", "o", "obj", "a", "lib",
];

/// Maximum allowed single file size inside .ulvo (1 MiB), preventing logs/dumps from bloating connector.
const MAX_SINGLE_FILE_BYTES: u64 = 1024 * 1024;

/// Performs an exhaustive audit of a project's `.ulvo` connector contract.
pub fn audit_project_connector(project_root: &Path) -> Result<ConnectorContract, String> {
    let canonical_root = project_root
        .canonicalize()
        .map_err(|e| format!("failed to canonicalize project path '{}': {e}", project_root.display()))?;

    let connector_dir = canonical_root.join(".ulvo");
    let manifest_path = connector_dir.join("manifest.toml");

    let mut diagnostics = Vec::new();
    let mut prohibited_entries = Vec::new();
    let mut total_bytes: u64 = 0;
    let mut file_count: usize = 0;

    // 1. Check connector directory existence
    if !connector_dir.exists() {
        return Ok(ConnectorContract {
            schema_version: 1,
            project_path: canonical_root.to_string_lossy().into_owned(),
            connector_dir: connector_dir.to_string_lossy().into_owned(),
            manifest_path: manifest_path.to_string_lossy().into_owned(),
            total_bytes: 0,
            file_count: 0,
            target_budget_bytes: CONNECTOR_TARGET_BUDGET_BYTES,
            max_budget_bytes: CONNECTOR_MAX_BUDGET_BYTES,
            is_within_target_budget: true,
            is_within_max_budget: true,
            prohibited_entries_detected: vec![],
            referenced_native_manifests: vec![],
            compliance_verdict: ConnectorComplianceVerdict::MissingManifest,
            diagnostics: vec!["Connector directory '.ulvo' does not exist".into()],
        });
    }

    // 2. Reject reparse points on connector directory
    if is_reparse_point(&connector_dir) {
        return Err(format!(
            "Connector directory '{}' is a reparse point / junction; rejected for security",
            connector_dir.display()
        ));
    }

    // 3. Check manifest.toml presence
    if !manifest_path.is_file() {
        return Ok(ConnectorContract {
            schema_version: 1,
            project_path: canonical_root.to_string_lossy().into_owned(),
            connector_dir: connector_dir.to_string_lossy().into_owned(),
            manifest_path: manifest_path.to_string_lossy().into_owned(),
            total_bytes: 0,
            file_count: 0,
            target_budget_bytes: CONNECTOR_TARGET_BUDGET_BYTES,
            max_budget_bytes: CONNECTOR_MAX_BUDGET_BYTES,
            is_within_target_budget: true,
            is_within_max_budget: true,
            prohibited_entries_detected: vec![],
            referenced_native_manifests: vec![],
            compliance_verdict: ConnectorComplianceVerdict::MissingManifest,
            diagnostics: vec!["Manifest file '.ulvo/manifest.toml' not found".into()],
        });
    }

    // 4. Parse manifest.toml
    let manifest_content = bounded_read(&manifest_path)?;
    let manifest: ProjectManifest = match toml::from_str(&manifest_content) {
        Ok(m) => m,
        Err(e) => {
            return Ok(ConnectorContract {
                schema_version: 1,
                project_path: canonical_root.to_string_lossy().into_owned(),
                connector_dir: connector_dir.to_string_lossy().into_owned(),
                manifest_path: manifest_path.to_string_lossy().into_owned(),
                total_bytes: manifest_content.len() as u64,
                file_count: 1,
                target_budget_bytes: CONNECTOR_TARGET_BUDGET_BYTES,
                max_budget_bytes: CONNECTOR_MAX_BUDGET_BYTES,
                is_within_target_budget: (manifest_content.len() as u64) <= CONNECTOR_TARGET_BUDGET_BYTES,
                is_within_max_budget: (manifest_content.len() as u64) <= CONNECTOR_MAX_BUDGET_BYTES,
                prohibited_entries_detected: vec![],
                referenced_native_manifests: vec![],
                compliance_verdict: ConnectorComplianceVerdict::InvalidManifest,
                diagnostics: vec![format!("Failed to parse .ulvo/manifest.toml: {e}")],
            });
        }
    };

    let referenced_native_manifests = manifest.referenced_native_manifests();

    // 5. Scan .ulvo directory entries recursively
    scan_connector_dir_recursive(
        &connector_dir,
        &connector_dir,
        &mut total_bytes,
        &mut file_count,
        &mut prohibited_entries,
        &mut diagnostics,
    )?;

    // 6. Check budget constraints
    let is_within_target_budget = total_bytes <= CONNECTOR_TARGET_BUDGET_BYTES;
    let is_within_max_budget = total_bytes <= CONNECTOR_MAX_BUDGET_BYTES;

    // 7. Check for native manifests in project root
    for native_rel in &referenced_native_manifests {
        let native_full = canonical_root.join(native_rel);
        if native_full.is_file() {
            diagnostics.push(format!("Native manifest '{native_rel}' verified in project root"));
        } else {
            diagnostics.push(format!("Referenced native manifest '{native_rel}' not found in project root"));
        }
    }

    // 8. Check for project-local environment presence in project root
    for local_env in &[".venv", "venv", "node_modules"] {
        let local_path = canonical_root.join(local_env);
        if local_path.is_dir() {
            diagnostics.push(format!(
                "Notice: Project-local environment '{local_env}' detected in project root; candidate for migration/retirement."
            ));
        }
    }

    // 9. Formulate verdict
    let compliance_verdict = if !prohibited_entries.is_empty() {
        ConnectorComplianceVerdict::ProhibitedArtifactDetected
    } else if !is_within_max_budget {
        ConnectorComplianceVerdict::MaxBudgetExceeded
    } else if !is_within_target_budget {
        ConnectorComplianceVerdict::TargetBudgetExceeded
    } else {
        ConnectorComplianceVerdict::Compliant
    };

    Ok(ConnectorContract {
        schema_version: 1,
        project_path: canonical_root.to_string_lossy().into_owned(),
        connector_dir: connector_dir.to_string_lossy().into_owned(),
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        total_bytes,
        file_count,
        target_budget_bytes: CONNECTOR_TARGET_BUDGET_BYTES,
        max_budget_bytes: CONNECTOR_MAX_BUDGET_BYTES,
        is_within_target_budget,
        is_within_max_budget,
        prohibited_entries_detected: prohibited_entries,
        referenced_native_manifests,
        compliance_verdict,
        diagnostics,
    })
}

fn scan_connector_dir_recursive(
    root_connector: &Path,
    current_dir: &Path,
    total_bytes: &mut u64,
    file_count: &mut usize,
    prohibited_entries: &mut Vec<String>,
    diagnostics: &mut Vec<String>,
) -> Result<(), String> {
    let read_dir = match std::fs::read_dir(current_dir) {
        Ok(rd) => rd,
        Err(e) => {
            diagnostics.push(format!("Cannot read directory '{}': {e}", current_dir.display()));
            return Ok(());
        }
    };

    for entry_res in read_dir {
        let entry = match entry_res {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if is_reparse_point(&path) {
            prohibited_entries.push(format!("reparse_point:{}", path.display()));
            continue;
        }

        let file_name = entry.file_name().to_string_lossy().to_string();
        let rel_path = path
            .strip_prefix(root_connector)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();

        // Check prohibited directory/artifact names
        let lower_name = file_name.to_lowercase();
        if PROHIBITED_CONNECTOR_ARTIFACTS.iter().any(|&p| lower_name == p.to_lowercase()) {
            prohibited_entries.push(format!("prohibited_artifact:{rel_path}"));
        }

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        if meta.is_dir() {
            scan_connector_dir_recursive(
                root_connector,
                &path,
                total_bytes,
                file_count,
                prohibited_entries,
                diagnostics,
            )?;
        } else if meta.is_file() {
            *file_count += 1;
            let file_len = meta.len();
            *total_bytes += file_len;

            // Check single file size limit
            if file_len > MAX_SINGLE_FILE_BYTES {
                prohibited_entries.push(format!(
                    "file_size_exceeded:{rel_path} ({} bytes > 1 MiB limit)",
                    file_len
                ));
            }

            // Check binary extension
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if PROHIBITED_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                    prohibited_entries.push(format!("binary_executable_artifact:{rel_path}"));
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_audit_nonexistent_project_fails() {
        let non_existent = PathBuf::from("E:\\Workspace\\nonexistent_project_xyz");
        assert!(audit_project_connector(&non_existent).is_err());
    }

    #[test]
    fn test_audit_missing_connector_directory() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_missing_connector");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let report = audit_project_connector(&temp_dir).unwrap();
        assert_eq!(report.compliance_verdict, ConnectorComplianceVerdict::MissingManifest);
        assert!(!report.connector_dir.is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_audit_valid_thin_connector() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_valid_thin_connector");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let ulvo_dir = temp_dir.join(".ulvo");
        std::fs::create_dir_all(&ulvo_dir).unwrap();

        let manifest_content = r#"schema_version = 1
name = "test_thin_project"

[runtimes]
python = ">=3.10"

[python]
manager = "uv"
requirements = ["requirements.txt"]
"#;
        std::fs::write(ulvo_dir.join("manifest.toml"), manifest_content).unwrap();
        std::fs::write(temp_dir.join("requirements.txt"), b"requests>=2.28\n").unwrap();

        let report = audit_project_connector(&temp_dir).unwrap();
        assert_eq!(report.compliance_verdict, ConnectorComplianceVerdict::Compliant);
        assert!(report.is_within_target_budget);
        assert!(report.is_within_max_budget);
        assert_eq!(report.prohibited_entries_detected.len(), 0);
        assert_eq!(report.referenced_native_manifests, vec!["requirements.txt".to_string()]);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_audit_detects_prohibited_artifact_in_connector() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_prohibited_connector");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let ulvo_dir = temp_dir.join(".ulvo");
        let venv_dir = ulvo_dir.join(".venv");
        std::fs::create_dir_all(&venv_dir).unwrap();

        let manifest_content = r#"schema_version = 1
name = "test_bloated_connector"

[runtimes]
python = "3.11"
"#;
        std::fs::write(ulvo_dir.join("manifest.toml"), manifest_content).unwrap();
        std::fs::write(venv_dir.join("pyvenv.cfg"), b"home = C:\\Python311\n").unwrap();

        let report = audit_project_connector(&temp_dir).unwrap();
        assert_eq!(report.compliance_verdict, ConnectorComplianceVerdict::ProhibitedArtifactDetected);
        assert!(!report.prohibited_entries_detected.is_empty());
        assert!(report.prohibited_entries_detected.iter().any(|p| p.contains(".venv")));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_audit_detects_binary_file_in_connector() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_binary_connector");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let ulvo_dir = temp_dir.join(".ulvo");
        std::fs::create_dir_all(&ulvo_dir).unwrap();

        let manifest_content = r#"schema_version = 1
name = "test_binary_connector"

[runtimes]
python = "3.11"
"#;
        std::fs::write(ulvo_dir.join("manifest.toml"), manifest_content).unwrap();
        std::fs::write(ulvo_dir.join("payload.exe"), b"fake_binary_payload").unwrap();

        let report = audit_project_connector(&temp_dir).unwrap();
        assert_eq!(report.compliance_verdict, ConnectorComplianceVerdict::ProhibitedArtifactDetected);
        assert!(report.prohibited_entries_detected.iter().any(|p| p.contains("binary_executable_artifact")));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
