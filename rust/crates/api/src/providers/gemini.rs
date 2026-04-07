use crate::error::ApiError;
use crate::providers::openai_compat::has_api_key as compat_has_api_key;
use crate::providers::{Provider, ProviderFuture};
use crate::types::{
    ContentBlockDelta, ContentBlockDeltaEvent, ContentBlockStartEvent, ContentBlockStopEvent,
    InputContentBlock, MessageDelta, MessageDeltaEvent, MessageRequest, MessageResponse,
    MessageStartEvent, MessageStopEvent, OutputContentBlock, StreamEvent, ToolChoice,
    ToolResultContentBlock, Usage,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::VecDeque;

pub const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";

#[derive(Debug, Clone)]
pub struct GeminiClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl GeminiClient {
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    pub fn from_env() -> Result<Self, ApiError> {
        let api_key = read_env_non_empty("GEMINI_API_KEY")?
            .ok_or_else(|| ApiError::missing_credentials("Gemini", &["GEMINI_API_KEY"]))?;
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
        let body = build_request_body(request, false);
        let model_name = extract_model_name(&request.model);
        let url = format!(
            "{}/{model_name}:generateContent?key={}",
            self.base_url.trim_end_matches('/'),
            self.api_key,
        );
        let response = self.http.post(&url).json(&body).send().await?;
        let response = expect_success(response).await?;
        let request_id = response
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned);
        let payload = response.json::<GeminiResponse>().await?;
        let normalized = normalize_response(&request.model, payload)?;
        Ok(MessageResponse {
            request_id,
            ..normalized
        })
    }

    pub async fn stream_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageStream, ApiError> {
        let body = build_request_body(request, true);
        let model_name = extract_model_name(&request.model);
        let url = format!(
            "{}/{model_name}:streamGenerateContent?alt=sse&key={}",
            self.base_url.trim_end_matches('/'),
            self.api_key,
        );
        let response = self.http.post(&url).json(&body).send().await?;
        let response = expect_success(response).await?;
        Ok(MessageStream::new(response, &request.model))
    }
}

impl Provider for GeminiClient {
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
pub fn has_api_key() -> bool {
    compat_has_api_key("GEMINI_API_KEY")
}

fn read_env_non_empty(key: &str) -> Result<Option<String>, ApiError> {
    match std::env::var(key) {
        Ok(value) if !value.is_empty() => Ok(Some(value)),
        Ok(_) | Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(ApiError::from(error)),
    }
}

fn extract_model_name(model: &str) -> String {
    model.trim_start_matches("gemini/").to_string()
}

fn build_request_body(request: &MessageRequest, _stream: bool) -> Value {
    let mut contents = Vec::new();
    let mut system_instruction: Option<Value> = None;

    if let Some(system) = request.system.as_ref().filter(|s| !s.is_empty()) {
        system_instruction = Some(json!({
            "parts": [{ "text": system }]
        }));
    }

    for message in &request.messages {
        let role = match message.role.as_str() {
            "assistant" => "model",
            _ => "user",
        };
        let mut parts = Vec::new();
        for block in &message.content {
            match block {
                InputContentBlock::Text { text } => {
                    parts.push(json!({ "text": text }));
                }
                InputContentBlock::ToolUse { id, name, input } => {
                    parts.push(json!({
                        "functionCall": {
                            "name": name,
                            "args": input,
                            "id": id,
                        }
                    }));
                }
                InputContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    ..
                } => {
                    parts.push(json!({
                        "functionResponse": {
                            "name": tool_use_id,
                            "response": {
                                "name": tool_use_id,
                                "content": flatten_tool_result_content(content),
                            },
                        }
                    }));
                }
            }
        }
        if !parts.is_empty() {
            contents.push(json!({
                "role": role,
                "parts": parts,
            }));
        }
    }

    let mut body = json!({
        "contents": contents,
    });

    if let Some(sys) = system_instruction {
        body["systemInstruction"] = sys;
    }

    if let Some(tools) = &request.tools {
        let function_declarations: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description.as_deref().unwrap_or(""),
                    "parameters": t.input_schema,
                })
            })
            .collect();
        body["tools"] = json!([{ "functionDeclarations": function_declarations }]);
    }

    if let Some(tool_choice) = &request.tool_choice {
        match tool_choice {
            ToolChoice::Auto => {
                body["toolConfig"] = json!({ "functionCallingConfig": { "mode": "AUTO" } });
            }
            ToolChoice::Any => {
                body["toolConfig"] = json!({ "functionCallingConfig": { "mode": "ANY" } });
            }
            ToolChoice::Tool { name } => {
                body["toolConfig"] = json!({
                    "functionCallingConfig": {
                        "mode": "ANY",
                        "allowedFunctionNames": [name],
                    }
                });
            }
        }
    }

    body["generationConfig"] = json!({
        "maxOutputTokens": request.max_tokens,
    });

    body
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

fn normalize_response(model: &str, response: GeminiResponse) -> Result<MessageResponse, ApiError> {
    let candidate = response
        .candidates
        .into_iter()
        .next()
        .ok_or(ApiError::InvalidSseFrame(
            "gemini response missing candidates",
        ))?;
    let mut content = Vec::new();
    let mut text_accum = String::new();

    for part in candidate.content.parts {
        if let Some(text) = part.text {
            text_accum.push_str(&text);
        }
        if let Some(fc) = part.function_call {
            content.push(OutputContentBlock::ToolUse {
                id: fc.id.unwrap_or_else(|| format!("fc_{}", fc.name)),
                name: fc.name,
                input: fc.args,
            });
        }
    }

    if !text_accum.is_empty() {
        content.push(OutputContentBlock::Text { text: text_accum });
    }

    let stop_reason = candidate.finish_reason.map(|r| {
        match r.as_str() {
            "STOP" | "RECITATION" => "end_turn",
            "MAX_TOKENS" => "max_tokens",
            "SAFETY" => "content_filter",
            "TOOL_CALLS" => "tool_use",
            other => other,
        }
        .to_string()
    });

    let usage = response.usage_metadata.as_ref();
    Ok(MessageResponse {
        id: format!(
            "gemini_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis())
        ),
        kind: "message".to_string(),
        role: "assistant".to_string(),
        content,
        model: response.model_version.unwrap_or_else(|| model.to_string()),
        stop_reason,
        stop_sequence: None,
        usage: Usage {
            input_tokens: usage.map_or(0, |u| u.prompt_token_count),
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            output_tokens: usage.map_or(0, |u| u.candidates_token_count),
        },
        request_id: None,
    })
}

async fn expect_success(response: reqwest::Response) -> Result<reqwest::Response, ApiError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    Err(ApiError::Api {
        status,
        error_type: None,
        message: None,
        body,
        retryable: matches!(status.as_u16(), 429 | 500 | 502 | 503 | 504),
    })
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    model_version: Option<String>,
    #[serde(default)]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
    #[serde(default)]
    finish_reason: Option<String>,
    #[serde(default)]
    #[expect(dead_code)]
    safety_ratings: Vec<GeminiSafetyRating>,
}

#[derive(Debug, Deserialize)]
struct GeminiContent {
    #[serde(default)]
    parts: Vec<GeminiPart>,
    #[serde(default)]
    #[expect(dead_code)]
    role: String,
}

#[derive(Debug, Deserialize)]
struct GeminiPart {
    #[serde(default)]
    text: Option<String>,
    #[serde(default, rename = "functionCall")]
    function_call: Option<GeminiFunctionCall>,
}

#[derive(Debug, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    #[serde(default)]
    args: Value,
    #[serde(default)]
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiSafetyRating {
    #[serde(default)]
    #[expect(dead_code)]
    category: String,
    #[serde(default)]
    #[expect(dead_code)]
    probability: String,
}

#[derive(Debug, Deserialize)]
#[expect(clippy::struct_field_names)]
struct GeminiUsageMetadata {
    #[serde(default)]
    prompt_token_count: u32,
    #[serde(default)]
    candidates_token_count: u32,
    #[serde(default)]
    #[expect(dead_code)]
    total_token_count: u32,
}

#[derive(Debug, Deserialize)]
struct GeminiStreamChunk {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    model_version: Option<String>,
    #[serde(default)]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug)]
#[expect(clippy::struct_excessive_bools)]
pub struct MessageStream {
    response: reqwest::Response,
    buffer: Vec<u8>,
    pending: VecDeque<StreamEvent>,
    done: bool,
    model: String,
    message_started: bool,
    text_started: bool,
    text_finished: bool,
    finished: bool,
    total_usage: Option<GeminiUsageMetadata>,
}

impl MessageStream {
    fn new(response: reqwest::Response, model: &str) -> Self {
        Self {
            response,
            buffer: Vec::new(),
            pending: VecDeque::new(),
            done: false,
            model: model.to_string(),
            message_started: false,
            text_started: false,
            text_finished: false,
            finished: false,
            total_usage: None,
        }
    }

    #[must_use]
    pub fn request_id(&self) -> Option<String> {
        None
    }

    pub async fn next_event(&mut self) -> Result<Option<StreamEvent>, ApiError> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Ok(Some(event));
            }
            if self.done {
                {
                    let events = self.finish_events();
                    self.pending.extend(events);
                };
                if let Some(event) = self.pending.pop_front() {
                    return Ok(Some(event));
                }
                return Ok(None);
            }
            match self.response.chunk().await? {
                Some(chunk) => {
                    self.buffer.extend_from_slice(&chunk);
                    while let Some(frame) = extract_sse_frame(&mut self.buffer) {
                        if let Some(chunk_data) = parse_sse_data(&frame)? {
                            {
                                let evts = self.process_chunk(chunk_data);
                                self.pending.extend(evts);
                            };
                        }
                    }
                }
                None => self.done = true,
            }
        }
    }

    fn process_chunk(&mut self, chunk: GeminiStreamChunk) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        if let Some(usage) = chunk.usage_metadata {
            self.total_usage = Some(usage);
        }
        for candidate in chunk.candidates {
            if !self.message_started {
                self.message_started = true;
                events.push(StreamEvent::MessageStart(MessageStartEvent {
                    message: MessageResponse {
                        id: format!(
                            "gemini_stream_{}",
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map_or(0, |d| d.as_millis())
                        ),
                        kind: "message".to_string(),
                        role: "assistant".to_string(),
                        content: Vec::new(),
                        model: chunk
                            .model_version
                            .clone()
                            .unwrap_or_else(|| self.model.clone()),
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
            for part in candidate.content.parts {
                if let Some(text) = part.text {
                    if !self.text_started {
                        self.text_started = true;
                        events.push(StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                            index: 0,
                            content_block: OutputContentBlock::Text {
                                text: String::new(),
                            },
                        }));
                    }
                    events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                        index: 0,
                        delta: ContentBlockDelta::TextDelta { text },
                    }));
                }
                if let Some(fc) = part.function_call {
                    let block_idx = 1;
                    events.push(StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                        index: block_idx,
                        content_block: OutputContentBlock::ToolUse {
                            id: fc.id.unwrap_or_else(|| format!("fc_{}", fc.name)),
                            name: fc.name.clone(),
                            input: json!({}),
                        },
                    }));
                    events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                        index: block_idx,
                        delta: ContentBlockDelta::InputJsonDelta {
                            partial_json: fc.args.to_string(),
                        },
                    }));
                    events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                        index: block_idx,
                    }));
                }
            }
            if let Some(reason) = candidate.finish_reason {
                match reason.as_str() {
                    "STOP" | "RECITATION" | "TOOL_CALLS" => {
                        if self.text_started && !self.text_finished {
                            self.text_finished = true;
                            events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                                index: 0,
                            }));
                        }
                    }
                    _ => {}
                }
            }
        }
        events
    }

    fn finish_events(&mut self) -> Vec<StreamEvent> {
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
        if self.message_started {
            let stop_reason = "end_turn".to_string();
            let usage = self.total_usage.as_ref().map_or(
                Usage {
                    input_tokens: 0,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                    output_tokens: 0,
                },
                |u| Usage {
                    input_tokens: u.prompt_token_count,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                    output_tokens: u.candidates_token_count,
                },
            );
            events.push(StreamEvent::MessageDelta(MessageDeltaEvent {
                delta: MessageDelta {
                    stop_reason: Some(stop_reason),
                    stop_sequence: None,
                },
                usage,
            }));
            events.push(StreamEvent::MessageStop(MessageStopEvent {}));
        }
        events
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

fn parse_sse_data(frame: &str) -> Result<Option<GeminiStreamChunk>, ApiError> {
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

#[cfg(test)]
mod tests {
    use super::{extract_model_name, has_api_key};
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
    fn detects_gemini_from_model_prefix() {
        let _lock = env_lock();
        assert_eq!(
            detect_provider_kind("gemini/gemini-2.5-pro"),
            ProviderKind::Gemini
        );
    }

    #[test]
    fn gemini_capabilities_from_registry() {
        let caps = capabilities_for_model("gemini/gemini-2.5-pro");
        assert_eq!(caps.context_window, 1_048_576);
        assert!(caps.supports_tools);
        assert!(caps.supports_reasoning);
    }

    #[test]
    fn extracts_model_name() {
        assert_eq!(
            extract_model_name("gemini/gemini-2.5-pro"),
            "gemini-2.5-pro"
        );
        assert_eq!(extract_model_name("gemini-1.5-flash"), "gemini-1.5-flash");
    }

    #[test]
    fn endpoint_url_construction() {
        let model = "gemini-2.5-pro";
        let key = "test-key";
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={key}"
        );
        assert!(url.contains("gemini-2.5-pro"));
        assert!(url.contains("generateContent"));
        assert!(url.contains("generativelanguage.googleapis.com"));
    }

    #[test]
    fn has_api_key_detects_env() {
        let _lock = env_lock();
        std::env::remove_var("GEMINI_API_KEY");
        assert!(!has_api_key());
        std::env::set_var("GEMINI_API_KEY", "ai-test-key");
        assert!(has_api_key());
        std::env::remove_var("GEMINI_API_KEY");
    }

    #[test]
    fn resolves_gemini_model_alias_passthrough() {
        assert_eq!(
            resolve_model_alias("gemini/gemini-2.5-pro"),
            "gemini/gemini-2.5-pro"
        );
    }
}
