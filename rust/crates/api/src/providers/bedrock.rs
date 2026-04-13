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
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

use ::url::Url as UrlType;

pub const DEFAULT_REGION: &str = "us-east-1";
const SERVICE: &str = "bedrock";

#[derive(Debug, Clone)]
pub struct BedrockClient {
    http: reqwest::Client,
    access_key_id: String,
    secret_access_key: String,
    region: String,
}

impl BedrockClient {
    #[must_use]
    pub fn new(
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        region: impl Into<String>,
    ) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("failed to build HTTP client"),
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            region: region.into(),
        }
    }

    pub fn from_env() -> Result<Self, ApiError> {
        let access_key = read_env_non_empty("AWS_ACCESS_KEY_ID")?
            .ok_or_else(|| ApiError::missing_credentials("AWS Bedrock", &["AWS_ACCESS_KEY_ID"]))?;
        let secret_key = read_env_non_empty("AWS_SECRET_ACCESS_KEY")?.ok_or_else(|| {
            ApiError::missing_credentials("AWS Bedrock", &["AWS_SECRET_ACCESS_KEY"])
        })?;
        let region = read_region();
        Ok(Self::new(access_key, secret_key, region))
    }

    pub async fn send_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageResponse, ApiError> {
        let body = build_request_body(request, false);
        let model_id = extract_model_id(&request.model);
        let url = format!(
            "https://bedrock-runtime.{}/model/{}/converse",
            self.region, model_id,
        );
        let response = self.send_signed_request("POST", &url, &body).await?;
        let response = expect_success(response).await?;
        let payload = response.json::<BedrockConverseResponse>().await?;
        Ok(normalize_response(&request.model, payload))
    }

    pub async fn stream_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageStream, ApiError> {
        let body = build_request_body(request, true);
        let model_id = extract_model_id(&request.model);
        let url = format!(
            "https://bedrock-runtime.{}/model/{}/converse-stream",
            self.region, model_id,
        );
        let response = self.send_signed_request("POST", &url, &body).await?;
        let response = expect_success(response).await?;
        Ok(MessageStream::new(response, &request.model))
    }

    async fn send_signed_request(
        &self,
        method: &str,
        url: &str,
        body: &Value,
    ) -> Result<reqwest::Response, ApiError> {
        let body_bytes = serde_json::to_vec(body).map_err(ApiError::from)?;
        let now = SystemTime::now();
        let amz_date = format_amz_date(now);
        let date_stamp = amz_date[..8].to_string();
        let credential_scope = format!("{date_stamp}/{}/{SERVICE}/aws4_request", self.region);

        let payload_hash = sha256_hex(&body_bytes);
        let canonical_request = build_canonical_request(method, url, &payload_hash, &amz_date);
        let string_to_sign = build_string_to_sign(&canonical_request, &amz_date, &credential_scope);
        let signing_key = derive_signing_key(&self.secret_access_key, &date_stamp, &self.region);
        let signature = hmac_sha256_hex(&signing_key, string_to_sign.as_bytes());

        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders=content-type;host;x-amz-date, Signature={signature}",
            self.access_key_id,
        );

        self.http
            .post(url)
            .header("content-type", "application/json")
            .header("x-amz-date", amz_date)
            .header("Authorization", authorization)
            .body(body_bytes)
            .send()
            .await
            .map_err(ApiError::from)
    }
}

impl Provider for BedrockClient {
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
pub fn read_region() -> String {
    read_env_non_empty("AWS_DEFAULT_REGION")
        .ok()
        .flatten()
        .or_else(|| read_env_non_empty("AWS_REGION").ok().flatten())
        .unwrap_or_else(|| DEFAULT_REGION.to_string())
}

#[must_use]
pub fn has_api_key() -> bool {
    compat_has_api_key("AWS_ACCESS_KEY_ID")
}

fn read_env_non_empty(key: &str) -> Result<Option<String>, ApiError> {
    match std::env::var(key) {
        Ok(value) if !value.is_empty() => Ok(Some(value)),
        Ok(_) | Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(ApiError::from(error)),
    }
}

fn extract_model_id(model: &str) -> String {
    model.trim_start_matches("bedrock/").to_string()
}

fn format_amz_date(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (year, month, day, hour, min, sec) = unix_ts_to_ymd_hms(secs);
    format!("{year:04}{month:02}{day:02}T{hour:02}{min:02}{sec:02}Z")
}

#[expect(clippy::cast_sign_loss)]
#[expect(clippy::cast_possible_truncation)]
fn unix_ts_to_ymd_hms(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    const DAYS_FROM_1970_TO_2000: u64 = 10957;
    const SECS_PER_MINUTE: u64 = 60;
    const SECS_PER_HOUR: u64 = 3600;
    const SECS_PER_DAY: u64 = 86400;

    let total_secs = secs;
    let day_num = total_secs / SECS_PER_DAY;
    let rem_secs = total_secs % SECS_PER_DAY;
    let hour = (rem_secs / SECS_PER_HOUR) as u32;
    let minute = ((rem_secs % SECS_PER_HOUR) / SECS_PER_MINUTE) as u32;
    let second = (rem_secs % SECS_PER_MINUTE) as u32;

    let days_since_2000 = day_num.saturating_sub(DAYS_FROM_1970_TO_2000);
    let mut year = 2000_u64;
    let mut remaining = days_since_2000;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }
    let month_days = [
        if is_leap_year(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1_u32;
    for &days in &month_days {
        if remaining < days as u64 {
            break;
        }
        remaining -= days as u64;
        month += 1;
    }
    let day = remaining as u32 + 1;
    (year as u32, month, day, hour, minute, second)
}

const fn is_leap_year(year: u64) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

fn build_canonical_request(method: &str, url: &str, payload_hash: &str, amz_date: &str) -> String {
    let parsed = url.parse::<UrlType>().expect("valid url");
    let host = parsed.host_str().expect("host in url");
    let path = parsed.path();
    format!(
        "{method}\n{path}\n\ncontent-type:application/json\nhost:{host}\nx-amz-date:{amz_date}\n\ncontent-type;host;x-amz-date\n{payload_hash}"
    )
}

fn build_string_to_sign(canonical_request: &str, amz_date: &str, credential_scope: &str) -> String {
    let hashed = sha256_hex(canonical_request.as_bytes());
    format!("AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{hashed}")
}

fn derive_signing_key(secret: &str, date_stamp: &str, region: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, SERVICE.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex::encode(result)
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::Mac;
    let mut mac =
        hmac::Hmac::<sha2::Sha256>::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn hmac_sha256_hex(key: &[u8], data: &[u8]) -> String {
    hex::encode(hmac_sha256(key, data))
}

fn build_request_body(request: &MessageRequest, _stream: bool) -> Value {
    let mut messages = Vec::new();
    let mut system_prompt: Option<Vec<Value>> = None;

    if let Some(system) = request.system.as_ref().filter(|s| !s.is_empty()) {
        system_prompt = Some(vec![json!({ "text": system })]);
    }

    for message in &request.messages {
        let role = match message.role.as_str() {
            "assistant" => "assistant",
            _ => "user",
        };
        let mut content = Vec::new();
        for block in &message.content {
            match block {
                InputContentBlock::Text { text } => {
                    content.push(json!({ "text": text }));
                }
                InputContentBlock::ToolUse { id, name, input } => {
                    let input_obj = match input {
                        Value::Object(map) => map.clone(),
                        other => {
                            let mut m = Map::new();
                            m.insert("value".to_string(), other.clone());
                            m
                        }
                    };
                    content.push(json!({
                        "toolUse": {
                            "toolUseId": id,
                            "name": name,
                            "input": input_obj,
                        }
                    }));
                }
                InputContentBlock::ToolResult {
                    tool_use_id,
                    content: result_content,
                    is_error,
                } => {
                    let text_content = flatten_tool_result_content(result_content);
                    let status = if *is_error { "error" } else { "success" };
                    content.push(json!({
                        "toolResult": {
                            "toolUseId": tool_use_id,
                            "content": [{ "text": text_content }],
                            "status": status,
                        }
                    }));
                }
            }
        }
        if !content.is_empty() {
            messages.push(json!({ "role": role, "content": content }));
        }
    }

    let mut inference_config = Map::new();
    inference_config.insert("maxTokens".to_string(), Value::from(request.max_tokens));

    let mut body = json!({
        "messages": messages,
        "inferenceConfig": inference_config,
    });

    if let Some(sys) = system_prompt {
        body["system"] = Value::Array(sys);
    }

    if let Some(tools) = &request.tools {
        let tool_configs: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "toolSpec": {
                        "name": t.name,
                        "description": t.description.as_deref().unwrap_or(""),
                        "inputSchema": { "json": t.input_schema },
                    }
                })
            })
            .collect();
        body["toolConfig"] = json!({ "tools": tool_configs });
    }

    if let Some(tool_choice) = &request.tool_choice {
        let existing_tool_config = body.get_mut("toolConfig");
        if let Some(tool_config) = existing_tool_config {
            match tool_choice {
                ToolChoice::Auto => {
                    tool_config["toolChoice"] = json!({ "auto": {} });
                }
                ToolChoice::Any => {
                    tool_config["toolChoice"] = json!({ "any": {} });
                }
                ToolChoice::Tool { name } => {
                    tool_config["toolChoice"] = json!({
                        "tool": { "name": name }
                    });
                }
            }
        }
    }

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

fn normalize_response(model: &str, response: BedrockConverseResponse) -> MessageResponse {
    let mut content = Vec::new();
    for block in response.output.message.content {
        if let Some(text) = block.text {
            content.push(OutputContentBlock::Text { text });
        }
        if let Some(tool_use) = block.tool_use {
            content.push(OutputContentBlock::ToolUse {
                id: tool_use.tool_use_id,
                name: tool_use.name,
                input: tool_use.input,
            });
        }
    }
    let stop_reason = response
        .stop_reason
        .unwrap_or_else(|| "end_turn".to_string());
    let usage = response.usage;
    MessageResponse {
        id: format!(
            "bedrock_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_millis())
        ),
        kind: "message".to_string(),
        role: "assistant".to_string(),
        content,
        model: model.to_string(),
        stop_reason: Some(normalize_stop_reason(&stop_reason)),
        stop_sequence: None,
        usage: Usage {
            input_tokens: usage.input_tokens,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            output_tokens: usage.output_tokens,
        },
        request_id: None,
    }
}

fn normalize_stop_reason(reason: &str) -> String {
    match reason {
        "max_tokens" => "max_tokens",
        "end_turn" | "stop_sequence" => "end_turn",
        "tool_use" => "tool_use",
        "content_filtered" => "content_filter",
        other => other,
    }
    .to_string()
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
struct BedrockConverseResponse {
    #[serde(default)]
    output: BedrockOutput,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    usage: BedrockUsage,
}

#[derive(Debug, Default, Deserialize)]
struct BedrockOutput {
    #[serde(default)]
    message: BedrockMessage,
}

#[derive(Debug, Default, Deserialize)]
struct BedrockMessage {
    #[serde(default)]
    #[expect(dead_code)]
    role: String,
    #[serde(default)]
    content: Vec<BedrockContentBlock>,
}

#[derive(Debug, Deserialize)]
struct BedrockContentBlock {
    #[serde(default)]
    text: Option<String>,
    #[serde(default, rename = "toolUse")]
    tool_use: Option<BedrockToolUse>,
}

#[derive(Debug, Deserialize)]
struct BedrockToolUse {
    #[serde(rename = "toolUseId")]
    tool_use_id: String,
    name: String,
    #[serde(default)]
    input: Value,
}

#[derive(Debug, Default, Deserialize)]
struct BedrockUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
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
    usage: Option<BedrockUsage>,
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
            usage: None,
        }
    }

    #[must_use]
    pub fn request_id(&self) -> Option<String> {
        self.response
            .headers()
            .get("x-amzn-requestid")
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned)
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
                        if let Some(events) = self.parse_stream_event(&frame)? {
                            self.pending.extend(events);
                        }
                    }
                }
                None => self.done = true,
            }
        }
    }

    fn parse_stream_event(&mut self, frame: &str) -> Result<Option<Vec<StreamEvent>>, ApiError> {
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
        let chunk: BedrockStreamChunk = serde_json::from_str(&payload)?;
        Ok(self.process_stream_chunk(chunk))
    }

    #[expect(clippy::too_many_lines)]
    fn process_stream_chunk(&mut self, chunk: BedrockStreamChunk) -> Option<Vec<StreamEvent>> {
        if let Some(metadata) = chunk.metadata {
            self.usage = Some(BedrockUsage {
                input_tokens: metadata.input_token_count,
                output_tokens: metadata.output_token_count,
            });
        }
        if let Some(cm) = chunk.content_block_start {
            if !self.message_started {
                self.message_started = true;
                let mut events = vec![StreamEvent::MessageStart(MessageStartEvent {
                    message: MessageResponse {
                        id: format!(
                            "bedrock_stream_{}",
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .map_or(0, |d| d.as_millis())
                        ),
                        kind: "message".to_string(),
                        role: "assistant".to_string(),
                        content: Vec::new(),
                        model: self.model.clone(),
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
                })];
                if let Some(tool_use) = cm.start.tool_use {
                    events.push(StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                        index: cm.content_block_index,
                        content_block: OutputContentBlock::ToolUse {
                            id: tool_use.tool_use_id,
                            name: tool_use.name,
                            input: json!({}),
                        },
                    }));
                } else {
                    self.text_started = true;
                    events.push(StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                        index: cm.content_block_index,
                        content_block: OutputContentBlock::Text {
                            text: String::new(),
                        },
                    }));
                }
                return Some(events);
            }
            if let Some(tool_use) = cm.start.tool_use {
                return Some(vec![StreamEvent::ContentBlockStart(
                    ContentBlockStartEvent {
                        index: cm.content_block_index,
                        content_block: OutputContentBlock::ToolUse {
                            id: tool_use.tool_use_id,
                            name: tool_use.name,
                            input: json!({}),
                        },
                    },
                )]);
            }
        }
        if let Some(delta) = chunk.content_block_delta {
            if let Some(text) = delta.delta.text {
                return Some(vec![StreamEvent::ContentBlockDelta(
                    ContentBlockDeltaEvent {
                        index: delta.content_block_index,
                        delta: ContentBlockDelta::TextDelta { text },
                    },
                )]);
            }
            if let Some(input) = delta.delta.input_json {
                return Some(vec![StreamEvent::ContentBlockDelta(
                    ContentBlockDeltaEvent {
                        index: delta.content_block_index,
                        delta: ContentBlockDelta::InputJsonDelta {
                            partial_json: input,
                        },
                    },
                )]);
            }
        }
        if let Some(stop) = chunk.content_block_stop {
            let idx = stop.content_block_index;
            if idx == 0 {
                self.text_finished = true;
            }
            return Some(vec![StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                index: idx,
            })]);
        }
        if let Some(msg_stop) = chunk.message_stop {
            if self.text_started && !self.text_finished {
                self.text_finished = true;
            }
            let stop_reason = msg_stop
                .stop_reason
                .unwrap_or_else(|| "end_turn".to_string());
            let usage = self.usage.as_ref().map_or(
                Usage {
                    input_tokens: 0,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                    output_tokens: 0,
                },
                |u| Usage {
                    input_tokens: u.input_tokens,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                    output_tokens: u.output_tokens,
                },
            );
            let events = vec![
                StreamEvent::MessageDelta(MessageDeltaEvent {
                    delta: MessageDelta {
                        stop_reason: Some(normalize_stop_reason(&stop_reason)),
                        stop_sequence: None,
                    },
                    usage,
                }),
                StreamEvent::MessageStop(MessageStopEvent {}),
            ];
            return Some(events);
        }
        None
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
            let usage = self.usage.as_ref().map_or(
                Usage {
                    input_tokens: 0,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                    output_tokens: 0,
                },
                |u| Usage {
                    input_tokens: u.input_tokens,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                    output_tokens: u.output_tokens,
                },
            );
            events.push(StreamEvent::MessageDelta(MessageDeltaEvent {
                delta: MessageDelta {
                    stop_reason: Some("end_turn".to_string()),
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

#[derive(Debug, Deserialize)]
struct BedrockStreamChunk {
    #[serde(default, rename = "contentBlockStart")]
    content_block_start: Option<BedrockContentBlockStart>,
    #[serde(default, rename = "contentBlockDelta")]
    content_block_delta: Option<BedrockContentBlockDelta>,
    #[serde(default, rename = "contentBlockStop")]
    content_block_stop: Option<BedrockContentBlockStop>,
    #[serde(default, rename = "messageStop")]
    message_stop: Option<BedrockMessageStop>,
    #[serde(default)]
    metadata: Option<BedrockStreamMetadata>,
}

#[derive(Debug, Deserialize)]
struct BedrockContentBlockStart {
    #[serde(rename = "contentBlockIndex")]
    content_block_index: u32,
    start: BedrockBlockStart,
}

#[derive(Debug, Deserialize)]
struct BedrockBlockStart {
    #[serde(default, rename = "toolUse")]
    tool_use: Option<BedrockStreamToolUse>,
}

#[derive(Debug, Deserialize)]
struct BedrockStreamToolUse {
    #[serde(rename = "toolUseId")]
    tool_use_id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct BedrockContentBlockDelta {
    #[serde(rename = "contentBlockIndex")]
    content_block_index: u32,
    delta: BedrockBlockDelta,
}

#[derive(Debug, Deserialize)]
struct BedrockBlockDelta {
    #[serde(default)]
    text: Option<String>,
    #[serde(default, rename = "inputJson")]
    input_json: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BedrockContentBlockStop {
    #[serde(rename = "contentBlockIndex")]
    #[allow(dead_code)]
    content_block_index: u32,
}

#[derive(Debug, Deserialize)]
struct BedrockMessageStop {
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BedrockStreamMetadata {
    #[serde(rename = "inputTokenCount")]
    input_token_count: u32,
    #[serde(rename = "outputTokenCount")]
    output_token_count: u32,
}

#[cfg(test)]
mod tests {
    use super::{extract_model_id, has_api_key, read_region, DEFAULT_REGION};
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
    fn detects_bedrock_from_model_prefix() {
        let _lock = env_lock();
        assert_eq!(
            detect_provider_kind("bedrock/anthropic.claude-3-5-sonnet-20241022-v2:0", None),
            ProviderKind::Bedrock
        );
    }

    #[test]
    fn bedrock_capabilities_from_registry() {
        let caps = capabilities_for_model("bedrock/anthropic.claude-3-5-sonnet-20241022-v2:0");
        assert_eq!(caps.context_window, 200_000);
        assert!(caps.supports_tools);
    }

    #[test]
    fn read_region_returns_default() {
        let _lock = env_lock();
        std::env::remove_var("AWS_DEFAULT_REGION");
        std::env::remove_var("AWS_REGION");
        assert_eq!(read_region(), DEFAULT_REGION);
    }

    #[test]
    fn extracts_model_id() {
        assert_eq!(
            extract_model_id("bedrock/anthropic.claude-3-5-sonnet-20241022-v2:0"),
            "anthropic.claude-3-5-sonnet-20241022-v2:0"
        );
    }

    #[test]
    fn endpoint_url_construction() {
        let region = "us-west-2";
        let model = "anthropic.claude-3-5-sonnet-20241022-v2:0";
        let url = format!("https://bedrock-runtime.{region}/model/{model}/converse");
        assert!(url.contains("bedrock-runtime.us-west-2"));
        assert!(url.contains("/converse"));
    }

    #[test]
    fn has_api_key_detects_env() {
        let _lock = env_lock();
        std::env::remove_var("AWS_ACCESS_KEY_ID");
        assert!(!has_api_key());
        std::env::set_var("AWS_ACCESS_KEY_ID", "AKIAIOSFODNN7EXAMPLE");
        assert!(has_api_key());
        std::env::remove_var("AWS_ACCESS_KEY_ID");
    }

    #[test]
    fn resolves_bedrock_model_alias_passthrough() {
        assert_eq!(
            resolve_model_alias("bedrock/anthropic.claude-3-5-sonnet-20241022-v2:0"),
            "bedrock/anthropic.claude-3-5-sonnet-20241022-v2:0"
        );
    }
}
