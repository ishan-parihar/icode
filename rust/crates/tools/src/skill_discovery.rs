use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Metadata stored alongside each cached skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCacheMetadata {
    pub source_url: String,
    pub cached_at: String,
    pub version: String,
    pub sha256: Option<String>,
}

/// A single entry from a remote skill index.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillIndexEntry {
    pub name: String,
    pub description: String,
    pub url: String,
    pub version: String,
    pub sha256: Option<String>,
}

/// Where a skill was resolved from.
#[derive(Debug, Clone)]
pub enum SkillSource {
    Local(PathBuf),
    Remote(PathBuf),
    Bundled,
}

/// Errors that can occur during skill discovery operations.
#[derive(Debug, Clone)]
pub enum SkillDiscoveryError {
    FetchError(String),
    ParseError(String),
    DownloadError(String),
    CacheError(String),
    IntegrityError(String),
}

impl std::fmt::Display for SkillDiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FetchError(msg) => write!(f, "fetch error: {msg}"),
            Self::ParseError(msg) => write!(f, "parse error: {msg}"),
            Self::DownloadError(msg) => write!(f, "download error: {msg}"),
            Self::CacheError(msg) => write!(f, "cache error: {msg}"),
            Self::IntegrityError(msg) => write!(f, "integrity error: {msg}"),
        }
    }
}

impl std::error::Error for SkillDiscoveryError {}

/// Default TTL for cached skills: 24 hours.
const DEFAULT_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Discovers, downloads, and caches skills from remote sources.
pub struct SkillDiscovery {
    cache_dir: PathBuf,
    ttl: Duration,
}

impl SkillDiscovery {
    /// Create a new `SkillDiscovery` with the given cache directory and TTL.
    #[must_use]
    pub fn new(cache_dir: PathBuf, ttl: Duration) -> Self {
        Self { cache_dir, ttl }
    }

    /// Create with the default 24-hour TTL.
    #[must_use]
    pub fn with_default_ttl(cache_dir: PathBuf) -> Self {
        Self::new(cache_dir, DEFAULT_TTL)
    }

    /// Fetch a remote `index.json` and return parsed skill entries.
    pub fn fetch_index(&self, url: &str) -> Result<Vec<SkillIndexEntry>, SkillDiscoveryError> {
        #[derive(Deserialize)]
        struct IndexWrapper {
            skills: Vec<SkillIndexEntry>,
        }

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| {
                SkillDiscoveryError::FetchError(format!("failed to build HTTP client: {e}"))
            })?;

        let resp = client
            .get(url)
            .send()
            .map_err(|e| SkillDiscoveryError::FetchError(format!("failed to fetch {url}: {e}")))?;

        if !resp.status().is_success() {
            return Err(SkillDiscoveryError::FetchError(format!(
                "HTTP {} from {}",
                resp.status(),
                url
            )));
        }

        let body = resp.text().map_err(|e| {
            SkillDiscoveryError::FetchError(format!("failed to read response body: {e}"))
        })?;

        if let Ok(entries) = serde_json::from_str::<Vec<SkillIndexEntry>>(&body) {
            return Ok(entries);
        }

        if let Ok(wrapper) = serde_json::from_str::<IndexWrapper>(&body) {
            return Ok(wrapper.skills);
        }

        Err(SkillDiscoveryError::ParseError(format!(
            "index.json at {url} is neither a skill array nor an object with 'skills' key"
        )))
    }

    /// Download a skill's SKILL.md from the given entry into the cache directory.
    pub fn download_skill(&self, entry: &SkillIndexEntry) -> Result<PathBuf, SkillDiscoveryError> {
        let skill_dir = self.cache_dir.join(&entry.name);
        fs::create_dir_all(&skill_dir).map_err(|e| {
            SkillDiscoveryError::DownloadError(format!("failed to create cache dir: {e}"))
        })?;

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| {
                SkillDiscoveryError::DownloadError(format!("failed to build HTTP client: {e}"))
            })?;

        let resp = client.get(&entry.url).send().map_err(|e| {
            SkillDiscoveryError::DownloadError(format!("failed to download skill: {e}"))
        })?;

        if !resp.status().is_success() {
            return Err(SkillDiscoveryError::DownloadError(format!(
                "HTTP {} from {}",
                resp.status(),
                entry.url
            )));
        }

        let content = resp.bytes().map_err(|e| {
            SkillDiscoveryError::DownloadError(format!("failed to read skill content: {e}"))
        })?;

        if let Some(expected_hash) = &entry.sha256 {
            let actual_hash = compute_sha256(&content);
            if actual_hash != *expected_hash {
                return Err(SkillDiscoveryError::IntegrityError(format!(
                    "sha256 mismatch for skill '{}': expected {expected_hash}, got {actual_hash}",
                    entry.name
                )));
            }
        }

        let skill_path = skill_dir.join("SKILL.md");
        fs::write(&skill_path, &content).map_err(|e| {
            SkillDiscoveryError::DownloadError(format!("failed to write SKILL.md: {e}"))
        })?;

        let metadata = SkillCacheMetadata {
            source_url: entry.url.clone(),
            cached_at: chrono::Utc::now().to_rfc3339(),
            version: entry.version.clone(),
            sha256: entry.sha256.clone(),
        };
        let meta_path = skill_dir.join(".metadata");
        fs::write(
            &meta_path,
            serde_json::to_string_pretty(&metadata).map_err(|e| {
                SkillDiscoveryError::CacheError(format!("failed to serialize metadata: {e}"))
            })?,
        )
        .map_err(|e| SkillDiscoveryError::CacheError(format!("failed to write metadata: {e}")))?;

        Ok(skill_path)
    }

    /// Check if a cached skill at `skill_path` exists and is not expired.
    #[must_use]
    pub fn resolve_skill_cached(&self, name: &str) -> Option<PathBuf> {
        let skill_path = self.cache_dir.join(name).join("SKILL.md");
        if !skill_path.exists() {
            return None;
        }

        if is_cache_expired_for_ttl(&skill_path, self.ttl) {
            return None;
        }

        Some(skill_path)
    }

    /// Refresh skills from a list of remote index.json URLs.
    /// Returns the list of updated skill names.
    pub fn refresh_from_urls(&self, urls: &[String]) -> Result<Vec<String>, SkillDiscoveryError> {
        let mut updated = Vec::new();

        for url in urls {
            let entries = self.fetch_index(url)?;
            for entry in &entries {
                self.download_skill(entry)?;
                updated.push(entry.name.clone());
            }
        }

        Ok(updated)
    }

    /// List all cached skills with their metadata.
    #[must_use]
    pub fn list_cached_skills(&self) -> Vec<(String, PathBuf, String)> {
        let mut result = Vec::new();

        if !self.cache_dir.exists() {
            return result;
        }

        let Ok(entries) = fs::read_dir(&self.cache_dir) else {
            return result;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let skill_path = path.join("SKILL.md");
            if !skill_path.exists() {
                continue;
            }

            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let meta_path = path.join(".metadata");
            let cached_at = if let Ok(meta_content) = fs::read_to_string(&meta_path) {
                if let Ok(meta) = serde_json::from_str::<SkillCacheMetadata>(&meta_content) {
                    meta.cached_at
                } else {
                    "<invalid metadata>".to_string()
                }
            } else {
                "<no metadata>".to_string()
            };

            result.push((name, skill_path, cached_at));
        }

        result
    }

    /// Remove all cached skills past their TTL.
    pub fn cleanup_expired(&self) {
        if !self.cache_dir.exists() {
            return;
        }

        let Ok(entries) = fs::read_dir(&self.cache_dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let skill_path = path.join("SKILL.md");
            if !skill_path.exists() {
                continue;
            }

            if is_cache_expired_for_ttl(&skill_path, self.ttl) {
                let _ = fs::remove_dir_all(&path);
            }
        }
    }
}

/// Compute the hex SHA-256 hash of the given bytes.
fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Check if a cached skill at `skill_path` is expired, given a TTL.
fn is_cache_expired_for_ttl(skill_path: &Path, ttl: Duration) -> bool {
    let meta_path = skill_path.parent().map(|p| p.join(".metadata"));
    let Some(meta_path) = meta_path else {
        return true;
    };

    let Ok(meta_content) = fs::read_to_string(&meta_path) else {
        return true;
    };

    let Ok(meta) = serde_json::from_str::<SkillCacheMetadata>(&meta_content) else {
        return true;
    };

    let Ok(cached_at) = chrono::DateTime::parse_from_rfc3339(&meta.cached_at) else {
        return true;
    };

    let now = chrono::Utc::now();
    let expiry = cached_at + ttl;

    now > expiry
}

/// Public helper: check if a cached skill at `skill_path` is expired (uses 24h default TTL).
#[must_use]
pub fn is_cache_expired(skill_path: &Path) -> bool {
    is_cache_expired_for_ttl(skill_path, DEFAULT_TTL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn make_discovery(cache_dir: PathBuf) -> SkillDiscovery {
        SkillDiscovery::new(cache_dir, DEFAULT_TTL)
    }

    #[test]
    fn test_fetch_index_parses_entries() {
        let json = r#"[
            {"name": "test-skill", "description": "A test skill", "url": "https://example.com/skill.md", "version": "1.0.0", "sha256": null},
            {"name": "another-skill", "description": "Another skill", "url": "https://example.com/other.md", "version": "2.0.0", "sha256": "abc123"}
        ]"#;

        let entries: Vec<SkillIndexEntry> = serde_json::from_str(json).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "test-skill");
        assert_eq!(entries[0].description, "A test skill");
        assert_eq!(entries[0].sha256, None);
        assert_eq!(entries[1].name, "another-skill");
        assert_eq!(entries[1].sha256, Some("abc123".to_string()));
    }

    #[test]
    fn test_download_skill_creates_cache() {
        let temp_dir = std::env::temp_dir().join(format!(
            "test_skill_cache_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let discovery = make_discovery(temp_dir.clone());

        let skill_dir = temp_dir.join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_path = skill_dir.join("SKILL.md");
        fs::write(&skill_path, "# My Skill\n\nThis is a test skill.").unwrap();

        let meta = SkillCacheMetadata {
            source_url: "https://example.com/my-skill/SKILL.md".to_string(),
            cached_at: chrono::Utc::now().to_rfc3339(),
            version: "1.0.0".to_string(),
            sha256: None,
        };
        fs::write(
            skill_dir.join(".metadata"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        assert!(skill_path.exists());
        assert!(skill_dir.join(".metadata").exists());

        let found = discovery.resolve_skill_cached("my-skill");
        assert!(found.is_some());
        assert_eq!(found.unwrap(), skill_path);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_cache_ttl_expiration() {
        let temp_dir = std::env::temp_dir().join(format!(
            "test_ttl_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let skill_dir = temp_dir.join("expired-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Expired").unwrap();

        let old_time = chrono::Utc::now() - chrono::Duration::hours(100);
        let meta = SkillCacheMetadata {
            source_url: "https://example.com/expired.md".to_string(),
            cached_at: old_time.to_rfc3339(),
            version: "0.1.0".to_string(),
            sha256: None,
        };
        fs::write(
            skill_dir.join(".metadata"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        assert!(is_cache_expired(&skill_dir.join("SKILL.md")));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_sha256_verification_success() {
        let content = b"hello world";
        let hash = compute_sha256(content);

        let temp_dir = std::env::temp_dir().join(format!(
            "test_sha256_ok_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let skill_dir = temp_dir.join("sha-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_path = skill_dir.join("SKILL.md");
        fs::write(&skill_path, content).unwrap();

        let meta = SkillCacheMetadata {
            source_url: "https://example.com/sha.md".to_string(),
            cached_at: chrono::Utc::now().to_rfc3339(),
            version: "1.0.0".to_string(),
            sha256: Some(hash.clone()),
        };
        fs::write(
            skill_dir.join(".metadata"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        assert!(!is_cache_expired(&skill_path));
        assert_eq!(compute_sha256(content), hash);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_sha256_verification_failure() {
        let content = b"hello world";
        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let actual_hash = compute_sha256(content);

        assert_ne!(actual_hash, wrong_hash);

        let expected_err = SkillDiscoveryError::IntegrityError(format!(
            "sha256 mismatch: expected {wrong_hash}, got {actual_hash}"
        ));
        match expected_err {
            SkillDiscoveryError::IntegrityError(msg) => {
                assert!(msg.contains(wrong_hash));
                assert!(msg.contains(&actual_hash));
            }
            _ => panic!("expected IntegrityError"),
        }
    }

    #[test]
    fn test_list_cached_skills_empty() {
        let temp_dir = std::env::temp_dir().join(format!(
            "test_empty_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let discovery = make_discovery(temp_dir.clone());

        let skills = discovery.list_cached_skills();
        assert!(skills.is_empty());

        fs::create_dir_all(&temp_dir).unwrap();
        let skills = discovery.list_cached_skills();
        assert!(skills.is_empty());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_resolve_skill_cached_hits() {
        let temp_dir = std::env::temp_dir().join(format!(
            "test_hit_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let discovery = make_discovery(temp_dir.clone());

        let skill_dir = temp_dir.join("hit-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Hit").unwrap();

        let meta = SkillCacheMetadata {
            source_url: "https://example.com/hit.md".to_string(),
            cached_at: chrono::Utc::now().to_rfc3339(),
            version: "1.0.0".to_string(),
            sha256: None,
        };
        fs::write(
            skill_dir.join(".metadata"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        let found = discovery.resolve_skill_cached("hit-skill");
        assert!(found.is_some());
        assert_eq!(found.unwrap().file_name().unwrap(), "SKILL.md");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_resolve_skill_cached_miss_expired() {
        let temp_dir = std::env::temp_dir().join(format!(
            "test_miss_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let discovery = SkillDiscovery::new(temp_dir.clone(), Duration::from_secs(1));

        let skill_dir = temp_dir.join("expired-miss");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Old").unwrap();

        let old_time = chrono::Utc::now() - chrono::Duration::seconds(10);
        let meta = SkillCacheMetadata {
            source_url: "https://example.com/old.md".to_string(),
            cached_at: old_time.to_rfc3339(),
            version: "0.1.0".to_string(),
            sha256: None,
        };
        fs::write(
            skill_dir.join(".metadata"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        let found = discovery.resolve_skill_cached("expired-miss");
        assert!(found.is_none());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_cleanup_expired_removes_old() {
        let temp_dir = std::env::temp_dir().join(format!(
            "test_cleanup_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let discovery = SkillDiscovery::new(temp_dir.clone(), Duration::from_secs(1));
        fs::create_dir_all(&temp_dir).unwrap();

        let fresh_dir = temp_dir.join("fresh");
        fs::create_dir_all(&fresh_dir).unwrap();
        fs::write(fresh_dir.join("SKILL.md"), "# Fresh").unwrap();
        let fresh_meta = SkillCacheMetadata {
            source_url: "https://example.com/fresh.md".to_string(),
            cached_at: chrono::Utc::now().to_rfc3339(),
            version: "1.0.0".to_string(),
            sha256: None,
        };
        fs::write(
            fresh_dir.join(".metadata"),
            serde_json::to_string_pretty(&fresh_meta).unwrap(),
        )
        .unwrap();

        let expired_dir = temp_dir.join("expired");
        fs::create_dir_all(&expired_dir).unwrap();
        fs::write(expired_dir.join("SKILL.md"), "# Expired").unwrap();
        let expired_time = chrono::Utc::now() - chrono::Duration::seconds(10);
        let expired_meta = SkillCacheMetadata {
            source_url: "https://example.com/expired.md".to_string(),
            cached_at: expired_time.to_rfc3339(),
            version: "0.1.0".to_string(),
            sha256: None,
        };
        fs::write(
            expired_dir.join(".metadata"),
            serde_json::to_string_pretty(&expired_meta).unwrap(),
        )
        .unwrap();

        assert!(fresh_dir.exists());
        assert!(expired_dir.exists());

        discovery.cleanup_expired();

        assert!(!expired_dir.exists(), "expired skill should be removed");
        assert!(fresh_dir.exists(), "fresh skill should remain");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
