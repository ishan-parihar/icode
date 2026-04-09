use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the AutoDream background consolidation system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoDreamConfig {
    /// Minimum hours that must elapse between consolidation runs.
    pub min_hours_between_consolidation: u64,
    /// Minimum sessions that must accumulate before triggering a dream.
    pub min_sessions_before_consolidation: u32,
    /// Maximum number of durable memories to retain after pruning.
    pub max_memories: usize,
}

/// Returns sensible defaults for AutoDream configuration.
///
/// - 24 hours between consolidations
/// - 5 sessions before triggering
/// - 20 maximum memories
#[must_use]
pub fn default_auto_dream_config() -> AutoDreamConfig {
    AutoDreamConfig {
        min_hours_between_consolidation: 24,
        min_sessions_before_consolidation: 5,
        max_memories: 20,
    }
}

// ---------------------------------------------------------------------------
// Consolidation state
// ---------------------------------------------------------------------------

/// Persistent state tracked across dream cycles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationState {
    /// ISO-8601 timestamp of the last successful consolidation, or `None`.
    pub last_consolidation: Option<String>,
    /// Number of sessions completed since last consolidation.
    pub session_count: u32,
    /// Total number of consolidations performed.
    pub consolidation_count: u32,
}

impl Default for ConsolidationState {
    fn default() -> Self {
        Self {
            last_consolidation: None,
            session_count: 0,
            consolidation_count: 0,
        }
    }
}

/// Dream phase determines which prompt is sent to the LLM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DreamPhase {
    /// Identify patterns and recurring decisions from recent sessions.
    Orient,
    /// Extract key learnings worth preserving.
    Gather,
    /// Create or update specific, actionable memory entries.
    Consolidate,
    /// Prune stale memories and reindex.
    PruneAndIndex,
}

// ---------------------------------------------------------------------------
// Dream gating
// ---------------------------------------------------------------------------

/// Evaluate whether a dream cycle should be triggered.
///
/// Three gates must all pass:
/// 1. **Time gate** — hours since last consolidation >= `min_hours_between_consolidation`
/// 2. **Session gate** — `session_count >= min_sessions_before_consolidation`
/// 3. **Lock gate** — no concurrent consolidation (dream lock not held)
#[must_use]
pub fn should_trigger_dream(state: &ConsolidationState, config: &AutoDreamConfig) -> bool {
    let time_gate = hours_since_consolidation(state) >= config.min_hours_between_consolidation;
    let session_gate = state.session_count >= config.min_sessions_before_consolidation;
    let lock_gate = acquire_dream_lock().is_ok();

    if lock_gate {
        if let Err(e) = release_dream_lock() {
            eprintln!("auto_dream: failed to release dream lock: {e}");
        }
    }

    time_gate && session_gate && lock_gate
}

fn hours_since_consolidation(state: &ConsolidationState) -> u64 {
    let Some(ref iso_ts) = state.last_consolidation else {
        // Never consolidated — treat as infinitely long ago.
        return u64::MAX;
    };
    let Ok(epoch_secs) = parse_iso8601_to_epoch_secs(iso_ts) else {
        return u64::MAX;
    };
    let now_secs = current_epoch_secs();
    let elapsed = now_secs.saturating_sub(epoch_secs);
    elapsed / 3600
}

// ---------------------------------------------------------------------------
// Dream lock
// ---------------------------------------------------------------------------

const DREAM_LOCK_STALE_SECS: u64 = 3600; // 1 hour

/// Acquire the dream lock by writing a lock file containing PID, timestamp,
/// and hostname.  Returns an error if the lock is already held by a *live*
/// process (non-stale).
pub fn acquire_dream_lock() -> Result<(), String> {
    let lock_path = dream_lock_path();

    if lock_path.exists() {
        if let Ok(contents) = fs::read_to_string(&lock_path) {
            if !is_lock_stale(&contents) {
                return Err("dream lock already held by active process".to_string());
            }
            // Stale lock — remove it.
            let _ = fs::remove_file(&lock_path);
        }
    }

    let pid = std::process::id();
    let timestamp = current_epoch_secs();
    let hostname = current_hostname();
    let lock_contents = format!("{pid}:{timestamp}:{hostname}");

    fs::write(&lock_path, &lock_contents)
        .map_err(|e| format!("failed to write dream lock: {e}"))?;

    Ok(())
}

/// Release the dream lock by removing the lock file.
pub fn release_dream_lock() -> Result<(), String> {
    let lock_path = dream_lock_path();
    fs::remove_file(&lock_path).map_err(|e| format!("failed to release dream lock: {e}"))
}

/// Check whether a lock file's contents represent a stale lock.
///
/// A lock is stale when the timestamp is older than `DREAM_LOCK_STALE_SECS`.
#[must_use]
pub fn is_lock_stale(lock_contents: &str) -> bool {
    let parts: Vec<&str> = lock_contents.splitn(3, ':').collect();
    let Some(timestamp_str) = parts.get(1) else {
        // Malformed lock — treat as stale so we can overwrite it.
        return true;
    };
    let Ok(lock_epoch) = timestamp_str.parse::<u64>() else {
        return true;
    };
    let now = current_epoch_secs();
    now.saturating_sub(lock_epoch) > DREAM_LOCK_STALE_SECS
}

// ---------------------------------------------------------------------------
// State persistence
// ---------------------------------------------------------------------------

/// Load consolidation state from `~/.icode/.dream_state.json`.
///
/// If the file does not exist or cannot be read, returns the default state.
pub fn load_consolidation_state() -> Result<ConsolidationState, String> {
    let state_path = dream_state_path();
    match fs::read_to_string(&state_path) {
        Ok(contents) => {
            serde_json::from_str(&contents).map_err(|e| format!("failed to parse dream state: {e}"))
        }
        Err(_) => {
            // Missing or unreadable state file — start from defaults.
            Ok(ConsolidationState::default())
        }
    }
}

/// Persist consolidation state to `~/.icode/.dream_state.json`.
pub fn save_consolidation_state(state: &ConsolidationState) -> Result<(), String> {
    let state_path = dream_state_path();
    if let Some(parent) = state_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create dream state directory: {e}"))?;
    }
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| format!("failed to serialize dream state: {e}"))?;
    fs::write(&state_path, json).map_err(|e| format!("failed to write dream state: {e}"))
}

/// Increment the session count, persist the updated state, and return the new count.
pub fn increment_session_count() -> Result<u32, String> {
    let mut state = load_consolidation_state()?;
    state.session_count = state.session_count.saturating_add(1);
    save_consolidation_state(&state)?;
    Ok(state.session_count)
}

// ---------------------------------------------------------------------------
// Prompt building
// ---------------------------------------------------------------------------

/// Build a phase-specific consolidation prompt from session summaries and
/// existing memories.
#[must_use]
pub fn build_dream_consolidation_prompt(
    phase: DreamPhase,
    session_summaries: &[String],
    existing_memories: &[String],
) -> String {
    let directive = match phase {
        DreamPhase::Orient => {
            "Review these recent session summaries. Identify patterns, recurring decisions, and areas of focus."
        }
        DreamPhase::Gather => {
            "Extract key learnings, decisions, and patterns worth preserving as durable memories."
        }
        DreamPhase::Consolidate => {
            "Create or update memory entries. Each memory should be specific, actionable, and non-obvious."
        }
        DreamPhase::PruneAndIndex => {
            "Remove stale memories. Update outdated ones. Ensure the memory index is current and organized."
        }
    };

    let mut prompt = String::new();
    prompt.push_str("# AutoDream: ");
    prompt.push_str(phase_label(phase));
    prompt.push_str("\n\n");
    prompt.push_str("## Directive\n");
    prompt.push_str(directive);
    prompt.push_str("\n\n");

    if !session_summaries.is_empty() {
        prompt.push_str("## Recent Session Summaries\n\n");
        for (i, summary) in session_summaries.iter().enumerate() {
            prompt.push_str(&format!("### Session {}\n{}\n\n", i + 1, summary));
        }
    }

    if !existing_memories.is_empty() {
        prompt.push_str("## Existing Memories\n\n");
        for (i, memory) in existing_memories.iter().enumerate() {
            prompt.push_str(&format!("### Memory {}\n{}\n\n", i + 1, memory));
        }
    }

    prompt
}

#[must_use]
fn phase_label(phase: DreamPhase) -> &'static str {
    match phase {
        DreamPhase::Orient => "Orient",
        DreamPhase::Gather => "Gather",
        DreamPhase::Consolidate => "Consolidate",
        DreamPhase::PruneAndIndex => "Prune & Index",
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

#[must_use]
fn icode_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".icode"))
        .unwrap_or_else(|| PathBuf::from(".icode"))
}

#[must_use]
fn dream_lock_path() -> PathBuf {
    icode_dir().join(".dream_lock")
}

#[must_use]
fn dream_state_path() -> PathBuf {
    icode_dir().join(".dream_state.json")
}

// ---------------------------------------------------------------------------
// Time helpers
// ---------------------------------------------------------------------------

#[must_use]
fn current_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Parse an ISO-8601 timestamp string to epoch seconds.
///
/// Supports the subset `YYYY-MM-DDTHH:MM:SSZ` and `YYYY-MM-DDTHH:MM:SS.sssZ`.
fn parse_iso8601_to_epoch_secs(iso: &str) -> Result<u64, String> {
    // Use a simple parser for the common ISO-8601 format.
    // Format: YYYY-MM-DDTHH:MM:SSZ or YYYY-MM-DDTHH:MM:SS+HH:MM
    let s = iso.trim();
    if s.is_empty() {
        return Err("empty timestamp".to_string());
    }

    // Try chrono-like manual parsing for the basic UTC format.
    // We handle: YYYY-MM-DDTHH:MM:SSZ
    let stripped = s.trim_end_matches('Z').trim_end_matches("+00:00");

    let datetime_parts: Vec<&str> = stripped.split('T').collect();
    if datetime_parts.len() != 2 {
        return Err(format!("invalid ISO-8601 format: {iso}"));
    }

    let date_parts: Vec<&str> = datetime_parts[0].split('-').collect();
    if date_parts.len() != 3 {
        return Err(format!("invalid date in ISO-8601: {iso}"));
    }

    let year: i64 = date_parts[0]
        .parse()
        .map_err(|_| format!("invalid year in ISO-8601: {iso}"))?;
    let month: u32 = date_parts[1]
        .parse()
        .map_err(|_| format!("invalid month in ISO-8601: {iso}"))?;
    let day: u32 = date_parts[2]
        .parse()
        .map_err(|_| format!("invalid day in ISO-8601: {iso}"))?;

    let time_str = datetime_parts[1];
    // Strip any subsecond portion
    let time_base = time_str.split('.').next().unwrap_or(time_str);
    let time_parts: Vec<&str> = time_base.split(':').collect();
    if time_parts.len() < 2 {
        return Err(format!("invalid time in ISO-8601: {iso}"));
    }

    let hour: u32 = time_parts[0]
        .parse()
        .map_err(|_| format!("invalid hour in ISO-8601: {iso}"))?;
    let minute: u32 = time_parts[1]
        .parse()
        .map_err(|_| format!("invalid minute in ISO-8601: {iso}"))?;
    let second: u32 = time_parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    // Calculate epoch seconds using a simplified algorithm
    // Days from year 0 to the given date
    let days_from_civil = days_from_civil(year, month, day);
    let epoch_days = days_from_civil - 719_468; // Days from year 0 to Unix epoch (1970-01-01)

    let epoch_secs = (epoch_days as i64) * 86_400
        + (hour as i64) * 3600
        + (minute as i64) * 60
        + (second as i64);

    u64::try_from(epoch_secs).map_err(|_| format!("timestamp out of range: {iso}"))
}

/// Calculate the number of days from year 0 to the given civil date.
/// Uses the algorithm from Howard Hinnant's civil_from_days.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let mut y = year;
    let m = month as i64;
    let d = day as i64;

    // Adjust for months before March
    if m <= 2 {
        y -= 1;
    }
    let era = (y - if y >= 0 { 0 } else { 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468 + 719_468
}

#[must_use]
fn current_hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Config defaults ----

    #[test]
    fn default_config_has_expected_values() {
        let config = default_auto_dream_config();
        assert_eq!(config.min_hours_between_consolidation, 24);
        assert_eq!(config.min_sessions_before_consolidation, 5);
        assert_eq!(config.max_memories, 20);
    }

    // ---- Gate checks ----

    #[test]
    fn should_trigger_when_all_gates_pass() {
        let state = ConsolidationState {
            last_consolidation: Some("2024-01-01T00:00:00Z".to_string()),
            session_count: 10,
            consolidation_count: 3,
        };
        let config = default_auto_dream_config();
        // Time gate: well over 24h; session gate: 10 >= 5; lock gate: should succeed
        assert!(should_trigger_dream(&state, &config));
    }

    #[test]
    fn should_not_trigger_when_session_count_too_low() {
        let state = ConsolidationState {
            last_consolidation: Some("2024-01-01T00:00:00Z".to_string()),
            session_count: 2,
            consolidation_count: 0,
        };
        let config = default_auto_dream_config();
        assert!(!should_trigger_dream(&state, &config));
    }

    #[test]
    fn should_trigger_when_never_consolidated_and_enough_sessions() {
        let state = ConsolidationState {
            last_consolidation: None,
            session_count: 5,
            consolidation_count: 0,
        };
        let config = default_auto_dream_config();
        // Time gate: u64::MAX >= 24; session gate: 5 >= 5; lock gate: should succeed
        assert!(should_trigger_dream(&state, &config));
    }

    #[test]
    fn should_not_trigger_when_session_count_below_threshold() {
        let state = ConsolidationState {
            last_consolidation: None,
            session_count: 4,
            consolidation_count: 0,
        };
        let config = default_auto_dream_config();
        assert!(!should_trigger_dream(&state, &config));
    }

    // ---- Lock staleness ----

    #[test]
    fn fresh_lock_is_not_stale() {
        let now = current_epoch_secs();
        let lock = format!("1234:{now}:myhost");
        assert!(!is_lock_stale(&lock));
    }

    #[test]
    fn old_lock_is_stale() {
        let old = current_epoch_secs() - 7200; // 2 hours ago
        let lock = format!("1234:{old}:myhost");
        assert!(is_lock_stale(&lock));
    }

    #[test]
    fn malformed_lock_is_stale() {
        assert!(is_lock_stale("garbage"));
        assert!(is_lock_stale(""));
        assert!(is_lock_stale("1234:notanumber:host"));
    }

    // ---- State load / save ----

    #[test]
    fn load_state_returns_defaults_when_file_missing() {
        // Point to a non-existent path via HOME override
        let temp_dir = std::env::temp_dir();
        let fake_home = temp_dir.join("auto_dream_test_missing_state");
        let _ = fs::remove_dir_all(&fake_home);

        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &fake_home);

        let state = load_consolidation_state().expect("should return default state");
        assert_eq!(state.session_count, 0);
        assert!(state.last_consolidation.is_none());

        // Restore
        restore_home(original_home);
        let _ = fs::remove_dir_all(&fake_home);
    }

    #[test]
    fn save_and_load_state_round_trip() {
        let temp_dir = std::env::temp_dir();
        let fake_home = temp_dir.join("auto_dream_test_round_trip");
        let _ = fs::remove_dir_all(&fake_home);

        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &fake_home);

        let state = ConsolidationState {
            last_consolidation: Some("2024-06-15T12:00:00Z".to_string()),
            session_count: 7,
            consolidation_count: 2,
        };
        save_consolidation_state(&state).expect("save should succeed");
        let loaded = load_consolidation_state().expect("load should succeed");
        assert_eq!(loaded.session_count, 7);
        assert_eq!(loaded.consolidation_count, 2);
        assert_eq!(
            loaded.last_consolidation.as_deref(),
            Some("2024-06-15T12:00:00Z")
        );

        restore_home(original_home);
        let _ = fs::remove_dir_all(&fake_home);
    }

    #[test]
    fn increment_session_count_persists() {
        let temp_dir = std::env::temp_dir();
        let fake_home = temp_dir.join("auto_dream_test_increment");
        let _ = fs::remove_dir_all(&fake_home);

        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &fake_home);

        // Start fresh
        let _ = fs::remove_file(dream_state_path());
        let count = increment_session_count().expect("increment should succeed");
        assert_eq!(count, 1);

        let count2 = increment_session_count().expect("second increment should succeed");
        assert_eq!(count2, 2);

        restore_home(original_home);
        let _ = fs::remove_dir_all(&fake_home);
    }

    // ---- Prompt building ----

    #[test]
    fn orient_prompt_contains_expected_directive() {
        let prompt = build_dream_consolidation_prompt(DreamPhase::Orient, &[], &[]);
        assert!(prompt.contains("Orient"));
        assert!(prompt.contains("Identify patterns, recurring decisions, and areas of focus"));
    }

    #[test]
    fn gather_prompt_contains_expected_directive() {
        let prompt = build_dream_consolidation_prompt(DreamPhase::Gather, &[], &[]);
        assert!(prompt.contains("Gather"));
        assert!(prompt.contains("Extract key learnings, decisions, and patterns worth preserving"));
    }

    #[test]
    fn consolidate_prompt_contains_expected_directive() {
        let prompt = build_dream_consolidation_prompt(DreamPhase::Consolidate, &[], &[]);
        assert!(prompt.contains("Consolidate"));
        assert!(prompt.contains("specific, actionable, and non-obvious"));
    }

    #[test]
    fn prune_and_index_prompt_contains_expected_directive() {
        let prompt = build_dream_consolidation_prompt(DreamPhase::PruneAndIndex, &[], &[]);
        assert!(prompt.contains("Prune"));
        assert!(prompt.contains("Remove stale memories"));
        assert!(prompt.contains("Ensure the memory index is current and organized"));
    }

    #[test]
    fn prompt_includes_session_summaries() {
        let summaries = vec!["Summary A".to_string(), "Summary B".to_string()];
        let prompt = build_dream_consolidation_prompt(DreamPhase::Gather, &summaries, &[]);
        assert!(prompt.contains("Session 1"));
        assert!(prompt.contains("Summary A"));
        assert!(prompt.contains("Session 2"));
        assert!(prompt.contains("Summary B"));
    }

    #[test]
    fn prompt_includes_existing_memories() {
        let memories = vec!["Memory X".to_string()];
        let prompt = build_dream_consolidation_prompt(DreamPhase::Consolidate, &[], &memories);
        assert!(prompt.contains("Memory 1"));
        assert!(prompt.contains("Memory X"));
    }

    // ---- Lock acquire / release ----

    #[test]
    fn acquire_and_release_lock_round_trip() {
        let temp_dir = std::env::temp_dir();
        let fake_home = temp_dir.join("auto_dream_test_lock");
        let _ = fs::remove_dir_all(&fake_home);

        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &fake_home);

        acquire_dream_lock().expect("acquire should succeed");
        assert!(dream_lock_path().exists());
        release_dream_lock().expect("release should succeed");
        assert!(!dream_lock_path().exists());

        restore_home(original_home);
        let _ = fs::remove_dir_all(&fake_home);
    }

    #[test]
    fn cannot_acquire_when_lock_held() {
        let temp_dir = std::env::temp_dir();
        let fake_home = temp_dir.join("auto_dream_test_lock_held");
        let _ = fs::remove_dir_all(&fake_home);

        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &fake_home);

        acquire_dream_lock().expect("first acquire should succeed");
        let result = acquire_dream_lock();
        assert!(result.is_err(), "second acquire should fail");

        release_dream_lock().expect("cleanup release should succeed");
        restore_home(original_home);
        let _ = fs::remove_dir_all(&fake_home);
    }

    #[test]
    fn stale_lock_is_replaced_on_acquire() {
        let temp_dir = std::env::temp_dir();
        let fake_home = temp_dir.join("auto_dream_test_stale");
        let _ = fs::remove_dir_all(&fake_home);

        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &fake_home);

        // Write a stale lock
        fs::create_dir_all(icode_dir()).expect("create icode dir");
        let old_timestamp = current_epoch_secs() - 7200;
        fs::write(dream_lock_path(), format!("9999:{old_timestamp}:oldhost"))
            .expect("write stale lock");

        // Acquiring should succeed by overwriting the stale lock
        acquire_dream_lock().expect("acquire after stale should succeed");
        release_dream_lock().expect("release should succeed");

        restore_home(original_home);
        let _ = fs::remove_dir_all(&fake_home);
    }

    // ---- ISO-8601 parsing ----

    #[test]
    fn parses_iso8601_utc() {
        let epoch = parse_iso8601_to_epoch_secs("2024-01-15T12:00:00Z").expect("valid timestamp");
        // 2024-01-15 12:00:00 UTC = 1705320000
        assert_eq!(epoch, 1_705_320_000);
    }

    #[test]
    fn rejects_empty_timestamp() {
        assert!(parse_iso8601_to_epoch_secs("").is_err());
    }

    fn restore_home(original: Option<std::ffi::OsString>) {
        match original {
            Some(val) => std::env::set_var("HOME", val),
            None => std::env::remove_var("HOME"),
        }
    }
}
