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

use crate::llm_logging::{
    estimate_cost, LLMCallTracker, LLMLogger, LLMRequest, LLMResponse, TokenCount,
};
use async_trait::async_trait;
use dotenvy::dotenv;
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use stele::nlu::llm_processor::LLMAdapter;

#[derive(Clone, Debug)]
pub struct LoggingLLMAdapter {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub api_version: String,
    pub provider: String,
    pub client: Client,
    pub logger: Arc<LLMLogger>,
}

impl LoggingLLMAdapter {
    pub fn new(
        endpoint: String,
        api_key: String,
        model: String,
        max_tokens: usize,
        temperature: f32,
        api_version: String,
        provider: String,
        logger: Arc<LLMLogger>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let timeout = Duration::from_secs(60);
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

        Ok(Self {
            endpoint,
            api_key,
            model,
            max_tokens,
            temperature,
            api_version,
            provider,
            client,
            logger,
        })
    }

    pub fn anthropic(logger: Arc<LLMLogger>) -> Result<Self, Box<dyn std::error::Error>> {
        dotenv().ok();

        let endpoint = std::env::var("ANTHROPIC_ENDPOINT")
            .unwrap_or_else(|_| "https://api.anthropic.com/v1/messages".to_string());
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| "ANTHROPIC_API_KEY environment variable not set")?;
        let model = std::env::var("ANTHROPIC_MODEL")
            .unwrap_or_else(|_| "claude-3-5-haiku-latest".to_string());
        let max_tokens = std::env::var("ANTHROPIC_MAX_TOKENS")
            .unwrap_or_else(|_| "8192".to_string())
            .parse()
            .unwrap_or(8192);
        let temperature = std::env::var("ANTHROPIC_TEMPERATURE")
            .unwrap_or_else(|_| "0.2".to_string())
            .parse()
            .unwrap_or(0.2);
        let api_version =
            std::env::var("ANTHROPIC_API_VERSION").unwrap_or_else(|_| "2023-06-01".to_string());

        Self::new(
            endpoint,
            api_key,
            model,
            max_tokens,
            temperature,
            api_version,
            "anthropic".to_string(),
            logger,
        )
    }

    pub fn openai(logger: Arc<LLMLogger>) -> Result<Self, Box<dyn std::error::Error>> {
        dotenv().ok();

        let endpoint = std::env::var("OPENAI_ENDPOINT")
            .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".to_string());
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| "OPENAI_API_KEY environment variable not set")?;
        let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4".to_string());
        let max_tokens = std::env::var("OPENAI_MAX_TOKENS")
            .unwrap_or_else(|_| "8192".to_string())
            .parse()
            .unwrap_or(8192);
        let temperature = std::env::var("OPENAI_TEMPERATURE")
            .unwrap_or_else(|_| "0.7".to_string())
            .parse()
            .unwrap_or(0.7);

        Self::new(
            endpoint,
            api_key,
            model,
            max_tokens,
            temperature,
            "".to_string(),
            "openai".to_string(),
            logger,
        )
    }

    pub fn ollama(logger: Arc<LLMLogger>) -> Result<Self, Box<dyn std::error::Error>> {
        dotenv().ok();

        let endpoint = std::env::var("OLLAMA_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:11434/api/chat".to_string());
        let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2:3b".to_string());
        let max_tokens = std::env::var("OLLAMA_MAX_TOKENS")
            .unwrap_or_else(|_| "4096".to_string())
            .parse()
            .unwrap_or(4096);
        let temperature = std::env::var("OLLAMA_TEMPERATURE")
            .unwrap_or_else(|_| "0.7".to_string())
            .parse()
            .unwrap_or(0.7);

        Self::new(
            endpoint,
            "N/A".to_string(),
            model,
            max_tokens,
            temperature,
            "v1".to_string(),
            "ollama".to_string(),
            logger,
        )
    }

    async fn send_anthropic_request(
        &self,
        payload: Value,
        tracker: LLMCallTracker,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let response = self
            .client
            .post(&self.endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.api_version)
            .header("content-type", "application/json")
            .json(&payload)
            .send()
            .await;

        let response = match response {
            Ok(resp) => resp,
            Err(e) => {
                let error_msg = format!("Request failed: {e}");
                tracker.complete_error(error_msg.clone(), json!({"error_type": "network"}));
                return Err(error_msg.into());
            }
        };

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            let error_msg = format!("HTTP {status} - {error_body}");
            tracker.complete_error(
                error_msg.clone(),
                json!({"error_type": "api", "status_code": status.as_u16(), "response_body": error_body}),
            );
            return Err(error_msg.into());
        }

        let response_text = match response.text().await {
            Ok(text) => text,
            Err(e) => {
                let error_msg = format!("Failed to read response: {e}");
                tracker.complete_error(error_msg.clone(), json!({"error_type": "parsing"}));
                return Err(error_msg.into());
            }
        };

        let response_data: Value = match serde_json::from_str(&response_text) {
            Ok(data) => data,
            Err(e) => {
                let error_msg = format!("Failed to parse response JSON: {e}");
                tracker.complete_error(
                    error_msg.clone(),
                    json!({"error_type": "json_parsing", "response_text": response_text}),
                );
                return Err(error_msg.into());
            }
        };

        let content = match response_data["content"][0]["text"].as_str() {
            Some(text) => text,
            None => {
                let error_msg = "Failed to extract content from Anthropic response".to_string();
                tracker.complete_error(
                    error_msg.clone(),
                    json!({"error_type": "content_extraction", "response_data": response_data}),
                );
                return Err(error_msg.into());
            }
        };

        let usage = response_data.get("usage");
        let tokens = usage.map(|u| TokenCount {
            prompt_tokens: u["input_tokens"].as_u64().map(|t| t as usize),
            completion_tokens: u["output_tokens"].as_u64().map(|t| t as usize),
            total_tokens: u["input_tokens"]
                .as_u64()
                .and_then(|i| u["output_tokens"].as_u64().map(|o| (i + o) as usize)),
        });

        let cost_estimate = tokens
            .as_ref()
            .and_then(|t| estimate_cost(&self.provider, &self.model, t));

        let llm_response = LLMResponse {
            content: content.to_string(),
            raw_response: response_data.clone(),
            finish_reason: response_data["stop_reason"].as_str().map(|s| s.to_string()),
        };

        tracker.complete_success(
            llm_response,
            tokens,
            cost_estimate,
            json!({"api_provider": "anthropic", "model": self.model}),
        );

        Ok(content.to_string())
    }

    async fn send_openai_request(
        &self,
        payload: Value,
        tracker: LLMCallTracker,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let response = self
            .client
            .post(&self.endpoint)
            .header("Authorisation", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&payload)
            .send()
            .await;

        let response = match response {
            Ok(resp) => resp,
            Err(e) => {
                let error_msg = format!("Request failed: {e}");
                tracker.complete_error(error_msg.clone(), json!({"error_type": "network"}));
                return Err(error_msg.into());
            }
        };

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            let error_msg = format!("HTTP {status} - {error_body}");
            tracker.complete_error(
                error_msg.clone(),
                json!({"error_type": "api", "status_code": status.as_u16(), "response_body": error_body}),
            );
            return Err(error_msg.into());
        }

        let response_text = match response.text().await {
            Ok(text) => text,
            Err(e) => {
                let error_msg = format!("Failed to read response: {e}");
                tracker.complete_error(error_msg.clone(), json!({"error_type": "parsing"}));
                return Err(error_msg.into());
            }
        };

        let response_data: Value = match serde_json::from_str(&response_text) {
            Ok(data) => data,
            Err(e) => {
                let error_msg = format!("Failed to parse response JSON: {e}");
                tracker.complete_error(
                    error_msg.clone(),
                    json!({"error_type": "json_parsing", "response_text": response_text}),
                );
                return Err(error_msg.into());
            }
        };

        let content = match response_data["choices"][0]["message"]["content"].as_str() {
            Some(text) => text,
            None => {
                let error_msg = "Failed to extract content from OpenAI response".to_string();
                tracker.complete_error(
                    error_msg.clone(),
                    json!({"error_type": "content_extraction", "response_data": response_data}),
                );
                return Err(error_msg.into());
            }
        };

        let usage = response_data.get("usage");
        let tokens = usage.map(|u| TokenCount {
            prompt_tokens: u["prompt_tokens"].as_u64().map(|t| t as usize),
            completion_tokens: u["completion_tokens"].as_u64().map(|t| t as usize),
            total_tokens: u["total_tokens"].as_u64().map(|t| t as usize),
        });

        let cost_estimate = tokens
            .as_ref()
            .and_then(|t| estimate_cost(&self.provider, &self.model, t));

        let llm_response = LLMResponse {
            content: content.to_string(),
            raw_response: response_data.clone(),
            finish_reason: response_data["choices"][0]["finish_reason"]
                .as_str()
                .map(|s| s.to_string()),
        };

        tracker.complete_success(
            llm_response,
            tokens,
            cost_estimate,
            json!({"api_provider": "openai", "model": self.model}),
        );

        Ok(content.to_string())
    }

    async fn send_ollama_request(
        &self,
        payload: Value,
        tracker: LLMCallTracker,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let response = self
            .client
            .post(&self.endpoint)
            .header("content-type", "application/json")
            .json(&payload)
            .send()
            .await;

        let response = match response {
            Ok(resp) => resp,
            Err(e) => {
                let error_msg = format!("Request failed: {e}");
                tracker.complete_error(error_msg.clone(), json!({"error_type": "network"}));
                return Err(error_msg.into());
            }
        };

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            let error_msg = format!("HTTP {status} - {error_body}");
            tracker.complete_error(
                error_msg.clone(),
                json!({"error_type": "api", "status_code": status.as_u16(), "response_body": error_body}),
            );
            return Err(error_msg.into());
        }

        let response_text = match response.text().await {
            Ok(text) => text,
            Err(e) => {
                let error_msg = format!("Failed to read response: {e}");
                tracker.complete_error(error_msg.clone(), json!({"error_type": "parsing"}));
                return Err(error_msg.into());
            }
        };

        let response_data: Value = match serde_json::from_str(&response_text) {
            Ok(data) => data,
            Err(e) => {
                let error_msg = format!("Failed to parse response JSON: {e}");
                tracker.complete_error(
                    error_msg.clone(),
                    json!({"error_type": "json_parsing", "response_text": response_text}),
                );
                return Err(error_msg.into());
            }
        };

        let content = match response_data["message"]["content"].as_str() {
            Some(text) => text,
            None => {
                let error_msg = "Failed to extract content from Ollama response".to_string();
                tracker.complete_error(
                    error_msg.clone(),
                    json!({"error_type": "content_extraction", "response_data": response_data}),
                );
                return Err(error_msg.into());
            }
        };

        let usage = response_data.get("usage");
        let tokens = usage.map(|u| TokenCount {
            prompt_tokens: u
                .get("prompt_tokens")
                .and_then(|t| t.as_u64().map(|v| v as usize)),
            completion_tokens: u
                .get("completion_tokens")
                .and_then(|t| t.as_u64().map(|v| v as usize)),
            total_tokens: u
                .get("total_tokens")
                .and_then(|t| t.as_u64().map(|v| v as usize)),
        });

        let cost_estimate = None;

        let llm_response = LLMResponse {
            content: content.to_string(),
            raw_response: response_data.clone(),
            finish_reason: response_data
                .get("done_reason")
                .and_then(|s| s.as_str())
                .map(String::from),
        };

        tracker.complete_success(
            llm_response,
            tokens,
            cost_estimate,
            json!({"api_provider": "ollama", "model": self.model}),
        );

        Ok(content.to_string())
    }

    async fn execute_request(&self, body: Value) -> Result<LLMResponse, String> {
        let response = self
            .client
            .post(&self.endpoint)
            .header("Authorisation", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            let error_msg = format!("HTTP {status} - {error_body}");
            return Err(error_msg);
        }

        let response_text = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {e}"))?;

        let response_data: Value = serde_json::from_str(&response_text)
            .map_err(|e| format!("Failed to parse response JSON: {e}"))?;

        let content = response_data["choices"][0]["message"]["content"]
            .as_str()
            .ok_or("Failed to extract content from response")?;

        let usage = response_data.get("usage");
        let tokens = usage.map(|u| TokenCount {
            prompt_tokens: u["prompt_tokens"].as_u64().map(|t| t as usize),
            completion_tokens: u["completion_tokens"].as_u64().map(|t| t as usize),
            total_tokens: u["total_tokens"].as_u64().map(|t| t as usize),
        });

        let cost_estimate = tokens
            .as_ref()
            .and_then(|t| estimate_cost(&self.provider, &self.model, t));

        Ok(LLMResponse {
            content: content.to_string(),
            raw_response: response_data.clone(),
            finish_reason: response_data["choices"][0]["finish_reason"]
                .as_str()
                .map(|s| s.to_string()),
        })
    }
}

#[async_trait]
impl LLMAdapter for LoggingLLMAdapter {
    async fn process_text(&self, input: &str) -> Result<String, Box<dyn std::error::Error>> {
        let payload = match self.provider.as_str() {
            "anthropic" => json!({
                "model": self.model,
                "max_tokens": self.max_tokens,
                "messages": [{"role": "user", "content": input}],
                "temperature": self.temperature
            }),
            "openai" => json!({
                "model": self.model,
                "max_tokens": self.max_tokens,
                "messages": [{"role": "user", "content": input}],
                "temperature": self.temperature
            }),
            "ollama" => json!({
                "model": self.model,
                "messages": [{"role": "user", "content": input}],
                "stream": false,
                "options": {
                    "temperature": self.temperature,
                    "num_predict": self.max_tokens
                }
            }),
            _ => return Err(format!("Unsupported provider: {}", self.provider).into()),
        };

        let request = LLMRequest {
            prompt: input.to_string(),
            system_prompt: None,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            raw_payload: payload.clone(),
        };

        let tracker = self.logger.start_call(
            "data_processor",
            "process_text",
            &self.provider,
            &self.model,
            request,
        );

        match self.provider.as_str() {
            "anthropic" => self.send_anthropic_request(payload, tracker).await,
            "openai" => self.send_openai_request(payload, tracker).await,
            "ollama" => self.send_ollama_request(payload, tracker).await,
            _ => unreachable!(),
        }
    }

    async fn generate_response(&self, prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
        self.process_text(prompt).await
    }
}
