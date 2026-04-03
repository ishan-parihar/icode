//! Bridge between MCP tool surface (ListMcpResources, ReadMcpResource, McpAuth, MCP)
//! and the existing McpServerManager runtime.
//!
//! Provides a stateful client registry that tool handlers can use to
//! connect to MCP servers and invoke their capabilities.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

/// Status of a managed MCP server connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    AuthRequired,
    Error,
}

impl std::fmt::Display for McpConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disconnected => write!(f, "disconnected"),
            Self::Connecting => write!(f, "connecting"),
            Self::Connected => write!(f, "connected"),
            Self::AuthRequired => write!(f, "auth_required"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Metadata about an MCP resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceInfo {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

/// Metadata about an MCP tool exposed by a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
}

/// Tracked state of an MCP server connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerState {
    pub server_name: String,
    pub status: McpConnectionStatus,
    pub tools: Vec<McpToolInfo>,
    pub resources: Vec<McpResourceInfo>,
    pub server_info: Option<String>,
    pub error_message: Option<String>,
}

/// Thread-safe registry of MCP server connections for tool dispatch.
#[derive(Debug, Clone, Default)]
pub struct McpToolRegistry {
    inner: Arc<Mutex<HashMap<String, McpServerState>>>,
}

impl McpToolRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or update an MCP server connection.
    pub fn register_server(
        &self,
        server_name: &str,
        status: McpConnectionStatus,
        tools: Vec<McpToolInfo>,
        resources: Vec<McpResourceInfo>,
        server_info: Option<String>,
    ) {
        let mut inner = self.inner.lock().expect("mcp registry lock poisoned");
        inner.insert(
            server_name.to_owned(),
            McpServerState {
                server_name: server_name.to_owned(),
                status,
                tools,
                resources,
                server_info,
                error_message: None,
            },
        );
    }

    /// Get current state of an MCP server.
    pub fn get_server(&self, server_name: &str) -> Option<McpServerState> {
        let inner = self.inner.lock().expect("mcp registry lock poisoned");
        inner.get(server_name).cloned()
    }

    /// List all registered MCP servers.
    pub fn list_servers(&self) -> Vec<McpServerState> {
        let inner = self.inner.lock().expect("mcp registry lock poisoned");
        inner.values().cloned().collect()
    }

    /// List resources from a specific server.
    pub fn list_resources(&self, server_name: &str) -> Result<Vec<McpResourceInfo>, String> {
        let inner = self.inner.lock().expect("mcp registry lock poisoned");
        match inner.get(server_name) {
            Some(state) => {
                if state.status != McpConnectionStatus::Connected {
                    return Err(format!(
                        "server '{}' is not connected (status: {})",
                        server_name, state.status
                    ));
                }
                Ok(state.resources.clone())
            }
            None => Err(format!("server '{}' not found", server_name)),
        }
    }

    /// Read a specific resource from a server.
    pub fn read_resource(&self, server_name: &str, uri: &str) -> Result<McpResourceInfo, String> {
        let inner = self.inner.lock().expect("mcp registry lock poisoned");
        let state = inner
            .get(server_name)
            .ok_or_else(|| format!("server '{}' not found", server_name))?;

        if state.status != McpConnectionStatus::Connected {
            return Err(format!(
                "server '{}' is not connected (status: {})",
                server_name, state.status
            ));
        }

        state
            .resources
            .iter()
            .find(|r| r.uri == uri)
            .cloned()
            .ok_or_else(|| format!("resource '{}' not found on server '{}'", uri, server_name))
    }

    /// List tools exposed by a specific server.
    pub fn list_tools(&self, server_name: &str) -> Result<Vec<McpToolInfo>, String> {
        let inner = self.inner.lock().expect("mcp registry lock poisoned");
        match inner.get(server_name) {
            Some(state) => {
                if state.status != McpConnectionStatus::Connected {
                    return Err(format!(
                        "server '{}' is not connected (status: {})",
                        server_name, state.status
                    ));
                }
                Ok(state.tools.clone())
            }
            None => Err(format!("server '{}' not found", server_name)),
        }
    }

    /// Call a tool on a specific server (returns placeholder for now;
    /// actual execution is handled by `McpServerManager::call_tool`).
    pub fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let inner = self.inner.lock().expect("mcp registry lock poisoned");
        let state = inner
            .get(server_name)
            .ok_or_else(|| format!("server '{}' not found", server_name))?;

        if state.status != McpConnectionStatus::Connected {
            return Err(format!(
                "server '{}' is not connected (status: {})",
                server_name, state.status
            ));
        }

        if !state.tools.iter().any(|t| t.name == tool_name) {
            return Err(format!(
                "tool '{}' not found on server '{}'",
                tool_name, server_name
            ));
        }

        // Return structured acknowledgment — actual execution is delegated
        // to the McpServerManager which handles the JSON-RPC call.
        Ok(serde_json::json!({
            "server": server_name,
            "tool": tool_name,
            "arguments": arguments,
            "status": "dispatched",
            "message": "Tool call dispatched to MCP server"
        }))
    }

    /// Set auth status for a server.
    pub fn set_auth_status(
        &self,
        server_name: &str,
        status: McpConnectionStatus,
    ) -> Result<(), String> {
        let mut inner = self.inner.lock().expect("mcp registry lock poisoned");
        let state = inner
            .get_mut(server_name)
            .ok_or_else(|| format!("server '{}' not found", server_name))?;
        state.status = status;
        Ok(())
    }

    /// Disconnect / remove a server.
    pub fn disconnect(&self, server_name: &str) -> Option<McpServerState> {
        let mut inner = self.inner.lock().expect("mcp registry lock poisoned");
        inner.remove(server_name)
    }

    /// Number of registered servers.
    #[must_use]
    pub fn len(&self) -> usize {
        let inner = self.inner.lock().expect("mcp registry lock poisoned");
        inner.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_and_retrieves_server() {
        let registry = McpToolRegistry::new();
        registry.register_server(
            "test-server",
            McpConnectionStatus::Connected,
            vec![McpToolInfo {
                name: "greet".into(),
                description: Some("Greet someone".into()),
                input_schema: None,
            }],
            vec![McpResourceInfo {
                uri: "res://data".into(),
                name: "Data".into(),
                description: None,
                mime_type: Some("application/json".into()),
            }],
            Some("TestServer v1.0".into()),
        );

        let server = registry.get_server("test-server").expect("should exist");
        assert_eq!(server.status, McpConnectionStatus::Connected);
        assert_eq!(server.tools.len(), 1);
        assert_eq!(server.resources.len(), 1);
    }

    #[test]
    fn lists_resources_from_connected_server() {
        let registry = McpToolRegistry::new();
        registry.register_server(
            "srv",
            McpConnectionStatus::Connected,
            vec![],
            vec![McpResourceInfo {
                uri: "res://alpha".into(),
                name: "Alpha".into(),
                description: None,
                mime_type: None,
            }],
            None,
        );

        let resources = registry.list_resources("srv").expect("should succeed");
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].uri, "res://alpha");
    }

    #[test]
    fn rejects_resource_listing_for_disconnected_server() {
        let registry = McpToolRegistry::new();
        registry.register_server(
            "srv",
            McpConnectionStatus::Disconnected,
            vec![],
            vec![],
            None,
        );
        assert!(registry.list_resources("srv").is_err());
    }

    #[test]
    fn reads_specific_resource() {
        let registry = McpToolRegistry::new();
        registry.register_server(
            "srv",
            McpConnectionStatus::Connected,
            vec![],
            vec![McpResourceInfo {
                uri: "res://data".into(),
                name: "Data".into(),
                description: Some("Test data".into()),
                mime_type: Some("text/plain".into()),
            }],
            None,
        );

        let resource = registry
            .read_resource("srv", "res://data")
            .expect("should find");
        assert_eq!(resource.name, "Data");

        assert!(registry.read_resource("srv", "res://missing").is_err());
    }

    #[test]
    fn calls_tool_on_connected_server() {
        let registry = McpToolRegistry::new();
        registry.register_server(
            "srv",
            McpConnectionStatus::Connected,
            vec![McpToolInfo {
                name: "greet".into(),
                description: None,
                input_schema: None,
            }],
            vec![],
            None,
        );

        let result = registry
            .call_tool("srv", "greet", &serde_json::json!({"name": "world"}))
            .expect("should dispatch");
        assert_eq!(result["status"], "dispatched");

        // Unknown tool should fail
        assert!(registry
            .call_tool("srv", "missing", &serde_json::json!({}))
            .is_err());
    }

    #[test]
    fn rejects_tool_call_on_disconnected_server() {
        let registry = McpToolRegistry::new();
        registry.register_server(
            "srv",
            McpConnectionStatus::AuthRequired,
            vec![McpToolInfo {
                name: "greet".into(),
                description: None,
                input_schema: None,
            }],
            vec![],
            None,
        );

        assert!(registry
            .call_tool("srv", "greet", &serde_json::json!({}))
            .is_err());
    }

    #[test]
    fn sets_auth_and_disconnects() {
        let registry = McpToolRegistry::new();
        registry.register_server(
            "srv",
            McpConnectionStatus::AuthRequired,
            vec![],
            vec![],
            None,
        );

        registry
            .set_auth_status("srv", McpConnectionStatus::Connected)
            .expect("should succeed");
        let state = registry.get_server("srv").unwrap();
        assert_eq!(state.status, McpConnectionStatus::Connected);

        let removed = registry.disconnect("srv");
        assert!(removed.is_some());
        assert!(registry.is_empty());
    }

    #[test]
    fn rejects_operations_on_missing_server() {
        let registry = McpToolRegistry::new();
        assert!(registry.list_resources("missing").is_err());
        assert!(registry.read_resource("missing", "uri").is_err());
        assert!(registry.list_tools("missing").is_err());
        assert!(registry
            .call_tool("missing", "tool", &serde_json::json!({}))
            .is_err());
        assert!(registry
            .set_auth_status("missing", McpConnectionStatus::Connected)
            .is_err());
    }
}
