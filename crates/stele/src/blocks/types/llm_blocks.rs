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

use crate::blocks::{BlockBehaviour, BlockError, BlockResult};
use anyhow::Result;
use serde_json::{json, Value};
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tracing::{info, warn};

pub trait LLMInterface: Send + Sync {
    fn generate_content<'life0, 'async_trait>(
        &'life0 self,
        content_type: &'life0 str,
        context: &'life0 str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait;

    fn analyse_content<'life0, 'async_trait>(
        &'life0 self,
        content: &'life0 str,
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait;

    fn health_check<'life0, 'async_trait>(
        &'life0 self,
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait;
}

#[derive(Clone)]
pub struct LLMContentGeneratorBlock {
    id: String,
    llm_interface: Arc<dyn LLMInterface>,
}

impl LLMContentGeneratorBlock {
    pub fn new(id: String, llm_interface: Arc<dyn LLMInterface>) -> Self {
        Self { id, llm_interface }
    }
}

impl BlockBehaviour for LLMContentGeneratorBlock {
    fn id(&self) -> &str {
        &self.id
    }

    fn clone_box(&self) -> Box<dyn BlockBehaviour> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn validate(&self) -> Result<(), BlockError> {
        if self.id.is_empty() {
            return Err(BlockError::ValidationError(
                "Block ID cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    fn process<'life0, 'async_trait>(
        &'life0 self,
        state: &'life0 mut HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<BlockResult, BlockError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        let llm_interface = self.llm_interface.clone();
        let id = self.id.clone();

        Box::pin(async move {
            info!("LLM Content Generator Block: {}", id);

            let content_type = state
                .get("content_type")
                .and_then(|v| v.as_str())
                .unwrap_or("test_data")
                .to_string();

            let context = state
                .get("context")
                .and_then(|v| v.as_str())
                .unwrap_or("general software testing")
                .to_string();

            info!(
                " Generating {} content with context: {}",
                content_type, context
            );

            match llm_interface
                .generate_content(&content_type, &context)
                .await
            {
                Ok(generated_content) => {
                    state.insert("generated_content".to_string(), json!(generated_content));
                    state.insert(
                        "generation_timestamp".to_string(),
                        json!(chrono::Utc::now().to_rfc3339()),
                    );
                    state.insert("content_type".to_string(), json!(content_type));
                    state.insert("generation_success".to_string(), json!(true));

                    info!("Successfully generated {} content", content_type);
                    Ok(BlockResult::Move("content_analyser_block".to_string()))
                }
                Err(e) => {
                    warn!("Content generation failed: {}", e);
                    state.insert("generation_error".to_string(), json!(e.to_string()));
                    state.insert("generation_success".to_string(), json!(false));

                    let fallback_content = json!({
                        "type": "fallback",
                        "message": "LLM generation failed, using fallback content",
                        "original_error": e.to_string(),
                        "fallback_data": {
                            "text": "Sample fallback text for processing",
                            "number": 42,
                            "array": [1, 2, 3, 4, 5],
                            "nested": {
                                "key": "value",
                                "timestamp": chrono::Utc::now().to_rfc3339()
                            }
                        }
                    });

                    state.insert("generated_content".to_string(), fallback_content);
                    Ok(BlockResult::Move("content_analyser_block".to_string()))
                }
            }
        })
    }
}

#[derive(Clone)]
pub struct LLMContentAnalyserBlock {
    id: String,
    llm_interface: Arc<dyn LLMInterface>,
}

impl LLMContentAnalyserBlock {
    pub fn new(id: String, llm_interface: Arc<dyn LLMInterface>) -> Self {
        Self { id, llm_interface }
    }
}

impl BlockBehaviour for LLMContentAnalyserBlock {
    fn id(&self) -> &str {
        &self.id
    }

    fn clone_box(&self) -> Box<dyn BlockBehaviour> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn validate(&self) -> Result<(), BlockError> {
        if self.id.is_empty() {
            return Err(BlockError::ValidationError(
                "Block ID cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    fn process<'life0, 'async_trait>(
        &'life0 self,
        state: &'life0 mut HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<BlockResult, BlockError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        let llm_interface = self.llm_interface.clone();
        let id = self.id.clone();

        Box::pin(async move {
            info!("LLM Content Analyser Block: {}", id);

            let content = state
                .get("generated_content")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| state.get("generated_content").map(|v| v.to_string()))
                .unwrap_or_else(|| "No content to analyse".to_string());

            info!("Analysing content: {:.100}...", content);

            match llm_interface.analyse_content(&content).await {
                Ok(analysis_result) => {
                    state.insert("content_analysis".to_string(), analysis_result);
                    state.insert(
                        "analysis_timestamp".to_string(),
                        json!(chrono::Utc::now().to_rfc3339()),
                    );
                    state.insert("analysis_success".to_string(), json!(true));

                    info!("Content analysis completed successfully");
                    Ok(BlockResult::Move("decision_block".to_string()))
                }
                Err(e) => {
                    warn!("Content analysis failed: {}", e);
                    state.insert("analysis_error".to_string(), json!(e.to_string()));
                    state.insert("analysis_success".to_string(), json!(false));

                    let basic_analysis = json!({
                        "type": "basic",
                        "content_length": content.len(),
                        "is_json": content.trim().starts_with('{') || content.trim().starts_with('['),
                        "has_content": !content.trim().is_empty(),
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "note": "Basic analysis performed due to LLM failure"
                    });

                    state.insert("content_analysis".to_string(), basic_analysis);
                    Ok(BlockResult::Move("decision_block".to_string()))
                }
            }
        })
    }
}

#[derive(Clone)]
pub struct StandardProcessorBlock {
    id: String,
}

impl StandardProcessorBlock {
    pub fn new(id: String) -> Self {
        Self { id }
    }
}

impl BlockBehaviour for StandardProcessorBlock {
    fn id(&self) -> &str {
        &self.id
    }

    fn clone_box(&self) -> Box<dyn BlockBehaviour> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn validate(&self) -> Result<(), BlockError> {
        if self.id.is_empty() {
            return Err(BlockError::ValidationError(
                "Block ID cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    fn process<'life0, 'async_trait>(
        &'life0 self,
        state: &'life0 mut HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<BlockResult, BlockError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        let id = self.id.clone();

        Box::pin(async move {
            info!("️ Standard Processor Block: {}", id);

            let processed_at = chrono::Utc::now().to_rfc3339();
            state.insert("processed_at".to_string(), json!(processed_at));
            state.insert("processor_id".to_string(), json!(id));

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            info!("Standard processing completed");
            Ok(BlockResult::Move("output_aggregator".to_string()))
        })
    }
}

#[derive(Clone)]
pub struct IntelligentDecisionBlock {
    id: String,
    llm_interface: Arc<dyn LLMInterface>,
}

impl IntelligentDecisionBlock {
    pub fn new(id: String, llm_interface: Arc<dyn LLMInterface>) -> Self {
        Self { id, llm_interface }
    }
}

impl BlockBehaviour for IntelligentDecisionBlock {
    fn id(&self) -> &str {
        &self.id
    }

    fn clone_box(&self) -> Box<dyn BlockBehaviour> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn validate(&self) -> Result<(), BlockError> {
        if self.id.is_empty() {
            return Err(BlockError::ValidationError(
                "Block ID cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    fn process<'life0, 'async_trait>(
        &'life0 self,
        state: &'life0 mut HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<BlockResult, BlockError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        let id = self.id.clone();

        let llm_interface = self.llm_interface.clone();

        Box::pin(async move {
            info!("Intelligent Decision Block: {}", id);

            let state_summary = serde_json::to_string_pretty(state)
                .unwrap_or_else(|_| "Unable to serialise state".to_string());

            let decision_prompt = format!(
                "Based on the current processing state, determine the best next block to execute. \
                State: {state_summary}\n\n\
                Available next blocks: output_aggregator, standard_processor\n\
                Respond with just the block name."
            );

            let next_block = match llm_interface
                .generate_content("decision", &decision_prompt)
                .await
            {
                Ok(response) => {
                    let decision = response.trim().to_lowercase();
                    if decision.contains("output_aggregator") {
                        "output_aggregator"
                    } else {
                        "standard_processor"
                    }
                }
                Err(e) => {
                    warn!("LLM decision failed, using fallback logic: {}", e);

                    let analysis_success = state
                        .get("analysis_success")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let generation_success = state
                        .get("generation_success")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    if analysis_success && generation_success {
                        "output_aggregator"
                    } else {
                        "standard_processor"
                    }
                }
            };

            state.insert("decision".to_string(), json!(next_block));
            state.insert(
                "decision_timestamp".to_string(),
                json!(chrono::Utc::now().to_rfc3339()),
            );

            info!("Decision made: proceeding to {}", next_block);
            Ok(BlockResult::Move(next_block.to_string()))
        })
    }
}

#[derive(Clone)]
pub struct OutputAggregatorBlock {
    id: String,
}

impl OutputAggregatorBlock {
    pub fn new(id: String) -> Self {
        Self { id }
    }
}

impl BlockBehaviour for OutputAggregatorBlock {
    fn id(&self) -> &str {
        &self.id
    }

    fn clone_box(&self) -> Box<dyn BlockBehaviour> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn validate(&self) -> Result<(), BlockError> {
        if self.id.is_empty() {
            return Err(BlockError::ValidationError(
                "Block ID cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    fn process<'life0, 'async_trait>(
        &'life0 self,
        state: &'life0 mut HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<BlockResult, BlockError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        let id = self.id.clone();

        Box::pin(async move {
            info!("Output Aggregator Block: {}", id);

            let final_output = json!({
                "flow_completed_at": chrono::Utc::now().to_rfc3339(),
                "aggregator_id": id,
                "summary": {
                    "generation_success": state.get("generation_success").unwrap_or(&json!(false)),
                    "analysis_success": state.get("analysis_success").unwrap_or(&json!(false)),
                    "processed": state.get("processed_at").is_some(),
                },
                "data": {
                    "generated_content": state.get("generated_content"),
                    "content_analysis": state.get("content_analysis"),
                    "processing_metadata": {
                        "generation_timestamp": state.get("generation_timestamp"),
                        "analysis_timestamp": state.get("analysis_timestamp"),
                        "processed_at": state.get("processed_at"),
                    }
                }
            });

            state.insert("final_output".to_string(), final_output);

            info!("Flow aggregation completed successfully");
            Ok(BlockResult::Terminate)
        })
    }
}
