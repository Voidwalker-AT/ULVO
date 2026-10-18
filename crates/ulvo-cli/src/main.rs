use clap::{Parser, Subcommand};
use std::path::PathBuf;
use ulvo_core::VaultRegistry;

#[derive(Parser)]
#[command(name = "ulvo", version, about = "ULVO — Universal Language & Virtual Environment Orchestrator")]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Inventory {
        #[arg(long)]
        all_backends: bool,
        /// Execute bounded version probes from fixed trusted installation roots.
        #[arg(long)]
        verify: bool,
    },
    Inspect {
        project_path: PathBuf,
    },
    Vault {
        #[arg(long, global = true)]
        vault_root: Option<PathBuf>,
        #[command(subcommand)]
        subcommand: VaultCommands,
    },
    Workspace {
        #[command(subcommand)]
        subcommand: WorkspaceCommands,
    },
}

#[derive(Subcommand)]
enum VaultCommands {
    /// List known managed resources in the vault
    List,
    /// Inspect a specific managed resource by its resource ID or name
    Inspect {
        resource_id: String,
    },
    /// Perform bounded verification of a managed resource by ID
    Verify {
        resource_id: String,
    },
    /// Display statistics of managed resources in the vault
    Stats,
}

#[derive(Subcommand)]
enum WorkspaceCommands {
    /// Perform a read-only scan of the workspace and report key metrics
    Scan {
        #[arg(default_value = "E:\\Workspace")]
        path: PathBuf,
    },
    /// Produce comprehensive workspace intelligence report
    Report {
        #[arg(default_value = "E:\\Workspace")]
        path: PathBuf,
    },
    /// Discover and list Python virtual environments across the workspace
    PythonEnvs {
        #[arg(default_value = "E:\\Workspace")]
        path: PathBuf,
    },
    /// Analyze package duplication across Python virtual environments
    Duplicates {
        #[arg(default_value = "E:\\Workspace")]
        path: PathBuf,
    },
    /// Calculate conservative, realistic, and theoretical storage savings
    Savings {
        #[arg(default_value = "E:\\Workspace")]
        path: PathBuf,
    },
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Commands::Inventory { all_backends, verify } => {
            let result = ulvo_discovery::inventory(all_backends, verify);
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?);
            } else {
                println!("ULVO inventory ({:?})", result.host_backend);
                for tool in result.toolchains {
                    println!("{:?} {} {} [{:?}, {:?}] {}", tool.backend, tool.tool,
                        tool.version.as_deref().unwrap_or("version unknown"), tool.verification,
                        tool.ownership, tool.executable);
                    if let Some(d) = tool.diagnostic { println!("  {d}"); }
                }
                for diagnostic in result.diagnostics { println!("Note: {diagnostic}"); }
            }
        }
        Commands::Inspect { project_path } => {
            let result = ulvo_discovery::inspect(&project_path)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?);
            } else {
                println!("Project: {}", result.path);
                println!("Native manifests: {}", result.native_manifests.join(", "));
                println!("ULVO manifest: {}", if result.ulvo_manifest.is_some() { "valid" } else { "absent" });
                for diagnostic in result.diagnostics { println!("Note: {diagnostic}"); }
            }
        }
        Commands::Vault { vault_root, subcommand } => {
            let vault_path = ulvo_discovery::resolve_vault_root(vault_root.as_deref())?;
            match subcommand {
                VaultCommands::List => {
                    let registry = ulvo_discovery::load_vault_registry(&vault_path)?;
                    let resources = registry.list();
                    if cli.json {
                        let resp = ulvo_discovery::VaultListResponse {
                            schema_version: 1,
                            total_resources: resources.len(),
                            resources: resources.into_iter().cloned().collect(),
                        };
                        println!("{}", serde_json::to_string_pretty(&resp).map_err(|e| e.to_string())?);
                    } else if resources.is_empty() {
                        println!("No managed resources found in vault: {}", vault_path.display());
                    } else {
                        println!("ULVO Vault Managed Resources ({}) [{}]:", vault_path.display(), resources.len());
                        for r in resources {
                            println!(
                                "  {:?} {:?} {} {} [{:?}, {:?}]",
                                r.backend,
                                r.resource_kind,
                                r.name,
                                r.version.as_deref().unwrap_or("unknown"),
                                r.verification_status,
                                r.ownership,
                            );
                            println!("    ID:   {}", r.resource_id);
                            if let Some(ref p) = r.installation_path {
                                println!("    Path: {}", p);
                            }
                        }
                    }
                }
                VaultCommands::Inspect { resource_id } => {
                    let registry = ulvo_discovery::load_vault_registry(&vault_path)?;
                    let resource = registry.get_by_str(&resource_id)
                        .ok_or_else(|| format!("resource '{resource_id}' not found in registry"))?;
                    if cli.json {
                        let resp = ulvo_discovery::VaultInspectResponse {
                            schema_version: 1,
                            resource: resource.clone(),
                        };
                        println!("{}", serde_json::to_string_pretty(&resp).map_err(|e| e.to_string())?);
                    } else {
                        println!("Resource: {}", resource.resource_id);
                        println!("  Kind:           {:?}", resource.resource_kind);
                        println!("  Ecosystem:      {:?}", resource.ecosystem);
                        println!("  Name:           {}", resource.name);
                        println!("  Version:        {}", resource.version.as_deref().unwrap_or("unknown"));
                        println!("  Platform:       {:?}", resource.platform);
                        println!("  Architecture:   {:?}", resource.architecture);
                        println!("  Backend:        {:?}", resource.backend);
                        println!("  ABI:            {:?}", resource.abi);
                        println!("  Installation:   {}", resource.installation_path.as_deref().unwrap_or("none"));
                        println!("  Manager:        {}", resource.manager.as_deref().unwrap_or("none"));
                        println!("  Ownership:      {:?}", resource.ownership);
                        println!("  Status:         {:?}", resource.verification_status);
                        println!("  Capabilities:   {}", resource.capabilities.join(", "));
                        if let Some(ref prov) = resource.provenance {
                            println!("  Provenance:");
                            println!("    Source:       {}", prov.source_type);
                            if let Some(ref u) = prov.canonical_url { println!("    URL:          {}", u); }
                            if let Some(ref h) = prov.expected_sha256 { println!("    Expected SHA: {}", h); }
                            if let Some(ref m) = prov.manifest_path { println!("    Manifest:     {}", m); }
                        }
                        if let Some(ref t) = resource.last_verified_at {
                            println!("  Last Verified:  {}", t);
                        }
                    }
                }
                VaultCommands::Verify { resource_id } => {
                    let mut registry = ulvo_discovery::load_vault_registry(&vault_path)?;
                    let (resource, output) = ulvo_discovery::verify_vault_resource_by_id(
                        &mut registry,
                        &resource_id,
                        &vault_path,
                    )?;
                    if cli.json {
                        let resp = ulvo_discovery::VaultVerifyResponse {
                            schema_version: 1,
                            resource,
                            output,
                        };
                        println!("{}", serde_json::to_string_pretty(&resp).map_err(|e| e.to_string())?);
                    } else {
                        println!("Verified Resource: {}", resource.resource_id);
                        println!("  Name:           {}", resource.name);
                        println!("  Status:         {:?}", resource.verification_status);
                        if let Some(ref t) = resource.last_verified_at {
                            println!("  Verified At:    {}", t);
                        }
                        println!("  Probe Output:   {}", output);
                    }
                }
                VaultCommands::Stats => {
                    let registry = ulvo_discovery::load_vault_registry(&vault_path)?;
                    let stats = registry.stats();
                    if cli.json {
                        let resp = ulvo_discovery::VaultStatsResponse {
                            schema_version: 1,
                            total_resources: stats.total_resources,
                            by_kind: stats.by_kind,
                            by_ecosystem: stats.by_ecosystem,
                            by_backend: stats.by_backend,
                            by_status: stats.by_status,
                            by_ownership: stats.by_ownership,
                        };
                        println!("{}", serde_json::to_string_pretty(&resp).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Vault Statistics ({}):", vault_path.display());
                        println!("  Total resources: {}", stats.total_resources);
                        if !stats.by_kind.is_empty() {
                            println!("  By Kind:");
                            for (k, v) in &stats.by_kind { println!("    {k}: {v}"); }
                        }
                        if !stats.by_ecosystem.is_empty() {
                            println!("  By Ecosystem:");
                            for (k, v) in &stats.by_ecosystem { println!("    {k}: {v}"); }
                        }
                        if !stats.by_backend.is_empty() {
                            println!("  By Backend:");
                            for (k, v) in &stats.by_backend { println!("    {k}: {v}"); }
                        }
                        if !stats.by_status.is_empty() {
                            println!("  By Verification Status:");
                            for (k, v) in &stats.by_status { println!("    {k}: {v}"); }
                        }
                        if !stats.by_ownership.is_empty() {
                            println!("  By Ownership:");
                            for (k, v) in &stats.by_ownership { println!("    {k}: {v}"); }
                        }
                    }
                }
            }
        }
        Commands::Workspace { subcommand } => {
            match subcommand {
                WorkspaceCommands::Scan { path } | WorkspaceCommands::Report { path } => {
                    let report = ulvo_discovery::audit_workspace(&path)?;
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Workspace Intelligence Audit ({})", report.workspace_root);
                        println!("  Total Storage:         {:.2} GB ({} files, {} unique physical allocations)",
                            report.total_logical_gb, report.total_files, report.unique_physical_files);
                        println!("  Hardlink Deduplication: {} files", report.hardlink_count);
                        println!("\nTop-Level Categories:");
                        for (cat, stats) in &report.category_breakdown {
                            println!("  {:<20} {:>8} files  {:>6.2} GB", cat, stats.file_count, stats.logical_gb);
                        }
                        println!("\nPython Virtual Environments: {} detected", report.python_environments.len());
                        println!("  Top Environments by Size:");
                        for env in report.python_environments.iter().take(8) {
                            let mb = (env.logical_bytes as f64) / (1024.0 * 1024.0);
                            println!("    {:<45} {:>7.1} MB  Python {:<8} ({} pkgs)",
                                env.relative_path, mb, env.python_version.as_deref().unwrap_or("unknown"), env.packages.len());
                        }
                        println!("\nPackage Duplication Analysis:");
                        println!("  Identical Package Copies: {} distinct duplicated distributions", report.package_duplications.len());
                        for dup in report.package_duplications.iter().take(6) {
                            let wasted_mb = (dup.total_wasted_bytes as f64) / (1024.0 * 1024.0);
                            println!("    {:<25} == {:<12} ({:>2} copies, {:>7.1} MB wasted)",
                                dup.package, dup.version, dup.instance_count, wasted_mb);
                        }
                        println!("\nStorage Savings Model:");
                        println!("  Conservative (exact package dedup)   : {:>6.2} GB ({:.1}% of workspace)",
                            report.storage_savings.conservative.recoverable_gb, report.storage_savings.conservative.percentage_of_workspace);
                        println!("  Realistic (uv cache + ephemeral envs): {:>6.2} GB ({:.1}% of workspace)",
                            report.storage_savings.realistic.recoverable_gb, report.storage_savings.realistic.percentage_of_workspace);
                        println!("  Theoretical (100% ephemeral venvs)  : {:>6.2} GB ({:.1}% of workspace)",
                            report.storage_savings.theoretical.recoverable_gb, report.storage_savings.theoretical.percentage_of_workspace);
                    }
                }
                WorkspaceCommands::PythonEnvs { path } => {
                    let envs = ulvo_discovery::scan_workspace_environments_only(&path)?;
                    if cli.json {
                        println!("{}", serde_json::json!({
                            "schema_version": 1,
                            "workspace_root": path.to_string_lossy(),
                            "total_environments": envs.len(),
                            "environments": envs
                        }));
                    } else {
                        println!("Discovered Python Virtual Environments ({}) [{}]:", path.display(), envs.len());
                        for env in envs {
                            let mb = (env.logical_bytes as f64) / (1024.0 * 1024.0);
                            let edit_flag = if env.has_editable { " [editable]" } else { "" };
                            let native_flag = if env.has_native { " [native]" } else { "" };
                            println!("  {:<50} {:>7.1} MB  Python {:<8}{}{}",
                                env.relative_path, mb, env.python_version.as_deref().unwrap_or("unknown"), edit_flag, native_flag);
                        }
                    }
                }
                WorkspaceCommands::Duplicates { path } => {
                    let envs = ulvo_discovery::scan_workspace_environments_only(&path)?;
                    let dups = ulvo_adapters::analyze_package_duplication(&envs);
                    if cli.json {
                        println!("{}", serde_json::json!({
                            "schema_version": 1,
                            "workspace_root": path.to_string_lossy(),
                            "total_duplicate_distributions": dups.len(),
                            "duplicates": dups
                        }));
                    } else {
                        println!("Duplicate Python Packages Across Environments ({}) [{}]:", path.display(), dups.len());
                        for dup in dups {
                            let wasted_mb = (dup.total_wasted_bytes as f64) / (1024.0 * 1024.0);
                            println!("  {:<25} == {:<12} ({:>2} copies, {:>7.1} MB wasted)",
                                dup.package, dup.version, dup.instance_count, wasted_mb);
                        }
                    }
                }
                WorkspaceCommands::Savings { path } => {
                    let envs = ulvo_discovery::scan_workspace_environments_only(&path)?;
                    let dups = ulvo_adapters::analyze_package_duplication(&envs);
                    let total_bytes: u64 = envs.iter().map(|e| e.logical_bytes).sum();
                    let savings = ulvo_adapters::calculate_storage_savings(total_bytes, &envs, &dups);
                    if cli.json {
                        println!("{}", serde_json::json!({
                            "schema_version": 1,
                            "workspace_root": path.to_string_lossy(),
                            "storage_savings": savings
                        }));
                    } else {
                        println!("ULVO Storage Optimization Potential ({}):", path.display());
                        println!("  Conservative (Package-Store Dedup)   : {:.2} GB ({:.1}%)",
                            savings.conservative.recoverable_gb, savings.conservative.percentage_of_workspace);
                        println!("    Confidence: {}", savings.conservative.confidence);
                        println!("  Realistic (uv cache + ephemeral envs): {:.2} GB ({:.1}%)",
                            savings.realistic.recoverable_gb, savings.realistic.percentage_of_workspace);
                        println!("    Confidence: {}", savings.realistic.confidence);
                        println!("  Theoretical (100% ephemeral venvs)  : {:.2} GB ({:.1}%)",
                            savings.theoretical.recoverable_gb, savings.theoretical.percentage_of_workspace);
                        println!("    Confidence: {}", savings.theoretical.confidence);
                    }
                }
            }
        }
    }
    Ok(())
}

fn main() {
    let cli = Cli::parse();
    let json = cli.json;
    if let Err(error) = run(cli) {
        if json {
            println!("{}", serde_json::json!({"schema_version": 1, "error": error}));
        } else {
            eprintln!("ulvo: {error}");
        }
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_legacy_and_vault() {
        assert!(Cli::try_parse_from(["ulvo", "inventory", "--all-backends", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "inspect", "/fixture", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "vault", "list"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "vault", "list", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "--json", "vault", "list"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "vault", "inspect", "some-id", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "vault", "verify", "some-id"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "vault", "stats"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "workspace", "scan", "E:\\Workspace", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "workspace", "report"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "workspace", "python-envs"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "workspace", "duplicates"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "workspace", "savings", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "install", "evil"]).is_err());
    }

    #[test]
    fn json_model() {
        let model = ulvo_core::Inventory {
            schema_version: 1,
            host_backend: ulvo_core::Backend::Wsl,
            toolchains: vec![],
            diagnostics: vec![],
        };
        let text = serde_json::to_string(&model).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["host_backend"], "wsl");
    }
}