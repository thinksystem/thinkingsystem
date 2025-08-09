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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub mid: String,
    pub content: String,
    pub sender: String,
    pub destination: MessageDestination,
    pub timestamp: DateTime<Utc>,
    pub msg_type: MessageType,

    pub format: Option<String>,
    pub encoding: Option<String>,
    pub refs: Option<Vec<String>>,
    pub ttl: Option<i64>,
    pub hops: Option<i64>,
    pub sig: Option<String>,

    pub metadata: Option<MessageMetadata>,
    pub reply_info: Option<ReplyInfo>,
    pub media: Option<MediaAttachment>,
    pub entities: Option<Vec<MessageEntity>>,
    pub reactions: Option<Vec<Reaction>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    Text,
    Presence,
    System,
    Ack,
    Alert,
    Notification,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum MessageDestination {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageStatus {
    Local,
    Sent,
    Delivered,
    Expired,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MessageMetadata {
    pub metadata: HashMap<String, MetadataValue>,
    pub additional_data: HashMap<String, String>,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MetadataValue {
    String(String),
    Int(i64),
    Bool(bool),
    Float(f64),
    StringArray(Vec<String>),
    OptionInt(Option<i64>),
    ReplyInfo(Box<ReplyInfo>),
    MediaAttachment(Box<MediaAttachment>),
    Entities(Vec<MessageEntity>),
    Reactions(Vec<Reaction>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplyInfo {
    pub message_id: String,
    pub user_id: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaAttachment {
    pub media_type: String,
    pub url: String,
    pub thumbnail_url: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageEntity {
    pub entity_type: String,
    pub offset: i32,
    pub length: i32,
    pub url: Option<String>,
    pub user: Option<User>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub user_id: String,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reaction {
    pub reaction: String,
    pub count: i32,
    pub users: Vec<String>,
}

impl Message {
    pub fn new(sender: String, destination: MessageDestination, content: String) -> Self {
        let now = Utc::now();
        Self {
            mid: Uuid::new_v4().to_string(),
            content,
            sender,
            destination,
            timestamp: now,
            msg_type: MessageType::Text,
            format: Some("text".to_string()),
            encoding: Some("utf8".to_string()),
            refs: None,
            ttl: None,
            hops: Some(0),
            sig: None,
            metadata: None,
            reply_info: None,
            media: None,
            entities: None,
            reactions: None,
        }
    }

    pub fn get_content(&self) -> &str {
        &self.content
    }

    pub fn with_metadata(mut self, metadata: MessageMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

impl MessageMetadata {
    pub fn new() -> Self {
        MessageMetadata {
            metadata: HashMap::new(),
            additional_data: HashMap::new(),
            last_updated: chrono::Utc::now(),
        }
    }

    pub fn insert(&mut self, key: String, value: MetadataValue) {
        self.metadata.insert(key, value);
    }

    pub fn get(&self, key: &str) -> Option<&MetadataValue> {
        self.metadata.get(key)
    }

    pub fn remove(&mut self, key: &str) -> Option<MetadataValue> {
        self.metadata.remove(key)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.metadata.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.metadata.len()
    }

    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty()
    }

    pub fn clear(&mut self) {
        self.metadata.clear();
    }

    pub fn keys(&self) -> Vec<&String> {
        self.metadata.keys().collect()
    }

    pub fn values(&self) -> Vec<&MetadataValue> {
        self.metadata.values().collect()
    }

    pub fn iter(&self) -> std::collections::hash_map::Iter<String, MetadataValue> {
        self.metadata.iter()
    }

    pub fn iter_mut(&mut self) -> std::collections::hash_map::IterMut<String, MetadataValue> {
        self.metadata.iter_mut()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub node_type: String,
    pub properties: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub from_node: String,
    pub to_node: String,
    pub label: String,
    pub properties: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EdgeLabel {
    RepliesTo,
    Mentions,
    ReactsTo,
    Forwards,
    ParticipatesIn,
    BelongsTo,
    Sends,
    Receives,
    Custom(String),
}

impl EdgeLabel {
    pub fn as_str(&self) -> &str {
        match self {
            EdgeLabel::RepliesTo => "replies_to",
            EdgeLabel::Mentions => "mentions",
            EdgeLabel::ReactsTo => "reacts_to",
            EdgeLabel::Forwards => "forwards",
            EdgeLabel::ParticipatesIn => "participates_in",
            EdgeLabel::BelongsTo => "belongs_to",
            EdgeLabel::Sends => "sends",
            EdgeLabel::Receives => "receives",
            EdgeLabel::Custom(label) => label,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    Message,
    User,
    Conversation,
    Channel,
    Thread,
    Custom(String),
}

impl NodeType {
    pub fn as_str(&self) -> &str {
        match self {
            NodeType::Message => "message",
            NodeType::User => "user",
            NodeType::Conversation => "conversation",
            NodeType::Channel => "channel",
            NodeType::Thread => "thread",
            NodeType::Custom(node_type) => node_type,
        }
    }
}

impl GraphNode {
    pub fn new(node_type: NodeType, properties: HashMap<String, serde_json::Value>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            node_type: node_type.as_str().to_string(),
            properties,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn get_property(&self, key: &str) -> Option<&serde_json::Value> {
        self.properties.get(key)
    }

    pub fn set_property(&mut self, key: String, value: serde_json::Value) {
        self.properties.insert(key, value);
        self.updated_at = Utc::now();
    }

    pub fn get_string_property(&self, key: &str) -> Option<String> {
        self.properties.get(key)?.as_str().map(String::from)
    }

    pub fn get_i64_property(&self, key: &str) -> Option<i64> {
        self.properties.get(key)?.as_i64()
    }

    pub fn get_bool_property(&self, key: &str) -> Option<bool> {
        self.properties.get(key)?.as_bool()
    }

    pub fn get_array_property(&self, key: &str) -> Option<&Vec<serde_json::Value>> {
        self.properties.get(key)?.as_array()
    }

    pub fn is_message_node(&self) -> bool {
        self.node_type == NodeType::Message.as_str()
    }

    pub fn is_user_node(&self) -> bool {
        self.node_type == NodeType::User.as_str()
    }

    pub fn is_conversation_node(&self) -> bool {
        self.node_type == NodeType::Conversation.as_str()
    }
}

impl GraphEdge {
    pub fn new(
        from_node: String,
        to_node: String,
        label: EdgeLabel,
        properties: HashMap<String, serde_json::Value>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            from_node,
            to_node,
            label: label.as_str().to_string(),
            properties,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn get_property(&self, key: &str) -> Option<&serde_json::Value> {
        self.properties.get(key)
    }

    pub fn set_property(&mut self, key: String, value: serde_json::Value) {
        self.properties.insert(key, value);
        self.updated_at = Utc::now();
    }

    pub fn get_string_property(&self, key: &str) -> Option<String> {
        self.properties.get(key)?.as_str().map(String::from)
    }

    pub fn get_i64_property(&self, key: &str) -> Option<i64> {
        self.properties.get(key)?.as_i64()
    }

    pub fn get_timestamp_property(&self, key: &str) -> Option<DateTime<Utc>> {
        let timestamp_str = self.get_string_property(key)?;
        DateTime::parse_from_rfc3339(&timestamp_str)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    pub fn is_reply(&self) -> bool {
        self.label == EdgeLabel::RepliesTo.as_str()
    }

    pub fn is_mention(&self) -> bool {
        self.label == EdgeLabel::Mentions.as_str()
    }

    pub fn is_reaction(&self) -> bool {
        self.label == EdgeLabel::ReactsTo.as_str()
    }

    pub fn is_participation(&self) -> bool {
        self.label == EdgeLabel::ParticipatesIn.as_str()
    }

    pub fn get_from_node_type(&self) -> Option<String> {
        self.from_node.split(':').next().map(String::from)
    }

    pub fn get_to_node_type(&self) -> Option<String> {
        self.to_node.split(':').next().map(String::from)
    }

    pub fn get_from_entity_id(&self) -> Option<String> {
        self.from_node.split(':').nth(1).map(String::from)
    }

    pub fn get_to_entity_id(&self) -> Option<String> {
        self.to_node.split(':').nth(1).map(String::from)
    }
}
