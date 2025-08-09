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

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DataItem {
    KeyValue { key: String, value: Value },
    Content { content: String, content_type: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageHeader {
    pub message_id: Uuid,
    pub mime_type: String,
    pub timestamp: String,
    pub source: Option<String>,
    pub destination: Option<String>,
    pub correlation_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub name: String,
    pub data: Vec<DataItem>,
    pub header: MessageHeader,
}
impl Message {
    pub fn new(
        name: String,
        data: Vec<DataItem>,
        source: Option<String>,
        destination: Option<String>,
    ) -> Self {
        let id = Uuid::new_v4();
        let header = MessageHeader {
            message_id: id,
            mime_type: "application/json".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            source,
            destination,
            correlation_id: None,
        };
        Self {
            id,
            name,
            data,
            header,
        }
    }
}
impl fmt::Display for MessageHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MessageHeader (ID: {}, Timestamp: {})",
            self.message_id, self.timestamp
        )
    }
}
#[derive(Debug)]
pub enum DecodeError {
    MissingField(String),
    InvalidField(String),
}
impl TryFrom<Message> for HashMap<String, Value> {
    type Error = DecodeError;
    fn try_from(value: Message) -> Result<Self, Self::Error> {
        let mut result = HashMap::new();
        for item in value.data {
            match item {
                DataItem::KeyValue { key, value } => {
                    result.insert(key, value);
                }
                DataItem::Content {
                    content,
                    content_type,
                } => {
                    result.insert("content".to_string(), Value::String(content));
                    result.insert("content_type".to_string(), Value::String(content_type));
                }
            }
        }
        Ok(result)
    }
}
