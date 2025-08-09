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

use crate::config::FlowsDemoConfig;
use anyhow::Result;
use async_trait::async_trait;
use dotenvy::dotenv;
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use stele::llm::unified_adapter::UnifiedLLMAdapter as SteleLLMAdapter;
use stele::nlu::llm_processor::LLMAdapter;
use tracing::{debug, error, info, warn};

#[derive(Clone)]
pub struct UnifiedLLMAdapter {
    config: FlowsDemoConfig,
    client: Client,
    stele_adapter: Option<Arc<SteleLLMAdapter>>,
}

#[derive(Debug)]
pub enum LLMProvider {
    Anthropic,
    Ollama,
    OpenAI,
}

impl UnifiedLLMAdapter {
    pub async fn new(config: FlowsDemoConfig) -> Result<Self> {
        dotenv().ok();

        let timeout = Duration::from_secs(60);
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create HTTP client: {}", e))?;

        let stele_adapter = if config.api_providers.fallback_provider == "ollama" {
            match SteleLLMAdapter::with_preferences("ollama", &config.api_providers.ollama.model)
                .await
            {
                Ok(adapter) => Some(Arc::new(adapter)),
                Err(e) => {
                    warn!("Failed to initialise stele unified adapter: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            config,
            client,
            stele_adapter,
        })
    }

    fn parse_provider(provider_str: &str) -> LLMProvider {
        match provider_str.to_lowercase().as_str() {
            "anthropic" => LLMProvider::Anthropic,
            "ollama" => LLMProvider::Ollama,
            "openai" => LLMProvider::OpenAI,
            _ => {
                warn!(
                    "Unknown provider '{}', defaulting to Anthropic",
                    provider_str
                );
                LLMProvider::Anthropic
            }
        }
    }

    async fn call_anthropic(&self, prompt: &str) -> Result<String> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY environment variable not set"))?;

        let endpoint = std::env::var("ANTHROPIC_ENDPOINT")
            .unwrap_or_else(|_| "https://api.anthropic.com/v1/messages".to_string());

        let payload = json!({
            "model": self.config.api_providers.anthropic.model,
            "max_tokens": self.config.api_providers.anthropic.max_tokens,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": self.config.api_providers.anthropic.temperature
        });

        debug!(
            "Calling Anthropic API with model: {}",
            self.config.api_providers.anthropic.model
        );

        let response = self
            .client
            .post(&endpoint)
            .header("x-api-key", &api_key)
            .header(
                "anthropic-version",
                &self.config.api_providers.anthropic.api_version,
            )
            .header("content-type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Anthropic request failed: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Anthropic API error {}: {}",
                status,
                error_body
            ));
        }

        let response_text = response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read Anthropic response: {}", e))?;

        let response_data: Value = serde_json::from_str(&response_text)
            .map_err(|e| anyhow::anyhow!("Failed to parse Anthropic JSON: {}", e))?;

        let content = response_data["content"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Failed to extract content from Anthropic response"))?;

        debug!(
            "Anthropic API call successful, response length: {}",
            content.len()
        );
        Ok(content.to_string())
    }

    async fn call_ollama(&self, prompt: &str) -> Result<String> {
        if let Some(adapter) = &self.stele_adapter {
            debug!(
                "Calling stele unified adapter with model: {}",
                self.config.api_providers.ollama.model
            );

            match adapter.generate_response(prompt).await {
                Ok(response) => {
                    debug!(
                        "Unified adapter call successful, response length: {}",
                        response.len()
                    );
                    Ok(response)
                }
                Err(e) => {
                    error!("Unified adapter call failed: {}", e);
                    Err(anyhow::anyhow!("Unified adapter generation failed: {}", e))
                }
            }
        } else {
            let endpoint = &self.config.api_providers.ollama.endpoint;

            let payload = serde_json::json!({
                "model": self.config.api_providers.ollama.model,
                "prompt": prompt,
                "stream": false
            });

            debug!("Calling Ollama HTTP API at: {}", endpoint);

            let response = self
                .client
                .post(endpoint)
                .header("content-type", "application/json")
                .json(&payload)
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Ollama HTTP request failed: {}", e))?;

            let status = response.status();
            if !status.is_success() {
                let error_body = response.text().await.unwrap_or_default();
                return Err(anyhow::anyhow!(
                    "Ollama API error {}: {}",
                    status,
                    error_body
                ));
            }

            let response_text = response
                .text()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to read Ollama response: {}", e))?;

            let response_data: Value = serde_json::from_str(&response_text)
                .map_err(|e| anyhow::anyhow!("Failed to parse Ollama JSON: {}", e))?;

            let content = response_data["response"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Failed to extract content from Ollama response"))?;

            debug!(
                "Ollama HTTP API call successful, response length: {}",
                content.len()
            );
            Ok(content.to_string())
        }
    }

    pub async fn generate_with_fallback(&self, prompt: &str) -> Result<String> {
        let primary_provider = Self::parse_provider(&self.config.api_providers.primary_provider);
        let fallback_provider = Self::parse_provider(&self.config.api_providers.fallback_provider);

        info!(
            "Generating response with primary provider: {:?}",
            primary_provider
        );

        let primary_result = match primary_provider {
            LLMProvider::Anthropic => self.call_anthropic(prompt).await,
            LLMProvider::Ollama => self.call_ollama(prompt).await,
            LLMProvider::OpenAI => Err(anyhow::anyhow!("OpenAI provider not yet implemented")),
        };

        match primary_result {
            Ok(response) => {
                debug!("Primary provider succeeded");
                Ok(response)
            }
            Err(primary_error) => {
                warn!(
                    "Primary provider {:?} failed: {}, trying fallback {:?}",
                    primary_provider, primary_error, fallback_provider
                );

                let fallback_result = match fallback_provider {
                    LLMProvider::Anthropic => self.call_anthropic(prompt).await,
                    LLMProvider::Ollama => self.call_ollama(prompt).await,
                    LLMProvider::OpenAI => {
                        Err(anyhow::anyhow!("OpenAI provider not yet implemented"))
                    }
                };

                match fallback_result {
                    Ok(response) => {
                        info!("Fallback provider {:?} succeeded", fallback_provider);
                        Ok(response)
                    }
                    Err(fallback_error) => {
                        error!(
                            "Both providers failed. Primary: {}, Fallback: {}",
                            primary_error, fallback_error
                        );
                        Err(anyhow::anyhow!(
                            "Both LLM providers failed. Primary error: {}. Fallback error: {}",
                            primary_error,
                            fallback_error
                        ))
                    }
                }
            }
        }
    }
}

#[async_trait]
impl LLMAdapter for UnifiedLLMAdapter {
    async fn process_text(&self, input: &str) -> Result<String, Box<dyn std::error::Error>> {
        self.generate_with_fallback(input)
            .await
            .map_err(|e| e.into())
    }

    async fn generate_response(&self, prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
        self.generate_with_fallback(prompt)
            .await
            .map_err(|e| e.into())
    }
}
