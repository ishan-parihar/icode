use crate::sse::event_bus_to_sse;
use crate::state::ServerState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::Json;
use futures::stream::StreamExt;
use runtime::{
    ApiClient as RA, ApiRequest, AssistantEvent, ContentBlock, ConversationMessage,
    ConversationRuntime, PermissionMode, PermissionPolicy, RuntimeError, Session, ToolError,
    ToolExecutor,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::mpsc;
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
#[utoipa::path(get,path="/health",operation_id="health_check",tag="Health",responses((status=200,description="OK",body=super::schemas::HealthResponse)))]
pub async fn health() -> Json<serde_json::Value> {
    Json(json!({"status":"ok","version":env!("CARGO_PKG_VERSION")}))
}
#[utoipa::path(post,path="/sessions",operation_id="create_session",tag="Sessions",request_body=Option<CreateSessionRequest>,responses((status=201,description="Created"),(status=500,description="Error",body=super::schemas::ErrorResponse)))]
pub async fn create_session(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<Option<CreateSessionRequest>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let s = Session::new();
    let sid = s.session_id.clone();
    let ca = s.created_at_ms;
    {
        let st = state.store.lock().await;
        let _ = st.create_session(
            &sid,
            i64::from(s.version),
            ca as i64,
            s.updated_at_ms as i64,
            None,
            None,
        );
    }
    state
        .event_bus
        .publish_server_event(&runtime::ServerEvent::SessionCreated {
            session_id: sid.clone(),
            model: body
                .as_ref()
                .and_then(|b| b.model.as_ref())
                .cloned()
                .unwrap_or_default(),
        });
    (
        StatusCode::CREATED,
        Json(json!({"session_id":sid,"created_at":ca})),
    )
}
#[utoipa::path(get,path="/sessions",operation_id="list_sessions",tag="Sessions",responses((status=200,description="List",body=inline(serde_json::Value))))]
pub async fn list_sessions(
    State(state): State<Arc<ServerState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let st = state.store.lock().await;
    match st.list_sessions() {
        Ok(rows) => (
            StatusCode::OK,
            Json(
                json!({"sessions":rows.iter().map(|r|json!({"session_id":r.session_id,"created_at":r.created_at_ms,"updated_at":r.updated_at_ms,"version":r.version})).collect::<Vec<_>>()}),
            ),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to list sessions: {e}")})),
        ),
    }
}
#[utoipa::path(get,path="/sessions/{id}",operation_id="get_session",tag="Sessions",params(("id"=String,Path)),responses((status=200,description="Details"),(status=404,description="Not found")))]
pub async fn get_session(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let st = state.store.lock().await;
    match st.get_session(&id) {
        Ok(Some(r)) => (
            StatusCode::OK,
            Json(
                json!({"session_id":r.session_id,"created_at":r.created_at_ms,"updated_at":r.updated_at_ms,"version":r.version}),
            ),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":format!("session not found: {id}")})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to get session: {e}")})),
        ),
    }
}
#[utoipa::path(post,path="/sessions/{id}/message",operation_id="send_message",tag="Sessions",params(("id"=String,Path)),request_body=SendMessageRequest,responses((status=200,description="SSE stream")))]
pub async fn send_message(
    State(state): State<Arc<ServerState>>,
    Path(sid): Path<String>,
    Json(body): Json<SendMessageRequest>,
) -> Sse<impl futures::Stream<Item = Result<SseEvent, std::convert::Infallible>>> {
    let model = body
        .model
        .clone()
        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    state
        .event_bus
        .publish_server_event(&runtime::ServerEvent::MessageStarted {
            session_id: sid.clone(),
            role: "user".into(),
        });
    let s = sid.clone();
    let m = body.message.clone();
    let st = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        if let Err(e) = run_turn(&s, &m, &model, &tx, &st) {
            let _ = tx.send(format!("error: {e}"));
        }
    });
    Sse::new(
        tokio_stream::wrappers::UnboundedReceiverStream::new(rx)
            .map(|t| Ok(SseEvent::default().event("content").data(t))),
    )
    .keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}

fn run_turn(
    sid: &str,
    msg: &str,
    model: &str,
    tx: &mpsc::UnboundedSender<String>,
    state: &ServerState,
) -> Result<(), String> {
    let h = tokio::runtime::Handle::current();
    let session = h
        .block_on(async {
            let st = state.store.lock().await;
            load_sess(&st, sid)
        })
        .map_err(|e| e.to_string())?;
    let auth = resolve_auth().map_err(|e| e.to_string())?;
    let client = SC::new(h.clone(), model.into(), auth).map_err(|e| e.to_string())?;
    let mut rt = ConversationRuntime::new(
        session,
        client,
        SE,
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        vec!["You are a helpful AI coding assistant.".into()],
    );
    let tc = tx.clone();
    let eb: Arc<runtime::EventBus> = Arc::clone(&state.event_bus);
    let prog = move |ev: AssistantEvent| {
        if let AssistantEvent::TextDelta(d) = ev {
            let _ = tc.send(d);
        }
    };
    match rt.run_turn(msg, None, Some(&prog)) {
        Ok(s) => {
            let _ = tx.send(format!("\n\n[turn_complete: {} iterations]", s.iterations));
            eb.publish_server_event(&runtime::ServerEvent::MessageCompleted {
                session_id: sid.into(),
                role: "assistant".into(),
                input_tokens: s.usage.input_tokens as u64,
                output_tokens: s.usage.output_tokens as u64,
            });
            Ok(())
        }
        Err(e) => {
            let _ = tx.send(format!("\n\n[error: {e}]"));
            Err(e.to_string())
        }
    }
}
fn load_sess(st: &runtime::SqliteStore, id: &str) -> Result<Session, String> {
    if let Some(r) = st
        .get_session(id)
        .map_err(|e: runtime::PersistenceError| e.to_string())?
    {
        let mut s = Session::new();
        s.session_id = r.session_id;
        s.created_at_ms = r.created_at_ms as u64;
        s.updated_at_ms = r.updated_at_ms as u64;
        s.version = u32::try_from(r.version).unwrap_or(1);
        return Ok(s);
    }
    let s = Session::new();
    let _ = st.create_session(
        &s.session_id,
        i64::from(s.version),
        s.created_at_ms as i64,
        s.updated_at_ms as i64,
        None,
        None,
    );
    Ok(s)
}
fn resolve_auth() -> Result<api::AuthSource, api::ApiError> {
    api::resolve_startup_auth_source(|| Ok(None))
}
struct SC {
    h: tokio::runtime::Handle,
    client: api::ProviderClient,
    model: String,
}
impl SC {
    fn new(
        h: tokio::runtime::Handle,
        m: String,
        a: api::AuthSource,
    ) -> Result<Self, api::ApiError> {
        Ok(Self {
            h,
            client: api::ProviderClient::from_model_with_anthropic_auth(&m, Some(a))?,
            model: m,
        })
    }
}
impl RA for SC {
    fn stream(
        &mut self,
        req: ApiRequest,
        prog: Option<&dyn Fn(AssistantEvent)>,
    ) -> Result<Vec<AssistantEvent>, RuntimeError> {
        let mr = api::MessageRequest {
            model: self.model.clone(),
            max_tokens: 4096,
            messages: cm(&req.messages),
            system: (!req.system_prompt.is_empty()).then(|| req.system_prompt.join("\n\n")),
            tools: None,
            tool_choice: None,
            stream: true,
        };
        self.h.block_on(async {
            let mut stream = self
                .client
                .stream_message(&mr)
                .await
                .map_err(|e| RuntimeError::new(e.to_string()))?;
            let mut evts = Vec::new();
            let mut tb = String::new();
            let mut pt: Option<(String, String, String)> = None;
            use api::StreamEvent as SEv;
            while let Some(ev) = stream
                .next_event()
                .await
                .map_err(|e| RuntimeError::new(e.to_string()))?
            {
                match ev {
                    SEv::ContentBlockDelta(d) => {
                        if let api::ContentBlockDelta::TextDelta { text } = d.delta {
                            tb.push_str(&text);
                            if let Some(p) = prog {
                                p(AssistantEvent::TextDelta(text));
                            }
                        }
                    }
                    SEv::ContentBlockStop(_) => {
                        if let Some((i, n, inp)) = pt.take() {
                            evts.push(AssistantEvent::ToolUse {
                                id: i,
                                name: n,
                                input: inp,
                            });
                        }
                        tb.clear();
                    }
                    SEv::ContentBlockStart(s) => match s.content_block {
                        api::OutputContentBlock::Text { text } => {
                            tb = text;
                        }
                        api::OutputContentBlock::ToolUse { id, name, input } => {
                            pt =
                                Some((id, name, serde_json::to_string(&input).unwrap_or_default()));
                        }
                        _ => {}
                    },
                    SEv::MessageStop(_) => {
                        if let Some((i, n, inp)) = pt.take() {
                            evts.push(AssistantEvent::ToolUse {
                                id: i,
                                name: n,
                                input: inp,
                            });
                        }
                        evts.push(AssistantEvent::MessageStop);
                    }
                    SEv::MessageDelta(d) => {
                        let u = d.usage;
                        evts.push(AssistantEvent::Usage(runtime::TokenUsage {
                            input_tokens: u.input_tokens,
                            output_tokens: u.output_tokens,
                            cache_creation_input_tokens: u.cache_creation_input_tokens,
                            cache_read_input_tokens: u.cache_read_input_tokens,
                        }));
                    }
                    _ => {}
                }
            }
            Ok(evts)
        })
    }
}
fn cm(msgs: &[ConversationMessage]) -> Vec<api::InputMessage> {
    msgs.iter()
        .filter_map(|m| {
            let role = match m.role {
                runtime::MessageRole::Assistant => "assistant",
                _ => "user",
            };
            let content = m
                .blocks
                .iter()
                .map(|b| match b {
                    ContentBlock::Text { text } => {
                        api::InputContentBlock::Text { text: text.clone() }
                    }
                    ContentBlock::ToolUse { id, name, input } => api::InputContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: serde_json::from_str(input)
                            .unwrap_or_else(|_| serde_json::json!({"raw":input})),
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
            Some(api::InputMessage {
                role: role.into(),
                content,
            })
        })
        .collect()
}
struct SE;
impl ToolExecutor for SE {
    fn execute(&mut self, tn: &str, _: &str) -> Result<String, ToolError> {
        Err(ToolError::new(format!(
            "tool `{tn}` not available in server mode"
        )))
    }
}
#[utoipa::path(get,path="/events",operation_id="events_stream",tag="Events",responses((status=200,description="SSE",body=super::schemas::EventResponse)))]
pub async fn events(State(state): State<Arc<ServerState>>) -> impl axum::response::IntoResponse {
    event_bus_to_sse(&state.event_bus)
}
#[utoipa::path(post,path="/files/read",operation_id="read_file",tag="Files",request_body=super::schemas::FileReadRequest,responses((status=200,description="File",body=super::schemas::FileReadResponse)))]
pub async fn read_file_handler(
    Json(body): Json<ReadFileRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match runtime::read_file(&body.path, None, None) {
        Ok(o) => (
            StatusCode::OK,
            Json(
                json!({"path":body.path,"content":o.file.content,"total_lines":o.file.total_lines}),
            ),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":format!("failed to read file: {e}")})),
        ),
    }
}
#[utoipa::path(get,path="/config",operation_id="get_config",tag="Config",responses((status=200,description="Config")))]
pub async fn get_config(
    State(state): State<Arc<ServerState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.config_loader.load() {
        Ok(c) => {
            let m: BTreeMap<String, serde_json::Value> = c
                .merged()
                .iter()
                .filter_map(|(k, v)| {
                    serde_json::from_str::<serde_json::Value>(&v.render())
                        .ok()
                        .map(|val| (k.clone(), val))
                })
                .collect();
            (StatusCode::OK, Json(json!({"config":m})))
        }
        Err(e) => (
            StatusCode::OK,
            Json(json!({"config":{},"note":format!("config load error: {e}")})),
        ),
    }
}

// Tasks
#[utoipa::path(get,path="/tasks",operation_id="list_tasks",tag="Tasks",responses((status=200,description="List",body=inline(serde_json::Value))))]
pub async fn list_tasks(
    State(state): State<Arc<ServerState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let st = state.store.lock().await;
    match st.list_tasks(None) {
        Ok(rows) => (
            StatusCode::OK,
            Json(
                json!({"tasks":rows.iter().map(|r|json!({"task_id":r.task_id,"prompt":r.prompt,"description":r.description,"status":r.status,"created_at":r.created_at,"updated_at":r.updated_at,"output":r.output,"team_id":r.team_id})).collect::<Vec<_>>()}),
            ),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to list tasks: {e}")})),
        ),
    }
}
#[utoipa::path(get,path="/tasks/{id}",operation_id="get_task",tag="Tasks",params(("id"=String,Path)),responses((status=200,description="Details"),(status=404,description="Not found")))]
pub async fn get_task(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let st = state.store.lock().await;
    match st.get_task(&id) {
        Ok(Some(r)) => (
            StatusCode::OK,
            Json(
                json!({"task_id":r.task_id,"prompt":r.prompt,"description":r.description,"status":r.status,"created_at":r.created_at,"updated_at":r.updated_at,"output":r.output,"team_id":r.team_id}),
            ),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":format!("task not found: {id}")})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to get task: {e}")})),
        ),
    }
}
#[utoipa::path(post,path="/tasks",operation_id="create_task",tag="Tasks",request_body=super::schemas::TaskCreateRequest,responses((status=201,description="Created")))]
pub async fn create_task(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<super::schemas::TaskCreateRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let tid = uuid::Uuid::new_v4().to_string();
    let tpj = body.task_packet.as_ref().map(|v| v.to_string());
    let st = state.store.lock().await;
    match st.create_task(
        &tid,
        &body.prompt,
        body.description.as_deref(),
        tpj.as_deref(),
        "pending",
        body.team_id.as_deref(),
    ) {
        Ok(r) => (
            StatusCode::CREATED,
            Json(json!({"task_id":r.task_id,"status":r.status,"created_at":r.created_at})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to create task: {e}")})),
        ),
    }
}
#[utoipa::path(post,path="/tasks/{id}/stop",operation_id="stop_task",tag="Tasks",params(("id"=String,Path)),responses((status=200,description="Stopped"),(status=404,description="Not found")))]
pub async fn stop_task(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let st = state.store.lock().await;
    match st.get_task(&id) {
        Ok(Some(_)) => match st.update_task(&id, Some("stopped"), None, None) {
            Ok(()) => (
                StatusCode::OK,
                Json(json!({"task_id":id,"status":"stopped"})),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":format!("failed to stop task: {e}")})),
            ),
        },
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":format!("task not found: {id}")})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to get task: {e}")})),
        ),
    }
}
// Teams
#[utoipa::path(get,path="/teams",operation_id="list_teams",tag="Teams",responses((status=200,description="List",body=inline(serde_json::Value))))]
pub async fn list_teams(
    State(state): State<Arc<ServerState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let st = state.store.lock().await;
    match st.list_teams() {
        Ok(rows) => (
            StatusCode::OK,
            Json(
                json!({"teams":rows.iter().map(|r|json!({"team_id":r.team_id,"name":r.name,"status":r.status,"created_at":r.created_at,"updated_at":r.updated_at})).collect::<Vec<_>>()}),
            ),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to list teams: {e}")})),
        ),
    }
}
#[utoipa::path(post,path="/teams",operation_id="create_team",tag="Teams",request_body=super::schemas::TeamCreateRequest,responses((status=201,description="Created")))]
pub async fn create_team(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<super::schemas::TeamCreateRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let tid = uuid::Uuid::new_v4().to_string();
    let st = state.store.lock().await;
    match st.create_team(&tid, &body.name) {
        Ok(r) => (
            StatusCode::CREATED,
            Json(
                json!({"team_id":r.team_id,"name":r.name,"status":r.status,"created_at":r.created_at}),
            ),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to create team: {e}")})),
        ),
    }
}
#[utoipa::path(delete,path="/teams/{id}",operation_id="delete_team",tag="Teams",params(("id"=String,Path)),responses((status=200,description="Deleted"),(status=404,description="Not found")))]
pub async fn delete_team(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let st = state.store.lock().await;
    match st.delete_team(&id) {
        Ok(true) => (StatusCode::OK, Json(json!({"team_id":id,"deleted":true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":format!("team not found: {id}")})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to delete team: {e}")})),
        ),
    }
}
// Cron
#[utoipa::path(get,path="/cron",operation_id="list_crons",tag="Cron",responses((status=200,description="List",body=inline(serde_json::Value))))]
pub async fn list_crons(
    State(state): State<Arc<ServerState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let st = state.store.lock().await;
    match st.list_crons() {
        Ok(rows) => (
            StatusCode::OK,
            Json(
                json!({"crons":rows.iter().map(|r|json!({"cron_id":r.cron_id,"schedule":r.schedule,"prompt":r.prompt,"description":r.description,"enabled":r.enabled,"created_at":r.created_at,"updated_at":r.updated_at,"last_run_at":r.last_run_at,"run_count":r.run_count})).collect::<Vec<_>>()}),
            ),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to list crons: {e}")})),
        ),
    }
}
#[utoipa::path(post,path="/cron",operation_id="create_cron",tag="Cron",request_body=super::schemas::CronCreateRequest,responses((status=201,description="Created")))]
pub async fn create_cron(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<super::schemas::CronCreateRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let cid = uuid::Uuid::new_v4().to_string();
    let st = state.store.lock().await;
    match st.create_cron(
        &cid,
        &body.schedule,
        &body.prompt,
        body.description.as_deref(),
    ) {
        Ok(r) => (
            StatusCode::CREATED,
            Json(
                json!({"cron_id":r.cron_id,"schedule":r.schedule,"prompt":r.prompt,"enabled":r.enabled,"created_at":r.created_at}),
            ),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to create cron: {e}")})),
        ),
    }
}
#[utoipa::path(delete,path="/cron/{id}",operation_id="delete_cron",tag="Cron",params(("id"=String,Path)),responses((status=200,description="Deleted"),(status=404,description="Not found")))]
pub async fn delete_cron(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let st = state.store.lock().await;
    match st.delete_cron(&id) {
        Ok(true) => (StatusCode::OK, Json(json!({"cron_id":id,"deleted":true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":format!("cron not found: {id}")})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to delete cron: {e}")})),
        ),
    }
}
// MCP
#[utoipa::path(get,path="/mcp",operation_id="list_mcp",tag="MCP",responses((status=200,description="List",body=inline(serde_json::Value))))]
pub async fn list_mcp(
    State(state): State<Arc<ServerState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let st = state.store.lock().await;
    match st.list_mcp_servers() {
        Ok(rows) => (
            StatusCode::OK,
            Json(
                json!({"mcp_servers":rows.iter().map(|r|json!({"server_name":r.server_name,"status":r.status,"server_info":r.server_info,"error_message":r.error_message})).collect::<Vec<_>>()}),
            ),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to list MCP servers: {e}")})),
        ),
    }
}
#[utoipa::path(post,path="/mcp/{name}/connect",operation_id="connect_mcp",tag="MCP",params(("name"=String,Path)),responses((status=200,description="Connected")))]
pub async fn connect_mcp(
    State(state): State<Arc<ServerState>>,
    Path(name): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let st = state.store.lock().await;
    match st.upsert_mcp_server(&name, "connecting", None, None) {
        Ok(r) => (
            StatusCode::OK,
            Json(json!({"server_name":r.server_name,"status":r.status})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to connect MCP server: {e}")})),
        ),
    }
}
#[utoipa::path(post,path="/mcp/{name}/disconnect",operation_id="disconnect_mcp",tag="MCP",params(("name"=String,Path)),responses((status=200,description="Disconnected"),(status=404,description="Not found")))]
pub async fn disconnect_mcp(
    State(state): State<Arc<ServerState>>,
    Path(name): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let st = state.store.lock().await;
    match st.get_mcp_server(&name) {
        Ok(Some(_)) => match st.upsert_mcp_server(&name, "disconnected", None, None) {
            Ok(r) => (
                StatusCode::OK,
                Json(json!({"server_name":r.server_name,"status":r.status})),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":format!("failed to disconnect MCP: {e}")})),
            ),
        },
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":format!("MCP server not found: {name}")})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to get MCP server: {e}")})),
        ),
    }
}
// Workers
#[utoipa::path(get,path="/workers",operation_id="list_workers",tag="Workers",responses((status=200,description="List",body=inline(serde_json::Value))))]
pub async fn list_workers(
    State(state): State<Arc<ServerState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let st = state.store.lock().await;
    match st.list_workers() {
        Ok(rows) => (
            StatusCode::OK,
            Json(
                json!({"workers":rows.iter().map(|r|json!({"worker_id":r.worker_id,"cwd":r.cwd,"status":r.status,"trust_auto_resolve":r.trust_auto_resolve,"trust_gate_cleared":r.trust_gate_cleared,"created_at":r.created_at,"updated_at":r.updated_at})).collect::<Vec<_>>()}),
            ),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to list workers: {e}")})),
        ),
    }
}
#[utoipa::path(post,path="/workers",operation_id="create_worker",tag="Workers",request_body=super::schemas::WorkerCreateRequest,responses((status=201,description="Created")))]
pub async fn create_worker(
    State(state): State<Arc<ServerState>>,
    Json(body): Json<super::schemas::WorkerCreateRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let wid = uuid::Uuid::new_v4().to_string();
    let ta = body.trust_auto_resolve.unwrap_or(false);
    let ar = body.auto_recover_prompt_misdelivery.unwrap_or(false);
    let st = state.store.lock().await;
    match st.create_worker(&wid, &body.cwd, ta, ar) {
        Ok(r) => (
            StatusCode::CREATED,
            Json(
                json!({"worker_id":r.worker_id,"cwd":r.cwd,"status":r.status,"created_at":r.created_at}),
            ),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to create worker: {e}")})),
        ),
    }
}
#[utoipa::path(get,path="/workers/{id}",operation_id="get_worker",tag="Workers",params(("id"=String,Path)),responses((status=200,description="Details"),(status=404,description="Not found")))]
pub async fn get_worker(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let st = state.store.lock().await;
    match st.get_worker(&id) {
        Ok(Some(r)) => (
            StatusCode::OK,
            Json(
                json!({"worker_id":r.worker_id,"cwd":r.cwd,"status":r.status,"trust_auto_resolve":r.trust_auto_resolve,"trust_gate_cleared":r.trust_gate_cleared,"auto_recover_prompt_misdelivery":r.auto_recover_prompt_misdelivery,"prompt_delivery_attempts":r.prompt_delivery_attempts,"last_prompt":r.last_prompt,"replay_prompt":r.replay_prompt,"last_error_json":r.last_error_json,"created_at":r.created_at,"updated_at":r.updated_at}),
            ),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":format!("worker not found: {id}")})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to get worker: {e}")})),
        ),
    }
}
#[utoipa::path(post,path="/workers/{id}/restart",operation_id="restart_worker",tag="Workers",params(("id"=String,Path)),responses((status=200,description="Restarted"),(status=404,description="Not found")))]
pub async fn restart_worker(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let st = state.store.lock().await;
    match st.get_worker(&id) {
        Ok(Some(r)) => {
            match st.update_worker(&id, Some("spawning"), None, None, None, None, None) {
                Ok(()) => (
                    StatusCode::OK,
                    Json(json!({"worker_id":id,"status":"spawning","cwd":r.cwd})),
                ),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error":format!("failed to restart worker: {e}")})),
                ),
            }
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":format!("worker not found: {id}")})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to get worker: {e}")})),
        ),
    }
}
// LSP
#[utoipa::path(get,path="/lsp",operation_id="list_lsp",tag="LSP",responses((status=200,description="List",body=inline(serde_json::Value))))]
pub async fn list_lsp(
    State(state): State<Arc<ServerState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let st = state.store.lock().await;
    match st.list_lsp_servers() {
        Ok(rows) => (
            StatusCode::OK,
            Json(
                json!({"lsp_servers":rows.iter().map(|r|json!({"language":r.language,"status":r.status,"root_path":r.root_path,"capabilities_json":r.capabilities_json})).collect::<Vec<_>>()}),
            ),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to list LSP servers: {e}")})),
        ),
    }
}
#[utoipa::path(post,path="/lsp/{language}/connect",operation_id="connect_lsp",tag="LSP",params(("language"=String,Path)),request_body=Option<super::schemas::LspConnectRequest>,responses((status=200,description="Connected")))]
pub async fn connect_lsp(
    State(state): State<Arc<ServerState>>,
    Path(language): Path<String>,
    Json(body): Json<Option<super::schemas::LspConnectRequest>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let rp = body.and_then(|b| b.root_path);
    let st = state.store.lock().await;
    match st.upsert_lsp_server(&language, "connecting", rp.as_deref(), None) {
        Ok(r) => (
            StatusCode::OK,
            Json(json!({"language":r.language,"status":r.status,"root_path":r.root_path})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":format!("failed to connect LSP server: {e}")})),
        ),
    }
}
