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

#[cfg(feature = "surrealdb")]
use crate::messaging::database::MessagingApp;
use crate::messaging::insight::MessageSecurity;
use crate::messaging::network::{MessageRouter, NetworkManager, RouteType};
use crate::messaging::platforms::PlatformManager;
use crate::messaging::types::{Message, MessageType};
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct MessageProcessor {
    security: Arc<Mutex<MessageSecurity>>,
}

impl MessageProcessor {
    pub fn new(security: Arc<Mutex<MessageSecurity>>) -> Self {
        Self { security }
    }

    pub async fn process_message(&self, mut message: Message) -> Result<Message> {
        let analysis = self
            .security
            .lock()
            .unwrap()
            .assess_message_risk(&message.content);

        if analysis.requires_scribes_review() {
            println!("Message flagged for scribes review: {analysis:?}");
        }

        if let Some(ref mut metadata) = message.metadata {
            metadata.last_updated = Utc::now();
        }

        Ok(message)
    }
}

#[async_trait::async_trait]
pub trait MetadataExtractor: Send + Sync {
    async fn extract(&self, message: &Message) -> Result<String>;
}

pub struct SentimentExtractor;

impl Default for SentimentExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl SentimentExtractor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl MetadataExtractor for SentimentExtractor {
    async fn extract(&self, message: &Message) -> Result<String> {
        let sentiment = if message.content.contains("!") || message.content.contains("?") {
            "excited"
        } else if message.content.to_lowercase().contains("problem")
            || message.content.to_lowercase().contains("issue")
        {
            "negative"
        } else {
            "neutral"
        };
        Ok(sentiment.to_string())
    }
}

pub struct PriorityExtractor;

impl Default for PriorityExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl PriorityExtractor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl MetadataExtractor for PriorityExtractor {
    async fn extract(&self, message: &Message) -> Result<String> {
        let priority = match message.msg_type {
            MessageType::Alert => "high",
            MessageType::Notification => "medium",
            MessageType::Text => "low",
            MessageType::System => "high",
            MessageType::Presence => "low",
            MessageType::Ack => "low",
        };
        Ok(priority.to_string())
    }
}

pub struct CategoryExtractor;

impl Default for CategoryExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl CategoryExtractor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl MetadataExtractor for CategoryExtractor {
    async fn extract(&self, message: &Message) -> Result<String> {
        let content_lower = message.content.to_lowercase();
        let category = if content_lower.contains("meeting") || content_lower.contains("schedule") {
            "scheduling"
        } else if content_lower.contains("task") || content_lower.contains("todo") {
            "tasks"
        } else if content_lower.contains("error") || content_lower.contains("bug") {
            "technical"
        } else {
            "general"
        };
        Ok(category.to_string())
    }
}

pub struct MessageManager {
    #[cfg(feature = "surrealdb")]
    database: Arc<RwLock<MessagingApp>>,
    network: Arc<RwLock<NetworkManager>>,
    platforms: Arc<RwLock<PlatformManager>>,
    processor: MessageProcessor,
    router: Arc<MessageRouter>,
}

impl MessageManager {
    #[cfg(feature = "surrealdb")]
    pub async fn new(database_url: &str, security: Arc<Mutex<MessageSecurity>>) -> Result<Self> {
        let database = Arc::new(RwLock::new(
            MessagingApp::new(database_url, "root", "root")
                .await
                .context("Initialising messaging database")?,
        ));

        let network = Arc::new(RwLock::new(NetworkManager::new(
            10,
            "local_peer".to_string(),
        )));
        let platforms = Arc::new(RwLock::new(PlatformManager::new()));
        let processor = MessageProcessor::new(security);
        let router = Arc::new(MessageRouter::new(10));

        Ok(Self {
            database,
            network,
            platforms,
            processor,
            router,
        })
    }

    #[cfg(not(feature = "surrealdb"))]
    pub async fn new(_database_url: &str, security: Arc<Mutex<MessageSecurity>>) -> Result<Self> {
        let network = Arc::new(RwLock::new(NetworkManager::new()));
        let platforms = Arc::new(RwLock::new(PlatformManager::new()));
        let processor = MessageProcessor::new(security.clone());
        let router = Arc::new(MessageRouter::new(network.clone()));
        Ok(Self { network, platforms, processor, router })
    }

    pub async fn send_message(&self, mut message: Message) -> Result<String> {
        message = self
            .processor
            .process_message(message)
            .await
            .context("Failed to process message")?;

        {
            let db = self.database.write().await;
            db.store_message(&message)
                .await
                .context("Failed to store message in database")?;
        }

        if let Some(ref reply_info) = message.reply_info {
            let db = self.database.write().await;
            db.create_reply_relationship(&message.mid, &reply_info.message_id, "direct")
                .await
                .context("Failed to create reply relationship")?;
        }

        let routing_decision = self.router.route_message(&message).await?;

        let network_result = {
            let network = self.network.read().await;
            match routing_decision.route_type {
                RouteType::Direct => network
                    .send_message(message.clone())
                    .await
                    .context("Failed to route message through network"),
                RouteType::Broadcast => self
                    .router
                    .broadcast_message(&message)
                    .await
                    .context("Failed to broadcast message"),
                RouteType::Multicast => network
                    .send_to_peers(message.clone(), &routing_decision.target_peers)
                    .await
                    .context("Failed to multicast message"),
            }
        };

        let platform_result = {
            let platforms = self.platforms.read().await;
            platforms
                .send_message(&message, None)
                .await
                .context("Failed to send message via platform")
        };

        match (network_result, platform_result) {
            (Ok(_), Ok(platform_id)) => Ok(platform_id),
            (Err(e), _) => Err(e),
            (_, Err(e)) => Err(e),
        }
    }

    pub async fn receive_messages(&self) -> Result<Vec<Message>> {
        let mut all_messages = Vec::new();

        {
            let platforms = self.platforms.read().await;
            let platform_messages = platforms
                .receive_all_messages()
                .await
                .context("Failed to receive messages from platforms")?;

            for (_platform, messages) in platform_messages {
                for message in messages {
                    all_messages.push(message);
                }
            }
        }

        let mut processed_messages = Vec::new();
        for message in all_messages {
            let processed = self
                .processor
                .process_message(message)
                .await
                .context("Failed to process received message")?;

            {
                let db = self.database.write().await;
                if let Err(e) = db.store_message(&processed).await {
                    tracing::error!("Failed to store received message: {}", e);
                }
            }

            processed_messages.push(processed);
        }

        Ok(processed_messages)
    }

    pub async fn get_message_history(
        &self,
        user_id: String,
        limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        let db = self.database.read().await;
        db.get_recent_messages(user_id, limit.unwrap_or(100))
            .await
            .context("Failed to get message history")
    }

    pub async fn search_messages(&self, query: &str, user_id: String) -> Result<Vec<Message>> {
        let db = self.database.read().await;
        db.search_messages(query.to_string(), user_id)
            .await
            .context("Failed to search messages")
    }

    pub async fn add_platform(
        &self,
        bridge: Box<dyn crate::messaging::platforms::PlatformBridge>,
    ) -> Result<()> {
        let mut platforms = self.platforms.write().await;
        platforms
            .add_platform(bridge)
            .await
            .context("Failed to add platform bridge")?;
        Ok(())
    }

    pub async fn get_system_status(&self) -> HashMap<String, String> {
        let mut status = HashMap::new();

        status.insert("database".to_string(), "connected".to_string());

        {
            let platforms = self.platforms.read().await;
            let platform_status = platforms.get_platform_status().await;
            for (platform, connected) in platform_status {
                status.insert(
                    format!("platform_{platform}"),
                    if connected {
                        "connected"
                    } else {
                        "disconnected"
                    }
                    .to_string(),
                );
            }
        }

        status.insert("network".to_string(), "available".to_string());

        status
    }

    pub async fn shutdown(&self) -> Result<()> {
        {
            let mut platforms = self.platforms.write().await;
            for platform_type in platforms.get_available_platforms() {
                if let Err(e) = platforms.remove_platform(&platform_type).await {
                    tracing::error!("Failed to disconnect platform {}: {}", platform_type, e);
                }
            }
        }

        Ok(())
    }

    pub async fn send_reply(
        &self,
        from: String,
        to: crate::messaging::types::MessageDestination,
        content: String,
        reply_to_message_id: String,
        reply_to_user_id: String,
    ) -> Result<String> {
        use crate::messaging::types::{MessageType, ReplyInfo};
        use chrono::Utc;
        use uuid::Uuid;

        let reply_info = ReplyInfo {
            message_id: reply_to_message_id.clone(),
            user_id: reply_to_user_id,
            timestamp: Utc::now().timestamp_millis(),
        };

        let message = crate::messaging::types::Message {
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

        self.send_message(message).await
    }

    pub async fn get_conversation_messages(
        &self,
        conversation_id: String,
    ) -> Result<Vec<crate::messaging::types::Message>> {
        let db = self.database.read().await;
        db.get_conversation_messages(&conversation_id)
            .await
            .context("Failed to get conversation messages")
    }

    pub async fn get_message_replies(
        &self,
        message_id: String,
    ) -> Result<Vec<crate::messaging::types::Message>> {
        let db = self.database.read().await;
        db.get_message_replies(&message_id)
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
        use crate::messaging::types::{GraphNode, NodeType};
        use std::collections::HashMap;

        let mut properties = HashMap::new();
        properties.insert(
            "conversation_id".to_string(),
            serde_json::Value::String(conversation_id.clone()),
        );
        properties.insert("title".to_string(), serde_json::Value::String(title));
        properties.insert(
            "created_by".to_string(),
            serde_json::Value::String(creator_id.clone()),
        );

        let conversation_node = GraphNode {
            id: format!("conversation:{conversation_id}"),
            node_type: NodeType::Conversation.as_str().to_string(),
            properties,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let db = self.database.write().await;

        db.create_graph_node(conversation_node)
            .await
            .context("Failed to create conversation node")?;

        db.add_user_to_conversation(&creator_id, &conversation_id, "admin")
            .await
            .context("Failed to add creator to conversation")?;

        for participant in participants {
            if participant != creator_id {
                db.add_user_to_conversation(&participant, &conversation_id, "member")
                    .await
                    .context("Failed to add participant to conversation")?;
            }
        }

        Ok(())
    }

    pub async fn add_user_to_conversation(
        &self,
        user_id: String,
        conversation_id: String,
        role: String,
    ) -> Result<()> {
        let db = self.database.write().await;
        db.add_user_to_conversation(&user_id, &conversation_id, &role)
            .await
            .context("Failed to add user to conversation")
    }
}

#[derive(Debug, Clone)]
pub struct ManagerConfig {
    pub database_url: String,
    pub max_message_size: usize,
    pub max_history_size: usize,
    pub enable_pii_scanning: bool,
    pub enable_metadata_extraction: bool,
    pub network_enabled: bool,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            database_url: "ws://127.0.0.1:8000/rpc".to_string(),
            max_message_size: 1024 * 1024,
            max_history_size: 10000,
            enable_pii_scanning: true,
            enable_metadata_extraction: true,
            network_enabled: true,
        }
    }
}

pub async fn create_message_manager(config: ManagerConfig) -> Result<MessageManager> {
    let security = Arc::new(Mutex::new(MessageSecurity::default()));
    MessageManager::new(&config.database_url, security).await
}
