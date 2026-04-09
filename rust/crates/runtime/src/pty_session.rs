use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize, PtySystem};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::{fmt, io};

const RING_BUFFER_MAX: usize = 65_536;
const READER_BUFFER_SIZE: usize = 8192;

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

/// Status of a PTY session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PtyStatus {
    Running,
    Exited(i32),
}

/// A single PTY session representation.
#[derive(Debug, Clone)]
pub struct PtySession {
    pub id: String,
    pub command: String,
    pub cwd: std::path::PathBuf,
    pub pid: Option<u32>,
    pub status: PtyStatus,
}

/// Errors that can occur during PTY operations.
#[derive(Debug)]
pub enum PtyError {
    SessionNotFound(String),
    PtyError(String),
    IoError(io::Error),
}

impl fmt::Display for PtyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SessionNotFound(id) => write!(f, "PTY session not found: {id}"),
            Self::PtyError(msg) => write!(f, "PTY error: {msg}"),
            Self::IoError(err) => write!(f, "IO error: {err}"),
        }
    }
}

impl std::error::Error for PtyError {}

impl From<io::Error> for PtyError {
    fn from(err: io::Error) -> Self {
        Self::IoError(err)
    }
}

impl From<anyhow::Error> for PtyError {
    fn from(err: anyhow::Error) -> Self {
        Self::PtyError(err.to_string())
    }
}

type PtyResult<T> = Result<T, PtyError>;

/// Internal state for an active PTY session.
struct PtySessionInner {
    master: Box<dyn MasterPty>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send>,
    output_ring: Arc<Mutex<std::collections::VecDeque<u8>>>,
    reader_thread: Option<std::thread::JoinHandle<()>>,
    reader_stop: Arc<AtomicBool>,
}

/// Manages multiple PTY sessions.
pub struct PtyManager {
    pty_system: Box<dyn PtySystem + Send>,
    sessions: HashMap<String, PtySessionInner>,
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyManager {
    /// Create a new PTY manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pty_system: native_pty_system(),
            sessions: HashMap::new(),
        }
    }

    /// Create a new PTY session, spawning the given program.
    ///
    /// Returns the session ID on success.
    pub fn create_pty(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: &Path,
        rows: u16,
        cols: u16,
    ) -> PtyResult<String> {
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = self
            .pty_system
            .openpty(size)
            .map_err(|e| PtyError::PtyError(e.to_string()))?;

        let mut cmd = CommandBuilder::new(program);
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        for arg in args {
            cmd.arg(arg);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::PtyError(e.to_string()))?;

        // CRITICAL: drop slave immediately — required for Windows ConPTY and Unix EOF.
        drop(pair.slave);

        let output_ring: Arc<Mutex<std::collections::VecDeque<u8>>> = Arc::new(Mutex::new(
            std::collections::VecDeque::with_capacity(RING_BUFFER_MAX),
        ));
        let reader_stop = Arc::new(AtomicBool::new(false));

        let ring_for_reader = Arc::clone(&output_ring);
        let stop_for_reader = Arc::clone(&reader_stop);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| PtyError::PtyError(e.to_string()))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| PtyError::PtyError(e.to_string()))?;

        let reader_thread = std::thread::spawn(move || {
            let mut buf = [0u8; READER_BUFFER_SIZE];
            loop {
                if stop_for_reader.load(Ordering::Acquire) {
                    break;
                }
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut ring = ring_for_reader.lock().unwrap_or_else(|e| e.into_inner());
                        for &byte in &buf[..n] {
                            if ring.len() >= RING_BUFFER_MAX {
                                ring.pop_front();
                            }
                            ring.push_back(byte);
                        }
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
        });

        let session_id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed).to_string();

        self.sessions.insert(
            session_id.clone(),
            PtySessionInner {
                master: pair.master,
                writer,
                child,
                output_ring,
                reader_thread: Some(reader_thread),
                reader_stop,
            },
        );

        Ok(session_id)
    }

    /// Write data to an existing PTY session.
    pub fn write_to_pty(&mut self, session_id: &str, data: &[u8]) -> PtyResult<()> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| PtyError::SessionNotFound(session_id.to_string()))?;

        session.writer.write_all(data).map_err(PtyError::IoError)?;
        session.writer.flush().map_err(PtyError::IoError)?;

        Ok(())
    }

    /// Resize the terminal dimensions of an existing PTY session.
    pub fn resize_pty(&mut self, session_id: &str, rows: u16, cols: u16) -> PtyResult<()> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| PtyError::SessionNotFound(session_id.to_string()))?;

        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        session
            .master
            .resize(size)
            .map_err(|e| PtyError::PtyError(e.to_string()))
    }

    /// Close a PTY session, killing the child and waiting for the reader thread.
    ///
    /// Returns the exit code if the child has exited, or `None` if the exit
    /// code could not be determined.
    pub fn close_pty(&mut self, session_id: &str) -> PtyResult<Option<i32>> {
        let mut session = self
            .sessions
            .remove(session_id)
            .ok_or_else(|| PtyError::SessionNotFound(session_id.to_string()))?;

        session.reader_stop.store(true, Ordering::Release);
        let _ = session.child.kill();

        if let Some(handle) = session.reader_thread.take() {
            // Give the reader thread a brief window to observe the stop flag
            // and exit. If it's blocked on I/O, we detach rather than hang.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                if handle.is_finished() {
                    let _ = handle.join();
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            // If the thread didn't exit in time, detach it. It will eventually
            // terminate when the PTY master is dropped (which happens when
            // PtySessionInner is dropped).
        }

        let exit_code = match session.child.wait() {
            Ok(status) => Some(status.exit_code().cast_signed()),
            Err(_) => Some(-1),
        };

        Ok(exit_code)
    }

    /// Drain all available output from the PTY session's ring buffer.
    ///
    /// Clears the buffer after reading.
    pub fn drain_output(&mut self, session_id: &str) -> PtyResult<Vec<u8>> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| PtyError::SessionNotFound(session_id.to_string()))?;

        let mut ring = session
            .output_ring
            .lock()
            .map_err(|e| PtyError::PtyError(format!("Failed to lock output ring: {e}")))?;

        let output: Vec<u8> = ring.drain(..).collect();
        Ok(output)
    }

    /// Get the current status of a PTY session.
    pub fn get_status(&mut self, session_id: &str) -> PtyResult<PtySession> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| PtyError::SessionNotFound(session_id.to_string()))?;

        let status = match session.child.try_wait() {
            Ok(Some(exit_status)) => PtyStatus::Exited(exit_status.exit_code().cast_signed()),
            Ok(None) => PtyStatus::Running,
            Err(_) => PtyStatus::Exited(-1),
        };

        let pid = session.child.process_id();

        Ok(PtySession {
            id: session_id.to_string(),
            command: String::new(),
            cwd: std::path::PathBuf::new(),
            pid,
            status,
        })
    }

    /// List all active session IDs.
    #[must_use]
    pub fn list_sessions(&self) -> Vec<String> {
        self.sessions.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    #[cfg_attr(not(feature = "pty-tests"), ignore = "requires PTY support")]
    fn test_create_pty_spawns_bash() {
        let mut manager = PtyManager::new();
        let session_id = manager
            .create_pty("bash", &[], Path::new("/tmp"), 24, 80)
            .expect("create_pty should succeed");

        let status = manager
            .get_status(&session_id)
            .expect("get_status should succeed");
        assert_eq!(status.status, PtyStatus::Running);
        assert!(status.pid.is_some());

        let exit_code = manager
            .close_pty(&session_id)
            .expect("close_pty should succeed");
        assert!(exit_code.is_some());
    }

    #[test]
    #[cfg_attr(not(feature = "pty-tests"), ignore = "requires PTY support")]
    fn test_write_to_pty_sends_commands() {
        let mut manager = PtyManager::new();
        let session_id = manager
            .create_pty(
                "bash",
                &["-c", "echo hello; sleep 0.1"],
                Path::new("/tmp"),
                24,
                80,
            )
            .expect("create_pty should succeed");

        std::thread::sleep(Duration::from_millis(200));

        let output = manager
            .drain_output(&session_id)
            .expect("drain_output should succeed");
        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("hello"),
            "Output should contain 'hello', got: {output_str}"
        );

        manager.close_pty(&session_id).ok();
    }

    #[test]
    #[cfg_attr(not(feature = "pty-tests"), ignore = "requires PTY support")]
    fn test_drain_output_returns_output() {
        let mut manager = PtyManager::new();
        let session_id = manager
            .create_pty(
                "bash",
                &["-c", "echo drain_test_output"],
                Path::new("/tmp"),
                24,
                80,
            )
            .expect("create_pty should succeed");

        std::thread::sleep(Duration::from_millis(300));

        let output = manager
            .drain_output(&session_id)
            .expect("drain_output should succeed");
        assert!(
            !output.is_empty(),
            "drain_output should return non-empty data"
        );

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("drain_test_output"),
            "Output should contain 'drain_test_output', got: {output_str}"
        );

        manager.close_pty(&session_id).ok();
    }

    #[test]
    #[cfg_attr(not(feature = "pty-tests"), ignore = "requires PTY support")]
    fn test_resize_pty_changes_dimensions() {
        let mut manager = PtyManager::new();
        let session_id = manager
            .create_pty("bash", &[], Path::new("/tmp"), 24, 80)
            .expect("create_pty should succeed");

        manager
            .resize_pty(&session_id, 40, 120)
            .expect("resize_pty should succeed");

        manager.close_pty(&session_id).ok();
    }

    #[test]
    #[cfg_attr(not(feature = "pty-tests"), ignore = "requires PTY support")]
    fn test_close_pty_returns_exit_code() {
        let mut manager = PtyManager::new();
        let session_id = manager
            .create_pty("bash", &["-c", "exit 42"], Path::new("/tmp"), 24, 80)
            .expect("create_pty should succeed");

        std::thread::sleep(Duration::from_millis(500));

        let exit_code = manager
            .close_pty(&session_id)
            .expect("close_pty should succeed");
        assert_eq!(
            exit_code,
            Some(42),
            "Exit code should be 42, got: {exit_code:?}"
        );
    }

    #[test]
    fn test_write_to_pty_nonexistent_session() {
        let mut manager = PtyManager::new();
        let result = manager.write_to_pty("nonexistent", b"test");
        assert!(matches!(result, Err(PtyError::SessionNotFound(_))));
    }

    #[test]
    fn test_resize_pty_nonexistent_session() {
        let mut manager = PtyManager::new();
        let result = manager.resize_pty("nonexistent", 24, 80);
        assert!(matches!(result, Err(PtyError::SessionNotFound(_))));
    }

    #[test]
    fn test_close_pty_nonexistent_session() {
        let mut manager = PtyManager::new();
        let result = manager.close_pty("nonexistent");
        assert!(matches!(result, Err(PtyError::SessionNotFound(_))));
    }

    #[test]
    fn test_drain_output_nonexistent_session() {
        let mut manager = PtyManager::new();
        let result = manager.drain_output("nonexistent");
        assert!(matches!(result, Err(PtyError::SessionNotFound(_))));
    }

    #[allow(clippy::cast_possible_truncation)]
    #[test]
    fn test_ring_buffer_overflow_drops_oldest() {
        let ring: Arc<Mutex<std::collections::VecDeque<u8>>> = Arc::new(Mutex::new(
            std::collections::VecDeque::with_capacity(RING_BUFFER_MAX),
        ));

        let overflow_size = RING_BUFFER_MAX + 100;
        {
            let mut r = ring.lock().unwrap();
            for i in 0..overflow_size {
                if r.len() >= RING_BUFFER_MAX {
                    r.pop_front();
                }
                r.push_back((i % 256) as u8);
            }
        }

        let r = ring.lock().unwrap();
        assert_eq!(r.len(), RING_BUFFER_MAX);
        let first_byte = r.front().copied().unwrap();
        assert_eq!(first_byte, 100u8);
    }

    #[test]
    fn test_list_sessions() {
        let manager = PtyManager::new();
        assert!(manager.list_sessions().is_empty());
        assert_eq!(manager.list_sessions().len(), 0);
    }

    #[test]
    fn test_pty_error_display() {
        let err = PtyError::SessionNotFound("abc".to_string());
        assert!(err.to_string().contains("abc"));

        let err = PtyError::PtyError("something broke".to_string());
        assert!(err.to_string().contains("something broke"));

        let err = PtyError::IoError(io::Error::from(io::ErrorKind::NotFound));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_pty_status_clone() {
        let status = PtyStatus::Running;
        let cloned = status.clone();
        assert_eq!(status, cloned);

        let status = PtyStatus::Exited(0);
        let cloned = status.clone();
        assert_eq!(status, cloned);
    }
}
