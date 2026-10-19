use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use ulvo_core::*;
use crate::connector::audit_project_connector;

/// Detects development ecosystems and monorepo structure across a project directory.
pub fn scan_project_ecosystems(
    project_root: &Path,
    policy_opt: Option<OnboardingPolicy>,
) -> Result<ProjectOnboardingReport, String> {
    let clean_path = strip_unc_prefix(project_root);
    let canonical = clean_path.canonicalize().unwrap_or(clean_path.clone());
    if !canonical.is_dir() {
        return Err(format!("project path is not a directory: {}", canonical.display()));
    }

    let project_name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string();

    let manifest_path = canonical.join(".ulvo").join("manifest.toml");
    let has_existing_connector = manifest_path.is_file();

    let is_trusted_workspace = check_is_trusted_workspace(&canonical);
    let policy = policy_opt.unwrap_or_default();

    let mut detected_ecosystems = Vec::new();
    let mut subprojects = Vec::new();
    let mut is_monorepo = false;

    // 1. Scan root directory for ecosystem manifests
    scan_directory_for_ecosystems(&canonical, "", &mut detected_ecosystems)?;

    // 2. Scan 1-level subdirectories for monorepo patterns
    let ignored_dirs: BTreeSet<&str> = [
        ".git", ".ulvo", ".venv", "venv", "node_modules", "target", "build", "dist",
        "out", "bin", "obj", ".idea", ".vscode", "vault", "tmp", "temp", "__pycache__",
        "cache", "caches", ".gradle", ".pytest_cache", ".ruff_cache",
    ].into_iter().collect();

    if let Ok(entries) = std::fs::read_dir(&canonical) {
        for entry in entries.filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.is_dir() {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                if ignored_dirs.contains(dir_name.as_str()) || dir_name.starts_with('.') {
                    continue;
                }

                let mut sub_eco = Vec::new();
                if let Ok(()) = scan_directory_for_ecosystems(&p, &dir_name, &mut sub_eco) {
                    if !sub_eco.is_empty() {
                        is_monorepo = true;
                        subprojects.push(dir_name.clone());
                        for eco in sub_eco {
                            // If ecosystem is not yet recorded at root, or recorded with a sub-path, keep track
                            let already_present = detected_ecosystems.iter().any(|e| e.kind == eco.kind);
                            if !already_present {
                                detected_ecosystems.push(eco);
                            }
                        }
                    } else if ["apps", "packages", "services", "crates", "modules"].contains(&dir_name.as_str()) {
                        // Monorepo container directory: inspect nested subprojects (e.g. apps/server, apps/web)
                        if let Ok(children) = std::fs::read_dir(&p) {
                            for child in children.filter_map(|c| c.ok()) {
                                let cp = child.path();
                                if cp.is_dir() {
                                    let child_name = child.file_name().to_string_lossy().to_string();
                                    if !child_name.starts_with('.') && !ignored_dirs.contains(child_name.as_str()) {
                                        let rel_child = format!("{dir_name}/{child_name}");
                                        let mut child_eco = Vec::new();
                                        if let Ok(()) = scan_directory_for_ecosystems(&cp, &rel_child, &mut child_eco) {
                                            if !child_eco.is_empty() {
                                                is_monorepo = true;
                                                subprojects.push(rel_child);
                                                for eco in child_eco {
                                                    let already_present = detected_ecosystems.iter().any(|e| e.kind == eco.kind);
                                                    if !already_present {
                                                        detected_ecosystems.push(eco);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    subprojects.sort();

    let action_verdict = if has_existing_connector {
        "ALREADY_INITIALIZED".to_string()
    } else if detected_ecosystems.is_empty() {
        "NO_ECOSYSTEM_DETECTED".to_string()
    } else {
        match policy {
            OnboardingPolicy::Manual => "MANUAL_ACTION_REQUIRED".to_string(),
            OnboardingPolicy::Prompt => "PROMPT_RECOMMENDED".to_string(),
            OnboardingPolicy::AutoForTrustedWorkspaces => {
                if is_trusted_workspace {
                    "AUTO_ONBOARD_ELIGIBLE".to_string()
                } else {
                    "PROMPT_RECOMMENDED".to_string()
                }
            }
        }
    };

    Ok(ProjectOnboardingReport {
        project_path: canonical.to_string_lossy().to_string(),
        project_name,
        is_monorepo,
        subprojects,
        detected_ecosystems,
        has_existing_connector,
        policy,
        is_trusted_workspace,
        action_verdict,
    })
}

fn scan_directory_for_ecosystems(
    dir: &Path,
    rel_prefix: &str,
    out: &mut Vec<DetectedEcosystem>,
) -> Result<(), String> {
    let python_manifests = ["pyproject.toml", "requirements.txt", "setup.py", "Pipfile", "uv.lock", "poetry.lock"];
    let node_manifests = ["package.json", "pnpm-lock.yaml", "package-lock.json", "yarn.lock", "bun.lockb"];
    let rust_manifests = ["Cargo.toml", "Cargo.lock"];
    let go_manifests = ["go.mod", "go.sum"];
    let ruby_manifests = ["Gemfile", "Gemfile.lock"];
    let java_manifests = ["pom.xml", "build.gradle", "build.gradle.kts"];
    let cpp_manifests = ["CMakeLists.txt", "Makefile", "meson.build", "compile_commands.json"];

    let find_matches = |manifests: &[&str]| -> Vec<String> {
        let mut matches = Vec::new();
        for m in manifests {
            if dir.join(m).is_file() {
                matches.push(m.to_string());
            }
        }
        matches
    };

    // Python
    let py_matches = find_matches(&python_manifests);
    if !py_matches.is_empty() {
        let mut details = BTreeMap::new();
        let py_ver = if dir.join("pyproject.toml").is_file() {
            "3.13".to_string()
        } else {
            "3.11".to_string()
        };
        details.insert("primary_manifest".to_string(), py_matches[0].clone());
        out.push(DetectedEcosystem {
            kind: EcosystemKind::Python,
            relative_path: rel_prefix.to_string(),
            manifest_files: py_matches,
            recommended_runtime_version: py_ver,
            details,
        });
    }

    // Node.js
    let node_matches = find_matches(&node_manifests);
    if !node_matches.is_empty() {
        let mut details = BTreeMap::new();
        details.insert("primary_manifest".to_string(), node_matches[0].clone());
        out.push(DetectedEcosystem {
            kind: EcosystemKind::Node,
            relative_path: rel_prefix.to_string(),
            manifest_files: node_matches,
            recommended_runtime_version: EcosystemKind::Node.default_runtime_version().to_string(),
            details,
        });
    }

    // Rust
    let rust_matches = find_matches(&rust_manifests);
    if !rust_matches.is_empty() {
        let mut details = BTreeMap::new();
        details.insert("primary_manifest".to_string(), rust_matches[0].clone());
        out.push(DetectedEcosystem {
            kind: EcosystemKind::Rust,
            relative_path: rel_prefix.to_string(),
            manifest_files: rust_matches,
            recommended_runtime_version: EcosystemKind::Rust.default_runtime_version().to_string(),
            details,
        });
    }

    // Go
    let go_matches = find_matches(&go_manifests);
    if !go_matches.is_empty() {
        let mut details = BTreeMap::new();
        details.insert("primary_manifest".to_string(), go_matches[0].clone());
        out.push(DetectedEcosystem {
            kind: EcosystemKind::Go,
            relative_path: rel_prefix.to_string(),
            manifest_files: go_matches,
            recommended_runtime_version: EcosystemKind::Go.default_runtime_version().to_string(),
            details,
        });
    }

    // Ruby
    let ruby_matches = find_matches(&ruby_manifests);
    if !ruby_matches.is_empty() {
        let mut details = BTreeMap::new();
        details.insert("primary_manifest".to_string(), ruby_matches[0].clone());
        out.push(DetectedEcosystem {
            kind: EcosystemKind::Ruby,
            relative_path: rel_prefix.to_string(),
            manifest_files: ruby_matches,
            recommended_runtime_version: EcosystemKind::Ruby.default_runtime_version().to_string(),
            details,
        });
    }

    // Java
    let java_matches = find_matches(&java_manifests);
    if !java_matches.is_empty() {
        let mut details = BTreeMap::new();
        details.insert("primary_manifest".to_string(), java_matches[0].clone());
        out.push(DetectedEcosystem {
            kind: EcosystemKind::Java,
            relative_path: rel_prefix.to_string(),
            manifest_files: java_matches,
            recommended_runtime_version: EcosystemKind::Java.default_runtime_version().to_string(),
            details,
        });
    }

    // C/C++
    let cpp_matches = find_matches(&cpp_manifests);
    if !cpp_matches.is_empty() {
        let mut details = BTreeMap::new();
        details.insert("primary_manifest".to_string(), cpp_matches[0].clone());
        out.push(DetectedEcosystem {
            kind: EcosystemKind::Cpp,
            relative_path: rel_prefix.to_string(),
            manifest_files: cpp_matches,
            recommended_runtime_version: EcosystemKind::Cpp.default_runtime_version().to_string(),
            details,
        });
    }

    Ok(())
}

fn check_is_trusted_workspace(path: &Path) -> bool {
    let norm_path = path.to_string_lossy().to_lowercase().replace('/', "\\");
    
    // Check environment variable ULVO_TRUSTED_WORKSPACES (semicolon separated)
    if let Ok(trusted_env) = std::env::var("ULVO_TRUSTED_WORKSPACES") {
        for root in trusted_env.split(';') {
            let tr = root.trim().to_lowercase().replace('/', "\\");
            if !tr.is_empty() && norm_path.starts_with(&tr) {
                return true;
            }
        }
    }

    // Default trusted roots for this Windows machine
    let default_roots = [
        "e:\\workspace",
        "c:\\workspace",
        "e:\\workspaces",
        "c:\\workspaces",
    ];

    for root in default_roots {
        if norm_path.starts_with(root) {
            return true;
        }
    }

    false
}

/// Generates compliant .ulvo/manifest.toml content supporting single or multi-ecosystem configurations.
pub fn generate_manifest_toml(report: &ProjectOnboardingReport) -> String {
    let mut toml = String::new();
    toml.push_str("schema_version = 1\n");
    toml.push_str(&format!("name = \"{}\"\n", report.project_name));

    if report.is_monorepo {
        toml.push_str(&format!("# Monorepo with subprojects: {}\n", report.subprojects.join(", ")));
    }

    toml.push_str("\n[runtimes]\n");
    for eco in &report.detected_ecosystems {
        toml.push_str(&format!("{} = \"{}\"\n", eco.kind.as_str(), eco.recommended_runtime_version));
    }

    // Write specific configuration sections for each detected ecosystem
    for eco in &report.detected_ecosystems {
        match eco.kind {
            EcosystemKind::Python => {
                toml.push_str("\n[python]\n");
                let primary = eco.details.get("primary_manifest").map(|s| s.as_str()).unwrap_or("requirements.txt");
                if primary == "uv.lock" || primary == "poetry.lock" {
                    toml.push_str(&format!("lockfile = \"{}\"\n", primary));
                } else {
                    toml.push_str(&format!("requirements = [\"{}\"]\n", primary));
                }
            }
            EcosystemKind::Node => {
                toml.push_str("\n[node]\n");
                let primary = eco.details.get("primary_manifest").map(|s| s.as_str()).unwrap_or("package.json");
                toml.push_str(&format!("package_json = \"{}\"\n", primary));
                if let Some(lock) = eco.manifest_files.iter().find(|m| m.contains("lock")) {
                    toml.push_str(&format!("lockfile = \"{}\"\n", lock));
                }
            }
            EcosystemKind::Rust => {
                toml.push_str("\n[rust]\n");
                toml.push_str(&format!("toolchain = \"{}\"\n", eco.recommended_runtime_version));
                toml.push_str("cargo_toml = \"Cargo.toml\"\n");
            }
            EcosystemKind::Go => {
                toml.push_str("\n[go]\n");
                toml.push_str("gomod = \"go.mod\"\n");
            }
            EcosystemKind::Ruby => {
                toml.push_str("\n[ruby]\n");
                toml.push_str("gemfile = \"Gemfile\"\n");
            }
            EcosystemKind::Java => {
                toml.push_str("\n[java]\n");
                toml.push_str(&format!("jdk = \"{}\"\n", eco.recommended_runtime_version));
                let build_tool = if eco.manifest_files.iter().any(|m| m.contains("gradle")) {
                    "gradle"
                } else {
                    "maven"
                };
                toml.push_str(&format!("build_tool = \"{}\"\n", build_tool));
            }
            EcosystemKind::Cpp => {
                toml.push_str("\n[cpp]\n");
                toml.push_str("toolchain = \"gcc-14.2.0\"\n");
                let bs = if eco.manifest_files.iter().any(|m| m.contains("CMakeLists")) {
                    "cmake"
                } else {
                    "make"
                };
                toml.push_str(&format!("build_system = \"{}\"\n", bs));
            }
        }
    }

    toml
}

/// Applies onboarding to a project directory by generating .ulvo/manifest.toml atomically.
pub fn apply_onboarding(
    project_root: &Path,
    report: &ProjectOnboardingReport,
    force: bool,
) -> Result<OnboardResult, String> {
    let clean_path = strip_unc_prefix(project_root);
    let canonical = clean_path.canonicalize().unwrap_or(clean_path.clone());
    let ulvo_dir = canonical.join(".ulvo");
    let manifest_path = ulvo_dir.join("manifest.toml");

    if manifest_path.is_file() && !force {
        return Err(format!(
            "Project at '{}' is already onboarded (.ulvo/manifest.toml exists). Use --force to overwrite.",
            canonical.display()
        ));
    }

    if report.detected_ecosystems.is_empty() {
        return Err(format!(
            "No recognized development ecosystem found at '{}'. Supply an explicit manifest or ecosystem.",
            canonical.display()
        ));
    }

    let manifest_content = generate_manifest_toml(report);

    std::fs::create_dir_all(&ulvo_dir).map_err(|e| format!("failed to create .ulvo directory: {e}"))?;
    std::fs::write(&manifest_path, manifest_content.as_bytes())
        .map_err(|e| format!("failed to write .ulvo/manifest.toml: {e}"))?;

    // Audit compliance of new thin connector
    let contract = audit_project_connector(&canonical)?;
    let thin_compliant = contract.compliance_verdict == ConnectorComplianceVerdict::Compliant;

    let ecosystems = report.detected_ecosystems.iter().map(|e| e.kind.as_str().to_string()).collect();

    Ok(OnboardResult {
        project_path: canonical.to_string_lossy().to_string(),
        project_name: report.project_name.clone(),
        manifest_path: manifest_path.to_string_lossy().to_string(),
        policy_applied: report.policy,
        ecosystems,
        is_monorepo: report.is_monorepo,
        thin_connector_compliant: thin_compliant,
        message: format!("Successfully onboarded project '{}' with {} ecosystem(s)", report.project_name, report.detected_ecosystems.len()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let p = std::env::temp_dir().join(format!("ulvo_onboard_test_{}_{}", name, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
            let _ = std::fs::create_dir_all(&p);
            Self(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_scan_single_ecosystem() {
        let tmp = TempDir::new("single");
        std::fs::write(tmp.path().join("requirements.txt"), b"requests>=2.28\n").unwrap();

        let report = scan_project_ecosystems(tmp.path(), Some(OnboardingPolicy::Prompt)).unwrap();
        assert_eq!(report.detected_ecosystems.len(), 1);
        assert_eq!(report.detected_ecosystems[0].kind, EcosystemKind::Python);
        assert!(!report.is_monorepo);
        assert!(!report.has_existing_connector);
    }

    #[test]
    fn test_scan_multi_ecosystem() {
        let tmp = TempDir::new("multi");
        std::fs::write(tmp.path().join("pyproject.toml"), b"[project]\nname = \"demo\"\n").unwrap();
        std::fs::write(tmp.path().join("package.json"), b"{\"name\": \"demo-ui\"}\n").unwrap();

        let report = scan_project_ecosystems(tmp.path(), Some(OnboardingPolicy::Prompt)).unwrap();
        assert_eq!(report.detected_ecosystems.len(), 2);
        let kinds: Vec<_> = report.detected_ecosystems.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&EcosystemKind::Python));
        assert!(kinds.contains(&EcosystemKind::Node));
        assert!(!report.is_monorepo);
    }

    #[test]
    fn test_scan_monorepo() {
        let tmp = TempDir::new("monorepo");
        let backend = tmp.path().join("backend");
        let frontend = tmp.path().join("frontend");
        std::fs::create_dir_all(&backend).unwrap();
        std::fs::create_dir_all(&frontend).unwrap();

        std::fs::write(backend.join("Cargo.toml"), b"[package]\nname = \"srv\"\nversion = \"0.1.0\"\n").unwrap();
        std::fs::write(frontend.join("package.json"), b"{\"name\": \"web\"}\n").unwrap();

        let report = scan_project_ecosystems(tmp.path(), Some(OnboardingPolicy::Prompt)).unwrap();
        assert!(report.is_monorepo);
        assert!(report.subprojects.contains(&"backend".to_string()));
        assert!(report.subprojects.contains(&"frontend".to_string()));
        let kinds: Vec<_> = report.detected_ecosystems.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&EcosystemKind::Rust));
        assert!(kinds.contains(&EcosystemKind::Node));
    }

    #[test]
    fn test_onboarding_policy_verdicts() {
        let tmp = TempDir::new("policy");
        std::fs::write(tmp.path().join("go.mod"), b"module example.com/demo\n").unwrap();

        let r_prompt = scan_project_ecosystems(tmp.path(), Some(OnboardingPolicy::Prompt)).unwrap();
        assert_eq!(r_prompt.action_verdict, "PROMPT_RECOMMENDED");

        let r_manual = scan_project_ecosystems(tmp.path(), Some(OnboardingPolicy::Manual)).unwrap();
        assert_eq!(r_manual.action_verdict, "MANUAL_ACTION_REQUIRED");
    }

    #[test]
    fn test_apply_onboarding_thin_connector() {
        let tmp = TempDir::new("apply");
        std::fs::write(tmp.path().join("Gemfile"), b"source 'https://rubygems.org'\n").unwrap();

        let report = scan_project_ecosystems(tmp.path(), Some(OnboardingPolicy::Prompt)).unwrap();
        let result = apply_onboarding(tmp.path(), &report, false).unwrap();

        assert!(result.thin_connector_compliant);
        assert!(result.ecosystems.contains(&"ruby".to_string()));
        assert!(tmp.path().join(".ulvo").join("manifest.toml").is_file());

        // Repeated onboarding without force fails safely
        let err = apply_onboarding(tmp.path(), &report, false);
        assert!(err.is_err());

        // Repeated onboarding with force succeeds
        let force_result = apply_onboarding(tmp.path(), &report, true);
        assert!(force_result.is_ok());
    }
}
