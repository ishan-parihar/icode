use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DiscoveredSkill {
    pub name: String,
    pub description: String,
    pub content: String,
    pub source: SkillSource,
}

#[derive(Debug, Clone)]
pub enum SkillSource {
    Local(PathBuf),
    Git(String),
}

pub fn discover_skills(paths: &[String]) -> Vec<DiscoveredSkill> {
    let mut skills = Vec::new();
    for path in paths {
        let path = Path::new(path);
        if path.is_dir() {
            discover_skills_in_dir(path, &mut skills);
        } else if path.is_file() && path.extension().map_or(false, |e| e == "md") {
            if let Ok(content) = fs::read_to_string(path) {
                skills.push(DiscoveredSkill {
                    name: path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    description: String::new(),
                    content,
                    source: SkillSource::Local(path.to_path_buf()),
                });
            }
        }
    }
    skills
}

fn discover_skills_in_dir(dir: &Path, skills: &mut Vec<DiscoveredSkill>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |e| e == "md") {
                if let Ok(content) = fs::read_to_string(&path) {
                    skills.push(DiscoveredSkill {
                        name: path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        description: String::new(),
                        content,
                        source: SkillSource::Local(path.clone()),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn discovers_skills_from_directory() {
        let dir = tempfile::tempdir().unwrap();
        let mut file1 = fs::File::create(dir.path().join("test_skill.md")).unwrap();
        file1.write_all(b"# Test Skill\nSome content").unwrap();
        let skills = discover_skills(&[dir.path().to_string_lossy().to_string()]);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "test_skill");
        assert!(skills[0].content.contains("Test Skill"));
    }

    #[test]
    fn discovers_single_skill_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("my_skill.md");
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(b"# My Skill").unwrap();
        let skills = discover_skills(&[file_path.to_string_lossy().to_string()]);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "my_skill");
    }

    #[test]
    fn ignores_non_md_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::File::create(dir.path().join("readme.txt")).unwrap();
        fs::File::create(dir.path().join("skill.rs")).unwrap();
        let skills = discover_skills(&[dir.path().to_string_lossy().to_string()]);
        assert_eq!(skills.len(), 0);
    }
}
