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

use crate::llm::{LLMError, LLMResult, UnifiedLLMAdapter};
use serde::{Deserialize, Serialize};
use stele::nlu::llm_processor::LLMAdapter;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMManagerConfig {
    pub primary_provider: String,
    pub primary_model: String,
    pub preferred_provider: Option<String>,
    pub preferred_model: Option<String>,
    pub fallback_providers: Vec<(String, String)>,
    pub retry_attempts: usize,
    pub enable_fallback: bool,
}

impl Default for LLMManagerConfig {
    fn default() -> Self {
        Self {
            primary_provider: "ollama".to_string(),
            primary_model: "llama3.1".to_string(),
            preferred_provider: Some("anthropic".to_string()),
            preferred_model: Some("claude-sonnet-4-20250514".to_string()),
            fallback_providers: vec![
                ("openai".to_string(), "gpt-4-turbo".to_string()),
                ("ollama".to_string(), "llama3.1".to_string()),
            ],
            retry_attempts: 3,
            enable_fallback: true,
        }
    }
}

pub struct LLMManager {
    primary_adapter: Box<dyn LLMAdapter + Send + Sync>,
    preferred_adapter: Option<Box<dyn LLMAdapter + Send + Sync>>,
    fallback_adapters: Vec<Box<dyn LLMAdapter + Send + Sync>>,
    config: LLMManagerConfig,
}

#[derive(Debug, Clone, Copy)]
pub enum AdapterStrategy {
    Primary,
    Preferred,
    BestAvailable,
    Fallback(usize),
}

impl LLMManager {
    pub async fn new(config: LLMManagerConfig) -> LLMResult<Self> {
        let primary_adapter =
            Self::create_adapter(&config.primary_provider, &config.primary_model).await?;
        info!(
            "Primary adapter initialised: {} ({})",
            config.primary_provider, config.primary_model
        );

        let preferred_adapter = if let (Some(provider), Some(model)) =
            (&config.preferred_provider, &config.preferred_model)
        {
            match Self::create_adapter(provider, model).await {
                Ok(adapter) => {
                    info!("Preferred adapter initialised: {} ({})", provider, model);
                    Some(adapter)
                }
                Err(e) => {
                    warn!("Failed to initialise preferred adapter {}: {}. Will use primary for high-reasoning tasks.", provider, e);
                    None
                }
            }
        } else {
            None
        };

        let mut fallback_adapters = Vec::new();
        if config.enable_fallback {
            for (provider, model) in &config.fallback_providers {
                match Self::create_adapter(provider, model).await {
                    Ok(adapter) => {
                        debug!("Fallback adapter initialised: {} ({})", provider, model);
                        fallback_adapters.push(adapter);
                    }
                    Err(e) => {
                        warn!("Failed to initialise fallback adapter {}: {}", provider, e);
                    }
                }
            }
        }

        Ok(Self {
            primary_adapter,
            preferred_adapter,
            fallback_adapters,
            config,
        })
    }

    pub async fn with_defaults() -> LLMResult<Self> {
        Self::new(LLMManagerConfig::default()).await
    }

    pub async fn simple(
        primary_provider: &str,
        primary_model: &str,
        preferred_provider: Option<(&str, &str)>,
    ) -> LLMResult<Self> {
        let config = LLMManagerConfig {
            primary_provider: primary_provider.to_string(),
            primary_model: primary_model.to_string(),
            preferred_provider: preferred_provider.map(|(p, _)| p.to_string()),
            preferred_model: preferred_provider.map(|(_, m)| m.to_string()),
            fallback_providers: vec![],
            retry_attempts: 3,
            enable_fallback: false,
        };
        Self::new(config).await
    }

    pub fn get_preferred(&self) -> &(dyn LLMAdapter + Send + Sync) {
        self.preferred_adapter
            .as_deref()
            .unwrap_or(&*self.primary_adapter)
    }

    pub fn get_primary(&self) -> &(dyn LLMAdapter + Send + Sync) {
        &*self.primary_adapter
    }

    pub fn get_fallback(&self, index: usize) -> Option<&(dyn LLMAdapter + Send + Sync)> {
        self.fallback_adapters.get(index).map(|a| &**a)
    }

    pub fn get_adapter(&self, strategy: AdapterStrategy) -> &(dyn LLMAdapter + Send + Sync) {
        match strategy {
            AdapterStrategy::Primary => self.get_primary(),
            AdapterStrategy::Preferred => self.get_preferred(),
            AdapterStrategy::BestAvailable => self.get_preferred(),
            AdapterStrategy::Fallback(index) => {
                self.get_fallback(index).unwrap_or(self.get_primary())
            }
        }
    }

    pub async fn try_with_fallback<F, T>(&self, operation: F) -> LLMResult<T>
    where
        F: Fn(
            &dyn LLMAdapter,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<T, Box<dyn std::error::Error>>> + Send + '_,
            >,
        >,
    {
        if let Some(preferred) = &self.preferred_adapter {
            for attempt in 1..=self.config.retry_attempts {
                match operation(&**preferred).await {
                    Ok(result) => {
                        debug!(
                            "Operation succeeded with preferred adapter on attempt {}",
                            attempt
                        );
                        return Ok(result);
                    }
                    Err(e) => {
                        let llm_error = LLMError::ApiError(e.to_string());
                        warn!(
                            "Preferred adapter failed on attempt {}: {}",
                            attempt, llm_error
                        );
                        if attempt == self.config.retry_attempts {
                            debug!("Exhausted retries on preferred adapter, trying primary");
                        }
                    }
                }
            }
        }

        for attempt in 1..=self.config.retry_attempts {
            match operation(&*self.primary_adapter).await {
                Ok(result) => {
                    debug!(
                        "Operation succeeded with primary adapter on attempt {}",
                        attempt
                    );
                    return Ok(result);
                }
                Err(e) => {
                    let llm_error = LLMError::ApiError(e.to_string());
                    warn!(
                        "Primary adapter failed on attempt {}: {}",
                        attempt, llm_error
                    );
                    if attempt == self.config.retry_attempts && self.config.enable_fallback {
                        debug!("Exhausted retries on primary adapter, trying fallbacks");
                    }
                }
            }
        }

        if self.config.enable_fallback {
            for (i, fallback) in self.fallback_adapters.iter().enumerate() {
                for attempt in 1..=self.config.retry_attempts {
                    match operation(&**fallback).await {
                        Ok(result) => {
                            debug!(
                                "Operation succeeded with fallback adapter {} on attempt {}",
                                i, attempt
                            );
                            return Ok(result);
                        }
                        Err(e) => {
                            let llm_error = LLMError::ApiError(e.to_string());
                            warn!(
                                "Fallback adapter {} failed on attempt {}: {}",
                                i, attempt, llm_error
                            );
                        }
                    }
                }
            }
        }

        Err(LLMError::ApiError(
            "All adapters exhausted. Operation failed across all available LLM providers."
                .to_string(),
        ))
    }

    pub async fn generate_response_with_fallback(&self, prompt: &str) -> LLMResult<String> {
        let prompt = prompt.to_string();
        self.try_with_fallback(move |adapter| {
            let prompt = prompt.clone();
            Box::pin(async move { adapter.generate_response(&prompt).await })
        })
        .await
    }

    pub async fn generate_structured_response_with_fallback(
        &self,
        system_prompt: &str,
        user_input: &str,
    ) -> LLMResult<serde_json::Value> {
        let system_prompt = system_prompt.to_string();
        let user_input = user_input.to_string();
        self.try_with_fallback(move |adapter| {
            let system_prompt = system_prompt.clone();
            let user_input = user_input.clone();
            Box::pin(async move {
                adapter
                    .generate_structured_response(&system_prompt, &user_input)
                    .await
            })
        })
        .await
    }

    pub fn get_config(&self) -> &LLMManagerConfig {
        &self.config
    }

    pub fn get_status(&self) -> LLMManagerStatus {
        LLMManagerStatus {
            primary_available: true,
            preferred_available: self.preferred_adapter.is_some(),
            fallback_count: self.fallback_adapters.len(),
            total_adapters: 1
                + if self.preferred_adapter.is_some() {
                    1
                } else {
                    0
                }
                + self.fallback_adapters.len(),
        }
    }

    async fn create_adapter(
        provider: &str,
        model: &str,
    ) -> LLMResult<Box<dyn LLMAdapter + Send + Sync>> {
        let adapter = UnifiedLLMAdapter::with_preferences(provider, model)
            .await
            .map_err(|e| {
                LLMError::ConfigError(format!(
                    "Failed to create adapter with preferences {provider}/{model}: {e}"
                ))
            })?;
        Ok(Box::new(adapter))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMManagerStatus {
    pub primary_available: bool,
    pub preferred_available: bool,
    pub fallback_count: usize,
    pub total_adapters: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_llm_manager_creation() {
        let config = LLMManagerConfig {
            primary_provider: "ollama".to_string(),
            primary_model: "llama3.1".to_string(),
            preferred_provider: None,
            preferred_model: None,
            fallback_providers: vec![],
            retry_attempts: 1,
            enable_fallback: false,
        };

        if let Ok(manager) = LLMManager::new(config).await {
            let status = manager.get_status();
            assert!(status.primary_available);
            assert!(!status.preferred_available);
            assert_eq!(status.fallback_count, 0);
        }
    }
}
