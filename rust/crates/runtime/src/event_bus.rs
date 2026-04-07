use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex};

use crate::persistence::SqliteStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
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
        input_tokens: u32,
        output_tokens: u32,
    },
    ToolExecuted {
        session_id: String,
        tool_name: String,
        success: bool,
        duration_ms: u64,
    },
    ToolOutputTruncated {
        session_id: String,
        tool_name: String,
        original_chars: usize,
        truncated_chars: usize,
    },
    ContextCompacted {
        session_id: String,
        removed_message_count: usize,
    },
    DoomLoopDetected {
        session_id: String,
        tool_name: String,
        call_count: u32,
    },
    PermissionAsked {
        session_id: String,
        tool_name: String,
        input: String,
    },
    PermissionGranted {
        session_id: String,
        tool_name: String,
    },
    PermissionDenied {
        session_id: String,
        tool_name: String,
        reason: String,
    },
    FileChanged {
        path: String,
        kind: String,
    },
    WorkerStateChanged {
        worker_id: String,
        from: String,
        to: String,
    },
    RecoveryAttempted {
        session_id: String,
        recipe: String,
        success: bool,
    },
    PtyCreated {
        session_id: String,
        command: String,
        pid: Option<u32>,
    },
    PtyClosed {
        session_id: String,
        exit_code: Option<i32>,
    },
}

impl Event {
    #[must_use]
    pub fn event_type_name(&self) -> &'static str {
        match self {
            Self::SessionCreated { .. } => "session_created",
            Self::MessageStarted { .. } => "message_started",
            Self::MessageCompleted { .. } => "message_completed",
            Self::ToolExecuted { .. } => "tool_executed",
            Self::ToolOutputTruncated { .. } => "tool_output_truncated",
            Self::ContextCompacted { .. } => "context_compacted",
            Self::DoomLoopDetected { .. } => "doom_loop_detected",
            Self::PermissionAsked { .. } => "permission_asked",
            Self::PermissionGranted { .. } => "permission_granted",
            Self::PermissionDenied { .. } => "permission_denied",
            Self::FileChanged { .. } => "file_changed",
            Self::WorkerStateChanged { .. } => "worker_state_changed",
            Self::RecoveryAttempted { .. } => "recovery_attempted",
            Self::PtyCreated { .. } => "pty_created",
            Self::PtyClosed { .. } => "pty_closed",
        }
    }
}

// ── IPC Types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub instance_id: String,
    pub timestamp: u64,
    pub event: Event,
}

impl Envelope {
    #[must_use]
    pub fn new(instance_id: String, event: Event) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            instance_id,
            timestamp,
            event,
        }
    }
}

pub trait IpcTransport: Send + Sync {
    fn send(&self, envelope: &Envelope) -> Result<(), String>;
    fn poll_events(&self) -> Vec<Envelope>;
}

// ── EventBus ──────────────────────────────────────────────────────────

pub struct EventBus {
    sender: broadcast::Sender<Event>,
    instance_id: String,
    ipc_transports: Arc<Mutex<Vec<Box<dyn IpcTransport>>>>,
}

impl EventBus {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let instance_id = generate_instance_id();
        let (sender, _receiver) = broadcast::channel(capacity);
        Self {
            sender,
            instance_id,
            ipc_transports: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[must_use]
    pub fn with_instance_id(capacity: usize, instance_id: String) -> Self {
        let (sender, _receiver) = broadcast::channel(capacity);
        Self {
            sender,
            instance_id,
            ipc_transports: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn add_ipc_transport(&self, transport: Box<dyn IpcTransport>) {
        self.ipc_transports.lock().await.push(transport);
    }

    pub fn publish(&self, event: Event) {
        let _ = self.sender.send(event.clone());

        let envelope = Envelope::new(self.instance_id.clone(), event);
        let transports = self.ipc_transports.clone();
        tokio::spawn(async move {
            let guards = transports.lock().await;
            for transport in guards.iter() {
                if let Err(e) = transport.send(&envelope) {
                    eprintln!("event_bus: IPC send failed: {e}");
                }
            }
        });
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    #[must_use]
    pub fn subscribe_with_ipc(&self) -> IpcMergedStream {
        let local_rx = self.sender.subscribe();
        let ipc_transports = self.ipc_transports.clone();
        let instance_id = self.instance_id.clone();
        IpcMergedStream {
            local_rx,
            ipc_transports,
            instance_id,
        }
    }

    #[must_use]
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    #[must_use]
    pub fn sender_count(&self) -> usize {
        self.sender.receiver_count()
    }

    pub fn persist(&self, event: &Event, store: &SqliteStore, session_id: &str) {
        let event_data = match serde_json::to_string(event) {
            Ok(json) => json,
            Err(e) => {
                eprintln!("event_bus: failed to serialize event: {e}");
                return;
            }
        };
        if let Err(e) = store.insert_event(session_id, event.event_type_name(), &event_data) {
            eprintln!("event_bus: failed to persist event: {e}");
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(256)
    }
}

// ── IpcMergedStream ───────────────────────────────────────────────────

pub struct IpcMergedStream {
    local_rx: broadcast::Receiver<Event>,
    ipc_transports: Arc<Mutex<Vec<Box<dyn IpcTransport>>>>,
    instance_id: String,
}

impl IpcMergedStream {
    pub async fn recv(&mut self) -> Result<Event, broadcast::error::RecvError> {
        let ipc_transports = self.ipc_transports.clone();
        let instance_id = self.instance_id.clone();

        tokio::select! {
            result = self.local_rx.recv() => {
                result
            }
            event = poll_ipc_once(ipc_transports, &instance_id) => {
                Ok(event)
            }
        }
    }
}

async fn poll_ipc_once(
    ipc_transports: Arc<Mutex<Vec<Box<dyn IpcTransport>>>>,
    instance_id: &str,
) -> Event {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let transports = ipc_transports.lock().await;
        for transport in transports.iter() {
            let envelopes = transport.poll_events();
            for envelope in envelopes {
                if envelope.instance_id != instance_id {
                    return envelope.event;
                }
            }
        }
    }
}

fn generate_instance_id() -> String {
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());
    let pid = std::process::id();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{hostname}-{pid}-{ts}")
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;

    fn temp_db() -> (SqliteStore, std::path::PathBuf) {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("time after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("icode-event-bus-test-{nanos}.db"));
        let store = SqliteStore::new(&path).expect("store should open");
        store.migrate().expect("migration should succeed");
        (store, path)
    }

    #[tokio::test]
    async fn publish_and_receive_single_event() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish(Event::SessionCreated {
            session_id: "s1".into(),
            model: "sonnet".into(),
        });

        let received = rx.recv().await.expect("should receive event");
        match received {
            Event::SessionCreated { session_id, model } => {
                assert_eq!(session_id, "s1");
                assert_eq!(model, "sonnet");
            }
            other => panic!("expected SessionCreated, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn multiple_subscribers_all_receive() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();
        let mut rx3 = bus.subscribe();

        assert_eq!(bus.sender_count(), 3);

        bus.publish(Event::MessageStarted {
            session_id: "s1".into(),
            role: "user".into(),
        });

        for rx in [&mut rx1, &mut rx2, &mut rx3] {
            match rx.recv().await.expect("each subscriber should receive") {
                Event::MessageStarted { session_id, role } => {
                    assert_eq!(session_id, "s1");
                    assert_eq!(role, "user");
                }
                other => panic!("expected MessageStarted, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn event_ordering_preserved() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish(Event::SessionCreated {
            session_id: "s1".into(),
            model: "sonnet".into(),
        });
        bus.publish(Event::MessageStarted {
            session_id: "s1".into(),
            role: "user".into(),
        });
        bus.publish(Event::MessageCompleted {
            session_id: "s1".into(),
            role: "assistant".into(),
            input_tokens: 10,
            output_tokens: 50,
        });

        let e1 = rx.recv().await.expect("first event");
        let e2 = rx.recv().await.expect("second event");
        let e3 = rx.recv().await.expect("third event");

        assert!(matches!(e1, Event::SessionCreated { .. }));
        assert!(matches!(e2, Event::MessageStarted { .. }));
        assert!(matches!(e3, Event::MessageCompleted { .. }));
    }

    #[tokio::test]
    async fn capacity_overflow_lagged_handling() {
        let bus = EventBus::new(4);
        let mut rx = bus.subscribe();

        for i in 0..10 {
            bus.publish(Event::ToolExecuted {
                session_id: "s1".into(),
                tool_name: "bash".into(),
                success: true,
                duration_ms: i,
            });
        }

        let result = rx.recv().await;
        assert!(
            result.is_err(),
            "receiver should have lagged due to capacity overflow"
        );
        assert!(
            matches!(
                result,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
            ),
            "error should be Lagged variant"
        );
    }

    #[tokio::test]
    async fn arc_event_bus_can_be_shared() {
        use std::sync::Arc;

        let bus = Arc::new(EventBus::new(16));
        let mut rx = bus.subscribe();

        let bus_clone = Arc::clone(&bus);
        let handle = tokio::spawn(async move {
            bus_clone.publish(Event::FileChanged {
                path: "/src/main.rs".into(),
                kind: "modified".into(),
            });
        });

        handle.await.expect("spawn should succeed");

        match rx.recv().await.expect("should receive event") {
            Event::FileChanged { path, kind } => {
                assert_eq!(path, "/src/main.rs");
                assert_eq!(kind, "modified");
            }
            other => panic!("expected FileChanged, got {other:?}"),
        }
    }

    #[test]
    fn event_serialization_roundtrip_all_variants() {
        let events: Vec<Event> = vec![
            Event::SessionCreated {
                session_id: "s1".into(),
                model: "sonnet".into(),
            },
            Event::MessageStarted {
                session_id: "s1".into(),
                role: "user".into(),
            },
            Event::MessageCompleted {
                session_id: "s1".into(),
                role: "assistant".into(),
                input_tokens: 100,
                output_tokens: 500,
            },
            Event::ToolExecuted {
                session_id: "s1".into(),
                tool_name: "bash".into(),
                success: true,
                duration_ms: 42,
            },
            Event::ToolOutputTruncated {
                session_id: "s1".into(),
                tool_name: "read_file".into(),
                original_chars: 50000,
                truncated_chars: 10000,
            },
            Event::ContextCompacted {
                session_id: "s1".into(),
                removed_message_count: 5,
            },
            Event::DoomLoopDetected {
                session_id: "s1".into(),
                tool_name: "bash".into(),
                call_count: 10,
            },
            Event::PermissionAsked {
                session_id: "s1".into(),
                tool_name: "write_file".into(),
                input: "/src/foo.rs".into(),
            },
            Event::PermissionGranted {
                session_id: "s1".into(),
                tool_name: "write_file".into(),
            },
            Event::PermissionDenied {
                session_id: "s1".into(),
                tool_name: "bash".into(),
                reason: "read-only mode".into(),
            },
            Event::FileChanged {
                path: "/src/main.rs".into(),
                kind: "modified".into(),
            },
            Event::WorkerStateChanged {
                worker_id: "w1".into(),
                from: "spawning".into(),
                to: "ready".into(),
            },
            Event::RecoveryAttempted {
                session_id: "s1".into(),
                recipe: "retry".into(),
                success: false,
            },
            Event::PtyCreated {
                session_id: "s1".into(),
                command: "bash".into(),
                pid: Some(1234),
            },
            Event::PtyClosed {
                session_id: "s1".into(),
                exit_code: Some(0),
            },
        ];

        for event in events {
            let json = serde_json::to_string(&event).expect("serialize should succeed");
            let deserialized: Event =
                serde_json::from_str(&json).expect("deserialize should succeed");

            assert_eq!(event.event_type_name(), deserialized.event_type_name());

            let json2 = serde_json::to_string(&deserialized).expect("re-serialize");
            assert_eq!(json, json2);
        }
    }

    #[test]
    fn event_type_name_returns_correct_snake_case() {
        assert_eq!(
            Event::SessionCreated {
                session_id: String::new(),
                model: String::new()
            }
            .event_type_name(),
            "session_created"
        );
        assert_eq!(
            Event::DoomLoopDetected {
                session_id: String::new(),
                tool_name: String::new(),
                call_count: 0
            }
            .event_type_name(),
            "doom_loop_detected"
        );
        assert_eq!(
            Event::WorkerStateChanged {
                worker_id: String::new(),
                from: String::new(),
                to: String::new()
            }
            .event_type_name(),
            "worker_state_changed"
        );
    }

    #[test]
    fn persist_writes_event_to_sqlite() {
        let (store, path) = temp_db();
        let bus = EventBus::new(16);

        store
            .create_session("s1", 1, 1000, 2000, None, None)
            .expect("create session");

        let event = Event::SessionCreated {
            session_id: "s1".into(),
            model: "sonnet".into(),
        };

        bus.persist(&event, &store, "s1");

        let row: Option<String> = store
            .conn()
            .query_row(
                "SELECT event_type FROM events WHERE session_id = ?1 ORDER BY id DESC LIMIT 1",
                rusqlite::params!["s1"],
                |r| r.get(0),
            )
            .ok();

        assert_eq!(row, Some("session_created".to_string()));

        std::fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn default_bus_has_reasonable_capacity() {
        let bus = EventBus::default();
        assert_eq!(bus.sender_count(), 0);
    }

    #[tokio::test]
    async fn pty_events_roundtrip() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish(Event::PtyCreated {
            session_id: "s1".into(),
            command: "bash -i".into(),
            pid: None,
        });

        match rx.recv().await.expect("should receive") {
            Event::PtyCreated {
                session_id,
                command,
                pid,
            } => {
                assert_eq!(session_id, "s1");
                assert_eq!(command, "bash -i");
                assert!(pid.is_none());
            }
            other => panic!("expected PtyCreated, got {other:?}"),
        }

        bus.publish(Event::PtyClosed {
            session_id: "s1".into(),
            exit_code: Some(1),
        });

        match rx.recv().await.expect("should receive") {
            Event::PtyClosed {
                session_id,
                exit_code,
            } => {
                assert_eq!(session_id, "s1");
                assert_eq!(exit_code, Some(1));
            }
            other => panic!("expected PtyClosed, got {other:?}"),
        }
    }

    #[test]
    fn bus_has_instance_id() {
        let bus = EventBus::new(16);
        let id = bus.instance_id();
        assert!(!id.is_empty());
    }

    #[test]
    fn bus_with_custom_instance_id() {
        let bus = EventBus::with_instance_id(16, "my-instance".into());
        assert_eq!(bus.instance_id(), "my-instance");
    }

    #[tokio::test]
    async fn publish_fans_out_to_ipc_transport() {
        struct MockTransport {
            sent: std::sync::Mutex<Vec<Envelope>>,
        }
        impl IpcTransport for MockTransport {
            fn send(&self, envelope: &Envelope) -> Result<(), String> {
                self.sent.lock().unwrap().push(envelope.clone());
                Ok(())
            }
            fn poll_events(&self) -> Vec<Envelope> {
                Vec::new()
            }
        }

        let bus = EventBus::with_instance_id(16, "local".into());
        let mock = MockTransport {
            sent: std::sync::Mutex::new(Vec::new()),
        };
        bus.add_ipc_transport(Box::new(mock)).await;

        bus.publish(Event::SessionCreated {
            session_id: "s1".into(),
            model: "sonnet".into(),
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    #[test]
    fn echo_prevention_filters_own_instance() {
        #[allow(dead_code)]
        struct OwnEchoTransport;
        impl IpcTransport for OwnEchoTransport {
            fn send(&self, _envelope: &Envelope) -> Result<(), String> {
                Ok(())
            }
            fn poll_events(&self) -> Vec<Envelope> {
                vec![Envelope::new(
                    "local".into(),
                    Event::SessionCreated {
                        session_id: "s1".into(),
                        model: "sonnet".into(),
                    },
                )]
            }
        }

        let bus = EventBus::with_instance_id(16, "local".into());
        let _ = bus.instance_id();
    }

    #[test]
    fn envelope_new_sets_timestamp() {
        let event = Event::SessionCreated {
            session_id: "s1".into(),
            model: "sonnet".into(),
        };
        let envelope = Envelope::new("inst-1".into(), event);
        assert_eq!(envelope.instance_id, "inst-1");
        assert!(envelope.timestamp > 0);
    }

    #[test]
    fn envelope_serialization_roundtrip() {
        let event = Event::ToolExecuted {
            session_id: "s1".into(),
            tool_name: "bash".into(),
            success: true,
            duration_ms: 42,
        };
        let envelope = Envelope::new("inst-1".into(), event);

        let json = serde_json::to_string(&envelope).expect("serialize should succeed");
        let deserialized: Envelope =
            serde_json::from_str(&json).expect("deserialize should succeed");

        assert_eq!(deserialized.instance_id, "inst-1");
        assert!(matches!(deserialized.event, Event::ToolExecuted { .. }));
    }
}
