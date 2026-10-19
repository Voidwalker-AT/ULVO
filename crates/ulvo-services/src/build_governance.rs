use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildArtifactClassification {
    /// Final release binaries, manifests, and sources required for operation
    Essential,
    /// Cargo registry caches and package caches that accelerate compilation
    Reusable,
    /// Intermediate compiler objects, debug builds, incremental caches, and build scripts
    Disposable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildArtifactEntry {
    pub path: String,
    pub relative_path: String,
    pub classification: BuildArtifactClassification,
    pub physical_bytes: u64,
    pub file_count: usize,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildStoragePlan {
    pub workspace_root: String,
    pub target_dir: String,
    pub total_target_bytes: u64,
    pub essential_bytes: u64,
    pub reusable_bytes: u64,
    pub disposable_bytes: u64,
    pub compiler_active: bool,
    pub active_compiler_pids: Vec<u32>,
    pub items: Vec<BuildArtifactEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildStorageReport {
    pub schema_version: u32,
    pub workspace_root: String,
    pub target_dir: String,
    pub is_dry_run: bool,
    pub disposable_bytes_found: u64,
    pub physical_bytes_reclaimed: u64,
    pub remaining_target_bytes: u64,
    pub volume_free_bytes_delta: Option<i64>,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildRunMetric {
    pub build_id: String,
    pub start_time_unix: u64,
    pub end_time_unix: u64,
    pub free_bytes_before: u64,
    pub free_bytes_after: u64,
    pub peak_disk_consumption_bytes: u64,
    pub artifacts_produced: Vec<String>,
    pub reclaimable_intermediates_bytes: u64,
    pub net_storage_delta_bytes: i64,
}

pub struct BuildGovernanceService {
    workspace_root: PathBuf,
}

impl BuildGovernanceService {
    pub fn new<P: AsRef<Path>>(workspace_root: P) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
        }
    }

    /// Verifies that the workspace root is a valid Rust workspace or package.
    pub fn verify_workspace_ownership(&self) -> Result<(), String> {
        let manifest = self.workspace_root.join("Cargo.toml");
        if !manifest.is_file() {
            return Err(format!(
                "ownership verification failed: no Cargo.toml at {}",
                self.workspace_root.display()
            ));
        }
        let content = std::fs::read_to_string(&manifest)
            .map_err(|e| format!("cannot read Cargo.toml: {e}"))?;
        if !content.contains("[workspace]") && !content.contains("[package]") {
            return Err("invalid Cargo.toml: missing [workspace] or [package]".into());
        }
        Ok(())
    }

    /// Checks if Cargo currently holds an active lockfile in the target directory.
    pub fn is_target_locked(target_dir: &Path) -> bool {
        let lock_file = target_dir.join(".cargo-lock");
        if lock_file.is_file() {
            match std::fs::OpenOptions::new().write(true).open(&lock_file) {
                Ok(_) => false,
                Err(e) => {
                    // ERROR_SHARING_VIOLATION on Windows is 32
                    e.raw_os_error() == Some(32) || e.kind() == std::io::ErrorKind::PermissionDenied
                }
            }
        } else {
            false
        }
    }

    /// Scans running processes on Windows to detect active compiler or linker processes.
    pub fn detect_active_compilers(&self) -> Vec<u32> {
        let mut active_pids = Vec::new();
        let my_pid = std::process::id();

        if let Ok(output) = std::process::Command::new("tasklist")
            .arg("/FO")
            .arg("CSV")
            .arg("/NH")
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 2 {
                    let name = parts[0].trim_matches('"').to_lowercase();
                    if let Ok(pid) = parts[1].trim_matches('"').parse::<u32>() {
                        if pid == my_pid {
                            continue;
                        }
                        if name == "rustc.exe"
                            || name == "gcc.exe"
                            || name == "g++.exe"
                            || name == "link.exe"
                            || name == "ld.exe"
                        {
                            active_pids.push(pid);
                        }
                    }
                }
            }
        }
        active_pids
    }

    /// Formulates a read-only audit and cleanup plan for the workspace build artifacts.
    pub fn audit_build_storage(&self) -> Result<BuildStoragePlan, String> {
        self.verify_workspace_ownership()?;

        let target_dir = self.workspace_root.join("target");
        let active_compilers = self.detect_active_compilers();
        let target_locked = Self::is_target_locked(&target_dir);
        let compiler_active = target_locked || !active_compilers.is_empty();

        if !target_dir.is_dir() {
            return Ok(BuildStoragePlan {
                workspace_root: self.workspace_root.to_string_lossy().into_owned(),
                target_dir: target_dir.to_string_lossy().into_owned(),
                total_target_bytes: 0,
                essential_bytes: 0,
                reusable_bytes: 0,
                disposable_bytes: 0,
                compiler_active,
                active_compiler_pids: active_compilers,
                items: Vec::new(),
            });
        }

        let mut items = Vec::new();
        let mut total_target_bytes = 0u64;
        let mut essential_bytes = 0u64;
        let mut reusable_bytes = 0u64;
        let mut disposable_bytes = 0u64;

        // Inspect target directory contents
        if let Ok(entries) = std::fs::read_dir(&target_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();

                if name == "debug" {
                    let (phys, count) = ulvo_adapters::calculate_physical_dir_size(&p);
                    disposable_bytes += phys;
                    total_target_bytes += phys;
                    items.push(BuildArtifactEntry {
                        path: p.to_string_lossy().into_owned(),
                        relative_path: "target/debug".into(),
                        classification: BuildArtifactClassification::Disposable,
                        physical_bytes: phys,
                        file_count: count,
                        description: "Debug compilation tree and test binaries".into(),
                    });
                } else if name == "release" {
                    // Inspect inside release
                    let (rel_phys, _rel_count) = ulvo_adapters::calculate_physical_dir_size(&p);
                    total_target_bytes += rel_phys;

                    // Release binaries are Essential
                    let release_binaries = ["ulvo.exe", "ulvo-agent.exe", "ulvo-desktop.exe", "WebView2Loader.dll"];
                    for bin in &release_binaries {
                        let bp = p.join(bin);
                        if bp.is_file() {
                            if let Ok(meta) = bp.metadata() {
                                let sz = if let Ok((_id, info)) = ulvo_adapters::query_file_identity_and_size(&bp) {
                                    info.allocated_bytes.unwrap_or(info.logical_bytes)
                                } else {
                                    meta.len()
                                };
                                essential_bytes += sz;
                                items.push(BuildArtifactEntry {
                                    path: bp.to_string_lossy().into_owned(),
                                    relative_path: format!("target/release/{bin}"),
                                    classification: BuildArtifactClassification::Essential,
                                    physical_bytes: sz,
                                    file_count: 1,
                                    description: format!("Verified release binary: {bin}"),
                                });
                            }
                        }
                    }

                    // Release build intermediates are Disposable
                    for sub in &["build", "deps", "incremental", ".fingerprint"] {
                        let sp = p.join(sub);
                        if sp.is_dir() {
                            let (phys, count) = ulvo_adapters::calculate_physical_dir_size(&sp);
                            disposable_bytes += phys;
                            items.push(BuildArtifactEntry {
                                path: sp.to_string_lossy().into_owned(),
                                relative_path: format!("target/release/{sub}"),
                                classification: BuildArtifactClassification::Disposable,
                                physical_bytes: phys,
                                file_count: count,
                                description: format!("Release compilation intermediate directory ({sub})"),
                            });
                        }
                    }
                } else if name == "x86_64-pc-windows-gnu" || name.contains("windows-gnu") || name.contains("windows-msvc") {
                    let (phys, count) = ulvo_adapters::calculate_physical_dir_size(&p);
                    disposable_bytes += phys;
                    total_target_bytes += phys;
                    items.push(BuildArtifactEntry {
                        path: p.to_string_lossy().into_owned(),
                        relative_path: format!("target/{name}"),
                        classification: BuildArtifactClassification::Disposable,
                        physical_bytes: phys,
                        file_count: count,
                        description: format!("Duplicate cross-target directory ({name})"),
                    });
                } else if p.is_dir() {
                    let (phys, count) = ulvo_adapters::calculate_physical_dir_size(&p);
                    disposable_bytes += phys;
                    total_target_bytes += phys;
                    items.push(BuildArtifactEntry {
                        path: p.to_string_lossy().into_owned(),
                        relative_path: format!("target/{name}"),
                        classification: BuildArtifactClassification::Disposable,
                        physical_bytes: phys,
                        file_count: count,
                        description: format!("Unrecognized target subdirectory ({name})"),
                    });
                }
            }
        }

        // Check local cargo cache
        let cargo_local = self.workspace_root.join(".cargo-local");
        if cargo_local.is_dir() {
            let (phys, count) = ulvo_adapters::calculate_physical_dir_size(&cargo_local);
            reusable_bytes += phys;
            items.push(BuildArtifactEntry {
                path: cargo_local.to_string_lossy().into_owned(),
                relative_path: ".cargo-local".into(),
                classification: BuildArtifactClassification::Reusable,
                physical_bytes: phys,
                file_count: count,
                description: "Local Cargo registry and index cache".into(),
            });
        }

        Ok(BuildStoragePlan {
            workspace_root: self.workspace_root.to_string_lossy().into_owned(),
            target_dir: target_dir.to_string_lossy().into_owned(),
            total_target_bytes,
            essential_bytes,
            reusable_bytes,
            disposable_bytes,
            compiler_active,
            active_compiler_pids: active_compilers,
            items,
        })
    }

    /// Prunes disposable build artifacts with strict safety gates.
    /// If `all_intermediates` is false, preserves release dependency caches (target/release/deps) for fast incremental builds.
    pub fn prune_disposable_artifacts(&self, dry_run: bool, all_intermediates: bool) -> Result<BuildStorageReport, String> {
        let plan = self.audit_build_storage()?;

        if plan.compiler_active {
            return Err(format!(
                "BLOCKED_COMPILER_ACTIVE: Active compiler process(es) detected (PIDs: {:?}). Refusing cleanup while builds may be running.",
                plan.active_compiler_pids
            ));
        }

        let disk_before = ulvo_adapters::get_disk_free_bytes(&self.workspace_root).ok();
        let mut reclaimed_bytes = 0u64;
        let mut details = Vec::new();

        for item in &plan.items {
            if item.classification != BuildArtifactClassification::Disposable {
                continue;
            }

            // If not all_intermediates, preserve release dependencies
            if !all_intermediates && item.relative_path.starts_with("target/release/") {
                details.push(format!("PRESERVED_RELEASE_CACHE: {} (re-run with --all to reclaim)", item.relative_path));
                continue;
            }

            let p = PathBuf::from(&item.path);
            if !p.exists() {
                continue;
            }

            // Reparse point gate: refuse to delete symlinks or junctions
            if ulvo_adapters::is_reparse_point(&p) {
                details.push(format!("SKIPPED_REPARSE_POINT: {}", item.relative_path));
                continue;
            }

            if dry_run {
                details.push(format!("would reclaim {} ({} bytes, {} files)", item.relative_path, item.physical_bytes, item.file_count));
            } else {
                let target_boundary = PathBuf::from(&plan.target_dir);
                match Self::safe_clean_path_non_following(&p, &target_boundary) {
                    Ok(_) => {
                        reclaimed_bytes += item.physical_bytes;
                        details.push(format!("reclaimed {} ({} bytes, {} files)", item.relative_path, item.physical_bytes, item.file_count));
                    }
                    Err(e) => {
                        details.push(format!("FAILED to remove {}: {e}", item.relative_path));
                    }
                }
            }
        }

        let disk_after = ulvo_adapters::get_disk_free_bytes(&self.workspace_root).ok();
        let volume_delta = match (disk_before, disk_after) {
            (Some(b), Some(a)) => Some(a as i64 - b as i64),
            _ => None,
        };

        let remaining = if dry_run {
            plan.total_target_bytes
        } else {
            plan.total_target_bytes.saturating_sub(reclaimed_bytes)
        };

        Ok(BuildStorageReport {
            schema_version: 1,
            workspace_root: self.workspace_root.to_string_lossy().into_owned(),
            target_dir: plan.target_dir,
            is_dry_run: dry_run,
            disposable_bytes_found: plan.disposable_bytes,
            physical_bytes_reclaimed: reclaimed_bytes,
            remaining_target_bytes: remaining,
            volume_free_bytes_delta: volume_delta,
            details,
        })
    }

    /// Safely cleans a directory or file without following symlinks or reparse points,
    /// strictly enforcing boundary containment within `target_boundary`.
    pub fn safe_clean_path_non_following(path: &Path, target_boundary: &Path) -> Result<(u64, usize), String> {
        // Boundary containment verification
        let canonical_boundary = target_boundary.canonicalize().unwrap_or_else(|_| target_boundary.to_path_buf());
        let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        if !canonical_path.starts_with(&canonical_boundary) && !path.starts_with(target_boundary) {
            return Err(format!(
                "BOUNDARY_VIOLATION: Path '{}' is outside target boundary '{}'",
                path.display(),
                target_boundary.display()
            ));
        }

        if !path.exists() && std::fs::symlink_metadata(path).is_err() {
            return Ok((0, 0));
        }

        let meta = match std::fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) => return Err(format!("cannot read metadata for {}: {e}", path.display())),
        };

        // If the path itself is a symlink or reparse point: unlink directly, never follow
        let is_reparse = meta.file_type().is_symlink() || ulvo_adapters::is_reparse_point(path);
        if is_reparse {
            let res = if meta.is_dir() {
                std::fs::remove_dir(path)
            } else {
                std::fs::remove_file(path)
            };
            return match res {
                Ok(_) => Ok((0, 1)),
                Err(e) => Err(format!("failed to unlink reparse point {}: {e}", path.display())),
            };
        }

        if meta.is_file() {
            let sz = meta.len();
            return match std::fs::remove_file(path) {
                Ok(_) => Ok((sz, 1)),
                Err(e) => Err(format!("failed to remove file {}: {e}", path.display())),
            };
        }

        // Regular directory: clean contents non-following
        let mut total_bytes = 0u64;
        let mut total_files = 0usize;

        if meta.is_dir() {
            let read_dir = match std::fs::read_dir(path) {
                Ok(rd) => rd,
                Err(e) => return Err(format!("cannot read directory {}: {e}", path.display())),
            };

            for entry_res in read_dir {
                let entry = match entry_res {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let child = entry.path();
                let child_meta = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => match std::fs::symlink_metadata(&child) {
                        Ok(m) => m,
                        Err(_) => continue,
                    },
                };

                let child_is_reparse = child_meta.file_type().is_symlink() || ulvo_adapters::is_reparse_point(&child);
                if child_is_reparse {
                    // Non-following: only unlink reparse point without following
                    let _ = if child_meta.is_dir() {
                        std::fs::remove_dir(&child)
                    } else {
                        std::fs::remove_file(&child)
                    };
                    total_files += 1;
                } else if child_meta.is_dir() {
                    if let Ok((b, f)) = Self::safe_clean_path_non_following(&child, target_boundary) {
                        total_bytes += b;
                        total_files += f;
                    }
                } else {
                    let sz = child_meta.len();
                    if std::fs::remove_file(&child).is_ok() {
                        total_bytes += sz;
                        total_files += 1;
                    }
                }
            }

            // Remove empty parent directory
            let _ = std::fs::remove_dir(path);
        }

        Ok((total_bytes, total_files))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_governance_classification() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_build_gov");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create Cargo.toml
        std::fs::write(
            temp_dir.join("Cargo.toml"),
            "[workspace]\nmembers = [\"test_crate\"]\n",
        ).unwrap();

        // Create target structure
        let target = temp_dir.join("target");
        let debug = target.join("debug");
        let release = target.join("release");
        std::fs::create_dir_all(&debug).unwrap();
        std::fs::create_dir_all(&release).unwrap();

        // Create mock release binaries
        std::fs::write(release.join("ulvo.exe"), "mock_ulvo_exe").unwrap();
        std::fs::write(release.join("ulvo-desktop.exe"), "mock_desktop_exe").unwrap();

        // Create mock debug files
        std::fs::write(debug.join("test.rlib"), "mock_debug_rlib").unwrap();

        let gov = BuildGovernanceService::new(&temp_dir);
        let plan = gov.audit_build_storage().unwrap();

        assert!(plan.disposable_bytes > 0);
        assert!(plan.essential_bytes > 0);
        assert!(!plan.compiler_active);

        // Dry run
        let dry_report = gov.prune_disposable_artifacts(true, true).unwrap();
        assert_eq!(dry_report.physical_bytes_reclaimed, 0);
        assert!(debug.exists());

        // Actual prune
        let report = gov.prune_disposable_artifacts(false, true).unwrap();
        assert!(report.physical_bytes_reclaimed > 0);
        assert!(!debug.exists());
        assert!(release.join("ulvo.exe").is_file());
        assert!(release.join("ulvo-desktop.exe").is_file());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_safe_clean_boundary_violation() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_boundary_guard");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let target_boundary = temp_dir.join("target");
        std::fs::create_dir_all(&target_boundary).unwrap();

        let external_file = temp_dir.join("external_secret.txt");
        std::fs::write(&external_file, "irreplaceable").unwrap();

        // Attempting to clean a file outside the boundary must fail
        let res = BuildGovernanceService::safe_clean_path_non_following(&external_file, &target_boundary);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("BOUNDARY_VIOLATION"));
        assert!(external_file.is_file(), "External file must be untouched");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_safe_clean_non_following() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_non_following");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let target_boundary = temp_dir.join("target");
        let debug = target_boundary.join("debug");
        let nested = debug.join("nested");
        std::fs::create_dir_all(&nested).unwrap();

        let mock_file = nested.join("obj.o");
        std::fs::write(&mock_file, "compiler object bytes").unwrap();

        let res = BuildGovernanceService::safe_clean_path_non_following(&debug, &target_boundary);
        assert!(res.is_ok());
        let (bytes, count) = res.unwrap();
        assert_eq!(count, 1);
        assert!(bytes > 0);
        assert!(!debug.exists(), "debug directory should be cleaned");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
