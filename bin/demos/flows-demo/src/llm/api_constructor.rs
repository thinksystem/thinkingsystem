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

#![allow(clippy::only_used_in_recursion)]

use crate::config::FlowsDemoConfig;
use anyhow::Result;
use llm_contracts::{GenerationConfig, LLMRequest, ModelRequirements};
use serde_json::{json, Value};
use std::sync::Arc;
use stele::llm::{core::LLMAdapter, unified_adapter::UnifiedLLMAdapter};
use tracing::{debug, info};
use uuid::Uuid;

pub struct IntelligentAPIConstructor {
    llm_adapter: Arc<UnifiedLLMAdapter>,
    config: FlowsDemoConfig,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct APIContract {
    pub endpoint_url: String,
    pub response_structure: Value,
    pub discovered_patterns: Vec<String>,
    pub data_paths: Vec<String>,
    pub potential_parameters: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct APICallPlan {
    pub optimised_url: String,
    pub method: String,
    pub expected_data_format: String,
    pub data_extraction_paths: Vec<String>,
    pub reasoning: String,
}

impl IntelligentAPIConstructor {
    pub fn new(llm_adapter: Arc<UnifiedLLMAdapter>, config: FlowsDemoConfig) -> Self {
        info!("️ Initialising IntelligentAPIConstructor with unified LLM adapter");
        Self {
            llm_adapter,
            config,
        }
    }

    pub async fn analyse_api_contract(
        &self,
        endpoint_url: &str,
        response_data: &Value,
    ) -> Result<APIContract> {
        info!("Analysing API contract for: {}", endpoint_url);

        let analysis_prompt = format!(
            "Analyse this API response structure and identify key patterns:\n\nEndpoint: {}\nResponse: {}\n\nProvide analysis covering:\n1. Data structure patterns (arrays, objects, nested data)\n2. Potential query parameters or filters\n3. Tabular data opportunities\n4. Key data paths for extraction\n5. API capabilities and limitations\n\nFormat as structured analysis.",
            endpoint_url,
            serde_json::to_string_pretty(response_data)?
        );

        let request = LLMRequest {
            id: Uuid::new_v4(),
            prompt: analysis_prompt,
            system_prompt: None,
            model_requirements: ModelRequirements {
                capabilities: vec![
                    // Prefer Anthropic capability for analysis
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

        let response = self
            .llm_adapter
            .generate_response(request)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to analyse API contract: {}", e))?;

        let analysis = response.content;

        let patterns = self.extract_patterns_from_analysis(&analysis);
        let data_paths = self.extract_data_paths(response_data);
        let parameters = self.identify_potential_parameters(&analysis);

        debug!(
            " Contract analysis complete - {} patterns, {} paths",
            patterns.len(),
            data_paths.len()
        );

        Ok(APIContract {
            endpoint_url: endpoint_url.to_string(),
            response_structure: response_data.clone(),
            discovered_patterns: patterns,
            data_paths,
            potential_parameters: parameters,
        })
    }

    pub async fn construct_api_call_plan(
        &self,
        contract: &APIContract,
        goal: &str,
    ) -> Result<APICallPlan> {
        info!("Constructing API call plan for goal: {}", goal);

        let construction_prompt = format!(
            "Given this API contract and goal, construct an optimised API call plan:\n\nAPI Contract:\n- Endpoint: {}\n- Patterns: {:?}\n- Data Paths: {:?}\n- Parameters: {:?}\n- Goal: {}\n\nProvide a detailed plan including:\n1. Optimised URL construction\n2. Best HTTP method\n3. Expected data format\n4. Specific data extraction paths\n5. Reasoning for choices\n\nFormat as structured analysis.",
            contract.endpoint_url,
            contract.discovered_patterns,
            contract.data_paths,
            contract.potential_parameters,
            goal
        );

        let request = LLMRequest {
            id: Uuid::new_v4(),
            prompt: construction_prompt,
            system_prompt: None,
            model_requirements: ModelRequirements {
                capabilities: vec![
                    // Prefer Anthropic capability for plan construction
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

        let response = self
            .llm_adapter
            .generate_response(request)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to generate API call plan: {}", e))?;

        let plan_response = response.content;

        let plan = APICallPlan {
            optimised_url: contract.endpoint_url.clone(),
            method: "GET".to_string(),
            expected_data_format: "JSON".to_string(),
            data_extraction_paths: contract.data_paths.clone(),
            reasoning: plan_response,
        };

        info!("API call plan constructed successfully");
        Ok(plan)
    }

    pub async fn execute_plan(&self, plan: &APICallPlan) -> Result<Value> {
        info!("Executing API call plan: {}", plan.optimised_url);

        let execution_result = json!({
            "plan_execution": {
                "url": plan.optimised_url,
                "method": plan.method,
                "status": "ready_for_execution",
                "extraction_paths": plan.data_extraction_paths,
                "reasoning": plan.reasoning
            },
            "next_steps": [
                "Execute the optimised API call",
                "Apply data extraction paths",
                "Transform data format if applicable"
            ]
        });

        debug!("Plan execution structure prepared");
        Ok(execution_result)
    }

    fn extract_patterns_from_analysis(&self, analysis: &str) -> Vec<String> {
        let mut patterns = Vec::new();

        if analysis.to_lowercase().contains("array") {
            patterns.push("Array data structure".to_string());
        }
        if analysis.to_lowercase().contains("nested") {
            patterns.push("Nested object structure".to_string());
        }
        if analysis.to_lowercase().contains("pagination") {
            patterns.push("Paginated results".to_string());
        }
        if analysis.to_lowercase().contains("filter") || analysis.to_lowercase().contains("query") {
            patterns.push("Filterable data".to_string());
        }

        patterns
    }

    fn extract_data_paths(&self, data: &Value) -> Vec<String> {
        let mut paths = Vec::new();
        self.collect_paths(data, "", &mut paths);
        paths
    }

    fn collect_paths(&self, value: &Value, current_path: &str, paths: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, val) in map {
                    let new_path = if current_path.is_empty() {
                        format!("/{key}")
                    } else {
                        format!("{current_path}/{key}")
                    };
                    paths.push(new_path.clone());
                    self.collect_paths(val, &new_path, paths);
                }
            }
            Value::Array(arr) => {
                if !arr.is_empty() {
                    let array_path = format!("{current_path}/0");
                    self.collect_paths(&arr[0], &array_path, paths);
                }
            }
            _ => {}
        }
    }

    fn identify_potential_parameters(&self, analysis: &str) -> Vec<String> {
        let mut parameters = Vec::new();

        if analysis.to_lowercase().contains("filter") {
            parameters.push("filter".to_string());
        }
        if analysis.to_lowercase().contains("limit") {
            parameters.push("limit".to_string());
        }
        if analysis.to_lowercase().contains("offset") {
            parameters.push("offset".to_string());
        }
        if analysis.to_lowercase().contains("sort") {
            parameters.push("sort".to_string());
        }

        parameters
    }
}
