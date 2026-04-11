//! `PtyBash` tool — persistent PTY-backed bash shell with cwd/env state across calls.
//!
//! On Unix, uses `portable_pty` to open a real PTY and spawn `bash -c <script>`,
//! persisting working directory and environment variables between invocations.
//! On Windows, falls back to `std::process::Command` with `cmd /C`.

#![allow(unsafe_code)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Write as IoWrite};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
#[cfg(windows)]
use std::thread;

#[derive(Debug, Clone)]
pub struct ShellState {
    pub cwd: Option<PathBuf>,
    pub env_vars: HashMap<String, String>,
}

impl ShellState {
    fn new() -> Self {
        Self {
            cwd: None,
            env_vars: HashMap::new(),
        }
    }
}

pub fn global_shell_state() -> Arc<Mutex<ShellState>> {
    static STATE: OnceLock<Arc<Mutex<ShellState>>> = OnceLock::new();
    STATE
        .get_or_init(|| Arc::new(Mutex::new(ShellState::new())))
        .clone()
}

type BgTaskMap = HashMap<String, Result<PtyBashOutput, String>>;

pub fn global_bg_task_registry() -> Arc<Mutex<BgTaskMap>> {
    static REG: OnceLock<Arc<Mutex<BgTaskMap>>> = OnceLock::new();
    REG.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

/// Remove a completed background task from the global registry to prevent memory leaks.
/// Returns the task result if it existed.
///
/// Callers should invoke this after reading a background task's result to avoid unbounded
/// growth of the registry.
#[allow(dead_code)]
pub fn cleanup_bg_task(task_id: &str) -> Option<Result<PtyBashOutput, String>> {
    global_bg_task_registry()
        .lock()
        .ok()
        .and_then(|mut map| map.remove(task_id))
}

#[cfg(windows)]
type BgHandleMap = HashMap<String, thread::JoinHandle<()>>;

#[cfg(windows)]
fn global_bg_handle_registry() -> Arc<Mutex<BgHandleMap>> {
    static REG: OnceLock<Arc<Mutex<BgHandleMap>>> = OnceLock::new();
    REG.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

/// RAII guard that kills a child process on drop.
/// Prevents orphaned processes when a thread panics or times out.
#[cfg(unix)]
struct ChildGuard {
    pid: u32,
    killed: bool,
}

#[cfg(unix)]
impl ChildGuard {
    fn new(pid: u32) -> Self {
        Self { pid, killed: false }
    }

    #[allow(dead_code)]
    fn pid(&self) -> u32 {
        self.pid
    }

    fn kill(&mut self) {
        if !self.killed {
            unsafe {
                libc::kill(self.pid.cast_signed(), libc::SIGKILL);
            }
            self.killed = true;
        }
    }
}

#[cfg(unix)]
impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Registry of background child PIDs for Unix, allowing later cleanup.
#[cfg(unix)]
type BgChildPidMap = HashMap<String, u32>;

#[cfg(unix)]
fn global_bg_child_pids() -> Arc<Mutex<BgChildPidMap>> {
    static REG: OnceLock<Arc<Mutex<BgChildPidMap>>> = OnceLock::new();
    REG.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

static BG_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PtyBashInput {
    pub command: String,
    #[allow(dead_code)]
    pub description: Option<String>,
    pub timeout: Option<u64>,
    pub run_in_background: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PtyBashOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub interrupted: bool,
    pub background_task_id: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum AnsiState {
    Normal,
    Escape,
    Csi,
    Osc,
    Designator,
}

fn strip_ansi_codes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut state = AnsiState::Normal;

    for ch in input.chars() {
        match state {
            AnsiState::Normal => {
                if ch == '\x1B' {
                    state = AnsiState::Escape;
                } else if ch != '\r' {
                    out.push(ch);
                }
            }
            AnsiState::Escape => {
                state = match ch {
                    '[' => AnsiState::Csi,
                    ']' => AnsiState::Osc,
                    '(' | ')' | '*' | '+' => AnsiState::Designator,
                    _ => {
                        state = AnsiState::Normal;
                        continue;
                    }
                };
            }
            AnsiState::Csi => {
                if ch.is_ascii_alphabetic() {
                    state = AnsiState::Normal;
                }
            }
            AnsiState::Osc => {
                if ch == '\x07' || ch == '\\' {
                    state = AnsiState::Normal;
                }
            }
            AnsiState::Designator => {
                state = AnsiState::Normal;
            }
        }
    }

    out
}

const SENTINEL: &str = "__CC_SHELL_STATE__";

fn parse_shell_state(output: &str) -> Option<(PathBuf, HashMap<String, String>)> {
    let idx = output.rfind(SENTINEL)?;
    let state_block = &output[idx + SENTINEL.len()..];

    let mut lines = state_block.lines().filter(|l| !l.trim().is_empty());
    let cwd = PathBuf::from(lines.next()?.trim());
    let mut env_vars = HashMap::new();

    for line in lines {
        let line = line.trim();
        if let Some(eq) = line.find('=') {
            let key = &line[..eq];
            let value = &line[eq + 1..];
            if !key.is_empty() {
                env_vars.insert(key.to_string(), value.to_string());
            }
        }
    }

    Some((cwd, env_vars))
}

fn escape_single_quote(s: &str) -> String {
    s.replace('\'', "'\\''")
}

use std::fmt::Write;

#[cfg(unix)]
#[allow(clippy::too_many_lines)]
fn execute_pty_bash_unix(input: &PtyBashInput) -> Result<PtyBashOutput, String> {
    use portable_pty::{CommandBuilder, PtySize};

    let timeout_secs = input.timeout.unwrap_or(300);
    let run_in_background = input.run_in_background.unwrap_or(false);
    let user_cmd = input.command.clone();

    let state = global_shell_state();
    let guard = state.lock().map_err(|e| format!("lock poisoned: {e}"))?;

    let cwd_str = match &guard.cwd {
        Some(p) => escape_single_quote(&p.to_string_lossy()),
        None => escape_single_quote(
            &std::env::current_dir()
                .map_err(|e| format!("cannot determine cwd: {e}"))?
                .to_string_lossy(),
        ),
    };

    let mut exports = String::new();
    for (k, v) in &guard.env_vars {
        let _ = write!(
            exports,
            "export '{}'='{}';",
            escape_single_quote(k),
            escape_single_quote(v)
        );
    }
    drop(guard);

    // `user_cmd` is shell code typed by the user; it is intentionally passed verbatim
    // to `bash -c` so that all shell metacharacters (pipes, redirects, single/double
    // quotes) are interpreted by Bash as the user intended.
    // Do NOT apply any escaping here — the old `.replace('\'', "'\\''")` was incorrect
    // because the command was never wrapped in single quotes in the format string,
    // which produced broken Bash syntax for any input containing a `'`.
    let script = format!(
        "cd '{cwd_str}'; {exports} {user_cmd}; exit_code=$?; printf '\\n{SENTINEL}\\n'; pwd; env; exit $exit_code"
    );

    let pty_system = portable_pty::native_pty_system();

    let pair = pty_system
        .openpty(PtySize {
            rows: 50,
            cols: 220,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("failed to open pty: {e}"))?;

    let mut cmd = CommandBuilder::new("bash");
    cmd.arg("-c");
    cmd.arg(&script);

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("failed to spawn bash in pty: {e}"))?;

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("failed to clone reader: {e}"))?;
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("failed to take writer: {e}"))?;

    let _ = writer.write_all(b"\n");
    let _ = writer.flush();

    if run_in_background {
        let id = format!("pty_bg_{}", BG_COUNTER.fetch_add(1, Ordering::SeqCst) + 1);
        let id_for_return = id.clone();
        let registry = global_bg_task_registry();
        let pid_registry = global_bg_child_pids();
        let pid_for_registry = child.process_id().unwrap_or(0);

        pid_registry
            .lock()
            .map(|mut map| map.insert(id.clone(), pid_for_registry))
            .ok();

        std::thread::spawn(move || {
            let mut guard = ChildGuard::new(pid_for_registry);
            let mut stdout = String::new();
            let mut buf = vec![0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let s = String::from_utf8_lossy(&buf[..n]);
                        stdout.push_str(&s);
                        if stdout.len() > 2_097_152 {
                            stdout.truncate(2_097_152);
                            break;
                        }
                    }
                }
            }
            let _ = child.wait();
            guard.killed = true;

            let output = PtyBashOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: 0,
                interrupted: false,
                background_task_id: Some(id.clone()),
            };
            let _ = registry.lock().map(|mut map| map.insert(id, Ok(output)));
        });

        return Ok(PtyBashOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            interrupted: false,
            background_task_id: Some(id_for_return),
        });
    }

    let pid = child.process_id().unwrap_or(0);
    let mut guard = ChildGuard::new(pid);

    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let mut stdout = String::new();
        let mut buf = vec![0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let s = String::from_utf8_lossy(&buf[..n]);
                    stdout.push_str(&s);
                    if stdout.len() > 2_097_152 {
                        stdout.truncate(2_097_152);
                        break;
                    }
                }
            }
        }
        let exit_status = child.wait();
        let _ = tx.send((exit_status, stdout));
    });

    let result = rx.recv_timeout(std::time::Duration::from_secs(timeout_secs));

    let (exit_result, raw_output) = match result {
        Ok(val) => {
            guard.killed = true;
            val
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            return Err("command timed out".to_string());
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            return Err("background thread panicked".to_string());
        }
    };

    let stripped = strip_ansi_codes(&raw_output);
    let interrupted = exit_result.is_err();

    let exit_code = match &exit_result {
        Ok(st) => st.exit_code().cast_signed(),
        Err(_) => -1,
    };

    if let Some((cwd, env_vars)) = parse_shell_state(&stripped) {
        if let Ok(mut s) = state.lock() {
            s.cwd = Some(cwd);
            s.env_vars = env_vars;
        }
    }

    Ok(PtyBashOutput {
        stdout: stripped,
        stderr: String::new(),
        exit_code,
        interrupted,
        background_task_id: None,
    })
}

#[cfg(windows)]
fn execute_pty_bash_windows(input: &PtyBashInput) -> Result<PtyBashOutput, String> {
    use std::process::Command;

    let timeout_secs = input.timeout.unwrap_or(300);
    let run_in_background = input.run_in_background.unwrap_or(false);

    if run_in_background {
        let id = format!("pty_bg_{}", BG_COUNTER.fetch_add(1, Ordering::SeqCst) + 1);
        let registry = global_bg_task_registry();
        let handle_registry = global_bg_handle_registry();
        let cmd_str = input.command.clone();
        let id_for_thread = id.clone();

        let handle = std::thread::spawn(move || {
            let output = Command::new("cmd")
                .args(["/C", &cmd_str])
                .output()
                .map(|o| PtyBashOutput {
                    stdout: String::from_utf8_lossy(&o.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&o.stderr).to_string(),
                    exit_code: o.status.code().unwrap_or(-1),
                    interrupted: false,
                    background_task_id: Some(id_for_thread.clone()),
                })
                .map_err(|e| e.to_string());

            let _ = registry.lock().map(|mut map| {
                map.insert(id_for_thread, output);
            });
        });

        if let Ok(mut map) = handle_registry.lock() {
            map.insert(id.clone(), handle);
        }

        return Ok(PtyBashOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            interrupted: false,
            background_task_id: Some(id),
        });
    }

    let mut child = Command::new("cmd")
        .args(["/C", &input.command])
        .spawn()
        .map_err(|e| format!("failed to spawn cmd: {e}"))?;

    let (tx, rx) = mpsc::channel();
    let mut child_for_thread = child
        .try_clone()
        .map_err(|e| format!("failed to clone child: {e}"))?;
    thread::spawn(move || {
        let _ = tx.send(child_for_thread.wait());
    });

    let result = rx
        .recv_timeout(std::time::Duration::from_secs(timeout_secs))
        .map_err(|_| {
            let _ = child.kill();
            "command timed out".to_string()
        })?
        .map_err(|e| format!("wait failed: {e}"))?;

    let output = child
        .wait_with_output()
        .map_err(|e| format!("failed to read output: {e}"))?;

    let exit_code = result.code().unwrap_or(-1);

    Ok(PtyBashOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code,
        interrupted: false,
        background_task_id: None,
    })
}

#[cfg(unix)]
pub fn execute_pty_bash(input: &PtyBashInput) -> Result<PtyBashOutput, String> {
    execute_pty_bash_unix(input)
}

#[cfg(windows)]
pub fn execute_pty_bash(input: &PtyBashInput) -> Result<PtyBashOutput, String> {
    execute_pty_bash_windows(input)
}

pub fn pty_bash_tool_spec() -> Value {
    serde_json::to_value(schemars::schema_for!(PtyBashInput)).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_csi_codes() {
        assert_eq!(strip_ansi_codes("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(strip_ansi_codes("\x1b[1;34mbold blue\x1b[0m"), "bold blue");
    }

    #[test]
    fn strip_ansi_osc_codes() {
        assert_eq!(strip_ansi_codes("\x1b]0;title\x07content"), "content");
    }

    #[test]
    fn strip_ansi_designator_codes() {
        assert_eq!(strip_ansi_codes("\x1b(Bhello"), "hello");
        assert_eq!(strip_ansi_codes("\x1b)Ctest"), "test");
    }

    #[test]
    fn strip_ansi_two_char_escapes() {
        assert_eq!(strip_ansi_codes("\x1b7save\x1b8restore"), "saverestore");
        assert_eq!(strip_ansi_codes("\x1bHtab"), "tab");
    }

    #[test]
    fn strip_ansi_discards_cr_only() {
        assert_eq!(strip_ansi_codes("\r\n"), "\n");
        assert_eq!(strip_ansi_codes("text\r\nmore"), "text\nmore");
    }

    #[test]
    fn strip_ansi_complex_terminal_output() {
        let input = "\x1b[?2004h\x1b[?1l\x1b[?25h\x1b[0m\x1b[0Jhello\x1b[0m\r\n\x1b[?2004lworld";
        let result = strip_ansi_codes(input);
        assert!(result.contains("hello"));
        assert!(result.contains("world"));
        assert!(!result.contains('\x1B'));
    }

    #[test]
    fn strip_ansi_no_codes_passthrough() {
        assert_eq!(strip_ansi_codes("plain text"), "plain text");
        assert_eq!(strip_ansi_codes("line1\nline2\n"), "line1\nline2\n");
    }

    #[test]
    fn escape_single_quote_basic() {
        assert_eq!(escape_single_quote("hello"), "hello");
    }

    #[test]
    fn escape_single_quote_with_quotes() {
        assert_eq!(escape_single_quote("it's"), "it'\\''s");
    }

    #[test]
    fn escape_single_quote_only_quotes() {
        assert_eq!(escape_single_quote("'''"), "'\\'''\\'''\\''");
    }

    #[test]
    fn shell_state_parsing() {
        let output =
            "some command output\n__CC_SHELL_STATE__\n/tmp/test\nPATH=/usr/bin\nHOME=/root\n";
        let (cwd, env) = parse_shell_state(output).expect("should parse");
        assert_eq!(cwd, PathBuf::from("/tmp/test"));
        assert_eq!(env.get("PATH"), Some(&"/usr/bin".to_string()));
        assert_eq!(env.get("HOME"), Some(&"/root".to_string()));
    }

    #[test]
    fn shell_state_parsing_no_sentinel() {
        let output = "no sentinel here";
        assert!(parse_shell_state(output).is_none());
    }

    #[test]
    fn tool_spec_has_correct_structure() {
        let spec = pty_bash_tool_spec();
        assert_eq!(spec["type"], "object");
        assert!(spec["properties"]["command"].is_object());
        assert!(spec["properties"]["description"].is_object());
        assert!(spec["properties"]["timeout"].is_object());
        assert!(spec["properties"]["run_in_background"].is_object());
        let required = &spec["required"];
        assert!(required
            .as_array()
            .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("command"))));
    }

    #[test]
    fn bg_counter_increments() {
        let id1 = format!("pty_bg_{}", BG_COUNTER.fetch_add(1, Ordering::SeqCst) + 1);
        let id2 = format!("pty_bg_{}", BG_COUNTER.fetch_add(1, Ordering::SeqCst) + 1);
        assert_ne!(id1, id2);
    }

    #[test]
    fn global_shell_state_is_singleton() {
        let s1 = global_shell_state();
        let s2 = global_shell_state();
        assert!(Arc::ptr_eq(&s1, &s2));
    }

    #[test]
    fn global_bg_registry_is_singleton() {
        let r1 = global_bg_task_registry();
        let r2 = global_bg_task_registry();
        assert!(Arc::ptr_eq(&r1, &r2));
    }

    #[test]
    fn cleanup_bg_task_removes_entry() {
        let registry = global_bg_task_registry();
        let test_id = "__test_cleanup__";

        {
            let mut map = registry.lock().unwrap();
            map.insert(
                test_id.to_string(),
                Ok(PtyBashOutput {
                    stdout: "test".into(),
                    stderr: String::new(),
                    exit_code: 0,
                    interrupted: false,
                    background_task_id: Some(test_id.to_string()),
                }),
            );
        }

        assert!(registry.lock().unwrap().contains_key(test_id));

        let result = cleanup_bg_task(test_id);
        assert!(result.is_some());

        assert!(!registry.lock().unwrap().contains_key(test_id));

        assert!(cleanup_bg_task("nonexistent").is_none());
    }
}
