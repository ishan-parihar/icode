use std::collections::HashSet;
use std::sync::Arc;

use crate::context::HookContext;
use crate::event_types::{HookEvent, SessionEvent};
use crate::hook_trait::{ApiParams, Hook, HookResult, Message, ToolInput, ToolOutput};

pub struct HookDispatcher {
    hooks: Vec<Arc<dyn Hook>>,
    disabled_hooks: HashSet<String>,
}

impl HookDispatcher {
    #[must_use]
    pub fn new() -> Self {
        Self {
            hooks: Vec::new(),
            disabled_hooks: HashSet::new(),
        }
    }

    pub fn register(&mut self, hook: Arc<dyn Hook>) {
        self.hooks.push(hook);
    }

    pub fn disable(&mut self, name: &str) {
        self.disabled_hooks.insert(name.to_string());
    }

    pub fn enable(&mut self, name: &str) {
        self.disabled_hooks.remove(name);
    }

    pub async fn dispatch_pre_tool_use(
        &self,
        ctx: &mut HookContext,
        input: &mut ToolInput,
    ) -> HookResult {
        let hooks = self.active_hooks(&HookEvent::PreToolUse);
        for hook in hooks {
            if hook.on_pre_tool_use(ctx, input).await.is_err() {
                return Err(format!("Hook '{}' errored during PreToolUse", hook.name()));
            }
            if ctx.blocked {
                return Ok(());
            }
        }
        Ok(())
    }

    pub async fn dispatch_post_tool_use(
        &self,
        ctx: &mut HookContext,
        output: &mut ToolOutput,
    ) -> HookResult {
        let hooks = self.active_hooks(&HookEvent::PostToolUse);
        for hook in hooks {
            if hook.on_post_tool_use(ctx, output).await.is_err() {
                return Err(format!("Hook '{}' errored during PostToolUse", hook.name()));
            }
            if ctx.blocked {
                return Ok(());
            }
        }
        Ok(())
    }

    pub async fn dispatch_message(
        &self,
        ctx: &mut HookContext,
        message: &mut Message,
    ) -> HookResult {
        let hooks = self.active_hooks(&HookEvent::Message);
        for hook in hooks {
            if hook.on_message(ctx, message).await.is_err() {
                return Err(format!("Hook '{}' errored during Message", hook.name()));
            }
            if ctx.blocked {
                return Ok(());
            }
        }
        Ok(())
    }

    pub async fn dispatch_event(&self, ctx: &mut HookContext, event: &SessionEvent) -> HookResult {
        let hooks = self.active_hooks(&HookEvent::SessionEvent);
        for hook in hooks {
            if hook.on_event(ctx, event).await.is_err() {
                return Err(format!(
                    "Hook '{}' errored during SessionEvent",
                    hook.name()
                ));
            }
            if ctx.blocked {
                return Ok(());
            }
        }
        Ok(())
    }

    pub async fn dispatch_transform(&self, ctx: &mut HookContext) -> HookResult {
        let hooks = self.active_hooks(&HookEvent::Transform);
        for hook in hooks {
            if hook.on_transform(ctx).await.is_err() {
                return Err(format!("Hook '{}' errored during Transform", hook.name()));
            }
            if ctx.blocked {
                return Ok(());
            }
        }
        Ok(())
    }

    pub async fn dispatch_params(
        &self,
        ctx: &mut HookContext,
        params: &mut ApiParams,
    ) -> HookResult {
        let hooks = self.active_hooks(&HookEvent::Params);
        for hook in hooks {
            if hook.on_params(ctx, params).await.is_err() {
                return Err(format!("Hook '{}' errored during Params", hook.name()));
            }
            if ctx.blocked {
                return Ok(());
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn active_hooks(&self, event: &HookEvent) -> Vec<Arc<dyn Hook>> {
        let mut matching: Vec<_> = self
            .hooks
            .iter()
            .filter(|h| !self.disabled_hooks.contains(h.name()) && h.events().contains(event))
            .cloned()
            .collect();
        matching.sort_by_key(|h| h.priority());
        matching
    }
}

impl Default for HookDispatcher {
    fn default() -> Self {
        Self::new()
    }
}
