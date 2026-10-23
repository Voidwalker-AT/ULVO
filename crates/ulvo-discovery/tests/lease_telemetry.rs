use std::fs;
use ulvo_discovery::project_lifecycle::ProjectOrchestrator;

#[test]
fn lease_status_counts_live_pids_and_rejects_corrupt_metadata() {
    let root = std::env::temp_dir().join(format!("ulvo_lease_telemetry_{}", std::process::id()));
    let env = root.join("environments/python/env_status");
    fs::create_dir_all(&env).unwrap();
    let orch = ProjectOrchestrator::new(root).unwrap();
    assert_eq!(orch.active_leased_environment_count().unwrap(), 0);
    let lease = serde_json::json!({
        "lease_id": "status", "environment_id": "env_status", "project_path": "fixture",
        "environment_path": env, "state": "EXECUTING", "active_pids": [std::process::id()],
        "leased_processes": [], "created_at_unix": 0, "updated_at_unix": 0,
        "lease_timeout_seconds": 60, "owner_token": "synthetic-test-token", "python_version": null
    });
    let file = env.join(".ulvo_lease.json");
    fs::write(&file, serde_json::to_vec(&lease).unwrap()).unwrap();
    assert_eq!(orch.active_leased_environment_count().unwrap(), 1);
    fs::write(file, b"{interrupted").unwrap();
    assert!(orch.active_leased_environment_count().is_err());
}
