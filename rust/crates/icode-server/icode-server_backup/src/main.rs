use axum::routing::{delete, get, post};
use axum::Json;
use axum::Router;
use icode_server::routes::{
    connect_lsp, connect_mcp, create_cron, create_session, create_task, create_team, create_worker,
    delete_cron, delete_team, disconnect_mcp, events, get_config, get_session, get_task,
    get_worker, health, list_crons, list_lsp, list_mcp, list_sessions, list_tasks, list_teams,
    list_workers, read_file_handler, restart_worker, send_message, stop_task,
};
use icode_server::state::ServerState;
use icode_server::ApiDoc;
use runtime::{ConfigLoader, EventBus, SqliteStore};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
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
    let store = SqliteStore::new(
        args.db_path
            .unwrap_or_else(|| std::env::temp_dir().join("icode").join("sessions.db")),
    )
    .expect("sqlite");
    let config_loader = ConfigLoader::default_for(&args.cwd);
    let state = Arc::new(ServerState::new(event_bus, store, config_loader));
    let app = create_router(state);
    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .expect("addr");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    tracing::info!("icode-server on {addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await
        .expect("serve")
}
fn create_router(s: Arc<ServerState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/config", get(get_config))
        .route("/sessions", post(create_session))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{id}", get(get_session))
        .route("/sessions/{id}/message", post(send_message))
        .route("/events", get(events))
        .route("/files/read", post(read_file_handler))
        .route("/tasks", get(list_tasks))
        .route("/tasks", post(create_task))
        .route("/tasks/{id}", get(get_task))
        .route("/tasks/{id}/stop", post(stop_task))
        .route("/teams", get(list_teams))
        .route("/teams", post(create_team))
        .route("/teams/{id}", delete(delete_team))
        .route("/cron", get(list_crons))
        .route("/cron", post(create_cron))
        .route("/cron/{id}", delete(delete_cron))
        .route("/mcp", get(list_mcp))
        .route("/mcp/{name}/connect", post(connect_mcp))
        .route("/mcp/{name}/disconnect", post(disconnect_mcp))
        .route("/workers", get(list_workers))
        .route("/workers", post(create_worker))
        .route("/workers/{id}", get(get_worker))
        .route("/workers/{id}/restart", post(restart_worker))
        .route("/lsp", get(list_lsp))
        .route("/lsp/{language}/connect", post(connect_lsp))
        .merge(SwaggerUi::new("/swagger-ui").url("/api/openapi.json", ApiDoc::openapi()))
        .route(
            "/api/openapi.json",
            get(|| async { Json(ApiDoc::openapi()) }),
        )
        .layer(CorsLayer::permissive())
        .with_state(s)
}
async fn shutdown() {
    let c = async {
        tokio::signal::ctrl_c().await.expect("ctrl");
    };
    #[cfg(unix)]
    let t = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("term")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let t = std::future::pending::<()>();
    tokio::select! {_ = c=>{},_ = t=>{},}
    tracing::info!("shutdown")
}
struct SA {
    host: String,
    port: u16,
    cwd: PathBuf,
    db_path: Option<PathBuf>,
}
fn parse_args() -> SA {
    let a: Vec<String> = std::env::args().collect();
    let mut h = "127.0.0.1".into();
    let mut p: u16 = 3000;
    let mut c = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut d: Option<PathBuf> = None;
    let mut i = 1;
    while i < a.len() {
        match a[i].as_str() {
            "--host" => {
                i += 1;
                if i < a.len() {
                    h = a[i].clone();
                }
            }
            "--port" => {
                i += 1;
                if i < a.len() {
                    p = a[i].parse().unwrap_or(3000);
                }
            }
            "--cwd" => {
                i += 1;
                if i < a.len() {
                    c = PathBuf::from(&a[i]);
                }
            }
            "--db" => {
                i += 1;
                if i < a.len() {
                    d = Some(PathBuf::from(&a[i]));
                }
            }
            "--help" | "-h" => {
                println!("icode-server [--host] [--port] [--cwd] [--db]");
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }
    SA {
        host: h,
        port: p,
        cwd: c,
        db_path: d,
    }
}
