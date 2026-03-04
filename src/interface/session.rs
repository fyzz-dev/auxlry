use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, mpsc};
use tokio::time::Instant;
use tracing::debug;

/// A pending message waiting to be batched.
#[derive(Debug, Clone)]
pub struct PendingMessage {
    pub author: String,
    pub content: String,
    pub received_at: Instant,
}

/// Per-channel session state for smart batching.
#[derive(Debug)]
struct ChannelSession {
    pending: Vec<PendingMessage>,
    last_received: Instant,
}

/// Manages smart batching and debounce across channels.
pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<String, ChannelSession>>>,
    debounce_ms: u64,
    batch_tx: mpsc::Sender<BatchedInput>,
}

/// A batch of messages ready for LLM processing.
#[derive(Debug, Clone)]
pub struct BatchedInput {
    pub interface: String,
    pub channel: String,
    pub messages: Vec<PendingMessage>,
}

impl SessionManager {
    pub fn new(debounce_ms: u64) -> (Self, mpsc::Receiver<BatchedInput>) {
        let (batch_tx, batch_rx) = mpsc::channel(64);
        let mgr = Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            debounce_ms,
            batch_tx,
        };
        (mgr, batch_rx)
    }

    /// Add a message and start/reset the debounce timer for its channel.
    pub async fn add_message(
        &self,
        interface: &str,
        channel: &str,
        author: &str,
        content: &str,
    ) {
        let key = format!("{interface}:{channel}");
        let msg = PendingMessage {
            author: author.to_string(),
            content: content.to_string(),
            received_at: Instant::now(),
        };

        let mut sessions = self.sessions.lock().await;
        let session = sessions.entry(key.clone()).or_insert_with(|| ChannelSession {
            pending: Vec::new(),
            last_received: Instant::now(),
        });
        session.pending.push(msg);
        session.last_received = Instant::now();

        // Spawn debounce timer if this is the first message in the batch
        if session.pending.len() == 1 {
            let sessions = self.sessions.clone();
            let debounce = Duration::from_millis(self.debounce_ms);
            let tx = self.batch_tx.clone();
            let interface = interface.to_string();
            let channel = channel.to_string();

            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(debounce).await;

                    let mut sessions = sessions.lock().await;
                    if let Some(session) = sessions.get(&key) {
                        // Check if more messages arrived during the debounce window
                        if session.last_received.elapsed() >= debounce {
                            // Debounce complete — flush the batch
                            if let Some(session) = sessions.remove(&key) {
                                debug!(
                                    channel = %channel,
                                    count = session.pending.len(),
                                    "flushing message batch"
                                );
                                let _ = tx
                                    .send(BatchedInput {
                                        interface: interface.clone(),
                                        channel: channel.clone(),
                                        messages: session.pending,
                                    })
                                    .await;
                            }
                            break;
                        }
                        // Otherwise, keep waiting
                    } else {
                        break;
                    }
                }
            });
        }
    }
}
