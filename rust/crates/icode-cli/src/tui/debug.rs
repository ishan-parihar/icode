use std::path::PathBuf;
use std::sync::Mutex;
use tracing_subscriber::{
    layer::SubscriberExt, reload, util::SubscriberInitExt, EnvFilter, Layer,
};

static RELOAD_HANDLE: Mutex<Option<reload::Handle<EnvFilter, tracing_subscriber::Registry>>> =
    Mutex::new(None);

/// Initialize structured file + optional stderr logging with runtime reload support.
///
/// Log files are written to ~/.icode/logs/ with daily rotation.
/// Control level via RUST_LOG env var (default: info for file, no stderr if unset).
/// Use RUST_LOG=debug for full TUI diagnostics.
/// At runtime, call [reload_log_level] to change the file filter without restarting.
///
/// # Arguments
/// * `log_level` - Log level for the file layer. One of: "error", "warn", "info", "debug", "trace".
///   Defaults to "info" if None.
pub fn init_logging(log_level: Option<&str>) {
    // Skip if already initialized in a parent process
    if std::env::var("ICODE_LOGGING_INITIALIZED").is_ok() {
        return;
    }

    // Clean up old log files first
    cleanup_old_logs(7);

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

    // Use configurable log level for the file layer (defaults to "info")
    let active_level = log_level.unwrap_or("info");
    let level_filter = EnvFilter::try_new(active_level).unwrap_or_else(|_| EnvFilter::default());

    // Create a reload layer so we can change the file log level at runtime
    let (file_filter_layer, reload_handle) = reload::Layer::new(level_filter);

    if let Some(sl) = stderr_layer {
        tracing_subscriber::registry()
            .with(file_layer.with_filter(file_filter_layer))
            .with(sl)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(file_layer.with_filter(file_filter_layer))
            .init();
    }

    // Store the reload handle for later use by /debug command
    if let Ok(mut guard) = RELOAD_HANDLE.lock() {
        *guard = Some(reload_handle);
    }

    tracing::info!(
        pid = std::process::id(),
        log_dir = %log_dir.display(),
        version = env!("CARGO_PKG_VERSION"),
        log_level = active_level,
        "icode logging initialized"
    );
}

/// Reload the file log level at runtime.
/// Returns an error message if the level string is invalid or logging is not initialized.
pub fn reload_log_level(level: &str) -> Result<(), String> {
    let new_filter =
        EnvFilter::try_new(level).map_err(|e| format!("Invalid log level '{level}': {e}"))?;

    let guard = RELOAD_HANDLE
        .lock()
        .map_err(|e| format!("Failed to acquire reload lock: {e}"))?;

    match guard.as_ref() {
        Some(handle) => handle
            .reload(new_filter)
            .map_err(|e| format!("Failed to reload filter: {e}")),
        None => Err("Logging not initialized yet".to_string()),
    }
}

/// Check whether the subscriber was initialised with a reload handle.
pub fn is_reload_available() -> bool {
    RELOAD_HANDLE
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
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
