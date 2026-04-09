use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use serde::{Deserialize, Serialize};

use crate::event_bus::{Envelope, IpcTransport};
use crate::persistence::SqliteStore;

/// Maximum concurrent client threads to prevent OOM from connection floods.
const MAX_CLIENTS: usize = 100;

/// Maximum events to buffer from the IPC socket receiver.
/// Older events are dropped when the buffer is full to prevent unbounded growth.
const MAX_RECEIVE_BUFFER: usize = 10_000;

fn default_socket_path() -> PathBuf {
    std::env::var("ICODE_EVENT_SOCKET").ok().map_or_else(
        || {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            let dir = PathBuf::from(&home).join(".icode");
            let _ = std::fs::create_dir_all(&dir);
            dir.join("events.sock")
        },
        PathBuf::from,
    )
}

// ── Protocol Messages ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ProtocolMessage {
    Handshake {
        instance_id: String,
        last_seen_id: Option<i64>,
    },
    HandshakeAck {
        events: Vec<serde_json::Value>,
    },
    Event {
        envelope: Envelope,
    },
    Poll,
    PollResponse {
        envelopes: Vec<Envelope>,
    },
}

// ── Client Guard ─────────────────────────────────────────────────────

struct ClientGuard<'a> {
    clients: &'a AtomicUsize,
    handles: &'a std::sync::Mutex<Vec<std::thread::JoinHandle<()>>>,
}

impl<'a> ClientGuard<'a> {
    fn new(
        clients: &'a AtomicUsize,
        handles: &'a std::sync::Mutex<Vec<std::thread::JoinHandle<()>>>,
    ) -> Self {
        clients.fetch_add(1, Ordering::SeqCst);
        Self { clients, handles }
    }
}

impl Drop for ClientGuard<'_> {
    fn drop(&mut self) {
        self.clients.fetch_sub(1, Ordering::SeqCst);
        let mut handles = self.handles.lock().expect("client_handles lock poisoned");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut finished = Vec::new();
        let mut still_running = Vec::new();
        for h in handles.drain(..) {
            if h.is_finished() {
                finished.push(h);
            } else {
                still_running.push(h);
            }
        }
        for h in finished {
            let _ = h.join();
        }
        *handles = still_running
            .into_iter()
            .filter(|_h| std::time::Instant::now() < deadline)
            .collect();
    }
}

// ── UnixSocketServer ──────────────────────────────────────────────────

pub struct UnixSocketServer {
    instance_id: String,
    store: Arc<std::sync::Mutex<SqliteStore>>,
    socket_path: PathBuf,
    running: Arc<AtomicBool>,
    accept_handle: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
    active_clients: Arc<AtomicUsize>,
    client_handles: Arc<std::sync::Mutex<Vec<std::thread::JoinHandle<()>>>>,
}

impl UnixSocketServer {
    pub fn new(instance_id: String, store: Arc<std::sync::Mutex<SqliteStore>>) -> Self {
        Self {
            instance_id,
            store,
            socket_path: default_socket_path(),
            running: Arc::new(AtomicBool::new(true)),
            accept_handle: std::sync::Mutex::new(None),
            active_clients: Arc::new(AtomicUsize::new(0)),
            client_handles: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn with_path(
        instance_id: String,
        store: Arc<std::sync::Mutex<SqliteStore>>,
        socket_path: PathBuf,
    ) -> Self {
        Self {
            instance_id,
            store,
            socket_path,
            running: Arc::new(AtomicBool::new(true)),
            accept_handle: std::sync::Mutex::new(None),
            active_clients: Arc::new(AtomicUsize::new(0)),
            client_handles: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn accept_loop(&self) -> std::io::Result<()> {
        if let Err(e) = std::fs::remove_file(&self.socket_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("ipc_socket: stale socket cleanup failed: {e}");
            }
        }

        let listener = UnixListener::bind(&self.socket_path)?;

        let running = self.running.clone();
        let instance_id = self.instance_id.clone();
        let store = self.store.clone();
        let active_clients = self.active_clients.clone();
        let client_handles = self.client_handles.clone();

        let handle = thread::spawn(move || {
            for stream in listener.incoming() {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                match stream {
                    Ok(stream) => {
                        if active_clients.load(Ordering::SeqCst) >= MAX_CLIENTS {
                            eprintln!(
                                "ipc_socket: max clients ({MAX_CLIENTS}) reached, rejecting connection"
                            );
                            continue;
                        }
                        let iid = instance_id.clone();
                        let s = store.clone();
                        let clients = active_clients.clone();
                        let handles_for_thread = client_handles.clone();
                        let client_handle = thread::spawn(move || {
                            let _guard = ClientGuard::new(&clients, &handles_for_thread);
                            if let Err(e) = handle_client(stream, &iid, &s) {
                                eprintln!("ipc_socket: client error: {e}");
                            }
                        });
                        client_handles
                            .lock()
                            .expect("client_handles lock poisoned")
                            .push(client_handle);
                    }
                    Err(e) => {
                        eprintln!("ipc_socket: accept error: {e}");
                    }
                }
            }
        });

        *self
            .accept_handle
            .lock()
            .expect("accept_handle lock poisoned") = Some(handle);

        Ok(())
    }

    pub fn shutdown(&self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = UnixStream::connect(&self.socket_path);
        let _ = std::fs::remove_file(&self.socket_path);

        if let Some(handle) = self
            .accept_handle
            .lock()
            .expect("accept_handle lock poisoned")
            .take()
        {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while !handle.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if !handle.is_finished() {
                eprintln!("ipc_socket: accept thread did not exit within 2s, detaching");
            }
            let _ = handle.join();
        }

        let mut client_handles = self
            .client_handles
            .lock()
            .expect("client_handles lock poisoned");
        let shutdown_deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        for client_handle in client_handles.drain(..) {
            let remaining = shutdown_deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                eprintln!("ipc_socket: shutdown timeout for client threads, detaching remaining");
                break;
            }
            let mut waited = std::time::Duration::ZERO;
            while !client_handle.is_finished() && waited < remaining {
                std::thread::sleep(std::time::Duration::from_millis(10));
                waited += std::time::Duration::from_millis(10);
            }
            if client_handle.is_finished() {
                let _ = client_handle.join();
            }
        }
    }

    pub fn broadcast_event(&self, envelope: &Envelope) {
        if let Ok(mut stream) = UnixStream::connect(&self.socket_path) {
            let msg = ProtocolMessage::Event {
                envelope: envelope.clone(),
            };
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = writeln!(stream, "{json}");
            }
        }
    }
}

impl Drop for UnixSocketServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn handle_client(
    mut stream: UnixStream,
    server_instance_id: &str,
    store: &std::sync::Mutex<SqliteStore>,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;

    let reader = BufReader::new(stream.try_clone()?);
    let mut lines = reader.lines();

    if let Some(Ok(first_line)) = lines.next() {
        if let Ok(msg) = serde_json::from_str::<ProtocolMessage>(&first_line) {
            match msg {
                ProtocolMessage::Handshake {
                    instance_id: _,
                    last_seen_id,
                } => {
                    let catch_up_events = if let Some(id) = last_seen_id {
                        let guard = store
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        guard.get_events_since_id(id, 1000).unwrap_or_default()
                    } else {
                        Vec::new()
                    };

                    let event_values: Vec<serde_json::Value> = catch_up_events
                        .into_iter()
                        .filter_map(|row| serde_json::from_str(&row.event_data).ok())
                        .collect();

                    let ack = ProtocolMessage::HandshakeAck {
                        events: event_values,
                    };
                    let ack_json = serde_json::to_string(&ack).map_err(io_error_other)?;
                    writeln!(stream, "{ack_json}")?;

                    for line in lines {
                        match line {
                            Ok(line) => {
                                if let Ok(ProtocolMessage::Event { envelope }) =
                                    serde_json::from_str(&line)
                                {
                                    if envelope.instance_id != server_instance_id {
                                        if let Ok(guard) =
                                            store.lock().map_err(std::sync::PoisonError::into_inner)
                                        {
                                            let _ = guard.insert_event(
                                                &get_session_id_from_envelope(&envelope),
                                                envelope.event.event_type_name(),
                                                &serde_json::to_string(&envelope.event)
                                                    .unwrap_or_default(),
                                            );
                                        }
                                    }
                                    let ack = ProtocolMessage::Event { envelope };
                                    let ack_json = serde_json::to_string(&ack).unwrap_or_default();
                                    if writeln!(stream, "{ack_json}").is_err() {
                                        break;
                                    }
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
                ProtocolMessage::Poll => {
                    let guard = store
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    let events = guard.get_events_since_id(0, 1000).unwrap_or_default();
                    let envelopes: Vec<Envelope> = events
                        .into_iter()
                        .filter_map(|row| {
                            let event: crate::event_bus::Event =
                                serde_json::from_str(&row.event_data).ok()?;
                            Some(Envelope {
                                instance_id: "remote".to_string(),
                                timestamp: 0,
                                event,
                            })
                        })
                        .collect();

                    let resp = ProtocolMessage::PollResponse { envelopes };
                    let resp_json = serde_json::to_string(&resp).map_err(io_error_other)?;
                    writeln!(stream, "{resp_json}")?;
                }
                ProtocolMessage::Event { envelope } => {
                    if envelope.instance_id != server_instance_id {
                        if let Ok(guard) = store.lock().map_err(std::sync::PoisonError::into_inner)
                        {
                            let _ = guard.insert_event(
                                &get_session_id_from_envelope(&envelope),
                                envelope.event.event_type_name(),
                                &serde_json::to_string(&envelope.event).unwrap_or_default(),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn get_session_id_from_envelope(envelope: &Envelope) -> String {
    use crate::event_bus::Event;
    match &envelope.event {
        Event::SessionCreated { session_id, .. }
        | Event::MessageStarted { session_id, .. }
        | Event::MessageCompleted { session_id, .. }
        | Event::ToolExecuted { session_id, .. }
        | Event::ToolOutputTruncated { session_id, .. }
        | Event::ContextCompacted { session_id, .. }
        | Event::DoomLoopDetected { session_id, .. }
        | Event::PermissionAsked { session_id, .. }
        | Event::PermissionGranted { session_id, .. }
        | Event::PermissionDenied { session_id, .. }
        | Event::RecoveryAttempted { session_id, .. }
        | Event::PtyCreated { session_id, .. }
        | Event::PtyClosed { session_id, .. } => session_id.clone(),
        Event::FileChanged { .. } | Event::WorkerStateChanged { .. } => "unknown".to_string(),
    }
}

fn io_error_other<E: std::fmt::Display>(e: E) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

// ── UnixSocketClient ──────────────────────────────────────────────────

pub struct UnixSocketClient {
    instance_id: String,
    socket_path: PathBuf,
    receive_buffer: Arc<std::sync::Mutex<Vec<Envelope>>>,
    receiver_running: Arc<AtomicBool>,
    receiver_handle: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl UnixSocketClient {
    #[must_use]
    pub fn new(socket_path: PathBuf, instance_id: String) -> Self {
        Self {
            instance_id,
            socket_path,
            receive_buffer: Arc::new(std::sync::Mutex::new(Vec::new())),
            receiver_running: Arc::new(AtomicBool::new(true)),
            receiver_handle: std::sync::Mutex::new(None),
        }
    }

    pub fn start_receiver(&self) {
        let socket_path = self.socket_path.clone();
        let buffer = self.receive_buffer.clone();
        let client_instance_id = self.instance_id.clone();
        let running = self.receiver_running.clone();

        let handle = thread::spawn(move || loop {
            if !running.load(Ordering::SeqCst) {
                break;
            }
            if let Ok(mut stream) = UnixStream::connect(&socket_path) {
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .ok();
                stream
                    .set_write_timeout(Some(std::time::Duration::from_secs(2)))
                    .ok();

                let handshake = ProtocolMessage::Handshake {
                    instance_id: client_instance_id.clone(),
                    last_seen_id: None,
                };
                if let Ok(json) = serde_json::to_string(&handshake) {
                    if writeln!(stream, "{json}").is_err() {
                        thread::sleep(std::time::Duration::from_secs(1));
                        continue;
                    }
                }

                let reader = BufReader::new(stream);
                for line in reader.lines() {
                    if !running.load(Ordering::SeqCst) {
                        break;
                    }
                    match line {
                        Ok(line) => {
                            if let Ok(ProtocolMessage::Event { envelope }) =
                                serde_json::from_str(&line)
                            {
                                if let Ok(mut buf) = buffer.lock() {
                                    if buf.len() >= MAX_RECEIVE_BUFFER {
                                        let drop_count = buf.len() / 4;
                                        buf.drain(..drop_count);
                                    }
                                    buf.push(envelope);
                                }
                            } else if let Ok(ProtocolMessage::HandshakeAck { .. }) =
                                serde_json::from_str::<ProtocolMessage>(&line)
                            {
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
            if !running.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(std::time::Duration::from_secs(1));
        });

        *self
            .receiver_handle
            .lock()
            .expect("receiver_handle lock poisoned") = Some(handle);
    }

    pub fn stop_receiver(&self) {
        self.receiver_running.store(false, Ordering::SeqCst);

        if let Some(handle) = self
            .receiver_handle
            .lock()
            .expect("receiver_handle lock poisoned")
            .take()
        {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while !handle.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if !handle.is_finished() {
                eprintln!("ipc_socket: receiver thread did not exit within 2s, detaching");
            }
            let _ = handle.join();
        }
    }

    pub fn send_envelope(&self, envelope: &Envelope) -> Result<(), String> {
        let mut stream =
            UnixStream::connect(&self.socket_path).map_err(|e| format!("connect failed: {e}"))?;
        let msg = ProtocolMessage::Event {
            envelope: envelope.clone(),
        };
        let json = serde_json::to_string(&msg).map_err(|e| e.to_string())?;
        writeln!(stream, "{json}").map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn poll_remote_events(&self) -> Result<Vec<Envelope>, String> {
        let stream =
            UnixStream::connect(&self.socket_path).map_err(|e| format!("connect failed: {e}"))?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .ok();
        stream
            .set_write_timeout(Some(std::time::Duration::from_secs(3)))
            .ok();

        let msg = ProtocolMessage::Poll;
        let json = serde_json::to_string(&msg).map_err(|e| e.to_string())?;

        let mut write_stream = stream.try_clone().map_err(|e| e.to_string())?;
        writeln!(write_stream, "{json}").map_err(|e| e.to_string())?;

        let reader = BufReader::new(stream);
        if let Some(Ok(line)) = reader.lines().next() {
            if let Ok(ProtocolMessage::PollResponse { envelopes }) = serde_json::from_str(&line) {
                return Ok(envelopes);
            }
        }
        Ok(Vec::new())
    }

    pub fn drain_receive_buffer(&self) -> Vec<Envelope> {
        let mut buf = self
            .receive_buffer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *buf)
    }
}

impl IpcTransport for UnixSocketClient {
    fn send(&self, envelope: &Envelope) -> Result<(), String> {
        self.send_envelope(envelope)
    }

    fn poll_events(&self) -> Vec<Envelope> {
        self.drain_receive_buffer()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn temp_socket_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("time after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("icode-ipc-test-{nanos}.sock"))
    }

    fn temp_db() -> (SqliteStore, PathBuf) {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("time after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("icode-ipc-db-test-{nanos}.db"));
        let store = SqliteStore::new(&path).expect("store should open");
        store.migrate().expect("migration should succeed");
        (store, path)
    }

    #[test]
    fn envelope_serialization_roundtrip() {
        use crate::event_bus::Event;

        let event = Event::SessionCreated {
            session_id: "s1".into(),
            model: "sonnet".into(),
        };
        let envelope = Envelope::new("instance-1".into(), event);

        let json = serde_json::to_string(&envelope).expect("serialize should succeed");
        let deserialized: Envelope =
            serde_json::from_str(&json).expect("deserialize should succeed");

        assert_eq!(deserialized.instance_id, "instance-1");
        assert!(matches!(deserialized.event, Event::SessionCreated { .. }));
    }

    #[test]
    fn unix_socket_server_starts_and_accepts() {
        let (store, db_path) = temp_db();
        let socket_path = temp_socket_path();

        let server = UnixSocketServer::with_path(
            "server-1".into(),
            Arc::new(std::sync::Mutex::new(store)),
            socket_path.clone(),
        );

        server.accept_loop().expect("accept_loop should start");
        thread::sleep(std::time::Duration::from_millis(200));

        let stream = UnixStream::connect(&socket_path);
        assert!(stream.is_ok(), "should be able to connect to server");

        server.shutdown();
        let _ = std::fs::remove_file(&socket_path);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn unix_socket_handshake_and_catchup() {
        let (store, db_path) = temp_db();
        let socket_path = temp_socket_path();

        store
            .create_session("s1", 1, 1000, 2000, None, None)
            .expect("create session");
        store
            .insert_event(
                "s1",
                "session_created",
                r#"{"type":"session_created","session_id":"s1","model":"sonnet"}"#,
            )
            .expect("insert event");

        let server = UnixSocketServer::with_path(
            "server-1".into(),
            Arc::new(std::sync::Mutex::new(store)),
            socket_path.clone(),
        );
        server.accept_loop().expect("accept_loop should start");
        thread::sleep(std::time::Duration::from_millis(200));

        let mut stream = UnixStream::connect(&socket_path).expect("connect should succeed");
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .ok();

        let handshake = ProtocolMessage::Handshake {
            instance_id: "client-1".into(),
            last_seen_id: Some(0),
        };
        let json = serde_json::to_string(&handshake).unwrap();
        writeln!(stream, "{json}").expect("send handshake");

        let reader = BufReader::new(stream.try_clone().unwrap());
        if let Some(Ok(line)) = reader.lines().next() {
            let msg: ProtocolMessage =
                serde_json::from_str(&line).expect("should parse handshake ack");
            match msg {
                ProtocolMessage::HandshakeAck { events } => {
                    assert!(!events.is_empty(), "should receive catch-up events");
                }
                other => panic!("expected HandshakeAck, got {other:?}"),
            }
        } else {
            panic!("should receive handshake ack");
        }

        server.shutdown();
        let _ = std::fs::remove_file(&socket_path);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn unix_socket_client_send_and_poll() {
        use crate::event_bus::Event;

        let (store, db_path) = temp_db();
        let socket_path = temp_socket_path();

        store
            .create_session("s1", 1, 1000, 2000, None, None)
            .expect("create session");

        let store_ref = Arc::new(std::sync::Mutex::new(store));
        let server =
            UnixSocketServer::with_path("server-1".into(), store_ref.clone(), socket_path.clone());
        server.accept_loop().expect("accept_loop should start");
        thread::sleep(std::time::Duration::from_millis(200));

        let client = UnixSocketClient::new(socket_path.clone(), "client-1".into());

        let event = Event::MessageStarted {
            session_id: "s1".into(),
            role: "user".into(),
        };
        let envelope = Envelope::new("client-1".into(), event);

        client
            .send_envelope(&envelope)
            .expect("send should succeed");
        thread::sleep(std::time::Duration::from_millis(200));

        let results = client.poll_remote_events().expect("poll should succeed");
        assert!(
            !results.is_empty(),
            "should find at least one event, got {results:?}"
        );

        server.shutdown();
        let _ = std::fs::remove_file(&socket_path);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn ipc_transport_impl_for_client() {
        use crate::event_bus::{Event, IpcTransport};

        let (store, db_path) = temp_db();
        let socket_path = temp_socket_path();

        store
            .create_session("s1", 1, 1000, 2000, None, None)
            .expect("create session");

        let store_ref = Arc::new(std::sync::Mutex::new(store));
        let server =
            UnixSocketServer::with_path("server-1".into(), store_ref.clone(), socket_path.clone());
        server.accept_loop().expect("accept_loop should start");
        thread::sleep(std::time::Duration::from_millis(200));

        let client = UnixSocketClient::new(socket_path.clone(), "client-1".into());

        let event = Event::ToolExecuted {
            session_id: "s1".into(),
            tool_name: "bash".into(),
            success: true,
            duration_ms: 42,
        };
        let envelope = Envelope::new("client-1".into(), event.clone());

        client.send(&envelope).expect("send via transport");
        thread::sleep(std::time::Duration::from_millis(200));

        let polled = client.poll_remote_events().expect("poll should succeed");
        assert!(
            !polled.is_empty(),
            "poll should return stored events, got {polled:?}"
        );

        server.shutdown();
        let _ = std::fs::remove_file(&socket_path);
        let _ = std::fs::remove_file(db_path);
    }
}
