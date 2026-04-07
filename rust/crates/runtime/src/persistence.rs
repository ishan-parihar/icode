use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags, OptionalExtension, Result as RusqliteResult};
use serde::{Deserialize, Serialize};

const MIGRATION_SQL: &str = include_str!("../../../migrations/001_initial.sql");
const MIGRATION_002_EVENTS: &str = include_str!("../../../migrations/002_events.sql");

#[derive(Debug)]
pub enum PersistenceError {
    Db(rusqlite::Error),
    Io(std::io::Error),
    Format(String),
    NotFound(String),
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Db(err) => write!(f, "database error: {err}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Format(msg) => write!(f, "{msg}"),
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
        }
    }
}

impl std::error::Error for PersistenceError {}

impl From<rusqlite::Error> for PersistenceError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Db(err)
    }
}

impl From<std::io::Error> for PersistenceError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

pub type Result<T> = std::result::Result<T, PersistenceError>;

// ── Data Transfer Types ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRow {
    pub id: i64,
    pub session_id: String,
    pub version: i64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub compaction_count: i64,
    pub compaction_removed: i64,
    pub compaction_summary: String,
    pub fork_parent_id: Option<String>,
    pub fork_branch_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRow {
    pub id: i64,
    pub session_id: String,
    pub message_index: i64,
    pub role: String,
    pub content_json: String,
    pub usage_input_tokens: Option<i64>,
    pub usage_output_tokens: Option<i64>,
    pub usage_cache_create: Option<i64>,
    pub usage_cache_read: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRow {
    pub id: i64,
    pub task_id: String,
    pub prompt: String,
    pub description: Option<String>,
    pub task_packet_json: Option<String>,
    pub status: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub output: String,
    pub team_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMessageRow {
    pub id: i64,
    pub task_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamRow {
    pub id: i64,
    pub team_id: String,
    pub name: String,
    pub status: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronRow {
    pub id: i64,
    pub cron_id: String,
    pub schedule: String,
    pub prompt: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub last_run_at: Option<u64>,
    pub run_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerRow {
    pub id: i64,
    pub worker_id: String,
    pub cwd: String,
    pub status: String,
    pub trust_auto_resolve: bool,
    pub trust_gate_cleared: bool,
    pub auto_recover_prompt_misdelivery: bool,
    pub prompt_delivery_attempts: i64,
    pub last_prompt: Option<String>,
    pub replay_prompt: Option<String>,
    pub last_error_json: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerEventRow {
    pub id: i64,
    pub worker_id: String,
    pub seq: i64,
    pub kind: String,
    pub status: String,
    pub detail: Option<String>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerRow {
    pub id: i64,
    pub server_name: String,
    pub status: String,
    pub server_info: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolRow {
    pub id: i64,
    pub server_name: String,
    pub name: String,
    pub description: Option<String>,
    pub input_schema_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceRow {
    pub id: i64,
    pub server_name: String,
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerRow {
    pub id: i64,
    pub language: String,
    pub status: String,
    pub root_path: Option<String>,
    pub capabilities_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspDiagnosticRow {
    pub id: i64,
    pub language: String,
    pub path: String,
    pub line: i64,
    pub character: i64,
    pub severity: String,
    pub message: String,
    pub source: Option<String>,
}

// ── SqliteStore ──────────────────────────────────────────────────────

pub struct SqliteStore {
    conn: Connection,
    db_path: PathBuf,
}

impl SqliteStore {
    pub fn new(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;

        let store = Self {
            conn,
            db_path: db_path.to_path_buf(),
        };
        store.set_pragmas()?;
        Ok(store)
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn,
            db_path: PathBuf::from(":memory:"),
        };
        store.set_pragmas()?;
        Ok(store)
    }

    #[must_use]
    pub fn default_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".icode").join("icode.db")
    }

    fn set_pragmas(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA busy_timeout=5000;
            PRAGMA foreign_keys=ON;
            PRAGMA synchronous=NORMAL;
            ",
        )?;
        Ok(())
    }

    pub fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(MIGRATION_SQL)?;
        self.conn.execute_batch(MIGRATION_002_EVENTS)?;
        Ok(())
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    // ── Session CRUD ─────────────────────────────────────────────

    pub fn create_session(
        &self,
        session_id: &str,
        version: i64,
        created_at_ms: u64,
        updated_at_ms: u64,
        fork_parent_id: Option<&str>,
        fork_branch_name: Option<&str>,
    ) -> Result<SessionRow> {
        self.conn.execute(
            "INSERT INTO sessions (session_id, version, created_at_ms, updated_at_ms, fork_parent_id, fork_branch_name)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                session_id, version, created_at_ms.cast_signed(), updated_at_ms.cast_signed(),
                fork_parent_id, fork_branch_name
            ],
        )?;
        self.get_session(session_id)?
            .ok_or_else(|| PersistenceError::Format("session created but not found".to_string()))
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<SessionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, version, created_at_ms, updated_at_ms,
                    compaction_count, compaction_removed, compaction_summary,
                    fork_parent_id, fork_branch_name
             FROM sessions WHERE session_id = ?1",
        )?;
        let row = stmt
            .query_row(rusqlite::params![session_id], |r| {
                Ok(SessionRow {
                    id: r.get(0)?,
                    session_id: r.get(1)?,
                    version: r.get(2)?,
                    created_at_ms: u64_from_i64(r.get(3)?),
                    updated_at_ms: u64_from_i64(r.get(4)?),
                    compaction_count: r.get(5)?,
                    compaction_removed: r.get(6)?,
                    compaction_summary: r.get(7)?,
                    fork_parent_id: r.get(8)?,
                    fork_branch_name: r.get(9)?,
                })
            })
            .optional()?;
        Ok(row)
    }

    pub fn update_session(
        &self,
        session_id: &str,
        updated_at_ms: u64,
        compaction_count: i64,
        compaction_removed: i64,
        compaction_summary: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET updated_at_ms = ?1, compaction_count = ?2,
             compaction_removed = ?3, compaction_summary = ?4
             WHERE session_id = ?5",
            rusqlite::params![
                updated_at_ms.cast_signed(),
                compaction_count,
                compaction_removed,
                compaction_summary,
                session_id
            ],
        )?;
        Ok(())
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, version, created_at_ms, updated_at_ms,
                    compaction_count, compaction_removed, compaction_summary,
                    fork_parent_id, fork_branch_name
             FROM sessions ORDER BY updated_at_ms DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(SessionRow {
                id: r.get(0)?,
                session_id: r.get(1)?,
                version: r.get(2)?,
                created_at_ms: u64_from_i64(r.get(3)?),
                updated_at_ms: u64_from_i64(r.get(4)?),
                compaction_count: r.get(5)?,
                compaction_removed: r.get(6)?,
                compaction_summary: r.get(7)?,
                fork_parent_id: r.get(8)?,
                fork_branch_name: r.get(9)?,
            })
        })?;
        rows.collect::<RusqliteResult<Vec<_>>>().map_err(Into::into)
    }

    pub fn delete_session(&self, session_id: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "DELETE FROM sessions WHERE session_id = ?1",
            rusqlite::params![session_id],
        )?;
        Ok(changed > 0)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn save_message(
        &self,
        session_id: &str,
        message_index: i64,
        role: &str,
        content_json: &str,
        usage_input_tokens: Option<i64>,
        usage_output_tokens: Option<i64>,
        usage_cache_create: Option<i64>,
        usage_cache_read: Option<i64>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO messages (session_id, message_index, role, content_json,
             usage_input_tokens, usage_output_tokens, usage_cache_create, usage_cache_read)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                session_id,
                message_index,
                role,
                content_json,
                usage_input_tokens,
                usage_output_tokens,
                usage_cache_create,
                usage_cache_read
            ],
        )?;
        Ok(())
    }

    pub fn get_messages(&self, session_id: &str) -> Result<Vec<MessageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, message_index, role, content_json,
                    usage_input_tokens, usage_output_tokens, usage_cache_create, usage_cache_read
             FROM messages WHERE session_id = ?1 ORDER BY message_index ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![session_id], |r| {
            Ok(MessageRow {
                id: r.get(0)?,
                session_id: r.get(1)?,
                message_index: r.get(2)?,
                role: r.get(3)?,
                content_json: r.get(4)?,
                usage_input_tokens: r.get(5)?,
                usage_output_tokens: r.get(6)?,
                usage_cache_create: r.get(7)?,
                usage_cache_read: r.get(8)?,
            })
        })?;
        rows.collect::<RusqliteResult<Vec<_>>>().map_err(Into::into)
    }

    // ── Task CRUD ────────────────────────────────────────────────

    pub fn create_task(
        &self,
        task_id: &str,
        prompt: &str,
        description: Option<&str>,
        task_packet_json: Option<&str>,
        status: &str,
        team_id: Option<&str>,
    ) -> Result<TaskRow> {
        let ts = now_secs();
        self.conn.execute(
            "INSERT INTO tasks (task_id, prompt, description, task_packet_json, status, created_at, updated_at, team_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                task_id, prompt, description, task_packet_json, status,
                ts.cast_signed(), ts.cast_signed(), team_id
            ],
        )?;
        self.get_task(task_id)?
            .ok_or_else(|| PersistenceError::Format("task created but not found".to_string()))
    }

    pub fn get_task(&self, task_id: &str) -> Result<Option<TaskRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, task_id, prompt, description, task_packet_json, status,
                    created_at, updated_at, output, team_id
             FROM tasks WHERE task_id = ?1",
        )?;
        let row = stmt
            .query_row(rusqlite::params![task_id], |r| {
                Ok(TaskRow {
                    id: r.get(0)?,
                    task_id: r.get(1)?,
                    prompt: r.get(2)?,
                    description: r.get(3)?,
                    task_packet_json: r.get(4)?,
                    status: r.get(5)?,
                    created_at: u64_from_i64(r.get(6)?),
                    updated_at: u64_from_i64(r.get(7)?),
                    output: r.get(8)?,
                    team_id: r.get(9)?,
                })
            })
            .optional()?;
        Ok(row)
    }

    pub fn update_task(
        &self,
        task_id: &str,
        status: Option<&str>,
        output: Option<&str>,
        team_id: Option<&str>,
    ) -> Result<()> {
        let ts = now_secs();
        let mut updates = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(s) = status {
            updates.push("status = ?");
            params.push(Box::new(s.to_string()));
        }
        if let Some(o) = output {
            updates.push("output = ?");
            params.push(Box::new(o.to_string()));
        }
        if let Some(t) = team_id {
            updates.push("team_id = ?");
            params.push(Box::new(t.to_string()));
        }
        updates.push("updated_at = ?");
        params.push(Box::new(ts.cast_signed()));
        params.push(Box::new(task_id.to_string()));

        let set_clause = updates.join(", ");
        let sql = format!("UPDATE tasks SET {set_clause} WHERE task_id = ?");
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(AsRef::as_ref).collect();
        self.conn
            .execute(&sql, rusqlite::params_from_iter(params_refs))?;
        Ok(())
    }

    pub fn list_tasks(&self, status_filter: Option<&str>) -> Result<Vec<TaskRow>> {
        let (sql, params_val): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match status_filter
        {
            Some(s) => (
                "SELECT id, task_id, prompt, description, task_packet_json, status,
                 created_at, updated_at, output, team_id
                 FROM tasks WHERE status = ?1 ORDER BY created_at DESC"
                    .to_string(),
                vec![Box::new(s.to_string())],
            ),
            None => (
                "SELECT id, task_id, prompt, description, task_packet_json, status,
                 created_at, updated_at, output, team_id
                 FROM tasks ORDER BY created_at DESC"
                    .to_string(),
                vec![],
            ),
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_val.iter().map(AsRef::as_ref).collect();
        let rows = stmt.query_map(rusqlite::params_from_iter(params_refs), |r| {
            Ok(TaskRow {
                id: r.get(0)?,
                task_id: r.get(1)?,
                prompt: r.get(2)?,
                description: r.get(3)?,
                task_packet_json: r.get(4)?,
                status: r.get(5)?,
                created_at: u64_from_i64(r.get(6)?),
                updated_at: u64_from_i64(r.get(7)?),
                output: r.get(8)?,
                team_id: r.get(9)?,
            })
        })?;
        rows.collect::<RusqliteResult<Vec<_>>>().map_err(Into::into)
    }

    pub fn delete_task(&self, task_id: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "DELETE FROM tasks WHERE task_id = ?1",
            rusqlite::params![task_id],
        )?;
        Ok(changed > 0)
    }

    // ── Team CRUD ────────────────────────────────────────────────

    pub fn create_team(&self, team_id: &str, name: &str) -> Result<TeamRow> {
        let ts = now_secs();
        self.conn.execute(
            "INSERT INTO teams (team_id, name, status, created_at, updated_at)
             VALUES (?1, ?2, 'created', ?3, ?4)",
            rusqlite::params![team_id, name, ts.cast_signed(), ts.cast_signed()],
        )?;
        self.get_team(team_id)?
            .ok_or_else(|| PersistenceError::Format("team created but not found".to_string()))
    }

    pub fn get_team(&self, team_id: &str) -> Result<Option<TeamRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, team_id, name, status, created_at, updated_at
             FROM teams WHERE team_id = ?1",
        )?;
        let row = stmt
            .query_row(rusqlite::params![team_id], |r| {
                Ok(TeamRow {
                    id: r.get(0)?,
                    team_id: r.get(1)?,
                    name: r.get(2)?,
                    status: r.get(3)?,
                    created_at: u64_from_i64(r.get(4)?),
                    updated_at: u64_from_i64(r.get(5)?),
                })
            })
            .optional()?;
        Ok(row)
    }

    pub fn list_teams(&self) -> Result<Vec<TeamRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, team_id, name, status, created_at, updated_at
             FROM teams ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(TeamRow {
                id: r.get(0)?,
                team_id: r.get(1)?,
                name: r.get(2)?,
                status: r.get(3)?,
                created_at: u64_from_i64(r.get(4)?),
                updated_at: u64_from_i64(r.get(5)?),
            })
        })?;
        rows.collect::<RusqliteResult<Vec<_>>>().map_err(Into::into)
    }

    pub fn delete_team(&self, team_id: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "DELETE FROM teams WHERE team_id = ?1",
            rusqlite::params![team_id],
        )?;
        Ok(changed > 0)
    }

    pub fn add_task_to_team(&self, team_id: &str, task_id: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO team_tasks (team_id, task_id) VALUES (?1, ?2)",
            rusqlite::params![team_id, task_id],
        )?;
        Ok(())
    }

    // ── Cron CRUD ────────────────────────────────────────────────

    pub fn create_cron(
        &self,
        cron_id: &str,
        schedule: &str,
        prompt: &str,
        description: Option<&str>,
    ) -> Result<CronRow> {
        let ts = now_secs();
        self.conn.execute(
            "INSERT INTO cron_entries (cron_id, schedule, prompt, description, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6)",
            rusqlite::params![cron_id, schedule, prompt, description, ts.cast_signed(), ts.cast_signed()],
        )?;
        self.get_cron(cron_id)?
            .ok_or_else(|| PersistenceError::Format("cron created but not found".to_string()))
    }

    pub fn get_cron(&self, cron_id: &str) -> Result<Option<CronRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, cron_id, schedule, prompt, description, enabled,
                    created_at, updated_at, last_run_at, run_count
             FROM cron_entries WHERE cron_id = ?1",
        )?;
        let row = stmt
            .query_row(rusqlite::params![cron_id], |r| {
                Ok(CronRow {
                    id: r.get(0)?,
                    cron_id: r.get(1)?,
                    schedule: r.get(2)?,
                    prompt: r.get(3)?,
                    description: r.get(4)?,
                    enabled: i64_to_bool(r.get(5)?),
                    created_at: u64_from_i64(r.get(6)?),
                    updated_at: u64_from_i64(r.get(7)?),
                    last_run_at: r.get::<_, Option<i64>>(8)?.map(u64_from_i64),
                    run_count: r.get(9)?,
                })
            })
            .optional()?;
        Ok(row)
    }

    pub fn list_crons(&self) -> Result<Vec<CronRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, cron_id, schedule, prompt, description, enabled,
                    created_at, updated_at, last_run_at, run_count
             FROM cron_entries ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(CronRow {
                id: r.get(0)?,
                cron_id: r.get(1)?,
                schedule: r.get(2)?,
                prompt: r.get(3)?,
                description: r.get(4)?,
                enabled: i64_to_bool(r.get(5)?),
                created_at: u64_from_i64(r.get(6)?),
                updated_at: u64_from_i64(r.get(7)?),
                last_run_at: r.get::<_, Option<i64>>(8)?.map(u64_from_i64),
                run_count: r.get(9)?,
            })
        })?;
        rows.collect::<RusqliteResult<Vec<_>>>().map_err(Into::into)
    }

    pub fn update_cron(&self, cron_id: &str, enabled: Option<bool>) -> Result<()> {
        let ts = now_secs();
        if let Some(e) = enabled {
            self.conn.execute(
                "UPDATE cron_entries SET enabled = ?1, updated_at = ?2 WHERE cron_id = ?3",
                rusqlite::params![bool_to_i64(e), ts.cast_signed(), cron_id],
            )?;
        }
        Ok(())
    }

    pub fn delete_cron(&self, cron_id: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "DELETE FROM cron_entries WHERE cron_id = ?1",
            rusqlite::params![cron_id],
        )?;
        Ok(changed > 0)
    }

    pub fn record_cron_run(&self, cron_id: &str) -> Result<()> {
        let ts = now_secs();
        self.conn.execute(
            "UPDATE cron_entries SET last_run_at = ?1, run_count = run_count + 1, updated_at = ?2
             WHERE cron_id = ?3",
            rusqlite::params![ts.cast_signed(), ts.cast_signed(), cron_id],
        )?;
        Ok(())
    }

    // ── Worker CRUD ──────────────────────────────────────────────

    pub fn create_worker(
        &self,
        worker_id: &str,
        cwd: &str,
        trust_auto_resolve: bool,
        auto_recover_prompt_misdelivery: bool,
    ) -> Result<WorkerRow> {
        let ts = now_secs();
        self.conn.execute(
            "INSERT INTO workers (worker_id, cwd, status, trust_auto_resolve, trust_gate_cleared,
             auto_recover_prompt_misdelivery, prompt_delivery_attempts, created_at, updated_at)
             VALUES (?1, ?2, 'spawning', ?3, 0, ?4, 0, ?5, ?6)",
            rusqlite::params![
                worker_id,
                cwd,
                bool_to_i64(trust_auto_resolve),
                bool_to_i64(auto_recover_prompt_misdelivery),
                ts.cast_signed(),
                ts.cast_signed()
            ],
        )?;
        self.get_worker(worker_id)?
            .ok_or_else(|| PersistenceError::Format("worker created but not found".to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_worker(
        &self,
        worker_id: &str,
        status: Option<&str>,
        trust_gate_cleared: Option<bool>,
        last_prompt: Option<&str>,
        replay_prompt: Option<&str>,
        last_error_json: Option<&str>,
        prompt_delivery_attempts: Option<i64>,
    ) -> Result<()> {
        let ts = now_secs();
        let mut updates = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(s) = status {
            updates.push("status = ?");
            params.push(Box::new(s.to_string()));
        }
        if let Some(v) = trust_gate_cleared {
            updates.push("trust_gate_cleared = ?");
            params.push(Box::new(bool_to_i64(v)));
        }
        if let Some(v) = last_prompt {
            updates.push("last_prompt = ?");
            params.push(Box::new(v.to_string()));
        }
        if let Some(v) = replay_prompt {
            updates.push("replay_prompt = ?");
            params.push(Box::new(v.to_string()));
        }
        if let Some(v) = last_error_json {
            updates.push("last_error_json = ?");
            params.push(Box::new(v.to_string()));
        }
        if let Some(v) = prompt_delivery_attempts {
            updates.push("prompt_delivery_attempts = ?");
            params.push(Box::new(v));
        }
        updates.push("updated_at = ?");
        params.push(Box::new(ts.cast_signed()));
        params.push(Box::new(worker_id.to_string()));

        let set_clause = updates.join(", ");
        let sql = format!("UPDATE workers SET {set_clause} WHERE worker_id = ?");
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(AsRef::as_ref).collect();
        self.conn
            .execute(&sql, rusqlite::params_from_iter(params_refs))?;
        Ok(())
    }

    pub fn get_worker(&self, worker_id: &str) -> Result<Option<WorkerRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, worker_id, cwd, status, trust_auto_resolve, trust_gate_cleared,
                    auto_recover_prompt_misdelivery, prompt_delivery_attempts,
                    last_prompt, replay_prompt, last_error_json, created_at, updated_at
             FROM workers WHERE worker_id = ?1",
        )?;
        let row = stmt
            .query_row(rusqlite::params![worker_id], |r| {
                Ok(WorkerRow {
                    id: r.get(0)?,
                    worker_id: r.get(1)?,
                    cwd: r.get(2)?,
                    status: r.get(3)?,
                    trust_auto_resolve: i64_to_bool(r.get(4)?),
                    trust_gate_cleared: i64_to_bool(r.get(5)?),
                    auto_recover_prompt_misdelivery: i64_to_bool(r.get(6)?),
                    prompt_delivery_attempts: r.get(7)?,
                    last_prompt: r.get(8)?,
                    replay_prompt: r.get(9)?,
                    last_error_json: r.get(10)?,
                    created_at: u64_from_i64(r.get(11)?),
                    updated_at: u64_from_i64(r.get(12)?),
                })
            })
            .optional()?;
        Ok(row)
    }

    pub fn list_workers(&self) -> Result<Vec<WorkerRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, worker_id, cwd, status, trust_auto_resolve, trust_gate_cleared,
                    auto_recover_prompt_misdelivery, prompt_delivery_attempts,
                    last_prompt, replay_prompt, last_error_json, created_at, updated_at
             FROM workers ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(WorkerRow {
                id: r.get(0)?,
                worker_id: r.get(1)?,
                cwd: r.get(2)?,
                status: r.get(3)?,
                trust_auto_resolve: i64_to_bool(r.get(4)?),
                trust_gate_cleared: i64_to_bool(r.get(5)?),
                auto_recover_prompt_misdelivery: i64_to_bool(r.get(6)?),
                prompt_delivery_attempts: r.get(7)?,
                last_prompt: r.get(8)?,
                replay_prompt: r.get(9)?,
                last_error_json: r.get(10)?,
                created_at: u64_from_i64(r.get(11)?),
                updated_at: u64_from_i64(r.get(12)?),
            })
        })?;
        rows.collect::<RusqliteResult<Vec<_>>>().map_err(Into::into)
    }

    pub fn add_worker_event(
        &self,
        worker_id: &str,
        seq: i64,
        kind: &str,
        status: &str,
        detail: Option<&str>,
    ) -> Result<()> {
        let ts = now_secs();
        self.conn.execute(
            "INSERT INTO worker_events (worker_id, seq, kind, status, detail, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![worker_id, seq, kind, status, detail, ts.cast_signed()],
        )?;
        Ok(())
    }

    // ── MCP CRUD ─────────────────────────────────────────────────

    pub fn upsert_mcp_server(
        &self,
        server_name: &str,
        status: &str,
        server_info: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<McpServerRow> {
        self.conn.execute(
            "INSERT INTO mcp_servers (server_name, status, server_info, error_message)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(server_name) DO UPDATE SET
                 status = excluded.status,
                 server_info = excluded.server_info,
                 error_message = excluded.error_message",
            rusqlite::params![server_name, status, server_info, error_message],
        )?;
        self.get_mcp_server(server_name)?.ok_or_else(|| {
            PersistenceError::Format("mcp server upserted but not found".to_string())
        })
    }

    pub fn get_mcp_server(&self, server_name: &str) -> Result<Option<McpServerRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, server_name, status, server_info, error_message
             FROM mcp_servers WHERE server_name = ?1",
        )?;
        let row = stmt
            .query_row(rusqlite::params![server_name], |r| {
                Ok(McpServerRow {
                    id: r.get(0)?,
                    server_name: r.get(1)?,
                    status: r.get(2)?,
                    server_info: r.get(3)?,
                    error_message: r.get(4)?,
                })
            })
            .optional()?;
        Ok(row)
    }

    pub fn list_mcp_servers(&self) -> Result<Vec<McpServerRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, server_name, status, server_info, error_message
             FROM mcp_servers ORDER BY server_name ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(McpServerRow {
                id: r.get(0)?,
                server_name: r.get(1)?,
                status: r.get(2)?,
                server_info: r.get(3)?,
                error_message: r.get(4)?,
            })
        })?;
        rows.collect::<RusqliteResult<Vec<_>>>().map_err(Into::into)
    }

    pub fn upsert_mcp_tool(
        &self,
        server_name: &str,
        name: &str,
        description: Option<&str>,
        input_schema_json: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO mcp_tools (server_name, name, description, input_schema_json)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT DO NOTHING",
            rusqlite::params![server_name, name, description, input_schema_json],
        )?;
        Ok(())
    }

    pub fn list_mcp_tools(&self, server_name: &str) -> Result<Vec<McpToolRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, server_name, name, description, input_schema_json
             FROM mcp_tools WHERE server_name = ?1 ORDER BY name ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![server_name], |r| {
            Ok(McpToolRow {
                id: r.get(0)?,
                server_name: r.get(1)?,
                name: r.get(2)?,
                description: r.get(3)?,
                input_schema_json: r.get(4)?,
            })
        })?;
        rows.collect::<RusqliteResult<Vec<_>>>().map_err(Into::into)
    }

    // ── LSP CRUD ─────────────────────────────────────────────────

    pub fn upsert_lsp_server(
        &self,
        language: &str,
        status: &str,
        root_path: Option<&str>,
        capabilities_json: Option<&str>,
    ) -> Result<LspServerRow> {
        self.conn.execute(
            "INSERT INTO lsp_servers (language, status, root_path, capabilities_json)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(language) DO UPDATE SET
                 status = excluded.status,
                 root_path = excluded.root_path,
                 capabilities_json = excluded.capabilities_json",
            rusqlite::params![language, status, root_path, capabilities_json],
        )?;
        self.get_lsp_server(language)?.ok_or_else(|| {
            PersistenceError::Format("lsp server upserted but not found".to_string())
        })
    }

    pub fn get_lsp_server(&self, language: &str) -> Result<Option<LspServerRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, language, status, root_path, capabilities_json
             FROM lsp_servers WHERE language = ?1",
        )?;
        let row = stmt
            .query_row(rusqlite::params![language], |r| {
                Ok(LspServerRow {
                    id: r.get(0)?,
                    language: r.get(1)?,
                    status: r.get(2)?,
                    root_path: r.get(3)?,
                    capabilities_json: r.get(4)?,
                })
            })
            .optional()?;
        Ok(row)
    }

    pub fn list_lsp_servers(&self) -> Result<Vec<LspServerRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, language, status, root_path, capabilities_json
             FROM lsp_servers ORDER BY language ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(LspServerRow {
                id: r.get(0)?,
                language: r.get(1)?,
                status: r.get(2)?,
                root_path: r.get(3)?,
                capabilities_json: r.get(4)?,
            })
        })?;
        rows.collect::<RusqliteResult<Vec<_>>>().map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_lsp_diagnostic(
        &self,
        language: &str,
        path: &str,
        line: i64,
        character: i64,
        severity: &str,
        message: &str,
        source: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO lsp_diagnostics (language, path, line, character, severity, message, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT DO NOTHING",
            rusqlite::params![language, path, line, character, severity, message, source],
        )?;
        Ok(())
    }

    pub fn get_lsp_diagnostics(
        &self,
        language: &str,
        path: Option<&str>,
    ) -> Result<Vec<LspDiagnosticRow>> {
        let (sql, params_val): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match path {
            Some(p) => (
                "SELECT id, language, path, line, character, severity, message, source
                 FROM lsp_diagnostics WHERE language = ?1 AND path = ?2 ORDER BY line ASC, character ASC"
                    .to_string(),
                vec![Box::new(language.to_string()), Box::new(p.to_string())],
            ),
            None => (
                "SELECT id, language, path, line, character, severity, message, source
                 FROM lsp_diagnostics WHERE language = ?1 ORDER BY line ASC, character ASC"
                    .to_string(),
                vec![Box::new(language.to_string())],
            ),
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_val.iter().map(AsRef::as_ref).collect();
        let rows = stmt.query_map(rusqlite::params_from_iter(params_refs), |r| {
            Ok(LspDiagnosticRow {
                id: r.get(0)?,
                language: r.get(1)?,
                path: r.get(2)?,
                line: r.get(3)?,
                character: r.get(4)?,
                severity: r.get(5)?,
                message: r.get(6)?,
                source: r.get(7)?,
            })
        })?;
        rows.collect::<RusqliteResult<Vec<_>>>().map_err(Into::into)
    }

    // ── Event Bus ────────────────────────────────────────────────

    pub fn insert_event(&self, session_id: &str, event_type: &str, event_data: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO events (session_id, event_type, event_data) VALUES (?1, ?2, ?3)",
            rusqlite::params![session_id, event_type, event_data],
        )?;
        Ok(())
    }

    // ── Event Read Queries ───────────────────────────────────

    pub fn get_events(
        &self,
        session_id: &str,
        since_id: Option<i64>,
        limit: usize,
    ) -> std::result::Result<Vec<EventRow>, rusqlite::Error> {
        let (sql, params_val): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match since_id {
            Some(id) => (
                "SELECT id, session_id, event_type, event_data, created_at \
                 FROM events WHERE session_id = ?1 AND id > ?2 ORDER BY id ASC LIMIT ?3"
                    .to_string(),
                vec![
                    Box::new(session_id.to_string()),
                    Box::new(id),
                    Box::new(limit.cast_signed()),
                ],
            ),
            None => (
                "SELECT id, session_id, event_type, event_data, created_at \
                 FROM events WHERE session_id = ?1 ORDER BY id ASC LIMIT ?2"
                    .to_string(),
                vec![
                    Box::new(session_id.to_string()),
                    Box::new(limit.cast_signed()),
                ],
            ),
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_val.iter().map(AsRef::as_ref).collect();
        let rows = stmt.query_map(rusqlite::params_from_iter(params_refs), |r| {
            Ok(EventRow {
                id: r.get(0)?,
                session_id: r.get(1)?,
                event_type: r.get(2)?,
                event_data: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_latest_event_id(
        &self,
        session_id: &str,
    ) -> std::result::Result<Option<i64>, rusqlite::Error> {
        self.conn
            .query_row(
                "SELECT id FROM events WHERE session_id = ?1 ORDER BY id DESC LIMIT 1",
                rusqlite::params![session_id],
                |r| r.get(0),
            )
            .optional()
    }

    pub fn get_events_since_id(
        &self,
        since_id: i64,
        limit: usize,
    ) -> std::result::Result<Vec<EventRow>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, event_type, event_data, created_at \
             FROM events WHERE id > ?1 ORDER BY id ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![since_id, limit.cast_signed()], |r| {
            Ok(EventRow {
                id: r.get(0)?,
                session_id: r.get(1)?,
                event_type: r.get(2)?,
                event_data: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?;
        rows.collect()
    }
}

// ── Event Row ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRow {
    pub id: i64,
    pub session_id: String,
    pub event_type: String,
    pub event_data: String,
    pub created_at: String,
}

// ── Helpers ──────────────────────────────────────────────────────────

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn u64_from_i64(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

fn i64_to_bool(v: i64) -> bool {
    v != 0
}

fn bool_to_i64(v: bool) -> i64 {
    i64::from(v)
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_db() -> (SqliteStore, PathBuf) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("icode-persistence-test-{nanos}.db"));
        let store = SqliteStore::new(&path).expect("store should open");
        store.migrate().expect("migration should succeed");
        (store, path)
    }

    #[test]
    fn default_path_returns_icode_db() {
        let path = SqliteStore::default_path();
        assert!(path.ends_with("icode.db"));
    }

    #[test]
    fn session_crud_roundtrip() {
        let (store, path) = temp_db();

        let created = store
            .create_session("sess-1", 1, 1000, 2000, None, None)
            .expect("create should succeed");
        assert_eq!(created.session_id, "sess-1");

        let fetched = store
            .get_session("sess-1")
            .expect("get should succeed")
            .expect("session should exist");
        assert_eq!(fetched.version, 1);

        store
            .update_session("sess-1", 3000, 1, 5, "compacted")
            .expect("update should succeed");

        let sessions = store.list_sessions().expect("list should succeed");
        assert_eq!(sessions.len(), 1);

        let deleted = store
            .delete_session("sess-1")
            .expect("delete should succeed");
        assert!(deleted);

        let gone = store.get_session("sess-1").expect("get should succeed");
        assert!(gone.is_none());

        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn session_with_fork_metadata() {
        let (store, path) = temp_db();

        let created = store
            .create_session(
                "sess-fork",
                1,
                1000,
                2000,
                Some("parent-1"),
                Some("branch-a"),
            )
            .expect("create should succeed");
        assert_eq!(created.fork_parent_id, Some("parent-1".to_string()));
        assert_eq!(created.fork_branch_name, Some("branch-a".to_string()));

        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn message_save_and_retrieve() {
        let (store, path) = temp_db();

        store
            .create_session("sess-msg", 1, 1000, 2000, None, None)
            .expect("create session");

        store
            .save_message(
                "sess-msg",
                0,
                "user",
                r#"[{"type":"text","text":"hello"}]"#,
                None,
                None,
                None,
                None,
            )
            .expect("save message");
        store
            .save_message(
                "sess-msg",
                1,
                "assistant",
                r#"[{"type":"text","text":"hi"}]"#,
                Some(10),
                Some(5),
                Some(1),
                Some(2),
            )
            .expect("save second message");

        let messages = store
            .get_messages("sess-msg")
            .expect("get messages should succeed");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].usage_input_tokens, Some(10));

        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn task_crud_roundtrip() {
        let (store, path) = temp_db();

        let task = store
            .create_task(
                "task-1",
                "do something",
                Some("a test"),
                None,
                "created",
                None,
            )
            .expect("create task");
        assert_eq!(task.task_id, "task-1");

        let fetched = store
            .get_task("task-1")
            .expect("get task")
            .expect("task exists");
        assert_eq!(fetched.prompt, "do something");

        store
            .update_task("task-1", Some("running"), None, None)
            .expect("update task");

        let all = store.list_tasks(None).expect("list tasks");
        assert_eq!(all.len(), 1);

        let running = store
            .list_tasks(Some("running"))
            .expect("list running tasks");
        assert_eq!(running.len(), 1);

        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn team_crud_roundtrip() {
        let (store, path) = temp_db();

        let team = store.create_team("team-1", "Alpha").expect("create team");
        assert_eq!(team.name, "Alpha");

        let fetched = store
            .get_team("team-1")
            .expect("get team")
            .expect("team exists");
        assert_eq!(fetched.team_id, "team-1");

        let teams = store.list_teams().expect("list teams");
        assert_eq!(teams.len(), 1);

        store
            .create_task("task-1", "team task", None, None, "created", None)
            .expect("create task for linking");
        store
            .add_task_to_team("team-1", "task-1")
            .expect("add task to team");

        let deleted = store.delete_team("team-1").expect("delete team");
        assert!(deleted);

        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn cron_crud_roundtrip() {
        let (store, path) = temp_db();

        let cron = store
            .create_cron("cron-1", "0 * * * *", "check status", Some("hourly"))
            .expect("create cron");
        assert_eq!(cron.schedule, "0 * * * *");
        assert!(cron.enabled);

        store
            .record_cron_run("cron-1")
            .expect("record run should succeed");
        store
            .record_cron_run("cron-1")
            .expect("second run should succeed");

        let fetched = store
            .get_cron("cron-1")
            .expect("get cron")
            .expect("cron exists");
        assert_eq!(fetched.run_count, 2);
        assert!(fetched.last_run_at.is_some());

        store
            .update_cron("cron-1", Some(false))
            .expect("disable cron");

        let disabled = store
            .get_cron("cron-1")
            .expect("get cron")
            .expect("cron exists");
        assert!(!disabled.enabled);

        let all = store.list_crons().expect("list crons");
        assert_eq!(all.len(), 1);

        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn worker_crud_roundtrip() {
        let (store, path) = temp_db();

        let worker = store
            .create_worker("w-1", "/tmp/work", true, false)
            .expect("create worker");
        assert_eq!(worker.worker_id, "w-1");
        assert!(worker.trust_auto_resolve);

        store
            .update_worker(
                "w-1",
                Some("ready_for_prompt"),
                None,
                Some("hello"),
                None,
                None,
                None,
            )
            .expect("update worker");

        let fetched = store
            .get_worker("w-1")
            .expect("get worker")
            .expect("worker exists");
        assert_eq!(fetched.status, "ready_for_prompt");
        assert_eq!(fetched.last_prompt, Some("hello".to_string()));

        store
            .add_worker_event("w-1", 1, "spawning", "spawning", Some("created"))
            .expect("add event");

        let workers = store.list_workers().expect("list workers");
        assert_eq!(workers.len(), 1);

        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn mcp_server_upsert_and_tools() {
        let (store, path) = temp_db();

        let server = store
            .upsert_mcp_server("github", "connected", Some("GitHub MCP v1"), None)
            .expect("upsert server");
        assert_eq!(server.server_name, "github");

        store
            .upsert_mcp_tool(
                "github",
                "create_issue",
                Some("Create an issue"),
                Some(r#"{"type":"object"}"#),
            )
            .expect("upsert tool");
        store
            .upsert_mcp_tool("github", "list_issues", Some("List issues"), None)
            .expect("upsert second tool");

        let tools = store.list_mcp_tools("github").expect("list tools");
        assert_eq!(tools.len(), 2);

        let updated = store
            .upsert_mcp_server("github", "error", None, Some("timeout"))
            .expect("update server status");
        assert_eq!(updated.status, "error");
        assert_eq!(updated.error_message, Some("timeout".to_string()));

        let servers = store.list_mcp_servers().expect("list servers");
        assert_eq!(servers.len(), 1);

        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn lsp_server_upsert_and_diagnostics() {
        let (store, path) = temp_db();

        let server = store
            .upsert_lsp_server(
                "rust",
                "connected",
                Some("/workspace"),
                Some(r#"["hover"]"#),
            )
            .expect("upsert lsp server");
        assert_eq!(server.language, "rust");

        store
            .upsert_lsp_diagnostic(
                "rust",
                "src/main.rs",
                10,
                5,
                "error",
                "mismatched types",
                Some("rust-analyzer"),
            )
            .expect("upsert diagnostic");
        store
            .upsert_lsp_diagnostic(
                "rust",
                "src/main.rs",
                15,
                3,
                "warning",
                "unused import",
                None,
            )
            .expect("upsert second diagnostic");

        let diags = store
            .get_lsp_diagnostics("rust", Some("src/main.rs"))
            .expect("get diagnostics");
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].severity, "error");

        let all_diags = store
            .get_lsp_diagnostics("rust", None)
            .expect("get all diagnostics");
        assert_eq!(all_diags.len(), 2);

        let servers = store.list_lsp_servers().expect("list lsp servers");
        assert_eq!(servers.len(), 1);

        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn delete_session_cascades_to_messages() {
        let (store, path) = temp_db();

        store
            .create_session("sess-cascade", 1, 1000, 2000, None, None)
            .expect("create session");
        store
            .save_message("sess-cascade", 0, "user", "[]", None, None, None, None)
            .expect("save message");

        store
            .delete_session("sess-cascade")
            .expect("delete session");

        let messages = store
            .get_messages("sess-cascade")
            .expect("get messages should succeed");
        assert!(messages.is_empty());

        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn get_events_returns_events_for_session() {
        let (store, path) = temp_db();

        store
            .create_session("s1", 1, 1000, 2000, None, None)
            .expect("create session");
        store
            .insert_event("s1", "session_created", r#"{"type":"session_created"}"#)
            .expect("insert first event");
        store
            .insert_event("s1", "message_started", r#"{"type":"message_started"}"#)
            .expect("insert second event");
        store
            .create_session("s2", 1, 1000, 2000, None, None)
            .expect("create second session");
        store
            .insert_event("s2", "session_created", r#"{"type":"session_created"}"#)
            .expect("insert event for different session");

        let events = store.get_events("s1", None, 10).expect("get events");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "session_created");
        assert_eq!(events[1].event_type, "message_started");

        let limited = store.get_events("s1", None, 1).expect("get events limited");
        assert_eq!(limited.len(), 1);

        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn get_events_since_id_filters_correctly() {
        let (store, path) = temp_db();

        store
            .create_session("s1", 1, 1000, 2000, None, None)
            .expect("create session");
        store
            .insert_event("s1", "event_a", r#"{"type":"a"}"#)
            .expect("insert event a");
        store
            .insert_event("s1", "event_b", r#"{"type":"b"}"#)
            .expect("insert event b");
        store
            .insert_event("s1", "event_c", r#"{"type":"c"}"#)
            .expect("insert event c");

        let latest_id = store
            .get_latest_event_id("s1")
            .expect("get latest")
            .expect("should have events");

        let since = store
            .get_events("s1", Some(latest_id - 1), 10)
            .expect("get events since id");
        assert_eq!(since.len(), 1);
        assert_eq!(since[0].event_type, "event_c");

        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn get_latest_event_id_returns_latest() {
        let (store, path) = temp_db();

        let none_result = store
            .get_latest_event_id("nonexistent")
            .expect("should not error");
        assert!(none_result.is_none());

        store
            .create_session("s1", 1, 1000, 2000, None, None)
            .expect("create session");
        store
            .insert_event("s1", "event_a", r#"{"type":"a"}"#)
            .expect("insert first");
        store
            .insert_event("s1", "event_b", r#"{"type":"b"}"#)
            .expect("insert second");

        let latest = store
            .get_latest_event_id("s1")
            .expect("get latest")
            .expect("should exist");
        assert!(latest > 0);

        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn get_events_since_id_returns_all_sessions() {
        let (store, path) = temp_db();

        store
            .create_session("s1", 1, 1000, 2000, None, None)
            .expect("create session s1");
        store
            .create_session("s2", 1, 1000, 2000, None, None)
            .expect("create session s2");
        store
            .insert_event("s1", "event_a", r#"{"type":"a"}"#)
            .expect("insert event a");
        store
            .insert_event("s2", "event_b", r#"{"type":"b"}"#)
            .expect("insert event b");

        let all = store.get_events_since_id(0, 10).expect("get all events");
        assert_eq!(all.len(), 2);

        fs::remove_file(path).expect("cleanup");
    }
}
