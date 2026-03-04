use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tracing::warn;

use crate::adapters::Adapter;

/// A handle that continuously sends typing indicators until dropped or stopped.
///
/// Discord's typing indicator lasts ~10 seconds, so we re-send every 8 seconds
/// to keep it active for the full duration of LLM generation.
pub struct TypingHandle {
    active: Arc<AtomicBool>,
}

impl TypingHandle {
    /// Start sending typing indicators on the given channel via the matching adapter.
    pub fn start(adapters: &[Arc<dyn Adapter>], interface: &str, channel: &str) -> Self {
        let active = Arc::new(AtomicBool::new(true));

        if let Some(adapter) = adapters.iter().find(|a| a.name() == interface) {
            let adapter = Arc::clone(adapter);
            let channel = channel.to_string();
            let active = active.clone();
            tokio::spawn(async move {
                while active.load(Ordering::Relaxed) {
                    if let Err(e) = adapter.send_typing(&channel).await {
                        warn!(error = %e, "failed to send typing indicator");
                    }
                    // Sleep in short increments so we can check the flag
                    for _ in 0..16 {
                        if !active.load(Ordering::Relaxed) {
                            return;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            });
        }

        Self { active }
    }
}

impl Drop for TypingHandle {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Relaxed);
    }
}
