use std::fmt::Write as FmtWrite;
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
    pub workspace: WorkspaceDiagnostic,
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
    pub pid: u32,
    pub binary_path: String,
    pub cwd: String,
    pub env_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDiagnostic {
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
        let config_dir = crate::paths::icode_config_dir();
        let log_dir = crate::paths::log_dir();
        let crash_dir = crate::paths::crash_reports_dir();

        let system = SystemDiagnostic {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            hostname: get_hostname(),
            cpu_count: num_cpus::get(),
            memory: capture_memory_info(),
            disk: DiskInfo {
                config_dir_mb: dir_size_mb(&config_dir),
                log_files_count: count_files(&log_dir),
                crash_reports_count: count_files(&crash_dir),
            },
        };

        let runtime = RuntimeDiagnostic {
            pid: std::process::id(),
            binary_path: std::env::current_exe()
                .map_or_else(|_| "<unknown>".to_string(), |p| p.display().to_string()),
            cwd: std::env::current_dir()
                .map_or_else(|_| "<unknown>".to_string(), |p| p.display().to_string()),
            env_keys: std::env::vars()
                .map(|(k, _)| k)
                .filter(|k| {
                    !k.contains("KEY")
                        && !k.contains("SECRET")
                        && !k.contains("TOKEN")
                        && !k.contains("PASSWORD")
                        && !k.contains("CREDENTIAL")
                })
                .collect(),
        };

        let config_files = list_dir_entries(&config_dir);
        let cwd = std::env::current_dir().unwrap_or_default();
        let has_claude_md = cwd.join("CLAUDE.md").exists();
        let has_opencode_json = cwd.join(".opencode.json").exists();

        let workspace = WorkspaceDiagnostic {
            config_dir: config_dir.display().to_string(),
            config_files,
            has_claude_md,
            has_opencode_json,
        };

        let sessions_dir = config_dir.join("sessions");
        let sessions = capture_session_info(&sessions_dir);
        let logs = capture_log_info(&log_dir);

        Self {
            timestamp: Utc::now().to_rfc3339(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            git_sha: option_env!("GIT_SHA").map(String::from),
            system,
            runtime,
            workspace,
            sessions,
            logs,
        }
    }

    pub fn persist(&self) -> Result<PathBuf, std::io::Error> {
        ensure_telemetry_dirs()?;
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let now = Utc::now();
        let ts = now.format("%Y%m%d-%H%M%S");
        let millis = now.timestamp_subsec_millis();
        let path = diagnostics_dir().join(format!("diagnostic-{ts}-{millis:03}.json"));
        fs::write(&path, &json)?;
        Ok(path)
    }

    #[must_use]
    pub fn summary(&self) -> String {
        let mut out = String::new();
        writeln!(out, "=== Diagnostic Snapshot ===\n").ok();
        writeln!(out, "Timestamp:    {}", self.timestamp).ok();
        writeln!(out, "Version:      {}", self.version).ok();
        if let Some(sha) = &self.git_sha {
            writeln!(out, "Git SHA:      {sha}").ok();
        }
        writeln!(out).ok();
        writeln!(out, "--- System ---").ok();
        writeln!(out, "OS:           {}", self.system.os).ok();
        writeln!(out, "CPU cores:    {}", self.system.cpu_count).ok();
        if let Some(rss) = self.system.memory.process_rss_mb {
            writeln!(out, "Process RSS:  {rss} MB").ok();
        }
        writeln!(
            out,
            "Config dir:   {} MB",
            self.system.disk.config_dir_mb.unwrap_or(0)
        )
        .ok();
        writeln!(out, "Log files:    {}", self.system.disk.log_files_count).ok();
        writeln!(
            out,
            "Crash reports: {}",
            self.system.disk.crash_reports_count
        )
        .ok();
        writeln!(out).ok();
        writeln!(out, "--- Sessions ---").ok();
        writeln!(out, "Session count: {}", self.sessions.session_count).ok();
        if let Some(latest) = &self.sessions.latest_session {
            writeln!(
                out,
                "Latest:        {} ({} bytes)",
                latest.path, latest.size_bytes
            )
            .ok();
        }
        writeln!(out).ok();
        writeln!(out, "--- Logs ---").ok();
        writeln!(out, "Log dir:       {}", self.logs.log_dir).ok();
        for log in &self.logs.log_files {
            writeln!(
                out,
                "  {} ({} bytes, {})",
                log.name, log.size_bytes, log.modified
            )
            .ok();
        }
        out
    }
}

/// Cross-platform hostname retrieval without the hostname crate.
fn get_hostname() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/etc/hostname") {
            let trimmed = content.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
}

fn capture_memory_info() -> MemoryInfo {
    let mut info = MemoryInfo {
        total_mb: 0,
        available_mb: 0,
        process_rss_mb: None,
    };

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

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("sysctl").arg("-n").arg("hw.memsize").output() {
            if let Ok(s) = String::from_utf8(output.stdout) {
                if let Ok(val) = s.trim().parse::<u64>() {
                    info.total_mb = val / (1024 * 1024);
                }
            }
        }
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

fn dir_size_mb(dir: &std::path::Path) -> Option<u64> {
    if !dir.exists() {
        return None;
    }
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(dir)
        .max_depth(5)
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file() {
            total += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    Some(total / (1024 * 1024))
}

fn count_files(dir: &std::path::Path) -> usize {
    if !dir.exists() {
        return 0;
    }
    std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(Result::ok).count())
        .unwrap_or(0)
}

fn list_dir_entries(dir: &std::path::Path) -> Vec<String> {
    if !dir.exists() {
        return vec![];
    }
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
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
                    let modified = meta.modified().map_or_else(
                        |_| "unknown".to_string(),
                        |t| {
                            chrono::DateTime::<Utc>::from(t)
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string()
                        },
                    );
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
                    let modified = meta.modified().map_or_else(
                        |_| "unknown".to_string(),
                        |t| {
                            chrono::DateTime::<Utc>::from(t)
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string()
                        },
                    );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_snapshot_has_required_fields() {
        let snap = DiagnosticSnapshot::capture();
        assert!(!snap.timestamp.is_empty());
        assert!(!snap.version.is_empty());
        assert!(!snap.system.os.is_empty());
        assert!(snap.system.cpu_count > 0);
        assert!(!snap.runtime.binary_path.is_empty());
        assert!(!snap.runtime.cwd.is_empty());
    }

    #[test]
    fn test_diagnostic_snapshot_persists() {
        let snap = DiagnosticSnapshot::capture();
        let path = snap.persist().expect("persist should succeed");
        assert!(path.exists());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_diagnostic_summary_is_nonempty() {
        let snap = DiagnosticSnapshot::capture();
        let summary = snap.summary();
        assert!(summary.contains("Diagnostic Snapshot"));
        assert!(summary.contains("System"));
    }
}
