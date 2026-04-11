use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::fs::{self, OpenOptions};
use std::io::Write as IoWrite;
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

fn get_hostname() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(name) = std::fs::read_to_string("/etc/hostname") {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    std::env::var("HOSTNAME").ok()
}

impl SystemInfo {
    #[must_use]
    pub fn capture() -> Self {
        let mut env_vars = HashMap::new();
        for key in &[
            "RUST_LOG",
            "RUST_BACKTRACE",
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "GEMINI_API_KEY",
            "CLAW_CONFIG_HOME",
            "ICODE_CONFIG_HOME",
            "TERM",
            "COLORTERM",
            "LANG",
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
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            hostname: get_hostname(),
            cwd: std::env::current_dir()
                .map_or_else(|_| "<unknown>".to_string(), |p| p.display().to_string()),
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
    /// Note: The backtrace is captured unconditionally (not gated by `RUST_BACKTRACE`).
    #[must_use]
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
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let latest = latest_crash_report_path();
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&latest)?;
        file.write_all(json.as_bytes())?;
        let now = Utc::now();
        let ts = now.format("%Y%m%d-%H%M%S");
        let millis = now.timestamp_subsec_millis();
        let copy_path = crash_reports_dir().join(format!("crash-{ts}-{millis:03}.json"));
        fs::write(&copy_path, &json)?;
        Ok(copy_path)
    }

    pub fn load_latest() -> Result<Option<Self>, std::io::Error> {
        let path = latest_crash_report_path();
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)?;
        let report: Self = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Some(report))
    }

    #[must_use]
    pub fn summary(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "=== Crash Report ===\n");
        let _ = writeln!(out, "Timestamp:     {}", self.timestamp);
        let _ = writeln!(out, "PID:           {}", self.pid);
        let _ = writeln!(out, "Thread:        {}", self.thread_name);
        let _ = writeln!(out, "Version:       {}", self.app_version);
        if let Some(sha) = &self.git_sha {
            let _ = writeln!(out, "Git SHA:       {sha}");
        }
        if let Some(sid) = &self.session_id {
            let _ = writeln!(out, "Session:       {sid}");
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "OS:            {}", self.system_info.os);
        let _ = writeln!(out, "CWD:           {}", self.system_info.cwd);
        let _ = writeln!(out);
        let _ = writeln!(out, "Panic:         {}", self.panic_message);
        if let Some(loc) = &self.panic_location {
            let _ = writeln!(out, "Location:      {loc}");
        }
        let _ = writeln!(out, "\n--- Backtrace (abbreviated) ---");
        let frames: Vec<&str> = self.backtrace.lines().collect();
        let show = frames.len().min(20);
        for frame in &frames[..show] {
            let _ = writeln!(out, "{frame}");
        }
        if frames.len() > show {
            let _ = writeln!(
                out,
                "\n... and {} more frames (see full report in file)",
                frames.len() - show
            );
        }
        if !self.context.is_empty() {
            let _ = writeln!(out, "\n--- Context ---");
            for (k, v) in &self.context {
                let _ = writeln!(out, "{k}: {v}");
            }
        }
        out
    }
}

#[expect(clippy::implicit_hasher)]
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
            let panic_loc = info
                .location()
                .map(|loc| (loc.file(), loc.line(), loc.column()));
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
    fn test_crash_report_serialization_roundtrip() {
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
