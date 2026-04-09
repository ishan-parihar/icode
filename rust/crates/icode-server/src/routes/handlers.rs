use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::Json;
use futures::stream::Stream;
use futures::stream::StreamExt;
use runtime::SqliteStore;
use runtime::{
    ApiClient as RuntimeApiClient, ApiRequest, AssistantEvent, ContentBlock, ConversationMessage,
    ConversationRuntime, PermissionMode, PermissionPolicy, RuntimeError, Session, ToolError,
    ToolExecutor,
};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::sse::event_bus_to_sse;
use crate::state::ServerState;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SendMessageRequest {
    pub message: String,
    pub model: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateSessionRequest {
    pub model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReadFileRequest {
    pub path: String,
}

#[utoipa::path(
    get,
    path = "/health",
    operation_id = "health_check",
    tag = "Health",
    responses(
        (status = 200, description = "Server is healthy", body = super::schemas::HealthResponse),
    ),
)]
pub async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

#[utoipa::path(
    post,
    path = "/sessions",
    operation_id = "create_session",
    tag = "Sessions",
    request_body = Option<CreateSessionRequest>,
    responses(
        (status = 201, description = "Session created successfully", body = super::schemas::SessionResponse),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn create_session(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<Option<CreateSessionRequest>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let session = Session::new();
    let session_id = session.session_id.clone();
    let created_at = session.created_at_ms;

    let store = state.store.lock().await;
    let _ = store.create_session(
        &session_id,
        i64::from(session.version),
        created_at,
        session.updated_at_ms,
        None,
        None,
    );
    drop(store);

    state.event_bus.publish(runtime::Event::SessionCreated {
        session_id: session_id.clone(),
        model: body
            .as_ref()
            .and_then(|b| b.model.as_ref())
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
    });

    (
        StatusCode::CREATED,
        Json(json!({
            "session_id": session_id,
            "created_at": created_at,
        })),
    )
}

#[utoipa::path(
    get,
    path = "/sessions",
    operation_id = "list_sessions",
    tag = "Sessions",
    responses(
        (status = 200, description = "List of all sessions", body = inline(serde_json::Value)),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn list_sessions(
    State(state): State<Arc<ServerState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.lock().await;
    match store.list_sessions() {
        Ok(rows) => {
            let sessions: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    json!({
                        "session_id": r.session_id,
                        "created_at": r.created_at_ms,
                        "updated_at": r.updated_at_ms,
                        "version": r.version,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "sessions": sessions })))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to list sessions: {e}") })),
        ),
    }
}

#[utoipa::path(
    get,
    path = "/sessions/{id}",
    operation_id = "get_session",
    tag = "Sessions",
    params(
        ("id" = String, Path, description = "Session ID"),
    ),
    responses(
        (status = 200, description = "Session details", body = super::schemas::SessionResponse),
        (status = 404, description = "Session not found", body = super::schemas::ErrorResponse),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn get_session(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.lock().await;
    match store.get_session(&id) {
        Ok(Some(row)) => (
            StatusCode::OK,
            Json(json!({
                "session_id": row.session_id,
                "created_at": row.created_at_ms,
                "updated_at": row.updated_at_ms,
                "version": row.version,
            })),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("session not found: {id}") })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to get session: {e}") })),
        ),
    }
}

/// Wraps an SSE stream with task abort handles.
/// When the stream is dropped (client disconnect), aborts the spawned
/// blocking conversation turn and its async wrapper task.
struct GuardedSseStream<S> {
    inner: S,
    async_handle: tokio::task::JoinHandle<()>,
}

impl<S: Stream + Unpin> Stream for GuardedSseStream<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Stream::poll_next(Pin::new(&mut self.inner), cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S> Drop for GuardedSseStream<S> {
    fn drop(&mut self) {
        // Abort the async wrapper task, which in turn drops the blocking handle
        // and signals cancellation to the blocking thread pool.
        self.async_handle.abort();
    }
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/message",
    operation_id = "send_message",
    tag = "Sessions",
    params(
        ("id" = String, Path, description = "Session ID"),
    ),
    request_body = SendMessageRequest,
    responses(
        (status = 200, description = "SSE stream of assistant response", body = String),
    ),
)]
pub async fn send_message(
    State(state): State<Arc<ServerState>>,
    Path(session_id): Path<String>,
    Json(body): Json<SendMessageRequest>,
) -> Sse<impl futures::Stream<Item = Result<SseEvent, std::convert::Infallible>>> {
    let model = body
        .model
        .clone()
        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());

    let (tx, rx) = mpsc::channel::<String>(1024);

    state.event_bus.publish(runtime::Event::MessageStarted {
        session_id: session_id.clone(),
        role: "user".to_string(),
    });

    let tx_for_blocking = tx.clone();
    let session_id_for_task = session_id.clone();
    let message = body.message.clone();
    let state_ref = Arc::clone(&state);
    let blocking_handle = tokio::task::spawn_blocking(move || {
        match run_conversation_turn(
            &session_id_for_task,
            &message,
            &model,
            &tx_for_blocking,
            &state_ref,
        ) {
            Ok(()) => {}
            Err(e) => {
                let _ = tx_for_blocking.blocking_send(format!("error: {e}"));
            }
        }
    });

    let async_handle = tokio::spawn(async move {
        if let Err(e) = blocking_handle.await {
            let _ = tx.try_send(format!("\n\n[error: conversation turn failed — {e}]"));
        }
    });

    let stream = GuardedSseStream {
        inner: ReceiverStream::new(rx).map(|text| Ok(SseEvent::default().event("content").data(text))),
        async_handle,
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}

fn run_conversation_turn(
    session_id: &str,
    user_message: &str,
    model: &str,
    tx: &mpsc::Sender<String>,
    state: &ServerState,
) -> Result<(), String> {
    let handle = tokio::runtime::Handle::current();

    let session = handle
        .block_on(async {
            let store = state.store.lock().await;
            load_or_create_session(&store, session_id)
        })
        .map_err(|e: String| e)?;

    let auth_source = resolve_auth_source().map_err(|e: api::ApiError| e.to_string())?;
    let api_client = ServerApiClient::new(handle.clone(), model.to_string(), auth_source)
        .map_err(|e: api::ApiError| e.to_string())?;

    let tool_executor = ServerToolExecutor;

    let permission_policy = PermissionPolicy::new(PermissionMode::DangerFullAccess);
    let system_prompt = vec!["You are a helpful AI coding assistant.".to_string()];

    let mut runtime = ConversationRuntime::new(
        session,
        api_client,
        tool_executor,
        permission_policy,
        system_prompt,
    );

    let tx_clone = tx.clone();
    let event_bus = Arc::clone(&state.event_bus);
    let progress = move |event: AssistantEvent| {
        if let AssistantEvent::TextDelta(delta) = event {
            let _ = tx_clone.blocking_send(delta);
        }
    };

    match runtime.run_turn(user_message, None, Some(&progress)) {
        Ok(summary) => {
            let _ = tx.blocking_send(format!(
                "\n\n[turn_complete: {} iterations]",
                summary.iterations
            ));
            let usage = summary.usage;
            event_bus.publish(runtime::Event::MessageCompleted {
                session_id: session_id.to_string(),
                role: "assistant".to_string(),
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
            });
            Ok(())
        }
        Err(e) => {
            let _ = tx.blocking_send(format!("\n\n[error: {e}]"));
            Err(e.to_string())
        }
    }
}

fn load_or_create_session(store: &SqliteStore, session_id: &str) -> Result<Session, String> {
    if let Some(row) = store
        .get_session(session_id)
        .map_err(|e: runtime::PersistenceError| e.to_string())?
    {
        let mut session = Session::new();
        session.session_id = row.session_id;
        session.created_at_ms = row.created_at_ms;
        session.updated_at_ms = row.updated_at_ms;
        session.version = u32::try_from(row.version).unwrap_or(1);
        return Ok(session);
    }
    let session = Session::new();
    let _ = store.create_session(
        &session.session_id,
        i64::from(session.version),
        session.created_at_ms,
        session.updated_at_ms,
        None,
        None,
    );
    Ok(session)
}

fn resolve_auth_source() -> Result<api::AuthSource, api::ApiError> {
    api::resolve_startup_auth_source(|| Ok(None))
}

struct ServerApiClient {
    handle: tokio::runtime::Handle,
    client: api::ProviderClient,
    model: String,
}

impl ServerApiClient {
    fn new(
        handle: tokio::runtime::Handle,
        model: String,
        auth_source: api::AuthSource,
    ) -> Result<Self, api::ApiError> {
        let client =
            api::ProviderClient::from_model_with_anthropic_auth(&model, Some(auth_source))?;
        Ok(Self {
            handle,
            client,
            model,
        })
    }
}

impl RuntimeApiClient for ServerApiClient {
    fn stream(
        &mut self,
        request: ApiRequest,
        progress: Option<&dyn Fn(AssistantEvent)>,
    ) -> Result<Vec<AssistantEvent>, RuntimeError> {
        let message_request = api::MessageRequest {
            model: self.model.clone(),
            max_tokens: 4096,
            messages: convert_messages(&request.messages),
            system: (!request.system_prompt.is_empty()).then(|| request.system_prompt.join("\n\n")),
            tools: None,
            tool_choice: None,
            stream: true,
        };

        self.handle.block_on(async {
            let mut stream = self
                .client
                .stream_message(&message_request)
                .await
                .map_err(|e| RuntimeError::new(e.to_string()))?;

            use api::StreamEvent as ApiStreamEvent;
            let mut events = Vec::new();
            let mut text_buf = String::new();
            let mut pending_tool: Option<(String, String, String)> = None;

            while let Some(event) = stream
                .next_event()
                .await
                .map_err(|e| RuntimeError::new(e.to_string()))?
            {
                match event {
                    ApiStreamEvent::ContentBlockDelta(delta) => {
                        if let api::ContentBlockDelta::TextDelta { text } = delta.delta {
                            text_buf.push_str(&text);
                            if let Some(progress) = progress {
                                progress(AssistantEvent::TextDelta(text));
                            }
                        }
                    }
                    ApiStreamEvent::ContentBlockStop(_stop) => {
                        if let Some((id, name, input)) = pending_tool.take() {
                            events.push(AssistantEvent::ToolUse { id, name, input });
                        }
                        if !text_buf.is_empty() {
                            text_buf.clear();
                        }
                    }
                    ApiStreamEvent::ContentBlockStart(start) => match start.content_block {
                        api::OutputContentBlock::Text { text } => {
                            text_buf = text;
                        }
                        api::OutputContentBlock::ToolUse { id, name, input } => {
                            pending_tool =
                                Some((id, name, serde_json::to_string(&input).unwrap_or_default()));
                        }
                        _ => {}
                    },
                    ApiStreamEvent::MessageStop(_) => {
                        if let Some((id, name, input)) = pending_tool.take() {
                            events.push(AssistantEvent::ToolUse { id, name, input });
                        }
                        events.push(AssistantEvent::MessageStop);
                        if let Some(cb) = &progress {
                            cb(runtime::AssistantEvent::MessageStop);
                        }
                    }
                    #[allow(clippy::match_wildcard_for_single_variants)]
                    ApiStreamEvent::MessageDelta(delta) => {
                        if let Some(cb) = &progress {
                            cb(runtime::AssistantEvent::Usage(delta.usage.token_usage()));
                        }
                    }
                    ApiStreamEvent::MessageStart(_) => {}
                }
            }

            Ok(events)
        })
    }
}

fn convert_messages(messages: &[ConversationMessage]) -> Vec<api::InputMessage> {
    messages
        .iter()
        .map(|message| {
            let role = match message.role {
                runtime::MessageRole::System
                | runtime::MessageRole::User
                | runtime::MessageRole::Tool => "user",
                runtime::MessageRole::Assistant => "assistant",
            };
            let content = message
                .blocks
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => {
                        api::InputContentBlock::Text { text: text.clone() }
                    }
                    ContentBlock::ToolUse { id, name, input } => api::InputContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: serde_json::from_str(input)
                            .unwrap_or_else(|_| serde_json::json!({ "raw": input })),
                    },
                    ContentBlock::ToolResult {
                        tool_use_id,
                        output,
                        is_error,
                        ..
                    } => api::InputContentBlock::ToolResult {
                        tool_use_id: tool_use_id.clone(),
                        content: vec![api::ToolResultContentBlock::Text {
                            text: output.clone(),
                        }],
                        is_error: *is_error,
                    },
                })
                .collect();
            api::InputMessage {
                role: role.to_string(),
                content,
            }
        })
        .collect()
}

struct ServerToolExecutor;

impl ToolExecutor for ServerToolExecutor {
    fn execute(&mut self, tool_name: &str, _input: &str) -> Result<String, ToolError> {
        Err(ToolError::new(format!(
            "tool `{tool_name}` not available in server mode"
        )))
    }
}

#[utoipa::path(
    get,
    path = "/events",
    operation_id = "events_stream",
    tag = "Events",
    responses(
        (status = 200, description = "SSE event stream", body = super::schemas::EventResponse),
    ),
)]
pub async fn events(State(state): State<Arc<ServerState>>) -> impl axum::response::IntoResponse {
    event_bus_to_sse(&state.event_bus)
}

#[utoipa::path(
    post,
    path = "/files/read",
    operation_id = "read_file",
    tag = "Files",
    request_body = super::schemas::FileReadRequest,
    responses(
        (status = 200, description = "File contents", body = super::schemas::FileReadResponse),
        (status = 400, description = "Bad request / file not found", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn read_file_handler(
    Json(body): Json<ReadFileRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match runtime::read_file(&body.path, None, None) {
        Ok(output) => (
            StatusCode::OK,
            Json(json!({
                "path": body.path,
                "content": output.file.content,
                "total_lines": output.file.total_lines,
            })),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("failed to read file: {e}") })),
        ),
    }
}

#[utoipa::path(
    get,
    path = "/config",
    operation_id = "get_config",
    tag = "Config",
    responses(
        (status = 200, description = "Current configuration", body = inline(serde_json::Value)),
    ),
)]
pub async fn get_config(
    State(state): State<Arc<ServerState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.config_loader.load() {
        Ok(config) => {
            let merged: BTreeMap<String, serde_json::Value> = config
                .merged()
                .iter()
                .filter_map(|(k, v)| {
                    let json_str = v.render();
                    serde_json::from_str::<serde_json::Value>(&json_str)
                        .ok()
                        .map(|val| (k.clone(), val))
                })
                .collect();
            (StatusCode::OK, Json(json!({ "config": merged })))
        }
        Err(e) => (
            StatusCode::OK,
            Json(json!({
                "config": {},
                "note": format!("config load error: {e}"),
            })),
        ),
    }
}

// ── Task handlers ────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/tasks",
    operation_id = "list_tasks",
    tag = "Tasks",
    params(
        ("status" = Option<String>, Query, description = "Filter by status"),
    ),
    responses(
        (status = 200, description = "List of tasks", body = inline(serde_json::Value)),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn list_tasks(
    State(state): State<Arc<ServerState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.lock().await;
    match store.list_tasks(None) {
        Ok(rows) => {
            let tasks: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    json!({
                        "task_id": r.task_id,
                        "prompt": r.prompt,
                        "description": r.description,
                        "status": r.status,
                        "created_at": r.created_at,
                        "updated_at": r.updated_at,
                        "output": r.output,
                        "team_id": r.team_id,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "tasks": tasks })))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to list tasks: {e}") })),
        ),
    }
}

#[utoipa::path(
    get,
    path = "/tasks/{id}",
    operation_id = "get_task",
    tag = "Tasks",
    params(
        ("id" = String, Path, description = "Task ID"),
    ),
    responses(
        (status = 200, description = "Task details", body = inline(serde_json::Value)),
        (status = 404, description = "Task not found", body = super::schemas::ErrorResponse),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn get_task(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.lock().await;
    match store.get_task(&id) {
        Ok(Some(row)) => (
            StatusCode::OK,
            Json(json!({
                "task_id": row.task_id,
                "prompt": row.prompt,
                "description": row.description,
                "status": row.status,
                "created_at": row.created_at,
                "updated_at": row.updated_at,
                "output": row.output,
                "team_id": row.team_id,
            })),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("task not found: {id}") })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to get task: {e}") })),
        ),
    }
}

#[utoipa::path(
    post,
    path = "/tasks",
    operation_id = "create_task",
    tag = "Tasks",
    request_body = super::schemas::TaskCreateRequest,
    responses(
        (status = 201, description = "Task created", body = inline(serde_json::Value)),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn create_task(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<super::schemas::TaskCreateRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let task_id = uuid::Uuid::new_v4().to_string();
    let task_packet_json = body
        .task_packet
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());

    let store = state.store.lock().await;
    match store.create_task(
        &task_id,
        &body.prompt,
        body.description.as_deref(),
        task_packet_json.as_deref(),
        "pending",
        body.team_id.as_deref(),
    ) {
        Ok(row) => {
            drop(store);
            (
                StatusCode::CREATED,
                Json(json!({
                    "task_id": row.task_id,
                    "status": row.status,
                    "created_at": row.created_at,
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to create task: {e}") })),
        ),
    }
}

#[utoipa::path(
    post,
    path = "/tasks/{id}/stop",
    operation_id = "stop_task",
    tag = "Tasks",
    params(
        ("id" = String, Path, description = "Task ID"),
    ),
    responses(
        (status = 200, description = "Task stopped", body = inline(serde_json::Value)),
        (status = 404, description = "Task not found", body = super::schemas::ErrorResponse),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn stop_task(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.lock().await;
    match store.get_task(&id) {
        Ok(Some(_)) => match store.update_task(&id, Some("stopped"), None, None) {
            Ok(()) => (
                StatusCode::OK,
                Json(json!({
                    "task_id": id,
                    "status": "stopped",
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to stop task: {e}") })),
            ),
        },
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("task not found: {id}") })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to get task: {e}") })),
        ),
    }
}

// ── Team handlers ────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/teams",
    operation_id = "list_teams",
    tag = "Teams",
    responses(
        (status = 200, description = "List of teams", body = inline(serde_json::Value)),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn list_teams(
    State(state): State<Arc<ServerState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.lock().await;
    match store.list_teams() {
        Ok(rows) => {
            let teams: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    json!({
                        "team_id": r.team_id,
                        "name": r.name,
                        "status": r.status,
                        "created_at": r.created_at,
                        "updated_at": r.updated_at,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "teams": teams })))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to list teams: {e}") })),
        ),
    }
}

#[utoipa::path(
    post,
    path = "/teams",
    operation_id = "create_team",
    tag = "Teams",
    request_body = super::schemas::TeamCreateRequest,
    responses(
        (status = 201, description = "Team created", body = inline(serde_json::Value)),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn create_team(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<super::schemas::TeamCreateRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let team_id = uuid::Uuid::new_v4().to_string();

    let store = state.store.lock().await;
    match store.create_team(&team_id, &body.name) {
        Ok(row) => (
            StatusCode::CREATED,
            Json(json!({
                "team_id": row.team_id,
                "name": row.name,
                "status": row.status,
                "created_at": row.created_at,
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to create team: {e}") })),
        ),
    }
}

#[utoipa::path(
    delete,
    path = "/teams/{id}",
    operation_id = "delete_team",
    tag = "Teams",
    params(
        ("id" = String, Path, description = "Team ID"),
    ),
    responses(
        (status = 200, description = "Team deleted", body = inline(serde_json::Value)),
        (status = 404, description = "Team not found", body = super::schemas::ErrorResponse),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn delete_team(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.lock().await;
    match store.delete_team(&id) {
        Ok(true) => (
            StatusCode::OK,
            Json(json!({ "team_id": id, "deleted": true })),
        ),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("team not found: {id}") })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to delete team: {e}") })),
        ),
    }
}

// ── Cron handlers ────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/cron",
    operation_id = "list_crons",
    tag = "Cron",
    responses(
        (status = 200, description = "List of cron entries", body = inline(serde_json::Value)),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn list_crons(
    State(state): State<Arc<ServerState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.lock().await;
    match store.list_crons() {
        Ok(rows) => {
            let crons: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    json!({
                        "cron_id": r.cron_id,
                        "schedule": r.schedule,
                        "prompt": r.prompt,
                        "description": r.description,
                        "enabled": r.enabled,
                        "created_at": r.created_at,
                        "updated_at": r.updated_at,
                        "last_run_at": r.last_run_at,
                        "run_count": r.run_count,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "crons": crons })))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to list crons: {e}") })),
        ),
    }
}

#[utoipa::path(
    post,
    path = "/cron",
    operation_id = "create_cron",
    tag = "Cron",
    request_body = super::schemas::CronCreateRequest,
    responses(
        (status = 201, description = "Cron entry created", body = inline(serde_json::Value)),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn create_cron(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<super::schemas::CronCreateRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let cron_id = uuid::Uuid::new_v4().to_string();

    let store = state.store.lock().await;
    match store.create_cron(
        &cron_id,
        &body.schedule,
        &body.prompt,
        body.description.as_deref(),
    ) {
        Ok(row) => (
            StatusCode::CREATED,
            Json(json!({
                "cron_id": row.cron_id,
                "schedule": row.schedule,
                "prompt": row.prompt,
                "enabled": row.enabled,
                "created_at": row.created_at,
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to create cron: {e}") })),
        ),
    }
}

#[utoipa::path(
    delete,
    path = "/cron/{id}",
    operation_id = "delete_cron",
    tag = "Cron",
    params(
        ("id" = String, Path, description = "Cron ID"),
    ),
    responses(
        (status = 200, description = "Cron entry deleted", body = inline(serde_json::Value)),
        (status = 404, description = "Cron not found", body = super::schemas::ErrorResponse),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn delete_cron(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.lock().await;
    match store.delete_cron(&id) {
        Ok(true) => (
            StatusCode::OK,
            Json(json!({ "cron_id": id, "deleted": true })),
        ),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("cron not found: {id}") })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to delete cron: {e}") })),
        ),
    }
}

// ── MCP handlers ─────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/mcp",
    operation_id = "list_mcp",
    tag = "MCP",
    responses(
        (status = 200, description = "List of MCP servers", body = inline(serde_json::Value)),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn list_mcp(
    State(state): State<Arc<ServerState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.lock().await;
    match store.list_mcp_servers() {
        Ok(rows) => {
            let servers: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    json!({
                        "server_name": r.server_name,
                        "status": r.status,
                        "server_info": r.server_info,
                        "error_message": r.error_message,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "mcp_servers": servers })))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to list MCP servers: {e}") })),
        ),
    }
}

#[utoipa::path(
    post,
    path = "/mcp/{name}/connect",
    operation_id = "connect_mcp",
    tag = "MCP",
    params(
        ("name" = String, Path, description = "MCP server name"),
    ),
    responses(
        (status = 200, description = "MCP server connected", body = inline(serde_json::Value)),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn connect_mcp(
    State(state): State<Arc<ServerState>>,
    Path(name): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.lock().await;
    match store.upsert_mcp_server(&name, "connecting", None, None) {
        Ok(row) => (
            StatusCode::OK,
            Json(json!({
                "server_name": row.server_name,
                "status": row.status,
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to connect MCP server: {e}") })),
        ),
    }
}

#[utoipa::path(
    post,
    path = "/mcp/{name}/disconnect",
    operation_id = "disconnect_mcp",
    tag = "MCP",
    params(
        ("name" = String, Path, description = "MCP server name"),
    ),
    responses(
        (status = 200, description = "MCP server disconnected", body = inline(serde_json::Value)),
        (status = 404, description = "MCP server not found", body = super::schemas::ErrorResponse),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn disconnect_mcp(
    State(state): State<Arc<ServerState>>,
    Path(name): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.lock().await;
    match store.get_mcp_server(&name) {
        Ok(Some(_)) => match store.upsert_mcp_server(&name, "disconnected", None, None) {
            Ok(row) => (
                StatusCode::OK,
                Json(json!({
                    "server_name": row.server_name,
                    "status": row.status,
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to disconnect MCP server: {e}") })),
            ),
        },
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("MCP server not found: {name}") })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to get MCP server: {e}") })),
        ),
    }
}

// ── Worker handlers ──────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/workers",
    operation_id = "list_workers",
    tag = "Workers",
    responses(
        (status = 200, description = "List of workers", body = inline(serde_json::Value)),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn list_workers(
    State(state): State<Arc<ServerState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.lock().await;
    match store.list_workers() {
        Ok(rows) => {
            let workers: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    json!({
                        "worker_id": r.worker_id,
                        "cwd": r.cwd,
                        "status": r.status,
                        "trust_auto_resolve": r.trust_auto_resolve,
                        "trust_gate_cleared": r.trust_gate_cleared,
                        "created_at": r.created_at,
                        "updated_at": r.updated_at,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "workers": workers })))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to list workers: {e}") })),
        ),
    }
}

#[utoipa::path(
    post,
    path = "/workers",
    operation_id = "create_worker",
    tag = "Workers",
    request_body = super::schemas::WorkerCreateRequest,
    responses(
        (status = 201, description = "Worker created", body = inline(serde_json::Value)),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn create_worker(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<super::schemas::WorkerCreateRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let worker_id = uuid::Uuid::new_v4().to_string();
    let trust_auto_resolve = body.trust_auto_resolve.unwrap_or(false);
    let auto_recover = body.auto_recover_prompt_misdelivery.unwrap_or(false);

    let store = state.store.lock().await;
    match store.create_worker(&worker_id, &body.cwd, trust_auto_resolve, auto_recover) {
        Ok(row) => (
            StatusCode::CREATED,
            Json(json!({
                "worker_id": row.worker_id,
                "cwd": row.cwd,
                "status": row.status,
                "created_at": row.created_at,
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to create worker: {e}") })),
        ),
    }
}

#[utoipa::path(
    get,
    path = "/workers/{id}",
    operation_id = "get_worker",
    tag = "Workers",
    params(
        ("id" = String, Path, description = "Worker ID"),
    ),
    responses(
        (status = 200, description = "Worker details", body = inline(serde_json::Value)),
        (status = 404, description = "Worker not found", body = super::schemas::ErrorResponse),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn get_worker(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.lock().await;
    match store.get_worker(&id) {
        Ok(Some(row)) => (
            StatusCode::OK,
            Json(json!({
                "worker_id": row.worker_id,
                "cwd": row.cwd,
                "status": row.status,
                "trust_auto_resolve": row.trust_auto_resolve,
                "trust_gate_cleared": row.trust_gate_cleared,
                "auto_recover_prompt_misdelivery": row.auto_recover_prompt_misdelivery,
                "prompt_delivery_attempts": row.prompt_delivery_attempts,
                "last_prompt": row.last_prompt,
                "replay_prompt": row.replay_prompt,
                "last_error_json": row.last_error_json,
                "created_at": row.created_at,
                "updated_at": row.updated_at,
            })),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("worker not found: {id}") })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to get worker: {e}") })),
        ),
    }
}

#[utoipa::path(
    post,
    path = "/workers/{id}/restart",
    operation_id = "restart_worker",
    tag = "Workers",
    params(
        ("id" = String, Path, description = "Worker ID"),
    ),
    responses(
        (status = 200, description = "Worker restarted", body = inline(serde_json::Value)),
        (status = 404, description = "Worker not found", body = super::schemas::ErrorResponse),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn restart_worker(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.lock().await;
    match store.get_worker(&id) {
        Ok(Some(row)) => {
            match store.update_worker(&id, Some("spawning"), None, None, None, None, None) {
                Ok(()) => (
                    StatusCode::OK,
                    Json(json!({
                        "worker_id": id,
                        "status": "spawning",
                        "cwd": row.cwd,
                    })),
                ),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("failed to restart worker: {e}") })),
                ),
            }
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("worker not found: {id}") })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to get worker: {e}") })),
        ),
    }
}

// ── LSP handlers ─────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/lsp",
    operation_id = "list_lsp",
    tag = "LSP",
    responses(
        (status = 200, description = "List of LSP servers", body = inline(serde_json::Value)),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn list_lsp(
    State(state): State<Arc<ServerState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let store = state.store.lock().await;
    match store.list_lsp_servers() {
        Ok(rows) => {
            let servers: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    json!({
                        "language": r.language,
                        "status": r.status,
                        "root_path": r.root_path,
                        "capabilities_json": r.capabilities_json,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "lsp_servers": servers })))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to list LSP servers: {e}") })),
        ),
    }
}

#[utoipa::path(
    post,
    path = "/lsp/{language}/connect",
    operation_id = "connect_lsp",
    tag = "LSP",
    params(
        ("language" = String, Path, description = "Language identifier"),
    ),
    request_body = Option<super::schemas::LspConnectRequest>,
    responses(
        (status = 200, description = "LSP server connected", body = inline(serde_json::Value)),
        (status = 500, description = "Internal server error", body = super::schemas::ErrorResponse),
    ),
)]
pub async fn connect_lsp(
    State(state): State<Arc<ServerState>>,
    Path(language): Path<String>,
    Json(body): Json<Option<super::schemas::LspConnectRequest>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let root_path = body.and_then(|b| b.root_path);

    let store = state.store.lock().await;
    match store.upsert_lsp_server(&language, "connecting", root_path.as_deref(), None) {
        Ok(row) => (
            StatusCode::OK,
            Json(json!({
                "language": row.language,
                "status": row.status,
                "root_path": row.root_path,
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to connect LSP server: {e}") })),
        ),
    }
}
