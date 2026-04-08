use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthStore {
    pub api_keys: HashMap<String, String>,
    pub oauth_tokens: HashMap<String, OAuthToken>,
}

impl AuthStore {
    #[must_use]
    pub fn config_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".icode")
    }

    #[must_use]
    pub fn load() -> Self {
        let path = Self::config_dir().join("auth.json");
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => Self::default(),
            }
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::config_dir().join("auth.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)
    }

    #[must_use]
    pub fn api_key_for(&self, provider: &str) -> Option<String> {
        self.api_keys.get(provider).cloned()
    }

    pub fn set_api_key(&mut self, provider: String, key: String) {
        self.api_keys.insert(provider, key);
    }

    #[must_use]
    pub fn oauth_token_for(&self, provider: &str) -> Option<&OAuthToken> {
        self.oauth_tokens.get(provider)
    }

    pub fn set_oauth_token(&mut self, provider: String, token: OAuthToken) {
        self.oauth_tokens.insert(provider, token);
    }

    pub fn remove_api_key(&mut self, provider: &str) -> Option<String> {
        self.api_keys.remove(provider)
    }

    pub fn remove_oauth_token(&mut self, provider: &str) -> Option<OAuthToken> {
        self.oauth_tokens.remove(provider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_auth_store_is_empty() {
        let store = AuthStore::default();
        assert!(store.api_keys.is_empty());
        assert!(store.oauth_tokens.is_empty());
    }

    #[test]
    fn set_and_get_api_key() {
        let mut store = AuthStore::default();
        store.set_api_key("anthropic".to_string(), "sk-test".to_string());
        assert_eq!(store.api_key_for("anthropic"), Some("sk-test".to_string()));
        assert_eq!(store.api_key_for("openai"), None);
    }

    #[test]
    fn set_and_get_oauth_token() {
        let mut store = AuthStore::default();
        let token = OAuthToken {
            access_token: "at-123".to_string(),
            refresh_token: Some("rt-456".to_string()),
            expires_at: Some(1_700_000_000),
            scopes: vec!["read".to_string(), "write".to_string()],
        };
        store.set_oauth_token("google".to_string(), token.clone());
        let got = store.oauth_token_for("google").unwrap();
        assert_eq!(got.access_token, "at-123");
        assert_eq!(got.refresh_token.as_deref(), Some("rt-456"));
        assert_eq!(got.scopes, vec!["read", "write"]);
    }

    #[test]
    fn remove_api_key() {
        let mut store = AuthStore::default();
        store.set_api_key("xai".to_string(), "xai-key".to_string());
        let removed = store.remove_api_key("xai");
        assert_eq!(removed, Some("xai-key".to_string()));
        assert!(store.api_key_for("xai").is_none());
    }

    #[test]
    fn config_dir_returns_icode_directory() {
        let dir = AuthStore::config_dir();
        assert!(dir.ends_with(".icode"));
    }
}
