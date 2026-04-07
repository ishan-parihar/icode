use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionAction {
    Ask,
    Allow,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionDuration {
    Session,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub tool_name: String,
    pub action: PermissionAction,
    pub duration: PermissionDuration,
    pub input_pattern: Option<String>,
    pub reason: Option<String>,
}

impl PermissionRule {
    pub fn matches(&self, tool_name: &str, input: &str) -> bool {
        if self.tool_name != tool_name {
            return false;
        }
        if let Some(ref pattern) = self.input_pattern {
            return input.contains(pattern.trim_matches('*'));
        }
        true
    }

    pub fn effective_action(&self, tool_name: &str, input: &str) -> Option<PermissionAction> {
        if self.matches(tool_name, input) {
            Some(self.action.clone())
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionRuleStore {
    pub rules: Vec<PermissionRule>,
}

impl PermissionRuleStore {
    pub fn add_rule(&mut self, rule: PermissionRule) {
        self.rules.push(rule);
    }

    pub fn find_matching(&self, tool_name: &str, input: &str) -> Option<&PermissionRule> {
        self.rules.iter().find(|r| r.matches(tool_name, input))
    }

    pub fn clear_session_rules(&mut self) {
        self.rules
            .retain(|r| r.duration == PermissionDuration::Always);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_matches_tool_name_only() {
        let rule = PermissionRule {
            tool_name: "Bash".to_string(),
            action: PermissionAction::Allow,
            duration: PermissionDuration::Always,
            input_pattern: None,
            reason: None,
        };
        assert!(rule.matches("Bash", "ls"));
        assert!(!rule.matches("Read", "file.txt"));
    }

    #[test]
    fn rule_matches_with_input_pattern() {
        let rule = PermissionRule {
            tool_name: "Read".to_string(),
            action: PermissionAction::Allow,
            duration: PermissionDuration::Always,
            input_pattern: Some("src".to_string()),
            reason: None,
        };
        assert!(rule.matches("Read", "src/main.rs"));
        assert!(!rule.matches("Read", "tests/test.rs"));
    }

    #[test]
    fn effective_action_returns_none_on_no_match() {
        let rule = PermissionRule {
            tool_name: "Bash".to_string(),
            action: PermissionAction::Deny,
            duration: PermissionDuration::Session,
            input_pattern: None,
            reason: None,
        };
        assert!(rule.effective_action("Read", "file.txt").is_none());
        assert_eq!(
            rule.effective_action("Bash", "ls"),
            Some(PermissionAction::Deny)
        );
    }

    #[test]
    fn clear_session_rules_removes_session_only() {
        let mut store = PermissionRuleStore::default();
        store.add_rule(PermissionRule {
            tool_name: "Bash".into(),
            action: PermissionAction::Allow,
            duration: PermissionDuration::Session,
            input_pattern: None,
            reason: None,
        });
        store.add_rule(PermissionRule {
            tool_name: "Read".into(),
            action: PermissionAction::Allow,
            duration: PermissionDuration::Always,
            input_pattern: None,
            reason: None,
        });
        store.clear_session_rules();
        assert_eq!(store.rules.len(), 1);
        assert_eq!(store.rules[0].tool_name, "Read");
    }
}
