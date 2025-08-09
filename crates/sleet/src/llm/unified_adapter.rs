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
use futures::Stream;
use llm_contracts::{
    GenerationConfig, LLMError, LLMRequest, LLMResponse, LLMResult, ModelRequirements,
};
use serde_json::Value;
use std::sync::Arc;
use stele::llm::core::LLMAdapter as SteleLLMAdapter;
use stele::llm::unified_adapter::UnifiedLLMAdapter as SteLeUnifiedLLMAdapter;
use tracing::debug;
use uuid::Uuid;

pub struct UnifiedLLMAdapter {
    stele_adapter: Arc<SteLeUnifiedLLMAdapter>,
}

impl UnifiedLLMAdapter {
    pub async fn new() -> LLMResult<Self> {
        let stele_adapter = Arc::new(SteLeUnifiedLLMAdapter::with_defaults().await?);
        Ok(Self { stele_adapter })
    }

    pub async fn with_defaults() -> LLMResult<Self> {
        Self::new().await
    }

    pub async fn with_preferences(provider: &str, model: &str) -> LLMResult<Self> {
        let stele_adapter =
            Arc::new(SteLeUnifiedLLMAdapter::with_preferences(provider, model).await?);
        Ok(Self { stele_adapter })
    }

    pub async fn anthropic() -> LLMResult<Self> {
        match Self::new().await {
            Ok(adapter) => Ok(adapter),
            Err(e) => Err(LLMError::Configuration(e.to_string())),
        }
    }

    pub async fn openai() -> LLMResult<Self> {
        match Self::new().await {
            Ok(adapter) => Ok(adapter),
            Err(e) => Err(LLMError::Configuration(e.to_string())),
        }
    }

    pub async fn ollama(model: String) -> LLMResult<Self> {
        Self::with_preferences("ollama", &model).await
    }
}

#[async_trait]
impl SteleLLMAdapter for UnifiedLLMAdapter {
    async fn generate_response(&self, request: LLMRequest) -> LLMResult<LLMResponse> {
        debug!("sleet orchestration: delegating to stele UnifiedLLMAdapter");
        self.stele_adapter.generate_response(request).await
    }

    async fn generate_streaming_response(
        &self,
        request: LLMRequest,
    ) -> LLMResult<tokio::sync::mpsc::Receiver<LLMResult<llm_contracts::StreamChunk>>> {
        debug!("sleet orchestration: delegating streaming to stele UnifiedLLMAdapter");
        self.stele_adapter
            .generate_streaming_response(request)
            .await
    }

    async fn get_available_models(&self) -> LLMResult<Vec<String>> {
        debug!("sleet orchestration: delegating model list to stele UnifiedLLMAdapter");
        self.stele_adapter.get_available_models().await
    }

    async fn health_check(&self) -> LLMResult<()> {
        debug!("sleet orchestration: delegating health check to stele UnifiedLLMAdapter");
        self.stele_adapter.health_check().await
    }
}

impl UnifiedLLMAdapter {
    pub async fn generate_response_legacy(&self, prompt: &str) -> LLMResult<String> {
        let request = LLMRequest {
            id: Uuid::new_v4(),
            prompt: prompt.to_string(),
            system_prompt: None,
            model_requirements: ModelRequirements {
                capabilities: vec!["reasoning".to_string()],
                preferred_speed_tier: None,
                max_cost_tier: None,
                min_max_tokens: None,
            },
            generation_config: GenerationConfig::default(),
            context: None,
        };

        match self.stele_adapter.generate_response(request).await {
            Ok(response) => Ok(response.content),
            Err(e) => Err(LLMError::Provider(e.to_string())),
        }
    }

    pub async fn generate_response_with_config(
        &self,
        prompt: &str,
        _config: Value,
    ) -> LLMResult<Value> {
        let request = LLMRequest {
            id: Uuid::new_v4(),
            prompt: prompt.to_string(),
            system_prompt: None,
            model_requirements: ModelRequirements {
                capabilities: vec!["reasoning".to_string()],
                preferred_speed_tier: None,
                max_cost_tier: None,
                min_max_tokens: None,
            },
            generation_config: GenerationConfig::default(),
            context: None,
        };

        match self.stele_adapter.generate_response(request).await {
            Ok(response) => {
                Ok(serde_json::json!({ "content": response.content, "usage": response.usage }))
            }
            Err(e) => Err(LLMError::Provider(e.to_string())),
        }
    }

    pub async fn stream_response(
        &self,
        prompt: &str,
    ) -> LLMResult<Box<dyn Stream<Item = LLMResult<String>> + Unpin + Send>> {
        let request = LLMRequest {
            id: Uuid::new_v4(),
            prompt: prompt.to_string(),
            system_prompt: None,
            model_requirements: ModelRequirements {
                capabilities: vec!["reasoning".to_string()],
                preferred_speed_tier: None,
                max_cost_tier: None,
                min_max_tokens: None,
            },
            generation_config: GenerationConfig::default(),
            context: None,
        };

        match self.stele_adapter.generate_response(request).await {
            Ok(response) => {
                let content = response.content;
                let stream = futures::stream::once(async move { Ok(content) });
                Ok(Box::new(Box::pin(stream)))
            }
            Err(e) => Err(LLMError::Provider(e.to_string())),
        }
    }

    pub fn get_config_legacy(&self) -> stele::LLMConfig {
        self.stele_adapter.get_config_legacy()
    }
}

#[async_trait]
impl stele::nlu::llm_processor::LLMAdapter for UnifiedLLMAdapter {
    async fn process_text(&self, input: &str) -> Result<String, Box<dyn std::error::Error>> {
        debug!("sleet orchestration: delegating process_text to stele UnifiedLLMAdapter");
        stele::nlu::llm_processor::LLMAdapter::process_text(&*self.stele_adapter, input).await
    }

    async fn generate_response(&self, prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
        debug!("sleet orchestration: delegating generate_response to stele UnifiedLLMAdapter");
        stele::nlu::llm_processor::LLMAdapter::generate_response(&*self.stele_adapter, prompt).await
    }

    async fn generate_structured_response(
        &self,
        system_prompt: &str,
        user_input: &str,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        debug!("sleet orchestration: delegating generate_structured_response to stele UnifiedLLMAdapter");
        stele::nlu::llm_processor::LLMAdapter::generate_structured_response(
            &*self.stele_adapter,
            system_prompt,
            user_input,
        )
        .await
    }
}
