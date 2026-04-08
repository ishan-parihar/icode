use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub id: String,
    pub title: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub model: Option<String>,
    pub provider: Option<String>,
}

pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    pub fn new(path: &PathBuf) -> Result<Self, rusqlite::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        Ok(Self { conn })
    }

    pub fn run_migrations(&self) -> Result<(), rusqlite::Error> {
        self.conn
            .execute_batch(include_str!("../../../migrations/001_sessions.sql"))
    }

    pub fn create_session(&self, session: &SessionRecord) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO sessions (id, title, created_at, updated_at, model, provider, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                session.id,
                session.title,
                session.created_at,
                session.updated_at,
                session.model,
                session.provider,
                None::<String>
            ],
        )?;
        Ok(())
    }

    pub fn get_session(&self, id: &str) -> Result<Option<SessionRecord>, rusqlite::Error> {
        self.conn.query_row(
            "SELECT id, title, created_at, updated_at, model, provider FROM sessions WHERE id = ?1",
            params![id],
            |row| Ok(SessionRecord {
                id: row.get(0)?, title: row.get(1)?, created_at: row.get(2)?,
                updated_at: row.get(3)?, model: row.get(4)?, provider: row.get(5)?,
            }),
        ).optional()
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionRecord>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, created_at, updated_at, model, provider FROM sessions
             ORDER BY updated_at DESC LIMIT 100",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SessionRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                model: row.get(4)?,
                provider: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn save_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO messages (session_id, role, content, created_at)
             VALUES (?1, ?2, ?3, strftime('%s', 'now'))",
            params![session_id, role, content],
        )?;
        Ok(())
    }

    pub fn get_messages(&self, session_id: &str) -> Result<Vec<(String, String)>, rusqlite::Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT role, content FROM messages WHERE session_id = ?1 ORDER BY id ASC")?;
        let rows = stmt.query_map(params![session_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect()
    }

    pub fn update_session(&self, id: &str, title: Option<&str>) -> Result<(), rusqlite::Error> {
        if let Some(t) = title {
            self.conn.execute(
                "UPDATE sessions SET title = ?1, updated_at = strftime('%s', 'now') WHERE id = ?2",
                params![t, id],
            )?;
        }
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> Result<usize, rusqlite::Error> {
        self.conn
            .execute("DELETE FROM sessions WHERE id = ?1", params![id])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (SqliteStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = SqliteStore::new(&path).unwrap();
        store.run_migrations().unwrap();
        (store, dir)
    }

    #[test]
    fn create_and_get_session() {
        let (store, _dir) = temp_store();
        let record = SessionRecord {
            id: "test-1".to_string(),
            title: Some("Test Session".to_string()),
            created_at: 1000,
            updated_at: 1000,
            model: Some("sonnet".to_string()),
            provider: Some("anthropic".to_string()),
        };
        store.create_session(&record).unwrap();
        let got = store.get_session("test-1").unwrap().unwrap();
        assert_eq!(got.id, "test-1");
        assert_eq!(got.title.as_deref(), Some("Test Session"));
        assert_eq!(got.model.as_deref(), Some("sonnet"));
    }

    #[test]
    fn list_sessions_ordered_by_updated() {
        let (store, _dir) = temp_store();
        store
            .create_session(&SessionRecord {
                id: "a".into(),
                title: None,
                created_at: 100,
                updated_at: 100,
                model: None,
                provider: None,
            })
            .unwrap();
        store
            .create_session(&SessionRecord {
                id: "b".into(),
                title: None,
                created_at: 200,
                updated_at: 200,
                model: None,
                provider: None,
            })
            .unwrap();
        let sessions = store.list_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].id, "b"); // most recent first
        assert_eq!(sessions[1].id, "a");
    }

    #[test]
    fn save_and_get_messages() {
        let (store, _dir) = temp_store();
        store
            .create_session(&SessionRecord {
                id: "msg-test".into(),
                title: None,
                created_at: 0,
                updated_at: 0,
                model: None,
                provider: None,
            })
            .unwrap();
        store.save_message("msg-test", "user", "hello").unwrap();
        store.save_message("msg-test", "assistant", "hi").unwrap();
        let msgs = store.get_messages("msg-test").unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0], ("user".to_string(), "hello".to_string()));
        assert_eq!(msgs[1], ("assistant".to_string(), "hi".to_string()));
    }

    #[test]
    fn get_nonexistent_session() {
        let (store, _dir) = temp_store();
        assert!(store.get_session("nonexistent").unwrap().is_none());
    }

    #[test]
    fn update_session_title() {
        let (store, _dir) = temp_store();
        store
            .create_session(&SessionRecord {
                id: "upd".into(),
                title: Some("old".into()),
                created_at: 0,
                updated_at: 0,
                model: None,
                provider: None,
            })
            .unwrap();
        store.update_session("upd", Some("new")).unwrap();
        let got = store.get_session("upd").unwrap().unwrap();
        assert_eq!(got.title.as_deref(), Some("new"));
    }
}
