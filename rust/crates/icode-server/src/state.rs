use std::sync::Arc;

use runtime::{ConfigLoader, EventBus, SqliteStore};
use tokio::sync::Mutex;

pub struct ServerState {
    pub event_bus: Arc<EventBus>,
    pub store: Arc<Mutex<SqliteStore>>,
    pub config_loader: ConfigLoader,
}

impl ServerState {
    pub fn new(event_bus: EventBus, store: SqliteStore, config_loader: ConfigLoader) -> Self {
        Self {
            event_bus: Arc::new(event_bus),
            store: Arc::new(Mutex::new(store)),
            config_loader,
        }
    }
}
