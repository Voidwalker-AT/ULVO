use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use ulvo_discovery::project_lifecycle::ProjectOrchestrator;
use crate::catalog::CatalogService;
use crate::storage::StorageService;

pub const DEFAULT_AGENT_PORT: u16 = 48590;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", content = "payload", rename_all = "snake_case")]
pub enum AgentAction {
    Ping,
    Status,
    ReconcileLeases,
    EnforceCacheBudget { max_gb: Option<f64> },
    CheckUpdates,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    pub auth_token: String,
    pub action: AgentAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatusInfo {
    pub is_running: bool,
    pub pid: u32,
    pub port: u16,
    pub uptime_seconds: u64,
    pub last_reconciliation: Option<u64>,
    pub active_leases: usize,
}

pub struct AgentIpcServer {
    vault_root: PathBuf,
    token_path: PathBuf,
    port_path: PathBuf,
    is_terminating: Arc<AtomicBool>,
}

impl AgentIpcServer {
    pub fn new<P: AsRef<Path>>(vault_root: P) -> Self {
        let vr = vault_root.as_ref().to_path_buf();
        let token_path = vr.join("agent.token");
        let port_path = vr.join("agent.port");
        Self {
            vault_root: vr,
            token_path,
            port_path,
            is_terminating: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Generates or retrieves the local IPC authentication secret token.
    pub fn get_or_create_auth_token(&self) -> Result<String, String> {
        if self.token_path.is_file() {
            if let Ok(content) = fs::read_to_string(&self.token_path) {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    return Ok(trimmed.to_string());
                }
            }
        }

        let token = format!(
            "ulvo_sec_{}_{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        );

        if let Some(parent) = self.token_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        fs::write(&self.token_path, &token)
            .map_err(|e| format!("cannot write agent authentication token: {e}"))?;

        Ok(token)
    }

    /// Sends a typed IPC request to the running local agent daemon and returns the response.
    pub fn send_request(&self, action: AgentAction) -> Result<AgentResponse, String> {
        let token = self.get_or_create_auth_token()?;
        let port = if self.port_path.is_file() {
            fs::read_to_string(&self.port_path)
                .ok()
                .and_then(|p| p.trim().parse::<u16>().ok())
                .unwrap_or(DEFAULT_AGENT_PORT)
        } else {
            DEFAULT_AGENT_PORT
        };

        let req = AgentRequest {
            auth_token: token,
            action,
        };

        let req_json = serde_json::to_string(&req)
            .map_err(|e| format!("cannot serialize request: {e}"))?;

        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}"))
            .map_err(|e| format!("ULVO background agent is not running on 127.0.0.1:{port} ({e})"))?;

        // Write length-prefixed or newline-terminated message
        let payload = format!("{req_json}\n");
        stream.write_all(payload.as_bytes())
            .map_err(|e| format!("cannot send IPC message to agent: {e}"))?;

        let mut resp_buf = Vec::new();
        stream.read_to_end(&mut resp_buf)
            .map_err(|e| format!("cannot read IPC response from agent: {e}"))?;

        let resp: AgentResponse = serde_json::from_slice(&resp_buf)
            .map_err(|e| format!("invalid IPC response payload: {e}"))?;

        Ok(resp)
    }

    /// Runs a single reconciliation cycle (dead lease recovery, abandoned envs, crash recovery).
    pub fn run_reconciliation_cycle(&self) -> Result<serde_json::Value, String> {
        let orchestrator = ProjectOrchestrator::new(self.vault_root.clone())?;
        let reclaimed_abandoned = orchestrator.scan_and_reclaim_abandoned_environments()?;
        let inactive_pruned = orchestrator.reclaim_all_inactive_environments(false)?;

        Ok(serde_json::json!({
            "reclaimed_abandoned": reclaimed_abandoned,
            "inactive_pruned": inactive_pruned.reclaimed_count,
            "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
        }))
    }

    /// Starts the background listener loop on localhost.
    pub fn serve(&self, port: u16) -> Result<(), String> {
        let token = self.get_or_create_auth_token()?;
        let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
            .map_err(|e| format!("cannot bind agent IPC listener on 127.0.0.1:{port}: {e}"))?;

        let _ = fs::write(&self.port_path, port.to_string());
        println!("ULVO Background Agent listening on 127.0.0.1:{port}");

        let terminating = self.is_terminating.clone();

        for stream_res in listener.incoming() {
            if terminating.load(Ordering::SeqCst) {
                break;
            }

            if let Ok(mut stream) = stream_res {
                let mut buf = [0u8; 8192];
                if let Ok(n) = stream.read(&mut buf) {
                    if n > 0 {
                        let req_str = String::from_utf8_lossy(&buf[..n]);
                        let resp = if let Ok(req) = serde_json::from_str::<AgentRequest>(req_str.trim()) {
                            if req.auth_token != token {
                                AgentResponse {
                                    success: false,
                                    message: "Authentication failure: invalid token".into(),
                                    data: None,
                                }
                            } else {
                                self.handle_action(req.action)
                            }
                        } else {
                            AgentResponse {
                                success: false,
                                message: "Malformed IPC request payload".into(),
                                data: None,
                            }
                        };

                        if let Ok(serialized) = serde_json::to_vec(&resp) {
                            let _ = stream.write_all(&serialized);
                        }

                        if terminating.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                }
            }
        }

        let _ = fs::remove_file(&self.port_path);
        Ok(())
    }

    fn handle_action(&self, action: AgentAction) -> AgentResponse {
        match action {
            AgentAction::Ping => AgentResponse {
                success: true,
                message: "pong".into(),
                data: Some(serde_json::json!({ "pid": std::process::id() })),
            },
            AgentAction::Status => {
                let active_count = if let Ok(orchestrator) = ProjectOrchestrator::new(self.vault_root.clone()) {
                    orchestrator.audit_vault_environments().map(|a| a.active_environments).unwrap_or(0)
                } else {
                    0
                };
                AgentResponse {
                    success: true,
                    message: "Agent operational".into(),
                    data: Some(serde_json::json!({
                        "pid": std::process::id(),
                        "vault_root": self.vault_root.to_string_lossy(),
                        "active_leases": active_count,
                    })),
                }
            }
            AgentAction::ReconcileLeases => match self.run_reconciliation_cycle() {
                Ok(data) => AgentResponse {
                    success: true,
                    message: "Reconciliation complete".into(),
                    data: Some(data),
                },
                Err(e) => AgentResponse {
                    success: false,
                    message: format!("Reconciliation failed: {e}"),
                    data: None,
                },
            },
            AgentAction::EnforceCacheBudget { max_gb } => {
                let storage_svc = StorageService::new(&self.vault_root);
                match storage_svc.gc_cache(max_gb, false) {
                    Ok(rep) => AgentResponse {
                        success: true,
                        message: format!("Cache budget enforced (reclaimed {} bytes)", rep.physical_bytes_reclaimed),
                        data: Some(serde_json::to_value(&rep).unwrap_or_default()),
                    },
                    Err(e) => AgentResponse {
                        success: false,
                        message: format!("Cache enforcement failed: {e}"),
                        data: None,
                    },
                }
            }
            AgentAction::CheckUpdates => {
                let cat_svc = CatalogService::new(&self.vault_root);
                let entries = cat_svc.list_catalog();
                AgentResponse {
                    success: true,
                    message: format!("Found {} catalog entries", entries.len()),
                    data: Some(serde_json::to_value(&entries).unwrap_or_default()),
                }
            }
            AgentAction::Shutdown => {
                self.is_terminating.store(true, Ordering::SeqCst);
                AgentResponse {
                    success: true,
                    message: "Shutting down background agent gracefully".into(),
                    data: None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_token_generation_and_persistence() {
        let temp_dir = std::env::temp_dir().join("ulvo_test_agent_token");
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(&temp_dir);

        let server = AgentIpcServer::new(&temp_dir);
        let t1 = server.get_or_create_auth_token().unwrap();
        assert!(t1.starts_with("ulvo_sec_"));

        // Second call returns identical token
        let t2 = server.get_or_create_auth_token().unwrap();
        assert_eq!(t1, t2);
    }
}
