use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serenity::all::{
    ChannelId, Context as SerenityCtx, CreateMessage, EditMessage, EventHandler, GatewayIntents,
    Message, MessageId, Ready,
};
use serenity::Client;
use tokio::sync::RwLock;

use crate::events::bus::EventBus;
use crate::events::types::{Event, EventPayload};

/// Discord adapter using Serenity.
pub struct DiscordAdapter {
    token: String,
    channels: HashSet<String>,
    bus: EventBus,
    interface_name: String,
    http: Arc<RwLock<Option<Arc<serenity::http::Http>>>>,
}

impl DiscordAdapter {
    pub fn new(
        token: String,
        channels: Vec<String>,
        bus: EventBus,
        interface_name: String,
    ) -> Self {
        Self {
            token,
            channels: channels.into_iter().collect(),
            bus,
            interface_name,
            http: Arc::new(RwLock::new(None)),
        }
    }
}

#[async_trait]
impl super::Adapter for DiscordAdapter {
    async fn start(&self) -> Result<()> {
        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT
            | GatewayIntents::DIRECT_MESSAGES;

        let handler = DiscordHandler {
            bus: self.bus.clone(),
            interface_name: self.interface_name.clone(),
            channels: self.channels.clone(),
            http: self.http.clone(),
        };

        let mut client = Client::builder(&self.token, intents)
            .event_handler(handler)
            .await
            .context("failed to create Discord client")?;

        client.start().await.context("Discord client error")?;
        Ok(())
    }

    async fn send_message(&self, channel: &str, content: &str) -> Result<()> {
        let http_guard = self.http.read().await;
        let http = http_guard
            .as_ref()
            .context("Discord not connected yet")?;

        let channel_id: u64 = channel.parse().context("invalid channel ID")?;
        let channel_id = ChannelId::new(channel_id);

        for chunk in chunk_message(content, 2000) {
            channel_id
                .send_message(http, CreateMessage::new().content(chunk))
                .await
                .context("failed to send Discord message")?;
        }
        Ok(())
    }

    async fn send_typing(&self, channel: &str) -> Result<()> {
        let http_guard = self.http.read().await;
        let http = http_guard
            .as_ref()
            .context("Discord not connected yet")?;

        let channel_id: u64 = channel.parse().context("invalid channel ID")?;
        let channel_id = ChannelId::new(channel_id);
        channel_id
            .broadcast_typing(http)
            .await
            .context("failed to send typing indicator")?;
        Ok(())
    }

    async fn edit_message(&self, channel: &str, message_id: &str, content: &str) -> Result<()> {
        let http_guard = self.http.read().await;
        let http = http_guard
            .as_ref()
            .context("Discord not connected yet")?;

        let channel_id: u64 = channel.parse().context("invalid channel ID")?;
        let msg_id: u64 = message_id.parse().context("invalid message ID")?;
        let channel_id = ChannelId::new(channel_id);
        channel_id
            .edit_message(http, MessageId::new(msg_id), EditMessage::new().content(content))
            .await
            .context("failed to edit Discord message")?;
        Ok(())
    }

    fn name(&self) -> &str {
        &self.interface_name
    }
}

/// Split a message into chunks that fit within `max_len`, preferring newline boundaries.
fn chunk_message(content: &str, max_len: usize) -> Vec<&str> {
    if content.len() <= max_len {
        return vec![content];
    }

    let mut chunks = Vec::new();
    let mut remaining = content;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining);
            break;
        }

        // Find the last newline within the limit
        let split_at = remaining[..max_len]
            .rfind('\n')
            .map(|i| i + 1) // include the newline in the current chunk
            .unwrap_or(max_len); // no newline found, hard split

        chunks.push(&remaining[..split_at]);
        remaining = &remaining[split_at..];
    }

    chunks
}

struct DiscordHandler {
    bus: EventBus,
    interface_name: String,
    channels: HashSet<String>,
    http: Arc<RwLock<Option<Arc<serenity::http::Http>>>>,
}

#[async_trait]
impl EventHandler for DiscordHandler {
    async fn ready(&self, ctx: SerenityCtx, ready: Ready) {
        tracing::info!(user = %ready.user.name, "Discord connected");
        *self.http.write().await = Some(ctx.http.clone());
    }

    async fn message(&self, _ctx: SerenityCtx, msg: Message) {
        // Ignore bot messages
        if msg.author.bot {
            return;
        }

        // Check channel filter (if channels list is non-empty)
        if !self.channels.is_empty() {
            let channel_name = msg.channel_id.to_string();
            if !self.channels.contains(&channel_name) {
                return;
            }
        }

        self.bus.publish(Event::new(EventPayload::MessageReceived {
            interface: self.interface_name.clone(),
            channel: msg.channel_id.to_string(),
            author: msg.author.name.clone(),
            content: msg.content.clone(),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_short_message() {
        let chunks = chunk_message("hello", 2000);
        assert_eq!(chunks, vec!["hello"]);
    }

    #[test]
    fn chunk_splits_on_newlines() {
        let msg = format!("{}\n{}", "a".repeat(1500), "b".repeat(1000));
        let chunks = chunk_message(&msg, 2000);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].len() <= 2000);
        assert!(chunks[1].len() <= 2000);
        assert!(chunks[0].ends_with('\n'));
    }

    #[test]
    fn chunk_hard_splits_without_newlines() {
        let msg = "x".repeat(5000);
        let chunks = chunk_message(&msg, 2000);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 2000);
        assert_eq!(chunks[1].len(), 2000);
        assert_eq!(chunks[2].len(), 1000);
    }

    #[test]
    fn chunk_empty_message() {
        let chunks = chunk_message("", 2000);
        assert_eq!(chunks, vec![""]);
    }
}
