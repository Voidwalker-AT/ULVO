pub mod vault;
pub use vault::*;
pub mod workspace_audit;
pub use workspace_audit::*;
pub mod project_lifecycle;
pub use project_lifecycle::*;
pub mod migration_orchestrator;
pub use migration_orchestrator::*;
pub mod toolchain_inventory;
pub use toolchain_inventory::*;
pub mod connector;
pub use connector::*;

use std::path::{Path, PathBuf};
use ulvo_core::*;
use ulvo_adapters::{binary_architecture, bounded_read, classify_path, host_backend, probe, reject_links, TOOLS};

const MANIFESTS: &[&str] = &["pyproject.toml", "requirements.txt", "uv.lock", "package.json",
    "pnpm-lock.yaml", "Cargo.toml", "go.mod", "pom.xml", "build.gradle", "build.gradle.kts",
    "CMakeLists.txt", "Gemfile", "global.json", "Makefile", "meson.build"];

pub fn parse_manifest(text: &str) -> Result<ProjectManifest, String> {
    let result: ProjectManifest = toml::from_str(text).map_err(|e| format!("invalid ULVO manifest: {e}"))?;
    if result.schema_version != 1 { return Err("unsupported manifest schema_version (expected 1)".into()); }
    if result.runtimes.iter().any(|(k,v)| k.trim().is_empty() || v.trim().is_empty()) {
        return Err("runtime names and version requests must be nonempty".into());
    }
    Ok(result)
}

pub fn inspect(path: &Path) -> Result<ProjectInspection, String> {
    let absolute = if path.is_absolute() { path.to_path_buf() }
        else { std::env::current_dir().map_err(|e| e.to_string())?.join(path) };
    if absolute.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("parent traversal is not accepted; supply a normalized path".into());
    }
    reject_links(&absolute)?;
    let root = absolute.canonicalize().map_err(|e| format!("project path: {e}"))?;
    if !root.is_dir() { return Err("project path must be a directory".into()); }
    let mut found = Vec::new();
    for name in MANIFESTS {
        let candidate = root.join(name);
        if candidate.exists() { reject_links(&candidate)?; if candidate.is_file() { found.push(name.to_string()); } }
    }
    let manifest = root.join(".ulvo/manifest.toml");
    reject_links(&manifest)?;
    let ulvo_manifest = if manifest.exists() { Some(parse_manifest(&bounded_read(&manifest)?)?) } else { None };
    let connector = crate::connector::audit_project_connector(&root).ok();
    Ok(ProjectInspection {
        schema_version: 1,
        path: root.to_string_lossy().into_owned(),
        native_manifests: found,
        ulvo_manifest,
        connector,
        diagnostics: vec![
            "Native manifests are detected, not resolved or executed.".into(),
        ],
    })
}

fn candidate(path: PathBuf, tool: &str, source: &str, verify: bool) -> Option<Toolchain> {
    if !path.is_file() { return None; }
    let canonical = path.canonicalize().ok()?;
    let executable = canonical.to_string_lossy().into_owned();
    let backend = classify_path(&executable, host_backend());
    let capability = TOOLS.iter().find(|s| s.0 == tool)?.2;
    let file_identity = {
        #[cfg(unix)] {
            use std::os::unix::fs::MetadataExt;
            canonical.metadata().ok().map(|m| format!("{}:{}", m.dev(), m.ino()))
        }
        #[cfg(not(unix))] { None }
    };
    let mut record = Toolchain { tool: tool.into(), version: None, executable, file_identity,
        backend: backend.clone(), distribution: if backend == Backend::Wsl {
            std::env::var("WSL_DISTRO_NAME").ok() } else { None },
        architecture: binary_architecture(&canonical).ok(), vendor: None, sources: vec![source.into()],
        ownership: Ownership::Unknown, verification: Verification::Present,
        capabilities: vec![capability.into()], diagnostic: None, evidence: vec![] };
    if verify {
        match probe(&canonical, tool, &backend) {
            Ok(text) => {
                record.version = parse_version(&text);
                if record.version.is_some() {
                    record.verification = Verification::Verified;
                    record.vendor = if text.contains("Temurin") { Some("Temurin".into()) }
                        else if text.contains("OpenJ9") { Some("OpenJ9".into()) } else { None };
                } else {
                    record.verification = Verification::Failed;
                    record.diagnostic = Some("successful probe has no recognized version".into());
                }
            },
            Err(e) => { record.diagnostic = Some(e); }
        }
    }
    Some(record)
}

fn scan_bin(dir: &Path, source: &str, verify: bool, records: &mut Vec<Toolchain>) {
    // Constant candidate count; never execute files inferred from arbitrary directory entries.
    for (tool, _, _) in TOOLS {
        for suffix in ["", ".exe"] {
            if let Some(r) = candidate(dir.join(format!("{tool}{suffix}")), tool, source, verify) { records.push(r); }
        }
    }
}

fn sorted_limit(mut paths: Vec<PathBuf>, limit: usize) -> Vec<PathBuf> {
    paths.sort(); paths.dedup(); paths.truncate(limit); paths
}

fn children(dir: &Path) -> Vec<PathBuf> {
    sorted_limit(std::fs::read_dir(dir).ok().into_iter().flatten()
        .filter_map(Result::ok).filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path()).collect(), 128)
}

pub fn vscode_references(text: &str, host: Backend, source: &str) -> Result<Vec<Toolchain>, String> {
    let json: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("VS Code strict JSON: {e}"))?;
    let mut records = Vec::new();
    let known = [("python.defaultInterpreterPath", "python"), ("typescript.tsdk", "typescript"),
        ("java.home", "java"), ("java.jdt.ls.java.home", "java"), ("go.goroot", "go"),
        ("rust-analyzer.server.path", "rust-analyzer"), ("clangd.path", "clangd")];
    for (key, tool) in known {
        if let Some(value) = json.get(key).and_then(|v| v.as_str()) {
            records.push(Toolchain { tool: tool.into(), version: None, executable: value.into(), file_identity: None,
                backend: classify_path(value, host.clone()), distribution: None, architecture: None,
                vendor: None, sources: vec![format!("{source}:{key}")], ownership: Ownership::Unknown,
                verification: Verification::Configured, capabilities: vec!["configuration_reference".into()],
                diagnostic: Some("Configuration may refer to a directory, placeholder, or nonexistent path; never probed.".into()), evidence: vec![] });
        }
    }
    Ok(records)
}

pub fn inventory(all_backends: bool, verify: bool) -> Inventory {
    let host = host_backend();
    let mut records = Vec::new();
    let mut diagnostics = vec!["Bounded discovery; absence from inventory does not prove absence from the machine.".into()];
    if let Some(path) = std::env::var_os("PATH") {
        for dir in sorted_limit(std::env::split_paths(&path).filter(|p| p.is_absolute()).collect(), 128) {
            scan_bin(&dir, "path", verify, &mut records);
        }
    }
    let mut roots = Vec::new();
    if host == Backend::Wsl {
        roots.extend([PathBuf::from("/usr/bin"), PathBuf::from("/usr/local/bin")]);
        if let Some(home) = std::env::var_os("HOME") {
            let h = PathBuf::from(home);
            roots.push(h.join(".cargo/bin"));
            for toolchain in children(&h.join(".rustup/toolchains")) { roots.push(toolchain.join("bin")); }
        }
        for jdk in children(Path::new("/usr/lib/jvm")) { roots.push(jdk.join("bin")); }
    }
    if host == Backend::Windows || all_backends {
        let c = if cfg!(windows) { PathBuf::from("C:/") } else { PathBuf::from("/mnt/c") };
        for relative in ["Program Files/nodejs", "Program Files/Go/bin", "Program Files/dotnet",
            "Program Files/CMake/bin", "msys64/ucrt64/bin", "msys64/mingw64/bin",
            "msys64/clang64/bin", "msys64/usr/bin"] { roots.push(c.join(relative)); }
        for parent in [c.clone(), c.join("Program Files/Java"), c.join("Program Files/Eclipse Adoptium"),
            c.join("Program Files/IBM/Semeru"), c.join("Program Files/NVIDIA GPU Computing Toolkit/CUDA")] {
            for child in children(&parent) {
                let name = child.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                if parent == c && !(name.starts_with("python") || name.starts_with("ruby")) { continue; }
                roots.push(child.clone()); roots.push(child.join("bin"));
            }
        }
        if host == Backend::Wsl {
            diagnostics.push("Windows/MSYS2 paths on /mnt/c are read-only candidates; no foreign executable probes. E:-managed runtimes are not scanned.".into());
        }
        diagnostics.push("Cross-distribution WSL enumeration is not implemented; only the current WSL distribution can be inventoried.".into());
    }
    roots.sort(); roots.dedup();
    for root in roots { scan_bin(&root, "known_location", verify, &mut records); }
    let mut settings = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") { settings.push(PathBuf::from(appdata).join("Code/User/settings.json")); }
    if let Some(home) = std::env::var_os("HOME") {
        settings.push(PathBuf::from(home).join(".config/Code/User/settings.json"));
    }
    let bases = settings.clone();
    for base in bases {
        if let Some(parent) = base.parent() {
            for profile in children(&parent.join("profiles")) { settings.push(profile.join("settings.json")); }
        }
    }
    settings.sort(); settings.dedup();
    for path in settings {
        if path.is_file() {
            match bounded_read(&path).and_then(|text| vscode_references(&text, host.clone(), &path.to_string_lossy())) {
                Ok(mut found) => records.append(&mut found), Err(e) => diagnostics.push(e),
            }
        }
    }
    diagnostics.push("VS Code JSONC/comments, extension inventory and java.configuration.runtimes arrays are not supported yet.".into());
    Inventory { schema_version: 1, host_backend: host, toolchains: deduplicate(records), diagnostics }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn selection_sorts_before_limit_and_deduplicates() {
        let paths: Vec<_> = (0..160).rev().map(|i| PathBuf::from(format!("/root/{i:03}"))).collect();
        let mut reversed = paths.clone(); reversed.reverse(); reversed.extend(paths.clone());
        let result = sorted_limit(paths, 128);
        assert_eq!(result, sorted_limit(reversed, 128));
        assert_eq!(result.len(), 128);
        assert_eq!(result[0], PathBuf::from("/root/000"));
        assert_eq!(result[127], PathBuf::from("/root/127"));
        assert!(sorted_limit(result, 0).is_empty());
        assert!(children(Path::new("/__ulvo_missing_children__")).is_empty());
    }
    #[test] fn manifest_valid() {
        let m = parse_manifest("schema_version = 1\nname = 'demo'\n[runtimes]\npython = '3.12'\n").unwrap();
        assert_eq!(m.runtimes["python"], "3.12");
        assert_eq!(m.project_name(), Some("demo"));
    }
    #[test] fn manifest_valid_extended_python() {
        let text = "schema_version = 1\n\n[project]\nname = 'example'\n\n[runtimes]\npython = '3.12'\n\n[python]\nmanager = 'uv'\nlockfile = 'uv.lock'\n\n[environment]\nmode = 'ephemeral'\ncleanup = 'after_release'\n";
        let m = parse_manifest(text).unwrap();
        assert_eq!(m.project_name(), Some("example"));
        assert_eq!(m.runtimes["python"], "3.12");
        assert_eq!(m.python.as_ref().unwrap().manager.as_deref(), Some("uv"));
        assert_eq!(m.environment.as_ref().unwrap().mode.as_deref(), Some("ephemeral"));
    }
    #[test] fn manifest_invalid() {
        for text in ["bad[", "schema_version = 2", "schema_version = 1\ncommand = 'evil'",
            "schema_version = 1\n[runtimes]\npython = ''"] { assert!(parse_manifest(text).is_err()); }
    }
    #[test] fn vscode_unverified() {
        let r = vscode_references(r#"{"python.defaultInterpreterPath":"C:\\fake\\python.exe", "clangd.path":"clangd"}"#, Backend::Windows, "fixture").unwrap();
        assert_eq!(r.len(), 2); assert_eq!(r[0].verification, Verification::Configured);
        assert!(r[0].version.is_none());
        assert!(vscode_references("{bad", Backend::Windows, "fixture").is_err());
    }
    #[test] fn duplicate_candidates() {
        let mut r = vscode_references(r#"{"python.defaultInterpreterPath":"C:\\fake\\python.exe"}"#, Backend::Windows, "a").unwrap();
        let mut second = r[0].clone(); second.sources = vec!["b".into()]; r.push(second);
        let result = deduplicate(r); assert_eq!(result.len(), 1); assert_eq!(result[0].sources.len(), 2);
    }
    #[test] fn missing_project() { assert!(inspect(Path::new("/__ulvo_missing_project__")).is_err()); }
    #[test] fn underlying_identity_dedup() {
        let mut r = vscode_references(r#"{"python.defaultInterpreterPath":"/one/python"}"#, Backend::Wsl, "a").unwrap();
        r[0].file_identity = Some("fixture-device:fixture-inode".into());
        let mut second = r[0].clone(); second.executable = "/two/python".into(); r.push(second);
        assert_eq!(deduplicate(r).len(), 1);
    }
    #[test] fn synthetic_project() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/valid").canonicalize().unwrap();
        let result = inspect(&root).unwrap();
        assert!(result.native_manifests.contains(&"package.json".into()));
        assert!(result.ulvo_manifest.is_some());
    }
    #[test] fn invalid_project() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/invalid").canonicalize().unwrap();
        assert!(inspect(&root).is_err());
    }
}