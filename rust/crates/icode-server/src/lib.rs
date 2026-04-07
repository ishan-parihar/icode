pub mod api_doc;
pub mod routes;
pub mod sse;
pub mod state;

pub use crate::api_doc::ApiDoc;
pub use crate::routes::{
    connect_lsp, connect_mcp, create_cron, create_session, create_task, create_team, create_worker,
    delete_cron, delete_team, disconnect_mcp, events, get_config, get_session, get_task,
    get_worker, health, list_crons, list_lsp, list_mcp, list_sessions, list_tasks, list_teams,
    list_workers, read_file_handler, restart_worker, send_message, stop_task,
};
pub use crate::state::ServerState;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::{delete, get, post};
    use axum::Router;
    use runtime::SqliteStore;
    use runtime::{ConfigLoader, EventBus};
    use serde_json::json;
    use tower::ServiceExt;
    use tower_http::cors::CorsLayer;
    use utoipa::OpenApi;

    use crate::routes::{
        connect_lsp, connect_mcp, create_cron, create_session, create_task, create_team,
        create_worker, delete_cron, delete_team, disconnect_mcp, events, get_config, get_session,
        get_task, get_worker, health, list_crons, list_lsp, list_mcp, list_sessions, list_tasks,
        list_teams, list_workers, read_file_handler, restart_worker, send_message, stop_task,
    };
    use crate::state::ServerState;

    fn test_router() -> Router {
        let event_bus = EventBus::default();
        let store = SqliteStore::in_memory().expect("in-memory store");
        store.migrate().expect("migration");
        let config_loader = ConfigLoader::default_for(std::env::current_dir().unwrap_or_default());
        let state = Arc::new(ServerState::new(event_bus, store, config_loader));

        Router::new()
            .route("/health", get(health))
            .route("/sessions", post(create_session).get(list_sessions))
            .route("/sessions/{id}", get(get_session))
            .route("/sessions/{id}/message", post(send_message))
            .route("/events", get(events))
            .route("/files/read", post(read_file_handler))
            .route("/config", get(get_config))
            .route("/tasks", get(list_tasks).post(create_task))
            .route("/tasks/{id}", get(get_task))
            .route("/tasks/{id}/stop", post(stop_task))
            .route("/teams", get(list_teams).post(create_team))
            .route("/teams/{id}", delete(delete_team))
            .route("/cron", get(list_crons).post(create_cron))
            .route("/cron/{id}", delete(delete_cron))
            .route("/mcp", get(list_mcp))
            .route("/mcp/{name}/connect", post(connect_mcp))
            .route("/mcp/{name}/disconnect", post(disconnect_mcp))
            .route("/workers", get(list_workers).post(create_worker))
            .route("/workers/{id}", get(get_worker))
            .route("/workers/{id}/restart", post(restart_worker))
            .route("/lsp", get(list_lsp))
            .route("/lsp/{language}/connect", post(connect_lsp))
            .layer(CorsLayer::permissive())
            .with_state(state)
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let val: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(val["status"], "ok");
    }

    #[tokio::test]
    async fn create_and_list_sessions() {
        let app = test_router();

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sessions")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let val: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let session_id = val["session_id"].as_str().expect("session_id");
        assert!(!session_id.is_empty());

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let val: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(val["sessions"].as_array().unwrap().len() >= 1);
    }

    #[tokio::test]
    async fn get_session_not_found() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/sessions/nonexistent-id")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unknown_route_returns_404() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn cors_headers_present() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/health")
                    .header("origin", "http://example.com")
                    .header("access-control-request-method", "GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().contains_key("access-control-allow-origin"));
    }

    #[tokio::test]
    async fn events_stream_starts() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let content_type = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(content_type.contains("text/event-stream"));
    }

    #[tokio::test]
    async fn get_config_returns_config() {
        let app = test_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn openapi_spec_generates_valid_json() {
        let doc = crate::ApiDoc::openapi();
        let json = serde_json::to_value(&doc).expect("should serialize");
        assert!(json.get("openapi").is_some());
        assert!(json.get("paths").is_some());
        let paths = json.get("paths").unwrap().as_object().unwrap();
        assert!(paths.contains_key("/health"));
        assert!(paths.contains_key("/sessions"));
        assert!(paths.contains_key("/events"));
        assert!(paths.contains_key("/config"));
    }
}
