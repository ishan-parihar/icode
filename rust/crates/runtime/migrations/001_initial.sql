CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL UNIQUE,
    version INTEGER NOT NULL DEFAULT 1, created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL, compaction_count INTEGER,
    compaction_removed INTEGER, compaction_summary TEXT,
    fork_parent_id TEXT, fork_branch_name TEXT
);
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    message_index INTEGER NOT NULL, role TEXT NOT NULL, content_json TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch('subsec')),
    usage_input_tokens INTEGER, usage_output_tokens INTEGER,
    usage_cache_create INTEGER, usage_cache_read INTEGER,
    UNIQUE(session_id, message_index)
);
