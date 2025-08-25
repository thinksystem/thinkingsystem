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



use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramMessage {
    pub message_id: i64,
    pub from_user: Option<String>,
    pub chat_id: i64,
    pub text: String,
    pub timestamp: DateTime<Utc>,
    pub is_outgoing: bool,
}

#[allow(dead_code)]
pub struct TelegramClient {
    client: Client,
    base_url: String,
    pub last_update_id: Option<i64>,
}

#[allow(dead_code)]
impl TelegramClient {
    pub fn new(token: String) -> Self {
        Self {
            client: Client::new(),
            base_url: format!("https://api.telegram.org/bot{token}"),
            last_update_id: None,
        }
    }

    pub async fn test_connection(&self) -> Result<()> {
        let url = format!("{}/getMe", self.base_url);
        let response = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "HTTP {}: {}",
                response.status(),
                response.text().await?
            ));
        }
        let result: serde_json::Value = response.json().await?;
        if !result["ok"].as_bool().unwrap_or(false) {
            return Err(anyhow!(
                "API error: {}",
                result["description"].as_str().unwrap_or("Unknown")
            ));
        }
        Ok(())
    }

    pub async fn send_message(&self, chat_id: i64, text: &str) -> Result<()> {
        let url = format!("{}/sendMessage", self.base_url);
        let payload = serde_json::json!({"chat_id": chat_id, "text": text});
        let response = self
            .client
            .post(&url)
            .json(&payload)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "HTTP {}: {}",
                response.status(),
                response.text().await?
            ));
        }
        let result: serde_json::Value = response.json().await?;
        if !result["ok"].as_bool().unwrap_or(false) {
            return Err(anyhow!(
                "API error: {}",
                result["description"].as_str().unwrap_or("Unknown")
            ));
        }
        Ok(())
    }

    pub async fn get_updates(&mut self) -> Result<Vec<TelegramMessage>> {
        let mut url = format!("{}/getUpdates", self.base_url);
        url = if let Some(offset) = self.last_update_id {
            format!("{}?offset={}&limit=10", url, offset + 1)
        } else {
            format!("{url}?limit=10")
        };
        let response = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "HTTP {}: {}",
                response.status(),
                response.text().await?
            ));
        }
        let result: serde_json::Value = response.json().await?;
        if !result["ok"].as_bool().unwrap_or(false) {
            return Err(anyhow!(
                "API error: {}",
                result["description"].as_str().unwrap_or("Unknown")
            ));
        }

        let empty_vec = vec![];
        let updates = result["result"].as_array().unwrap_or(&empty_vec);
        let mut messages = Vec::new();
        for update in updates {
            if let Some(update_id) = update["update_id"].as_i64() {
                self.last_update_id = Some(update_id);
            }
            if let Some(message) = update["message"].as_object() {
                if let Some(text) = message["text"].as_str() {
                    let message_id = message["message_id"].as_i64().unwrap_or(0);
                    let chat_id = message["chat"]["id"].as_i64().unwrap_or(0);
                    let date = message["date"].as_i64().unwrap_or(0);
                    let from_user = message["from"]["first_name"]
                        .as_str()
                        .map(|s| s.to_string());
                    let timestamp = DateTime::from_timestamp(date, 0).unwrap_or_else(Utc::now);
                    messages.push(TelegramMessage {
                        message_id,
                        from_user,
                        chat_id,
                        text: text.to_string(),
                        timestamp,
                        is_outgoing: false,
                    });
                }
            }
        }
        Ok(messages)
    }

    pub async fn discover_chats(&mut self) -> Result<Vec<(i64, String)>> {
        let url = format!("{}/getUpdates?limit=100&offset=-100", self.base_url);
        let response = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "HTTP {}: {}",
                response.status(),
                response.text().await?
            ));
        }
        let result: serde_json::Value = response.json().await?;
        if !result["ok"].as_bool().unwrap_or(false) {
            return Err(anyhow!(
                "API error: {}",
                result["description"].as_str().unwrap_or("Unknown")
            ));
        }

        let empty_vec = vec![];
        let updates = result["result"].as_array().unwrap_or(&empty_vec);
        let mut chats = std::collections::HashMap::new();
        for update in updates {
            if let Some(message) = update["message"].as_object() {
                if let Some(chat) = message["chat"].as_object() {
                    let chat_id = chat["id"].as_i64().unwrap_or(0);
                    let chat_type = chat["type"].as_str().unwrap_or("unknown");
                    let display_name = match chat_type {
                        "private" => {
                            let first_name = chat["first_name"].as_str().unwrap_or("Unknown");
                            format!("{first_name} (Private)")
                        }
                        "group" | "supergroup" => {
                            let title = chat["title"].as_str().unwrap_or("Group");
                            format!("{title} (Group)")
                        }
                        "channel" => {
                            let title = chat["title"].as_str().unwrap_or("Channel");
                            format!("{title} (Channel)")
                        }
                        _ => format!("Chat {chat_id}"),
                    };
                    chats.insert(chat_id, display_name);
                }
            }
        }
        Ok(chats.into_iter().collect())
    }
}
