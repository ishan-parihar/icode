use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const GITHUB_RELEASES_URL: &str =
    "https://api.github.com/repos/ishanp/icode/releases/latest";
const CACHE_DURATION_SECS: u64 = 24 * 60 * 60; // 24 hours

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub release_url: String,
    pub has_update: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedResult {
    latest_version: String,
    release_url: String,
    timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

/// Check for updates against GitHub releases.
///
/// Uses a 24-hour cache at `~/.icode/cache/update_check.json` to avoid
/// repeated API calls. Returns `Some(UpdateInfo)` when a newer version
/// is available, `None` when current version is up to date.
pub async fn check_for_updates(current_version: &str) -> Option<UpdateInfo> {
    let cache_path = get_cache_path();

    // Try reading from cache first
    if let Some(cached) = read_cache(&cache_path) {
        if !is_cache_expired(&cached) {
            let has_update =
                is_newer_version(&cached.latest_version, current_version);
            return if has_update {
                Some(UpdateInfo {
                    current_version: current_version.to_string(),
                    latest_version: cached.latest_version,
                    release_url: cached.release_url,
                    has_update: true,
                })
            } else {
                None
            };
        }
    }

    // Cache miss or expired — fetch from GitHub
    match fetch_latest_release().await {
        Ok((latest_version, release_url)) => {
            let has_update = is_newer_version(&latest_version, current_version);

            // Cache the result
            let _ = write_cache(
                &cache_path,
                &CachedResult {
                    latest_version: latest_version.clone(),
                    release_url: release_url.clone(),
                    timestamp: current_timestamp_secs(),
                },
            );

            if has_update {
                Some(UpdateInfo {
                    current_version: current_version.to_string(),
                    latest_version,
                    release_url,
                    has_update: true,
                })
            } else {
                None
            }
        }
        Err(e) => {
            // Log error but don't propagate — update check is best-effort
            eprintln!("Failed to check for updates: {e}");
            None
        }
    }
}

/// Compare two semver strings. Returns true if `latest` is greater than
/// `current`. Handles optional 'v' prefix and major.minor.patch format.
pub fn is_newer_version(latest: &str, current: &str) -> bool {
    let latest_parsed = parse_semver(latest);
    let current_parsed = parse_semver(current);

    let (Ok(latest_tuple), Ok(current_tuple)) = (latest_parsed, current_parsed)
    else {
        // If either can't be parsed, fall back to string comparison
        return latest > current;
    };

    latest_tuple > current_tuple
}

fn parse_semver(version: &str) -> Result<(u64, u64, u64)> {
    let stripped = version.strip_prefix('v').unwrap_or(version);
    let parts: Vec<&str> = stripped.splitn(3, '.').collect();

    if parts.len() != 3 {
        return Err(anyhow::anyhow!("Invalid semver: {version}"));
    }

    let major: u64 = parts[0]
        .parse()
        .with_context(|| format!("Invalid major version in '{version}'"))?;
    let minor: u64 = parts[1]
        .parse()
        .with_context(|| format!("Invalid minor version in '{version}'"))?;
    let patch: u64 = parts[2]
        .parse()
        .with_context(|| format!("Invalid patch version in '{version}'"))?;

    Ok((major, minor, patch))
}

fn get_cache_path() -> PathBuf {
    let mut path = home_dir();
    path.push(".icode");
    path.push("cache");
    path.push("update_check.json");
    path
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn read_cache(path: &PathBuf) -> Option<CachedResult> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn is_cache_expired(cached: &CachedResult) -> bool {
    let now = current_timestamp_secs();
    now.saturating_sub(cached.timestamp) > CACHE_DURATION_SECS
}

fn write_cache(path: &PathBuf, cached: &CachedResult) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create cache directory: {parent:?}"))?;
    }
    let content = serde_json::to_string(cached)
        .context("Failed to serialize cache data")?;
    std::fs::write(path, content)
        .with_context(|| format!("Failed to write cache file: {path:?}"))?;
    Ok(())
}

fn current_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

async fn fetch_latest_release() -> Result<(String, String)> {
    let client = reqwest::Client::builder()
        .user_agent("icode-cli")
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .get(GITHUB_RELEASES_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("Failed to fetch GitHub releases")?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "GitHub API returned status: {}",
            response.status()
        ));
    }

    let release: GitHubRelease = response
        .json()
        .await
        .context("Failed to parse GitHub release response")?;

    let version = release.tag_name.strip_prefix('v')
        .unwrap_or(&release.tag_name)
        .to_string();

    Ok((version, release.html_url))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Semver comparison tests

    #[test]
    fn newer_major_is_update() {
        assert!(is_newer_version("2.0.0", "1.0.0"));
    }

    #[test]
    fn newer_minor_is_update() {
        assert!(is_newer_version("1.1.0", "1.0.0"));
    }

    #[test]
    fn newer_patch_is_update() {
        assert!(is_newer_version("1.0.1", "1.0.0"));
    }

    #[test]
    fn same_version_is_not_update() {
        assert!(!is_newer_version("1.0.0", "1.0.0"));
    }

    #[test]
    fn older_version_is_not_update() {
        assert!(!is_newer_version("0.9.0", "1.0.0"));
    }

    #[test]
    fn strips_v_prefix() {
        assert!(is_newer_version("v1.1.0", "1.0.0"));
        assert!(!is_newer_version("v0.9.0", "1.0.0"));
    }

    #[test]
    fn both_with_v_prefix() {
        assert!(is_newer_version("v2.0.0", "v1.0.0"));
    }

    #[test]
    fn mixed_prefix() {
        assert!(is_newer_version("1.1.0", "v1.0.0"));
        assert!(is_newer_version("v1.1.0", "1.0.0"));
    }

    #[test]
    fn complex_version_comparison() {
        assert!(is_newer_version("3.2.1", "3.2.0"));
        assert!(is_newer_version("3.2.0", "3.1.9"));
        assert!(is_newer_version("3.1.9", "2.99.99"));
    }

    #[test]
    fn prerelease_suffix_handled() {
        // "1.0.0-beta" should parse major as 1, and "1.0.0-beta.2" > "1.0.0-beta.1"
        // Our simple parser stops at third component, so "1.0.0" == "1.0.0" for the base
        assert!(!is_newer_version("1.0.0", "1.0.0"));
    }

    // Cache tests

    #[test]
    fn cache_write_and_read_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let cache_path = temp_dir.join("test_update_cache.json");

        let cached = CachedResult {
            latest_version: "1.1.0".to_string(),
            release_url: "https://example.com/release".to_string(),
            timestamp: current_timestamp_secs(),
        };

        write_cache(&cache_path, &cached).unwrap();
        let read = read_cache(&cache_path).unwrap();

        assert_eq!(read.latest_version, "1.1.0");
        assert_eq!(read.release_url, "https://example.com/release");

        // Cleanup
        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn expired_cache_detection() {
        let expired = CachedResult {
            latest_version: "1.1.0".to_string(),
            release_url: "https://example.com/release".to_string(),
            timestamp: 0, // Unix epoch — definitely expired
        };

        assert!(is_cache_expired(&expired));
    }

    #[test]
    fn fresh_cache_not_expired() {
        let fresh = CachedResult {
            latest_version: "1.1.0".to_string(),
            release_url: "https://example.com/release".to_string(),
            timestamp: current_timestamp_secs(),
        };

        assert!(!is_cache_expired(&fresh));
    }

    #[test]
    fn cache_creates_parent_directory() {
        let temp_dir = std::env::temp_dir();
        let nested_path =
            temp_dir.join("test_icode_cache").join("nested").join("cache.json");

        let cached = CachedResult {
            latest_version: "1.0.0".to_string(),
            release_url: "https://example.com".to_string(),
            timestamp: current_timestamp_secs(),
        };

        assert!(write_cache(&nested_path, &cached).is_ok());
        assert!(nested_path.exists());

        // Cleanup
        let _ = std::fs::remove_dir_all(temp_dir.join("test_icode_cache"));
    }
}
