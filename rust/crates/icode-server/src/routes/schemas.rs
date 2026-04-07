use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct SessionResponse {
    pub session_id: String,
    pub created_at: String,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct FileReadResponse {
    pub content: String,
    pub total_lines: usize,
    pub path: String,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct EventResponse {
    pub event: String,
    pub data: String,
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct MessageRequest {
    pub content: String,
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct FileReadRequest {
    pub path: String,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct TaskCreateRequest {
    pub prompt: String,
    pub description: Option<String>,
    pub task_packet: Option<serde_json::Value>,
    pub team_id: Option<String>,
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct TeamCreateRequest {
    pub name: String,
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct CronCreateRequest {
    pub schedule: String,
    pub prompt: String,
    pub description: Option<String>,
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct WorkerCreateRequest {
    pub cwd: String,
    pub trust_auto_resolve: Option<bool>,
    pub auto_recover_prompt_misdelivery: Option<bool>,
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct LspConnectRequest {
    pub root_path: Option<String>,
}
