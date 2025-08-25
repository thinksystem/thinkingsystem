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

use async_trait::async_trait;
use dotenvy::dotenv;
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationConfig {
    pub max_history_entries: usize,
    pub max_context_length: usize,
    pub system_prompt: Option<String>,
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            max_history_entries: 10,
            max_context_length: 8000,
            system_prompt: None,
        }
    }
}

pub struct LLMProcessor {
    adapter: Box<dyn LLMAdapter>,
    config: ConversationConfig,
    conversation_history: Vec<String>,
}
static HTTP_CLIENT: Lazy<Client> = Lazy::new(|| Client::builder().build().expect("HTTP client"));

impl LLMProcessor {
    pub fn new(adapter: Box<dyn LLMAdapter>, config: ConversationConfig) -> Self {
        Self {
            adapter,
            config,
            conversation_history: Vec::new(),
        }
    }

    pub async fn process_message(
        &mut self,
        message: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let context = if let Some(system_prompt) = &self.config.system_prompt {
            format!("{system_prompt}\n\n{message}")
        } else {
            message.to_string()
        };

        let response = self.adapter.generate_response(&context).await?;

        self.conversation_history.push(format!("User: {message}"));
        self.conversation_history
            .push(format!("Assistant: {response}"));

        while self.conversation_history.len() > self.config.max_history_entries {
            self.conversation_history.remove(0);
        }

        Ok(response)
    }

    pub fn clear_history(&mut self) {
        self.conversation_history.clear();
    }
}

#[derive(Clone, Debug)]
pub struct CustomLLMAdapter {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub api_version: String,
}
impl CustomLLMAdapter {
    pub fn new(model_name: String, max_tokens: usize, temperature: f32) -> Self {
        Self {
            model: model_name,
            max_tokens,
            temperature,
            ..Default::default()
        }
    }

    pub fn anthropic() -> Result<Self, Box<dyn std::error::Error>> {
        dotenv().ok();
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| "ANTHROPIC_API_KEY environment variable not set")?;

        Ok(Self {
            endpoint: std::env::var("ANTHROPIC_ENDPOINT")
                .unwrap_or_else(|_| "https://api.anthropic.com/v1/messages".to_string()),
            api_key,
            model: std::env::var("ANTHROPIC_MODEL")
                .unwrap_or_else(|_| "claude-3-5-haiku-latest".to_string()),
            max_tokens: std::env::var("ANTHROPIC_MAX_TOKENS")
                .unwrap_or_else(|_| "50000".to_string())
                .parse()
                .unwrap_or(50000),
            temperature: std::env::var("ANTHROPIC_TEMPERATURE")
                .unwrap_or_else(|_| "0.7".to_string())
                .parse()
                .unwrap_or(0.7),
            api_version: std::env::var("ANTHROPIC_API_VERSION")
                .unwrap_or_else(|_| "2023-06-01".to_string()),
        })
    }

    pub fn ollama(model: String) -> Result<Self, Box<dyn std::error::Error>> {
        dotenv().ok();

        Ok(Self {
            endpoint: std::env::var("OLLAMA_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:11434/api/generate".to_string()),
            api_key: "".to_string(),
            model,
            max_tokens: std::env::var("OLLAMA_MAX_TOKENS")
                .unwrap_or_else(|_| "32768".to_string())
                .parse()
                .unwrap_or(32768),
            temperature: std::env::var("OLLAMA_TEMPERATURE")
                .unwrap_or_else(|_| "0.7".to_string())
                .parse()
                .unwrap_or(0.7),
            api_version: "".to_string(),
        })
    }

    fn get_provider(&self) -> &str {
        if self.endpoint.contains("anthropic.com") {
            "anthropic"
        } else if self.endpoint.contains("11434") || self.endpoint.contains("ollama") {
            "ollama"
        } else if self.endpoint.contains("openai.com") {
            "openai"
        } else {
            "anthropic"
        }
    }
}
impl Default for CustomLLMAdapter {
    fn default() -> Self {
        dotenv().ok();
        Self {
            endpoint: std::env::var("ANTHROPIC_ENDPOINT")
                .unwrap_or_else(|_| "https://api.anthropic.com/v1/messages".to_string()),
            api_key: std::env::var("ANTHROPIC_API_KEY").unwrap_or_else(|_| "test-key".to_string()),
            model: std::env::var("ANTHROPIC_MODEL")
                .unwrap_or_else(|_| "claude-3-5-haiku-latest".to_string()),
            max_tokens: std::env::var("ANTHROPIC_MAX_TOKENS")
                .unwrap_or_else(|_| "50000".to_string())
                .parse()
                .unwrap_or(50000),
            temperature: std::env::var("ANTHROPIC_TEMPERATURE")
                .unwrap_or_else(|_| "0.2".to_string())
                .parse()
                .unwrap_or(0.2),
            api_version: std::env::var("ANTHROPIC_API_VERSION")
                .unwrap_or_else(|_| "2023-06-01".to_string()),
        }
    }
}
#[async_trait]
pub trait LLMAdapter: Send + Sync {
    async fn process_text(&self, input: &str) -> Result<String, Box<dyn std::error::Error>>;
    async fn generate_response(&self, prompt: &str) -> Result<String, Box<dyn std::error::Error>>;

    async fn generate_structured_response(
        &self,
        system_prompt: &str,
        user_input: &str,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        let response = self
            .generate_response(&format!("System: {system_prompt}\n\nUser: {user_input}"))
            .await?;

        if let Some(json_str) = extract_json_from_response(&response) {
            match serde_json::from_str::<Value>(&json_str) {
                Ok(value) => return Ok(value),
                Err(e) => debug!("Failed to parse extracted JSON: {}", e),
            }
        }

        Ok(json!({"response": response}))
    }
}

fn extract_json_from_response(content: &str) -> Option<String> {
    if let Some(start) = content.find("```json") {
        if let Some(end) = content[start + 7..].find("```") {
            let json_block = &content[start + 7..start + 7 + end];
            if serde_json::from_str::<serde_json::Value>(json_block.trim()).is_ok() {
                return Some(json_block.trim().to_string());
            }
        }
    }

    if let Some(start_pos) = content.find('{') {
        let mut brace_count = 0;
        let mut in_string = false;
        let mut escape_next = false;

        for (i, char) in content[start_pos..].char_indices() {
            if escape_next {
                escape_next = false;
                continue;
            }

            match char {
                '"' if !escape_next => in_string = !in_string,
                '\\' if in_string => escape_next = true,
                '{' if !in_string => brace_count += 1,
                '}' if !in_string => {
                    brace_count -= 1;
                    if brace_count == 0 {
                        let json_str = &content[start_pos..start_pos + i + 1];
                        if serde_json::from_str::<serde_json::Value>(json_str).is_ok() {
                            return Some(json_str.to_string());
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    None
}

pub async fn generate_chat_response(
    adapter: &impl LLMAdapter,
    user_input: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let prompt = format!("User: {user_input}\nAssistant:");
    adapter.generate_response(&prompt).await
}
#[async_trait]
impl LLMAdapter for CustomLLMAdapter {
    async fn process_text(&self, input: &str) -> Result<String, Box<dyn std::error::Error>> {
        let client = &*HTTP_CLIENT;
        let provider = self.get_provider();

        let response = match provider {
            "anthropic" => {
                let payload = json!({
                    "model": self.model,
                    "max_tokens": self.max_tokens,
                    "messages": [{
                        "role": "user",
                        "content": input
                    }],
                    "temperature": self.temperature
                });
                debug!(payload = ?payload, "Sending request to Anthropic API");
                client
                    .post(&self.endpoint)
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", &self.api_version)
                    .header("content-type", "application/json")
                    .json(&payload)
                    .send()
                    .await?
            }
            "ollama" => {
                
                let mut num_predict = self.max_tokens;
                let lower = input.to_ascii_lowercase();
                let likely_json = lower.contains("return json")
                    || lower.contains("respond with json")
                    || lower.contains("re-emit strictly")
                    || lower.contains("structured response")
                    || lower.contains("json now");

                if likely_json {
                    let json_cap: usize = std::env::var("OLLAMA_JSON_MAX_TOKENS")
                        .unwrap_or_else(|_| "4096".to_string())
                        .parse()
                        .unwrap_or(4096);
                    num_predict = num_predict.min(json_cap);
                }

                debug!(likely_json, num_predict, "Ollama num_predict chosen");

                let payload = json!({
                    "model": self.model,
                    "prompt": input,
                    "stream": false,
                    "options": {
                        "temperature": self.temperature,
                        "num_predict": num_predict
                    }
                });
                debug!(payload = ?payload, "Sending request to Ollama API");
                client
                    .post(&self.endpoint)
                    .header("content-type", "application/json")
                    .json(&payload)
                    .send()
                    .await?
            }
            _ => {
                let payload = json!({
                    "model": self.model,
                    "max_tokens": self.max_tokens,
                    "messages": [{
                        "role": "user",
                        "content": input
                    }],
                    "temperature": self.temperature
                });
                client
                    .post(&self.endpoint)
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", &self.api_version)
                    .header("content-type", "application/json")
                    .json(&payload)
                    .send()
                    .await?
            }
        };

        let status = response.status();
        info!(%status, provider = %provider, "Received response from LLM API");

        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Err(format!("{provider} API error {status}: {error_body}").into());
        }

        let response_data: Value = response.json().await?;
        debug!(response_data = ?response_data, "Raw API Response");

        let content = match provider {
            "anthropic" => response_data["content"][0]["text"]
                .as_str()
                .ok_or("Failed to extract content from Anthropic response")?,
            "ollama" => response_data["response"]
                .as_str()
                .ok_or("Failed to extract content from Ollama response")?,
            _ => {
                if let Some(content) = response_data["content"][0]["text"].as_str() {
                    content
                } else if let Some(content) = response_data["response"].as_str() {
                    content
                } else {
                    return Err("Failed to extract content from response".into());
                }
            }
        };

        info!("Returning full content from LLM response");
        Ok(content.to_string())
    }

    async fn generate_response(&self, prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
        self.process_text(prompt).await
    }
}
