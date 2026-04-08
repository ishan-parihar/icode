use std::sync::Arc;
use tokio::sync::RwLock;

use super::skill_discovery::DiscoveredSkill;

#[derive(Clone)]
pub struct SharedSkillIndex {
    inner: Arc<RwLock<SkillIndex>>,
}

pub struct SkillIndex {
    skills: Vec<DiscoveredSkill>,
}

impl Default for SkillIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillIndex {
    #[must_use]
    pub fn new() -> Self {
        Self { skills: Vec::new() }
    }

    pub fn add_skills(&mut self, skills: Vec<DiscoveredSkill>) {
        self.skills.extend(skills);
    }

    #[must_use]
    pub fn search(&self, query: &str) -> Vec<&DiscoveredSkill> {
        let query_lower = query.to_lowercase();
        self.skills
            .iter()
            .filter(|s| {
                s.name.to_lowercase().contains(&query_lower)
                    || s.description.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    #[must_use]
    pub fn list(&self) -> &[DiscoveredSkill] {
        &self.skills
    }
}

impl SharedSkillIndex {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(SkillIndex::new())),
        }
    }

    pub async fn search(&self, query: &str) -> Vec<DiscoveredSkill> {
        let guard = self.inner.read().await;
        guard.search(query).into_iter().cloned().collect()
    }

    pub async fn add_skills(&self, skills: Vec<DiscoveredSkill>) {
        let mut guard = self.inner.write().await;
        guard.add_skills(skills);
    }

    pub async fn list(&self) -> Vec<DiscoveredSkill> {
        let guard = self.inner.read().await;
        guard.skills.clone()
    }
}

impl Default for SharedSkillIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill_discovery::{DiscoveredSkill, SkillSource};

    #[tokio::test]
    async fn shared_skill_index_adds_and_lists() {
        let index = SharedSkillIndex::new();
        index
            .add_skills(vec![DiscoveredSkill {
                name: "test".to_string(),
                description: String::new(),
                content: String::new(),
                source: SkillSource::Local(".".into()),
            }])
            .await;
        let skills = index.list().await;
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "test");
    }

    #[tokio::test]
    async fn shared_skill_index_searches() {
        let index = SharedSkillIndex::new();
        index
            .add_skills(vec![
                DiscoveredSkill {
                    name: "react-hooks".to_string(),
                    description: "React hooks guide".to_string(),
                    content: String::new(),
                    source: SkillSource::Local(".".into()),
                },
                DiscoveredSkill {
                    name: "python-tips".to_string(),
                    description: String::new(),
                    content: String::new(),
                    source: SkillSource::Local(".".into()),
                },
            ])
            .await;
        let results = index.search("react").await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "react-hooks");
    }
}
