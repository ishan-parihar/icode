use std::collections::BTreeMap;
use std::fmt::{self, Display};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerCredentials {
    pub server_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
    pub scopes: Vec<String>,
    pub registered_at: String,
    pub metadata_url: Option<String>,
}

#[derive(Debug)]
pub enum McpOAuthStoreError {
    Io(io::Error),
    Parse(serde_json::Error),
    Permission(io::Error),
}

impl Display for McpOAuthStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Parse(err) => write!(f, "Parse error: {err}"),
            Self::Permission(err) => write!(f, "Permission error: {err}"),
        }
    }
}

impl std::error::Error for McpOAuthStoreError {}

impl From<io::Error> for McpOAuthStoreError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for McpOAuthStoreError {
    fn from(err: serde_json::Error) -> Self {
        Self::Parse(err)
    }
}

pub struct McpOAuthStore {
    config_home: PathBuf,
}

impl McpOAuthStore {
    #[must_use]
    pub fn new(config_home: PathBuf) -> Self {
        Self { config_home }
    }

    fn file_path(&self) -> PathBuf {
        self.config_home.join("mcp-auth.json")
    }

    fn load_all(&self) -> Result<BTreeMap<String, McpServerCredentials>, McpOAuthStoreError> {
        let path = self.file_path();
        match fs::read_to_string(&path) {
            Ok(contents) => {
                if contents.trim().is_empty() {
                    return Ok(BTreeMap::new());
                }
                let map: BTreeMap<String, McpServerCredentials> = serde_json::from_str(&contents)?;
                Ok(map)
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(err) => Err(McpOAuthStoreError::Io(err)),
        }
    }

    fn save_all(
        &self,
        map: &BTreeMap<String, McpServerCredentials>,
    ) -> Result<(), McpOAuthStoreError> {
        let path = self.file_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let rendered = serde_json::to_string_pretty(map)?;
        let tmp_path = path.with_extension("json.tmp");

        let result = (|| {
            {
                let mut file = File::create(&tmp_path)?;
                file.write_all(rendered.as_bytes())?;
                file.write_all(b"\n")?;
                file.sync_all()?;
            }

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&tmp_path)?.permissions();
                perms.set_mode(0o600);
                fs::set_permissions(&tmp_path, perms).map_err(McpOAuthStoreError::Permission)?;
            }

            fs::rename(&tmp_path, &path)?;
            Ok(())
        })();

        if result.is_err() {
            let _ = std::fs::remove_file(&tmp_path);
        }

        result
    }

    #[must_use]
    pub fn load_credentials(&self, server_key: &str) -> Option<McpServerCredentials> {
        self.load_all().ok()?.remove(server_key)
    }

    pub fn save_credentials(
        &self,
        server_key: &str,
        creds: &McpServerCredentials,
    ) -> Result<(), McpOAuthStoreError> {
        let mut map = self.load_all()?;
        map.insert(server_key.to_string(), creds.clone());
        self.save_all(&map)
    }

    pub fn delete_credentials(&self, server_key: &str) -> Result<(), McpOAuthStoreError> {
        let mut map = self.load_all()?;
        if map.remove(server_key).is_some() {
            self.save_all(&map)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn list_servers(&self) -> Vec<String> {
        self.load_all()
            .map(|map| map.into_keys().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn temp_config_home() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("mcp-oauth-store-test-{nanos}"))
    }

    fn sample_credentials() -> McpServerCredentials {
        McpServerCredentials {
            server_url: "https://mcp.example.com".to_string(),
            client_id: "client-123".to_string(),
            client_secret: Some("secret-456".to_string()),
            access_token: "access-789".to_string(),
            refresh_token: Some("refresh-abc".to_string()),
            expires_at: Some(1_700_000_000),
            scopes: vec!["read".to_string(), "write".to_string()],
            registered_at: "2024-01-01T00:00:00Z".to_string(),
            metadata_url: Some(
                "https://auth.example.com/.well-known/oauth-authorization-server".to_string(),
            ),
        }
    }

    #[test]
    fn save_and_load_round_trip() {
        let home = temp_config_home();
        let store = McpOAuthStore::new(home.clone());
        let creds = sample_credentials();

        store
            .save_credentials("test-server", &creds)
            .expect("save should succeed");

        let loaded = store
            .load_credentials("test-server")
            .expect("load should return Some");

        assert_eq!(loaded, creds);

        fs::remove_dir_all(home).ok();
    }

    #[test]
    fn delete_credentials_removes_entry() {
        let home = temp_config_home();
        let store = McpOAuthStore::new(home.clone());
        let creds = sample_credentials();

        store
            .save_credentials("to-delete", &creds)
            .expect("save should succeed");

        store
            .delete_credentials("to-delete")
            .expect("delete should succeed");

        assert!(store.load_credentials("to-delete").is_none());
        assert_eq!(store.list_servers(), Vec::<String>::new());

        fs::remove_dir_all(home).ok();
    }

    #[test]
    fn list_servers_returns_all_keys() {
        let home = temp_config_home();
        let store = McpOAuthStore::new(home.clone());
        let creds = sample_credentials();

        let mut c1 = creds.clone();
        c1.server_url = "https://alpha.example.com".to_string();
        let mut c2 = creds.clone();
        c2.server_url = "https://beta.example.com".to_string();

        store.save_credentials("alpha", &c1).expect("save alpha");
        store.save_credentials("beta", &c2).expect("save beta");

        let mut servers = store.list_servers();
        servers.sort();
        assert_eq!(servers, vec!["alpha".to_string(), "beta".to_string()]);

        fs::remove_dir_all(home).ok();
    }

    #[test]
    fn missing_server_returns_none() {
        let home = temp_config_home();
        let store = McpOAuthStore::new(home.clone());

        assert!(store.load_credentials("nonexistent").is_none());

        fs::remove_dir_all(home).ok();
    }

    #[test]
    #[cfg(unix)]
    fn file_has_0o600_permissions() {
        let home = temp_config_home();
        let store = McpOAuthStore::new(home.clone());
        let creds = sample_credentials();

        store
            .save_credentials("perm-test", &creds)
            .expect("save should succeed");

        let path = store.file_path();
        let metadata = fs::metadata(&path).expect("file should exist");
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "file permissions should be 0o600, got {mode:o}"
        );

        fs::remove_dir_all(home).ok();
    }

    #[test]
    fn store_handles_missing_file_gracefully() {
        let home = temp_config_home();
        let store = McpOAuthStore::new(home.clone());

        assert!(store.load_credentials("anything").is_none());
        assert_eq!(store.list_servers(), Vec::<String>::new());
        assert!(store.delete_credentials("anything").is_ok());

        fs::remove_dir_all(home).ok();
    }
}
