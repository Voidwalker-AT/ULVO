use std::{fs, io::{Read, Write}, net::{TcpListener, TcpStream}, path::PathBuf, process::{Child, Command}, time::{Duration, Instant}};
use ulvo_services::AgentIpcServer;

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
}

#[test]
fn agent_child() {
    let Some(root) = std::env::var_os("ULVO_TRANSPORT_TEST_ROOT") else { return };
    let port = std::env::var("ULVO_TRANSPORT_TEST_PORT").unwrap().parse().unwrap();
    AgentIpcServer::new(PathBuf::from(root)).serve(port).unwrap();
}

#[test]
fn idle_client_does_not_block_status_and_shutdown() {
    let root = std::env::temp_dir().join(format!("ulvo_transport_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut child = ChildGuard(Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "agent_child", "--test-threads=1"])
        .env("ULVO_TRANSPORT_TEST_ROOT", &root)
        .env("ULVO_TRANSPORT_TEST_PORT", port.to_string())
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.join("agent.port").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(root.join("agent.port").exists(), "agent startup deadline exceeded");
    let mut duplicate = ChildGuard(Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "agent_child", "--test-threads=1"])
        .env("ULVO_TRANSPORT_TEST_ROOT", &root)
        .env("ULVO_TRANSPORT_TEST_PORT", "0")
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = duplicate.0.try_wait().unwrap() { assert!(!status.success(), "a second agent must be rejected even on another port"); break; }
        assert!(Instant::now() < deadline, "second agent did not reject startup");
        std::thread::sleep(Duration::from_millis(20));
    }
    let _idle = TcpStream::connect(("127.0.0.1", port)).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    let token = fs::read_to_string(root.join("agent.token")).unwrap();
    let mut ping = TcpStream::connect(("127.0.0.1", port)).unwrap();
    ping.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let request = serde_json::json!({"auth_token": token, "action": {"action": "ping"}});
    writeln!(ping, "{request}").unwrap();
    let mut bytes = Vec::new();
    ping.read_to_end(&mut bytes).expect("an idle connection must not indefinitely block the agent");
    let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(response["success"], true);
    let mut fragmented = TcpStream::connect(("127.0.0.1", port)).unwrap();
    fragmented.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let request = format!("{request}\n");
    let mid = request.len() / 2;
    fragmented.write_all(&request.as_bytes()[..mid]).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    fragmented.write_all(&request.as_bytes()[mid..]).unwrap();
    bytes.clear();
    fragmented.read_to_end(&mut bytes).unwrap();
    let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(response["success"], true, "fragmented TCP requests must be reassembled");
    assert!(AgentIpcServer::new(&root).send_request(ulvo_services::AgentAction::Shutdown).unwrap().success);
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if let Some(status) = child.0.try_wait().unwrap() { assert!(status.success()); return; }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("graceful shutdown deadline exceeded");
}
