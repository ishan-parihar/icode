use async_trait::async_trait;
use hooks_engine::context::HookContext;
use hooks_engine::dispatcher::HookDispatcher;
use hooks_engine::event_types::{HookEvent, SessionEvent, SessionEventType};
use hooks_engine::hook_trait::{Hook, HookResult, ToolInput, ToolOutput};
use hooks_engine::registry::HookRegistry;
use std::sync::Arc;

// -- HookContext tests --

#[test]
fn hook_context_new_is_not_blocked() {
    let ctx = HookContext::new();
    assert!(!ctx.blocked);
    assert!(ctx.block_reason.is_none());
    assert!(ctx.injected_messages.is_empty());
    assert!(ctx.warnings.is_empty());
}

#[test]
fn hook_context_inject_message() {
    let mut ctx = HookContext::new();
    ctx.inject_message("hello".to_string());
    ctx.inject_message("world".to_string());
    assert_eq!(ctx.injected_messages, vec!["hello", "world"]);
}

#[test]
fn hook_context_block_sets_flag() {
    let mut ctx = HookContext::new();
    ctx.block("safety violation".to_string());
    assert!(ctx.blocked);
    assert_eq!(ctx.block_reason, Some("safety violation".to_string()));
}

#[test]
fn hook_context_add_warning() {
    let mut ctx = HookContext::new();
    ctx.add_warning("deprecated usage".to_string());
    ctx.add_warning("rate limit approaching".to_string());
    assert_eq!(
        ctx.warnings,
        vec!["deprecated usage", "rate limit approaching"]
    );
}

#[test]
fn hook_context_metadata_roundtrip() {
    let mut ctx = HookContext::new();
    ctx.set_metadata("key1".to_string(), "val1".to_string());
    ctx.set_metadata("key2".to_string(), "val2".to_string());
    assert_eq!(ctx.get_metadata("key1"), Some("val1"));
    assert_eq!(ctx.get_metadata("key2"), Some("val2"));
    assert_eq!(ctx.get_metadata("missing"), None);
}

// -- Dispatcher tests --

struct TestHook {
    hook_name: String,
    events: Vec<HookEvent>,
    pri: u8,
    block_on_call: bool,
    error_on_call: bool,
}

impl TestHook {
    fn new(name: &str, events: Vec<HookEvent>, pri: u8) -> Self {
        Self {
            hook_name: name.to_string(),
            events,
            pri,
            block_on_call: false,
            error_on_call: false,
        }
    }

    fn blocking(name: &str, events: Vec<HookEvent>, pri: u8) -> Self {
        Self {
            hook_name: name.to_string(),
            events,
            pri,
            block_on_call: true,
            error_on_call: false,
        }
    }

    fn erroring(name: &str, events: Vec<HookEvent>, pri: u8) -> Self {
        Self {
            hook_name: name.to_string(),
            events,
            pri,
            block_on_call: false,
            error_on_call: true,
        }
    }
}

#[async_trait]
impl Hook for TestHook {
    fn name(&self) -> &str {
        &self.hook_name
    }

    fn events(&self) -> Vec<HookEvent> {
        self.events.clone()
    }

    fn priority(&self) -> u8 {
        self.pri
    }

    async fn on_pre_tool_use(&self, ctx: &mut HookContext, _input: &mut ToolInput) -> HookResult {
        if self.error_on_call {
            return Err("test error".to_string());
        }
        if self.block_on_call {
            ctx.block("blocked by test hook".to_string());
        }
        Ok(())
    }

    async fn on_post_tool_use(
        &self,
        ctx: &mut HookContext,
        _output: &mut ToolOutput,
    ) -> HookResult {
        if self.error_on_call {
            return Err("test error".to_string());
        }
        if self.block_on_call {
            ctx.block("blocked by test hook".to_string());
        }
        Ok(())
    }
}

#[tokio::test]
async fn dispatcher_register_and_dispatch() {
    let mut dispatcher = HookDispatcher::new();
    let hook = Arc::new(TestHook::new("test", vec![HookEvent::PreToolUse], 50));
    dispatcher.register(hook);

    let mut ctx = HookContext::new();
    let mut input = ToolInput {
        tool_name: "bash".to_string(),
        arguments: serde_json::json!({}),
    };
    let result = dispatcher.dispatch_pre_tool_use(&mut ctx, &mut input).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn dispatcher_runs_hooks_in_priority_order() {
    let mut dispatcher = HookDispatcher::new();

    let low = Arc::new(TestHook::new("low", vec![HookEvent::PreToolUse], 100));
    let high = Arc::new(TestHook::new("high", vec![HookEvent::PreToolUse], 10));
    dispatcher.register(low);
    dispatcher.register(high);

    let hooks = dispatcher.active_hooks(&HookEvent::PreToolUse);
    assert_eq!(hooks[0].name(), "high");
    assert_eq!(hooks[1].name(), "low");
}

#[tokio::test]
async fn dispatcher_skips_disabled_hooks() {
    let mut dispatcher = HookDispatcher::new();
    let hook = Arc::new(TestHook::new(
        "disabled_test",
        vec![HookEvent::PreToolUse],
        50,
    ));
    dispatcher.register(hook);
    dispatcher.disable("disabled_test");

    let hooks = dispatcher.active_hooks(&HookEvent::PreToolUse);
    assert!(hooks.is_empty());
}

#[tokio::test]
async fn dispatcher_stops_on_block() {
    let mut dispatcher = HookDispatcher::new();
    dispatcher.register(Arc::new(TestHook::blocking(
        "blocker",
        vec![HookEvent::PreToolUse],
        10,
    )));
    dispatcher.register(Arc::new(TestHook::new(
        "should_not_run",
        vec![HookEvent::PreToolUse],
        20,
    )));

    let mut ctx = HookContext::new();
    let mut input = ToolInput {
        tool_name: "bash".to_string(),
        arguments: serde_json::json!({}),
    };
    let result = dispatcher.dispatch_pre_tool_use(&mut ctx, &mut input).await;
    assert!(result.is_ok());
    assert!(ctx.blocked);
}

#[tokio::test]
async fn dispatcher_stops_on_error() {
    let mut dispatcher = HookDispatcher::new();
    dispatcher.register(Arc::new(TestHook::erroring(
        "error_hook",
        vec![HookEvent::PreToolUse],
        10,
    )));

    let mut ctx = HookContext::new();
    let mut input = ToolInput {
        tool_name: "bash".to_string(),
        arguments: serde_json::json!({}),
    };
    let result = dispatcher.dispatch_pre_tool_use(&mut ctx, &mut input).await;
    assert!(result.is_err());
}

// -- Registry tests --

#[test]
fn registry_register_and_get() {
    let mut registry = HookRegistry::new();
    let hook = Arc::new(TestHook::new("my_hook", vec![HookEvent::PreToolUse], 50));
    registry.register(hook);

    let found = registry.get("my_hook");
    assert!(found.is_some());
    assert_eq!(found.unwrap().name(), "my_hook");

    assert!(registry.get("nonexistent").is_none());
}

#[test]
fn registry_into_dispatcher() {
    let mut registry = HookRegistry::new();
    registry.register(Arc::new(TestHook::new(
        "hook1",
        vec![HookEvent::PreToolUse],
        50,
    )));
    registry.register(Arc::new(TestHook::new(
        "hook2",
        vec![HookEvent::PostToolUse],
        50,
    )));

    let dispatcher = registry.into_dispatcher();
    assert_eq!(dispatcher.active_hooks(&HookEvent::PreToolUse).len(), 1);
    assert_eq!(dispatcher.active_hooks(&HookEvent::PostToolUse).len(), 1);
}

// -- Serialization tests --

#[test]
fn hook_event_serialize() {
    let event = HookEvent::PreToolUse;
    let json = serde_json::to_string(&event).unwrap();
    assert_eq!(json, "\"PreToolUse\"");

    let deserialized: HookEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, event);
}

#[test]
fn session_event_serialize() {
    let session_event = SessionEvent {
        r#type: SessionEventType::Created,
        session_id: "sess-123".to_string(),
        timestamp: "2024-01-01T00:00:00Z".to_string(),
    };
    let json = serde_json::to_string(&session_event).unwrap();
    assert!(json.contains("Created"));
    assert!(json.contains("sess-123"));

    let error_event = SessionEvent {
        r#type: SessionEventType::Error("oops".to_string()),
        session_id: "sess-456".to_string(),
        timestamp: "2024-01-01T00:00:00Z".to_string(),
    };
    let json = serde_json::to_string(&error_event).unwrap();
    assert!(json.contains("Error"));
    assert!(json.contains("oops"));
}

// -- Integration tests for new hooks --

use hooks_engine::hook_trait::{ApiParams, Message};
use hooks_engine::hooks::{
    CommentChecker, ContextWindowMonitor, KeywordDetector, RalphLoop, RulesInjector,
    SessionRecovery, StartWork, ThinkMode, TodoContinuationEnforcer, ToolOutputTruncator,
    Ultrawork,
};

// KeywordDetector tests

#[tokio::test]
async fn keyword_detector_injects_ultrawork_prompt() {
    let hook = KeywordDetector;
    let mut ctx = HookContext::new();
    let mut msg = Message {
        role: "user".into(),
        content: "let's do ultrawork".into(),
    };
    hook.on_message(&mut ctx, &mut msg).await.unwrap();
    assert_eq!(ctx.injected_messages.len(), 1);
    assert!(ctx.injected_messages[0].contains("Ultrawork"));
}

#[tokio::test]
async fn keyword_detector_injects_multiple_prompts() {
    let hook = KeywordDetector;
    let mut ctx = HookContext::new();
    let mut msg = Message {
        role: "user".into(),
        content: "search and analyze the code".into(),
    };
    hook.on_message(&mut ctx, &mut msg).await.unwrap();
    assert_eq!(ctx.injected_messages.len(), 2);
}

// Ultrawork tests

#[tokio::test]
async fn ultrawork_hook_priority_is_five() {
    let hook = Ultrawork;
    assert_eq!(hook.priority(), 5);
}

#[tokio::test]
async fn ultrawork_injects_system_msg() {
    let hook = Ultrawork;
    let mut ctx = HookContext::new();
    let mut msg = Message {
        role: "user".into(),
        content: "activate ulw".into(),
    };
    hook.on_message(&mut ctx, &mut msg).await.unwrap();
    assert_eq!(ctx.injected_messages.len(), 1);
    assert!(ctx.injected_messages[0].contains("ULTRAWORK"));
}

// TodoContinuationEnforcer tests

#[tokio::test]
async fn todo_enforcer_warnings_on_false_completion() {
    let hook = TodoContinuationEnforcer;
    let mut ctx = HookContext::new();
    ctx.set_metadata("pending_todos".into(), "2".into());
    let mut msg = Message {
        role: "assistant".into(),
        content: "Complete!".into(),
    };
    hook.on_message(&mut ctx, &mut msg).await.unwrap();
    assert_eq!(ctx.injected_messages.len(), 1);
    assert!(ctx.injected_messages[0].contains("WARNING"));
}

#[tokio::test]
async fn todo_enforcer_ignores_zero_pending() {
    let hook = TodoContinuationEnforcer;
    let mut ctx = HookContext::new();
    ctx.set_metadata("pending_todos".into(), "0".into());
    let mut msg = Message {
        role: "assistant".into(),
        content: "Done".into(),
    };
    hook.on_message(&mut ctx, &mut msg).await.unwrap();
    assert_eq!(ctx.injected_messages.len(), 0);
}

// CommentChecker tests

#[tokio::test]
async fn comment_checker_flags_excessive_comments() {
    let hook = CommentChecker;
    let mut ctx = HookContext::new();
    let mut output = ToolOutput {
        tool_name: "write_file".into(),
        result: "// TODO: fix\n// HACK: temp\n/// docs here\n// FIXME: bug".into(),
    };
    hook.on_post_tool_use(&mut ctx, &mut output).await.unwrap();
    assert_eq!(ctx.warnings.len(), 1);
}

// SessionRecovery tests

#[tokio::test]
async fn session_recovery_handles_empty() {
    let hook = SessionRecovery;
    let mut ctx = HookContext::new();
    let mut msg = Message {
        role: "user".into(),
        content: "".into(),
    };
    hook.on_message(&mut ctx, &mut msg).await.unwrap();
    assert_eq!(ctx.injected_messages.len(), 1);
}

#[tokio::test]
async fn session_recovery_handles_session_error() {
    let hook = SessionRecovery;
    let mut ctx = HookContext::new();
    let event = SessionEvent {
        r#type: SessionEventType::Error("crash".into()),
        session_id: "s1".into(),
        timestamp: "now".into(),
    };
    hook.on_event(&mut ctx, &event).await.unwrap();
    assert_eq!(ctx.injected_messages.len(), 1);
}

// ContextWindowMonitor tests

#[tokio::test]
async fn context_monitor_warns_at_threshold() {
    let hook = ContextWindowMonitor;
    let mut ctx = HookContext::new();
    ctx.set_metadata("context_usage_pct".into(), "80.0".into());
    let mut params = ApiParams {
        model: "sonnet".into(),
        temperature: None,
        max_tokens: None,
        reasoning_effort: None,
    };
    hook.on_params(&mut ctx, &mut params).await.unwrap();
    assert_eq!(ctx.injected_messages.len(), 1);
}

#[tokio::test]
async fn context_monitor_silent_below_threshold() {
    let hook = ContextWindowMonitor;
    let mut ctx = HookContext::new();
    ctx.set_metadata("context_usage_pct".into(), "60.0".into());
    let mut params = ApiParams {
        model: "sonnet".into(),
        temperature: None,
        max_tokens: None,
        reasoning_effort: None,
    };
    hook.on_params(&mut ctx, &mut params).await.unwrap();
    assert_eq!(ctx.injected_messages.len(), 0);
}

// RulesInjector tests

#[tokio::test]
async fn rules_injector_handles_missing_dir() {
    let hook = RulesInjector::with_base_path("/nonexistent/rules".into());
    let mut ctx = HookContext::new();
    let mut params = ApiParams {
        model: "sonnet".into(),
        temperature: None,
        max_tokens: None,
        reasoning_effort: None,
    };
    hook.on_params(&mut ctx, &mut params).await.unwrap();
    assert_eq!(ctx.injected_messages.len(), 0);
}

// ToolOutputTruncator tests

#[tokio::test]
async fn truncator_truncates_large_output() {
    let hook = ToolOutputTruncator::with_threshold(50);
    let mut ctx = HookContext::new();
    let mut output = ToolOutput {
        tool_name: "grep_search".into(),
        result: "x".repeat(200),
    };
    hook.on_post_tool_use(&mut ctx, &mut output).await.unwrap();
    assert!(output.result.contains("(truncated)"));
    assert!(ctx.warnings.len() > 0);
}

// RalphLoop tests

#[tokio::test]
async fn ralph_loop_continues_when_work_remains() {
    let hook = RalphLoop;
    let mut ctx = HookContext::new();
    ctx.set_metadata("work_remaining".into(), "more".into());
    let mut msg = Message {
        role: "assistant".into(),
        content: "I am done".into(),
    };
    hook.on_message(&mut ctx, &mut msg).await.unwrap();
    assert_eq!(ctx.injected_messages.len(), 1);
}

// ThinkMode tests

#[tokio::test]
async fn think_mode_activates_on_keyword() {
    let hook = ThinkMode;
    let mut ctx = HookContext::new();
    let mut msg = Message {
        role: "user".into(),
        content: "think deeply about this".into(),
    };
    hook.on_message(&mut ctx, &mut msg).await.unwrap();
    assert_eq!(ctx.injected_messages.len(), 1);
    assert_eq!(ctx.get_metadata("thinking_mode"), Some("extended"));
}

// StartWork tests

#[tokio::test]
async fn start_work_detects_command() {
    let hook = StartWork;
    let mut ctx = HookContext::new();
    let mut msg = Message {
        role: "user".into(),
        content: "/start-work".into(),
    };
    hook.on_message(&mut ctx, &mut msg).await.unwrap();
    assert_eq!(ctx.injected_messages.len(), 1);
}

#[tokio::test]
async fn start_work_tracks_progress() {
    let hook = StartWork;
    let mut ctx = HookContext::new();
    ctx.set_metadata("boulder_state".into(), "phase-2".into());
    ctx.set_metadata("work_progress".into(), "50%".into());
    let mut msg = Message {
        role: "user".into(),
        content: "start work".into(),
    };
    hook.on_message(&mut ctx, &mut msg).await.unwrap();
    assert_eq!(ctx.get_metadata("last_checkpoint"), Some("phase-2"));
}

// Hook name uniqueness test

#[test]
fn all_hook_names_are_unique() {
    let rules = RulesInjector::new();
    let truncator = ToolOutputTruncator::new();
    let hooks: Vec<&dyn Hook> = vec![
        &KeywordDetector,
        &Ultrawork,
        &TodoContinuationEnforcer,
        &CommentChecker,
        &SessionRecovery,
        &ContextWindowMonitor,
        &rules,
        &truncator,
        &RalphLoop,
        &ThinkMode,
        &StartWork,
    ];
    let mut names: Vec<&str> = hooks.iter().map(|h| h.name()).collect();
    names.sort();
    let mut unique: Vec<&str> = names.clone();
    unique.dedup();
    assert_eq!(names.len(), unique.len(), "Duplicate hook names detected");
}
