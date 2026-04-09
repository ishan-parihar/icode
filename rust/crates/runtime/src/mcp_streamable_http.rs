use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::Value as JsonValue;
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};

use crate::mcp_stdio::{
    JsonRpcError, JsonRpcId, JsonRpcRequest, McpInitializeParams, McpInitializeResult,
    McpListResourcesResult, McpReadResourceParams, McpReadResourceResult, McpToolCallParams,
    McpToolCallResult,
};

// ── Timeouts ────────────────────────────────────────────────────────────────

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

// ── Error types ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum McpStreamableHttpError {
    Http {
        status: u16,
        method: &'static str,
        details: String,
    },
    Network {
        method: &'static str,
        source: reqwest::Error,
    },
    JsonRpc {
        method: &'static str,
        error: JsonRpcError,
    },
    InvalidResponse {
        method: &'static str,
        details: String,
    },
    Timeout {
        method: &'static str,
        duration_ms: u64,
    },
    SessionExpired {
        method: &'static str,
    },
    AuthRequired {
        method: &'static str,
    },
}

impl fmt::Display for McpStreamableHttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http {
                status,
                method,
                details,
            } => {
                write!(
                    f,
                    "StreamableHTTP error {status} during {method}: {details}"
                )
            }
            Self::Network { method, source } => {
                write!(f, "StreamableHTTP network error during {method}: {source}")
            }
            Self::JsonRpc { method, error } => {
                write!(
                    f,
                    "StreamableHTTP JSON-RPC error during {method}: {} (code {})",
                    error.message, error.code
                )
            }
            Self::InvalidResponse { method, details } => {
                write!(
                    f,
                    "StreamableHTTP invalid response during {method}: {details}"
                )
            }
            Self::Timeout {
                method,
                duration_ms,
            } => {
                write!(
                    f,
                    "StreamableHTTP timed out after {duration_ms} ms during {method}"
                )
            }
            Self::SessionExpired { method } => {
                write!(
                    f,
                    "StreamableHTTP session expired during {method}: server terminated the session"
                )
            }
            Self::AuthRequired { method } => {
                write!(f, "StreamableHTTP authentication required during {method}")
            }
        }
    }
}

impl std::error::Error for McpStreamableHttpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Network { source, .. } => Some(source),
            Self::Http { .. }
            | Self::JsonRpc { .. }
            | Self::InvalidResponse { .. }
            | Self::Timeout { .. }
            | Self::SessionExpired { .. }
            | Self::AuthRequired { .. } => None,
        }
    }
}

// ── Transport ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct McpStreamableHttpTransport {
    url: String,
    headers: BTreeMap<String, String>,
    client: reqwest::Client,
    next_id: AtomicU64,
    session_id: Arc<RwLock<Option<String>>>,
}

impl McpStreamableHttpTransport {
    #[must_use]
    pub fn new(url: String, headers: BTreeMap<String, String>) -> Self {
        Self {
            url,
            headers,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("failed to build HTTP client"),
            next_id: AtomicU64::new(1),
            session_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Returns the current session ID if one has been established.
    pub async fn session_id(&self) -> Option<String> {
        self.session_id.read().await.clone()
    }

    fn next_request_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    async fn post_request(
        &self,
        method: &'static str,
        params: JsonValue,
        timeout_ms: u64,
    ) -> Result<JsonValue, McpStreamableHttpError> {
        let request_id = self.next_request_id();
        let request = JsonRpcRequest::new(JsonRpcId::Number(request_id), method, Some(params));

        let body = serde_json::to_string(&request).map_err(|e| {
            McpStreamableHttpError::InvalidResponse {
                method,
                details: format!("failed to serialize request: {e}"),
            }
        })?;

        // Build the POST request
        let mut req = self
            .client
            .post(&self.url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(
                reqwest::header::ACCEPT,
                "application/json, text/event-stream",
            );

        // Add custom headers
        for (key, value) in &self.headers {
            req = req.header(key.as_str(), value.as_str());
        }

        // Add session ID if we have one
        if let Some(session) = self.session_id.read().await.as_ref() {
            req = req.header("Mcp-Session-Id", session);
        }

        // Send with timeout
        let response = match timeout(Duration::from_millis(timeout_ms), req.body(body).send()).await
        {
            Ok(Ok(resp)) => resp,
            Ok(Err(source)) => {
                return Err(McpStreamableHttpError::Network { method, source });
            }
            Err(_) => {
                return Err(McpStreamableHttpError::Timeout {
                    method,
                    duration_ms: timeout_ms,
                });
            }
        };

        let status = response.status().as_u16();

        // Handle specific status codes
        match status {
            401 | 403 => {
                return Err(McpStreamableHttpError::AuthRequired { method });
            }
            404 => {
                // Session may have been terminated
                self.session_id.write().await.take();
                return Err(McpStreamableHttpError::SessionExpired { method });
            }
            _ if !response.status().is_success() => {
                let details = response.text().await.unwrap_or_default();
                return Err(McpStreamableHttpError::Http {
                    status,
                    method,
                    details,
                });
            }
            _ => {}
        }

        // Capture session ID from response header
        if let Some(new_session) = response
            .headers()
            .get("Mcp-Session-Id")
            .and_then(|v| v.to_str().ok())
        {
            self.session_id
                .write()
                .await
                .replace(new_session.to_string());
        }

        // Parse JSON response
        let json: JsonValue = response
            .json()
            .await
            .map_err(|source| McpStreamableHttpError::Network { method, source })?;

        // Check for JSON-RPC error
        if let Some(error) = json.get("error") {
            let json_rpc_error: JsonRpcError =
                serde_json::from_value(error.clone()).map_err(|e| {
                    McpStreamableHttpError::InvalidResponse {
                        method,
                        details: format!("failed to parse error: {e}"),
                    }
                })?;
            return Err(McpStreamableHttpError::JsonRpc {
                method,
                error: json_rpc_error,
            });
        }

        // Extract result
        json.get("result")
            .cloned()
            .ok_or_else(|| McpStreamableHttpError::InvalidResponse {
                method,
                details: "response missing both result and error".to_string(),
            })
    }

    /// Send a raw JSON-RPC request.
    pub async fn send_request(
        &self,
        method: &'static str,
        params: JsonValue,
    ) -> Result<JsonValue, McpStreamableHttpError> {
        let timeout_ms = match method {
            "initialize" => INIT_TIMEOUT_MS,
            "tools/call" => CALL_TOOL_TIMEOUT_MS,
            "resources/list" => LIST_RESOURCES_TIMEOUT_MS,
            "resources/read" => READ_RESOURCE_TIMEOUT_MS,
            _ => LIST_TOOLS_TIMEOUT_MS,
        };
        self.post_request(method, params, timeout_ms).await
    }

    pub async fn initialize(
        &self,
        params: McpInitializeParams,
    ) -> Result<McpInitializeResult, McpStreamableHttpError> {
        let json =
            serde_json::to_value(&params).map_err(|e| McpStreamableHttpError::InvalidResponse {
                method: "initialize",
                details: format!("failed to serialize params: {e}"),
            })?;
        let response = self
            .post_request("initialize", json, INIT_TIMEOUT_MS)
            .await?;
        serde_json::from_value(response).map_err(|e| McpStreamableHttpError::InvalidResponse {
            method: "initialize",
            details: format!("failed to deserialize result: {e}"),
        })
    }

    pub async fn list_tools(
        &self,
    ) -> Result<crate::mcp_stdio::McpListToolsResult, McpStreamableHttpError> {
        let params = crate::mcp_stdio::McpListToolsParams { cursor: None };
        let json =
            serde_json::to_value(&params).map_err(|e| McpStreamableHttpError::InvalidResponse {
                method: "tools/list",
                details: format!("failed to serialize params: {e}"),
            })?;
        let response = self
            .post_request("tools/list", json, LIST_TOOLS_TIMEOUT_MS)
            .await?;
        serde_json::from_value(response).map_err(|e| McpStreamableHttpError::InvalidResponse {
            method: "tools/list",
            details: format!("failed to deserialize result: {e}"),
        })
    }

    pub async fn call_tool(
        &self,
        params: McpToolCallParams,
    ) -> Result<McpToolCallResult, McpStreamableHttpError> {
        let json =
            serde_json::to_value(&params).map_err(|e| McpStreamableHttpError::InvalidResponse {
                method: "tools/call",
                details: format!("failed to serialize params: {e}"),
            })?;
        let response = self
            .post_request("tools/call", json, CALL_TOOL_TIMEOUT_MS)
            .await?;
        serde_json::from_value(response).map_err(|e| McpStreamableHttpError::InvalidResponse {
            method: "tools/call",
            details: format!("failed to deserialize result: {e}"),
        })
    }

    pub async fn list_resources(&self) -> Result<McpListResourcesResult, McpStreamableHttpError> {
        let params = crate::mcp_stdio::McpListResourcesParams { cursor: None };
        let json =
            serde_json::to_value(&params).map_err(|e| McpStreamableHttpError::InvalidResponse {
                method: "resources/list",
                details: format!("failed to serialize params: {e}"),
            })?;
        let response = self
            .post_request("resources/list", json, LIST_RESOURCES_TIMEOUT_MS)
            .await?;
        serde_json::from_value(response).map_err(|e| McpStreamableHttpError::InvalidResponse {
            method: "resources/list",
            details: format!("failed to deserialize result: {e}"),
        })
    }

    pub async fn read_resource(
        &self,
        params: McpReadResourceParams,
    ) -> Result<McpReadResourceResult, McpStreamableHttpError> {
        let json =
            serde_json::to_value(&params).map_err(|e| McpStreamableHttpError::InvalidResponse {
                method: "resources/read",
                details: format!("failed to serialize params: {e}"),
            })?;
        let response = self
            .post_request("resources/read", json, READ_RESOURCE_TIMEOUT_MS)
            .await?;
        serde_json::from_value(response).map_err(|e| McpStreamableHttpError::InvalidResponse {
            method: "resources/read",
            details: format!("failed to deserialize result: {e}"),
        })
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{McpStreamableHttpError, McpStreamableHttpTransport};
    use crate::mcp_stdio::{JsonRpcError, JsonRpcId, JsonRpcRequest};

    #[test]
    fn constructs_transport_with_empty_headers() {
        let transport = McpStreamableHttpTransport::new(
            "http://localhost:3000/mcp".to_string(),
            BTreeMap::new(),
        );
        assert_eq!(transport.url, "http://localhost:3000/mcp");
        assert!(transport.headers.is_empty());
    }

    #[test]
    fn constructs_transport_with_headers() {
        let mut headers = BTreeMap::new();
        headers.insert("Authorization".to_string(), "Bearer token".to_string());
        headers.insert("X-Custom".to_string(), "value".to_string());

        let transport = McpStreamableHttpTransport::new(
            "http://localhost:3000/mcp".to_string(),
            headers.clone(),
        );
        assert_eq!(transport.headers, headers);
    }

    #[test]
    fn generates_incrementing_request_ids() {
        let transport = McpStreamableHttpTransport::new(
            "http://localhost:3000/mcp".to_string(),
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
    fn session_id_is_none_initially() {
        let transport = McpStreamableHttpTransport::new(
            "http://localhost:3000/mcp".to_string(),
            BTreeMap::new(),
        );
        let rt = tokio::runtime::Runtime::new().unwrap();
        let session = rt.block_on(transport.session_id());
        assert!(session.is_none());
    }

    #[test]
    fn error_display_http() {
        let error = McpStreamableHttpError::Http {
            status: 500,
            method: "tools/list",
            details: "internal server error".to_string(),
        };
        let display = format!("{error}");
        assert!(display.contains("500"));
        assert!(display.contains("tools/list"));
        assert!(display.contains("internal server error"));
    }

    #[test]
    fn error_display_network() {
        let error = McpStreamableHttpError::Timeout {
            method: "initialize",
            duration_ms: 5000,
        };
        let display = format!("{error}");
        assert!(display.contains("initialize"));
    }

    #[test]
    fn error_display_jsonrpc() {
        let error = McpStreamableHttpError::JsonRpc {
            method: "tools/call",
            error: JsonRpcError {
                code: -32_601,
                message: "Method not found".to_string(),
                data: None,
            },
        };
        let display = format!("{error}");
        assert!(display.contains("tools/call"));
        assert!(display.contains("Method not found"));
        assert!(display.contains("-32601"));
    }

    #[test]
    fn error_display_invalid_response() {
        let error = McpStreamableHttpError::InvalidResponse {
            method: "resources/read",
            details: "missing contents field".to_string(),
        };
        let display = format!("{error}");
        assert!(display.contains("resources/read"));
        assert!(display.contains("missing contents field"));
    }

    #[test]
    fn error_display_timeout() {
        let error = McpStreamableHttpError::Timeout {
            method: "tools/call",
            duration_ms: 60_000,
        };
        let display = format!("{error}");
        assert!(display.contains("tools/call"));
        assert!(display.contains("60000"));
    }

    #[test]
    fn error_display_session_expired() {
        let error = McpStreamableHttpError::SessionExpired {
            method: "tools/list",
        };
        let display = format!("{error}");
        assert!(display.contains("tools/list"));
        assert!(display.contains("session expired"));
    }

    #[test]
    fn error_display_auth_required() {
        let error = McpStreamableHttpError::AuthRequired {
            method: "initialize",
        };
        let display = format!("{error}");
        assert!(display.contains("initialize"));
        assert!(display.contains("authentication required"));
    }

    #[test]
    fn json_rpc_request_serializes_correctly() {
        let request = JsonRpcRequest::new(
            JsonRpcId::Number(99),
            "initialize",
            Some(json!({
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "0.1.0" }
            })),
        );
        let serialized = serde_json::to_string(&request).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], 99);
        assert_eq!(parsed["method"], "initialize");
        assert_eq!(parsed["params"]["protocolVersion"], "2025-03-26");
    }

    #[test]
    fn json_rpc_request_with_null_params() {
        let request: JsonRpcRequest<serde_json::Value> =
            JsonRpcRequest::new(JsonRpcId::String("abc".to_string()), "ping", None);
        let serialized = serde_json::to_string(&request).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed["id"], "abc");
        assert_eq!(parsed["method"], "ping");
        assert!(parsed.get("params").is_none());
    }

    #[tokio::test]
    async fn session_id_tracks_value() {
        let transport = McpStreamableHttpTransport::new(
            "http://localhost:3000/mcp".to_string(),
            BTreeMap::new(),
        );

        // Initially no session
        assert!(transport.session_id().await.is_none());

        // Simulate setting a session ID (internal mechanism)
        transport
            .session_id
            .write()
            .await
            .replace("test-session-123".to_string());

        assert_eq!(
            transport.session_id().await,
            Some("test-session-123".to_string())
        );
    }
}
