use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use ulvo_core::{
    MemoryVaultRegistry, PersistentRegistryEnvelope, Provenance, ReconciliationReport,
    RegistryTransactionRecord, Resource, VaultRegistry,
};
use ulvo_adapters::{
    bounded_read, check_rust_filesystem_integrity, discover_python_resources,
    discover_vault_msys2, discover_vault_rust, reject_links, sha256_hex, verify_rust_resource, write_atomic,
    EnvironmentFileLock,
};

#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactEntry {
    pub artifact_name: String,
    pub installer_version: Option<String>,
    pub toolchain_version: Option<String>,
    pub platform: Option<String>,
    pub target: Option<String>,
    pub canonical_url: Option<String>,
    pub expected_sha256: Option<String>,
    pub verification_policy: Option<String>,
    pub approval_status: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactManifest {
    pub schema_version: String,
    #[serde(default)]
    pub artifacts: Vec<ArtifactEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultListResponse {
    pub schema_version: u32,
    pub total_resources: usize,
    pub resources: Vec<Resource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultInspectResponse {
    pub schema_version: u32,
    pub resource: Resource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultVerifyResponse {
    pub schema_version: u32,
    pub resource: Resource,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStatsResponse {
    pub schema_version: u32,
    pub total_resources: usize,
    pub by_kind: BTreeMap<String, usize>,
    pub by_ecosystem: BTreeMap<String, usize>,
    pub by_backend: BTreeMap<String, usize>,
    pub by_status: BTreeMap<String, usize>,
    pub by_ownership: BTreeMap<String, usize>,
}

pub fn resolve_vault_root(custom: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(p) = custom {
        reject_links(p)?;
        if p.exists() {
            return Ok(p.canonicalize().unwrap_or_else(|_| p.to_path_buf()));
        } else {
            return Err(format!("specified vault root does not exist: {}", p.display()));
        }
    }

    if let Ok(env_val) = std::env::var("ULVO_VAULT_ROOT") {
        let env_p = PathBuf::from(env_val);
        if env_p.is_dir() && reject_links(&env_p).is_ok() {
            return Ok(env_p.canonicalize().unwrap_or(env_p));
        }
    }

    let direct = PathBuf::from("vault");
    if direct.is_dir() {
        reject_links(&direct)?;
        return Ok(direct.canonicalize().unwrap_or(direct));
    }

    if let Ok(cwd) = std::env::current_dir() {
        let candidate = cwd.join("vault");
        if candidate.is_dir() {
            reject_links(&candidate)?;
            return Ok(candidate.canonicalize().unwrap_or(candidate));
        }
        if let Some(parent) = cwd.parent() {
            let candidate2 = parent.join("vault");
            if candidate2.is_dir() {
                reject_links(&candidate2)?;
                return Ok(candidate2.canonicalize().unwrap_or(candidate2));
            }
            if let Some(grandparent) = parent.parent() {
                let candidate3 = grandparent.join("vault");
                if candidate3.is_dir() {
                    reject_links(&candidate3)?;
                    return Ok(candidate3.canonicalize().unwrap_or(candidate3));
                }
            }
        }
    }

    let well_known = PathBuf::from(r"E:\Workspace\Active\ULVO\vault");
    if well_known.is_dir() && reject_links(&well_known).is_ok() {
        return Ok(well_known.canonicalize().unwrap_or(well_known));
    }

    Ok(PathBuf::from("vault").canonicalize().unwrap_or_else(|_| PathBuf::from("vault")))
}

pub fn resolve_artifact_manifest(vault_root: &Path) -> Option<Provenance> {
    let parent = vault_root.parent()?;
    let manifest_path = parent.join("config").join("toolchain-artifacts.toml");
    if manifest_path.is_file() && reject_links(&manifest_path).is_ok() {
        if let Ok(content) = bounded_read(&manifest_path) {
            if let Ok(manifest) = toml::from_str::<ArtifactManifest>(&content) {
                if let Some(art) = manifest.artifacts.iter().find(|a| {
                    a.target.as_deref() == Some("x86_64-pc-windows-gnu")
                }) {
                    return Some(Provenance {
                        source_type: art.verification_policy.clone().unwrap_or_else(|| "manifest-sha256-pinned".into()),
                        canonical_url: art.canonical_url.clone(),
                        expected_sha256: art.expected_sha256.clone(),
                        actual_sha256: None,
                        manifest_path: Some(manifest_path.to_string_lossy().to_string()),
                        notes: art.notes.clone(),
                    });
                }
            }
        }
    }
    None
}

pub fn discover_fresh_vault_registry(vault_root: &Path) -> Result<MemoryVaultRegistry, String> {
    let mut registry = MemoryVaultRegistry::new();
    let provenance = resolve_artifact_manifest(vault_root);

    // 1. Discover Windows Rust managed resources
    let rust_root = vault_root.join("runtimes").join("rust");
    if rust_root.is_dir() {
        let rust_resources = discover_vault_rust(&rust_root, provenance.as_ref())?;
        for r in rust_resources {
            registry.register(r).map_err(|e| e.to_string())?;
        }
    }

    // 2. Discover Python and uv resources
    let python_resources = discover_python_resources(vault_root)?;
    for r in python_resources {
        registry.register(r).map_err(|e| e.to_string())?;
    }

    // 3. Discover MSYS2 managed resources
    let msys2_resources = discover_vault_msys2(vault_root)?;
    for r in msys2_resources {
        registry.register(r).map_err(|e| e.to_string())?;
    }

    // 4. Discover machine-wide external & adopted toolchains (Ruby, Go, Java, GCC, etc.)
    let scanner = crate::UniversalToolchainScanner::new(vault_root.to_path_buf());
    let discovered = scanner.scan_all();
    for tool in discovered {
        let res = tool.to_resource(vault_root);
        if registry.get(&res.resource_id).is_none() {
            let _ = registry.register(res);
        }
    }

    Ok(registry)
}

/// Read-only vault registry loading.
/// Reads existing persistent registry if present; otherwise runs in-memory discovery
/// without mutating disk or creating new registry files.
pub fn load_vault_registry_readonly(vault_root: &Path) -> Result<MemoryVaultRegistry, String> {
    reject_links(vault_root)?;
    let registry_dir = vault_root.join("registry");
    let registry_file = registry_dir.join("vault_registry.json");

    let mut reg = if registry_file.is_file() {
        load_durable_vault_registry(vault_root)?
    } else {
        discover_fresh_vault_registry(vault_root)?
    };

    let scanner = crate::UniversalToolchainScanner::new(vault_root.to_path_buf());
    for tool in scanner.scan_all() {
        let res = tool.to_resource(vault_root);
        if reg.get(&res.resource_id).is_none() {
            let _ = reg.register(res);
        }
    }

    Ok(reg)
}

pub fn load_vault_registry(vault_root: &Path) -> Result<MemoryVaultRegistry, String> {
    reject_links(vault_root)?;
    let registry_dir = vault_root.join("registry");
    let registry_file = registry_dir.join("vault_registry.json");

    if registry_file.is_file() {
        return load_durable_vault_registry(vault_root);
    }

    // Perform initial discovery and persist durable envelope
    let registry = discover_fresh_vault_registry(vault_root)?;
    save_durable_vault_registry(vault_root, &registry, "INITIAL_DISCOVERY", "system")?;
    Ok(registry)
}

pub fn load_durable_vault_registry(vault_root: &Path) -> Result<MemoryVaultRegistry, String> {
    reject_links(vault_root)?;
    let registry_dir = vault_root.join("registry");
    let registry_file = registry_dir.join("vault_registry.json");
    let lock_file = registry_dir.join("vault_registry.lock");

    let _lock = EnvironmentFileLock::acquire(&lock_file, 5000)
        .map_err(|e| format!("cannot acquire lock for durable registry: {e}"))?;

    let content = bounded_read(&registry_file)
        .map_err(|e| format!("failed to read durable registry at {}: {e}", registry_file.display()))?;

    let envelope: PersistentRegistryEnvelope = match serde_json::from_str(&content) {
        Ok(env) => env,
        Err(e) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let backup_file = registry_dir.join(format!("vault_registry.corrupt.{now}.json"));
            let _ = std::fs::copy(&registry_file, &backup_file);
            return Err(format!(
                "REGISTRY_CORRUPTION_DETECTED: JSON parse error '{e}'. Corrupted registry preserved at {}",
                backup_file.display()
            ));
        }
    };

    // Verify SHA-256 integrity
    let payload_json = serde_json::to_string(&envelope.payload)
        .map_err(|e| format!("failed to serialize payload for checksum: {e}"))?;
    let computed_hash = sha256_hex(payload_json.as_bytes());

    if computed_hash != envelope.sha256_checksum {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let backup_file = registry_dir.join(format!("vault_registry.corrupt.{now}.json"));
        let _ = std::fs::copy(&registry_file, &backup_file);
        return Err(format!(
            "REGISTRY_CORRUPTION_DETECTED: SHA-256 checksum mismatch (expected {}, computed {}). Corrupted registry preserved at {}",
            envelope.sha256_checksum, computed_hash, backup_file.display()
        ));
    }

    Ok(envelope.payload)
}

/// Executes a transactional read-modify-write on the durable vault registry.
/// Acquires the lock for the full duration of the transaction, preserves complete
/// transaction history, handles journal writes fail-closed, and persists atomically.
pub fn with_vault_registry_transaction<F, R>(
    vault_root: &Path,
    action: &str,
    initiator: &str,
    f: F,
) -> Result<R, String>
where
    F: FnOnce(&mut MemoryVaultRegistry) -> Result<R, String>,
{
    reject_links(vault_root)?;
    let registry_dir = vault_root.join("registry");
    std::fs::create_dir_all(&registry_dir).map_err(|e| e.to_string())?;

    let registry_file = registry_dir.join("vault_registry.json");
    let lock_file = registry_dir.join("vault_registry.lock");
    let journal_file = registry_dir.join("journal.jsonl");

    let _lock = EnvironmentFileLock::acquire(&lock_file, 10000)
        .map_err(|e| format!("cannot acquire lock for registry transaction: {e}"))?;

    let (mut registry, mut existing_transactions) = if registry_file.is_file() {
        let content = bounded_read(&registry_file)
            .map_err(|e| format!("failed to read registry for transaction: {e}"))?;
        let envelope: PersistentRegistryEnvelope = serde_json::from_str(&content)
            .map_err(|e| format!("corrupted registry schema: {e}"))?;
        (envelope.payload, envelope.transactions)
    } else {
        (discover_fresh_vault_registry(vault_root)?, Vec::new())
    };

    let result = f(&mut registry)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let tx_record = RegistryTransactionRecord {
        transaction_id: format!("tx_{now}_{}", std::process::id()),
        timestamp_unix: now,
        action: action.to_string(),
        resource_id: "registry".to_string(),
        initiator: initiator.to_string(),
        rollback_payload: None,
    };
    existing_transactions.push(tx_record.clone());

    // Write journal append-only; fail closed if journal cannot be written
    let tx_json = serde_json::to_string(&tx_record)
        .map_err(|e| format!("failed to serialize transaction record: {e}"))?;
    use std::io::Write;
    let mut jfile = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&journal_file)
        .map_err(|e| format!("failed to open transaction journal at {}: {e}", journal_file.display()))?;
    writeln!(jfile, "{}", tx_json)
        .map_err(|e| format!("failed to write transaction journal entry: {e}"))?;
    jfile.flush()
        .map_err(|e| format!("failed to flush transaction journal: {e}"))?;

    let payload_json = serde_json::to_string(&registry)
        .map_err(|e| format!("failed to serialize registry payload: {e}"))?;
    let checksum = sha256_hex(payload_json.as_bytes());

    let mut envelope = PersistentRegistryEnvelope::new(registry, checksum, now);
    envelope.transactions = existing_transactions;

    let envelope_json = serde_json::to_string_pretty(&envelope)
        .map_err(|e| format!("failed to serialize envelope: {e}"))?;

    write_atomic(&registry_file, envelope_json.as_bytes())?;
    Ok(result)
}

pub fn save_durable_vault_registry(
    vault_root: &Path,
    registry: &MemoryVaultRegistry,
    action: &str,
    initiator: &str,
) -> Result<(), String> {
    with_vault_registry_transaction(vault_root, action, initiator, |reg| {
        *reg = registry.clone();
        Ok(())
    })
}

pub fn verify_vault_resource_by_id(
    registry: &mut MemoryVaultRegistry,
    identifier: &str,
    vault_root: &Path,
) -> Result<(Resource, String), String> {
    let rust_root = vault_root.join("runtimes").join("rust");
    let target_id = {
        let res = registry.get_by_str(identifier)
            .ok_or_else(|| format!("resource '{identifier}' not found in registry"))?;
        res.resource_id.clone()
    };

    let mut resource = registry.resources.remove(&target_id)
        .ok_or_else(|| format!("resource '{identifier}' missing from registry map"))?;

    let output = match resource.ecosystem {
        ulvo_core::Ecosystem::Rust => {
            verify_rust_resource(&mut resource, &rust_root)?
        }
        _ => {
            return Err(format!("ecosystem {:?} does not support verification yet", resource.ecosystem));
        }
    };

    let result_clone = resource.clone();
    registry.resources.insert(target_id, resource);
    Ok((result_clone, output))
}

pub fn reconcile_vault_registry(
    registry: &MemoryVaultRegistry,
    vault_root: &Path,
) -> ReconciliationReport {
    let rust_root = vault_root.join("runtimes").join("rust");
    registry.reconcile_with(|r| {
        match r.ecosystem {
            ulvo_core::Ecosystem::Rust => {
                check_rust_filesystem_integrity(r, &rust_root)
            }
            _ => Vec::new(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_vault_discovery() {
        let temp = std::env::temp_dir().join("ulvo_empty_vault_discovery");
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();

        let reg = load_vault_registry(&temp).unwrap();
        // An empty vault fixture has zero managed resources
        let managed_count = reg.list().into_iter().filter(|r| r.ownership == ulvo_core::Ownership::Managed).count();
        assert_eq!(managed_count, 0);

        let stats = reg.stats();
        assert_eq!(stats.by_ownership.get("managed").copied().unwrap_or(0), 0);

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn json_schema_stability() {
        let list_resp = VaultListResponse {
            schema_version: 1,
            total_resources: 0,
            resources: vec![],
        };
        let text = serde_json::to_string(&list_resp).unwrap();
        let val: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(val["schema_version"], 1);
        assert_eq!(val["total_resources"], 0);
        assert!(val["resources"].is_array());
    }

    #[test]
    fn synthetic_reconciliation() {
        let temp = std::env::temp_dir().join("ulvo_synthetic_reconcile");
        let _ = std::fs::remove_dir_all(&temp);
        let rust_root = temp.join("runtimes/rust");
        std::fs::create_dir_all(&rust_root.join("rustup")).unwrap();

        std::fs::write(
            rust_root.join("rustup/settings.toml"),
            "default_toolchain = \"1.96.0-x86_64-pc-windows-gnu\"\ndefault_host_triple = \"x86_64-pc-windows-gnu\"\n",
        ).unwrap();

        let reg = load_vault_registry(&temp).unwrap();
        let report = reconcile_vault_registry(&reg, &temp);
        assert_eq!(report.schema_version, 1);
        let rust_count = reg.list().into_iter().filter(|r| r.ecosystem == ulvo_core::Ecosystem::Rust && r.ownership == ulvo_core::Ownership::Managed).count();
        assert_eq!(rust_count, 6);
        assert!(report.total_checked >= 6);
        // Since files are missing in synthetic empty fixture, clean must be false and issues must be reported
        assert!(!report.clean);
        assert!(!report.issues.is_empty());

        let _ = std::fs::remove_dir_all(&temp);
    }
}
