use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

/// Initialize structured file + optional stderr logging.
///
/// Log files are written to ~/.icode/logs/ with daily rotation.
/// Control level via RUST_LOG env var (default: info for file, no stderr if unset).
/// Use RUST_LOG=debug for full TUI diagnostics.
pub fn init_logging() {
    let log_dir = icode_log_dir();
    std::fs::create_dir_all(&log_dir).ok();

    // File appender: daily rotation
    let file_appender = tracing_appender::rolling::daily(&log_dir, "icode");

    // JSON file layer (machine-readable, for post-mortem analysis)
    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
        .with_writer(file_appender)
        .with_ansi(false);

    // Stderr layer: human-readable, only if RUST_LOG is explicitly set
    let stderr_layer = std::env::var("RUST_LOG")
        .ok()
        .and_then(|s| EnvFilter::try_new(s).ok())
        .map(|filter| {
            tracing_subscriber::fmt::layer()
                .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
                .with_writer(std::io::stderr)
                .with_ansi(true)
                .with_filter(filter)
        });

    // Always apply info-level filter to the file layer
    let info_filter = EnvFilter::try_new("info").unwrap_or_else(|_| EnvFilter::default());

    if let Some(sl) = stderr_layer {
        tracing_subscriber::registry()
            .with(file_layer.with_filter(info_filter))
            .with(sl)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(file_layer.with_filter(info_filter))
            .init();
    }

    tracing::info!(
        log_dir = %log_dir.display(),
        version = env!("CARGO_PKG_VERSION"),
        "icode logging initialized"
    );
}

/// Return the log directory path (~/.icode/logs/).
pub fn icode_log_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("CLAW_CONFIG_HOME") {
        return PathBuf::from(path).join("logs");
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".icode").join("logs")
}

/// Clean up log files older than max_days.
pub fn cleanup_old_logs(max_days: u64) {
    let log_dir = icode_log_dir();
    let Ok(entries) = std::fs::read_dir(&log_dir) else {
        return;
    };
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(max_days * 86400))
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    for entry in entries.flatten() {
        if let Ok(meta) = entry.metadata() {
            if let Ok(modified) = meta.modified() {
                if modified < cutoff {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}
