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

use crate::llm::DemoLLMWrapper;
use crate::modules::schema_provider::SchemaProvider;
use crate::modules::validation_service::{UnifiedValidator, ValidationResult};
use anyhow::Result;
use llm_contracts::{GenerationConfig, LLMRequest, ModelRequirements};
use std::sync::Arc;
use stele::flows::core::FlowDefinition;
use stele::llm::{core::LLMAdapter, unified_adapter::UnifiedLLMAdapter};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub struct LLMFlowGenerator {
    llm_wrapper: Option<Arc<DemoLLMWrapper>>,
    unified_adapter: Option<Arc<UnifiedLLMAdapter>>,
    validator: Arc<UnifiedValidator>,
    schema_provider: Arc<SchemaProvider>,
}

impl LLMFlowGenerator {
    pub fn new(
        llm_wrapper: Arc<DemoLLMWrapper>,
        validator: Arc<UnifiedValidator>,
        schema_provider: Arc<SchemaProvider>,
    ) -> Self {
        info!("️ Initialising LLMFlowGenerator with validation feedback loop (legacy mode)");
        Self {
            llm_wrapper: Some(llm_wrapper),
            unified_adapter: None,
            validator,
            schema_provider,
        }
    }

    pub fn new_with_unified_adapter(
        unified_adapter: Arc<UnifiedLLMAdapter>,
        validator: Arc<UnifiedValidator>,
        schema_provider: Arc<SchemaProvider>,
    ) -> Self {
        info!("️ Initialising LLMFlowGenerator with unified adapter");
        Self {
            llm_wrapper: None,
            unified_adapter: Some(unified_adapter),
            validator,
            schema_provider,
        }
    }

    async fn generate_llm_response(
        &self,
        prompt: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(unified_adapter) = &self.unified_adapter {
            debug!("Using unified adapter for LLM generation");

            let request = LLMRequest {
                id: Uuid::new_v4(),
                prompt: prompt.to_string(),
                system_prompt: None,
                model_requirements: ModelRequirements {
                    capabilities: vec![
                        // Put the unique capability first; selector uses the first capability
                        "anthropic_flow".to_string(),
                        "reasoning".to_string(),
                    ],
                    preferred_speed_tier: None,
                    max_cost_tier: None,
                    min_max_tokens: Some(8000),
                },
                generation_config: GenerationConfig::default(),
                context: None,
            };

            match unified_adapter.generate_response(request).await {
                Ok(response) => Ok(response.content),
                Err(e) => Err(format!("LLM generation failed: {e}").into()),
            }
        } else if let Some(llm_wrapper) = &self.llm_wrapper {
            debug!("Using legacy wrapper for LLM generation");
            llm_wrapper.generate_response(prompt).await
        } else {
            Err("No LLM backend configured".into())
        }
    }

    pub async fn generate_validated_workflow(
        &self,
        prompt: &str,
    ) -> Result<FlowDefinition, Box<dyn std::error::Error + Send + Sync>> {
        debug!("Starting guided flow generation with validation feedback loop");

        let mut iteration = 0;
        let mut last_error_feedback = String::new();

        let llm_context = self.schema_provider.generate_llm_context();
        let flow_patterns = self.schema_provider.get_flow_patterns();

        let system_prompt = format!(
            "You create flows using these predefined blocks.

            {}

            SUCCESSFUL FLOW PATTERNS:
            {}

            Design a complete flow using this precise schema and format:

            {{
              \"id\": \"your_flow_id\",
              \"name\": \"Your Flow Name\",
              \"start_block_id\": \"first_block_id\",
              \"blocks\": [
                {{
                  \"id\": \"block_id_1\",
                  \"block_type\": \"Display\",
                  \"properties\": {{
                    \"message\": \"Hello World\",
                    \"next_block\": \"block_id_2\"
                  }}
                }},
                {{
                  \"id\": \"block_id_2\",
                  \"block_type\": \"Input\",
                  \"properties\": {{
                    \"prompt\": \"Enter your name\"
                  }}
                }}
              ]
            }}

            CRITICAL REQUIREMENTS:
            1. Must have 'id', 'name', 'start_block_id', and 'blocks' fields at top level
            2. Each block must have 'id', 'block_type', and 'properties' fields
            3. The start_block_id must match the id of the first block in the blocks array
            4. Use Terminal block type for proper flow termination
            5. Respond with ONLY the JSON flow definition, no additional text.

            Focus on creating practical, well-structured flows that solve real problems.",
            llm_context,
            flow_patterns.join("\n\n")
        );

        loop {
            iteration += 1;
            debug!("Generation iteration {}", iteration);

            let full_prompt = if iteration == 1 {
                format!("{system_prompt}\n\nUser Request: {prompt}")
            } else {
                format!(
                    "{}\n\nUser Request: {}\n\nPREVIOUS ATTEMPT #{} FAILED:\n{}\n\nCRITICAL: Fix these specific issues in your next response.",
                    system_prompt, prompt, iteration - 1, last_error_feedback
                )
            };

            debug!("LLM prompt (iteration {}): {}", iteration, full_prompt);

            let start_time = std::time::Instant::now();
            let response = self.generate_llm_response(&full_prompt).await?;
            let duration = start_time.elapsed();

            debug!(
                "LLM response (iteration {}) took {:?}: {}",
                iteration, duration, response
            );

            match serde_json::from_str::<FlowDefinition>(&response) {
                Ok(flow_definition) => {
                    debug!(
                        "Successfully parsed flow definition: {}",
                        flow_definition.id
                    );

                    let validation_result = self.validator.validate(&flow_definition);

                    if validation_result.is_valid {
                        info!("Flow validation successful on iteration {}", iteration);
                        return Ok(flow_definition);
                    } else {
                        warn!(
                            "Flow validation failed on iteration {}: {} errors",
                            iteration,
                            validation_result.errors.len()
                        );

                        let feedback = self.generate_structured_feedback(&validation_result);
                        last_error_feedback = format!(
                            "VALIDATION ERRORS:\n{}\n\nYour generated JSON was:\n{}\n\nFix these specific issues!",
                            feedback,
                            serde_json::to_string_pretty(&flow_definition).unwrap_or_else(|_| "Unable to serialise".to_string())
                        );
                        debug!("Generated feedback for LLM: {}", last_error_feedback);

                        continue;
                    }
                }
                Err(parse_error) => {
                    warn!(
                        "Failed to parse JSON on iteration {}: {}",
                        iteration, parse_error
                    );
                    debug!("Raw response was: {}", response);

                    let error_msg = parse_error.to_string();
                    let mut specific_feedback = Vec::new();

                    if error_msg.contains("expected") {
                        specific_feedback.push(
                            "JSON syntax error - check for missing commas, brackets, or quotes"
                                .to_string(),
                        );
                    }
                    if error_msg.contains("EOF") {
                        specific_feedback.push(
                            "Incomplete JSON - ensure all objects and arrays are properly closed"
                                .to_string(),
                        );
                    }
                    if error_msg.contains("duplicate") {
                        specific_feedback.push(
                            "Duplicate keys in JSON object - each key must be unique".to_string(),
                        );
                    }

                    if specific_feedback.is_empty() {
                        specific_feedback.push(
                            "General JSON parsing error - validate JSON structure".to_string(),
                        );
                    }

                    last_error_feedback = format!(
                        "JSON PARSING FAILED:\n{}\n\nSpecific Issues:\n{}\n\nYour response was:\n{}\n\nProvide valid JSON only!",
                        error_msg,
                        specific_feedback.join("\n"),
                        response
                    );

                    continue;
                }
            }
        }
    }

    pub async fn generate_api_processing_flow(
        &self,
        api_endpoint: &str,
        processing_goal: &str,
    ) -> Result<FlowDefinition, Box<dyn std::error::Error + Send + Sync>> {
        info!("Generating API processing flow for: {}", api_endpoint);

        let api_flow_prompt = format!(
            "Create a flow that processes data from this API endpoint for a specific goal:\n\nAPI Endpoint: {api_endpoint}\nProcessing Goal: {processing_goal}\n\nThe flow should:\n1. Fetch data from the external API\n2. Process/analyse the response\n3. Display meaningful results\n4. Terminate cleanly\n\nUse ExternalData block for API calls and Terminal block for clean termination."
        );

        self.generate_validated_workflow(&api_flow_prompt).await
    }

    fn generate_structured_feedback(&self, result: &ValidationResult) -> String {
        let mut feedback = Vec::new();

        let critical_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| {
                matches!(
                    e.severity,
                    crate::modules::validation_service::ErrorSeverity::Critical
                )
            })
            .collect();

        let high_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| {
                matches!(
                    e.severity,
                    crate::modules::validation_service::ErrorSeverity::High
                )
            })
            .collect();

        if !critical_errors.is_empty() {
            feedback.push("CRITICAL ERRORS (must fix):".to_string());
            for error in critical_errors {
                feedback.push(format!("- {}: {}", error.message, error.suggestion));
            }
        }

        if !high_errors.is_empty() {
            feedback.push("\nHIGH PRIORITY ERRORS:".to_string());
            for error in high_errors {
                feedback.push(format!("- {}: {}", error.message, error.suggestion));
            }
        }

        feedback.push("\nFLOW ANALYSIS:".to_string());
        feedback.push(format!(
            "- Total blocks: {}",
            result.flow_analysis.total_blocks
        ));
        feedback.push(format!(
            "- Has termination: {}",
            result.flow_analysis.has_termination
        ));
        feedback.push(format!(
            "- Reachable blocks: {}",
            result.flow_analysis.reachable_blocks.len()
        ));

        if !result.flow_analysis.has_termination {
            feedback
                .push("- IMPORTANT: Add a Terminal block for proper flow termination".to_string());
        }

        feedback.join("\n")
    }

    pub async fn health_check(&self) -> Result<bool> {
        info!("Performing LLM flow generator health check");

        let llm_healthy = if let Some(unified_adapter) = &self.unified_adapter {
            let test_request = LLMRequest {
                id: Uuid::new_v4(),
                prompt: "test".to_string(),
                system_prompt: None,
                model_requirements: ModelRequirements {
                    capabilities: vec!["reasoning".to_string(), "local".to_string()],
                    preferred_speed_tier: Some("fast".to_string()),
                    max_cost_tier: Some("free".to_string()),
                    min_max_tokens: Some(4000),
                },
                generation_config: GenerationConfig::default(),
                context: None,
            };

            match unified_adapter.generate_response(test_request).await {
                Ok(_) => true,
                Err(e) => {
                    warn!("Unified adapter health check failed: {}", e);
                    false
                }
            }
        } else if let Some(llm_wrapper) = &self.llm_wrapper {
            llm_wrapper
                .health_check()
                .await
                .map_err(|e| anyhow::anyhow!("LLM health check failed: {}", e))?
        } else {
            error!("No LLM backend configured for health check");
            false
        };

        if !llm_healthy {
            warn!("LLM health check failed");
            return Ok(false);
        }

        let simple_prompt = "Create a basic flow with one Display block that says 'Hello' and ends with a Terminal block";

        match self.generate_validated_workflow(simple_prompt).await {
            Ok(_flow) => {
                info!("LLM flow generator health check passed");
                Ok(true)
            }
            Err(e) => {
                error!("LLM flow generator health check failed: {}", e);
                Ok(false)
            }
        }
    }
}
