// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2024 Jonathan Lee
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License version 3
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see https://www.gnu.org/licenses/.

use crate::messaging::insight::MessageSecurity;
use crate::messaging::management::MessageManager;
use crate::messaging::types::{
    MediaAttachment, Message, MessageDestination, MessageEntity, MessageMetadata, MessageType,
    MetadataValue, ReplyInfo,
};
use anyhow::{Context, Result};
use chrono::Utc;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

type AttachmentTuple = (
    String,
    String,
    Option<String>,
    Option<i32>,
    Option<i32>,
    Option<i32>,
);

pub struct MessagingClient {
    manager: Arc<MessageManager>,
    security: Arc<Mutex<MessageSecurity>>,
}

impl MessagingClient {
    pub async fn new(database_url: &str) -> Result<Self> {
        let security = Arc::new(Mutex::new(MessageSecurity::default()));

        let manager = Arc::new(
            MessageManager::new(database_url, security.clone())
                .await
                .context("Failed to create message manager")?,
        );

        Ok(Self { manager, security })
    }

    pub async fn send_text_message(
        &self,
        from: String,
        to: MessageDestination,
        content: String,
    ) -> Result<String> {
        let message = Message {
            mid: Uuid::new_v4().to_string(),
            content,
            sender: from,
            destination: to,
            timestamp: Utc::now(),
            msg_type: MessageType::Text,
            format: Some("text/plain".to_string()),
            encoding: Some("utf-8".to_string()),
            refs: None,
            ttl: None,
            hops: Some(0),
            sig: None,
            metadata: None,
            reply_info: None,
            media: None,
            entities: None,
            reactions: None,
        };

        self.manager
            .send_message(message)
            .await
            .context("Failed to send text message")
    }

    pub async fn send_media_message(
        &self,
        from: String,
        to: MessageDestination,
        content: String,
        media: MediaAttachment,
    ) -> Result<String> {
        let message = Message {
            mid: Uuid::new_v4().to_string(),
            content,
            sender: from,
            destination: to,
            timestamp: Utc::now(),
            msg_type: MessageType::Text,
            format: Some("text/plain".to_string()),
            encoding: Some("utf-8".to_string()),
            refs: None,
            ttl: None,
            hops: Some(0),
            sig: None,
            metadata: None,
            reply_info: None,
            media: Some(media),
            entities: None,
            reactions: None,
        };

        self.manager
            .send_message(message)
            .await
            .context("Failed to send media message")
    }

    pub async fn send_multi_media_message(
        &self,
        from: String,
        to: MessageDestination,
        content: String,
        media_attachments: Vec<MediaAttachment>,
    ) -> Result<String> {
        let entities: Vec<MessageEntity> = media_attachments
            .iter()
            .map(|_| MessageEntity {
                entity_type: "media".to_string(),
                offset: 0,
                length: content.len() as i32,
                url: None,
                user: None,
            })
            .collect();

        let message = Message {
            mid: Uuid::new_v4().to_string(),
            content,
            sender: from,
            destination: to,
            timestamp: Utc::now(),
            msg_type: MessageType::Text,
            format: Some("text/plain".to_string()),
            encoding: Some("utf-8".to_string()),
            refs: None,
            ttl: None,
            hops: Some(0),
            sig: None,
            metadata: None,
            reply_info: None,
            media: media_attachments.first().cloned(),
            entities: if entities.is_empty() {
                None
            } else {
                Some(entities)
            },
            reactions: None,
        };

        self.manager
            .send_message(message)
            .await
            .context("Failed to send multi-media message")
    }

    pub async fn send_reply(
        &self,
        from: String,
        to: MessageDestination,
        content: String,
        reply_to_message_id: String,
        reply_to_user_id: String,
    ) -> Result<String> {
        let reply_info = ReplyInfo {
            message_id: reply_to_message_id.clone(),
            user_id: reply_to_user_id,
            timestamp: Utc::now().timestamp_millis(),
        };

        let message = Message {
            mid: Uuid::new_v4().to_string(),
            content,
            sender: from,
            destination: to,
            timestamp: Utc::now(),
            msg_type: MessageType::Text,
            format: Some("text/plain".to_string()),
            encoding: Some("utf-8".to_string()),
            refs: Some(vec![reply_to_message_id]),
            ttl: None,
            hops: Some(0),
            sig: None,
            metadata: None,
            reply_info: Some(reply_info),
            media: None,
            entities: None,
            reactions: None,
        };

        self.manager
            .send_message(message)
            .await
            .context("Failed to send reply message")
    }

    pub async fn send_system_notification(
        &self,
        to: MessageDestination,
        content: String,
        metadata: Option<MessageMetadata>,
    ) -> Result<String> {
        let message = Message {
            mid: Uuid::new_v4().to_string(),
            content,
            sender: "system".to_string(),
            destination: to,
            timestamp: Utc::now(),
            msg_type: MessageType::System,
            format: Some("text/plain".to_string()),
            encoding: Some("utf-8".to_string()),
            refs: None,
            ttl: Some(86400),
            hops: Some(0),
            sig: None,
            metadata,
            reply_info: None,
            media: None,
            entities: None,
            reactions: None,
        };

        self.manager
            .send_message(message)
            .await
            .context("Failed to send system notification")
    }

    pub async fn send_alert(
        &self,
        from: String,
        to: MessageDestination,
        content: String,
        priority: AlertPriority,
    ) -> Result<String> {
        let mut metadata = MessageMetadata::default();
        metadata.insert(
            "priority".to_string(),
            MetadataValue::String(priority.to_string()),
        );
        metadata.insert(
            "alert_type".to_string(),
            MetadataValue::String("general".to_string()),
        );

        let message = Message {
            mid: Uuid::new_v4().to_string(),
            content,
            sender: from,
            destination: to,
            timestamp: Utc::now(),
            msg_type: MessageType::Alert,
            format: Some("text/plain".to_string()),
            encoding: Some("utf-8".to_string()),
            refs: None,
            ttl: Some(3600),
            hops: Some(0),
            sig: None,
            metadata: Some(metadata),
            reply_info: None,
            media: None,
            entities: None,
            reactions: None,
        };

        self.manager
            .send_message(message)
            .await
            .context("Failed to send alert message")
    }

    pub async fn get_message_history(
        &self,
        user_id: String,
        limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        self.manager
            .get_message_history(user_id, limit)
            .await
            .context("Failed to get message history")
    }

    pub async fn search_messages(&self, query: &str, user_id: String) -> Result<Vec<Message>> {
        self.manager
            .search_messages(query, user_id)
            .await
            .context("Failed to search messages")
    }

    pub async fn add_reaction(
        &self,
        _message_id: String,
        _user_id: String,
        _reaction: String,
    ) -> Result<()> {
        anyhow::bail!("Reactions not yet implemented in message manager")
    }

    pub async fn get_status(&self) -> Result<ClientStatus> {
        let system_status = self.manager.get_system_status().await;

        Ok(ClientStatus {
            connected: true,
            system_status,
        })
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.manager
            .shutdown()
            .await
            .context("Failed to shutdown message manager")
    }

    pub async fn get_conversation_messages(&self, conversation_id: String) -> Result<Vec<Message>> {
        self.manager
            .get_conversation_messages(conversation_id)
            .await
            .context("Failed to get conversation messages")
    }

    pub async fn get_message_replies(&self, message_id: String) -> Result<Vec<Message>> {
        self.manager
            .get_message_replies(message_id)
            .await
            .context("Failed to get message replies")
    }

    pub async fn create_conversation(
        &self,
        conversation_id: String,
        title: String,
        creator_id: String,
        participants: Vec<String>,
    ) -> Result<()> {
        self.manager
            .create_conversation(conversation_id, title, creator_id, participants)
            .await
            .context("Failed to create conversation")
    }

    pub async fn add_user_to_conversation(
        &self,
        user_id: String,
        conversation_id: String,
        role: String,
    ) -> Result<()> {
        self.manager
            .add_user_to_conversation(user_id, conversation_id, role)
            .await
            .context("Failed to add user to conversation")
    }

    pub async fn send_graph_reply(
        &self,
        from: String,
        to: MessageDestination,
        content: String,
        reply_to_message_id: String,
        reply_to_user_id: String,
    ) -> Result<String> {
        self.manager
            .send_reply(from, to, content, reply_to_message_id, reply_to_user_id)
            .await
            .context("Failed to send graph-aware reply")
    }

    pub async fn requires_pii_processing(&self, content: &str) -> bool {
        self.security
            .lock()
            .unwrap()
            .requires_scribes_processing(content)
    }
}

#[derive(Debug, Clone)]
pub enum AlertPriority {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for AlertPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AlertPriority::Low => write!(f, "low"),
            AlertPriority::Medium => write!(f, "medium"),
            AlertPriority::High => write!(f, "high"),
            AlertPriority::Critical => write!(f, "critical"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientStatus {
    pub connected: bool,
    pub system_status: std::collections::HashMap<String, String>,
}

pub fn text_destination(recipient: &str) -> MessageDestination {
    MessageDestination::Single(recipient.to_string())
}

pub fn group_destination(recipients: Vec<String>) -> MessageDestination {
    MessageDestination::Multiple(recipients)
}

pub fn create_media_attachment(
    media_type: String,
    url: String,
    thumbnail_url: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
    duration: Option<i32>,
) -> MediaAttachment {
    MediaAttachment {
        media_type,
        url,
        thumbnail_url,
        width,
        height,
        duration,
    }
}

pub fn create_media_attachments(attachments: Vec<AttachmentTuple>) -> Vec<MediaAttachment> {
    attachments
        .into_iter()
        .map(
            |(media_type, url, thumbnail_url, width, height, duration)| {
                create_media_attachment(media_type, url, thumbnail_url, width, height, duration)
            },
        )
        .collect()
}

pub fn create_message_entity(
    entity_type: String,
    offset: i32,
    length: i32,
    url: Option<String>,
    user: Option<crate::messaging::types::User>,
) -> MessageEntity {
    MessageEntity {
        entity_type,
        offset,
        length,
        url,
        user,
    }
}
