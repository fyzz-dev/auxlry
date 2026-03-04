pub mod discord;

use anyhow::Result;
use async_trait::async_trait;

/// Trait for chat platform adapters (Discord, Telegram, Webhook, etc.)
#[async_trait]
pub trait Adapter: Send + Sync + 'static {
    /// Start the adapter and begin listening for messages.
    async fn start(&self) -> Result<()>;

    /// Send a message to a specific channel.
    async fn send_message(&self, channel: &str, content: &str) -> Result<()>;

    /// Send a typing indicator to a channel.
    async fn send_typing(&self, channel: &str) -> Result<()>;

    /// Edit a previously sent message.
    async fn edit_message(&self, channel: &str, message_id: &str, content: &str) -> Result<()>;

    /// The adapter's name (for logging/config).
    fn name(&self) -> &str;
}
