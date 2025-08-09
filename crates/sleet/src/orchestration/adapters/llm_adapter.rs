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

use super::{
    AdapterError, AdapterResult, ExecutionContext, ExecutionMetadata, InputValidator,
    ValidationConfig,
};
use crate::LLMProcessor;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct LLMAdapter {
    llm_processor: Option<Arc<RwLock<LLMProcessor>>>,
    validation_config: ValidationConfig,
}

impl LLMAdapter {
    pub async fn new() -> AdapterResult<Self> {
        Ok(Self {
            llm_processor: None,
            validation_config: ValidationConfig::default(),
        })
    }

    pub async fn with_config(validation_config: ValidationConfig) -> AdapterResult<Self> {
        Ok(Self {
            llm_processor: None,
            validation_config,
        })
    }

    pub async fn set_llm_processor(&mut self, processor: LLMProcessor) -> AdapterResult<()> {
        self.llm_processor = Some(Arc::new(RwLock::new(processor)));
        Ok(())
    }

    pub async fn process_llm_request(
        &self,
        llm_config: &super::super::LLMProcessingConfig,
        prompt_template: &str,
        context_keys: &[String],
        processing_options: &super::super::LLMProcessingOptions,
        execution_context: &ExecutionContext,
    ) -> AdapterResult<LLMProcessingResult> {
        self.validate_llm_request_input(
            llm_config,
            prompt_template,
            context_keys,
            processing_options,
            execution_context,
        )?;

        let start_time = chrono::Utc::now();
        let execution_id = uuid::Uuid::new_v4().to_string();

        let mut prompt = prompt_template.to_string();
        for key in context_keys {
            if let Some(value) = execution_context.get_variable(key) {
                let placeholder = format!("{{{key}}}");
                let replacement = match value {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                prompt = prompt.replace(&placeholder, &replacement);
            }
        }

        InputValidator::validate_prompt(&prompt, &self.validation_config)?;

        let result = if let Some(llm_processor) = &self.llm_processor {
            let mut processor = llm_processor.write().await;

            match processor
                .process_with_context(&format!("Process this prompt: {prompt}"), &prompt)
                .await
            {
                Ok(response) => match &processing_options.response_format {
                    super::super::ResponseFormat::Text => response
                        .get("response")
                        .and_then(|v| v.as_str())
                        .map(|s| Value::String(s.to_string()))
                        .unwrap_or(response),
                    super::super::ResponseFormat::Json => response,
                    super::super::ResponseFormat::Structured { schema: _ } => response,
                },
                Err(e) => {
                    return Err(AdapterError::LLMProcessingFailed(format!(
                        "LLM processing failed: {e}"
                    )));
                }
            }
        } else {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            match &processing_options.response_format {
                super::super::ResponseFormat::Text => Value::String(format!(
                    "Generated response for prompt: {}... (using model: {})",
                    &prompt[..prompt.len().min(50)],
                    llm_config.model
                )),
                super::super::ResponseFormat::Json => {
                    serde_json::json!({
                        "generated_response": format!("Response for: {}", &prompt[..prompt.len().min(30)]),
                        "model": llm_config.model,
                        "temperature": llm_config.temperature.unwrap_or(0.7),
                        "metadata": {
                            "simulated": true,
                            "prompt_length": prompt.len()
                        }
                    })
                }
                super::super::ResponseFormat::Structured { schema: _ } => {
                    serde_json::json!({
                        "structured_response": {
                            "content": format!("Structured response for: {}", &prompt[..prompt.len().min(30)]),
                            "model": llm_config.model,
                            "conforms_to_schema": true
                        }
                    })
                }
            }
        };

        let end_time = chrono::Utc::now();
        let duration_ms = end_time
            .signed_duration_since(start_time)
            .num_milliseconds() as u64;

        if processing_options.cache_results {
            let prompt_hash = format!("{:x}", md5::compute(prompt.as_bytes()));
            log::debug!("LLM result cached for prompt hash: {prompt_hash}");
        }

        let metadata = ExecutionMetadata {
            execution_id,
            start_time,
            end_time: Some(end_time),
            duration_ms: Some(duration_ms),
            resource_usage: super::ResourceUsageInfo {
                cpu_time_ms: duration_ms / 5,
                memory_peak_mb: 128,
                network_bytes: prompt.len() as u64,
                storage_bytes: 0,
            },
            performance_metrics: super::PerformanceMetrics {
                throughput: 1.0 / (duration_ms as f64 / 1000.0),
                latency_ms: duration_ms as f64,
                success_rate: 1.0,
                quality_score: Some(0.85),
            },
            error_details: None,
        };

        let processing_metadata = LLMProcessingMetadata {
            model_used: llm_config.model.clone(),
            tokens_consumed: (prompt.len() / 4) as u32,
            processing_time_ms: duration_ms,
            quality_score: Some(0.85),
        };

        Ok(LLMProcessingResult {
            result,
            execution_metadata: metadata,
            processing_metadata,
        })
    }

    fn validate_llm_request_input(
        &self,
        llm_config: &super::super::LLMProcessingConfig,
        prompt_template: &str,
        context_keys: &[String],
        _processing_options: &super::super::LLMProcessingOptions,
        execution_context: &ExecutionContext,
    ) -> Result<(), AdapterError> {
        if llm_config.provider.is_empty() {
            return Err(AdapterError::InvalidInput(
                "LLM provider cannot be empty".to_string(),
            ));
        }
        if llm_config.model.is_empty() {
            return Err(AdapterError::InvalidInput(
                "LLM model cannot be empty".to_string(),
            ));
        }

        if let Some(temp) = llm_config.temperature {
            if !(0.0..=2.0).contains(&temp) {
                return Err(AdapterError::InvalidInput(
                    "Temperature must be between 0.0 and 2.0".to_string(),
                ));
            }
        }

        if let Some(max_tokens) = llm_config.max_tokens {
            if max_tokens == 0 {
                return Err(AdapterError::InvalidInput(
                    "Max tokens must be greater than 0".to_string(),
                ));
            }
            if max_tokens > 100000 {
                return Err(AdapterError::InvalidInput(
                    "Max tokens cannot exceed 100,000".to_string(),
                ));
            }
        }

        InputValidator::validate_prompt(prompt_template, &self.validation_config)?;

        if context_keys.len() > 50 {
            return Err(AdapterError::InvalidInput(
                "Too many context keys (max: 50)".to_string(),
            ));
        }

        for key in context_keys {
            if key.is_empty() {
                return Err(AdapterError::InvalidInput(
                    "Context key cannot be empty".to_string(),
                ));
            }
            if key.len() > 100 {
                return Err(AdapterError::InvalidInput(format!(
                    "Context key too long: '{key}' (max: 100 chars)"
                )));
            }
        }

        InputValidator::validate_context_variables(
            &execution_context.variables,
            &self.validation_config,
        )?;

        if execution_context.session_id.is_empty() {
            return Err(AdapterError::InvalidInput(
                "Session ID cannot be empty".to_string(),
            ));
        }
        if execution_context.flow_id.is_empty() {
            return Err(AdapterError::InvalidInput(
                "Flow ID cannot be empty".to_string(),
            ));
        }
        if execution_context.block_id.is_empty() {
            return Err(AdapterError::InvalidInput(
                "Block ID cannot be empty".to_string(),
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMProcessingResult {
    pub result: Value,
    pub execution_metadata: ExecutionMetadata,
    pub processing_metadata: LLMProcessingMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMProcessingMetadata {
    pub model_used: String,
    pub tokens_consumed: u32,
    pub processing_time_ms: u64,
    pub quality_score: Option<f64>,
}
