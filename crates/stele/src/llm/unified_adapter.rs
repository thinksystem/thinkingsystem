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

use crate::llm::{
    core::LLMAdapter as SteleLLMAdapter, dynamic_selector::DynamicModelSelector,
    dynamic_selector::ModelSelection, dynamic_selector::SelectionRequest,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::{stream, Stream};
use llm_contracts::{
    GenerationConfig, LLMError, LLMRequest, LLMResponse, LLMResult, ProviderRequest,
    ResponseMetadata, StreamChunk,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use steel::llm::{AnthropicClient, ApiClient, OllamaClient, OpenAIClient};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub struct UnifiedLLMAdapter {
    model_selector: Arc<DynamicModelSelector>,
    client_pool: Arc<RwLock<ClientPool>>,
    preferred_provider: Option<String>,
    preferred_model: Option<String>,
}

struct ClientPool {
    anthropic_clients: Vec<Arc<AnthropicClient>>,
    openai_clients: Vec<Arc<OpenAIClient>>,
    ollama_clients: Vec<Arc<OllamaClient>>,
    anthropic_index: usize,
    openai_index: usize,
    ollama_index: usize,
}

macro_rules! get_client_from_pool {
    ($pool:expr, $clients:ident, $index:ident, $provider_name:expr) => {{
        if $pool.$clients.is_empty() {
            return Err(LLMError::Provider(format!(
                "No {} clients available in the pool",
                $provider_name
            )));
        }
        let client = $pool.$clients[$pool.$index % $pool.$clients.len()].clone();
        $pool.$index = ($pool.$index + 1) % $pool.$clients.len();
        Ok(client as Arc<dyn ApiClient>)
    }};
}

impl UnifiedLLMAdapter {
    pub async fn new(model_selector: Arc<DynamicModelSelector>) -> LLMResult<Self> {
        let client_pool = Arc::new(RwLock::new(ClientPool::new().await?));
        Ok(Self {
            model_selector,
            client_pool,
            preferred_provider: None,
            preferred_model: None,
        })
    }

    pub async fn with_defaults() -> Result<Self, LLMError> {
        let config_path =
            if std::path::Path::new("crates/stele/src/nlu/config/llm_models.yml").exists() {
                "crates/stele/src/nlu/config/llm_models.yml"
            } else {
                "../../../crates/stele/src/nlu/config/llm_models.yml"
            };
        match DynamicModelSelector::from_config_path(config_path) {
            Ok(selector) => {
                info!(
                    "UnifiedLLMAdapter initialised with default configuration from {}",
                    config_path
                );
                Self::new(Arc::new(selector)).await
            }
            Err(e) => {
                error!("Failed to load llm_models.yml from stele: {}", e);
                Err(LLMError::Configuration(format!(
                    "Could not load default model configuration: {e}"
                )))
            }
        }
    }

    pub async fn with_preferences(
        preferred_provider: &str,
        preferred_model: &str,
    ) -> Result<Self, LLMError> {
        let config_path =
            if std::path::Path::new("crates/stele/src/nlu/config/llm_models.yml").exists() {
                "crates/stele/src/nlu/config/llm_models.yml"
            } else {
                "../../../crates/stele/src/nlu/config/llm_models.yml"
            };
        let model_selector = Arc::new(
            DynamicModelSelector::from_config_path(config_path).map_err(|e| {
                LLMError::Configuration(format!("Failed to load llm_models.yml: {e}"))
            })?,
        );

        let client_pool = Arc::new(RwLock::new(ClientPool::new().await?));

        info!(
            "Created UnifiedLLMAdapter with preferences: provider={}, model={}",
            preferred_provider, preferred_model
        );

        Ok(Self {
            model_selector,
            client_pool,
            preferred_provider: Some(preferred_provider.to_string()),
            preferred_model: Some(preferred_model.to_string()),
        })
    }

    pub fn model_selector(&self) -> Arc<DynamicModelSelector> {
        self.model_selector.clone()
    }

    async fn get_available_providers(&self) -> Vec<String> {
        let pool = self.client_pool.read().await;
        let mut available = Vec::with_capacity(3);
        if !pool.anthropic_clients.is_empty() {
            available.push("anthropic".to_string());
        }
        if !pool.openai_clients.is_empty() {
            available.push("openai".to_string());
        }
        if !pool.ollama_clients.is_empty() {
            available.push("ollama".to_string());
        }
        available
    }

    async fn get_client(&self, provider: &str) -> LLMResult<Arc<dyn ApiClient>> {
        let mut pool = self.client_pool.write().await;
        match provider {
            "anthropic" => {
                get_client_from_pool!(pool, anthropic_clients, anthropic_index, "Anthropic")
            }
            "openai" => get_client_from_pool!(pool, openai_clients, openai_index, "OpenAI"),
            "ollama" => get_client_from_pool!(pool, ollama_clients, ollama_index, "Ollama"),
            _ => Err(LLMError::Provider(format!(
                "Unsupported provider: {provider}"
            ))),
        }
    }

    async fn select_model_for_request(&self, request: &LLMRequest) -> LLMResult<ModelSelection> {
        let available_providers = self.get_available_providers().await;
        debug!("Available providers: {:?}", available_providers);

        let mut selection_request = SelectionRequest::new(
            &request
                .model_requirements
                .capabilities
                .first()
                .cloned()
                .unwrap_or_else(|| "reasoning".to_string()),
        );

        if let (Some(provider), Some(model)) = (&self.preferred_provider, &self.preferred_model) {
            selection_request = selection_request.with_preferences(provider, model);
        }

        selection_request = selection_request.with_available_providers(available_providers);

        let selected_model = self
            .model_selector
            .select_model(&selection_request)
            .map_err(|e| LLMError::ModelNotFound(format!("Model selection failed: {e}")))?;

        debug!(
            "Selected model: {} from provider: {} (score: {:.4})",
            selected_model.model.name, selected_model.model.provider, selected_model.score
        );
        Ok(selected_model)
    }

    fn build_provider_request(&self, request: &LLMRequest, model_name: &str) -> ProviderRequest {
        let mut messages = Vec::new();

        if let Some(system_prompt) = &request.system_prompt {
            messages.push(llm_contracts::Message {
                role: "system".to_string(),
                content: system_prompt.clone(),
            });
        }

        messages.push(llm_contracts::Message {
            role: "user".to_string(),
            content: request.prompt.clone(),
        });

        ProviderRequest {
            model: model_name.to_string(),
            messages,
            max_tokens: request.generation_config.max_tokens,
            temperature: request.generation_config.temperature,
            top_p: request.generation_config.top_p,
            stop_sequences: request.generation_config.stop_sequences.clone(),
            stream: request.generation_config.stream,
            provider_specific: HashMap::new(),
        }
    }

    fn build_llm_response(
        &self,
        request_id: Uuid,
        provider_response: llm_contracts::ProviderResponse,
        model_selection: &ModelSelection,
        processing_time_ms: u64,
    ) -> LLMResponse {
        LLMResponse {
            id: Uuid::new_v4(),
            request_id,
            content: provider_response.content,
            model_used: model_selection.model.name.clone(),
            provider_used: model_selection.model.provider.clone(),
            usage: provider_response.usage,
            metadata: ResponseMetadata {
                processing_time_ms,
                model_selection_reason: model_selection.reason.clone(),
                security_checks_passed: true,
                cached: false,
                retry_count: 0,
                cost_estimate: None,
                additional_data: HashMap::new(),
            },
            created_at: Utc::now(),
        }
    }

    fn create_response_chunks(&self, response: &str) -> Vec<String> {
        let words: Vec<&str> = response.split_whitespace().collect();
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();

        for (i, word) in words.iter().enumerate() {
            if !current_chunk.is_empty() {
                current_chunk.push(' ');
            }
            current_chunk.push_str(word);

            let should_emit = match i % 7 {
                0 | 3 | 5 => current_chunk.len() > 20,
                1 | 4 => current_chunk.len() > 15,
                2 | 6 => current_chunk.len() > 25,
                _ => false,
            };

            if (should_emit || i == words.len() - 1) && !current_chunk.is_empty() {
                chunks.push(current_chunk.clone());
                current_chunk.clear();
            }
        }

        if !current_chunk.is_empty() {
            chunks.push(current_chunk);
        }

        debug!("Created {} chunks for streaming", chunks.len());
        chunks
    }
}

#[async_trait]
impl SteleLLMAdapter for UnifiedLLMAdapter {
    async fn generate_response(&self, request: LLMRequest) -> LLMResult<LLMResponse> {
        let start_time = std::time::Instant::now();
        info!("Processing LLM request with ID: {}", request.id);

        let selected_model = self.select_model_for_request(&request).await?;
        let client = self.get_client(&selected_model.model.provider).await?;
        let provider_request = self.build_provider_request(&request, &selected_model.model.name);

        let provider_response = client.send_request(provider_request).await?;

        let processing_time = start_time.elapsed().as_millis() as u64;
        let response = self.build_llm_response(
            request.id,
            provider_response,
            &selected_model,
            processing_time,
        );

        self.model_selector
            .update_performance(
                &response.model_used,
                start_time.elapsed(),
                Some(response.usage.completion_tokens as u32),
                None,
                None,
                true,
            )
            .ok();

        info!(
            "Successfully processed LLM request in {}ms",
            processing_time
        );
        Ok(response)
    }

    async fn generate_streaming_response(
        &self,
        request: LLMRequest,
    ) -> LLMResult<tokio::sync::mpsc::Receiver<LLMResult<StreamChunk>>> {
        let start_time = std::time::Instant::now();
        info!("Processing streaming LLM request with ID: {}", request.id);

        let selected_model = self.select_model_for_request(&request).await?;
        let client = self.get_client(&selected_model.model.provider).await?;

        let mut provider_request =
            self.build_provider_request(&request, &selected_model.model.name);
        provider_request.stream = Some(true);

        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let request_id = request.id;

        let model_selector = self.model_selector.clone();
        let model_used = selected_model.model.name.clone();

        tokio::spawn(async move {
            match client.send_streaming_request(provider_request).await {
                Ok(mut stream) => {
                    let mut final_usage = None;
                    while let Some(provider_chunk) = stream.recv().await {
                        if provider_chunk.is_final {
                            final_usage = provider_chunk.usage.clone();
                        }
                        let chunk = StreamChunk {
                            id: Uuid::new_v4(),
                            request_id,
                            content_delta: provider_chunk.content_delta,
                            is_final: provider_chunk.is_final,
                            usage: provider_chunk.usage,
                        };
                        if tx.send(Ok(chunk)).await.is_err() {
                            debug!("Streaming receiver dropped, stopping stream");
                            break;
                        }
                    }
                    let total_tokens = final_usage.map_or(0, |u| u.completion_tokens);
                    model_selector
                        .update_performance(
                            &model_used,
                            start_time.elapsed(),
                            Some(total_tokens),
                            None,
                            None,
                            true,
                        )
                        .ok();
                }
                Err(e) => {
                    let error = LLMError::Provider(e.to_string());
                    model_selector
                        .update_performance(
                            &model_used,
                            start_time.elapsed(),
                            None,
                            None,
                            None,
                            false,
                        )
                        .ok();
                    if tx.send(Err(error)).await.is_err() {
                        debug!("Streaming receiver dropped while sending initial error");
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn get_available_models(&self) -> LLMResult<Vec<String>> {
        let model_names = self
            .model_selector
            .get_models()
            .iter()
            .map(|m| m.name.clone())
            .collect::<Vec<String>>();
        Ok(model_names)
    }

    async fn health_check(&self) -> LLMResult<()> {
        let pool = self.client_pool.read().await;
        if pool.anthropic_clients.is_empty()
            && pool.openai_clients.is_empty()
            && pool.ollama_clients.is_empty()
        {
            return Err(LLMError::Internal(
                "No LLM clients available in pool".to_string(),
            ));
        }

        info!(
            "Health check passed: {} Anthropic, {} OpenAI, {} Ollama clients available",
            pool.anthropic_clients.len(),
            pool.openai_clients.len(),
            pool.ollama_clients.len()
        );
        Ok(())
    }
}

impl ClientPool {
    async fn new() -> LLMResult<Self> {
        let mut anthropic_clients = Vec::new();
        if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
            anthropic_clients.push(Arc::new(AnthropicClient::new(
                api_key, None, None, None, None,
            )));
            info!("Created Anthropic client");
        } else {
            warn!("ANTHROPIC_API_KEY not found, Anthropic client not available");
        }

        let mut openai_clients = Vec::new();
        if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
            openai_clients.push(Arc::new(OpenAIClient::new(api_key, None, None, None)));
            info!("Created OpenAI client");
        } else {
            warn!("OPENAI_API_KEY not found, OpenAI client not available");
        }

        let mut ollama_clients = Vec::new();
        let ollama_base_url = std::env::var("OLLAMA_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());
        let client = OllamaClient::new(Some(ollama_base_url), None, None);
        if client.health_check().await.is_ok() {
            ollama_clients.push(Arc::new(client));
            info!("Created and connected to Ollama client");
        } else {
            warn!("Ollama not available, client not created");
        }

        Ok(Self {
            anthropic_clients,
            openai_clients,
            ollama_clients,
            anthropic_index: 0,
            openai_index: 0,
            ollama_index: 0,
        })
    }
}

impl UnifiedLLMAdapter {
    pub async fn generate_response_legacy(
        &self,
        prompt: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let request = LLMRequest {
            id: Uuid::new_v4(),
            prompt: prompt.to_string(),
            system_prompt: None,
            model_requirements: llm_contracts::ModelRequirements {
                capabilities: vec!["reasoning".to_string()],
                preferred_speed_tier: None,
                max_cost_tier: None,
                min_max_tokens: None,
            },
            generation_config: GenerationConfig::default(),
            context: None,
        };

        self.generate_response(request)
            .await
            .map(|resp| resp.content)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }

    pub async fn generate_structured_response_legacy(
        &self,
        system_prompt: &str,
        user_input: &str,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        let request = LLMRequest {
            id: Uuid::new_v4(),
            prompt: user_input.to_string(),
            system_prompt: Some(system_prompt.to_string()),
            model_requirements: llm_contracts::ModelRequirements {
                capabilities: vec!["reasoning".to_string()],
                preferred_speed_tier: None,
                max_cost_tier: None,
                min_max_tokens: None,
            },
            generation_config: GenerationConfig::default(),
            context: None,
        };

        let response = self.generate_response(request).await?;
        match serde_json::from_str::<Value>(&response.content) {
            Ok(json) => Ok(json),
            Err(_) => Ok(serde_json::json!({
                "response": response.content,
                "model_used": response.model_used,
                "provider_used": response.provider_used
            })),
        }
    }

    pub async fn stream_response_legacy(
        &self,
        prompt: &str,
    ) -> Result<
        Box<dyn Stream<Item = Result<String, Box<dyn std::error::Error>>> + Unpin + Send>,
        Box<dyn std::error::Error>,
    > {
        let full_response = self.generate_response_legacy(prompt).await?;
        let chunks = self.create_response_chunks(&full_response);
        let stream = stream::iter(chunks.into_iter().map(Ok));
        Ok(Box::new(stream))
    }

    pub fn get_config_legacy(&self) -> crate::LLMConfig {
        crate::LLMConfig {
            model_name: "claude-3-sonnet-20240229".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
            api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
        }
    }
}

#[async_trait]
impl crate::nlu::llm_processor::LLMAdapter for UnifiedLLMAdapter {
    async fn process_text(&self, input: &str) -> Result<String, Box<dyn std::error::Error>> {
        self.generate_response_legacy(input).await
    }

    async fn generate_response(&self, prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
        self.generate_response_legacy(prompt).await
    }

    async fn generate_structured_response(
        &self,
        system_prompt: &str,
        user_input: &str,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        self.generate_structured_response_legacy(system_prompt, user_input)
            .await
    }
}
