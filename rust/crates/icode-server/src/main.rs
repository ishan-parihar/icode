use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::routing::{delete, get, post};
use axum::Json;
use axum::Router;
use icode_server::state::ServerState;
use icode_server::ApiDoc;
use icode_server::{
    connect_lsp, connect_mcp, create_cron, create_session, create_task, create_team, create_worker,
    delete_cron, delete_team, disconnect_mcp, events, get_config, get_session, get_task,
    get_worker, health, list_crons, list_lsp, list_mcp, list_sessions, list_tasks, list_teams,
    list_workers, read_file_handler, restart_worker, send_message, stop_task,
};
use runtime::{ConfigLoader, EventBus, SqliteStore};
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[tokio::main]
async fn main() {
    let args = parse_args();

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let event_bus = EventBus::default();
    let db_path = args.db_path.unwrap_or_else(SqliteStore::default_path);
    let store = SqliteStore::new(&db_path).expect("sqlite store should open");
    store.migrate().expect("migration should succeed");

    let config_loader = ConfigLoader::default_for(&args.cwd);

    let state = Arc::new(ServerState::new(event_bus, store, config_loader));

    let app = create_router(state);

    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .expect("valid socket address");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("server should bind");

    tracing::info!("icode-server listening on {addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server should run");
}

fn create_router(state: Arc<ServerState>) -> Router {
    Router::new()
        // Health & Config
        .route("/health", get(health))
        .route("/config", get(get_config))
        // Sessions
        .route("/sessions", post(create_session))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{id}", get(get_session))
        .route("/sessions/{id}/message", post(send_message))
        // Events
        .route("/events", get(events))
        // Files
        .route("/files/read", post(read_file_handler))
        // Tasks
        .route("/tasks", get(list_tasks))
        .route("/tasks", post(create_task))
        .route("/tasks/{id}", get(get_task))
        .route("/tasks/{id}/stop", post(stop_task))
        // Teams
        .route("/teams", get(list_teams))
        .route("/teams", post(create_team))
        .route("/teams/{id}", delete(delete_team))
        // Cron
        .route("/cron", get(list_crons))
        .route("/cron", post(create_cron))
        .route("/cron/{id}", delete(delete_cron))
        // MCP
        .route("/mcp", get(list_mcp))
        .route("/mcp/{name}/connect", post(connect_mcp))
        .route("/mcp/{name}/disconnect", post(disconnect_mcp))
        // Workers
        .route("/workers", get(list_workers))
        .route("/workers", post(create_worker))
        .route("/workers/{id}", get(get_worker))
        .route("/workers/{id}/restart", post(restart_worker))
        // LSP
        .route("/lsp", get(list_lsp))
        .route("/lsp/{language}/connect", post(connect_lsp))
        // Swagger & OpenAPI
        .merge(SwaggerUi::new("/swagger-ui").url("/api/openapi.json", ApiDoc::openapi()))
        .route(
            "/api/openapi.json",
            get(|| async { Json(ApiDoc::openapi()) }),
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received");
}

// ── CLI args ──────────────────────────────────────────────────────────

struct ServerArgs {
    host: String,
    port: u16,
    cwd: PathBuf,
    db_path: Option<PathBuf>,
}

fn parse_args() -> ServerArgs {
    let args: Vec<String> = std::env::args().collect();
    let mut host = "127.0.0.1".to_string();
    let mut port: u16 = 3000;
    let mut cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut db_path: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--host" => {
                i += 1;
                if i < args.len() {
                    host = args[i].clone();
                }
            }
            "--port" => {
                i += 1;
                if i < args.len() {
                    port = args[i].parse().unwrap_or(3000);
                }
            }
            "--cwd" => {
                i += 1;
                if i < args.len() {
                    cwd = PathBuf::from(&args[i]);
                }
            }
            "--db" => {
                i += 1;
                if i < args.len() {
                    db_path = Some(PathBuf::from(&args[i]));
                }
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }

    ServerArgs {
        host,
        port,
        cwd,
        db_path,
    }
}

fn print_help() {
    println!(
        "icode-server: HTTP API for icode AI assistant

Usage: icode-server [OPTIONS]

Options:
  --host <HOST>    Bind address (default: 127.0.0.1)
  --port <PORT>    Port number (default: 3000)
  --cwd <DIR>      Working directory (default: current)
  --db <PATH>      SQLite database path (default: ~/.icode/icode.db)
  --help, -h       Show this help message

Endpoints:
  GET  /health                  Health check
  POST /sessions                Create a new session
  GET  /sessions                List all sessions
  GET  /sessions/:id            Get session details
  POST /sessions/:id/message    Send a message (SSE stream)
  GET  /events                  SSE event stream
  POST /files/read              Read a file
  GET  /config                  Get current config
  GET  /tasks                   List all tasks
  POST /tasks                   Create a new task
  GET  /tasks/:id               Get task details
  POST /tasks/:id/stop          Stop a task
  GET  /teams                   List all teams
  POST /teams                   Create a new team
  DELETE /teams/:id             Delete a team
  GET  /cron                    List all cron entries
  POST /cron                    Create a cron entry
  DELETE /cron/:id              Delete a cron entry
  GET  /mcp                     List MCP servers
  POST /mcp/:name/connect       Connect MCP server
  POST /mcp/:name/disconnect    Disconnect MCP server
  GET  /workers                 List all workers
  POST /workers                 Create a new worker
  GET  /workers/:id             Get worker details
  POST /workers/:id/restart     Restart a worker
  GET  /lsp                     List LSP servers
  POST /lsp/:language/connect   Connect LSP server"
    );
}
