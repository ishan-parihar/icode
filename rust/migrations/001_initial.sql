-- 001_initial.sql: Core schema for iCode SQLite persistence layer.
-- Covers sessions, messages, tasks, teams, cron, workers, MCP, and LSP.

CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT UNIQUE NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    compaction_count INTEGER DEFAULT 0,
    compaction_removed INTEGER DEFAULT 0,
    compaction_summary TEXT DEFAULT '',
    fork_parent_id TEXT,
    fork_branch_name TEXT
);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    message_index INTEGER NOT NULL,
    role TEXT NOT NULL,
    content_json TEXT NOT NULL,
    usage_input_tokens INTEGER,
    usage_output_tokens INTEGER,
    usage_cache_create INTEGER,
    usage_cache_read INTEGER,
    UNIQUE(session_id, message_index)
);

CREATE TABLE IF NOT EXISTS tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT UNIQUE NOT NULL,
    prompt TEXT NOT NULL,
    description TEXT,
    task_packet_json TEXT,
    status TEXT NOT NULL DEFAULT 'created',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    output TEXT DEFAULT '',
    team_id TEXT
);

CREATE TABLE IF NOT EXISTS task_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    timestamp INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS teams (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'created',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS team_tasks (
    team_id TEXT NOT NULL REFERENCES teams(team_id) ON DELETE CASCADE,
    task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    PRIMARY KEY (team_id, task_id)
);

CREATE TABLE IF NOT EXISTS cron_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    cron_id TEXT UNIQUE NOT NULL,
    schedule TEXT NOT NULL,
    prompt TEXT NOT NULL,
    description TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    last_run_at INTEGER,
    run_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS workers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    worker_id TEXT UNIQUE NOT NULL,
    cwd TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'spawning',
    trust_auto_resolve INTEGER NOT NULL DEFAULT 0,
    trust_gate_cleared INTEGER NOT NULL DEFAULT 0,
    auto_recover_prompt_misdelivery INTEGER NOT NULL DEFAULT 0,
    prompt_delivery_attempts INTEGER NOT NULL DEFAULT 0,
    last_prompt TEXT,
    replay_prompt TEXT,
    last_error_json TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS worker_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    worker_id TEXT NOT NULL REFERENCES workers(worker_id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    detail TEXT,
    timestamp INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS mcp_servers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    server_name TEXT UNIQUE NOT NULL,
    status TEXT NOT NULL DEFAULT 'disconnected',
    server_info TEXT,
    error_message TEXT
);

CREATE TABLE IF NOT EXISTS mcp_tools (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    server_name TEXT NOT NULL REFERENCES mcp_servers(server_name) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    input_schema_json TEXT
);

CREATE TABLE IF NOT EXISTS mcp_resources (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    server_name TEXT NOT NULL REFERENCES mcp_servers(server_name) ON DELETE CASCADE,
    uri TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    mime_type TEXT
);

CREATE TABLE IF NOT EXISTS lsp_servers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    language TEXT UNIQUE NOT NULL,
    status TEXT NOT NULL DEFAULT 'disconnected',
    root_path TEXT,
    capabilities_json TEXT
);

CREATE TABLE IF NOT EXISTS lsp_diagnostics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    language TEXT NOT NULL REFERENCES lsp_servers(language) ON DELETE CASCADE,
    path TEXT NOT NULL,
    line INTEGER NOT NULL,
    character INTEGER NOT NULL,
    severity TEXT NOT NULL,
    message TEXT NOT NULL,
    source TEXT
);
