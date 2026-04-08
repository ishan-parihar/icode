//! `OpenAPI` schema types for `utoipa` documentation.
//!
//! These types exist solely for API documentation purposes and derive
//! `utoipa::ToSchema` so that the generated `OpenAPI` spec includes proper
//! request/response schemas.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ── Responses ─────────────────────────────────────────────────────────

/// Health check response.
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    /// Server status indicator.
    pub status: String,
    /// Server version from Cargo.toml.
    pub version: String,
}

/// Session creation / listing response item.
#[derive(Debug, Serialize, ToSchema)]
pub struct SessionResponse {
    /// Unique session identifier (UUID v4).
    pub session_id: String,
    /// ISO-8601 timestamp of session creation.
    pub created_at: String,
}

/// File read response.
#[derive(Debug, Serialize, ToSchema)]
pub struct FileReadResponse {
    /// File contents as a string.
    pub content: String,
    /// Total number of lines in the file.
    pub total_lines: usize,
    /// Path of the file that was read.
    pub path: String,
}

/// Generic error response.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    /// Human-readable error description.
    pub error: String,
}

/// SSE event response wrapper (for documentation purposes).
#[derive(Debug, Serialize, ToSchema)]
pub struct EventResponse {
    /// Event type identifier.
    pub event: String,
    /// Event payload as a JSON string.
    pub data: String,
}

// ── Requests ──────────────────────────────────────────────────────────

/// Message to send to a session.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MessageRequest {
    /// The message content to send.
    pub content: String,
}

/// File read request.
#[derive(Debug, Deserialize, ToSchema)]
pub struct FileReadRequest {
    /// Path to the file to read.
    pub path: String,
    /// Optional start line (1-indexed).
    pub start_line: Option<usize>,
    /// Optional end line (1-indexed, inclusive).
    pub end_line: Option<usize>,
}

// ── Task types ────────────────────────────────────────────────────────

/// Task creation request.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TaskCreateRequest {
    /// Task prompt/instructions.
    pub prompt: String,
    /// Optional task description.
    pub description: Option<String>,
    /// Optional task packet as JSON.
    pub task_packet: Option<serde_json::Value>,
    /// Optional team ID to associate with.
    pub team_id: Option<String>,
}

// ── Team types ────────────────────────────────────────────────────────

/// Team creation request.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TeamCreateRequest {
    /// Team name.
    pub name: String,
}

// ── Cron types ────────────────────────────────────────────────────────

/// Cron creation request.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CronCreateRequest {
    /// Cron schedule expression.
    pub schedule: String,
    /// Prompt to execute on schedule.
    pub prompt: String,
    /// Optional description.
    pub description: Option<String>,
}

// ── Worker types ──────────────────────────────────────────────────────

/// Worker creation request.
#[derive(Debug, Deserialize, ToSchema)]
pub struct WorkerCreateRequest {
    /// Working directory for the worker.
    pub cwd: String,
    /// Whether to auto-resolve trust prompts.
    pub trust_auto_resolve: Option<bool>,
    /// Whether to auto-recover from prompt misdelivery.
    pub auto_recover_prompt_misdelivery: Option<bool>,
}

// ── LSP types ─────────────────────────────────────────────────────────

/// LSP connect request.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LspConnectRequest {
    /// Optional root path for the LSP server.
    pub root_path: Option<String>,
}
