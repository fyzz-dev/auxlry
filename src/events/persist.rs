use tokio::sync::broadcast;
use tracing::{error, debug};

use crate::events::types::Event;
use crate::storage::database::Database;

/// Spawns a background task that persists events from the bus to SQLite.
pub fn spawn_persister(mut rx: broadcast::Receiver<Event>, db: Database) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    debug!(kind = event.kind(), id = %event.id, "persisting event");
                    if let Err(e) = db.insert_event(&event).await {
                        error!(error = %e, "failed to persist event");
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "event persister lagged, some events not persisted");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    debug!("event bus closed, persister shutting down");
                    break;
                }
            }
        }
    })
}
