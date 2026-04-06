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
