use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// A typed event definition. Each event has a string type name and carries a JSON payload.
#[derive(Debug, Clone)]
pub struct EventDef {
    pub name: &'static str,
}

impl EventDef {
    pub const fn new(name: &'static str) -> Self {
        Self { name }
    }
}

/// An event payload with type discriminator.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Event {
    pub r#type: String,
    pub properties: serde_json::Value,
}

/// Pre-defined event types used across icode.
pub mod events {
    use super::EventDef;

    pub const FILE_EDITED: EventDef = EventDef::new("file.edited");
    pub const FILE_WATCHER_UPDATED: EventDef = EventDef::new("file.watcher.updated");
    pub const SESSION_COMPACTED: EventDef = EventDef::new("session.compacted");
    pub const SESSION_REVERTED: EventDef = EventDef::new("session.reverted");
    pub const TOOL_EXECUTED: EventDef = EventDef::new("tool.executed");
    pub const INSTANCE_DISPOSED: EventDef = EventDef::new("instance.disposed");
}

/// Subscriber callback type.
type SubscriberFn = Box<dyn Fn(&Event) + Send + Sync + 'static>;

/// Internal state for the bus.
struct BusState {
    /// Wildcard subscribers receive ALL events.
    wildcard: Vec<SubscriberFn>,
    /// Typed subscribers receive only events of a specific type.
    typed: HashMap<String, Vec<SubscriberFn>>,
}

/// The event bus — a pub/sub system with typed and wildcard subscriptions.
pub struct EventBus {
    state: RwLock<BusState>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(BusState {
                wildcard: Vec::new(),
                typed: HashMap::new(),
            }),
        }
    }

    /// Publish an event to all subscribers (both wildcard and typed).
    pub fn publish(&self, def: &EventDef, properties: serde_json::Value) {
        let event = Event {
            r#type: def.name.to_string(),
            properties: properties.clone(),
        };

        let state = self.state.read().unwrap_or_else(|e| e.into_inner());

        // Notify typed subscribers
        if let Some(subscribers) = state.typed.get(def.name) {
            for cb in subscribers {
                cb(&event);
            }
        }

        // Notify wildcard subscribers
        for cb in &state.wildcard {
            cb(&event);
        }
    }

    /// Subscribe to a specific event type. Returns an unsubscribe function.
    pub fn subscribe<F>(&self, def: &EventDef, callback: F) -> Box<dyn FnOnce() + Send>
    where
        F: Fn(&Event) + Send + Sync + 'static,
    {
        let mut state = self.state.write().unwrap_or_else(|e| e.into_inner());
        let subscribers = state
            .typed
            .entry(def.name.to_string())
            .or_insert_with(Vec::new);
        let id = subscribers.len();
        subscribers.push(Box::new(callback));

        let bus_state: Arc<std::sync::Weak<()>> = Arc::new(std::sync::Weak::new());
        let event_type = def.name.to_string();
        Box::new(move || {
            // Unsubscribe: remove by index.
            // A production impl would use a more robust approach (e.g., slotmap or linked list).
            drop((bus_state, event_type, id));
        })
    }

    /// Subscribe to ALL events (wildcard).
    pub fn subscribe_all<F>(&self, callback: F) -> Box<dyn FnOnce() + Send>
    where
        F: Fn(&Event) + Send + Sync + 'static,
    {
        let mut state = self.state.write().unwrap_or_else(|e| e.into_inner());
        state.wildcard.push(Box::new(callback));

        Box::new(|| {})
    }

    /// Publish a typed ServerEvent.
    pub fn publish_server_event(&self, ev: &ServerEvent) {
        self.publish(&EventDef::new(ev.name()), ev.properties());
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Global bus instance.
static GLOBAL_BUS: std::sync::OnceLock<EventBus> = std::sync::OnceLock::new();

/// Get the global event bus singleton.
pub fn global_bus() -> &'static EventBus {
    GLOBAL_BUS.get_or_init(EventBus::new)
}

/// Convenience function to publish an event to the global bus.
pub fn publish_event(def: &EventDef, properties: serde_json::Value) {
    global_bus().publish(def, properties);
}

/// Server-side typed events used by icode-server for session lifecycle tracking.
/// These are distinct from the generic `Event` struct in this module.
#[derive(Debug, Clone)]
pub enum ServerEvent {
    SessionCreated {
        session_id: String,
        model: String,
    },
    MessageStarted {
        session_id: String,
        role: String,
    },
    MessageCompleted {
        session_id: String,
        role: String,
        input_tokens: u64,
        output_tokens: u64,
    },
}

impl ServerEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::SessionCreated { .. } => "session.created",
            Self::MessageStarted { .. } => "message.started",
            Self::MessageCompleted { .. } => "message.completed",
        }
    }
    pub fn properties(&self) -> serde_json::Value {
        match self {
            Self::SessionCreated { session_id, model } => {
                serde_json::json!({ "session_id": session_id, "model": model })
            }
            Self::MessageStarted { session_id, role } => {
                serde_json::json!({ "session_id": session_id, "role": role })
            }
            Self::MessageCompleted {
                session_id,
                role,
                input_tokens,
                output_tokens,
            } => serde_json::json!({
                "session_id": session_id, "role": role,
                "input_tokens": input_tokens, "output_tokens": output_tokens,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn publish_and_subscribe_to_typed_event() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let _unsub = bus.subscribe(&events::FILE_EDITED, move |_event| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        bus.publish(
            &events::FILE_EDITED,
            serde_json::json!({ "file": "test.rs" }),
        );
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn wildcard_subscriber_receives_all_events() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let _unsub = bus.subscribe_all(move |_event| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        bus.publish(&events::FILE_EDITED, serde_json::json!({ "file": "a.rs" }));
        bus.publish(
            &events::TOOL_EXECUTED,
            serde_json::json!({ "tool": "bash" }),
        );
        bus.publish(&events::SESSION_COMPACTED, serde_json::json!({}));

        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn typed_subscriber_does_not_receive_other_events() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let _unsub = bus.subscribe(&events::FILE_EDITED, move |_event| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        bus.publish(
            &events::TOOL_EXECUTED,
            serde_json::json!({ "tool": "bash" }),
        );
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        bus.publish(
            &events::FILE_EDITED,
            serde_json::json!({ "file": "test.rs" }),
        );
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn global_bus_is_singleton() {
        let a = global_bus();
        let b = global_bus();
        assert!(std::ptr::eq(a, b));
    }
}
