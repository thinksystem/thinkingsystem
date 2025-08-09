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
use crate::llm::{DemoLLMWrapper, IntelligentAPIConstructor, LLMFlowGenerator};
use crate::modules::orchestrator_core::CoreFlowOrchestrator;
use crate::modules::schema_provider::SchemaProvider;
use crate::modules::validation_service::UnifiedValidator;
use anyhow::Result;
use serde_json::{json, Value};
use std::sync::Arc;
use stele::flows::core::FlowDefinition;
use stele::llm::{unified_adapter::UnifiedLLMAdapter, dynamic_selector::DynamicModelSelector};
use tracing::warn;

pub struct LLMEnhancedOrchestrator {
    core_orchestrator: CoreFlowOrchestrator,
    llm_wrapper: Arc<DemoLLMWrapper>,
    api_constructor: IntelligentAPIConstructor,
    flow_generator: LLMFlowGenerator,
}

impl LLMEnhancedOrchestrator {
    pub async fn new(config_loader: ConfigLoader) -> Result<Self> {
        let flows_config = config_loader.load_flows_config()?;

        
        let model_selector = Arc::new(
            DynamicModelSelector::from_config_path("../../../crates/stele/src/nlu/config/llm_models.yml")
                .map_err(|e| anyhow::anyhow!("Failed to create model selector: {}", e))?
        );

        let unified_adapter = Arc::new(UnifiedLLMAdapter::new(model_selector).await?);

        Self::new_with_unified_adapter(config_loader, unified_adapter, flows_config).await
    }

    pub async fn new_with_unified_adapter(
        config_loader: ConfigLoader,
        llm_adapter: Arc<UnifiedLLMAdapter>,
        flows_config: FlowsDemoConfig,
    ) -> Result<Self> {
        let core_orchestrator = CoreFlowOrchestrator::new(config_loader.clone()).await?;

        let llm_wrapper: Arc<DemoLLMWrapper> = Arc::new(DemoLLMWrapper::new().await?);
        let api_constructor =
            IntelligentAPIConstructor::new(llm_adapter.clone(), flows_config.clone());

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
            llm_wrapper,
            api_constructor,
            flow_generator,
        })
    }

    pub async fn flow_negotiation(
        &mut self,
        endpoint_url: &str,
        processing_goal: &str,
    ) -> Result<Value> {
        let execution_result = self
            .core_orchestrator
            .execute_api_exploration(endpoint_url)
            .await?;

        let api_response = execution_result
            .get("captured_api_response")
            .or_else(|| {
                execution_result
                    .get("execution_data")
                    .and_then(|data| data.get("api_response"))
            })
            .unwrap_or(&execution_result);

        let flow_template = {
            let registry =
                Arc::new(stele::blocks::registry::BlockRegistry::with_standard_blocks()?);
            let config_loader = crate::config::ConfigLoader::new("config");
            let schema_provider = SchemaProvider::new(registry, config_loader);
            schema_provider.generate_llm_context()
        };

        let api_flow_prompt = format!(
            "Create a flow that processes data from this API endpoint for a specific goal:

API Endpoint: {}
Processing Goal: {}
API Response Sample: {}

IMPORTANT: Use the EXACT endpoint URL provided above (including all query parameters) in the api_url field.

The flow should:
1. Fetch data from the external API using ExternalData block with the full URL including parameters
2. Process/analyse the response using Compute blocks
3. Display meaningful results using Display blocks
4. Terminate cleanly using Compute block without next_block

Use the correct block types: ExternalData, Compute, Display",
            endpoint_url,
            processing_goal,
            serde_json::to_string_pretty(api_response)?
        );

        match self
            .flow_generator
            .generate_validated_workflow(&api_flow_prompt)
            .await
        {
            Ok(validated_flow) => Ok(json!({
                "flow_negotiation_result": {
                    "api_structure": {
                        "endpoint": endpoint_url,
                        "response_sample": api_response
                    },
                    "flow_template_provided": flow_template,
                    "final_validated_flow": {
                        "id": validated_flow.id,
                        "name": validated_flow.name,
                        "start_block_id": validated_flow.start_block_id,
                        "blocks": validated_flow.blocks
                    },
                    "status": "SUCCESS_WITH_VALIDATION_LOOP"
                }
            })),
            Err(e) => Err(anyhow::anyhow!(
                "Flow negotiation failed after all iterations: {}",
                e
            )),
        }
    }

    async fn construct_flow_construction_prompt(
        &self,
        endpoint_url: &str,
        api_response: &Value,
        _flow_template: &Value,
        processing_goal: &str,
    ) -> Result<String> {
        let prompt = format!(
            "You must create a flow definition JSON. Here's the exact format required:

{{
  \"id\": \"your_flow_id\",
  \"name\": \"Your Flow Name\",
  \"start_block_id\": \"first_block_id\",
  \"blocks\": [
    {{
      \"id\": \"first_block_id\",
      \"block_type\": \"ExternalData\",
      \"properties\": {{
        \"endpoint\": \"your_api_endpoint\",
        \"method\": \"GET\",
        \"next_block\": \"process_data\"
      }}
    }},
    {{
      \"id\": \"process_data\",
      \"block_type\": \"Compute\",
      \"properties\": {{
        \"expression\": \"\\\"Data processed successfully\\\"\",
        \"next_block\": \"show_results\"
      }}
    }},
    {{
      \"id\": \"show_results\",
      \"block_type\": \"Display\",
      \"properties\": {{
        \"message\": \"Results displayed\",
        \"next_block\": \"flow_complete\"
      }}
    }},
    {{
      \"id\": \"flow_complete\",
      \"block_type\": \"Compute\",
      \"properties\": {{
        \"expression\": \"\\\"Flow completed\\\"\"
      }}
    }}
  ]
}}

API to use: {}
Goal: {}
API Response Sample: {}

Create a flow definition JSON that:
1. Fetches data from the API using ExternalData block
2. Processes the data using Compute block
3. Shows results using Display block
4. Ends with final Compute block (no next_block)

Return ONLY the JSON flow definition:",
            endpoint_url,
            processing_goal,
            serde_json::to_string_pretty(api_response)?
        );

        Ok(prompt)
    }

    fn validate_flow_structure(&self, generated_flow: &Value) -> Value {
        let has_id = generated_flow.get("id").is_some();
        let has_name = generated_flow.get("name").is_some();
        let has_blocks = generated_flow
            .get("blocks")
            .and_then(|b| b.as_array())
            .is_some();
        let has_start_block = generated_flow.get("start_block_id").is_some();

        let valid_block_types = [
            "HTTPRequestBlock",
            "JSONProcessorBlock",
            "FilterBlock",
            "TransformBlock",
            "Terminal",
        ];
        let blocks_valid =
            if let Some(blocks) = generated_flow.get("blocks").and_then(|b| b.as_array()) {
                blocks.iter().all(|block| {
                    if let Some(block_type) = block.get("block_type").and_then(|t| t.as_str()) {
                        valid_block_types.contains(&block_type)
                    } else {
                        false
                    }
                })
            } else {
                false
            };

        json!({
            "valid": has_id && has_name && has_blocks && has_start_block && blocks_valid,
            "checks": {
                "has_id": has_id,
                "has_name": has_name,
                "has_blocks": has_blocks,
                "has_start_block": has_start_block,
                "blocks_valid": blocks_valid
            },
            "issues": if !blocks_valid {
                vec!["Invalid block types detected - use only: HTTPRequestBlock, JSONProcessorBlock, FilterBlock, TransformBlock, Terminal".to_string()]
            } else {
                vec![]
            }
        })
    }

    pub async fn generate_intelligent_api_flow(
        &self,
        endpoint_url: &str,
        processing_goal: &str,
    ) -> Result<Value> {
        let flow_def = self
            .flow_generator
            .generate_api_processing_flow(endpoint_url, processing_goal)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to generate API processing flow: {}", e))?;

        Ok(json!({
            "generated_flow": {
                "id": flow_def.id,
                "name": flow_def.name,
                "start_block": flow_def.start_block_id,
                "total_blocks": flow_def.blocks.len(),
                "blocks": flow_def.blocks
            },
            "generation_method": "LLM with validation feedback loop",
            "status": "Ready for execution"
        }))
    }

    pub async fn comprehensive_health_check(&self) -> Result<Value> {
        let core_status = self.core_orchestrator.show_system_info().is_ok();

        let llm_status = self.llm_wrapper.health_check().await.unwrap_or(false);

        let flow_gen_status = self.flow_generator.health_check().await.unwrap_or(false);

        let overall_health = core_status && llm_status && flow_gen_status;

        let health_report = json!({
            "overall_health": overall_health,
            "components": {
                "core_orchestrator": core_status,
                "llm_wrapper": llm_status,
                "flow_generator": flow_gen_status,
                "api_constructor": true
            },
            "capabilities": {
                "api_exploration": true,
                "contract_analysis": llm_status,
                "intelligent_flow_generation": flow_gen_status,
                "csv_strategy_construction": llm_status && core_status
            },
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "status": if overall_health { "All systems operational" } else { "Some components need attention" }
        });

        if !overall_health {
            warn!("Some components need attention");
        }

        Ok(health_report)
    }

    pub async fn execute_generated_flow(
        &mut self,
        flow_definition: &FlowDefinition,
        context: &str,
    ) -> Result<Value> {
        tracing::info!("Executing LLM-generated flow: {}", flow_definition.id);
        tracing::info!("Context: {}", context);

        self.core_orchestrator
            .execute_flow_with_engine(flow_definition, context)
            .await
    }

    pub async fn complete_flow_cycle(
        &mut self,
        endpoint_url: &str,
        processing_goal: &str,
    ) -> Result<Value> {
        tracing::info!("Starting complete flow cycle: negotiation + execution");

        let negotiation_result = self.flow_negotiation(endpoint_url, processing_goal).await?;

        let flow_data = negotiation_result
            .get("flow_negotiation_result")
            .and_then(|r| r.get("final_validated_flow"))
            .ok_or_else(|| anyhow::anyhow!("No flow definition found in negotiation result"))?;

        let flow_definition: FlowDefinition = serde_json::from_value(flow_data.clone())
            .map_err(|e| anyhow::anyhow!("Failed to deserialise flow definition: {}", e))?;

        tracing::info!("Flow negotiation completed, now executing...");

        let execution_result = self
            .execute_generated_flow(&flow_definition, processing_goal)
            .await?;

        Ok(json!({
            "complete_flow_cycle": {
                "negotiation_phase": negotiation_result,
                "execution_phase": execution_result,
                "status": "COMPLETE_SUCCESS",
                "flow_id": flow_definition.id,
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        }))
    }

    pub fn show_enhanced_system_info(&self) -> Result<()> {
        self.core_orchestrator.show_system_info()
    }
}
