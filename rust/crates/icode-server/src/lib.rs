pub mod routes;
pub mod sse;
pub mod state;
use utoipa::OpenApi;
#[derive(OpenApi)]
#[openapi(
paths(routes::health,routes::create_session,routes::list_sessions,routes::get_session,routes::send_message,routes::events,routes::read_file_handler,routes::get_config,routes::list_tasks,routes::create_task,routes::get_task,routes::stop_task,routes::list_teams,routes::create_team,routes::delete_team,routes::list_crons,routes::create_cron,routes::delete_cron,routes::list_mcp,routes::connect_mcp,routes::disconnect_mcp,routes::list_workers,routes::create_worker,routes::get_worker,routes::restart_worker,routes::list_lsp,routes::connect_lsp),
components(schemas(routes::schemas::HealthResponse,routes::schemas::SessionResponse,routes::schemas::FileReadResponse,routes::schemas::ErrorResponse,routes::schemas::EventResponse,routes::schemas::MessageRequest,routes::schemas::FileReadRequest,routes::schemas::TaskCreateRequest,routes::schemas::TeamCreateRequest,routes::schemas::CronCreateRequest,routes::schemas::WorkerCreateRequest,routes::schemas::LspConnectRequest)),
tags((name="Health"),(name="Sessions"),(name="Events"),(name="Files"),(name="Config"),(name="Tasks"),(name="Teams"),(name="Cron"),(name="MCP"),(name="Workers"),(name="LSP")),)]
pub struct ApiDoc;
