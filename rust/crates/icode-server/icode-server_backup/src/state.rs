use runtime::{ConfigLoader, EventBus, SqliteStore};
use std::sync::Arc;
use tokio::sync::Mutex;
pub struct ServerState {
    pub event_bus: Arc<EventBus>,
    pub store: Arc<Mutex<SqliteStore>>,
    pub config_loader: ConfigLoader,
}
impl ServerState {
    pub fn new(e: EventBus, s: SqliteStore, c: ConfigLoader) -> Self {
        Self {
            event_bus: Arc::new(e),
            store: Arc::new(Mutex::new(s)),
            config_loader: c,
        }
    }
}
