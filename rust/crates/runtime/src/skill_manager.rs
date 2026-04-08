use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::skill_registry::SkillRegistry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillSource {
    Local,
    Remote,
    Bundled,
}

#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub content: String,
    pub source: SkillSource,
    pub shadowed_by: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DiscoveredSkill {
    pub name: String,
    pub description: String,
    pub content: String,
    pub source: SkillSource,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillIndexEntry {
    pub name: String,
    pub description: String,
    pub url: String,
    pub version: String,
    pub sha256: Option<String>,
}

#[derive(Debug)]
pub enum SkillManagerError {
    FetchError(String),
    ParseError(String),
    DownloadError(String),
    CacheError(String),
    IntegrityError(String),
}

impl std::fmt::Display for SkillManagerError {
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

impl std::error::Error for SkillManagerError {}

pub const DEFAULT_REMOTE_INDEX_URL: &str = "https://index.opencode.ai/skills/index.json";

const DEFAULT_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SkillCacheMetadata {
    source_url: String,
    cached_at: String,
    version: String,
    sha256: Option<String>,
}

pub struct SkillManager {
    local_registry: SkillRegistry,
    remote_cache_dir: PathBuf,
    remote_index_url: String,
    ttl: Duration,
    agent_overrides: HashMap<String, Vec<String>>,
}

impl SkillManager {
    #[must_use]
    pub fn new(local_roots: &[PathBuf], remote_cache_dir: PathBuf) -> Self {
        Self {
            local_registry: SkillRegistry::discover(local_roots),
            remote_cache_dir,
            remote_index_url: DEFAULT_REMOTE_INDEX_URL.to_string(),
            ttl: DEFAULT_TTL,
            agent_overrides: HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_remote_index_url(mut self, url: impl Into<String>) -> Self {
        self.remote_index_url = url.into();
        self
    }

    #[must_use]
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    #[must_use]
    pub fn with_agent_overrides(
        mut self,
        agent_name: impl Into<String>,
        skill_names: Vec<String>,
    ) -> Self {
        self.agent_overrides.insert(agent_name.into(), skill_names);
        self
    }

    pub async fn refresh_remote(&self) -> Result<Vec<DiscoveredSkill>, SkillManagerError> {
        let entries = self.fetch_index(&self.remote_index_url).await?;
        let mut discovered = Vec::new();

        for entry in &entries {
            let result = self.download_skill(entry).await;
            match result {
                Ok(path) => {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let description = parse_skill_description(&content);
                        discovered.push(DiscoveredSkill {
                            name: entry.name.clone(),
                            description,
                            content,
                            source: SkillSource::Remote,
                        });
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[skill_manager] failed to download skill '{}': {e}",
                        entry.name
                    );
                }
            }
        }

        Ok(discovered)
    }

    #[must_use]
    pub fn all(&self) -> Vec<SkillInfo> {
        let mut result: HashMap<String, SkillInfo> = HashMap::new();

        for entry in self.local_registry.all() {
            result.insert(
                entry.name.clone(),
                SkillInfo {
                    name: entry.name.clone(),
                    description: entry.description.clone(),
                    path: entry.path.clone(),
                    content: entry.content.clone(),
                    source: SkillSource::Local,
                    shadowed_by: None,
                },
            );
        }

        let local_names: std::collections::HashSet<String> = result.keys().cloned().collect();
        for (name, info) in self.remote_skills_internal() {
            if !local_names.contains(&name) {
                result.insert(
                    name,
                    SkillInfo {
                        shadowed_by: None,
                        ..info
                    },
                );
            }
        }

        let mut skills: Vec<SkillInfo> = result.into_values().collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<SkillInfo> {
        self.all().into_iter().find(|s| s.name == name)
    }

    #[must_use]
    pub fn local(&self) -> Vec<SkillInfo> {
        self.all()
            .into_iter()
            .filter(|s| matches!(s.source, SkillSource::Local))
            .collect()
    }

    #[must_use]
    pub fn remote(&self) -> Vec<SkillInfo> {
        self.all()
            .into_iter()
            .filter(|s| matches!(s.source, SkillSource::Remote))
            .collect()
    }

    #[must_use]
    pub fn bundled(&self) -> Vec<SkillInfo> {
        self.all()
            .into_iter()
            .filter(|s| matches!(s.source, SkillSource::Bundled))
            .collect()
    }

    #[must_use]
    pub fn available_for_agent(&self, agent_name: Option<&str>) -> Vec<SkillInfo> {
        let all = self.all();
        let Some(agent) = agent_name else {
            return all;
        };

        if let Some(allowed) = self.agent_overrides.get(agent) {
            let allowed_set: std::collections::HashSet<&str> =
                allowed.iter().map(String::as_str).collect();
            all.into_iter()
                .filter(|s| allowed_set.contains(s.name.as_str()))
                .collect()
        } else {
            all
        }
    }

    #[must_use]
    pub fn skill_dirs(&self) -> Vec<PathBuf> {
        self.all()
            .into_iter()
            .filter_map(|s| s.path.parent().map(PathBuf::from))
            .collect()
    }

    async fn fetch_index(&self, url: &str) -> Result<Vec<SkillIndexEntry>, SkillManagerError> {
        #[derive(Deserialize)]
        struct IndexWrapper {
            skills: Vec<SkillIndexEntry>,
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| {
                SkillManagerError::FetchError(format!("failed to build HTTP client: {e}"))
            })?;

        let resp =
            client.get(url).send().await.map_err(|e| {
                SkillManagerError::FetchError(format!("failed to fetch {url}: {e}"))
            })?;

        if !resp.status().is_success() {
            return Err(SkillManagerError::FetchError(format!(
                "HTTP {} from {}",
                resp.status(),
                url
            )));
        }

        let body = resp.text().await.map_err(|e| {
            SkillManagerError::FetchError(format!("failed to read response body: {e}"))
        })?;

        if let Ok(entries) = serde_json::from_str::<Vec<SkillIndexEntry>>(&body) {
            return Ok(entries);
        }

        if let Ok(wrapper) = serde_json::from_str::<IndexWrapper>(&body) {
            return Ok(wrapper.skills);
        }

        Err(SkillManagerError::ParseError(format!(
            "index.json at {url} is neither a skill array nor an object with 'skills' key"
        )))
    }

    async fn download_skill(&self, entry: &SkillIndexEntry) -> Result<PathBuf, SkillManagerError> {
        let skill_dir = self.remote_cache_dir.join(&entry.name);
        std::fs::create_dir_all(&skill_dir).map_err(|e| {
            SkillManagerError::DownloadError(format!("failed to create cache dir: {e}"))
        })?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| {
                SkillManagerError::DownloadError(format!("failed to build HTTP client: {e}"))
            })?;

        let resp = client.get(&entry.url).send().await.map_err(|e| {
            SkillManagerError::DownloadError(format!("failed to download skill: {e}"))
        })?;

        if !resp.status().is_success() {
            return Err(SkillManagerError::DownloadError(format!(
                "HTTP {} from {}",
                resp.status(),
                entry.url
            )));
        }

        let content = resp.bytes().await.map_err(|e| {
            SkillManagerError::DownloadError(format!("failed to read skill content: {e}"))
        })?;

        if let Some(expected_hash) = &entry.sha256 {
            let actual_hash = compute_sha256(&content);
            if actual_hash != *expected_hash {
                return Err(SkillManagerError::IntegrityError(format!(
                    "sha256 mismatch for skill '{}': expected {expected_hash}, got {actual_hash}",
                    entry.name
                )));
            }
        }

        let skill_path = skill_dir.join("SKILL.md");
        std::fs::write(&skill_path, &content).map_err(|e| {
            SkillManagerError::DownloadError(format!("failed to write SKILL.md: {e}"))
        })?;

        let metadata = SkillCacheMetadata {
            source_url: entry.url.clone(),
            cached_at: chrono::Utc::now().to_rfc3339(),
            version: entry.version.clone(),
            sha256: entry.sha256.clone(),
        };
        let meta_path = skill_dir.join(".metadata");
        std::fs::write(
            &meta_path,
            serde_json::to_string_pretty(&metadata).map_err(|e| {
                SkillManagerError::CacheError(format!("failed to serialize metadata: {e}"))
            })?,
        )
        .map_err(|e| SkillManagerError::CacheError(format!("failed to write metadata: {e}")))?;

        Ok(skill_path)
    }

    fn remote_skills_internal(&self) -> Vec<(String, SkillInfo)> {
        let mut result = Vec::new();

        if !self.remote_cache_dir.exists() {
            return result;
        }

        let Ok(entries) = std::fs::read_dir(&self.remote_cache_dir) else {
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

            if is_cache_expired(&skill_path, self.ttl) {
                continue;
            }

            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if let Ok(content) = std::fs::read_to_string(&skill_path) {
                let description = parse_skill_description(&content);
                result.push((
                    name.clone(),
                    SkillInfo {
                        name,
                        description,
                        path: skill_path,
                        content,
                        source: SkillSource::Remote,
                        shadowed_by: None,
                    },
                ));
            }
        }

        result
    }
}

fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn is_cache_expired(skill_path: &Path, ttl: Duration) -> bool {
    let Some(parent) = skill_path.parent() else {
        return true;
    };
    let meta_path = parent.join(".metadata");

    let Ok(meta_content) = std::fs::read_to_string(&meta_path) else {
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

fn parse_skill_description(content: &str) -> String {
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("description:") {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_skill_dir(base: &Path, name: &str, description: &str, content: &str) -> PathBuf {
        let skill_dir = base.join(name);
        fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
        let skill_md = skill_dir.join("SKILL.md");
        let full_content = if description.is_empty() {
            content.to_string()
        } else {
            format!("---\ndescription: {description}\n---\n\n{content}")
        };
        fs::write(&skill_md, full_content).expect("failed to write SKILL.md");
        skill_dir
    }

    fn create_cached_skill(
        cache_dir: &Path,
        name: &str,
        description: &str,
        content: &str,
        cached_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> PathBuf {
        let skill_dir = cache_dir.join(name);
        fs::create_dir_all(&skill_dir).expect("failed to create cache dir");
        let skill_md = skill_dir.join("SKILL.md");
        let full_content = if description.is_empty() {
            content.to_string()
        } else {
            format!("---\ndescription: {description}\n---\n\n{content}")
        };
        fs::write(&skill_md, &full_content).expect("failed to write SKILL.md");
        let meta = SkillCacheMetadata {
            source_url: format!("https://example.com/{name}/SKILL.md"),
            cached_at: cached_at.unwrap_or_else(chrono::Utc::now).to_rfc3339(),
            version: "1.0.0".to_string(),
            sha256: None,
        };
        fs::write(
            skill_dir.join(".metadata"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();
        skill_md
    }

    #[test]
    fn test_local_discovery() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let cache_dir = tempfile::tempdir().expect("failed to create cache dir");
        create_skill_dir(
            temp_dir.path(),
            "frontend-design",
            "Create distinctive UI",
            "# Frontend Design",
        );
        create_skill_dir(
            temp_dir.path(),
            "audit",
            "Run quality checks",
            "# Audit Skill",
        );
        let manager = SkillManager::new(
            &[temp_dir.path().to_path_buf()],
            cache_dir.path().to_path_buf(),
        );
        let all = manager.all();
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|s| s.name == "frontend-design"));
        assert!(all.iter().any(|s| s.name == "audit"));
        assert!(all.iter().all(|s| matches!(s.source, SkillSource::Local)));
    }

    #[test]
    fn test_remote_merge() {
        let local_dir = tempfile::tempdir().expect("failed to create temp dir");
        let cache_dir = tempfile::tempdir().expect("failed to create cache dir");
        create_skill_dir(
            local_dir.path(),
            "local-skill",
            "Local skill",
            "Local content",
        );
        create_cached_skill(
            cache_dir.path(),
            "remote-alpha",
            "Remote skill alpha",
            "Remote alpha content",
            None,
        );
        create_cached_skill(
            cache_dir.path(),
            "remote-beta",
            "Remote skill beta",
            "Remote beta content",
            None,
        );
        let manager = SkillManager::new(
            &[local_dir.path().to_path_buf()],
            cache_dir.path().to_path_buf(),
        );
        let all = manager.all();
        assert_eq!(all.len(), 3);
        let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"local-skill"));
        assert!(names.contains(&"remote-alpha"));
        assert!(names.contains(&"remote-beta"));
        let local_skill = all.iter().find(|s| s.name == "local-skill").unwrap();
        assert!(matches!(local_skill.source, SkillSource::Local));
        let remote_skill = all.iter().find(|s| s.name == "remote-alpha").unwrap();
        assert!(matches!(remote_skill.source, SkillSource::Remote));
    }

    #[test]
    fn test_deduplication_local_shadows_remote() {
        let local_dir = tempfile::tempdir().expect("failed to create temp dir");
        let cache_dir = tempfile::tempdir().expect("failed to create cache dir");
        create_skill_dir(
            local_dir.path(),
            "shared-skill",
            "Local version",
            "Local content",
        );
        create_cached_skill(
            cache_dir.path(),
            "shared-skill",
            "Remote version",
            "Remote content",
            None,
        );
        create_cached_skill(
            cache_dir.path(),
            "unique-remote",
            "Unique remote",
            "Unique content",
            None,
        );
        let manager = SkillManager::new(
            &[local_dir.path().to_path_buf()],
            cache_dir.path().to_path_buf(),
        );
        let all = manager.all();
        assert_eq!(all.len(), 2);
        let shared = all.iter().find(|s| s.name == "shared-skill").unwrap();
        assert!(matches!(shared.source, SkillSource::Local));
        assert!(shared.content.contains("Local content"));
    }

    #[test]
    fn test_get_by_name() {
        let local_dir = tempfile::tempdir().expect("failed to create temp dir");
        let cache_dir = tempfile::tempdir().expect("failed to create cache dir");
        create_skill_dir(
            local_dir.path(),
            "my-skill",
            "My skill desc",
            "My skill content",
        );
        create_cached_skill(
            cache_dir.path(),
            "cached-skill",
            "Cached desc",
            "Cached content",
            None,
        );
        let manager = SkillManager::new(
            &[local_dir.path().to_path_buf()],
            cache_dir.path().to_path_buf(),
        );
        let found = manager.get("my-skill");
        assert!(found.is_some());
        let skill = found.unwrap();
        assert_eq!(skill.name, "my-skill");
        assert!(matches!(skill.source, SkillSource::Local));
        let found = manager.get("cached-skill");
        assert!(found.is_some());
        assert!(matches!(found.unwrap().source, SkillSource::Remote));
        assert!(manager.get("nonexistent").is_none());
    }

    #[test]
    fn test_available_for_agent() {
        let local_dir = tempfile::tempdir().expect("failed to create temp dir");
        let cache_dir = tempfile::tempdir().expect("failed to create cache dir");
        create_skill_dir(local_dir.path(), "skill-a", "Skill A", "A");
        create_skill_dir(local_dir.path(), "skill-b", "Skill B", "B");
        create_skill_dir(local_dir.path(), "skill-c", "Skill C", "C");
        let manager = SkillManager::new(
            &[local_dir.path().to_path_buf()],
            cache_dir.path().to_path_buf(),
        )
        .with_agent_overrides(
            "restricted-agent",
            vec!["skill-a".to_string(), "skill-c".to_string()],
        );
        let all = manager.available_for_agent(None);
        assert_eq!(all.len(), 3);
        let restricted = manager.available_for_agent(Some("restricted-agent"));
        assert_eq!(restricted.len(), 2);
        let names: Vec<&str> = restricted.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"skill-a"));
        assert!(names.contains(&"skill-c"));
        assert!(!names.contains(&"skill-b"));
        let unknown = manager.available_for_agent(Some("unknown-agent"));
        assert_eq!(unknown.len(), 3);
    }

    #[test]
    fn test_skill_dirs() {
        let local_dir = tempfile::tempdir().expect("failed to create temp dir");
        let cache_dir = tempfile::tempdir().expect("failed to create cache dir");
        create_skill_dir(local_dir.path(), "skill-x", "X", "X content");
        let manager = SkillManager::new(
            &[local_dir.path().to_path_buf()],
            cache_dir.path().to_path_buf(),
        );
        let dirs = manager.skill_dirs();
        assert_eq!(dirs.len(), 1);
        assert!(dirs[0].ends_with("skill-x"));
    }

    #[test]
    fn test_empty_manager() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let manager = SkillManager::new(&[], temp_dir.path().to_path_buf());
        assert!(manager.all().is_empty());
        assert!(manager.local().is_empty());
        assert!(manager.remote().is_empty());
        assert!(manager.bundled().is_empty());
        assert!(manager.get("anything").is_none());
        assert!(manager.skill_dirs().is_empty());
    }

    #[test]
    fn test_expired_remote_skills_excluded() {
        let local_dir = tempfile::tempdir().expect("failed to create temp dir");
        let cache_dir = tempfile::tempdir().expect("failed to create cache dir");
        let now = chrono::Utc::now();
        let expired_time = now - chrono::Duration::hours(48);
        create_cached_skill(
            cache_dir.path(),
            "expired-skill",
            "Expired",
            "Expired content",
            Some(expired_time),
        );
        let manager = SkillManager::new(
            &[local_dir.path().to_path_buf()],
            cache_dir.path().to_path_buf(),
        );
        let all = manager.all();
        assert!(
            !all.iter().any(|s| s.name == "expired-skill"),
            "expired remote skill should not appear"
        );
    }

    #[test]
    fn test_builder_remote_index_url() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let manager = SkillManager::new(&[], temp_dir.path().to_path_buf())
            .with_remote_index_url("https://custom.example.com/skills.json");
        assert!(manager.all().is_empty());
    }
}
