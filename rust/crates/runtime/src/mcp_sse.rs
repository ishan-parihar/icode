use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value as JsonValue;
use tokio::sync::Mutex;

use crate::mcp_stdio::{
    JsonRpcError, JsonRpcId, JsonRpcRequest, McpInitializeParams, McpInitializeResult,
    McpListResourcesResult, McpReadResourceParams, McpReadResourceResult, McpToolCallParams,
    McpToolCallResult,
};
use crate::sse::IncrementalSseParser;

#[cfg(test)]
const INIT_TIMEOUT_MS: u64 = 200;
#[cfg(not(test))]
const INIT_TIMEOUT_MS: u64 = 10_000;

#[cfg(test)]
const LIST_TOOLS_TIMEOUT_MS: u64 = 300;
#[cfg(not(test))]
const LIST_TOOLS_TIMEOUT_MS: u64 = 30_000;

#[cfg(test)]
const CALL_TOOL_TIMEOUT_MS: u64 = 500;
#[cfg(not(test))]
const CALL_TOOL_TIMEOUT_MS: u64 = 60_000;

#[cfg(test)]
const LIST_RESOURCES_TIMEOUT_MS: u64 = 300;
#[cfg(not(test))]
const LIST_RESOURCES_TIMEOUT_MS: u64 = 30_000;

#[cfg(test)]
const READ_RESOURCE_TIMEOUT_MS: u64 = 300;
#[cfg(not(test))]
const READ_RESOURCE_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug)]
pub enum McpSseTransportError {
    Http {
        method: String,
        source: reqwest::Error,
    },
    JsonRpc {
        method: String,
        error: JsonRpcError,
    },
    InvalidResponse {
        method: String,
        details: String,
    },
    Timeout {
        method: String,
        duration_ms: u64,
    },
    SseConnection {
        details: String,
    },
    MissingEndpoint {
        details: String,
    },
}

impl fmt::Display for McpSseTransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http { method, source } => {
                write!(f, "SSE transport HTTP error during {method}: {source}")
            }
            Self::JsonRpc { method, error } => {
                write!(
                    f,
                    "SSE transport JSON-RPC error during {method}: {} (code {})",
                    error.message, error.code
                )
            }
            Self::InvalidResponse { method, details } => {
                write!(
                    f,
                    "SSE transport invalid response during {method}: {details}"
                )
            }
            Self::Timeout {
                method,
                duration_ms,
            } => {
                write!(
                    f,
                    "SSE transport timed out after {duration_ms} ms during {method}"
                )
            }
            Self::SseConnection { details } => {
                write!(f, "SSE connection error: {details}")
            }
            Self::MissingEndpoint { details } => {
                write!(f, "SSE missing endpoint: {details}")
            }
        }
    }
}

impl std::error::Error for McpSseTransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http { source, .. } => Some(source),
            Self::JsonRpc { .. }
            | Self::InvalidResponse { .. }
            | Self::Timeout { .. }
            | Self::SseConnection { .. }
            | Self::MissingEndpoint { .. } => None,
        }
    }
}

#[derive(Debug)]
pub struct McpSseTransport {
    base_url: String,
    headers: BTreeMap<String, String>,
    client: reqwest::Client,
    next_id: AtomicU64,
    endpoint_url: Mutex<Option<String>>,
    last_event_id: Mutex<Option<String>>,
}

impl McpSseTransport {
    #[must_use]
    pub fn new(base_url: String, headers: BTreeMap<String, String>) -> Self {
        Self {
            base_url,
            headers,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("failed to build HTTP client"),
            next_id: AtomicU64::new(1),
            endpoint_url: Mutex::new(None),
            last_event_id: Mutex::new(None),
        }
    }

    fn next_request_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    async fn discover_endpoint(&self) -> Result<String, McpSseTransportError> {
        {
            let guard = self.endpoint_url.lock().await;
            if let Some(ref url) = *guard {
                return Ok(url.clone());
            }
        }

        let mut request = self
            .client
            .get(&self.base_url)
            .header(reqwest::header::ACCEPT, "text/event-stream");

        for (key, value) in &self.headers {
            request = request.header(key.as_str(), value.as_str());
        }

        if let Some(ref last_id) = *self.last_event_id.lock().await {
            request = request.header("Last-Event-ID", last_id);
        }

        let response = request.send().await.map_err(|source| {
            McpSseTransportError::Http {
                method: "sse_connect".to_string(),
                source,
            }
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(McpSseTransportError::SseConnection {
                details: format!("SSE connection failed with status {status}: {body}"),
            });
        }

        let body = response.text().await.map_err(|source| {
            McpSseTransportError::Http {
                method: "sse_read".to_string(),
                source,
            }
        })?;

        let mut parser = IncrementalSseParser::new();
        let events = parser.push_chunk(&body);

        for event in events {
            if let Some(ref id) = event.id {
                *self.last_event_id.lock().await = Some(id.clone());
            }

            if event.event.as_deref() == Some("endpoint") {
                let endpoint_url = event.data.trim().to_string();
                if endpoint_url.is_empty() {
                    return Err(McpSseTransportError::MissingEndpoint {
                        details: "endpoint event contained empty URL".to_string(),
                    });
                }
                *self.endpoint_url.lock().await = Some(endpoint_url.clone());
                return Ok(endpoint_url);
            }
        }

        Err(McpSseTransportError::MissingEndpoint {
            details: "SSE response did not contain endpoint event".to_string(),
        })
    }

    async fn send_request_with_timeout(
        &self,
        method: &str,
        params: JsonValue,
        _timeout_ms: u64,
    ) -> Result<JsonValue, McpSseTransportError> {
        let endpoint_url = self.discover_endpoint().await?;

        let request_id = self.next_request_id();
        let request = JsonRpcRequest::new(JsonRpcId::Number(request_id), method, Some(params));

        let body = serde_json::to_string(&request).map_err(|e| {
            McpSseTransportError::InvalidResponse {
                method: method.to_string(),
                details: format!("failed to serialize request: {e}"),
            }
        })?;

        let mut post_request = self
            .client
            .post(&endpoint_url)
            .header(reqwest::header::CONTENT_TYPE, "application/json");

        for (key, value) in &self.headers {
            post_request = post_request.header(key.as_str(), value.as_str());
        }

        if let Some(ref last_id) = *self.last_event_id.lock().await {
            post_request = post_request.header("Last-Event-ID", last_id);
        }

        let response = post_request.body(body).send().await.map_err(|source| {
            McpSseTransportError::Http {
                method: method.to_string(),
                source,
            }
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let details = response.text().await.unwrap_or_default();
            return Err(McpSseTransportError::InvalidResponse {
                method: method.to_string(),
                details: format!("HTTP {status}: {details}"),
            });
        }

        let json: JsonValue = response.json().await.map_err(|source| {
            McpSseTransportError::Http {
                method: method.to_string(),
                source,
            }
        })?;

        if let Some(error) = json.get("error") {
            let json_rpc_error: JsonRpcError =
                serde_json::from_value(error.clone()).map_err(|e| {
                    McpSseTransportError::InvalidResponse {
                        method: method.to_string(),
                        details: format!("failed to parse error: {e}"),
                    }
                })?;
            return Err(McpSseTransportError::JsonRpc {
                method: method.to_string(),
                error: json_rpc_error,
            });
        }

        json.get("result")
            .cloned()
            .ok_or_else(|| McpSseTransportError::InvalidResponse {
                method: method.to_string(),
                details: "response missing both result and error".to_string(),
            })
    }

    pub async fn send_request(
        &self,
        method: &str,
        params: JsonValue,
    ) -> Result<JsonValue, McpSseTransportError> {
        let timeout_ms = match method {
            "initialize" => INIT_TIMEOUT_MS,
            "tools/call" => CALL_TOOL_TIMEOUT_MS,
            "resources/list" => LIST_RESOURCES_TIMEOUT_MS,
            "resources/read" => READ_RESOURCE_TIMEOUT_MS,
            _ => LIST_TOOLS_TIMEOUT_MS,
        };
        self.send_request_with_timeout(method, params, timeout_ms)
            .await
    }

    pub async fn initialize(
        &self,
        params: McpInitializeParams,
    ) -> Result<McpInitializeResult, McpSseTransportError> {
        let json = serde_json::to_value(&params).map_err(|e| {
            McpSseTransportError::InvalidResponse {
                method: "initialize".to_string(),
                details: format!("failed to serialize params: {e}"),
            }
        })?;
        let response = self
            .send_request_with_timeout("initialize", json, INIT_TIMEOUT_MS)
            .await?;
        serde_json::from_value(response).map_err(|e| {
            McpSseTransportError::InvalidResponse {
                method: "initialize".to_string(),
                details: format!("failed to deserialize result: {e}"),
            }
        })
    }

    pub async fn list_tools(
        &self,
    ) -> Result<crate::mcp_stdio::McpListToolsResult, McpSseTransportError> {
        let params = crate::mcp_stdio::McpListToolsParams { cursor: None };
        let json = serde_json::to_value(&params).map_err(|e| {
            McpSseTransportError::InvalidResponse {
                method: "tools/list".to_string(),
                details: format!("failed to serialize params: {e}"),
            }
        })?;
        let response = self
            .send_request_with_timeout("tools/list", json, LIST_TOOLS_TIMEOUT_MS)
            .await?;
        serde_json::from_value(response).map_err(|e| {
            McpSseTransportError::InvalidResponse {
                method: "tools/list".to_string(),
                details: format!("failed to deserialize result: {e}"),
            }
        })
    }

    pub async fn call_tool(
        &self,
        params: McpToolCallParams,
    ) -> Result<McpToolCallResult, McpSseTransportError> {
        let json = serde_json::to_value(&params).map_err(|e| {
            McpSseTransportError::InvalidResponse {
                method: "tools/call".to_string(),
                details: format!("failed to serialize params: {e}"),
            }
        })?;
        let response = self
            .send_request_with_timeout("tools/call", json, CALL_TOOL_TIMEOUT_MS)
            .await?;
        serde_json::from_value(response).map_err(|e| {
            McpSseTransportError::InvalidResponse {
                method: "tools/call".to_string(),
                details: format!("failed to deserialize result: {e}"),
            }
        })
    }

    pub async fn list_resources(&self) -> Result<McpListResourcesResult, McpSseTransportError> {
        let params = crate::mcp_stdio::McpListResourcesParams { cursor: None };
        let json = serde_json::to_value(&params).map_err(|e| {
            McpSseTransportError::InvalidResponse {
                method: "resources/list".to_string(),
                details: format!("failed to serialize params: {e}"),
            }
        })?;
        let response = self
            .send_request_with_timeout("resources/list", json, LIST_RESOURCES_TIMEOUT_MS)
            .await?;
        serde_json::from_value(response).map_err(|e| {
            McpSseTransportError::InvalidResponse {
                method: "resources/list".to_string(),
                details: format!("failed to deserialize result: {e}"),
            }
        })
    }

    pub async fn read_resource(
        &self,
        params: McpReadResourceParams,
    ) -> Result<McpReadResourceResult, McpSseTransportError> {
        let json = serde_json::to_value(&params).map_err(|e| {
            McpSseTransportError::InvalidResponse {
                method: "resources/read".to_string(),
                details: format!("failed to serialize params: {e}"),
            }
        })?;
        let response = self
            .send_request_with_timeout("resources/read", json, READ_RESOURCE_TIMEOUT_MS)
            .await?;
        serde_json::from_value(response).map_err(|e| {
            McpSseTransportError::InvalidResponse {
                method: "resources/read".to_string(),
                details: format!("failed to deserialize result: {e}"),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{McpSseTransport, McpSseTransportError};
    use crate::mcp_stdio::{JsonRpcError, JsonRpcId, JsonRpcRequest};

    #[test]
    fn constructs_transport_with_empty_headers() {
        let transport = McpSseTransport::new(
            "http://localhost:3000/sse".to_string(),
            BTreeMap::new(),
        );
        assert_eq!(transport.base_url, "http://localhost:3000/sse");
        assert!(transport.headers.is_empty());
    }

    #[test]
    fn constructs_transport_with_headers() {
        let mut headers = BTreeMap::new();
        headers.insert("Authorization".to_string(), "Bearer token".to_string());
        headers.insert("X-Custom".to_string(), "value".to_string());

        let transport =
            McpSseTransport::new("http://localhost:3000/sse".to_string(), headers.clone());
        assert_eq!(transport.headers, headers);
    }

    #[test]
    fn generates_incrementing_request_ids() {
        let transport = McpSseTransport::new(
            "http://localhost:3000/sse".to_string(),
            BTreeMap::new(),
        );
        let id1 = transport.next_request_id();
        let id2 = transport.next_request_id();
        let id3 = transport.next_request_id();
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
    }

    #[test]
    fn error_display_http() {
        let request = reqwest::Client::new()
            .get("http://invalid..url")
            .build()
            .unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let send_result = rt.block_on(reqwest::Client::new().execute(request));
        let source = send_result.unwrap_err();

        let error = McpSseTransportError::Http {
            method: "initialize".to_string(),
            source,
        };
        let display = format!("{error}");
        assert!(display.contains("initialize"));
        assert!(display.contains("HTTP error"));
    }

    #[test]
    fn error_display_jsonrpc() {
        let error = McpSseTransportError::JsonRpc {
            method: "tools/list".to_string(),
            error: JsonRpcError {
                code: -32_601,
                message: "Method not found".to_string(),
                data: None,
            },
        };
        let display = format!("{error}");
        assert!(display.contains("tools/list"));
        assert!(display.contains("Method not found"));
        assert!(display.contains("-32601"));
    }

    #[test]
    fn error_display_invalid_response() {
        let error = McpSseTransportError::InvalidResponse {
            method: "tools/call".to_string(),
            details: "missing result field".to_string(),
        };
        let display = format!("{error}");
        assert!(display.contains("tools/call"));
        assert!(display.contains("missing result field"));
    }

    #[test]
    fn error_display_timeout() {
        let error = McpSseTransportError::Timeout {
            method: "resources/list".to_string(),
            duration_ms: 30_000,
        };
        let display = format!("{error}");
        assert!(display.contains("resources/list"));
        assert!(display.contains("30000"));
    }

    #[test]
    fn error_display_sse_connection() {
        let error = McpSseTransportError::SseConnection {
            details: "connection refused".to_string(),
        };
        let display = format!("{error}");
        assert!(display.contains("connection refused"));
    }

    #[test]
    fn error_display_missing_endpoint() {
        let error = McpSseTransportError::MissingEndpoint {
            details: "no endpoint event received".to_string(),
        };
        let display = format!("{error}");
        assert!(display.contains("no endpoint event received"));
    }

    #[test]
    fn json_rpc_request_serializes_correctly() {
        let request = JsonRpcRequest::new(
            JsonRpcId::Number(42),
            "tools/list",
            Some(json!({"cursor": null})),
        );
        let serialized = serde_json::to_string(&request).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], 42);
        assert_eq!(parsed["method"], "tools/list");
        assert_eq!(parsed["params"]["cursor"], serde_json::Value::Null);
    }
}
