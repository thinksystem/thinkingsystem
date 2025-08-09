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

use crate::config::{ConfigLoader, FlowsDemoConfig};
use crate::llm::LLMFlowGenerator;
use crate::modules::orchestrator_core::CoreFlowOrchestrator;
use crate::modules::schema_provider::SchemaProvider;
use crate::modules::validation_service::UnifiedValidator;
use anyhow::Result;
use serde_json::{json, Value};
use std::sync::Arc;
use stele::flows::core::FlowDefinition;
use stele::llm::unified_adapter::UnifiedLLMAdapter;
use stele::nlu::LLMAdapter;
use tracing::{error, info, warn};

pub struct AdaptiveFlowOrchestrator {
    core_orchestrator: CoreFlowOrchestrator,
    flow_generator: LLMFlowGenerator,
    llm_adapter: Arc<UnifiedLLMAdapter>,
    max_iterations: usize,
}

impl AdaptiveFlowOrchestrator {
    pub async fn new_with_unified_adapter(
        config_loader: ConfigLoader,
        llm_adapter: Arc<UnifiedLLMAdapter>,
        _flows_config: FlowsDemoConfig,
    ) -> Result<Self> {
        let core_orchestrator = CoreFlowOrchestrator::new(config_loader.clone()).await?;

        let registry = Arc::new(stele::blocks::registry::BlockRegistry::with_standard_blocks()?);
        let validator = Arc::new(UnifiedValidator::new(registry.clone()));
        let schema_provider = Arc::new(SchemaProvider::new(registry.clone(), config_loader));

        let flow_generator = LLMFlowGenerator::new_with_unified_adapter(
            llm_adapter.clone(),
            validator,
            schema_provider,
        );

        Ok(Self {
            core_orchestrator,
            flow_generator,
            llm_adapter,
            max_iterations: 3,
        })
    }

    pub async fn adaptive_flow_execution(
        &mut self,
        endpoint_url: &str,
        processing_goal: &str,
    ) -> Result<Value> {
        info!("Starting adaptive flow execution with error recovery");
        info!("Goal: {}", processing_goal);
        info!("Endpoint: {}", endpoint_url);

        let mut iteration = 1;
        let mut last_error: Option<String> = None;
        let mut execution_context = processing_goal.to_string();

        while iteration <= self.max_iterations {
            info!("Iteration {} of {}", iteration, self.max_iterations);

            let flow_result = if let Some(error) = &last_error {
                info!("Regenerating flow to address error: {}", error);
                self.generate_error_recovery_flow(endpoint_url, &execution_context, error)
                    .await
                    .map_err(|e| anyhow::anyhow!("Error recovery flow generation failed: {}", e))
            } else {
                info!("Generating initial flow");
                self.flow_generator
                    .generate_api_processing_flow(endpoint_url, &execution_context)
                    .await
                    .map_err(|e| anyhow::anyhow!("Initial flow generation failed: {}", e))
            };

            let flow_definition = match flow_result {
                Ok(flow) => flow,
                Err(e) => {
                    error!(" Flow generation failed on iteration {}: {}", iteration, e);
                    if iteration == self.max_iterations {
                        return Err(anyhow::anyhow!(
                            "Flow generation failed after {} iterations: {}",
                            self.max_iterations,
                            e
                        ));
                    }
                    iteration += 1;
                    continue;
                }
            };

            info!("Generated flow: {}", flow_definition.id);

            match self
                .execute_flow_with_recovery(&flow_definition, &execution_context)
                .await
            {
                Ok(result) => {
                    info!("Flow execution succeeded on iteration {}", iteration);
                    return Ok(json!({
                        "adaptive_execution": {
                            "status": "SUCCESS",
                            "iterations": iteration,
                            "final_flow": flow_definition,
                            "execution_result": result,
                            "endpoint": endpoint_url,
                            "goal": processing_goal,
                            "recovery_applied": iteration > 1,
                            "timestamp": chrono::Utc::now().to_rfc3339()
                        }
                    }));
                }
                Err(e) => {
                    warn!("️ Flow execution failed on iteration {}: {}", iteration, e);
                    last_error = Some(e.to_string());

                    if iteration == self.max_iterations {
                        error!(
                            " Flow execution failed after {} iterations. Final error: {}",
                            self.max_iterations, e
                        );
                        return Err(anyhow::anyhow!(
                            "Adaptive flow execution failed after {} iterations: {}",
                            self.max_iterations,
                            e
                        ));
                    }

                    execution_context = self
                        .analyse_error_and_update_context(&execution_context, &e.to_string())
                        .await?;

                    info!(" Updated context for next iteration: {}", execution_context);
                }
            }

            iteration += 1;
        }

        Err(anyhow::anyhow!(
            "Should not reach here - max iterations exceeded"
        ))
    }

    async fn generate_error_recovery_flow(
        &self,
        endpoint_url: &str,
        context: &str,
        error: &str,
    ) -> Result<FlowDefinition, Box<dyn std::error::Error + Send + Sync>> {
        info!("️ Analysing error and generating recovery flow");

        let enhanced_context = format!(
            "{context}\n\nPREVIOUS ERROR ENCOUNTERED:\n{error}\n\nINSTRUCTIONS: Analyse this error and modify the flow to address it. If the API requires parameters, add them to the URL or request body. If it's a missing parameter error like 'Missing name parameter', add the appropriate parameter to the API request."
        );

        self.flow_generator
            .generate_api_processing_flow(endpoint_url, &enhanced_context)
            .await
    }

    async fn execute_flow_with_recovery(
        &mut self,
        flow_definition: &FlowDefinition,
        context: &str,
    ) -> Result<Value> {
        self.core_orchestrator
            .execute_flow_with_engine(flow_definition, context)
            .await
    }

    async fn analyse_error_and_update_context(
        &self,
        current_context: &str,
        error: &str,
    ) -> Result<String> {
        let analysis_prompt = format!(
            r#"You are an API integration expert. Analyse this error and provide an improved context for the next attempt.

Current Context: {current_context}

Error Encountered: {error}

Provide an improved context that addresses this error. If it's a missing parameter error, specify what parameters should be added. If it's an authentication error, suggest adding headers. Be specific and actionable.

Respond with just the improved context, no additional explanation."#
        );

        match self.llm_adapter.generate_response(&analysis_prompt).await {
            Ok(response) => {
                let improved_context = response.trim().to_string();
                info!("LLM suggested improved context: {}", improved_context);
                Ok(improved_context)
            }
            Err(e) => {
                warn!("️ LLM analysis failed, using fallback context update: {}", e);

                let fallback_context = if error.contains("Missing") && error.contains("parameter") {
                    format!("{current_context}\n\nNote: Add required parameters to API request")
                } else if error.contains("401") || error.contains("Unauthorised") {
                    format!("{current_context}\n\nNote: Add authentication headers")
                } else {
                    format!("{current_context}\n\nNote: Handle API error: {error}")
                };
                Ok(fallback_context)
            }
        }
    }

    pub fn show_system_info(&self) -> Result<()> {
        self.core_orchestrator.show_system_info()
    }

    pub async fn health_check(&self) -> Result<Value> {
        let core_status = self.core_orchestrator.show_system_info().is_ok();

        let llm_status = (self.llm_adapter.generate_response("Test").await).is_ok();

        Ok(json!({
            "adaptive_orchestrator": {
                "core_orchestrator": core_status,
                "llm_adapter": llm_status,
                "max_iterations": self.max_iterations,
                "capabilities": {
                    "error_recovery": true,
                    "context_analysis": true,
                    "flow_regeneration": true
                }
            }
        }))
    }
}
