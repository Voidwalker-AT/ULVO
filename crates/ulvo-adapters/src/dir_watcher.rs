use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use ulvo_core::strip_unc_prefix;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsWatchKind {
    Created,
    Deleted,
    Modified,
    Renamed,
    Overflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsWatchEvent {
    pub root: PathBuf,
    pub path: PathBuf,
    pub kind: FsWatchKind,
    pub old_path: Option<PathBuf>,
}

#[cfg(windows)]
enum Harvest {
    Bytes(u32),
    Incomplete,
    Failed(DWORD),
}

pub struct DirectoryWatcher {
    root: PathBuf,
    is_running: Arc<AtomicBool>,
    rx: Receiver<FsWatchEvent>,
    worker_handle: Option<JoinHandle<()>>,
    #[cfg(windows)]
    stop_event: Arc<std::sync::atomic::AtomicUsize>,
}

impl DirectoryWatcher {
    pub fn watch<P: AsRef<Path>>(root: P) -> Result<Self, String> {
        let clean_root = strip_unc_prefix(root.as_ref());
        if !clean_root.is_dir() {
            return Err(format!("watch target is not a directory: {}", clean_root.display()));
        }

        let is_running = Arc::new(AtomicBool::new(true));
        let (tx, rx) = channel::<FsWatchEvent>();

        #[cfg(windows)]
        {
            // Create a manual-reset event for stop signaling
            let stop_event_handle = unsafe { CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null()) };
            if stop_event_handle == std::ptr::null_mut() {
                let err = unsafe { GetLastError() };
                return Err(format!("CreateEventW failed for watcher stop event: {err}"));
            }

            let stop_event = Arc::new(std::sync::atomic::AtomicUsize::new(stop_event_handle as usize));
            let worker_root = clean_root.clone();
            let worker_running = is_running.clone();
            let worker_stop_event = stop_event.clone();

            let handle = thread::Builder::new()
                .name(format!(
                    "ulvo-watcher-{}",
                    clean_root.file_name().unwrap_or_default().to_string_lossy()
                ))
                .spawn(move || {
                    Self::windows_watch_loop(worker_root, worker_running, worker_stop_event, tx);
                })
                .map_err(|e| format!("cannot spawn directory watcher worker thread: {e}"))?;

            Ok(Self {
                root: clean_root,
                is_running,
                rx,
                worker_handle: Some(handle),
                stop_event,
            })
        }

        #[cfg(not(windows))]
        {
            let worker_root = clean_root.clone();
            let worker_running = is_running.clone();

            let handle = thread::Builder::new()
                .name("ulvo-watcher-fallback".into())
                .spawn(move || {
                    Self::fallback_watch_loop(worker_root, worker_running, tx);
                })
                .map_err(|e| format!("cannot spawn fallback watcher thread: {e}"))?;

            Ok(Self {
                root: clean_root,
                is_running,
                rx,
                worker_handle: Some(handle),
            })
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn receiver(&self) -> &Receiver<FsWatchEvent> {
        &self.rx
    }

    /// Signals the worker to stop, waits for it to exit, and releases the stop event.
    ///
    /// Bounded by the worker's own guarantees: the worker wakes on the stop event
    /// (or, worst case, on its finite wait slice), cancels any outstanding I/O with
    /// a deadline-bounded drain, and exits. Join never waits indefinitely.
    pub fn stop(&mut self) {
        if self.is_running.swap(false, Ordering::SeqCst) {
            #[cfg(windows)]
            {
                let raw_stop = self.stop_event.load(Ordering::SeqCst);
                if raw_stop != 0 {
                    unsafe {
                        SetEvent(raw_stop as HANDLE);
                    }
                }
            }

            if let Some(h) = self.worker_handle.take() {
                let _ = h.join();
            }

            #[cfg(windows)]
            {
                let raw_stop = self.stop_event.swap(0, Ordering::SeqCst);
                if raw_stop != 0 {
                    unsafe {
                        CloseHandle(raw_stop as HANDLE);
                    }
                }
            }
        }
    }

    fn should_ignore_rel_path(rel: &Path) -> bool {
        let ignored_names: &[&str] = &[
            ".git", ".venv", "venv", "node_modules", "target", "build", "dist",
            "out", "bin", "obj", ".idea", ".vscode", "tmp", "temp", "__pycache__",
            ".gradle", ".pytest_cache", ".ruff_cache", "site-packages", "wheels", "gems",
            "vault", "cache", "caches",
        ];

        for comp in rel.components() {
            let s = comp.as_os_str().to_string_lossy().to_lowercase();
            if ignored_names.contains(&s.as_ref()) {
                return true;
            }
            // Ignore hidden files except .ulvo
            if s.starts_with('.') && s != ".ulvo" {
                return true;
            }
        }
        false
    }

    /// Bounded wait slice for the watch loop. Even if the stop event were never
    /// observed, the worker re-checks `is_running` after every slice, which keeps
    /// shutdown hard-bounded instead of parked forever in an infinite wait.
    #[cfg(windows)]
    const WAIT_SLICE_MS: DWORD = 250;
    /// Slice used while draining a cancelled overlapped operation.
    #[cfg(windows)]
    const DRAIN_SLICE_MS: DWORD = 50;
    /// Overall deadline for draining a cancelled overlapped operation.
    /// CancelIoEx on ReadDirectoryChangesW completes near-instantly in practice;
    /// this ceiling exists so shutdown can never wait indefinitely.
    #[cfg(windows)]
    const DRAIN_DEADLINE_MS: u64 = 1500;
    /// Backoff after a transient ReadDirectoryChangesW failure.
    #[cfg(windows)]
    const ERROR_RETRY_SLEEP_MS: u64 = 50;

    #[cfg(windows)]
    fn harvest(dir_handle: HANDLE, overlapped: &mut OVERLAPPED) -> Harvest {
        let mut transferred: DWORD = 0;
        let ok = unsafe {
            GetOverlappedResult(
                dir_handle,
                overlapped as *mut OVERLAPPED as LPVOID,
                &mut transferred,
                0,
            )
        };
        if ok != 0 {
            Harvest::Bytes(transferred)
        } else {
            let err = unsafe { GetLastError() };
            if err == ERROR_IO_INCOMPLETE {
                Harvest::Incomplete
            } else {
                Harvest::Failed(err)
            }
        }
    }

    /// Cancels a pending ReadDirectoryChangesW and waits (bounded) for the
    /// completion to actually land so the OVERLAPPED buffer can be reused or
    /// freed safely. Returns true once the operation has completed.
    #[cfg(windows)]
    fn cancel_and_drain(dir_handle: HANDLE, io_event: HANDLE, overlapped: &mut OVERLAPPED) -> bool {
        let deadline = Instant::now() + Duration::from_millis(Self::DRAIN_DEADLINE_MS);
        unsafe {
            CancelIoEx(dir_handle, overlapped as *mut OVERLAPPED as LPVOID);
        }
        loop {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let remaining_ms = (deadline - now).as_millis() as u32;
            let slice = remaining_ms.clamp(1, Self::DRAIN_SLICE_MS);
            let res = unsafe { WaitForSingleObject(io_event, slice) };
            if res == WAIT_OBJECT_0 {
                // Reap the completion so the kernel is finished with our memory.
                let mut transferred: DWORD = 0;
                unsafe {
                    GetOverlappedResult(
                        dir_handle,
                        overlapped as *mut OVERLAPPED as LPVOID,
                        &mut transferred,
                        0,
                    );
                }
                return true;
            }
            if res != WAIT_TIMEOUT {
                return false;
            }
        }
    }

    #[cfg(windows)]
    fn send_overflow(root: &Path, tx: &Sender<FsWatchEvent>) {
        let _ = tx.send(FsWatchEvent {
            root: root.to_path_buf(),
            path: root.to_path_buf(),
            kind: FsWatchKind::Overflow,
            old_path: None,
        });
    }

    /// Parses a FILE_NOTIFY_INFORMATION buffer and forwards individual events.
    #[cfg(windows)]
    fn dispatch_notify_buffer(
        root: &Path,
        buffer: &[u8],
        transferred: usize,
        tx: &Sender<FsWatchEvent>,
        rename_old_name: &mut Option<PathBuf>,
    ) {
        let mut offset = 0usize;
        loop {
            if transferred > buffer.len() || offset + 12 > transferred {
                break;
            }

            // The OS byte buffer has no Rust alignment guarantee. Decode the
            // fixed 12-byte header and UTF-16 payload without typed pointer casts.
            let read_u32 = |at| u32::from_le_bytes(buffer[at..at + 4].try_into().unwrap());
            let next_entry_offset = read_u32(offset) as usize;
            let action = read_u32(offset + 4);
            let filename_len_bytes = read_u32(offset + 8) as usize;

            if filename_len_bytes > 0 && filename_len_bytes % 2 == 0 && filename_len_bytes <= transferred - offset - 12 {
                let name: Vec<u16> = buffer[offset + 12..offset + 12 + filename_len_bytes]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                let rel_str = String::from_utf16_lossy(&name);
                let rel_path = PathBuf::from(rel_str);

                if !Self::should_ignore_rel_path(&rel_path) {
                    let full_path = root.join(&rel_path);

                    match action {
                        FILE_ACTION_ADDED => {
                            let _ = tx.send(FsWatchEvent {
                                root: root.to_path_buf(),
                                path: full_path,
                                kind: FsWatchKind::Created,
                                old_path: None,
                            });
                        }
                        FILE_ACTION_REMOVED => {
                            let _ = tx.send(FsWatchEvent {
                                root: root.to_path_buf(),
                                path: full_path,
                                kind: FsWatchKind::Deleted,
                                old_path: None,
                            });
                        }
                        FILE_ACTION_MODIFIED => {
                            let _ = tx.send(FsWatchEvent {
                                root: root.to_path_buf(),
                                path: full_path,
                                kind: FsWatchKind::Modified,
                                old_path: None,
                            });
                        }
                        FILE_ACTION_RENAMED_OLD_NAME => {
                            *rename_old_name = Some(full_path);
                        }
                        FILE_ACTION_RENAMED_NEW_NAME => {
                            let old = rename_old_name.take();
                            let _ = tx.send(FsWatchEvent {
                                root: root.to_path_buf(),
                                path: full_path,
                                kind: FsWatchKind::Renamed,
                                old_path: old,
                            });
                        }
                        _ => {}
                    }
                }
            }

            if next_entry_offset == 0 {
                break;
            }
            offset += next_entry_offset;
        }
    }

    #[cfg(windows)]
    fn windows_watch_loop(
        root: PathBuf,
        is_running: Arc<AtomicBool>,
        stop_event: Arc<std::sync::atomic::AtomicUsize>,
        tx: Sender<FsWatchEvent>,
    ) {
        use std::os::windows::ffi::OsStrExt;

        let wide: Vec<u16> = root.as_os_str().encode_wide().chain(std::iter::once(0)).collect();

        let dir_handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
                std::ptr::null_mut(),
            )
        };

        if dir_handle == INVALID_HANDLE_VALUE {
            let err = unsafe { GetLastError() };
            eprintln!("[ULVO Watcher] CreateFileW failed on {}: err {}", root.display(), err);
            return;
        }

        // Create an event for the overlapped I/O structure
        let io_event = unsafe { CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null()) };
        if io_event == std::ptr::null_mut() {
            unsafe {
                CloseHandle(dir_handle);
            }
            return;
        }

        let mut buffer = vec![0u8; 65536];
        let mut rename_old_name: Option<PathBuf> = None;

        let notify_filter = FILE_NOTIFY_CHANGE_FILE_NAME
            | FILE_NOTIFY_CHANGE_DIR_NAME
            | FILE_NOTIFY_CHANGE_LAST_WRITE
            | FILE_NOTIFY_CHANGE_CREATION;

        let raw_stop = stop_event.load(Ordering::SeqCst) as HANDLE;

        // Heap-allocated so its address stays valid while any I/O is outstanding.
        // If a pathological drain ever times out, the box is forgotten instead of
        // freed, so a late kernel completion can never write to released memory.
        let mut overlapped: Box<OVERLAPPED> = Box::new(unsafe { std::mem::zeroed() });
        let mut io_pending = false;

        loop {
            if !is_running.load(Ordering::SeqCst) {
                break;
            }

            if !io_pending {
                *overlapped = unsafe { std::mem::zeroed() };
                overlapped.hEvent = io_event;
                unsafe {
                    ResetEvent(io_event);
                }

                let success = unsafe {
                    ReadDirectoryChangesW(
                        dir_handle,
                        buffer.as_mut_ptr() as *mut std::ffi::c_void,
                        buffer.len() as DWORD,
                        1, // watch subtree
                        notify_filter,
                        std::ptr::null_mut(), // asynchronous I/O reports bytes through OVERLAPPED
                        &mut *overlapped as *mut OVERLAPPED as LPVOID,
                        std::ptr::null_mut(),
                    )
                };

                if success != 0 {
                    // Completed synchronously: harvest immediately, no wait needed.
                    match Self::harvest(dir_handle, &mut overlapped) {
                        Harvest::Bytes(n) => {
                            if n == 0 {
                                Self::send_overflow(&root, &tx);
                            } else {
                                Self::dispatch_notify_buffer(&root, &buffer, n as usize, &tx, &mut rename_old_name);
                            }
                            continue;
                        }
                        Harvest::Incomplete => {
                            io_pending = true;
                        }
                        Harvest::Failed(err) => {
                            if err == ERROR_OPERATION_ABORTED || err == ERROR_INVALID_HANDLE {
                                break;
                            }
                            if err == ERROR_NOTIFY_ENUM_DIR {
                                Self::send_overflow(&root, &tx);
                            }
                            thread::sleep(Duration::from_millis(Self::ERROR_RETRY_SLEEP_MS));
                            continue;
                        }
                    }
                } else {
                    let last_err = unsafe { GetLastError() };
                    if last_err == ERROR_IO_PENDING {
                        io_pending = true;
                    } else if last_err == ERROR_OPERATION_ABORTED || last_err == ERROR_INVALID_HANDLE {
                        break;
                    } else {
                        if last_err == ERROR_NOTIFY_ENUM_DIR {
                            Self::send_overflow(&root, &tx);
                        }
                        thread::sleep(Duration::from_millis(Self::ERROR_RETRY_SLEEP_MS));
                        continue;
                    }
                }
            }

            // At this point I/O is outstanding (io_pending == true). Wait for
            // either the stop signal or completion — but only for a finite slice
            // so the loop always re-checks is_running and cannot park forever.
            let wait_handles = [raw_stop, io_event];
            let wait_res = unsafe {
                WaitForMultipleObjects(2, wait_handles.as_ptr(), 0, Self::WAIT_SLICE_MS)
            };

            if wait_res == WAIT_OBJECT_0 {
                // stop_event signaled: leave the loop; outstanding I/O is
                // cancelled and drained (bounded) after the loop.
                break;
            } else if wait_res == WAIT_OBJECT_0 + 1 {
                match Self::harvest(dir_handle, &mut overlapped) {
                    Harvest::Bytes(n) => {
                        io_pending = false;
                        if n == 0 {
                            // ReadDirectoryChangesW reports buffer overflow as a
                            // zero-byte completion; the notification queue was lost.
                            Self::send_overflow(&root, &tx);
                        } else {
                            Self::dispatch_notify_buffer(&root, &buffer, n as usize, &tx, &mut rename_old_name);
                        }
                    }
                    Harvest::Incomplete => {
                        // Spurious wake: still pending, keep waiting.
                    }
                    Harvest::Failed(err) => {
                        io_pending = false;
                        if err == ERROR_OPERATION_ABORTED || err == ERROR_INVALID_HANDLE {
                            break;
                        }
                        if err == ERROR_NOTIFY_ENUM_DIR {
                            Self::send_overflow(&root, &tx);
                        }
                        thread::sleep(Duration::from_millis(Self::ERROR_RETRY_SLEEP_MS));
                    }
                }
            } else if wait_res == WAIT_TIMEOUT {
                // Finite slice elapsed: loop re-checks is_running above.
                continue;
            } else {
                // WAIT_FAILED or unexpected return: treat as shutdown request.
                break;
            }
        }

        if io_pending {
            let drained = Self::cancel_and_drain(dir_handle, io_event, &mut overlapped);
            if !drained {
                // Deadline expired: the operation is (theoretically) still live.
                // Leak the box so a late completion writes into memory that
                // remains valid, then bail without closing handles twice.
                std::mem::forget(overlapped);
                // ReadDirectoryChangesW may still reference the payload buffer.
                // Preserve both allocations until the process exits.
                std::mem::forget(buffer);
                unsafe {
                    CloseHandle(io_event);
                    CloseHandle(dir_handle);
                }
                return;
            }
        }

        unsafe {
            CloseHandle(io_event);
            CloseHandle(dir_handle);
        }
    }

    #[cfg(not(windows))]
    fn fallback_watch_loop(
        _root: PathBuf,
        is_running: Arc<AtomicBool>,
        _tx: Sender<FsWatchEvent>,
    ) {
        while is_running.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(100));
        }
    }
}

impl Drop for DirectoryWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(windows)]
type HANDLE = *mut std::ffi::c_void;
#[cfg(windows)]
type DWORD = u32;
#[cfg(windows)]
type BOOL = i32;
#[cfg(windows)]
type LPCWSTR = *const u16;
#[cfg(windows)]
type LPVOID = *mut std::ffi::c_void;

#[cfg(windows)]
const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
#[cfg(windows)]
const FILE_LIST_DIRECTORY: DWORD = 0x0001;
#[cfg(windows)]
const FILE_SHARE_READ: DWORD = 1;
#[cfg(windows)]
const FILE_SHARE_WRITE: DWORD = 2;
#[cfg(windows)]
const FILE_SHARE_DELETE: DWORD = 4;
#[cfg(windows)]
const OPEN_EXISTING: DWORD = 3;
#[cfg(windows)]
const FILE_FLAG_BACKUP_SEMANTICS: DWORD = 0x02000000;
#[cfg(windows)]
const FILE_FLAG_OVERLAPPED: DWORD = 0x40000000;

#[cfg(windows)]
const FILE_ACTION_ADDED: DWORD = 1;
#[cfg(windows)]
const FILE_ACTION_REMOVED: DWORD = 2;
#[cfg(windows)]
const FILE_ACTION_MODIFIED: DWORD = 3;
#[cfg(windows)]
const FILE_ACTION_RENAMED_OLD_NAME: DWORD = 4;
#[cfg(windows)]
const FILE_ACTION_RENAMED_NEW_NAME: DWORD = 5;

#[cfg(windows)]
const FILE_NOTIFY_CHANGE_FILE_NAME: DWORD = 0x00000001;
#[cfg(windows)]
const FILE_NOTIFY_CHANGE_DIR_NAME: DWORD = 0x00000002;
#[cfg(windows)]
const FILE_NOTIFY_CHANGE_LAST_WRITE: DWORD = 0x00000010;
#[cfg(windows)]
const FILE_NOTIFY_CHANGE_CREATION: DWORD = 0x00000040;

#[cfg(windows)]
const ERROR_INVALID_HANDLE: DWORD = 6;
#[cfg(windows)]
const ERROR_OPERATION_ABORTED: DWORD = 995;
#[cfg(windows)]
const ERROR_IO_INCOMPLETE: DWORD = 996;
#[cfg(windows)]
const ERROR_IO_PENDING: DWORD = 997;
#[cfg(windows)]
const ERROR_NOTIFY_ENUM_DIR: DWORD = 1022;

#[cfg(windows)]
const WAIT_OBJECT_0: DWORD = 0;
#[cfg(windows)]
const WAIT_TIMEOUT: DWORD = 258;

#[cfg(windows)]
#[repr(C)]
#[allow(non_snake_case)]
struct OVERLAPPED {
    Internal: usize,
    InternalHigh: usize,
    Offset: DWORD,
    OffsetHigh: DWORD,
    hEvent: HANDLE,
}

#[cfg(windows)]
extern "system" {
    fn CreateFileW(
        lpFileName: LPCWSTR,
        dwDesiredAccess: DWORD,
        dwShareMode: DWORD,
        lpSecurityAttributes: LPVOID,
        dwCreationDisposition: DWORD,
        dwFlagsAndAttributes: DWORD,
        hTemplateFile: HANDLE,
    ) -> HANDLE;

    fn CreateEventW(
        lpEventAttributes: LPVOID,
        bManualReset: BOOL,
        bInitialState: BOOL,
        lpName: LPCWSTR,
    ) -> HANDLE;

    fn SetEvent(hEvent: HANDLE) -> BOOL;
    fn ResetEvent(hEvent: HANDLE) -> BOOL;

    fn WaitForSingleObject(hHandle: HANDLE, dwMilliseconds: DWORD) -> DWORD;
    fn WaitForMultipleObjects(
        nCount: DWORD,
        lpHandles: *const HANDLE,
        bWaitAll: BOOL,
        dwMilliseconds: DWORD,
    ) -> DWORD;

    fn GetOverlappedResult(
        hFile: HANDLE,
        lpOverlapped: LPVOID,
        lpNumberOfBytesTransferred: *mut DWORD,
        bWait: BOOL,
    ) -> BOOL;

    fn CancelIoEx(hFile: HANDLE, lpOverlapped: LPVOID) -> BOOL;

    fn ReadDirectoryChangesW(
        hDirectory: HANDLE,
        lpBuffer: LPVOID,
        nBufferLength: DWORD,
        bWatchSubtree: BOOL,
        dwNotifyFilter: DWORD,
        lpBytesReturned: *mut DWORD,
        lpOverlapped: LPVOID,
        lpCompletionRoutine: LPVOID,
    ) -> BOOL;

    fn CloseHandle(hObject: HANDLE) -> BOOL;
    fn GetLastError() -> DWORD;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn watcher_excludes_vault_and_dependency_caches_case_insensitively() {
        for path in ["ULVO/vault/registry/projects.json", "app/Node_Modules/pkg/package.json", "app/cache/x"] {
            assert!(DirectoryWatcher::should_ignore_rel_path(Path::new(path)));
        }
        assert!(!DirectoryWatcher::should_ignore_rel_path(Path::new("app/.ulvo/manifest.toml")));
    }

    #[cfg(windows)]
    #[test]
    fn notification_parser_accepts_short_unaligned_names_and_truncated_input() {
        let (tx, rx) = channel();
        let root = Path::new("workspace");
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&0u32.to_le_bytes());
        buffer.extend_from_slice(&FILE_ACTION_ADDED.to_le_bytes());
        buffer.extend_from_slice(&2u32.to_le_bytes());
        buffer.extend_from_slice(&(b'x' as u16).to_le_bytes());
        let mut old = None;
        DirectoryWatcher::dispatch_notify_buffer(root, &buffer, buffer.len(), &tx, &mut old);
        assert_eq!(rx.try_recv().unwrap().path, root.join("x"));
        for len in 0..buffer.len() {
            DirectoryWatcher::dispatch_notify_buffer(root, &buffer, len, &tx, &mut old);
        }
        assert!(rx.try_recv().is_err());
    }

    fn fresh_temp_dir(tag: &str) -> PathBuf {
        let temp_dir = std::env::temp_dir().join(format!("ulvo_wtest_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        temp_dir
    }

    #[test]
    fn test_directory_watcher_detects_creation_and_stops_cleanly() {
        let temp_dir = fresh_temp_dir("create");

        let mut watcher = DirectoryWatcher::watch(&temp_dir).expect("watcher started");
        assert_eq!(watcher.root(), &temp_dir);

        // Allow worker thread to initialize
        thread::sleep(Duration::from_millis(50));

        let file_path = temp_dir.join("created_manifest.toml");
        fs::write(&file_path, "name = 'test'").expect("write test file");

        let evt = watcher
            .receiver()
            .recv_timeout(Duration::from_millis(3000))
            .expect("should receive file created event");

        assert_eq!(evt.root, temp_dir);
        assert!(evt.path.ends_with("created_manifest.toml"));
        assert!(matches!(evt.kind, FsWatchKind::Created | FsWatchKind::Modified));

        let start = std::time::Instant::now();
        watcher.stop();
        let stop_elapsed = start.elapsed();
        assert!(
            stop_elapsed < Duration::from_millis(1000),
            "stop after events must complete within 1000ms, took {stop_elapsed:?}"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_directory_watcher_shutdown_without_events() {
        let temp_dir = fresh_temp_dir("no_events");

        let mut watcher = DirectoryWatcher::watch(&temp_dir).expect("watcher started");
        thread::sleep(Duration::from_millis(50));

        // Shutdown immediately without any events generated
        let start = std::time::Instant::now();
        watcher.stop();
        let elapsed = start.elapsed();

        assert!(elapsed < Duration::from_millis(500), "shutdown without events must take < 500ms, took {elapsed:?}");
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_directory_watcher_repeated_lifecycle() {
        let temp_dir = fresh_temp_dir("repeated");

        for i in 0..5 {
            let mut watcher = DirectoryWatcher::watch(&temp_dir).expect("watcher start");
            thread::sleep(Duration::from_millis(30));
            let f = temp_dir.join(format!("file_{i}.txt"));
            fs::write(&f, "content").expect("write");
            let _ = watcher.receiver().recv_timeout(Duration::from_millis(1500));
            let start = std::time::Instant::now();
            watcher.stop();
            assert!(
                start.elapsed() < Duration::from_millis(1000),
                "cycle {i} stop must be bounded"
            );
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_directory_watcher_detects_modification_and_deletion() {
        let temp_dir = fresh_temp_dir("mod_del");

        let test_file = temp_dir.join("live.txt");
        fs::write(&test_file, "initial").expect("write initial");

        let mut watcher = DirectoryWatcher::watch(&temp_dir).expect("watcher started");
        thread::sleep(Duration::from_millis(50));

        // Modify file
        fs::write(&test_file, "updated content").expect("modify file");
        let evt_mod = watcher.receiver().recv_timeout(Duration::from_millis(3000)).expect("recv mod");
        assert!(evt_mod.path.ends_with("live.txt"));

        // Delete file
        fs::remove_file(&test_file).expect("remove file");
        let mut got_delete = false;
        let deadline = std::time::Instant::now() + Duration::from_millis(3000);
        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match watcher.receiver().recv_timeout(remaining) {
                Ok(evt) if matches!(evt.kind, FsWatchKind::Deleted) && evt.path.ends_with("live.txt") => {
                    got_delete = true;
                    break;
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        assert!(got_delete, "expected a Deleted event for live.txt");

        watcher.stop();
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_directory_watcher_detects_rename() {
        let temp_dir = fresh_temp_dir("rename");

        let old_file = temp_dir.join("old_name.txt");
        fs::write(&old_file, "before rename").expect("write initial");

        let mut watcher = DirectoryWatcher::watch(&temp_dir).expect("watcher started");
        thread::sleep(Duration::from_millis(50));

        let new_file = temp_dir.join("new_name.txt");
        fs::rename(&old_file, &new_file).expect("rename file");

        let mut got_rename = false;
        let deadline = std::time::Instant::now() + Duration::from_millis(3000);
        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match watcher.receiver().recv_timeout(remaining) {
                Ok(evt) if matches!(evt.kind, FsWatchKind::Renamed) => {
                    assert_eq!(evt.path, new_file);
                    assert!(
                        evt.old_path.as_ref().map(|p| p == &old_file).unwrap_or(false),
                        "rename event should carry the old path, got {:?}",
                        evt.old_path
                    );
                    got_rename = true;
                    break;
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        assert!(got_rename, "expected a Renamed event for the file rename");

        watcher.stop();
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
