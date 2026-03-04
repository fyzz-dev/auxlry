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
        channel_id
            .send_message(http, CreateMessage::new().content(content))
            .await
            .context("failed to send Discord message")?;
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
