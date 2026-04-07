use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Parsed representation of a remote skill index file.
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteSkillIndex {
    pub skills: Vec<RemoteSkillEntry>,
}

/// A single skill entry from the remote index.
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteSkillEntry {
    pub name: String,
    pub files: Vec<String>,
}

/// Facade for remote skill discovery operations.
///
/// Provides methods to fetch a remote skill index and download
/// skill files to a local cache directory.
pub struct RemoteSkillDiscovery;

impl RemoteSkillDiscovery {
    /// Fetch skills from a remote URL and download them to `cache_dir`.
    ///
    /// # Arguments
    /// * `url` - Base URL of the remote skill repository (e.g. `https://example.com/skills`)
    /// * `cache_dir` - Local directory to store downloaded skill files
    ///
    /// # Returns
    /// `Vec<PathBuf>` containing paths to each successfully downloaded skill directory.
    ///
    /// # Behavior
    /// - Fetches `<url>/index.json` and parses it into a [`RemoteSkillIndex`].
    /// - For each skill, validates that `SKILL.md` is present in the file list.
    ///   Skills without `SKILL.md` are skipped with a warning.
    /// - Downloads each file from `<url>/<skill-name>/<file>` into `cache_dir/<skill-name>/`.
    /// - Individual skill failures are warned but do not abort the entire pull.
    pub fn pull(url: &str, cache_dir: &Path) -> Result<Vec<PathBuf>, String> {
        fs::create_dir_all(cache_dir).map_err(|e| {
            format!(
                "failed to create cache directory '{}': {e}",
                cache_dir.display()
            )
        })?;

        let index_url = format!("{url}/index.json");
        let resp = reqwest::blocking::get(&index_url)
            .map_err(|e| format!("failed to fetch index from '{index_url}': {e}"))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(format!(
                "index request returned HTTP {status} from '{index_url}'"
            ));
        }

        let index: RemoteSkillIndex = resp
            .json()
            .map_err(|e| format!("failed to parse index.json from '{index_url}': {e}"))?;

        let mut downloaded_dirs = Vec::new();

        for skill in &index.skills {
            if !skill.files.iter().any(|f| f == "SKILL.md") {
                eprintln!(
                    "[skill_discovery] skipping skill '{}': SKILL.md not in file list",
                    skill.name
                );
                continue;
            }

            let skill_dir = cache_dir.join(&skill.name);

            match Self::download_skill(url, skill, &skill_dir) {
                Ok(()) => {
                    downloaded_dirs.push(skill_dir);
                }
                Err(e) => {
                    eprintln!(
                        "[skill_discovery] failed to download skill '{}': {e}",
                        skill.name
                    );
                }
            }
        }

        Ok(downloaded_dirs)
    }

    fn download_skill(
        base_url: &str,
        skill: &RemoteSkillEntry,
        skill_dir: &Path,
    ) -> Result<(), String> {
        fs::create_dir_all(skill_dir).map_err(|e| {
            format!(
                "failed to create skill directory '{}': {e}",
                skill_dir.display()
            )
        })?;

        for file in &skill.files {
            let file_url = format!("{base_url}/{}/{}", skill.name, file);
            let dest_path = skill_dir.join(file);

            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!("failed to create directory '{}': {e}", parent.display())
                })?;
            }

            let resp = reqwest::blocking::get(&file_url)
                .map_err(|e| format!("failed to fetch '{file_url}': {e}"))?;

            let status = resp.status();
            if !status.is_success() {
                return Err(format!("download of '{file_url}' returned HTTP {status}"));
            }

            let bytes = resp
                .bytes()
                .map_err(|e| format!("failed to read body from '{file_url}': {e}"))?;

            let mut file = fs::File::create(&dest_path)
                .map_err(|e| format!("failed to create file '{}': {e}", dest_path.display()))?;

            file.write_all(&bytes)
                .map_err(|e| format!("failed to write file '{}': {e}", dest_path.display()))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_remote_skill_entry_deserialize() {
        let json = r#"{"skills":[{"name":"test-skill","files":["SKILL.md","helper.sh"]}]}"#;
        let index: RemoteSkillIndex = serde_json::from_str(json).expect("should parse");
        assert_eq!(index.skills.len(), 1);
        assert_eq!(index.skills[0].name, "test-skill");
        assert_eq!(index.skills[0].files, vec!["SKILL.md", "helper.sh"]);
    }

    #[test]
    fn test_pull_invalid_url_returns_error() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let result = RemoteSkillDiscovery::pull("http://localhost:1", temp_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_pull_nonexistent_url_returns_error() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let result = RemoteSkillDiscovery::pull(
            "http://this-domain-definitely-does-not-exist-12345.com",
            temp_dir.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_pull_creates_cache_dir() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let cache_dir = temp_dir.path().join("skills");
        assert!(!cache_dir.exists());

        let _ = RemoteSkillDiscovery::pull("http://localhost:1", &cache_dir);

        assert!(cache_dir.exists());
    }

    #[test]
    fn test_skill_dir_structure() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let skill_dir = temp_dir.path().join("my-skill");

        assert!(!skill_dir.exists());
        fs::create_dir_all(&skill_dir).expect("should create skill dir");
        assert!(skill_dir.exists());

        let nested = skill_dir.join("scripts");
        fs::create_dir_all(&nested).expect("should create nested dir");
        assert!(nested.exists());
    }

    #[test]
    fn test_skill_entry_without_skill_md_should_be_skipped() {
        let skill_without = RemoteSkillEntry {
            name: "bad-skill".to_string(),
            files: vec!["README.md".to_string(), "helper.sh".to_string()],
        };
        let has_skill_md = skill_without.files.iter().any(|f| f == "SKILL.md");
        assert!(!has_skill_md);

        let skill_with = RemoteSkillEntry {
            name: "good-skill".to_string(),
            files: vec!["SKILL.md".to_string(), "helper.sh".to_string()],
        };
        let has_skill_md = skill_with.files.iter().any(|f| f == "SKILL.md");
        assert!(has_skill_md);
    }
}
