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
    /// Project commands for inspecting, resolving, and diagnosing projects
    Project {
        #[command(subcommand)]
        subcommand: ProjectCommands,
    },
    /// Run a command inside an ephemeral or managed ULVO project environment
    Run {
        /// Project directory path
        project_path: PathBuf,
        /// Automatically reclaim the environment after execution quiesces
        #[arg(long)]
        reclaim: bool,
        /// Command and arguments to execute
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Environment lifecycle management commands
    Env {
        #[command(subcommand)]
        subcommand: EnvCommands,
    },
    /// Automated workspace-wide Python virtual-environment migration system
    Migrate {
        #[command(subcommand)]
        subcommand: MigrateCommands,
    },
    /// Universal multi-ecosystem toolchain inventory and staged adoption
    Toolchain {
        #[command(subcommand)]
        subcommand: ToolchainCommands,
    },
    /// GPU hardware, driver, and framework compatibility inspection
    Gpu {
        #[command(subcommand)]
        subcommand: GpuCommands,
    },
}

#[derive(Subcommand)]
enum ToolchainCommands {
    /// Comprehensive metadata-only scan across Windows PATH, known roots, MSYS2, VS Code, and WSL
    Inventory,
    /// View or formulate a typed 9-stage adoption plan for an external toolchain
    Plan {
        tool_name: String,
    },
}

#[derive(Subcommand)]
enum GpuCommands {
    /// Detect host physical GPU devices and driver configuration
    Detect,
    /// Inspect a project's GPU framework compatibility and storage requirements
    Inspect {
        project_path: PathBuf,
    },
}

#[derive(Subcommand)]
enum MigrateCommands {
    /// Perform a read-only scan of the workspace for Python environments and projects
    Scan {
        #[arg(default_value = "E:\\Workspace")]
        path: PathBuf,
    },
    /// Formulate a capacity-aware, prioritized migration plan
    Plan {
        #[arg(default_value = "E:\\Workspace")]
        path: PathBuf,
        /// Safety reserve in bytes (default: 500 MB)
        #[arg(long, default_value = "524288000")]
        safety_reserve: u64,
    },
    /// Verify a specific project against Gates A through J
    Verify {
        project_path: PathBuf,
    },
    /// Apply ULVO integration to a project (generates .ulvo/manifest.toml atomically without touching legacy env)
    Apply {
        project_path: PathBuf,
    },
    /// Safely retire legacy virtual environment if and only if all gates pass
    Retire {
        project_path: PathBuf,
        /// Explicit confirmation flag to authorize retirement
        #[arg(long)]
        yes: bool,
    },
    /// Display migration status of discovered projects in workspace
    Status {
        #[arg(default_value = "E:\\Workspace")]
        path: PathBuf,
    },
    /// Generate comprehensive migration summary report
    Report {
        #[arg(default_value = "E:\\Workspace")]
        path: PathBuf,
    },
}

#[derive(Subcommand)]
enum ProjectCommands {
    /// Inspect a project's ULVO manifest and environment configuration
    Inspect {
        project_path: PathBuf,
    },
    /// Resolve project runtime requirements and determine environment ID
    Resolve {
        project_path: PathBuf,
    },
    /// Validate prerequisites, runtimes, package managers, and directories
    Doctor {
        project_path: PathBuf,
    },
}

#[derive(Subcommand)]
enum EnvCommands {
    /// Check current environment state, leases, processes, and storage
    Status {
        project_path: PathBuf,
    },
    /// Safely reclaim environment if quiesced
    Reclaim {
        project_path: PathBuf,
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

fn run(cli: Cli) -> Result<i32, String> {
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
            Ok(0)
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
            Ok(0)
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
            Ok(0)
        }
        Commands::Workspace { subcommand } => {
            match subcommand {
                WorkspaceCommands::Scan { path } | WorkspaceCommands::Report { path } => {
                    let report = ulvo_discovery::audit_workspace(&path)?;
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Workspace Intelligence Audit ({})", report.workspace_root);
                        let scan_status_str = if report.scan_incomplete {
                            format!("INCOMPLETE ({} inaccessible entries)", report.inaccessible_count)
                        } else {
                            "COMPLETE".to_string()
                        };
                        println!("  Scan Status:           {}", scan_status_str);
                        println!("  Logical Storage:       {:.2} GB ({} files, {} unique physical allocations)",
                            report.total_logical_gb, report.total_files, report.unique_physical_files);
                        let phys_str = match report.total_physical_allocated_gb {
                            Some(gb) => format!("{:.2} GB", gb),
                            None => "Unknown (incomplete physical allocation telemetry)".to_string(),
                        };
                        println!("  Physical Allocation:   {}", phys_str);
                        println!("  Hardlink Deduplication: {} files", report.hardlink_count);
                        println!("\nTop-Level Categories:");
                        for (cat, stats) in &report.category_breakdown {
                            let cat_phys = match stats.physical_allocated_gb {
                                Some(gb) => format!("{:>6.2} GB physical", gb),
                                None => "  unknown physical".to_string(),
                            };
                            let inacc_note = if stats.inaccessible_count > 0 {
                                format!(" ({} inaccessible)", stats.inaccessible_count)
                            } else {
                                String::new()
                            };
                            println!("  {:<20} {:>8} files  {:>6.2} GB logical  {} {}",
                                cat, stats.file_count, stats.logical_gb, cat_phys, inacc_note);
                        }
                        println!("\nPython Virtual Environments: {} detected", report.python_environments.len());
                        println!("  Top Environments by Size:");
                        for env in report.python_environments.iter().take(8) {
                            let mb = (env.logical_bytes as f64) / (1024.0 * 1024.0);
                            let phys_mb = match env.physical_allocated_bytes {
                                Some(b) => format!("{:>7.1} MB physical", (b as f64) / (1024.0 * 1024.0)),
                                None => "   unknown physical".to_string(),
                            };
                            let edit_flag = if env.has_editable { " [editable]" } else { "" };
                            let native_flag = if env.has_native { " [native]" } else { "" };
                            println!("    {:<40} {:>7.1} MB logical  {}  Python {:<8} ({} pkgs){}{}",
                                env.relative_path, mb, phys_mb, env.python_version.as_deref().unwrap_or("unknown"), env.packages.len(), edit_flag, native_flag);
                        }
                        println!("\nPackage Duplication Analysis:");
                        println!("  Duplicated Package Distributions: {} detected", report.package_duplications.len());
                        for dup in report.package_duplications.iter().take(8) {
                            let wasted_mb = (dup.total_wasted_bytes as f64) / (1024.0 * 1024.0);
                            let flag = if dup.is_verified_identical {
                                "[verified identical]"
                            } else {
                                "[compatible candidate - differing ABI/RECORD]"
                            };
                            println!("    {:<25} == {:<12} ({:>2} copies, {:>7.1} MB wasted) {}",
                                dup.package, dup.version, dup.instance_count, wasted_mb, flag);
                        }
                        println!("\nStorage Savings Model:");
                        println!("  Conservative (verified identical dedup): {:>6.2} GB ({:.1}% of workspace)",
                            report.storage_savings.conservative.recoverable_gb, report.storage_savings.conservative.percentage_of_workspace);
                        println!("    Confidence: {}", report.storage_savings.conservative.confidence);
                        println!("  Realistic (ephemeral envs + persistent dedup): {:>6.2} GB ({:.1}% of workspace)",
                            report.storage_savings.realistic.recoverable_gb, report.storage_savings.realistic.percentage_of_workspace);
                        println!("    Confidence: {}", report.storage_savings.realistic.confidence);
                        println!("  Theoretical (100% ephemeral venvs)  : {:>6.2} GB ({:.1}% of workspace)",
                            report.storage_savings.theoretical.recoverable_gb, report.storage_savings.theoretical.percentage_of_workspace);
                        println!("    Confidence: {}", report.storage_savings.theoretical.confidence);
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
                            let flag = if dup.is_verified_identical {
                                "[verified identical]"
                            } else {
                                "[compatible candidate]"
                            };
                            println!("  {:<25} == {:<12} ({:>2} copies, {:>7.1} MB wasted) {}",
                                dup.package, dup.version, dup.instance_count, wasted_mb, flag);
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
                        for a in &savings.conservative.assumptions {
                            println!("      - {}", a);
                        }
                        println!("  Realistic (uv cache + ephemeral envs): {:.2} GB ({:.1}%)",
                            savings.realistic.recoverable_gb, savings.realistic.percentage_of_workspace);
                        println!("    Confidence: {}", savings.realistic.confidence);
                        for a in &savings.realistic.assumptions {
                            println!("      - {}", a);
                        }
                        println!("  Theoretical (100% ephemeral venvs)  : {:.2} GB ({:.1}%)",
                            savings.theoretical.recoverable_gb, savings.theoretical.percentage_of_workspace);
                        println!("    Confidence: {}", savings.theoretical.confidence);
                        for a in &savings.theoretical.assumptions {
                            println!("      - {}", a);
                        }
                    }
                }
            }
            Ok(0)
        }
        Commands::Project { subcommand } => {
            let vault_path = ulvo_discovery::resolve_vault_root(None)?;
            let orchestrator = ulvo_discovery::ProjectOrchestrator::new(vault_path)?;
            match subcommand {
                ProjectCommands::Inspect { project_path } => {
                    let result = orchestrator.inspect(&project_path)?;
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Project Inspection ({})", result.path);
                        println!("  Native manifests: {}", if result.native_manifests.is_empty() { "none".to_string() } else { result.native_manifests.join(", ") });
                        if let Some(ref m) = result.ulvo_manifest {
                            println!("  ULVO manifest:    valid (name: '{}', schema: {})", m.project_name().unwrap_or("unnamed"), m.schema_version);
                            println!("  Runtimes:         {:?}", m.runtimes);
                            if let Some(ref pm) = m.package_manager {
                                println!("  Package Manager:  {} (reqs: {:?})", pm.name.as_deref().unwrap_or("unknown"), pm.requirements);
                            }
                            if let Some(ref lc) = m.lifecycle {
                                println!("  Lifecycle:        {} (timeout: {}s, auto_reclaim: {:?})",
                                    lc.policy.as_deref().unwrap_or("ephemeral"),
                                    lc.lease_timeout_seconds.unwrap_or(300),
                                    lc.allow_reclaim_on_quiescence.unwrap_or(false)
                                );
                            }
                        } else {
                            println!("  ULVO manifest:    absent");
                        }
                        for diagnostic in result.diagnostics { println!("  Note: {diagnostic}"); }
                    }
                    Ok(0)
                }
                ProjectCommands::Resolve { project_path } => {
                    let result = orchestrator.resolve(&project_path)?;
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Project Resolution ({})", result.project_path);
                        println!("  Project Name:            {}", result.project_name);
                        println!("  Runtime Requested:       {}", result.runtime_requested);
                        println!("  Runtime Resolved:        {}", result.runtime_resolved);
                        println!("  Package Manager:         {}", result.package_manager);
                        println!("  Requirement Files:       {}", if result.requirement_files.is_empty() { "none".to_string() } else { result.requirement_files.join(", ") });
                        println!("  Environment Target:      {}", result.environment_target_path);
                        println!("  Specification Hash:      {}", result.specification_hash);
                        println!("  Lifecycle State:         {}", result.state);
                    }
                    Ok(0)
                }
                ProjectCommands::Doctor { project_path } => {
                    let result = orchestrator.doctor(&project_path)?;
                    let all_passed = result.checks.iter().all(|c| c.passed);
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Project Doctor ({})", result.project_path);
                        println!("  Overall Status:          {}", if all_passed { "PASSED" } else { "ATTENTION REQUIRED" });
                        println!("  Manifest Valid:          {}", result.manifest_valid);
                        println!("  uv Available:            {} ({})", result.uv_available, result.uv_version.as_deref().unwrap_or("not found"));
                        println!("  Runtime Available:       {} (requested: {}, resolved: {})",
                            result.runtime_available,
                            result.runtime_requested.as_deref().unwrap_or("none"),
                            result.runtime_resolved.as_deref().unwrap_or("none")
                        );
                        println!("  Dependencies Resolvable: {}", result.dependencies_resolvable);
                        println!("  Environment State:       {} ({} active processes)", result.environment_state, result.active_processes);
                        println!("\n  Diagnostic Checks:");
                        for check in &result.checks {
                            let flag = if check.passed { "[PASS]" } else { "[FAIL]" };
                            println!("    {:<7} {}: {}", flag, check.name, check.message);
                        }
                    }
                    Ok(if all_passed { 0 } else { 1 })
                }
            }
        }
        Commands::Run { project_path, reclaim, command } => {
            let vault_path = ulvo_discovery::resolve_vault_root(None)?;
            let orchestrator = ulvo_discovery::ProjectOrchestrator::new(vault_path)?;
            let exit_code = orchestrator.run(&project_path, &command, reclaim)?;
            Ok(exit_code)
        }
        Commands::Env { subcommand } => {
            let vault_path = ulvo_discovery::resolve_vault_root(None)?;
            let orchestrator = ulvo_discovery::ProjectOrchestrator::new(vault_path)?;
            match subcommand {
                EnvCommands::Status { project_path } => {
                    let result = orchestrator.status(&project_path)?;
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Environment Status ({})", result.project_path);
                        println!("  Environment ID:     {}", result.environment_id);
                        println!("  Environment Path:   {}", result.environment_path);
                        println!("  Lifecycle State:    {}", result.state);
                        println!("  Materialized:       {}", result.is_materialized);
                        println!("  Quiescent:          {}", result.is_quiescent);
                        println!("  Can Reclaim:        {}", result.can_reclaim);
                        println!("  Active Processes:   {}", result.active_pids.len());
                        if !result.active_pids.is_empty() {
                            println!("    PIDs:             {:?}", result.active_pids);
                        }
                        if let Some(ref spec) = result.specification_hash {
                            println!("  Specification Hash: {}", spec);
                        }
                        if let Some(ref exe) = result.python_executable {
                            println!("  Python Executable:  {}", exe);
                        }
                        if let Some(ref ver) = result.python_version {
                            println!("  Python Version:     {}", ver);
                        }
                    }
                    Ok(0)
                }
                EnvCommands::Reclaim { project_path } => {
                    let result = orchestrator.reclaim(&project_path)?;
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Environment Reclaim ({})", result.project_path);
                        println!("  Environment ID:     {}", result.environment_id);
                        println!("  Environment Path:   {}", result.environment_path);
                        println!("  Reclaimed:          {}", result.reclaimed);
                        println!("  Physical Freed:     {} bytes ({:.2} MB, {} files)",
                            result.physical_bytes_reclaimed,
                            (result.physical_bytes_reclaimed as f64) / (1024.0 * 1024.0),
                            result.file_count_reclaimed
                        );
                        if let Some(delta) = result.volume_free_bytes_delta {
                            println!("  Volume Delta:       {} bytes", delta);
                        }
                        println!("  Status:             {}", result.message);
                    }
                    Ok(0)
                }
            }
        }
        Commands::Migrate { subcommand } => {
            let vault_path = ulvo_discovery::resolve_vault_root(None)?;
            let orchestrator = ulvo_discovery::MigrationOrchestrator::new(vault_path)?;
            match subcommand {
                MigrateCommands::Scan { path } => {
                    let result = orchestrator.scan(&path)?;
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Workspace Migration Scan ({})", result.workspace_root);
                        println!("  Discovered Candidates:     {}", result.total_candidates);
                        println!("  Total Logical Storage:     {} bytes ({:.2} MB)",
                            result.total_logical_bytes,
                            result.total_logical_bytes as f64 / 1_048_576.0
                        );
                        println!("  Total Physical Allocation: {} bytes ({:.2} MB)",
                            result.total_physical_allocated_bytes,
                            result.total_physical_allocated_bytes as f64 / 1_048_576.0
                        );
                        println!("\n  Candidates:");
                        for c in &result.candidates {
                            let size_mb = c.physical_allocated_bytes.unwrap_or(c.logical_bytes) as f64 / 1_048_576.0;
                            println!("    - {:<28} [{:<6}] Size: {:>8.2} MB | Reproducibility: {:<20} | Status: {:<10} | Risk: {:<8}",
                                c.project_name,
                                c.python_version.as_deref().unwrap_or("?"),
                                size_mb,
                                c.reproducibility.to_string(),
                                c.status.to_string(),
                                c.risk_level.to_string(),
                            );
                            if let Some(ref reason) = c.blocker_reason {
                                println!("        Note: {}", reason);
                            }
                        }
                    }
                    Ok(0)
                }
                MigrateCommands::Plan { path, safety_reserve } => {
                    let result = orchestrator.plan(&path, safety_reserve)?;
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Migration Capacity-Aware Plan ({})", result.workspace_root);
                        println!("  Drive Free Storage:        {:.2} MB ({} bytes)",
                            result.drive_free_bytes as f64 / 1_048_576.0,
                            result.drive_free_bytes
                        );
                        println!("  Safety Reserve:            {:.2} MB",
                            result.safety_reserve_bytes as f64 / 1_048_576.0
                        );
                        println!("  Available Budget:          {:.2} MB",
                            result.available_budget_bytes as f64 / 1_048_576.0
                        );
                        println!("  Total Recoverable Storage: {:.2} MB",
                            result.total_recoverable_bytes as f64 / 1_048_576.0
                        );
                        println!("  Capacity-Blocked Storage:  {:.2} MB",
                            result.capacity_blocked_bytes as f64 / 1_048_576.0
                        );
                        println!("\n  Prioritized Migration Queue (Ordered):");
                        if result.prioritized_queue.is_empty() {
                            println!("    (No candidates currently eligible within capacity budget)");
                        } else {
                            for (idx, p) in result.prioritized_queue.iter().enumerate() {
                                let cand = result.candidates.iter().find(|c| &c.project_path == p);
                                let (name, size_mb, repro) = match cand {
                                    Some(c) => (
                                        c.project_name.as_str(),
                                        c.physical_allocated_bytes.unwrap_or(c.logical_bytes) as f64 / 1_048_576.0,
                                        c.reproducibility.to_string(),
                                    ),
                                    None => ("unknown", 0.0, "unknown".to_string()),
                                };
                                println!("    {:>2}. {:<24} Size: {:>7.2} MB | Reproducibility: {:<20} | Path: {}",
                                    idx + 1, name, size_mb, repro, p
                                );
                            }
                        }
                    }
                    Ok(0)
                }
                MigrateCommands::Verify { project_path } => {
                    let result = orchestrator.verify(&project_path)?;
                    let can_retire = result.can_retire;
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Migration Safety Gate Verification");
                        println!("  Project Path:       {}", result.project_path);
                        println!("  Legacy Environment: {}", result.legacy_env_path);
                        println!("  Retirement Status:  {}", if can_retire { "ELIGIBLE FOR RETIREMENT" } else { "RETIREMENT BLOCKED" });
                        println!("\n  Safety Gates (A - J):");
                        for gate in &result.gates {
                            let flag = if gate.passed { "[PASS]" } else { "[FAIL]" };
                            println!("    {:<7} {}: {}", flag, gate.name, gate.message);
                        }
                        if let Some(ref diag) = result.diagnostic {
                            println!("\n  Diagnostic: {}", diag);
                        }
                    }
                    Ok(if can_retire { 0 } else { 1 })
                }
                MigrateCommands::Apply { project_path } => {
                    let result = orchestrator.apply(&project_path)?;
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Migration Apply Integration");
                        println!("  Project Path:         {}", result.project_path);
                        println!("  Manifest Path:        {}", result.manifest_path);
                        println!("  Manifest Created:     {}", result.manifest_created);
                        println!("  Legacy Env Untouched: {}", result.legacy_env_untouched);
                        println!("  Status:               {}", result.message);
                    }
                    Ok(0)
                }
                MigrateCommands::Retire { project_path, yes } => {
                    let result = orchestrator.retire(&project_path, yes)?;
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Migration Legacy Environment Retirement");
                        println!("  Project Path:        {}", result.project_path);
                        println!("  Retired Target:      {}", result.legacy_env_path);
                        println!("  Physical Freed:      {} bytes ({:.2} MB)",
                            result.physical_bytes_reclaimed,
                            result.physical_bytes_reclaimed as f64 / 1_048_576.0
                        );
                        if let Some(delta) = result.volume_free_bytes_delta {
                            println!("  Volume Delta:        {} bytes", delta);
                        }
                        println!("  Post-Verification:   {}", if result.post_verification_passed { "PASSED" } else { "FAILED" });
                        println!("  Status:              {}", result.message);
                    }
                    Ok(0)
                }
                MigrateCommands::Status { path } => {
                    let candidates = orchestrator.status(&path)?;
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&candidates).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Migration Status ({})", path.display());
                        for c in &candidates {
                            let size_mb = c.physical_allocated_bytes.unwrap_or(c.logical_bytes) as f64 / 1_048_576.0;
                            println!("  - {:<24} Size: {:>7.2} MB | Status: {:<12} | Risk: {:<8} | Integrated: {}",
                                c.project_name, size_mb, c.status.to_string(), c.risk_level.to_string(), c.ulvo_integrated
                            );
                        }
                    }
                    Ok(0)
                }
                MigrateCommands::Report { path } => {
                    let result = orchestrator.report(&path)?;
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Workspace Migration & Reclamation Report");
                        println!("  Workspace Root:                   {}", result.workspace_root);
                        println!("  Discovered Environments:          {}", result.total_environments_discovered);
                        println!("  Successfully Integrated Projects: {}", result.successfully_integrated_projects);
                        println!("  Verified Retirements:             {}", result.verified_retirements);
                        println!("  Blocked Environments:             {}", result.blocked_environments);
                        println!("  Remaining Legacy Environments:    {}", result.remaining_legacy_environments);
                        println!("  Total Measured Physical Recovery: {} bytes ({:.2} MB)",
                            result.total_measured_physical_recovered_bytes,
                            result.total_measured_physical_recovered_bytes as f64 / 1_048_576.0
                        );
                        println!("\n  Migration Summary Table:");
                        println!("  {:<24} | {:<7} | {:>9} | {:<16} | {:<10} | {:<10} | {:<14} | {:>9} | {:<15}",
                            "Project Name", "Python", "Phys MB", "Reproducibility", "ULVO Int", "Func Ver", "Eligibility", "Recov MB", "Blocker"
                        );
                        println!("  {}", "-".repeat(125));
                        for row in &result.summary_table {
                            let phys_mb = row.physical_bytes as f64 / 1_048_576.0;
                            let recov_mb = row.actual_recovered_bytes as f64 / 1_048_576.0;
                            let blocker_str = row.blocker.as_deref().unwrap_or("-");
                            let truncated_blocker = if blocker_str.len() > 25 {
                                &blocker_str[..25]
                            } else {
                                blocker_str
                            };
                            println!("  {:<24} | {:<7} | {:>9.2} | {:<16} | {:<10} | {:<10} | {:<14} | {:>9.2} | {}",
                                if row.project_name.len() > 24 { &row.project_name[..24] } else { &row.project_name },
                                if row.python_version.len() > 7 { &row.python_version[..7] } else { &row.python_version },
                                phys_mb,
                                if row.reproducibility.len() > 16 { &row.reproducibility[..16] } else { &row.reproducibility },
                                row.ulvo_integration_status,
                                row.functional_verification,
                                row.retirement_eligibility,
                                recov_mb,
                                truncated_blocker
                            );
                        }
                    }
                    Ok(0)
                }
            }
        }
        Commands::Toolchain { subcommand } => {
            let vault_path = ulvo_discovery::resolve_vault_root(None)?;
            match subcommand {
                ToolchainCommands::Inventory => {
                    let scanner = ulvo_discovery::UniversalToolchainScanner::new(vault_path);
                    let toolchains = scanner.scan_all();
                    if cli.json {
                        let json_arr: Vec<_> = toolchains.iter().map(|t| serde_json::json!({
                            "name": t.name,
                            "executable": t.executable_path,
                            "backend": t.backend,
                            "ecosystem": t.ecosystem,
                            "kind": t.kind,
                            "ownership": t.ownership,
                            "source": t.discovery_source,
                            "capabilities": t.capabilities,
                        })).collect();
                        println!("{}", serde_json::to_string_pretty(&json_arr).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Universal Toolchain Inventory ({} discovered)", toolchains.len());
                        println!("  {:<16} | {:<8} | {:<10} | {:<12} | {}",
                            "TOOL", "BACKEND", "ECOSYSTEM", "OWNERSHIP", "EXECUTABLE PATH");
                        println!("  {}", "-".repeat(85));
                        for t in &toolchains {
                            println!("  {:<16} | {:<8?} | {:<10?} | {:<12?} | {}",
                                t.name, t.backend, t.ecosystem, t.ownership, t.executable_path.display());
                        }
                    }
                    Ok(0)
                }
                ToolchainCommands::Plan { tool_name } => {
                    let scanner = ulvo_discovery::UniversalToolchainScanner::new(vault_path.clone());
                    let toolchains = scanner.scan_all();
                    let found = toolchains.into_iter().find(|t| t.name.eq_ignore_ascii_case(&tool_name));
                    match found {
                        Some(t) => {
                            let plan = ulvo_core::ToolchainAdoptionPlan::new(
                                &t.name,
                                t.ecosystem.clone(),
                                t.executable_path.to_string_lossy(),
                                format!("{}\\{}", vault_path.join("runtimes").display(), t.name),
                                t.discovery_source.clone(),
                                t.version.clone(),
                                ulvo_core::Architecture::X86_64,
                                match t.backend {
                                    ulvo_core::Backend::Msys2 => ulvo_core::Abi::Gnu,
                                    _ => ulvo_core::Abi::None,
                                },
                                ulvo_core::AdoptionStrategy::ReferenceExternal,
                            );
                            if cli.json {
                                println!("{}", serde_json::to_string_pretty(&plan).map_err(|e| e.to_string())?);
                            } else {
                                println!("ULVO Toolchain Adoption Plan ({})", plan.tool_id);
                                println!("  Plan ID:             {}", plan.plan_id);
                                println!("  Ecosystem:           {:?}", plan.ecosystem);
                                println!("  Original Location:   {}", plan.original_location);
                                println!("  Intended Vault Path: {}", plan.intended_ulvo_location);
                                println!("  Lifecycle State:     {}", plan.state);
                                println!("  Adoption Strategy:   {:?}", plan.strategy);
                                println!("  Ownership:           {:?}", plan.ownership_classification);
                                println!("  Rollback Available:  {}", plan.rollback_plan.can_rollback_automatically);
                            }
                            Ok(0)
                        }
                        None => {
                            Err(format!("Tool '{tool_name}' not found in discovered inventory. Run 'ulvo toolchain inventory' to view detected tools."))
                        }
                    }
                }
            }
        }
        Commands::Gpu { subcommand } => {
            match subcommand {
                GpuCommands::Detect => {
                    let gpu_opt = ulvo_discovery::ProjectOrchestrator::detect_host_gpu();
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&gpu_opt).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO Host GPU Device Telemetry");
                        match gpu_opt {
                            Some(gpu) => {
                                println!("  Device Name:        {}", gpu.name);
                                println!("  Driver Version:     {}", gpu.driver_version);
                                println!("  Total VRAM:         {} MiB ({:.2} GB)", gpu.vram_mib, gpu.vram_mib as f64 / 1024.0);
                                println!("  Compute Capability: {}", gpu.compute_capability);
                                println!("  Classification:     SYSTEM_MANAGED (Kernel display driver relocation prohibited)");
                            }
                            None => {
                                println!("  No NVIDIA GPU or display driver detected on host system.");
                            }
                        }
                    }
                    Ok(0)
                }
                GpuCommands::Inspect { project_path } => {
                    let vault_path = ulvo_discovery::resolve_vault_root(None)?;
                    let orch = ulvo_discovery::ProjectOrchestrator::new(vault_path.clone())?;
                    let root = orch.resolve_project_root(&project_path)?;
                    let manifest_res = orch.load_manifest(&root);
                    let host_gpu = ulvo_discovery::ProjectOrchestrator::detect_host_gpu();
                    let free_bytes = ulvo_adapters::get_disk_free_bytes(&vault_path).unwrap_or(0);

                    if cli.json {
                        let payload = serde_json::json!({
                            "project_path": root.to_string_lossy(),
                            "manifest_found": manifest_res.is_ok(),
                            "host_gpu": host_gpu,
                            "vault_free_bytes": free_bytes,
                        });
                        println!("{}", serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?);
                    } else {
                        println!("ULVO GPU Environment Inspector ({})", root.display());
                        if let Ok(ref m) = manifest_res {
                            if let Some(ref g) = m.gpu {
                                println!("  GPU Required:       {}", g.required.unwrap_or(false));
                                println!("  Target Framework:   {}", g.framework.as_deref().unwrap_or("none"));
                                println!("  Framework Version:  {}", g.framework_version.as_deref().unwrap_or("none"));
                                println!("  Target CUDA:        {}", g.cuda_version.as_deref().unwrap_or("none"));
                                println!("  Min Compute Arch:   {}", g.min_compute_capability.as_deref().unwrap_or("none"));
                            } else {
                                println!("  GPU Configuration:  None declared in manifest (CPU execution mode)");
                            }
                        } else {
                            println!("  Manifest:           Not found or invalid");
                        }

                        println!("\n  Host GPU Telemetry:");
                        if let Some(gpu) = host_gpu {
                            println!("    Device:           {}", gpu.name);
                            println!("    Driver Version:   {}", gpu.driver_version);
                            println!("    VRAM:             {} MiB", gpu.vram_mib);
                            println!("    Compute Arch:     {}", gpu.compute_capability);
                        } else {
                            println!("    No NVIDIA GPU detected");
                        }

                        println!("\n  Storage Capacity:");
                        println!("    Vault Free Bytes: {} ({:.2} GB)", free_bytes, free_bytes as f64 / 1_073_741_824.0);
                        let can_fit = free_bytes >= 629_145_600;
                        println!("    Capacity Verdict: {}", if can_fit { "SUFFICIENT_SPACE" } else { "BLOCKED_CAPACITY" });
                    }
                    Ok(0)
                }
            }
        }
    }
}

fn main() {
    let cli = Cli::parse();
    let json = cli.json;
    match run(cli) {
        Ok(code) => {
            if code != 0 {
                std::process::exit(code);
            }
        }
        Err(error) => {
            if json {
                println!("{}", serde_json::json!({"schema_version": 1, "error": error}));
            } else {
                eprintln!("ulvo: {error}");
            }
            std::process::exit(1);
        }
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
        assert!(Cli::try_parse_from(["ulvo", "project", "inspect", "/fixture", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "project", "resolve", "/fixture"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "project", "doctor", "/fixture"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "run", "/fixture", "--", "python", "-m", "unittest"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "run", "/fixture", "--reclaim", "--", "python", "app.py"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "env", "status", "/fixture"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "env", "reclaim", "/fixture"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "toolchain", "inventory"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "toolchain", "inventory", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "toolchain", "plan", "python"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "gpu", "detect"]).is_ok());
        assert!(Cli::try_parse_from(["ulvo", "gpu", "inspect", "/fixture"]).is_ok());
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