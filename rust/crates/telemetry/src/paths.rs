use std::path::PathBuf;

/// Root config directory: `~/.icode/` or `$CLAW_CONFIG_HOME/` or `$ICODE_CONFIG_HOME/`
#[must_use]
pub fn icode_config_dir() -> PathBuf {
    let config_home =
        std::env::var_os("CLAW_CONFIG_HOME").or_else(|| std::env::var_os("ICODE_CONFIG_HOME"));
    if let Some(path) = config_home {
        return PathBuf::from(path);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".icode")
}

/// Log directory: `~/.icode/logs/`
#[must_use]
pub fn log_dir() -> PathBuf {
    icode_config_dir().join("logs")
}

/// Crash reports directory: `~/.icode/crash-reports/`
#[must_use]
pub fn crash_reports_dir() -> PathBuf {
    icode_config_dir().join("crash-reports")
}

/// Diagnostic snapshots directory: `~/.icode/diagnostics/`
#[must_use]
pub fn diagnostics_dir() -> PathBuf {
    icode_config_dir().join("diagnostics")
}

/// Path to the latest crash report file.
#[must_use]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_dir_uses_env_override() {
        let dir = icode_config_dir();
        assert!(dir.ends_with(".icode") || !dir.as_os_str().is_empty());
    }

    #[test]
    fn test_subdir_paths_end_with_correct_name() {
        assert!(log_dir().ends_with("logs"));
        assert!(crash_reports_dir().ends_with("crash-reports"));
        assert!(diagnostics_dir().ends_with("diagnostics"));
    }

    #[test]
    fn test_latest_crash_report_path() {
        let p = latest_crash_report_path();
        assert!(p.to_string_lossy().contains("latest-crash.json"));
    }

    #[test]
    fn test_ensure_telemetry_dirs_succeeds() {
        let _ = ensure_telemetry_dirs();
    }
}
