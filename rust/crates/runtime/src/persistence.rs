use crate::usage::TokenUsage;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug)]
pub enum PersistenceError {
    Db(rusqlite::Error),
    Io(std::io::Error),
    Format(String),
}
impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Format(s) => write!(f, "{s}"),
        }
    }
}
impl std::error::Error for PersistenceError {}
impl From<rusqlite::Error> for PersistenceError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e)
    }
}
impl From<std::io::Error> for PersistenceError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRow {
    pub session_id: String,
    pub version: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub compaction_count: Option<i64>,
    pub compaction_removed: Option<i64>,
    pub compaction_summary: Option<String>,
    pub fork_parent_id: Option<String>,
    pub fork_branch_name: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRow {
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
    pub task_id: String,
    pub prompt: String,
    pub description: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub output: Option<String>,
    pub team_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamRow {
    pub team_id: String,
    pub name: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronRow {
    pub cron_id: String,
    pub schedule: String,
    pub prompt: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_run_at: Option<String>,
    pub run_count: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerRow {
    pub server_name: String,
    pub status: String,
    pub server_info: Option<String>,
    pub error_message: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerRow {
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
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerRow {
    pub language: String,
    pub status: String,
    pub root_path: Option<String>,
    pub capabilities_json: Option<String>,
}

#[derive(Debug)]
pub struct SqliteStore {
    conn: Connection,
    #[allow(dead_code)]
    db_path: Option<PathBuf>,
}

impl SqliteStore {
    pub fn new(db_path: impl Into<PathBuf>) -> Result<Self, PersistenceError> {
        let path = db_path.into();
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let conn = Connection::open(&path)?;
        let store = Self {
            conn,
            db_path: Some(path),
        };
        store.migrate()?;
        Ok(store)
    }
    pub fn in_memory() -> Result<Self, PersistenceError> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn,
            db_path: None,
        };
        store.migrate()?;
        Ok(store)
    }
    pub fn default_path() -> Result<Self, PersistenceError> {
        Self::new(std::env::temp_dir().join("icode").join("sessions.db"))
    }
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
    fn migrate(&self) -> Result<(), PersistenceError> {
        let md = find_migrations_dir()?;
        let mut files: Vec<_> = std::fs::read_dir(&md)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
            .collect();
        files.sort_by_key(|e| e.file_name());
        for entry in &files {
            let sql = std::fs::read_to_string(entry.path())?;
            self.conn.execute_batch(&sql)?;
        }
        Ok(())
    }
    pub fn get_session(&self, sid: &str) -> Result<Option<SessionRow>, PersistenceError> {
        self.conn.query_row("SELECT session_id, version, created_at_ms, updated_at_ms, compaction_count, compaction_removed, compaction_summary, fork_parent_id, fork_branch_name FROM sessions WHERE session_id = ?1", params![sid],
            |r| Ok(SessionRow { session_id: r.get(0)?, version: r.get(1)?, created_at_ms: r.get(2)?, updated_at_ms: r.get(3)?, compaction_count: r.get(4)?, compaction_removed: r.get(5)?, compaction_summary: r.get(6)?, fork_parent_id: r.get(7)?, fork_branch_name: r.get(8)? })
        ).optional().map_err(Into::into)
    }
    pub fn list_sessions(&self) -> Result<Vec<SessionRow>, PersistenceError> {
        let mut stmt = self.conn.prepare("SELECT session_id, version, created_at_ms, updated_at_ms, compaction_count, compaction_removed, compaction_summary, fork_parent_id, fork_branch_name FROM sessions ORDER BY updated_at_ms DESC")?;
        let result: Result<Vec<_>, _> = stmt
            .query_map([], |r| {
                Ok(SessionRow {
                    session_id: r.get(0)?,
                    version: r.get(1)?,
                    created_at_ms: r.get(2)?,
                    updated_at_ms: r.get(3)?,
                    compaction_count: r.get(4)?,
                    compaction_removed: r.get(5)?,
                    compaction_summary: r.get(6)?,
                    fork_parent_id: r.get(7)?,
                    fork_branch_name: r.get(8)?,
                })
            })?
            .collect();
        result.map_err(Into::into)
    }
    pub fn delete_session(&self, sid: &str) -> Result<bool, PersistenceError> {
        Ok(self
            .conn
            .execute("DELETE FROM sessions WHERE session_id = ?1", params![sid])?
            > 0)
    }
    pub fn save_message(
        &self,
        sid: &str,
        idx: i64,
        role: &str,
        cj: &str,
        u: Option<&TokenUsage>,
    ) -> Result<(), PersistenceError> {
        let (a, b, c, d) = u.map_or((None, None, None, None), |x| {
            (
                Some(x.input_tokens as i64),
                Some(x.output_tokens as i64),
                Some(x.cache_creation_input_tokens as i64),
                Some(x.cache_read_input_tokens as i64),
            )
        });
        self.conn.execute("INSERT OR REPLACE INTO messages (session_id, message_index, role, content_json, usage_input_tokens, usage_output_tokens, usage_cache_create, usage_cache_read) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", params![sid, idx, role, cj, a, b, c, d])?;
        Ok(())
    }
    pub fn get_messages(&self, sid: &str) -> Result<Vec<MessageRow>, PersistenceError> {
        let mut stmt = self.conn.prepare("SELECT session_id, message_index, role, content_json, usage_input_tokens, usage_output_tokens, usage_cache_create, usage_cache_read FROM messages WHERE session_id = ?1 ORDER BY message_index ASC")?;
        let result: Result<Vec<_>, _> = stmt
            .query_map(params![sid], |r| {
                Ok(MessageRow {
                    session_id: r.get(0)?,
                    message_index: r.get(1)?,
                    role: r.get(2)?,
                    content_json: r.get(3)?,
                    usage_input_tokens: r.get(4)?,
                    usage_output_tokens: r.get(5)?,
                    usage_cache_create: r.get(6)?,
                    usage_cache_read: r.get(7)?,
                })
            })?
            .collect();
        result.map_err(Into::into)
    }
    pub fn upsert_session_with_messages(
        &mut self,
        sid: &str,
        ver: i64,
        cam: i64,
        uam: i64,
        msgs: &[(String, String, Option<&TokenUsage>)],
    ) -> Result<(), PersistenceError> {
        let tx = self.conn.transaction()?;
        tx.execute("INSERT OR REPLACE INTO sessions (session_id, version, created_at_ms, updated_at_ms) VALUES (?1,?2,?3,?4)", params![sid, ver, cam, uam])?;
        tx.execute("DELETE FROM messages WHERE session_id = ?1", params![sid])?;
        for (i, (role, cj, u)) in msgs.iter().enumerate() {
            let (a, b, c, d) = u.map_or((None, None, None, None), |x| {
                (
                    Some(x.input_tokens as i64),
                    Some(x.output_tokens as i64),
                    Some(x.cache_creation_input_tokens as i64),
                    Some(x.cache_read_input_tokens as i64),
                )
            });
            tx.execute("INSERT INTO messages (session_id, message_index, role, content_json, usage_input_tokens, usage_output_tokens, usage_cache_create, usage_cache_read) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", params![sid, i as i64, role, cj, a, b, c, d])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn append_message(
        &mut self,
        sid: &str,
        uam: i64,
        role: &str,
        cj: &str,
        u: Option<&TokenUsage>,
    ) -> Result<(), PersistenceError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE sessions SET updated_at_ms = ?2 WHERE session_id = ?1",
            params![sid, uam],
        )?;
        let mx: i64 = tx
            .query_row(
                "SELECT MAX(message_index) FROM messages WHERE session_id = ?1",
                params![sid],
                |r| r.get::<_, Option<i64>>(0),
            )
            .ok()
            .flatten()
            .unwrap_or(-1);
        let (a, b, c, d) = u.map_or((None, None, None, None), |x| {
            (
                Some(x.input_tokens as i64),
                Some(x.output_tokens as i64),
                Some(x.cache_creation_input_tokens as i64),
                Some(x.cache_read_input_tokens as i64),
            )
        });
        tx.execute("INSERT INTO messages (session_id, message_index, role, content_json, usage_input_tokens, usage_output_tokens, usage_cache_create, usage_cache_read) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", params![sid, mx+1, role, cj, a, b, c, d])?;
        tx.commit()?;
        Ok(())
    }
    pub fn create_session(
        &self,
        sid: &str,
        ver: i64,
        cam: i64,
        uam: i64,
        _compaction: Option<i64>,
        _fork: Option<String>,
    ) -> Result<(), PersistenceError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO sessions (session_id, version, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4)",
            params![sid, ver, cam, uam],
        )?;
        Ok(())
    }
    fn now_iso() -> String {
        Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }
    pub fn create_task(
        &self,
        tid: &str,
        prompt: &str,
        desc: Option<&str>,
        _tpj: Option<&str>,
        status: &str,
        team_id: Option<&str>,
    ) -> Result<TaskRow, PersistenceError> {
        let now = Self::now_iso();
        self.conn.execute(
            "INSERT OR REPLACE INTO tasks (task_id,prompt,description,status,created_at,updated_at,output,team_id) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![tid, prompt, desc, status, &now, &now, None as Option<String>, team_id],
        )?;
        Ok(TaskRow {
            task_id: tid.into(),
            prompt: prompt.into(),
            description: desc.map(Into::into),
            status: status.into(),
            created_at: now.clone(),
            updated_at: now,
            output: None,
            team_id: team_id.map(Into::into),
        })
    }
    pub fn get_task(&self, id: &str) -> Result<Option<TaskRow>, PersistenceError> {
        self.conn.query_row(
            "SELECT task_id,prompt,description,status,created_at,updated_at,output,team_id FROM tasks WHERE task_id=?1",
            params![id],
            |r| {
                Ok(TaskRow {
                    task_id: r.get(0)?,
                    prompt: r.get(1)?,
                    description: r.get(2)?,
                    status: r.get(3)?,
                    created_at: r.get(4)?,
                    updated_at: r.get(5)?,
                    output: r.get(6)?,
                    team_id: r.get(7)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }
    pub fn list_tasks(&self, _team_id: Option<&str>) -> Result<Vec<TaskRow>, PersistenceError> {
        let mut stmt = self.conn.prepare(
            "SELECT task_id,prompt,description,status,created_at,updated_at,output,team_id FROM tasks ORDER BY created_at DESC",
        )?;
        {
            let result: Result<Vec<_>, _> = stmt
                .query_map([], |r| {
                    Ok(TaskRow {
                        task_id: r.get(0)?,
                        prompt: r.get(1)?,
                        description: r.get(2)?,
                        status: r.get(3)?,
                        created_at: r.get(4)?,
                        updated_at: r.get(5)?,
                        output: r.get(6)?,
                        team_id: r.get(7)?,
                    })
                })?
                .collect();
            result.map_err(Into::into)
        }
    }
    pub fn update_task(
        &self,
        id: &str,
        status: Option<&str>,
        output: Option<&str>,
        _updated_at: Option<&str>,
    ) -> Result<(), PersistenceError> {
        let now = Self::now_iso();
        if status.is_some() || output.is_some() {
            let mut parts = vec!["updated_at=?3".to_string()];
            if status.is_some() {
                parts.push("status=?1".to_string());
            }
            if output.is_some() {
                parts.push("output=?2".to_string());
            }
            let sql = format!("UPDATE tasks SET {} WHERE task_id=?4", parts.join(","));
            self.conn.execute(
                &sql,
                params![status.unwrap_or(""), output.unwrap_or(""), now, id],
            )?;
        }
        Ok(())
    }
    pub fn create_team(&self, tid: &str, name: &str) -> Result<TeamRow, PersistenceError> {
        let now = Self::now_iso();
        self.conn.execute(
            "INSERT OR REPLACE INTO teams (team_id,name,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5)",
            params![tid, name, "active", &now, &now],
        )?;
        Ok(TeamRow {
            team_id: tid.into(),
            name: name.into(),
            status: "active".into(),
            created_at: now.clone(),
            updated_at: now,
        })
    }
    pub fn list_teams(&self) -> Result<Vec<TeamRow>, PersistenceError> {
        let mut stmt = self.conn.prepare(
            "SELECT team_id,name,status,created_at,updated_at FROM teams ORDER BY created_at DESC",
        )?;
        {
            let result: Result<Vec<_>, _> = stmt
                .query_map([], |r| {
                    Ok(TeamRow {
                        team_id: r.get(0)?,
                        name: r.get(1)?,
                        status: r.get(2)?,
                        created_at: r.get(3)?,
                        updated_at: r.get(4)?,
                    })
                })?
                .collect();
            result.map_err(Into::into)
        }
    }
    pub fn delete_team(&self, id: &str) -> Result<bool, PersistenceError> {
        Ok(self
            .conn
            .execute("DELETE FROM teams WHERE team_id = ?1", params![id])?
            > 0)
    }
    pub fn create_cron(
        &self,
        cid: &str,
        schedule: &str,
        prompt: &str,
        desc: Option<&str>,
    ) -> Result<CronRow, PersistenceError> {
        let now = Self::now_iso();
        self.conn.execute(
            "INSERT OR REPLACE INTO crons (cron_id,schedule,prompt,description,enabled,created_at,updated_at,last_run_at,run_count) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![cid, schedule, prompt, desc, true, &now, &now, None as Option<String>, 0i64],
        )?;
        Ok(CronRow {
            cron_id: cid.into(),
            schedule: schedule.into(),
            prompt: prompt.into(),
            description: desc.map(Into::into),
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
            last_run_at: None,
            run_count: 0,
        })
    }
    pub fn list_crons(&self) -> Result<Vec<CronRow>, PersistenceError> {
        let mut stmt = self.conn.prepare(
            "SELECT cron_id,schedule,prompt,description,enabled,created_at,updated_at,last_run_at,run_count FROM crons ORDER BY created_at DESC",
        )?;
        {
            let result: Result<Vec<_>, _> = stmt
                .query_map([], |r| {
                    Ok(CronRow {
                        cron_id: r.get(0)?,
                        schedule: r.get(1)?,
                        prompt: r.get(2)?,
                        description: r.get(3)?,
                        enabled: r.get(4)?,
                        created_at: r.get(5)?,
                        updated_at: r.get(6)?,
                        last_run_at: r.get(7)?,
                        run_count: r.get(8)?,
                    })
                })?
                .collect();
            result.map_err(Into::into)
        }
    }
    pub fn delete_cron(&self, id: &str) -> Result<bool, PersistenceError> {
        Ok(self
            .conn
            .execute("DELETE FROM crons WHERE cron_id = ?1", params![id])?
            > 0)
    }
    pub fn list_mcp_servers(&self) -> Result<Vec<McpServerRow>, PersistenceError> {
        let mut stmt = self
            .conn
            .prepare("SELECT server_name,status,server_info,error_message FROM mcp_servers ORDER BY server_name")?;
        {
            let result: Result<Vec<_>, _> = stmt
                .query_map([], |r| {
                    Ok(McpServerRow {
                        server_name: r.get(0)?,
                        status: r.get(1)?,
                        server_info: r.get(2)?,
                        error_message: r.get(3)?,
                    })
                })?
                .collect();
            result.map_err(Into::into)
        }
    }
    pub fn upsert_mcp_server(
        &self,
        name: &str,
        status: &str,
        server_info: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<McpServerRow, PersistenceError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO mcp_servers (server_name,status,server_info,error_message) VALUES (?1,?2,?3,?4)",
            params![name, status, server_info, error_message],
        )?;
        Ok(McpServerRow {
            server_name: name.into(),
            status: status.into(),
            server_info: server_info.map(Into::into),
            error_message: error_message.map(Into::into),
        })
    }
    pub fn get_mcp_server(&self, name: &str) -> Result<Option<McpServerRow>, PersistenceError> {
        self.conn.query_row(
            "SELECT server_name,status,server_info,error_message FROM mcp_servers WHERE server_name=?1",
            params![name],
            |r| {
                Ok(McpServerRow {
                    server_name: r.get(0)?,
                    status: r.get(1)?,
                    server_info: r.get(2)?,
                    error_message: r.get(3)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }
    pub fn list_workers(&self) -> Result<Vec<WorkerRow>, PersistenceError> {
        let mut stmt = self.conn.prepare(
            "SELECT worker_id,cwd,status,trust_auto_resolve,trust_gate_cleared,auto_recover_prompt_misdelivery,prompt_delivery_attempts,last_prompt,replay_prompt,last_error_json,created_at,updated_at FROM workers ORDER BY created_at DESC",
        )?;
        {
            let result: Result<Vec<_>, _> = stmt
                .query_map([], |r| {
                    Ok(WorkerRow {
                        worker_id: r.get(0)?,
                        cwd: r.get(1)?,
                        status: r.get(2)?,
                        trust_auto_resolve: r.get(3)?,
                        trust_gate_cleared: r.get(4)?,
                        auto_recover_prompt_misdelivery: r.get(5)?,
                        prompt_delivery_attempts: r.get(6)?,
                        last_prompt: r.get(7)?,
                        replay_prompt: r.get(8)?,
                        last_error_json: r.get(9)?,
                        created_at: r.get(10)?,
                        updated_at: r.get(11)?,
                    })
                })?
                .collect();
            result.map_err(Into::into)
        }
    }
    pub fn create_worker(
        &self,
        wid: &str,
        cwd: &str,
        trust_auto_resolve: bool,
        auto_recover: bool,
    ) -> Result<WorkerRow, PersistenceError> {
        let now = Self::now_iso();
        self.conn.execute(
            "INSERT OR REPLACE INTO workers (worker_id,cwd,status,trust_auto_resolve,trust_gate_cleared,auto_recover_prompt_misdelivery,prompt_delivery_attempts,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![wid, cwd, "idle", trust_auto_resolve, false, auto_recover, 0i64, &now, &now],
        )?;
        Ok(WorkerRow {
            worker_id: wid.into(),
            cwd: cwd.into(),
            status: "idle".into(),
            trust_auto_resolve,
            trust_gate_cleared: false,
            auto_recover_prompt_misdelivery: auto_recover,
            prompt_delivery_attempts: 0,
            last_prompt: None,
            replay_prompt: None,
            last_error_json: None,
            created_at: now.clone(),
            updated_at: now,
        })
    }
    pub fn get_worker(&self, id: &str) -> Result<Option<WorkerRow>, PersistenceError> {
        self.conn.query_row(
            "SELECT worker_id,cwd,status,trust_auto_resolve,trust_gate_cleared,auto_recover_prompt_misdelivery,prompt_delivery_attempts,last_prompt,replay_prompt,last_error_json,created_at,updated_at FROM workers WHERE worker_id=?1",
            params![id],
            |r| {
                Ok(WorkerRow {
                    worker_id: r.get(0)?,
                    cwd: r.get(1)?,
                    status: r.get(2)?,
                    trust_auto_resolve: r.get(3)?,
                    trust_gate_cleared: r.get(4)?,
                    auto_recover_prompt_misdelivery: r.get(5)?,
                    prompt_delivery_attempts: r.get(6)?,
                    last_prompt: r.get(7)?,
                    replay_prompt: r.get(8)?,
                    last_error_json: r.get(9)?,
                    created_at: r.get(10)?,
                    updated_at: r.get(11)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }
    pub fn update_worker(
        &self,
        id: &str,
        status: Option<&str>,
        _cwd: Option<&str>,
        trust_gate: Option<bool>,
        _prompt_attempts: Option<i64>,
        _last_prompt: Option<&str>,
        _replay_prompt: Option<&str>,
    ) -> Result<(), PersistenceError> {
        let now = Self::now_iso();
        if let Some(s) = status {
            self.conn.execute(
                "UPDATE workers SET status=?2, updated_at=?3 WHERE worker_id=?1",
                params![id, s, &now],
            )?;
        }
        if let Some(t) = trust_gate {
            self.conn.execute(
                "UPDATE workers SET trust_gate_cleared=?2, updated_at=?3 WHERE worker_id=?1",
                params![id, t, &now],
            )?;
        }
        Ok(())
    }
    pub fn list_lsp_servers(&self) -> Result<Vec<LspServerRow>, PersistenceError> {
        let mut stmt = self.conn.prepare(
            "SELECT language,status,root_path,capabilities_json FROM lsp_servers ORDER BY language",
        )?;
        {
            let result: Result<Vec<_>, _> = stmt
                .query_map([], |r| {
                    Ok(LspServerRow {
                        language: r.get(0)?,
                        status: r.get(1)?,
                        root_path: r.get(2)?,
                        capabilities_json: r.get(3)?,
                    })
                })?
                .collect();
            result.map_err(Into::into)
        }
    }
    pub fn upsert_lsp_server(
        &self,
        language: &str,
        status: &str,
        root_path: Option<&str>,
        capabilities: Option<&str>,
    ) -> Result<LspServerRow, PersistenceError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO lsp_servers (language,status,root_path,capabilities_json) VALUES (?1,?2,?3,?4)",
            params![language, status, root_path, capabilities],
        )?;
        Ok(LspServerRow {
            language: language.into(),
            status: status.into(),
            root_path: root_path.map(Into::into),
            capabilities_json: capabilities.map(Into::into),
        })
    }
}

fn find_migrations_dir() -> Result<PathBuf, PersistenceError> {
    let md = env!("CARGO_MANIFEST_DIR");
    let cands = [
        option_env!("ICODE_MIGRATIONS_DIR").map(PathBuf::from),
        Some(PathBuf::from(md).join("migrations")),
        Some(std::env::temp_dir().join("icode").join("migrations")),
    ];
    for c in cands.into_iter().flatten() {
        if c.exists() {
            return Ok(c);
        }
    }
    Err(PersistenceError::Format(
        "migrations directory not found".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn in_memory_migrates() {
        let _s = SqliteStore::in_memory().unwrap();
    }
    #[test]
    fn session_crud() {
        let s = SqliteStore::in_memory().unwrap();
        s.create_session("s1", 1, 1000, 2000, None, None).unwrap();
        assert_eq!(s.get_session("s1").unwrap().unwrap().session_id, "s1");
        s.delete_session("s1").unwrap();
        assert!(s.get_session("s1").unwrap().is_none());
    }
    #[test]
    fn save_and_get_messages() {
        let s = SqliteStore::in_memory().unwrap();
        s.create_session("s1", 1, 1000, 2000, None, None).unwrap();
        s.save_message("s1", 0, "user", "{}", None).unwrap();
        s.save_message("s1", 1, "assistant", "{}", None).unwrap();
        let m = s.get_messages("s1").unwrap();
        assert_eq!(m.len(), 2);
    }
    #[test]
    fn delete_cascades_messages() {
        let s = SqliteStore::in_memory().unwrap();
        s.create_session("s1", 1, 1000, 2000, None, None).unwrap();
        s.save_message("s1", 0, "user", "{}", None).unwrap();
        s.delete_session("s1").unwrap();
        assert!(s.get_messages("s1").unwrap().is_empty());
    }
    #[test]
    fn upsert_session() {
        let mut s = SqliteStore::in_memory().unwrap();
        s.upsert_session_with_messages("s1", 1, 1000, 2000, &[("user".into(), "{}".into(), None)])
            .unwrap();
        assert_eq!(s.get_session("s1").unwrap().unwrap().session_id, "s1");
        assert_eq!(s.get_messages("s1").unwrap().len(), 1);
    }
}
