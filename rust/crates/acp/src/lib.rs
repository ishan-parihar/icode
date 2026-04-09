use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

// ── JSON-RPC types ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    fn result(id: Option<serde_json::Value>, value: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(value),
            error: None,
        }
    }

    fn error(id: Option<serde_json::Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

// ── Dispatch ────────────────────────────────────────────────────────────────

fn dispatch_request(req: &JsonRpcRequest) -> JsonRpcResponse {
    match req.method.as_str() {
        "initialize" => handle_initialize(req.id.as_ref()),
        "tool/list" => handle_tool_list(req.id.as_ref()),
        "session/create" => handle_session_create(req.id.as_ref()),
        "session/list" => handle_session_list(req.id.as_ref()),
        "model/list" => handle_model_list(req.id.as_ref()),
        "session/message" => handle_session_message(req.id.as_ref(), req.params.as_ref()),
        _ => JsonRpcResponse::error(
            req.id.clone(),
            -32601,
            format!("Method not found: {}", req.method),
        ),
    }
}

fn handle_initialize(id: Option<&serde_json::Value>) -> JsonRpcResponse {
    let result = serde_json::json!({
        "serverInfo": {
            "name": "icode",
            "version": "0.1.0"
        },
        "capabilities": {
            "sessions": {
                "create": true,
                "list": true
            },
            "tools": {
                "list": true
            },
            "models": {
                "list": true
            }
        }
    });
    JsonRpcResponse::result(id.cloned(), result)
}

fn handle_tool_list(id: Option<&serde_json::Value>) -> JsonRpcResponse {
    let tools = [
        "Bash",
        "Read",
        "Write",
        "Edit",
        "Glob",
        "Grep",
        "PtyBash",
        "BatchEdit",
        "ApplyPatch",
        "format_file",
        "WebFetch",
        "WebSearch",
    ];
    let result: serde_json::Value = serde_json::Value::Array(
        tools
            .into_iter()
            .map(|name| serde_json::json!({ "name": name }))
            .collect(),
    );
    JsonRpcResponse::result(id.cloned(), result)
}

fn handle_session_create(id: Option<&serde_json::Value>) -> JsonRpcResponse {
    let now = chrono::Utc::now().timestamp_millis();
    let nonce: u32 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let session_id = format!("acp-{now:x}-{nonce:x}");
    let result = serde_json::json!({
        "session_id": session_id,
        "status": "created"
    });
    JsonRpcResponse::result(id.cloned(), result)
}

fn handle_session_list(id: Option<&serde_json::Value>) -> JsonRpcResponse {
    let sessions = list_sessions();
    let result = serde_json::Value::Array(sessions);
    JsonRpcResponse::result(id.cloned(), result)
}

fn list_sessions() -> Vec<serde_json::Value> {
    let session_dir = session_dir();
    if !session_dir.exists() {
        return vec![];
    }

    let mut entries: Vec<_> = match std::fs::read_dir(&session_dir) {
        Ok(iter) => iter
            .filter_map(std::result::Result::ok)
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|ext| ext == "json" || ext == "jsonl")
            })
            .collect(),
        Err(_) => return vec![],
    };

    // Sort by modification time, most recent first
    entries.sort_by(|a, b| {
        let a_mtime = a.metadata().ok().and_then(|m| m.modified().ok());
        let b_mtime = b.metadata().ok().and_then(|m| m.modified().ok());
        b_mtime.cmp(&a_mtime)
    });

    // Return up to 100 most recent
    let sessions: Vec<_> = entries
        .into_iter()
        .take(100)
        .filter_map(|entry| {
            let path = entry.path();
            let file_name = path.file_name()?.to_str()?.to_string();
            let metadata = entry.metadata().ok()?;
            let modified = metadata
                .modified()
                .ok()
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());
            Some(serde_json::json!({
                "file": file_name,
                "path": path.to_string_lossy().to_string(),
                "modified": modified,
                "size": metadata.len()
            }))
        })
        .collect();

    sessions
}

fn session_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("ICODE_SESSION_DIR") {
        return std::path::PathBuf::from(dir);
    }
    if let Some(home) = dirs_like_home() {
        let p = home.join(".icode/sessions");
        if p.exists() {
            return p;
        }
    }
    std::env::current_dir()
        .unwrap_or_default()
        .join(".icode/sessions")
}

/// Best-effort home directory without the `dirs` crate dependency.
fn dirs_like_home() -> Option<std::path::PathBuf> {
    std::env::var("HOME").ok().map(std::path::PathBuf::from)
}

fn handle_model_list(id: Option<&serde_json::Value>) -> JsonRpcResponse {
    let mut models: Vec<_> = api::list_all_models()
        .map(|entry| {
            let provider = provider_kind_to_string(entry.provider);
            serde_json::json!({
                "alias": entry.alias,
                "canonical": entry.canonical,
                "provider": provider,
                "context_window": entry.capabilities.context_window,
                "max_output": entry.capabilities.max_output,
                "supports_tools": entry.capabilities.supports_tools,
                "supports_reasoning": entry.capabilities.supports_reasoning,
                "supports_images": entry.capabilities.supports_images,
            })
        })
        .collect();

    // Sort by provider then canonical id
    models.sort_by(|a, b| {
        let a_provider = a["provider"].as_str().unwrap_or("");
        let b_provider = b["provider"].as_str().unwrap_or("");
        let provider_cmp = a_provider.cmp(b_provider);
        if provider_cmp.is_ne() {
            return provider_cmp;
        }
        let a_canonical = a["canonical"].as_str().unwrap_or("");
        let b_canonical = b["canonical"].as_str().unwrap_or("");
        a_canonical.cmp(b_canonical)
    });

    JsonRpcResponse::result(id.cloned(), serde_json::Value::Array(models))
}

fn provider_kind_to_string(kind: api::ProviderKind) -> &'static str {
    match kind {
        api::ProviderKind::Anthropic => "Anthropic",
        api::ProviderKind::Xai => "xAI",
        api::ProviderKind::OpenAi => "OpenAI",
        api::ProviderKind::QwenProxy => "QwenProxy",
        api::ProviderKind::Azure => "Azure",
        api::ProviderKind::Gemini => "Gemini",
        api::ProviderKind::Bedrock => "Bedrock",
        api::ProviderKind::OpenRouter => "OpenRouter",
        api::ProviderKind::Mistral => "Mistral",
        api::ProviderKind::Groq => "Groq",
    }
}

#[allow(clippy::too_many_lines)]
fn handle_session_message(
    id: Option<&serde_json::Value>,
    params: Option<&serde_json::Value>,
) -> JsonRpcResponse {
    let Some(params) = params else {
        return JsonRpcResponse::error(
            id.cloned(),
            -32602,
            "Missing params: requires { session_id, message }",
        );
    };

    let message = match params.get("message").and_then(|v| v.as_str()) {
        Some(m) => m.to_string(),
        None => {
            return JsonRpcResponse::error(
                id.cloned(),
                -32602,
                "Missing or invalid 'message' field (string required)",
            );
        }
    };

    let session_id = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    let model = params
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("sonnet")
        .to_string();

    let permission_mode_str = params
        .get("permission_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("danger-full-access");

    let permission_mode = match permission_mode_str {
        "read-only" => runtime::PermissionMode::ReadOnly,
        "workspace-write" => runtime::PermissionMode::WorkspaceWrite,
        #[allow(clippy::match_same_arms)]
        "danger-full-access" => runtime::PermissionMode::DangerFullAccess,
        _ => runtime::PermissionMode::DangerFullAccess,
    };

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let session_dir = acp_session_dir();
    let session_path = session_dir.join(format!("{session_id}.json"));

    let session = if session_path.exists() {
        match runtime::Session::load_from_path(&session_path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load session, creating new one");
                runtime::Session::new().with_persistence_path(&session_path)
            }
        }
    } else {
        std::fs::create_dir_all(&session_dir).ok();
        runtime::Session::new().with_persistence_path(&session_path)
    };

    let system_prompt = match build_system_prompt(&cwd) {
        Ok(prompt) => prompt,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to build system prompt, using minimal prompt");
            vec!["You are a helpful AI coding assistant.".to_string()]
        }
    };

    let permission_policy = runtime::PermissionPolicy::new(permission_mode);

    let api_client = match AcpApiClient::new(&model, session_id) {
        Ok(c) => c,
        Err(e) => {
            return JsonRpcResponse::error(
                id.cloned(),
                -1,
                format!("Failed to initialize API client: {e}"),
            );
        }
    };

    let tool_executor = AcpToolExecutor::new();

    let mut runtime = runtime::ConversationRuntime::new(
        session,
        api_client,
        tool_executor,
        permission_policy,
        system_prompt,
    );

    let result = runtime.run_turn(&message, None::<&mut dyn runtime::PermissionPrompter>, None);

    if let Err(e) = runtime.session().save_to_path(&session_path) {
        tracing::warn!(error = %e, "Failed to persist session");
    }

    match result {
        Ok(summary) => {
            let assistant_text = extract_assistant_text(&summary);
            let result = serde_json::json!({
                "status": "completed",
                "session_id": session_id,
                "message": assistant_text,
                "iterations": summary.iterations,
                "usage": {
                    "input_tokens": summary.usage.input_tokens,
                    "output_tokens": summary.usage.output_tokens,
                    "cache_creation_input_tokens": summary.usage.cache_creation_input_tokens,
                    "cache_read_input_tokens": summary.usage.cache_read_input_tokens,
                },
                "tool_uses": collect_tool_uses(&summary),
            });
            JsonRpcResponse::result(id.cloned(), result)
        }
        Err(e) => JsonRpcResponse::error(id.cloned(), -1, format!("Conversation error: {e}")),
    }
}

// ── ACP API Client ──────────────────────────────────────────────────────────

struct AcpApiClient {
    handle: tokio::runtime::Handle,
    client: api::ProviderClient,
    model: String,
}

impl AcpApiClient {
    fn new(model: &str, session_id: &str) -> anyhow::Result<Self> {
        let resolved = api::resolve_model_alias(model);
        let client = api::ProviderClient::from_model(&resolved)
            .map_err(|e| anyhow::anyhow!("Failed to create provider client: {e}"))?;
        let client = client.with_prompt_cache(api::PromptCache::new(session_id));
        Ok(Self {
            handle: tokio::runtime::Handle::current(),
            client,
            model: model.to_string(),
        })
    }
}

impl runtime::ApiClient for AcpApiClient {
    #[allow(clippy::too_many_lines)]
    fn stream(
        &mut self,
        request: runtime::ApiRequest,
        progress: Option<&dyn Fn(runtime::AssistantEvent)>,
    ) -> Result<Vec<runtime::AssistantEvent>, runtime::RuntimeError> {
        let messages: Vec<api::InputMessage> = request
            .messages
            .iter()
            .filter_map(|message| {
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
                        runtime::ContentBlock::Text { text } => {
                            api::InputContentBlock::Text { text: text.clone() }
                        }
                        runtime::ContentBlock::ToolUse { id, name, input } => {
                            api::InputContentBlock::ToolUse {
                                id: id.clone(),
                                name: name.clone(),
                                input: serde_json::from_str(input)
                                    .unwrap_or_else(|_| serde_json::json!({ "raw": input })),
                            }
                        }
                        runtime::ContentBlock::ToolResult {
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
                    .collect::<Vec<_>>();
                (!content.is_empty()).then(|| api::InputMessage {
                    role: role.to_string(),
                    content,
                })
            })
            .collect();

        let tool_defs = tools::mvp_tool_specs()
            .into_iter()
            .map(|spec| api::ToolDefinition {
                name: spec.name.to_string(),
                description: Some(spec.description.to_string()),
                input_schema: spec.input_schema,
            })
            .collect();

        let max_tokens = api::max_tokens_for_model(&self.model);
        let message_request = api::MessageRequest {
            model: self.model.clone(),
            max_tokens,
            messages,
            system: (!request.system_prompt.is_empty()).then(|| request.system_prompt.join("\n\n")),
            tools: Some(tool_defs),
            tool_choice: Some(api::ToolChoice::Auto),
            stream: true,
        };

        self.handle.block_on(async {
            let mut stream = self
                .client
                .stream_message(&message_request)
                .await
                .map_err(|e| runtime::RuntimeError::new(e.to_string()))?;

            let mut events = Vec::new();
            let mut pending_tool: Option<(String, String, String)> = None;
            let mut saw_stop = false;

            while let Some(event) = stream
                .next_event()
                .await
                .map_err(|e| runtime::RuntimeError::new(e.to_string()))?
            {
                match event {
                    api::StreamEvent::MessageStart(start) => {
                        for block in start.message.content {
                            match block {
                                api::OutputContentBlock::Text { text } => {
                                    if let Some(cb) = &progress {
                                        cb(runtime::AssistantEvent::TextDelta(text));
                                    }
                                }
                                api::OutputContentBlock::ToolUse { id, name, input } => {
                                    pending_tool = Some((
                                        id,
                                        name,
                                        serde_json::to_string(&input).unwrap_or_default(),
                                    ));
                                }
                                api::OutputContentBlock::Thinking { .. }
                                | api::OutputContentBlock::RedactedThinking { .. } => {}
                            }
                        }
                    }
                    api::StreamEvent::ContentBlockStart(start) => match start.content_block {
                        api::OutputContentBlock::Text { text } => {
                            if let Some(cb) = &progress {
                                cb(runtime::AssistantEvent::TextDelta(text));
                            }
                        }
                        api::OutputContentBlock::ToolUse { id, name, input } => {
                            pending_tool =
                                Some((id, name, serde_json::to_string(&input).unwrap_or_default()));
                        }
                        api::OutputContentBlock::Thinking { .. }
                        | api::OutputContentBlock::RedactedThinking { .. } => {}
                    },
                    api::StreamEvent::ContentBlockDelta(delta) => match delta.delta {
                        api::ContentBlockDelta::TextDelta { text } => {
                            if !text.is_empty() {
                                if let Some(cb) = &progress {
                                    cb(runtime::AssistantEvent::TextDelta(text.clone()));
                                }
                                events.push(runtime::AssistantEvent::TextDelta(text));
                            }
                        }
                        api::ContentBlockDelta::InputJsonDelta { partial_json } => {
                            if let Some((_, _, input)) = &mut pending_tool {
                                input.push_str(&partial_json);
                            }
                        }
                        api::ContentBlockDelta::ThinkingDelta { .. }
                        | api::ContentBlockDelta::SignatureDelta { .. } => {}
                    },
                    api::StreamEvent::ContentBlockStop(_) => {
                        if let Some((id, name, input)) = pending_tool.take() {
                            if let Some(cb) = &progress {
                                cb(runtime::AssistantEvent::ToolUse {
                                    id: id.clone(),
                                    name: name.clone(),
                                    input: input.clone(),
                                });
                            }
                            events.push(runtime::AssistantEvent::ToolUse { id, name, input });
                        }
                    }
                    api::StreamEvent::MessageDelta(delta) => {
                        if let Some(cb) = &progress {
                            cb(runtime::AssistantEvent::Usage(delta.usage.token_usage()));
                        }
                    }
                    api::StreamEvent::MessageStop(_) => {
                        saw_stop = true;
                        if let Some(cb) = &progress {
                            cb(runtime::AssistantEvent::MessageStop);
                        }
                    }
                }
            }

            if !saw_stop
                && events.iter().any(|e| {
                    matches!(e, runtime::AssistantEvent::TextDelta(t) if !t.is_empty())
                        || matches!(e, runtime::AssistantEvent::ToolUse { .. })
                })
            {
                events.push(runtime::AssistantEvent::MessageStop);
            }

            Ok(events)
        })
    }
}

// ── ACP Tool Executor ───────────────────────────────────────────────────────

struct AcpToolExecutor {
    registry: tools::GlobalToolRegistry,
}

impl AcpToolExecutor {
    fn new() -> Self {
        Self {
            registry: tools::GlobalToolRegistry::builtin(),
        }
    }
}

impl runtime::ToolExecutor for AcpToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, runtime::ToolError> {
        let input_value: serde_json::Value = serde_json::from_str(input).map_err(|e| {
            runtime::ToolError::new(format!("Invalid JSON input for {tool_name}: {e}"))
        })?;

        self.registry
            .execute(tool_name, &input_value)
            .map_err(runtime::ToolError::new)
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn build_system_prompt(cwd: &PathBuf) -> anyhow::Result<Vec<String>> {
    let current_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let os_name = std::env::consts::OS.to_string();
    let os_version = os_release();
    Ok(runtime::load_system_prompt(
        cwd,
        current_date,
        os_name,
        os_version,
    )?)
}

fn os_release() -> String {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/version").ok().map_or_else(
            || "linux".to_string(),
            |v| v.split_whitespace().take(3).collect::<Vec<_>>().join(" "),
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        std::env::consts::OS.to_string()
    }
}

fn extract_assistant_text(summary: &runtime::TurnSummary) -> String {
    summary
        .assistant_messages
        .iter()
        .flat_map(|msg| msg.blocks.iter())
        .filter_map(|block| match block {
            runtime::ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn collect_tool_uses(summary: &runtime::TurnSummary) -> Vec<serde_json::Value> {
    summary
        .assistant_messages
        .iter()
        .flat_map(|msg| msg.blocks.iter())
        .filter_map(|block| match block {
            runtime::ContentBlock::ToolUse { id, name, input } => {
                let output = summary
                    .tool_results
                    .iter()
                    .flat_map(|rmsg| rmsg.blocks.iter())
                    .find_map(|rblock| match rblock {
                        runtime::ContentBlock::ToolResult {
                            tool_use_id,
                            output,
                            is_error,
                            ..
                        } if tool_use_id == id => Some((output.clone(), *is_error)),
                        _ => None,
                    });
                let (output, is_error) = output.unwrap_or_default();
                Some(serde_json::json!({
                    "tool": name,
                    "input": input,
                    "output": output,
                    "is_error": is_error,
                }))
            }
            _ => None,
        })
        .collect()
}

fn acp_session_dir() -> PathBuf {
    let dir = session_dir();
    std::fs::create_dir_all(&dir).ok();
    dir
}

// ── Server loop ─────────────────────────────────────────────────────────────

pub async fn run_acp_server() -> anyhow::Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut writer = BufWriter::new(stdout);

    let ready = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "server/ready",
        "params": {
            "name": "icode",
            "version": "0.1.0",
            "capabilities": {
                "sessions": true,
                "tools": true,
                "streaming": false
            }
        }
    });
    let ready_str = serde_json::to_string(&ready)?;
    writer.write_all(ready_str.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    tracing::info!("ACP server ready, waiting for requests");

    let mut line = String::new();
    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            tracing::info!("EOF received, shutting down ACP server");
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                let error_response =
                    JsonRpcResponse::error(None, -32700, format!("Parse error: {e}"));
                let json = serde_json::to_string(&error_response)?;
                writer.write_all(json.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
                continue;
            }
        };

        let response = dispatch_request(&req);
        let json = serde_json::to_string(&response)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }

    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dispatch(method: &str, params: Option<serde_json::Value>) -> JsonRpcResponse {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: method.to_string(),
            params,
        };
        dispatch_request(&req)
    }

    #[test]
    fn initialize_returns_server_info() {
        let resp = dispatch("initialize", None);
        assert!(resp.error.is_none());
        let result = resp.result.expect("expected result");
        assert_eq!(result["serverInfo"]["name"], "icode");
        assert_eq!(result["serverInfo"]["version"], "0.1.0");
        assert!(result["capabilities"]["sessions"]["create"]
            .as_bool()
            .is_some());
    }

    #[test]
    fn tool_list_returns_tools() {
        let resp = dispatch("tool/list", None);
        assert!(resp.error.is_none());
        let result = resp.result.expect("expected result");
        let tools = result.as_array().expect("tools should be an array");
        assert!(!tools.is_empty());
        let names: Vec<_> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"Bash"));
        assert!(names.contains(&"Read"));
        assert!(names.contains(&"WebSearch"));
    }

    #[test]
    fn session_create_returns_id() {
        let resp = dispatch("session/create", None);
        assert!(resp.error.is_none());
        let result = resp.result.expect("expected result");
        let session_id = result["session_id"].as_str().expect("expected session_id");
        assert!(session_id.starts_with("acp-"));
        assert_eq!(result["status"], "created");
    }

    #[test]
    fn model_list_returns_models() {
        let resp = dispatch("model/list", None);
        assert!(resp.error.is_none());
        let result = resp.result.expect("expected result");
        let models = result.as_array().expect("models should be an array");
        assert!(!models.is_empty());
        // Verify sorted by provider then canonical
        for i in 1..models.len() {
            let prev_provider = models[i - 1]["provider"].as_str().unwrap_or("");
            let curr_provider = models[i]["provider"].as_str().unwrap_or("");
            if prev_provider == curr_provider {
                let prev_canonical = models[i - 1]["canonical"].as_str().unwrap_or("");
                let curr_canonical = models[i]["canonical"].as_str().unwrap_or("");
                assert!(
                    prev_canonical <= curr_canonical,
                    "models should be sorted by canonical within provider"
                );
            } else {
                assert!(
                    prev_provider <= curr_provider,
                    "models should be sorted by provider"
                );
            }
        }
    }

    #[test]
    fn unknown_method_returns_error() {
        let resp = dispatch("foo/bar", None);
        assert!(resp.result.is_none());
        let error = resp.error.expect("expected error");
        assert_eq!(error.code, -32601);
        assert!(error.message.contains("foo/bar"));
    }

    #[test]
    fn parse_error_returns_code_32700() {
        let bad_input = "not json at all";
        let parse_err: Result<JsonRpcRequest, _> = serde_json::from_str(bad_input);
        assert!(parse_err.is_err());

        let resp = JsonRpcResponse::error(
            Some(serde_json::json!(1)),
            -32700,
            "Parse error: expected value at line 1 column 1",
        );
        assert_eq!(resp.error.as_ref().expect("expected error").code, -32700);
    }
}
