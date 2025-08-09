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

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use stele::llm::unified_adapter::UnifiedLLMAdapter;
use stele::nlu::llm_processor::LLMAdapter;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct OllamaConfig {
    pub base_url: String,
    pub timeout_secs: u64,
    pub default_model: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            timeout_secs: 60,
            default_model: "llama3.2:3b".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    pub modified_at: Option<String>,
    pub size: Option<u64>,
    pub digest: Option<String>,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateResponse {
    pub model: String,
    pub created_at: String,
    pub response: String,
    pub done: bool,
    pub context: Option<Vec<u32>>,
    pub total_duration: Option<u64>,
    pub load_duration: Option<u64>,
    pub prompt_eval_count: Option<u32>,
    pub prompt_eval_duration: Option<u64>,
    pub eval_count: Option<u32>,
    pub eval_duration: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub model: String,
    pub created_at: String,
    pub message: ChatMessage,
    pub done: bool,
    pub total_duration: Option<u64>,
    pub load_duration: Option<u64>,
    pub prompt_eval_count: Option<u32>,
    pub prompt_eval_duration: Option<u64>,
    pub eval_count: Option<u32>,
    pub eval_duration: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsResponse {
    pub models: Vec<OllamaModel>,
}

pub struct LocalLLMInterface {
    llm_adapter: Arc<UnifiedLLMAdapter>,
    client: Client,
    config: OllamaConfig,
}

impl LocalLLMInterface {
    pub fn new(llm_adapter: Arc<UnifiedLLMAdapter>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client for LocalLLMInterface");

        Self {
            llm_adapter,
            client,
            config: OllamaConfig::default(),
        }
    }

    pub fn with_config(llm_adapter: Arc<UnifiedLLMAdapter>, config: OllamaConfig) -> Self {
        let timeout = Duration::from_secs(config.timeout_secs);
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .expect("Failed to create HTTP client for LocalLLMInterface");

        Self {
            llm_adapter,
            client,
            config,
        }
    }

    pub async fn test_connection(&self) -> Result<bool, String> {
        info!("Testing Ollama connection at {}", self.config.base_url);

        let url = format!("{}/api/tags", self.config.base_url);
        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    info!("✓ Ollama connection successful");
                    Ok(true)
                } else {
                    let status = response.status();
                    warn!("✗ Ollama connection failed with status: {}", status);
                    Err(format!("HTTP {status}"))
                }
            }
            Err(e) => {
                warn!("✗ Ollama connection failed: {}", e);
                Err(format!("Connection error: {e}"))
            }
        }
    }

    pub async fn get_available_models(&self) -> Result<Vec<OllamaModel>, String> {
        info!("Retrieving available models from Ollama");

        let url = format!("{}/api/tags", self.config.base_url);
        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<ModelsResponse>().await {
                        Ok(models_response) => {
                            info!("✓ Found {} available models", models_response.models.len());
                            for model in &models_response.models {
                                debug!("Available model: {}", model.name);
                            }
                            Ok(models_response.models)
                        }
                        Err(e) => {
                            error!("Failed to parse models response: {}", e);
                            Err(format!("Parse error: {e}"))
                        }
                    }
                } else {
                    let status = response.status();
                    error!("Failed to get models, status: {}", status);
                    Err(format!("HTTP {status}"))
                }
            }
            Err(e) => {
                error!("Failed to connect to Ollama: {}", e);
                Err(format!("Connection error: {e}"))
            }
        }
    }

    pub async fn is_model_available(&self, model_name: &str) -> Result<bool, String> {
        let models = self.get_available_models().await?;
        Ok(models.iter().any(|m| m.name == model_name))
    }

    pub async fn get_best_available_model(&self) -> Result<String, String> {
        let models = self.get_available_models().await?;

        if models.is_empty() {
            return Err("No models available".to_string());
        }

        if models.iter().any(|m| m.name == self.config.default_model) {
            info!(
                "Using configured default model: {}",
                self.config.default_model
            );
            Ok(self.config.default_model.clone())
        } else {
            let first_model = &models[0].name;
            info!("Default model not available, using: {}", first_model);
            Ok(first_model.clone())
        }
    }

    pub async fn generate_direct(
        &self,
        prompt: &str,
        model: Option<&str>,
    ) -> Result<GenerateResponse, String> {
        let model = model.unwrap_or(&self.config.default_model);
        info!("Generating response using model: {}", model);

        let url = format!("{}/api/generate", self.config.base_url);
        let payload = json!({
            "model": model,
            "prompt": prompt,
            "stream": false
        });

        debug!("Generate API payload: {}", payload);

        match self.client.post(&url).json(&payload).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<GenerateResponse>().await {
                        Ok(generate_response) => {
                            info!(
                                "✓ Generate API call successful, response length: {}",
                                generate_response.response.len()
                            );
                            debug!("Generate response: {}", generate_response.response);
                            Ok(generate_response)
                        }
                        Err(e) => {
                            error!("Failed to parse generate response: {}", e);
                            Err(format!("Parse error: {e}"))
                        }
                    }
                } else {
                    let status = response.status();
                    let error_body = response.text().await.unwrap_or_default();
                    error!(
                        "Generate API failed - Status: {}, Body: {}",
                        status, error_body
                    );
                    Err(format!("HTTP {status} - {error_body}"))
                }
            }
            Err(e) => {
                error!("Generate API request failed: {}", e);
                Err(format!("Request error: {e}"))
            }
        }
    }

    pub async fn chat_direct(
        &self,
        messages: Vec<ChatMessage>,
        model: Option<&str>,
    ) -> Result<ChatResponse, String> {
        let model = model.unwrap_or(&self.config.default_model);
        info!("Chat API call using model: {}", model);

        let url = format!("{}/api/chat", self.config.base_url);
        let payload = json!({
            "model": model,
            "messages": messages,
            "stream": false
        });

        debug!("Chat API payload: {}", payload);

        match self.client.post(&url).json(&payload).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<ChatResponse>().await {
                        Ok(chat_response) => {
                            info!(
                                "✓ Chat API call successful, response length: {}",
                                chat_response.message.content.len()
                            );
                            debug!("Chat response: {}", chat_response.message.content);
                            Ok(chat_response)
                        }
                        Err(e) => {
                            error!("Failed to parse chat response: {}", e);
                            Err(format!("Parse error: {e}"))
                        }
                    }
                } else {
                    let status = response.status();
                    let error_body = response.text().await.unwrap_or_default();
                    error!("Chat API failed - Status: {}, Body: {}", status, error_body);
                    Err(format!("HTTP {status} - {error_body}"))
                }
            }
            Err(e) => {
                error!("Chat API request failed: {}", e);
                Err(format!("Request error: {e}"))
            }
        }
    }

    pub async fn chat_simple(&self, message: &str, model: Option<&str>) -> Result<String, String> {
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: message.to_string(),
        }];

        let response = self.chat_direct(messages, model).await?;
        Ok(response.message.content)
    }

    pub async fn generate_simple(
        &self,
        prompt: &str,
        model: Option<&str>,
    ) -> Result<String, String> {
        let response = self.generate_direct(prompt, model).await?;
        Ok(response.response)
    }

    pub async fn query(&self, prompt: &str) -> Result<String, String> {
        info!("Querying local LLM via UnifiedLLMAdapter");

        self.llm_adapter.process_text(prompt).await.map_err(|e| {
            warn!("Local LLM query failed: {}", e);
            e.to_string()
        })
    }

    pub async fn query_robust(
        &self,
        prompt: &str,
        preferred_model: Option<&str>,
    ) -> Result<String, String> {
        info!("Starting robust query with multiple fallback strategies");

        match self.chat_simple(prompt, preferred_model).await {
            Ok(response) => {
                info!("✓ Robust query succeeded via direct chat API");
                return Ok(response);
            }
            Err(e) => {
                warn!("Chat API failed, trying generate API: {}", e);
            }
        }

        match self.generate_simple(prompt, preferred_model).await {
            Ok(response) => {
                info!("✓ Robust query succeeded via direct generate API");
                return Ok(response);
            }
            Err(e) => {
                warn!("Generate API failed, trying UnifiedLLMAdapter: {}", e);
            }
        }

        match self.query(prompt).await {
            Ok(response) => {
                info!("✓ Robust query succeeded via UnifiedLLMAdapter");
                Ok(response)
            }
            Err(e) => {
                error!("All query strategies failed");
                Err(format!("All query methods failed. Last error: {e}"))
            }
        }
    }

    pub async fn health_check(&self) -> Result<Value, String> {
        info!("Performing comprehensive health check");

        let mut health_data = json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "config": {
                "base_url": self.config.base_url,
                "default_model": self.config.default_model,
                "timeout_secs": self.config.timeout_secs
            },
            "tests": {}
        });

        let connection_test = match self.test_connection().await {
            Ok(true) => json!({"status": "pass", "message": "Connection successful"}),
            Ok(false) => json!({"status": "fail", "message": "Connection failed"}),
            Err(e) => json!({"status": "error", "message": e}),
        };
        health_data["tests"]["connection"] = connection_test;

        let models_test = match self.get_available_models().await {
            Ok(models) => json!({
                "status": "pass",
                "message": format!("Found {} models", models.len()),
                "models": models.iter().map(|m| &m.name).collect::<Vec<_>>()
            }),
            Err(e) => json!({"status": "error", "message": e}),
        };
        health_data["tests"]["models"] = models_test;

        let generation_test = match self.generate_simple("Hello", None).await {
            Ok(response) => json!({
                "status": "pass",
                "message": "Generation successful",
                "response_length": response.len(),
                "response_preview": if response.len() > 100 { format!("{}...", &response[..100] )} else { response }
            }),
            Err(e) => json!({"status": "error", "message": e}),
        };
        health_data["tests"]["generation"] = generation_test;

        let chat_test = match self.chat_simple("Hello", None).await {
            Ok(response) => json!({
                "status": "pass",
                "message": "Chat successful",
                "response_length": response.len(),
                "response_preview": if response.len() > 100 { format!("{}...", &response[..100] )} else { response }
            }),
            Err(e) => json!({"status": "error", "message": e}),
        };
        health_data["tests"]["chat"] = chat_test;

        let all_tests_passed = health_data["tests"]
            .as_object()
            .unwrap()
            .values()
            .all(|test| test["status"] == "pass");

        health_data["overall_status"] = if all_tests_passed {
            json!("healthy")
        } else {
            json!("degraded")
        };

        info!("Health check completed: {}", health_data["overall_status"]);
        Ok(health_data)
    }
}
