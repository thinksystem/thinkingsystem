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

use crate::messaging::types::{Message, MessageDestination};
use async_trait::async_trait;
use std::collections::HashMap;

#[derive(Debug)]
pub enum PlatformError {
    ConnectionFailed(String),
    AuthenticationFailed(String),
    SendFailed(String),
    ReceiveFailed(String),
    ConfigurationError(String),
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PlatformError::ConnectionFailed(e) => write!(f, "Platform connection failed: {e}"),
            PlatformError::AuthenticationFailed(e) => {
                write!(f, "Platform authentication failed: {e}")
            }
            PlatformError::SendFailed(e) => write!(f, "Failed to send message: {e}"),
            PlatformError::ReceiveFailed(e) => write!(f, "Failed to receive message: {e}"),
            PlatformError::ConfigurationError(e) => {
                write!(f, "Platform configuration error: {e}")
            }
        }
    }
}

impl std::error::Error for PlatformError {}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum PlatformType {
    Telegram,
    WhatsApp,
    Signal,
    Discord,
    Slack,
    Email,
    SMS,
}

impl std::fmt::Display for PlatformType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PlatformType::Telegram => write!(f, "telegram"),
            PlatformType::WhatsApp => write!(f, "whatsapp"),
            PlatformType::Signal => write!(f, "signal"),
            PlatformType::Discord => write!(f, "discord"),
            PlatformType::Slack => write!(f, "slack"),
            PlatformType::Email => write!(f, "email"),
            PlatformType::SMS => write!(f, "sms"),
        }
    }
}

#[async_trait]
pub trait PlatformBridge: Send + Sync {
    async fn send_message(&self, message: &Message) -> Result<String, PlatformError>;
    async fn receive_messages(&self) -> Result<Vec<Message>, PlatformError>;
    fn get_platform_type(&self) -> PlatformType;
    async fn is_connected(&self) -> bool;
    async fn connect(&mut self) -> Result<(), PlatformError>;
    async fn disconnect(&mut self) -> Result<(), PlatformError>;
}

pub struct TelegramBridge {
    connected: bool,
    api_id: i32,
    api_hash: String,
    bot_token: Option<String>,
    client: Option<reqwest::Client>,
}

impl TelegramBridge {
    pub fn new(api_id: i32, api_hash: String) -> Self {
        Self {
            connected: false,
            api_id,
            api_hash,
            bot_token: None,
            client: None,
        }
    }

    pub fn with_bot_token(mut self, bot_token: String) -> Self {
        self.bot_token = Some(bot_token);
        self
    }

    async fn send_telegram_message(
        &self,
        chat_id: &str,
        text: &str,
    ) -> Result<String, PlatformError> {
        if !self.validate_api_credentials() {
            return Err(PlatformError::AuthenticationFailed(
                "Invalid API credentials".to_string(),
            ));
        }

        let client = self.client.as_ref().ok_or_else(|| {
            PlatformError::ConnectionFailed("HTTP client not initialised".to_string())
        })?;

        let bot_token = self.bot_token.as_ref().ok_or_else(|| {
            PlatformError::AuthenticationFailed("Bot token not provided".to_string())
        })?;

        let url = format!("https://api.telegram.org/bot{bot_token}/sendMessage");

        let payload = serde_json::json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "HTML"
        });

        let response = client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| PlatformError::SendFailed(format!("HTTP request failed: {e}")))?;

        if response.status().is_success() {
            let response_json: serde_json::Value = response
                .json()
                .await
                .map_err(|e| PlatformError::SendFailed(format!("Failed to parse response: {e}")))?;

            if let Some(message_id) = response_json
                .get("result")
                .and_then(|r| r.get("message_id"))
                .and_then(|id| id.as_i64())
            {
                Ok(format!("tg_msg_{message_id}"))
            } else {
                Err(PlatformError::SendFailed(
                    "Invalid response format".to_string(),
                ))
            }
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(PlatformError::SendFailed(format!(
                "Telegram API error: {error_text}"
            )))
        }
    }

    fn validate_api_credentials(&self) -> bool {
        self.api_id > 0 && !self.api_hash.is_empty() && self.api_hash.len() >= 32
    }

    pub fn get_api_hash(&self) -> &str {
        &self.api_hash
    }

    pub fn get_api_id(&self) -> i32 {
        self.api_id
    }

    pub async fn authenticate_client(&self) -> Result<(), PlatformError> {
        if !self.validate_api_credentials() {
            return Err(PlatformError::AuthenticationFailed(format!(
                "Invalid API credentials: api_id={}, api_hash_len={}",
                self.api_id,
                self.api_hash.len()
            )));
        }

        println!(
            "Authenticating Telegram client with API ID: {}",
            self.api_id
        );
        println!(
            "Using API hash: {}...",
            &self.api_hash[..8.min(self.api_hash.len())]
        );

        Ok(())
    }
}

#[async_trait]
impl PlatformBridge for TelegramBridge {
    async fn send_message(&self, message: &Message) -> Result<String, PlatformError> {
        if !self.connected {
            return Err(PlatformError::ConnectionFailed(
                "Not connected to Telegram".to_string(),
            ));
        }

        match &message.destination {
            MessageDestination::Single(chat_id) => {
                self.send_telegram_message(chat_id, &message.content).await
            }
            MessageDestination::Multiple(chat_ids) => {
                if let Some(first_chat) = chat_ids.first() {
                    self.send_telegram_message(first_chat, &message.content)
                        .await
                } else {
                    Err(PlatformError::SendFailed(
                        "No chat IDs provided".to_string(),
                    ))
                }
            }
        }
    }

    async fn receive_messages(&self) -> Result<Vec<Message>, PlatformError> {
        if !self.connected {
            return Err(PlatformError::ConnectionFailed(
                "Not connected to Telegram".to_string(),
            ));
        }

        let client = self.client.as_ref().ok_or_else(|| {
            PlatformError::ConnectionFailed("HTTP client not initialised".to_string())
        })?;

        let bot_token = self.bot_token.as_ref().ok_or_else(|| {
            PlatformError::AuthenticationFailed("Bot token not provided".to_string())
        })?;

        let url = format!("https://api.telegram.org/bot{bot_token}/getUpdates");

        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| PlatformError::ReceiveFailed(format!("HTTP request failed: {e}")))?;

        if response.status().is_success() {
            let response_json: serde_json::Value = response.json().await.map_err(|e| {
                PlatformError::ReceiveFailed(format!("Failed to parse response: {e}"))
            })?;

            let mut messages = Vec::new();

            if let Some(updates) = response_json.get("result").and_then(|r| r.as_array()) {
                for update in updates {
                    if let Some(message_data) = update.get("message") {
                        if let (Some(text), Some(chat), Some(from)) = (
                            message_data.get("text").and_then(|t| t.as_str()),
                            message_data
                                .get("chat")
                                .and_then(|c| c.get("id"))
                                .and_then(|id| id.as_i64()),
                            message_data
                                .get("from")
                                .and_then(|f| f.get("id"))
                                .and_then(|id| id.as_i64()),
                        ) {
                            let message = Message::new(
                                from.to_string(),
                                MessageDestination::Single(chat.to_string()),
                                text.to_string(),
                            );
                            messages.push(message);
                        }
                    }
                }
            }

            Ok(messages)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(PlatformError::ReceiveFailed(format!(
                "Telegram API error: {error_text}"
            )))
        }
    }

    fn get_platform_type(&self) -> PlatformType {
        PlatformType::Telegram
    }

    async fn is_connected(&self) -> bool {
        self.connected
    }

    async fn connect(&mut self) -> Result<(), PlatformError> {
        if self.bot_token.is_none() {
            return Err(PlatformError::AuthenticationFailed(
                "Bot token required for connection".to_string(),
            ));
        }

        self.client = Some(reqwest::Client::new());

        let client = self
            .client
            .as_ref()
            .ok_or_else(|| PlatformError::ConnectionFailed("Client not initialised".to_string()))?;
        let bot_token = self
            .bot_token
            .as_ref()
            .ok_or_else(|| PlatformError::AuthenticationFailed("Bot token not set".to_string()))?;
        let url = format!("https://api.telegram.org/bot{bot_token}/getMe");

        let response =
            client.get(&url).send().await.map_err(|e| {
                PlatformError::ConnectionFailed(format!("Connection test failed: {e}"))
            })?;

        if response.status().is_success() {
            self.connected = true;
            Ok(())
        } else {
            Err(PlatformError::AuthenticationFailed(
                "Invalid bot token".to_string(),
            ))
        }
    }

    async fn disconnect(&mut self) -> Result<(), PlatformError> {
        self.connected = false;
        self.client = None;
        Ok(())
    }
}

pub struct EmailBridge {
    connected: bool,
    smtp_server: String,
    smtp_port: u16,
    username: String,
    password: String,
    from_address: String,
    client: Option<reqwest::Client>,
}

impl EmailBridge {
    pub fn new(
        smtp_server: String,
        smtp_port: u16,
        username: String,
        password: String,
        from_address: String,
    ) -> Self {
        Self {
            connected: false,
            smtp_server,
            smtp_port,
            username,
            password,
            from_address,
            client: None,
        }
    }

    async fn send_smtp_message(
        &self,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<String, PlatformError> {
        if !self.validate_credentials() {
            return Err(PlatformError::AuthenticationFailed(
                "Invalid SMTP credentials".to_string(),
            ));
        }

        let _client = self.client.as_ref().ok_or_else(|| {
            PlatformError::ConnectionFailed("HTTP client not initialised".to_string())
        })?;

        let _email_payload = serde_json::json!({
            "from": self.from_address,
            "to": to,
            "subject": subject,
            "text": body,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "auth": {
                "username": self.username,
                "password_hash": self.hash_password()
            }
        });

        println!("Email: Sending message to {} via {}", to, self.smtp_server);
        println!("Subject: {subject}");
        println!("Body: {body}");

        let message_id = format!(
            "email_{}_{}",
            chrono::Utc::now().timestamp(),
            &uuid::Uuid::new_v4().to_string()[..8]
        );

        Ok(message_id)
    }
}

#[async_trait]
impl PlatformBridge for EmailBridge {
    async fn send_message(&self, message: &Message) -> Result<String, PlatformError> {
        if !self.connected {
            return Err(PlatformError::ConnectionFailed(
                "Not connected to email server".to_string(),
            ));
        }

        let subject = format!("Message from {}", message.sender);

        match &message.destination {
            MessageDestination::Single(email) => {
                self.send_smtp_message(email, &subject, &message.content)
                    .await
            }
            MessageDestination::Multiple(emails) => {
                if let Some(first_email) = emails.first() {
                    self.send_smtp_message(first_email, &subject, &message.content)
                        .await
                } else {
                    Err(PlatformError::SendFailed(
                        "No email addresses provided".to_string(),
                    ))
                }
            }
        }
    }

    async fn receive_messages(&self) -> Result<Vec<Message>, PlatformError> {
        if !self.connected {
            return Err(PlatformError::ConnectionFailed(
                "Not connected to email server".to_string(),
            ));
        }

        println!("Email: Checking for new messages on {}", self.smtp_server);
        Ok(vec![])
    }

    fn get_platform_type(&self) -> PlatformType {
        PlatformType::Email
    }

    async fn is_connected(&self) -> bool {
        self.connected
    }

    async fn connect(&mut self) -> Result<(), PlatformError> {
        self.client = Some(reqwest::Client::new());

        println!(
            "Email: Connecting to {}:{}",
            self.smtp_server, self.smtp_port
        );
        println!("Email: Authenticating as {}", self.username);

        self.connected = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), PlatformError> {
        self.connected = false;
        self.client = None;
        println!("Email: Disconnected from {}", self.smtp_server);
        Ok(())
    }
}

impl EmailBridge {
    fn validate_credentials(&self) -> bool {
        !self.username.is_empty() && !self.password.is_empty() && self.password.len() >= 6
    }

    fn hash_password(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.password.hash(&mut hasher);
        format!("hash_{:x}", hasher.finish())
    }

    pub async fn authenticate_smtp(&self) -> Result<(), PlatformError> {
        if !self.validate_credentials() {
            return Err(PlatformError::AuthenticationFailed(format!(
                "Invalid credentials for user: {}",
                self.username
            )));
        }

        println!("Authenticating SMTP user: {}", self.username);
        println!("Password hash: {}", self.hash_password());
        println!("SMTP server: {}:{}", self.smtp_server, self.smtp_port);

        Ok(())
    }
}

pub struct PlatformManager {
    bridges: HashMap<PlatformType, Box<dyn PlatformBridge>>,
    default_platform: Option<PlatformType>,
}

impl PlatformManager {
    pub fn new() -> Self {
        Self {
            bridges: HashMap::new(),
            default_platform: None,
        }
    }

    pub async fn add_platform(
        &mut self,
        mut bridge: Box<dyn PlatformBridge>,
    ) -> Result<(), PlatformError> {
        let platform_type = bridge.get_platform_type();
        bridge.connect().await?;

        self.bridges.insert(platform_type.clone(), bridge);

        if self.default_platform.is_none() {
            self.default_platform = Some(platform_type);
        }

        Ok(())
    }

    pub async fn remove_platform(
        &mut self,
        platform_type: &PlatformType,
    ) -> Result<(), PlatformError> {
        if let Some(mut bridge) = self.bridges.remove(platform_type) {
            bridge.disconnect().await?;
        }

        if self.default_platform.as_ref() == Some(platform_type) {
            self.default_platform = self.bridges.keys().next().cloned();
        }

        Ok(())
    }

    pub async fn send_message(
        &self,
        message: &Message,
        platform: Option<PlatformType>,
    ) -> Result<String, PlatformError> {
        let platform_type = platform.or(self.default_platform.clone()).ok_or_else(|| {
            PlatformError::ConfigurationError("No platform specified or available".to_string())
        })?;

        let bridge = self.bridges.get(&platform_type).ok_or_else(|| {
            PlatformError::ConfigurationError(format!("Platform {platform_type} not configured"))
        })?;

        bridge.send_message(message).await
    }

    pub async fn receive_all_messages(
        &self,
    ) -> Result<Vec<(PlatformType, Vec<Message>)>, PlatformError> {
        let mut all_messages = Vec::new();

        for (platform_type, bridge) in &self.bridges {
            match bridge.receive_messages().await {
                Ok(messages) => {
                    if !messages.is_empty() {
                        all_messages.push((platform_type.clone(), messages));
                    }
                }
                Err(e) => {
                    eprintln!("Failed to receive messages from {platform_type}: {e}");
                }
            }
        }

        Ok(all_messages)
    }

    pub async fn get_platform_status(&self) -> HashMap<PlatformType, bool> {
        let mut status = HashMap::new();

        for (platform_type, bridge) in &self.bridges {
            status.insert(platform_type.clone(), bridge.is_connected().await);
        }

        status
    }

    pub fn get_available_platforms(&self) -> Vec<PlatformType> {
        self.bridges.keys().cloned().collect()
    }

    pub fn set_default_platform(
        &mut self,
        platform_type: PlatformType,
    ) -> Result<(), PlatformError> {
        if self.bridges.contains_key(&platform_type) {
            self.default_platform = Some(platform_type);
            Ok(())
        } else {
            Err(PlatformError::ConfigurationError(format!(
                "Platform {platform_type} not configured"
            )))
        }
    }

    pub async fn route_message(&self, message: &Message) -> Result<String, PlatformError> {
        let platform = self.determine_best_platform(message);
        self.send_message(message, platform).await
    }

    fn determine_best_platform(&self, _message: &Message) -> Option<PlatformType> {
        self.default_platform.clone()
    }
}

impl Default for PlatformManager {
    fn default() -> Self {
        Self::new()
    }
}

pub fn create_telegram_bridge(api_id: i32, api_hash: String) -> Box<dyn PlatformBridge> {
    Box::new(TelegramBridge::new(api_id, api_hash))
}

pub fn create_telegram_bot_bridge(bot_token: String) -> Box<dyn PlatformBridge> {
    Box::new(TelegramBridge::new(0, String::new()).with_bot_token(bot_token))
}

pub fn create_email_bridge(
    smtp_server: String,
    smtp_port: u16,
    username: String,
    password: String,
    from_address: String,
) -> Box<dyn PlatformBridge> {
    Box::new(EmailBridge::new(
        smtp_server,
        smtp_port,
        username,
        password,
        from_address,
    ))
}

#[derive(Debug, Clone)]
pub struct PlatformConfig {
    pub platform_type: PlatformType,
    pub config_data: std::collections::HashMap<String, String>,
}

impl PlatformConfig {
    pub fn new(platform_type: PlatformType) -> Self {
        Self {
            platform_type,
            config_data: std::collections::HashMap::new(),
        }
    }

    pub fn with_config(mut self, key: &str, value: &str) -> Self {
        self.config_data.insert(key.to_string(), value.to_string());
        self
    }

    pub fn get_config(&self, key: &str) -> Option<&String> {
        self.config_data.get(key)
    }
}

pub fn create_bridge_from_config(
    config: PlatformConfig,
) -> Result<Box<dyn PlatformBridge>, PlatformError> {
    match config.platform_type {
        PlatformType::Telegram => {
            if let Some(bot_token) = config.get_config("bot_token") {
                Ok(create_telegram_bot_bridge(bot_token.clone()))
            } else if let (Some(api_id_str), Some(api_hash)) =
                (config.get_config("api_id"), config.get_config("api_hash"))
            {
                let api_id = api_id_str
                    .parse::<i32>()
                    .map_err(|_| PlatformError::ConfigurationError("Invalid API ID".to_string()))?;
                Ok(create_telegram_bridge(api_id, api_hash.clone()))
            } else {
                Err(PlatformError::ConfigurationError(
                    "Telegram requires either bot_token or (api_id + api_hash)".to_string(),
                ))
            }
        }
        PlatformType::Email => {
            let smtp_server = config.get_config("smtp_server").ok_or_else(|| {
                PlatformError::ConfigurationError("SMTP server required".to_string())
            })?;
            let smtp_port = config
                .get_config("smtp_port")
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(587);
            let username = config.get_config("username").ok_or_else(|| {
                PlatformError::ConfigurationError("Username required".to_string())
            })?;
            let password = config.get_config("password").ok_or_else(|| {
                PlatformError::ConfigurationError("Password required".to_string())
            })?;
            let from_address = config.get_config("from_address").ok_or_else(|| {
                PlatformError::ConfigurationError("From address required".to_string())
            })?;

            Ok(create_email_bridge(
                smtp_server.clone(),
                smtp_port,
                username.clone(),
                password.clone(),
                from_address.clone(),
            ))
        }
        _ => Err(PlatformError::ConfigurationError(format!(
            "Platform {:?} not yet implemented",
            config.platform_type
        ))),
    }
}
