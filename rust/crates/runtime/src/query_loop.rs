use crate::compact::{compact_session, estimate_session_tokens, CompactionConfig};
use crate::config::RuntimeFeatureConfig;
use crate::hooks::{HookAbortSignal, HookProgressReporter, HookRunner};
use crate::permissions::PermissionPolicy;
use crate::session::{ContentBlock, ConversationMessage, MessageRole, Session};
use crate::usage::{TokenUsage, UsageTracker};

#[derive(Debug)]
pub enum QueryOutcome {
    EndTurn {
        messages: Vec<ConversationMessage>,
        usage: TokenUsage,
    },
    MaxTokens {
        partial_message: ConversationMessage,
        usage: TokenUsage,
    },
    Cancelled,
    Error(String),
    BudgetExceeded {
        cost_usd: f64,
        limit_usd: f64,
    },
}

#[derive(Clone)]
pub struct QueryLoopConfig {
    pub max_turns: u32,
    pub max_budget_usd: Option<f64>,
    pub fallback_model: Option<String>,
    pub agent_name: Option<String>,
    pub agent_max_turns: Option<u32>,
    pub agent_temperature: Option<f64>,
    pub tool_result_budget: usize,
    pub compact_threshold: f32,
}

impl Default for QueryLoopConfig {
    fn default() -> Self {
        Self {
            max_turns: 30,
            max_budget_usd: None,
            fallback_model: None,
            agent_name: None,
            agent_max_turns: None,
            agent_temperature: None,
            tool_result_budget: 50_000,
            compact_threshold: 0.8,
        }
    }
}

#[expect(dead_code)]
pub struct EnhancedQueryLoop {
    session: Session,
    config: QueryLoopConfig,
    permission_policy: PermissionPolicy,
    system_prompt: Vec<String>,
    hook_runner: HookRunner,
    hook_abort_signal: HookAbortSignal,
    hook_progress_reporter: Option<Box<dyn HookProgressReporter>>,
    usage_tracker: UsageTracker,
}

impl EnhancedQueryLoop {
    #[must_use]
    pub fn new(
        session: Session,
        config: QueryLoopConfig,
        permission_policy: PermissionPolicy,
        system_prompt: Vec<String>,
        feature_config: &RuntimeFeatureConfig,
    ) -> Self {
        let usage_tracker = UsageTracker::from_session(&session);
        Self {
            session,
            config,
            permission_policy,
            system_prompt,
            hook_runner: HookRunner::from_feature_config(feature_config),
            hook_abort_signal: HookAbortSignal::default(),
            hook_progress_reporter: None,
            usage_tracker,
        }
    }

    #[must_use]
    pub fn with_hook_progress_reporter(mut self, reporter: Box<dyn HookProgressReporter>) -> Self {
        self.hook_progress_reporter = Some(reporter);
        self
    }

    pub fn apply_tool_result_budget(&mut self) {
        if self.config.tool_result_budget == 0 {
            return;
        }
        let total_chars: usize = self
            .session
            .messages
            .iter()
            .filter(|m| matches!(m.role, MessageRole::Tool))
            .flat_map(|m| m.blocks.iter())
            .filter_map(|b| match b {
                ContentBlock::ToolResult { output, .. } => Some(output.len()),
                _ => None,
            })
            .sum();
        if total_chars <= self.config.tool_result_budget {
            return;
        }
        let mut to_shed = total_chars - self.config.tool_result_budget;
        for msg in &mut self.session.messages {
            if !matches!(msg.role, MessageRole::Tool) {
                continue;
            }
            for block in &mut msg.blocks {
                if let ContentBlock::ToolResult { output, .. } = block {
                    let size = output.len();
                    if size == 0 {
                        continue;
                    }
                    *output = "[tool result truncated to save context]".to_string();
                    if size > to_shed {
                        return;
                    }
                    to_shed -= size;
                }
            }
        }
    }

    #[expect(dead_code)]
    fn should_auto_compact(&self) -> bool {
        let estimated = estimate_session_tokens(&self.session);
        let threshold = self
            .usage_tracker
            .cumulative_usage()
            .input_tokens
            .saturating_mul((self.config.compact_threshold * 1000.0) as u32)
            / 1000;
        estimated > threshold as usize
    }

    #[expect(dead_code)]
    fn maybe_auto_compact(&mut self) -> Option<crate::conversation::AutoCompactionEvent> {
        let result = compact_session(
            &self.session,
            CompactionConfig {
                max_estimated_tokens: 0,
                ..CompactionConfig::default()
            },
        );
        if result.removed_message_count == 0 {
            return None;
        }
        self.session = result.compacted_session;
        Some(crate::conversation::AutoCompactionEvent {
            removed_message_count: result.removed_message_count,
        })
    }

    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    #[must_use]
    pub fn into_session(self) -> Session {
        self.session
    }

    #[must_use]
    pub fn usage(&self) -> &UsageTracker {
        &self.usage_tracker
    }

    #[must_use]
    pub fn config(&self) -> &QueryLoopConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut QueryLoopConfig {
        &mut self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::PermissionMode;

    #[test]
    fn query_loop_config_defaults() {
        let config = QueryLoopConfig::default();
        assert_eq!(config.max_turns, 30);
        assert_eq!(config.tool_result_budget, 50_000);
        assert!((config.compact_threshold - 0.8).abs() < f32::EPSILON);
        assert!(config.max_budget_usd.is_none());
        assert!(config.fallback_model.is_none());
    }

    #[test]
    fn enhanced_query_loop_construction() {
        let session = Session::new();
        let policy = PermissionPolicy::new(PermissionMode::DangerFullAccess);
        let loop_ = EnhancedQueryLoop::new(
            session,
            QueryLoopConfig::default(),
            policy,
            vec!["system".to_string()],
            &RuntimeFeatureConfig::default(),
        );
        assert_eq!(loop_.session().messages.len(), 0);
        assert_eq!(loop_.usage().turns(), 0);
    }
}
