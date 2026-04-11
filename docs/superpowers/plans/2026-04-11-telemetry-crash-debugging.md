# Telemetry & Crash Debugging Infrastructure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add comprehensive crash reporting, post-mortem telemetry, structured logging, and diagnostic snapshot capabilities so that after any icode crash, developers can reconstruct exactly what happened from both backend and frontend perspectives.

**Architecture:** Extend the existing `telemetry` crate with a crash reporter (panic hook + backtrace capture), add a `DiagnosticSnapshot` for system state capture, wire panic hook into both CLI and server binaries, add a `--crash-report` CLI command to print the last crash report, and add `/crash-report` slash command for in-TUI access.

**Tech Stack:** Rust 2021, `backtrace` crate, `tracing`/`tracing-subscriber`, existing `telemetry` crate with `TelemetrySink`/`SessionTracer`, `serde`/`serde_json`, `color-eyre` (already in CLI deps), `tracing-appender` for file rotation.

---

## File Structure Map

| File | Action | Responsibility |
|---|---|---|
| `rust/crates/telemetry/Cargo.toml` | Modify | Add `backtrace`, `chrono`, `tracing-appender` deps |
| `rust/crates/telemetry/src/paths.rs` | Create | Centralized path constants for logs, crash reports, diagnostics |
| `rust/crates/telemetry/src/crash_report.rs` | Create | CrashReport struct, panic hook installer, backtrace capture, system info |
| `rust/crates/telemetry/src/diagnostic.rs` | Create | DiagnosticSnapshot - system state, memory, git, config, session state capture |
| `rust/crates/telemetry/src/lib.rs` | Modify | Add mod declarations + pub re-exports for new modules |
| `rust/crates/icode-cli/Cargo.toml` | Modify | Add `telemetry` dep |
| `rust/crates/icode-cli/src/tui/debug.rs` | Modify | Integrate crash report path display in debug panel |
| `rust/crates/icode-cli/src/main.rs` | Modify | Wire `install_panic_hook()` at startup, handle `--crash-report` flag |
| `rust/crates/commands/src/lib.rs` | Modify | Add CrashReport slash command spec |
| `rust/crates/icode-server/Cargo.toml` | Modify | Add `telemetry` dep |
| `rust/crates/icode-server/src/main.rs` | Modify | Wire panic hook for server |
| `rust/crates/icode-server/src/routes/diagnostics.rs` | Create | `/diagnostics` and `/crash-report` HTTP endpoints |

---

### Task 1: Telemetry Crate - Paths Module

**Files:**
- Create: `rust/crates/telemetry/src/paths.rs`

- [ ] **Step 1: Create centralized paths module**

```rust
// rust/crates/telemetry/src/paths.rs
use std::path::PathBuf;

/// Root config directory: ~/.icode/ or $CLAW_CONFIG_HOME/
pub fn icode_config_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("CLAW_CONFIG_HOME") {
        return PathBuf::from(path);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".icode")
}

/// Log directory: ~/.icode/logs/
pub fn log_dir() -> PathBuf {
    icode_config_dir().join("logs")
}

/// Crash reports directory: ~/.icode/crash-reports/
pub fn crash_reports_dir() -> PathBuf {
    icode_config_dir().join("crash-reports")
}

/// Diagnostic snapshots directory: ~/.icode/diagnostics/
pub fn diagnostics_dir() -> PathBuf {
    icode_config_dir().join("diagnostics")
}

/// Path to the latest crash report file.
pub fn latest_crash_report_path() -> PathBuf {
    crash_reports_dir().join("latest-crash.json")
}

/// Ensure all telemetry directories exist.
pub fn ensure_telemetry_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(log_dir())?;
    std::fs::create_dir_all(crash_reports_dir())?;
    std::fs::create_dir_all(diagnostics_dir())?;
    Ok(())
}
```

- [ ] **Step 2: Verify compiles**

Run: `cd rust && cargo check -p telemetry`
Expected: clean compilation (may warn about unused functions, that's fine)

---

### Task 2: Telemetry Crate - Crash Reporter

**Files:**
- Create: `rust/crates/telemetry/src/crash_report.rs`
- Modify: `rust/crates/telemetry/Cargo.toml`

- [ ] **Step 1: Add dependencies**

Append to `[dependencies]` in `rust/crates/telemetry/Cargo.toml`:
```toml
backtrace = "0.3"
chrono = { workspace = true }
```

- [ ] **Step 2: Create crash_report.rs with CrashReport struct and panic hook**

Full file content for `rust/crates/telemetry/src/crash_report.rs`:

```rust
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;
use std::thread;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::paths::{crash_reports_dir, ensure_telemetry_dirs, latest_crash_report_path};

static HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);
static INSTALL_ONCE: Once = Once::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashReport {
    pub timestamp: String,
    pub pid: u32,
    pub thread_name: String,
    pub panic_message: String,
    pub panic_location: Option<String>,
    pub backtrace: String,
    pub system_info: SystemInfo,
    pub app_version: String,
    pub build_target: Option<String>,
    pub git_sha: Option<String>,
    pub session_id: Option<String>,
    pub context: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
    pub hostname: Option<String>,
    pub cwd: String,
    pub env_vars: HashMap<String, String>,
}

impl SystemInfo {
    pub fn capture() -> Self {
        let mut env_vars = HashMap::new();
        for key in &[
            "RUST_LOG", "RUST_BACKTRACE", "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY", "GEMINI_API_KEY", "CLAW_CONFIG_HOME",
            "TERM", "COLORTERM", "LANG",
        ] {
            if let Ok(val) = std::env::var(key) {
                let masked = if key.contains("API_KEY") || key.contains("SECRET") {
                    mask_secret(&val)
                } else {
                    val
                };
                env_vars.insert(key.to_string(), masked);
            }
        }
        Self {
            os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            arch: std::env::consts::ARCH.to_string(),
            hostname: hostname::get().ok().and_then(|h| h.into_string().ok()),
            cwd: std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "<unknown>".to_string()),
            env_vars,
        }
    }
}

fn mask_secret(val: &str) -> String {
    if val.len() <= 8 {
        "***".to_string()
    } else {
        format!("{}***{}", &val[..4], &val[val.len() - 4..])
    }
}

impl CrashReport {
    pub fn from_panic(
        panic_msg: &str,
        panic_loc: Option<(&str, u32, u32)>,
        context: HashMap<String, String>,
    ) -> Self {
        let bt = backtrace::Backtrace::new();
        let backtrace_str = format!("{bt:?}");
        Self {
            timestamp: Utc::now().to_rfc3339(),
            pid: std::process::id(),
            thread_name: thread::current().name().unwrap_or("unknown").to_string(),
            panic_message: panic_msg.to_string(),
            panic_location: panic_loc.map(|(f, l, c)| format!("{f}:{l}:{c}")),
            backtrace: backtrace_str,
            system_info: SystemInfo::capture(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            build_target: option_env!("TARGET").map(String::from),
            git_sha: option_env!("GIT_SHA").map(String::from),
            session_id: None,
            context,
        }
    }

    pub fn persist(&self) -> Result<PathBuf, std::io::Error> {
        ensure_telemetry_dirs()?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let latest = latest_crash_report_path();
        let mut file = OpenOptions::new()
            .create(true).write(true).truncate(true).open(&latest)?;
        file.write_all(json.as_bytes())?;
        let ts = Utc::now().format("%Y%m%d-%H%M%S");
        let copy_path = crash_reports_dir().join(format!("crash-{ts}.json"));
        fs::write(&copy_path, &json)?;
        Ok(copy_path)
    }

    pub fn load_latest() -> Result<Option<Self>, std::io::Error> {
        let path = latest_crash_report_path();
        if !path.exists() { return Ok(None); }
        let content = fs::read_to_string(&path)?;
        let report: Self = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Some(report))
    }

    pub fn summary(&self) -> String {
        let mut out = String::new();
        out.push_str("=== Crash Report ===\n\n");
        out.push_str(&format!("Timestamp:     {}\n", self.timestamp));
        out.push_str(&format!("PID:           {}\n", self.pid));
        out.push_str(&format!("Thread:        {}\n", self.thread_name));
        out.push_str(&format!("Version:       {}\n", self.app_version));
        if let Some(sha) = &self.git_sha { out.push_str(&format!("Git SHA:       {sha}\n")); }
        if let Some(sid) = &self.session_id { out.push_str(&format!("Session:       {sid}\n")); }
        out.push('\n');
        out.push_str(&format!("OS:            {}\n", self.system_info.os));
        out.push_str(&format!("CWD:           {}\n", self.system_info.cwd));
        out.push('\n');
        out.push_str(&format!("Panic:         {}\n", self.panic_message));
        if let Some(loc) = &self.panic_location { out.push_str(&format!("Location:      {loc}\n")); }
        out.push_str("\n--- Backtrace (abbreviated) ---\n");
        let frames: Vec<&str> = self.backtrace.lines().collect();
        let show = frames.len().min(20);
        for frame in &frames[..show] {
            out.push_str(frame);
            out.push('\n');
        }
        if frames.len() > show {
            out.push_str(&format!("\n... and {} more frames (see full report in file)\n", frames.len() - show));
        }
        if !self.context.is_empty() {
            out.push_str("\n--- Context ---\n");
            for (k, v) in &self.context {
                out.push_str(&format!("{k}: {v}\n"));
            }
        }
        out
    }
}

pub fn install_panic_hook(context: HashMap<String, String>) {
    INSTALL_ONCE.call_once(|| {
        let _ = ensure_telemetry_dirs();
        let prev_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let panic_msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = info.payload().downcast_ref::<String>() {
                s.clone()
            } else {
                "Box<dyn Any>".to_string()
            };
            let panic_loc = info.location().map(|loc| (loc.file(), loc.line(), loc.column()));
            let report = CrashReport::from_panic(&panic_msg, panic_loc, context.clone());
            if let Ok(path) = report.persist() {
                let _ = writeln!(
                    std::io::stderr(),
                    "\n=== icode crashed - crash report saved to: {} ===\n",
                    path.display()
                );
            }
            prev_hook(info);
        }));
        HOOK_INSTALLED.store(true, Ordering::SeqCst);
    });
}

pub fn is_hook_installed() -> bool {
    HOOK_INSTALLED.load(Ordering::SeqCst)
}
```

- [ ] **Step 3: Update lib.rs to include new modules**

In `rust/crates/telemetry/src/lib.rs`, add at the top after existing `use` statements:
```rust
pub mod paths;
pub mod crash_report;
```

And add pub re-exports at the bottom (before tests):
```rust
pub use crash_report::{CrashReport, SystemInfo, install_panic_hook, is_hook_installed};
pub use paths::{
    icode_config_dir, log_dir, crash_reports_dir, diagnostics_dir,
    latest_crash_report_path, ensure_telemetry_dirs,
};
```

- [ ] **Step 4: Verify compiles**

Run: `cd rust && cargo check -p telemetry`
Expected: clean compilation

- [ ] **Step 5: Add basic tests for crash_report**

Add to bottom of `crash_report.rs` (before closing):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_secret_short() {
        assert_eq!(mask_secret("abc"), "***");
    }

    #[test]
    fn test_mask_secret_long() {
        let result = mask_secret("sk-ant-1234567890abcdef");
        assert!(result.starts_with("sk-a"));
        assert!(result.ends_with("cdef"));
        assert!(result.contains("***"));
    }

    #[test]
    fn test_system_info_capture_has_os_and_cwd() {
        let info = SystemInfo::capture();
        assert!(!info.os.is_empty());
        assert!(!info.cwd.is_empty());
    }

    #[test]
    fn test_crash_report_persists_and_loads() {
        let report = CrashReport::from_panic(
            "test panic",
            Some(("test.rs", 42, 10)),
            HashMap::from([("key".to_string(), "val".to_string())]),
        );
        let path = report.persist().expect("persist should succeed");
        let loaded = CrashReport::load_latest()
            .expect("load should succeed")
            .expect("report should exist");
        assert_eq!(loaded.panic_message, "test panic");
        assert!(loaded.panic_location.as_ref().unwrap().contains("test.rs"));
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(latest_crash_report_path());
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cd rust && cargo test -p telemetry`
Expected: all 4 tests pass

---

### Task 3: Telemetry Crate - Diagnostic Snapshot

**Files:**
- Create: `rust/crates/telemetry/src/diagnostic.rs`

- [ ] **Step 1: Create diagnostic snapshot module**

Full file content for `rust/crates/telemetry/src/diagnostic.rs`:

```rust
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::paths::{diagnostics_dir, ensure_telemetry_dirs};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticSnapshot {
    pub timestamp: String,
    pub version: String,
    pub git_sha: Option<String>,
    pub system: SystemDiagnostic,
    pub runtime: RuntimeDiagnostic,
    pub config: ConfigDiagnostic,
    pub sessions: SessionDiagnostic,
    pub logs: LogDiagnostic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemDiagnostic {
    pub os: String,
    pub arch: String,
    pub hostname: Option<String>,
    pub cpu_count: usize,
    pub memory: MemoryInfo,
    pub disk: DiskInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total_mb: u64,
    pub available_mb: u64,
    pub process_rss_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub config_dir_mb: Option<u64>,
    pub log_files_count: usize,
    pub crash_reports_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDiagnostic {
    pub uptime_secs: u64,
    pub pid: u32,
    pub binary_path: String,
    pub cwd: String,
    pub env_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDiagnostic {
    pub config_dir: String,
    pub config_files: Vec<String>,
    pub has_claude_md: bool,
    pub has_opencode_json: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDiagnostic {
    pub sessions_dir: String,
    pub session_count: usize,
    pub latest_session: Option<SessionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub path: String,
    pub size_bytes: u64,
    pub modified: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogDiagnostic {
    pub log_dir: String,
    pub log_files: Vec<LogFileInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogFileInfo {
    pub name: String,
    pub size_bytes: u64,
    pub modified: String,
}

impl DiagnosticSnapshot {
    pub fn capture() -> Self {
        let start = std::time::Instant::now();

        let config_dir = crate::paths::icode_config_dir();
        let log_dir = crate::paths::log_dir();
        let crash_dir = crate::paths::crash_reports_dir();

        // System info
        let system = SystemDiagnostic {
            os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            arch: std::env::consts::ARCH.to_string(),
            hostname: hostname::get().ok().and_then(|h| h.into_string().ok()),
            cpu_count: num_cpus::get(),
            memory: capture_memory_info(),
            disk: DiskInfo {
                config_dir_mb: dir_size_mb(&config_dir),
                log_files_count: count_files(&log_dir),
                crash_reports_count: count_files(&crash_dir),
            },
        };

        // Runtime info
        let runtime = RuntimeDiagnostic {
            uptime_secs: start.elapsed().as_secs(), // approximate
            pid: std::process::id(),
            binary_path: std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "<unknown>".to_string()),
            cwd: std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "<unknown>".to_string()),
            env_keys: std::env::vars().map(|(k, _)| k).collect(),
        };

        // Config info
        let config_files = list_dir_entries(&config_dir);
        let mut has_claude_md = false;
        let mut has_opencode_json = false;
        let cwd = std::env::current_dir().unwrap_or_default();
        if cwd.join("CLAUDE.md").exists() { has_claude_md = true; }
        if cwd.join(".opencode.json").exists() { has_opencode_json = true; }

        let config = ConfigDiagnostic {
            config_dir: config_dir.display().to_string(),
            config_files,
            has_claude_md,
            has_opencode_json,
        };

        // Session info
        let sessions_dir = config_dir.join("sessions");
        let sessions = capture_session_info(&sessions_dir);

        // Log info
        let logs = capture_log_info(&log_dir);

        Self {
            timestamp: Utc::now().to_rfc3339(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            git_sha: option_env!("GIT_SHA").map(String::from),
            system,
            runtime,
            config,
            sessions,
            logs,
        }
    }

    pub fn persist(&self) -> Result<PathBuf, std::io::Error> {
        ensure_telemetry_dirs()?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let ts = Utc::now().format("%Y%m%d-%H%M%S");
        let path = diagnostics_dir().join(format!("diagnostic-{ts}.json"));
        fs::write(&path, &json)?;
        Ok(path)
    }

    pub fn summary(&self) -> String {
        let mut out = String::new();
        out.push_str("=== Diagnostic Snapshot ===\n\n");
        out.push_str(&format!("Timestamp:    {}\n", self.timestamp));
        out.push_str(&format!("Version:      {}\n", self.version));
        if let Some(sha) = &self.git_sha { out.push_str(&format!("Git SHA:      {sha}\n")); }
        out.push('\n');
        out.push_str("--- System ---\n");
        out.push_str(&format!("OS:           {}\n", self.system.os));
        out.push_str(&format!("CPU cores:    {}\n", self.system.cpu_count));
        if let Some(rss) = self.system.memory.process_rss_mb {
            out.push_str(&format!("Process RSS:  {} MB\n", rss));
        }
        out.push_str(&format!("Config dir:   {} MB\n", self.system.disk.config_dir_mb.unwrap_or(0)));
        out.push_str(&format!("Log files:    {}\n", self.system.disk.log_files_count));
        out.push_str(&format!("Crash reports: {}\n", self.system.disk.crash_reports_count));
        out.push('\n');
        out.push_str("--- Sessions ---\n");
        out.push_str(&format!("Session count: {}\n", self.sessions.session_count));
        if let Some(latest) = &self.sessions.latest_session {
            out.push_str(&format!("Latest:        {} ({} bytes)\n", latest.path, latest.size_bytes));
        }
        out.push('\n');
        out.push_str("--- Logs ---\n");
        out.push_str(&format!("Log dir:       {}\n", self.logs.log_dir));
        for log in &self.logs.log_files {
            out.push_str(&format!("  {} ({} bytes, {})\n", log.name, log.size_bytes, log.modified));
        }
        out
    }
}

fn capture_memory_info() -> MemoryInfo {
    let mut info = MemoryInfo {
        total_mb: 0,
        available_mb: 0,
        process_rss_mb: None,
    };

    // Linux: /proc/meminfo and /proc/self/status
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    info.total_mb = parse_meminfo_line_kb(line) / 1024;
                } else if line.starts_with("MemAvailable:") {
                    info.available_mb = parse_meminfo_line_kb(line) / 1024;
                }
            }
        }
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<u64>() {
                            info.process_rss_mb = Some(kb / 1024);
                        }
                    }
                }
            }
        }
    }

    // macOS: sysctl
    #[cfg(target_os = "macos")]
    {
        info.total_mb = get_sysctl_u64("hw.memsize") / (1024 * 1024);
        // Process RSS via proc_pidinfo would require unsafe, skip for now
    }

    info
}

#[cfg(target_os = "linux")]
fn parse_meminfo_line_kb(line: &str) -> u64 {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        parts[1].parse().unwrap_or(0)
    } else {
        0
    }
}

#[cfg(target_os = "macos")]
fn get_sysctl_u64(name: &str) -> u64 {
    use std::process::Command;
    let output = Command::new("sysctl").arg("-n").arg(name).output().ok();
    output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0)
}

fn dir_size_mb(dir: &std::path::Path) -> Option<u64> {
    if !dir.exists() { return None; }
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
        if entry.file_type().is_file() {
            total += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    Some(total / (1024 * 1024))
}

fn count_files(dir: &std::path::Path) -> usize {
    if !dir.exists() { return 0; }
    std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(|e| e.ok()).count())
        .unwrap_or(0)
}

fn list_dir_entries(dir: &std::path::Path) -> Vec<String> {
    if !dir.exists() { return vec![]; }
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn capture_session_info(sessions_dir: &std::path::Path) -> SessionDiagnostic {
    if !sessions_dir.exists() {
        return SessionDiagnostic {
            sessions_dir: sessions_dir.display().to_string(),
            session_count: 0,
            latest_session: None,
        };
    }

    let mut sessions = Vec::new();
    if let Ok(entries) = std::fs::read_dir(sessions_dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    let modified = meta.modified()
                        .ok()
                        .map(|t| {
                            chrono::DateTime::<Utc>::from(t)
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string()
                        })
                        .unwrap_or_else(|| "unknown".to_string());
                    sessions.push(SessionInfo {
                        path: entry.file_name().to_string_lossy().to_string(),
                        size_bytes: meta.len(),
                        modified,
                    });
                }
            }
        }
    }

    sessions.sort_by(|a, b| b.modified.cmp(&a.modified));
    let latest = sessions.first().cloned();

    SessionDiagnostic {
        sessions_dir: sessions_dir.display().to_string(),
        session_count: sessions.len(),
        latest_session: latest,
    }
}

fn capture_log_info(log_dir: &std::path::Path) -> LogDiagnostic {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    let modified = meta.modified()
                        .ok()
                        .map(|t| {
                            chrono::DateTime::<Utc>::from(t)
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string()
                        })
                        .unwrap_or_else(|| "unknown".to_string());
                    files.push(LogFileInfo {
                        name: entry.file_name().to_string_lossy().to_string(),
                        size_bytes: meta.len(),
                        modified,
                    });
                }
            }
        }
    }
    files.sort_by(|a, b| b.modified.cmp(&a.modified));
    LogDiagnostic {
        log_dir: log_dir.display().to_string(),
        log_files: files,
    }
}
```

- [ ] **Step 2: Add walkdir and num_cpus to telemetry Cargo.toml**

```toml
walkdir = { workspace = true }
num_cpus = "1"
```

Also add to workspace Cargo.toml `[workspace.dependencies]`:
```toml
num_cpus = "1"
```

- [ ] **Step 3: Add mod declaration to lib.rs**

Add to `rust/crates/telemetry/src/lib.rs`:
```rust
pub mod diagnostic;
```

And re-export:
```rust
pub use diagnostic::DiagnosticSnapshot;
```

- [ ] **Step 4: Verify compiles**

Run: `cd rust && cargo check -p telemetry`
Expected: clean compilation

---

### Task 4: Wire Panic Hook into CLI Binary

**Files:**
- Modify: `rust/crates/icode-cli/Cargo.toml`
- Modify: `rust/crates/icode-cli/src/main.rs`

- [ ] **Step 1: Add telemetry dependency to icode-cli**

Add to `rust/crates/icode-cli/Cargo.toml` under `[dependencies]`:
```toml
telemetry = { path = "../telemetry" }
```

- [ ] **Step 2: Wire panic hook into main.rs**

In `rust/crates/icode-cli/src/main.rs`, add `use telemetry;` near the top with other imports.

In the `run()` function, add this near the beginning (after `apply_env_from_config();`):
```rust
// Install crash reporter panic hook
let mut crash_context = std::collections::HashMap::new();
crash_context.insert("binary".to_string(), "icode-cli".to_string());
crash_context.insert("mode".to_string(), "interactive".to_string());
telemetry::install_panic_hook(crash_context);
```

- [ ] **Step 3: Add --crash-report CLI flag**

In the `parse_args` function in `main.rs`, add handling for `--crash-report`:
```rust
"--crash-report" => {
    return Ok(CliAction::CrashReport);
}
```

Add `CrashReport` to the `CliAction` enum:
```rust
enum CliAction {
    // ... existing variants ...
    CrashReport,
}
```

Add the handler in `run()` match arm:
```rust
CliAction::CrashReport => print_crash_report(),
```

Add the `print_crash_report` function:
```rust
fn print_crash_report() {
    match telemetry::CrashReport::load_latest() {
        Ok(Some(report)) => {
            println!("{}", report.summary());
            println!("\nFull report: {}", telemetry::latest_crash_report_path().display());
        }
        Ok(None) => {
            println!("No crash reports found.");
            println!("Crash reports are saved to: {}", telemetry::crash_reports_dir().display());
        }
        Err(e) => {
            eprintln!("Error loading crash report: {e}");
            std::process::exit(1);
        }
    }
}
```

- [ ] **Step 4: Verify compiles**

Run: `cd rust && cargo check -p icode-cli`
Expected: clean compilation

---

### Task 5: Add /crash-report Slash Command

**Files:**
- Modify: `rust/crates/commands/src/lib.rs`
- Create: `rust/crates/commands/src/crash_report.rs` (if commands crate uses module-per-command pattern)

- [ ] **Step 1: Check commands crate structure**

Look at `rust/crates/commands/src/` to understand the pattern. If commands are in `lib.rs`, add inline. If modular, create a new file.

- [ ] **Step 2: Add CrashReport slash command spec**

In the slash command specs list, add:
```rust
SlashCommandSpec {
    name: "crash-report",
    aliases: &["crash"],
    description: "Show the last crash report (if any)",
    resume_supported: false,
}
```

- [ ] **Step 3: Implement handler**

Add handler that calls `telemetry::CrashReport::load_latest()` and formats output. If no crash report found, display a message saying so along with the crash reports directory path.

- [ ] **Step 4: Wire into the TUI**

In the TUI's slash command handler (in `main.rs` around the `handle_slash_command` call), add a match arm for `CrashReport` that displays the formatted crash report.

- [ ] **Step 5: Verify compiles**

Run: `cd rust && cargo check -p icode-cli`
Expected: clean compilation

---

### Task 6: Wire Panic Hook into Server Binary

**Files:**
- Modify: `rust/crates/icode-server/Cargo.toml`
- Modify: `rust/crates/icode-server/src/main.rs`
- Create: `rust/crates/icode-server/src/routes/diagnostics.rs`
- Modify: `rust/crates/icode-server/src/lib.rs`

- [ ] **Step 1: Add telemetry dependency**

Add to `rust/crates/icode-server/Cargo.toml`:
```toml
telemetry = { path = "../telemetry" }
```

- [ ] **Step 2: Wire panic hook in server main.rs**

At the top of `main()`, before `tracing_subscriber::registry()...`:
```rust
let mut crash_context = std::collections::HashMap::new();
crash_context.insert("binary".to_string(), "icode-server".to_string());
telemetry::install_panic_hook(crash_context);
```

- [ ] **Step 3: Create diagnostics route**

Create `rust/crates/icode-server/src/routes/diagnostics.rs`:
```rust
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct CrashReportResponse {
    pub has_crash_report: bool,
    pub report: Option<telemetry::CrashReport>,
}

pub async fn get_crash_report() -> Json<CrashReportResponse> {
    match telemetry::CrashReport::load_latest() {
        Ok(Some(report)) => Json(CrashReportResponse {
            has_crash_report: true,
            report: Some(report),
        }),
        _ => Json(CrashReportResponse {
            has_crash_report: false,
            report: None,
        }),
    }
}

#[derive(Serialize)]
pub struct DiagnosticResponse {
    pub snapshot: telemetry::DiagnosticSnapshot,
}

pub async fn get_diagnostics() -> Json<DiagnosticResponse> {
    let snapshot = telemetry::DiagnosticSnapshot::capture();
    Json(DiagnosticResponse { snapshot })
}
```

- [ ] **Step 4: Export and wire routes**

In `rust/crates/icode-server/src/lib.rs`, add:
```rust
pub mod routes;
pub use routes::diagnostics::{get_crash_report, get_diagnostics};
```

In `main.rs`, add to the router:
```rust
.route("/diagnostics", get(icode_server::get_diagnostics))
.route("/crash-report", get(icode_server::get_crash_report))
```

- [ ] **Step 5: Verify compiles**

Run: `cd rust && cargo check -p icode-server`
Expected: clean compilation

---

### Task 7: End-to-End Verification

- [ ] **Step 1: Full workspace build**

Run: `cd rust && cargo build --workspace`
Expected: clean build

- [ ] **Step 2: Full workspace tests**

Run: `cd rust && cargo test --workspace`
Expected: all tests pass

- [ ] **Step 3: Clippy check**

Run: `cd rust && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings

- [ ] **Step 4: Format check**

Run: `cd rust && cargo fmt --all -- --check`
Expected: no formatting issues

- [ ] **Step 5: Test crash report CLI**

Run: `cargo run -p icode-cli -- --crash-report`
Expected: "No crash reports found" message with directory path

- [ ] **Step 6: Test panic hook (manual)**

Create a test that triggers a panic and verify crash report is written:
```bash
cd rust && cargo test -p telemetry test_crash_report_persists_and_loads
```

---

### Task 8: Add Tracing Instrumentation to Key Runtime Paths

**Files:**
- Modify: `rust/crates/runtime/src/conversation.rs`
- Modify: `rust/crates/runtime/src/bash.rs`
- Modify: `rust/crates/runtime/src/file_ops.rs`

- [ ] **Step 1: Add tracing spans to conversation turn execution**

In `conversation.rs`, at the top of `execute_turn` or equivalent method, add:
```rust
let span = tracing::info_span!("turn_execute", session_id = %self.session.id, turn = self.session.turns);
let _enter = span.enter();
tracing::info!("starting turn execution");
```

On success:
```rust
tracing::info!(input_tokens = summary.input_tokens, output_tokens = summary.output_tokens, "turn completed");
```

On failure:
```rust
tracing::error!(error = %error, "turn failed");
```

- [ ] **Step 2: Add tracing to bash execution**

In `bash.rs`, add spans around command execution:
```rust
tracing::info!(command = %cmd, timeout_secs = timeout, "executing bash command");
// ... execution ...
tracing::info!(exit_code = code, stdout_bytes = stdout.len(), "bash command completed");
```

- [ ] **Step 3: Add tracing to file operations**

In `file_ops.rs`, add spans:
```rust
tracing::info!(path = %path, "reading file");
tracing::info!(path = %path, bytes = content.len(), "file read completed");
```

For write/edit, include size and permission mode in the span.

- [ ] **Step 4: Verify tracing compiles**

Run: `cd rust && cargo clippy -p runtime -- -D warnings`
Expected: clean

---

## Summary of Capabilities After Implementation

| Capability | Location | Description |
|---|---|---|
| Crash report on panic | `telemetry::install_panic_hook()` | Captures backtrace, system info, env vars (API keys masked), persists to ~/.icode/crash-reports/ |
| View last crash | `icode --crash-report` | Prints human-readable crash summary |
| View last crash | `/crash-report` | In-TUI slash command |
| View last crash | `GET /crash-report` | Server HTTP endpoint |
| Full diagnostic | `GET /diagnostics` | Server endpoint with system, runtime, config, session, log info |
| Structured logs | `~/.icode/logs/icode.YYYY-MM-DD` | JSON-formatted daily log files |
| Log rotation | `tracing-appender::rolling::daily` | Automatic daily rotation |
| Tool execution tracing | runtime crate spans | Every turn, bash, file op traced |
