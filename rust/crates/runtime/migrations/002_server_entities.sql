CREATE TABLE IF NOT EXISTS tasks (
    task_id TEXT PRIMARY KEY, prompt TEXT NOT NULL, description TEXT,
    status TEXT NOT NULL DEFAULT 'pending', created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL, output TEXT, team_id TEXT
);
CREATE TABLE IF NOT EXISTS teams (
    team_id TEXT PRIMARY KEY, name TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS crons (
    cron_id TEXT PRIMARY KEY, schedule TEXT NOT NULL, prompt TEXT NOT NULL,
    description TEXT, enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
    last_run_at TEXT, run_count INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS mcp_servers (
    server_name TEXT PRIMARY KEY, status TEXT NOT NULL,
    server_info TEXT, error_message TEXT
);
CREATE TABLE IF NOT EXISTS workers (
    worker_id TEXT PRIMARY KEY, cwd TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'idle',
    trust_auto_resolve INTEGER NOT NULL DEFAULT 0,
    trust_gate_cleared INTEGER NOT NULL DEFAULT 0,
    auto_recover_prompt_misdelivery INTEGER NOT NULL DEFAULT 0,
    prompt_delivery_attempts INTEGER NOT NULL DEFAULT 0,
    last_prompt TEXT, replay_prompt TEXT, last_error_json TEXT,
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS lsp_servers (
    language TEXT PRIMARY KEY, status TEXT NOT NULL,
    root_path TEXT, capabilities_json TEXT
);
