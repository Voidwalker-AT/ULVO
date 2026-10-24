use std::{fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};
use serde::{Deserialize, Serialize};
use ulvo_adapters::{bounded_read, reject_links, write_atomic, EnvironmentFileLock};

const MAX_RECORD_BYTES: usize = 32768;
const MAX_OPERATIONS: usize = 100;
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024;
const RETENTION_SECONDS: u64 = 14 * 24 * 3600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    pub schema_version: u32,
    pub operation_id: String,
    pub component: String,
    pub timestamp: u64,
    pub classification: String,
    pub message: String,
}

pub struct DiagnosticStore { root: PathBuf }
pub fn install_crash_handler(config: PathBuf, component: &'static str) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Do not persist panic payloads, stack locals, arguments or environment.
        let _ = DiagnosticStore::new(config.clone()).record(component, "CRASH", "Process panicked; runtime recovery requires a reviewed repair plan");
        previous(info);
    }));
}
pub fn redact(input: &str) -> String {
    let mut result = String::new();
    for line in input.lines().take(256) {
        let lower = line.to_ascii_lowercase();
        let secret = ["token", "secret", "password", "authorization", "credential", "api_key", "api-key", "apikey", "bearer", "ghp_", "github_pat_", "sk-", "akia", "eyj", "://"]
            .iter().any(|key| lower.contains(key));
        let safe = if secret { "[REDACTED sensitive line]".into() } else {
            let mut s = line.to_string();
            if let Some(home) = std::env::var_os("USERPROFILE") { s = s.replace(&home.to_string_lossy().to_string(), "[USER]"); }
            s.chars().filter(|c| !c.is_control() || *c == '\t').take(1024).collect::<String>()
        };
        if result.len() + safe.len() + 1 > 16000 { break; }
        result.push_str(&safe); result.push('\n');
    }
    result
}

impl DiagnosticStore {
    pub fn new(config_root: PathBuf) -> Self { Self { root: config_root.join("logs") } }
    pub fn record(&self, component: &str, classification: &str, message: &str) -> Result<String, String> {
        reject_links(&self.root)?;
        let _lock = EnvironmentFileLock::acquire(&self.root.join("logs.lock"), 1000)?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?;
        let id = format!("op_{}_{}_{}", now.as_secs(), std::process::id(), now.subsec_nanos());
        let event = DiagnosticEvent { schema_version: 1, operation_id: id.clone(), component: redact(component).trim().chars().take(40).collect(), timestamp: now.as_secs(), classification: redact(classification).trim().chars().take(40).collect(), message: redact(message) };
        let bytes = serde_json::to_vec_pretty(&event).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_RECORD_BYTES { return Err("diagnostic event exceeds record budget".into()); }
        write_atomic(&self.root.join(format!("{id}.json")), &bytes)?;
        self.rotate(now.as_secs())?;
        Ok(id)
    }
    fn rotate(&self, now: u64) -> Result<(), String> {
        let mut files = self.owned_files()?;
        files.sort_by_key(|(_, e, _)| e.timestamp);
        let mut total = files.iter().map(|(_, _, size)| size).sum::<u64>();
        let mut count = files.len();
        for (path, event, size) in files {
            if count > MAX_OPERATIONS || total > MAX_TOTAL_BYTES || now.saturating_sub(event.timestamp) > RETENTION_SECONDS {
                reject_links(&path)?;
                fs::remove_file(&path).map_err(|e| format!("diagnostic retention failed: {e}"))?;
                total = total.saturating_sub(size); count -= 1;
            }
        }
        Ok(())
    }
    fn valid_id(id: &str) -> bool { id.starts_with("op_") && id.len() <= 100 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') }
    fn owned_files(&self) -> Result<Vec<(PathBuf, DiagnosticEvent, u64)>, String> {
        let mut files = vec![];
        if !self.root.exists() { return Ok(files); }
        reject_links(&self.root)?;
        // Do not recurse, follow links, or remove files not matching our schema.
        for (index, entry) in fs::read_dir(&self.root).map_err(|e| e.to_string())?.take(1001).enumerate() {
            if index == 1000 { return Err("diagnostic directory entry limit exceeded; retain files for manual review".into()); }
            let path = entry.map_err(|e| e.to_string())?.path();
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if path.extension().and_then(|s| s.to_str()) != Some("json") || !Self::valid_id(id) || reject_links(&path).is_err() { continue; }
            let size = fs::metadata(&path).map_err(|e| e.to_string())?.len();
            if size > MAX_RECORD_BYTES as u64 { continue; }
            if let Ok(event) = serde_json::from_str::<DiagnosticEvent>(&bounded_read(&path)?) {
                if event.schema_version == 1 && event.operation_id == id { files.push((path, event, size)); }
            }
        }
        Ok(files)
    }
    pub fn list(&self) -> Result<Vec<DiagnosticEvent>, String> {
        let mut events = self.owned_files()?.into_iter().map(|(_, e, _)| e).collect::<Vec<_>>();
        events.sort_by_key(|e| std::cmp::Reverse(e.timestamp)); Ok(events)
    }
    pub fn show(&self, id: &str) -> Result<DiagnosticEvent, String> {
        if !Self::valid_id(id) { return Err("invalid operation ID".into()); }
        self.list()?.into_iter().find(|e| e.operation_id == id).ok_or("operation not found".into())
    }
    pub fn export(&self, destination: &std::path::Path) -> Result<(), String> {
        if !destination.is_absolute() || destination.exists() { return Err("export requires a new absolute destination file".into()); }
        reject_links(destination)?;
        let events = self.list()?.into_iter().map(|mut e| { e.message = redact(&e.message); e }).collect::<Vec<_>>();
        write_atomic(destination, &serde_json::to_vec_pretty(&events).map_err(|e| e.to_string())?)
    }

    /// Explicit, noninteractive native-tool capture. Arguments are never logged.
    pub fn capture(&self, resolver: &ulvo_adapters::GatewayResolver<'_>, tool: &str, args: &[String], timeout_seconds: u64) -> Result<(String, i32), String> {
        use std::{io::Read, process::Stdio, time::{Duration, Instant}};
        if !["uv", "pip", "npm", "npx", "pnpm", "cargo", "rustup", "go", "gem", "mvn", "gradle", "pacman"].contains(&tool) {
            return Err("unsupported native diagnostic coordinator".into());
        }
        if !(1..=300).contains(&timeout_seconds) { return Err("capture timeout must be 1..300 seconds".into()); }
        let mut command = resolver.build_command(tool, args)?;
        command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let supervisor = ulvo_adapters::JobSupervisor::new()?;
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        if let Err(e) = supervisor.assign_process(&child) { let _ = child.kill(); let _ = child.wait(); return Err(e); }
        fn drain<R: Read + Send + 'static>(mut pipe: R) -> std::sync::mpsc::Receiver<Vec<u8>> {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || { let mut data = Vec::new(); let mut buf = [0u8; 4096]; while let Ok(n) = pipe.read(&mut buf) { if n == 0 { break; } let keep = n.min(65536usize.saturating_sub(data.len())); data.extend_from_slice(&buf[..keep]); } let _ = tx.send(data); }); rx
        }
        let stdout = drain(child.stdout.take().unwrap());
        let stderr = drain(child.stderr.take().unwrap());
        let started = Instant::now();
        let code = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? { break status.code().unwrap_or(1); }
            if started.elapsed() >= Duration::from_secs(timeout_seconds) { let _ = child.kill(); let _ = child.wait(); break 124; }
            std::thread::sleep(Duration::from_millis(20));
        };
        drop(supervisor); // terminate descendants before waiting for pipe EOF
        let out = stdout.recv_timeout(Duration::from_secs(1)).unwrap_or_default();
        let err = stderr.recv_timeout(Duration::from_secs(1)).unwrap_or_default();
        let message = format!("Native coordinator {tool}; exit={code}; output capped at 64 KiB per stream\n{}\n{}", String::from_utf8_lossy(&out), String::from_utf8_lossy(&err));
        let id = self.record("native_tool", if code == 0 { "VERIFIED" } else if code == 124 { "TIMEOUT" } else { "FAILED" }, &message)?;
        Ok((id, code))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_and_url_credentials_are_not_exported() {
        let s = redact("Authorization: Bearer xyz\n--password=example\nhttps://user:pass@host/path?key=x\nregular failure");
        assert!(!s.contains("xyz")); assert!(!s.contains("example")); assert!(!s.contains("user:pass")); assert!(s.contains("regular failure"));
    }
    #[test]
    fn budgets_export_and_unowned_file_preservation() {
        let root = std::env::temp_dir().join(format!("ulvo_diagnostics_{}", std::process::id()));
        let store = DiagnosticStore::new(root.clone());
        fs::create_dir_all(&store.root).unwrap(); fs::write(store.root.join("user.txt"), "retain").unwrap();
        for _ in 0..105 { store.record("test", "FAILED", "bounded failure").unwrap(); }
        assert_eq!(store.list().unwrap().len(), 100);
        assert!(store.root.join("user.txt").is_file());
        assert!(store.show("../user").is_err());
        store.export(&root.join("export.json")).unwrap();
        assert!(store.export(&root.join("export.json")).is_err());
    }
}
