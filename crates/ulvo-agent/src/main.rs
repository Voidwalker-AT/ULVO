use std::path::PathBuf;
use clap::{Parser, Subcommand};
use ulvo_services::{AgentAction, AgentIpcServer, UlvoServices, DEFAULT_AGENT_PORT};

#[derive(Parser, Debug)]
#[command(name = "ulvo-agent", version = "1.2.0", about = "ULVO Background Reconciliation & Governance Agent")]
struct Cli {
    #[arg(long, global = true, default_value = "E:\\Workspace\\Active\\ULVO\\vault", help = "Path to ULVO vault root")]
    vault: PathBuf,

    #[command(subcommand)]
    command: Option<AgentCommands>,
}

#[derive(Subcommand, Debug)]
enum AgentCommands {
    /// Start the background agent server
    Run {
        #[arg(long, default_value_t = DEFAULT_AGENT_PORT, help = "IPC TCP port")]
        port: u16,
    },
    /// Query the status of the running background agent
    Status,
    /// Trigger an immediate lease and storage reconciliation cycle
    Reconcile,
    /// Send graceful shutdown signal to running agent
    Stop,
}

fn main() {
    let cli = Cli::parse();
    let _services = UlvoServices::new(&cli.vault);

    match cli.command.unwrap_or(AgentCommands::Run { port: DEFAULT_AGENT_PORT }) {
        AgentCommands::Run { port } => {
            println!("Starting ULVO Background Agent on port {port} (Vault: {})", cli.vault.display());
            let server = AgentIpcServer::new(&cli.vault);
            if let Err(e) = server.serve(port) {
                eprintln!("Agent server error: {e}");
                std::process::exit(1);
            }
        }
        AgentCommands::Status => {
            let server = AgentIpcServer::new(&cli.vault);
            match server.send_request(AgentAction::Status) {
                Ok(resp) => {
                    println!("ULVO Agent Status: {}", if resp.success { "OPERATIONAL" } else { "DEGRADED" });
                    println!("Message: {}", resp.message);
                    if let Some(data) = resp.data {
                        println!("Data: {}", serde_json::to_string_pretty(&data).unwrap_or_default());
                    }
                }
                Err(e) => {
                    eprintln!("Cannot connect to agent: {e}");
                    std::process::exit(1);
                }
            }
        }
        AgentCommands::Reconcile => {
            let server = AgentIpcServer::new(&cli.vault);
            match server.send_request(AgentAction::ReconcileLeases) {
                Ok(resp) => {
                    println!("Reconciliation triggered: {}", resp.message);
                    if let Some(data) = resp.data {
                        println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
                    }
                }
                Err(e) => {
                    eprintln!("Cannot communicate with agent: {e}");
                    std::process::exit(1);
                }
            }
        }
        AgentCommands::Stop => {
            let server = AgentIpcServer::new(&cli.vault);
            match server.send_request(AgentAction::Shutdown) {
                Ok(resp) => {
                    println!("Shutdown response: {}", resp.message);
                }
                Err(e) => {
                    eprintln!("Failed to signal shutdown to agent: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}
