use crate::error::ApiError;
use crate::providers::openai_compat::{expect_success, has_api_key as compat_has_api_key};
use crate::providers::{Provider, ProviderFuture};
use crate::types::{
    ContentBlockDelta, ContentBlockDeltaEvent, ContentBlockStartEvent, ContentBlockStopEvent,
    InputContentBlock, InputMessage, MessageDelta, MessageDeltaEvent, MessageRequest, MessageResponse,
    MessageStartEvent, MessageStopEvent, OutputContentBlock, StreamEvent, ToolChoice,
    ToolDefinition, ToolResultContentBlock, Usage,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::collections::VecDeque;

pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

#[derive(Debug, Clone)]
pub struct OpenRouterClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl OpenRouterClient {
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key: api_key.into(),
            base_url: read_base_url(),
        }
    }

    pub fn from_env() -> Result<Self, ApiError> {
        let api_key = read_env_non_empty("OPENROUTER_API_KEY")?
            .ok_or_else(|| ApiError::missing_credentials("OpenRouter", &["OPENROUTER_API_KEY"]))?;
        Ok(Self::new(api_key))
    }

    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub async fn send_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageResponse, ApiError> {
        let request = MessageRequest {
            stream: false,
            ..request.clone()
        };
        let response = self.send_raw_request(&request).await?;
        let response = expect_success(response).await?;
        let request_id = request_id_from_headers(response.headers());
        let payload = response.json::<ChatCompletionResponse>().await?;
        let mut normalized = normalize_response(&request.model, payload)?;
        if normalized.request_id.is_none() {
            normalized.request_id = request_id;
        }
        Ok(normalized)
    }

    pub async fn stream_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageStream, ApiError> {
        let request = MessageRequest {
            stream: true,
            ..request.clone()
        };
        let response = self.send_raw_request(&request).await?;
        let response = expect_success(response).await?;
        Ok(MessageStream::new(response, &request.model))
    }

    async fn send_raw_request(
        &self,
        request: &MessageRequest,
    ) -> Result<reqwest::Response, ApiError> {
        let payload = build_request_body(request);
        let request_url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        self.http
            .post(&request_url)
            .header("content-type", "application/json")
            .header("Authorization", format!("Bearer {}", &self.api_key))
            .header("HTTP-Referer", "https://github.com/ishanp/icode")
            .header("X-Title", "iCode")
            .json(&payload)
            .send()
            .await
            .map_err(ApiError::from)
    }
}

impl Provider for OpenRouterClient {
    type Stream = MessageStream;

    fn send_message<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> ProviderFuture<'a, MessageResponse> {
        Box::pin(async move { self.send_message(request).await })
    }

    fn stream_message<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> ProviderFuture<'a, Self::Stream> {
        Box::pin(async move { self.stream_message(request).await })
    }
}

#[must_use]
pub fn read_base_url() -> String {
    read_env_non_empty("OPENROUTER_BASE_URL")
        .ok()
        .flatten()
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

#[must_use]
pub fn has_api_key() -> bool {
    compat_has_api_key("OPENROUTER_API_KEY")
}

fn read_env_non_empty(key: &str) -> Result<Option<String>, ApiError> {
    match std::env::var(key) {
        Ok(value) if !value.is_empty() => Ok(Some(value)),
        Ok(_) | Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(ApiError::from(error)),
    }
}

fn request_id_from_headers(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

fn build_request_body(request: &MessageRequest) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = request.system.as_ref().filter(|s| !s.is_empty()) {
        messages.push(json!({ "role": "system", "content": system }));
    }
    for message in &request.messages {
        messages.extend(translate_message(message));
    }
    let mut payload = json!({
        "model": request.model,
        "max_tokens": request.max_tokens,
        "messages": messages,
        "stream": request.stream,
    });
    if request.stream {
        payload["stream_options"] = json!({ "include_usage": true });
    }
    if let Some(tools) = &request.tools {
        payload["tools"] =
            Value::Array(tools.iter().map(openai_tool_definition).collect::<Vec<_>>());
    }
    if let Some(tool_choice) = &request.tool_choice {
        payload["tool_choice"] = openai_tool_choice(tool_choice);
    }
    payload
}

fn translate_message(message: &InputMessage) -> Vec<Value> {
    match message.role.as_str() {
        "assistant" => {
            let mut text = String::new();
            let mut tool_calls = Vec::new();
            for block in &message.content {
                match block {
                    InputContentBlock::Text { text: value } => text.push_str(value),
                    InputContentBlock::ToolUse { id, name, input } => {
                        tool_calls.push(json!({
                            "id": id,
                            "type": "function",
                            "function": { "name": name, "arguments": input.to_string() }
                        }));
                    }
                    InputContentBlock::ToolResult { .. } => {}
                }
            }
            if text.is_empty() && tool_calls.is_empty() {
                Vec::new()
            } else {
                let mut obj = serde_json::Map::new();
                obj.insert("role".into(), json!("assistant"));
                if !text.is_empty() {
                    obj.insert("content".into(), json!(text));
                }
                if !tool_calls.is_empty() {
                    obj.insert("tool_calls".into(), json!(tool_calls));
                }
                vec![serde_json::Value::Object(obj)]
            }
        }
        _ => message
            .content
            .iter()
            .filter_map(|block| match block {
                InputContentBlock::Text { text } => Some(json!({
                    "role": "user", "content": text,
                })),
                InputContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => Some(json!({
                    "role": "tool",
                    "tool_call_id": tool_use_id,
                    "content": flatten_tool_result_content(content),
                    "is_error": is_error,
                })),
                InputContentBlock::ToolUse { .. } => None,
            })
            .collect(),
    }
}

fn flatten_tool_result_content(content: &[ToolResultContentBlock]) -> String {
    content
        .iter()
        .map(|block| match block {
            ToolResultContentBlock::Text { text } => text.clone(),
            ToolResultContentBlock::Json { value } => value.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn openai_tool_definition(tool: &ToolDefinition) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
        }
    })
}

fn openai_tool_choice(tool_choice: &ToolChoice) -> Value {
    match tool_choice {
        ToolChoice::Auto => Value::String("auto".to_string()),
        ToolChoice::Any => Value::String("required".to_string()),
        ToolChoice::Tool { name } => json!({
            "type": "function", "function": { "name": name },
        }),
    }
}

fn normalize_response(
    model: &str,
    response: ChatCompletionResponse,
) -> Result<MessageResponse, ApiError> {
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or(ApiError::InvalidSseFrame(
            "chat completion response missing choices",
        ))?;
    let mut content = Vec::new();
    if let Some(text) = choice.message.content.filter(|t| !t.is_empty()) {
        content.push(OutputContentBlock::Text { text });
    }
    for tool_call in choice.message.tool_calls {
        content.push(OutputContentBlock::ToolUse {
            id: tool_call.id,
            name: tool_call.function.name,
            input: parse_tool_arguments(&tool_call.function.arguments),
        });
    }
    Ok(MessageResponse {
        id: response.id,
        kind: "message".to_string(),
        role: choice.message.role,
        content,
        model: if response.model.is_empty() {
            model.to_string()
        } else {
            response.model
        },
        stop_reason: choice.finish_reason.map(|r| normalize_finish_reason(&r)),
        stop_sequence: None,
        usage: Usage {
            input_tokens: response.usage.as_ref().map_or(0, |u| u.prompt_tokens),
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            output_tokens: response.usage.as_ref().map_or(0, |u| u.completion_tokens),
        },
        request_id: None,
    })
}

fn parse_tool_arguments(arguments: &str) -> Value {
    serde_json::from_str(arguments).unwrap_or_else(|_| json!({ "raw": arguments }))
}

fn normalize_finish_reason(value: &str) -> String {
    match value {
        "stop" => "end_turn",
        "tool_calls" => "tool_use",
        other => other,
    }
    .to_string()
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    id: String,
    #[serde(default)]
    model: String,
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ResponseToolCall>,
}

#[derive(Debug, Deserialize)]
struct ResponseToolCall {
    id: String,
    function: ResponseToolFunction,
}

#[derive(Debug, Deserialize)]
struct ResponseToolFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

#[derive(Debug)]
pub struct MessageStream {
    response: reqwest::Response,
    parser: SseParser,
    pending: VecDeque<StreamEvent>,
    done: bool,
    state: StreamState,
}

impl MessageStream {
    fn new(response: reqwest::Response, model: &str) -> Self {
        Self {
            response,
            parser: SseParser::new(),
            pending: VecDeque::new(),
            done: false,
            state: StreamState::new(model.to_string()),
        }
    }

    #[must_use]
    pub fn request_id(&self) -> Option<String> {
        request_id_from_headers(self.response.headers())
    }

    pub async fn next_event(&mut self) -> Result<Option<StreamEvent>, ApiError> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Ok(Some(event));
            }
            if self.done {
                self.pending.extend(self.state.finish());
                if let Some(event) = self.pending.pop_front() {
                    return Ok(Some(event));
                }
                return Ok(None);
            }
            match self.response.chunk().await? {
                Some(chunk) => {
                    for parsed in self.parser.push(&chunk)? {
                        self.pending.extend(self.state.ingest_chunk(parsed));
                    }
                }
                None => self.done = true,
            }
        }
    }
}

#[derive(Debug, Default)]
struct SseParser {
    buffer: Vec<u8>,
}

impl SseParser {
    fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, chunk: &[u8]) -> Result<Vec<ChunkData>, ApiError> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some(frame) = extract_sse_frame(&mut self.buffer) {
            if let Some(data) = parse_sse_data(&frame)? {
                events.push(data);
            }
        }
        Ok(events)
    }
}

fn extract_sse_frame(buffer: &mut Vec<u8>) -> Option<String> {
    let separator = buffer
        .windows(2)
        .position(|w| w == b"\n\n")
        .map(|p| (p, 2))
        .or_else(|| {
            buffer
                .windows(4)
                .position(|w| w == b"\r\n\r\n")
                .map(|p| (p, 4))
        })?;
    let (pos, sep_len) = separator;
    let frame = buffer.drain(..pos + sep_len).collect::<Vec<_>>();
    let frame_len = frame.len().saturating_sub(sep_len);
    Some(String::from_utf8_lossy(&frame[..frame_len]).into_owned())
}

fn parse_sse_data(frame: &str) -> Result<Option<ChunkData>, ApiError> {
    let trimmed = frame.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let mut data_lines = Vec::new();
    for line in trimmed.lines() {
        if line.starts_with(':') {
            continue;
        }
        if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.trim_start());
        }
    }
    if data_lines.is_empty() {
        return Ok(None);
    }
    let payload = data_lines.join("\n");
    if payload == "[DONE]" {
        return Ok(None);
    }
    serde_json::from_str(&payload)
        .map(Some)
        .map_err(ApiError::from)
}

#[derive(Debug, Deserialize)]
struct ChunkData {
    id: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    choices: Vec<ChunkChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct ChunkChoice {
    #[serde(default)]
    delta: ChunkDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ChunkDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<DeltaToolCall>,
}

#[derive(Debug, Deserialize)]
struct DeltaToolCall {
    #[serde(default)]
    index: u32,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: DeltaFunction,
}

#[derive(Debug, Default, Deserialize)]
struct DeltaFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug)]
#[expect(clippy::struct_excessive_bools)]
struct StreamState {
    model: String,
    message_started: bool,
    text_started: bool,
    text_finished: bool,
    finished: bool,
    stop_reason: Option<String>,
    usage: Option<Usage>,
    tool_calls: BTreeMap<u32, ToolCallStreamState>,
}

impl StreamState {
    fn new(model: String) -> Self {
        Self {
            model,
            message_started: false,
            text_started: false,
            text_finished: false,
            finished: false,
            stop_reason: None,
            usage: None,
            tool_calls: BTreeMap::new(),
        }
    }

    #[expect(clippy::too_many_lines)]
    fn ingest_chunk(&mut self, chunk: ChunkData) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        if !self.message_started {
            self.message_started = true;
            events.push(StreamEvent::MessageStart(MessageStartEvent {
                message: MessageResponse {
                    id: chunk.id.clone(),
                    kind: "message".to_string(),
                    role: "assistant".to_string(),
                    content: Vec::new(),
                    model: chunk.model.clone().unwrap_or_else(|| self.model.clone()),
                    stop_reason: None,
                    stop_sequence: None,
                    usage: Usage {
                        input_tokens: 0,
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: 0,
                        output_tokens: 0,
                    },
                    request_id: None,
                },
            }));
        }
        if let Some(usage) = chunk.usage {
            self.usage = Some(Usage {
                input_tokens: usage.prompt_tokens,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
                output_tokens: usage.completion_tokens,
            });
        }
        for choice in chunk.choices {
            if let Some(content) = choice.delta.content.filter(|t| !t.is_empty()) {
                if !self.text_started {
                    self.text_started = true;
                    events.push(StreamEvent::ContentBlockStart(
                        crate::types::ContentBlockStartEvent {
                            index: 0,
                            content_block: OutputContentBlock::Text {
                                text: String::new(),
                            },
                        },
                    ));
                }
                events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                    index: 0,
                    delta: ContentBlockDelta::TextDelta { text: content },
                }));
            }
            for tc in choice.delta.tool_calls {
                let state = self.tool_calls.entry(tc.index).or_default();
                state.apply(tc);
                let block_index = state.block_index();
                if !state.started {
                    if state.start_event().is_some() {
                        state.started = true;
                        events.push(StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                            index: block_index,
                            content_block: OutputContentBlock::ToolUse {
                                id: state.id.clone().unwrap_or_default(),
                                name: state.name.clone().unwrap_or_default(),
                                input: json!({}),
                            },
                        }));
                    } else {
                        continue;
                    }
                }
                if let Some(ev) = state.delta_event() {
                    events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                        index: block_index,
                        delta: ContentBlockDelta::InputJsonDelta { partial_json: ev },
                    }));
                }
                if choice.finish_reason.as_deref() == Some("tool_calls") && !state.stopped {
                    state.stopped = true;
                    events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                        index: block_index,
                    }));
                }
            }
            if let Some(finish_reason) = choice.finish_reason {
                self.stop_reason = Some(normalize_finish_reason(&finish_reason));
                if finish_reason == "tool_calls" {
                    for st in self.tool_calls.values_mut() {
                        if st.started && !st.stopped {
                            st.stopped = true;
                            events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                                index: st.block_index(),
                            }));
                        }
                    }
                }
            }
        }
        events
    }

    fn finish(&mut self) -> Vec<StreamEvent> {
        if self.finished {
            return Vec::new();
        }
        self.finished = true;
        let mut events = Vec::new();
        if self.text_started && !self.text_finished {
            self.text_finished = true;
            events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                index: 0,
            }));
        }
        for st in self.tool_calls.values_mut() {
            if !st.started && st.start_event().is_some() {
                st.started = true;
                events.push(StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                    index: st.block_index(),
                    content_block: OutputContentBlock::ToolUse {
                        id: st.id.clone().unwrap_or_default(),
                        name: st.name.clone().unwrap_or_default(),
                        input: json!({}),
                    },
                }));
                if let Some(ev) = st.delta_event() {
                    events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                        index: st.block_index(),
                        delta: ContentBlockDelta::InputJsonDelta { partial_json: ev },
                    }));
                }
            }
            if st.started && !st.stopped {
                st.stopped = true;
                events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                    index: st.block_index(),
                }));
            }
        }
        if self.message_started {
            events.push(StreamEvent::MessageDelta(MessageDeltaEvent {
                delta: MessageDelta {
                    stop_reason: Some(
                        self.stop_reason
                            .clone()
                            .unwrap_or_else(|| "end_turn".to_string()),
                    ),
                    stop_sequence: None,
                },
                usage: self.usage.clone().unwrap_or(Usage {
                    input_tokens: 0,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                    output_tokens: 0,
                }),
            }));
            events.push(StreamEvent::MessageStop(MessageStopEvent {}));
        }
        events
    }
}

#[derive(Debug, Default)]
struct ToolCallStreamState {
    openai_index: u32,
    id: Option<String>,
    name: Option<String>,
    arguments: String,
    emitted_len: usize,
    started: bool,
    stopped: bool,
}

impl ToolCallStreamState {
    fn apply(&mut self, tc: DeltaToolCall) {
        self.openai_index = tc.index;
        if let Some(id) = tc.id {
            self.id = Some(id);
        }
        if let Some(name) = tc.function.name {
            self.name = Some(name);
        }
        if let Some(args) = tc.function.arguments {
            self.arguments.push_str(&args);
        }
    }

    const fn block_index(&self) -> u32 {
        self.openai_index + 1
    }

    fn start_event(&self) -> Option<()> {
        self.name.as_ref().map(|_| ())
    }

    fn delta_event(&mut self) -> Option<String> {
        if self.emitted_len >= self.arguments.len() {
            return None;
        }
        let delta = self.arguments[self.emitted_len..].to_string();
        self.emitted_len = self.arguments.len();
        Some(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::{has_api_key, read_base_url, DEFAULT_BASE_URL};
    use crate::providers::ProviderKind;
    use crate::providers::{capabilities_for_model, detect_provider_kind, resolve_model_alias};
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock")
    }

    #[test]
    fn detects_openrouter_from_model_prefix() {
        let _lock = env_lock();
        assert_eq!(
            detect_provider_kind("openrouter/anthropic/claude-3.5-sonnet"),
            ProviderKind::OpenRouter
        );
    }

    #[test]
    fn openrouter_capabilities_from_registry() {
        let caps = capabilities_for_model("openrouter/claude-sonnet");
        assert_eq!(caps.context_window, 200_000);
        assert!(caps.supports_tools);
        assert!(caps.supports_reasoning);
    }

    #[test]
    fn read_base_url_returns_default() {
        let _lock = env_lock();
        std::env::remove_var("OPENROUTER_BASE_URL");
        assert_eq!(read_base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn endpoint_url_construction() {
        let base = "https://openrouter.ai/api/v1";
        let url = format!("{base}/chat/completions");
        assert_eq!(url, "https://openrouter.ai/api/v1/chat/completions");
    }

    #[test]
    fn has_api_key_detects_env() {
        let _lock = env_lock();
        std::env::remove_var("OPENROUTER_API_KEY");
        assert!(!has_api_key());
        std::env::set_var("OPENROUTER_API_KEY", "sk-or-test-key");
        assert!(has_api_key());
        std::env::remove_var("OPENROUTER_API_KEY");
    }

    #[test]
    fn resolves_openrouter_model_alias_passthrough() {
        assert_eq!(
            resolve_model_alias("openrouter/claude-sonnet"),
            "openrouter/anthropic/claude-3.5-sonnet"
        );
    }
}
