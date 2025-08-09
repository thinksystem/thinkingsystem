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

use super::{AdapterResult, ExecutionContext, InputValidator, ValidationConfig};
use crate::orchestration::{session_manager::WorkflowResult, WorkflowExecutionMode};
use serde_json::Value;
use std::collections::HashMap;

pub struct WorkflowAdapter {
    validation_config: ValidationConfig,
}

impl WorkflowAdapter {
    pub async fn new() -> AdapterResult<Self> {
        Ok(Self {
            validation_config: ValidationConfig::default(),
        })
    }

    pub async fn with_config(validation_config: ValidationConfig) -> AdapterResult<Self> {
        Ok(Self { validation_config })
    }

    pub async fn invoke_workflow(
        &self,
        workflow_id: &str,
        input_mapping: &HashMap<String, String>,
        output_mapping: &HashMap<String, String>,
        execution_mode: &WorkflowExecutionMode,
        execution_context: &ExecutionContext,
    ) -> AdapterResult<WorkflowResult> {
        self.validate_workflow_invocation_input(
            workflow_id,
            input_mapping,
            output_mapping,
            execution_context,
        )?;

        let start_time = chrono::Utc::now();

        let mut workflow_inputs = HashMap::new();
        for (workflow_param, context_key) in input_mapping {
            if let Some(value) = execution_context.get_variable(context_key) {
                workflow_inputs.insert(workflow_param.clone(), value.clone());
            } else {
                log::warn!("Input mapping key '{context_key}' not found in execution context");
            }
        }

        let (result, _execution_time_ms) = match execution_mode {
            WorkflowExecutionMode::Sequential => {
                tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;
                let result = serde_json::json!({
                    "workflow_id": workflow_id,
                    "execution_mode": "sequential",
                    "status": "completed",
                    "steps_completed": 5,
                    "inputs_processed": workflow_inputs,
                    "results": {
                        "step_1": "Data validation completed",
                        "step_2": "Processing completed",
                        "step_3": "Analysis completed",
                        "step_4": "Transformation completed",
                        "step_5": "Output generation completed"
                    }
                });
                (result, 800u64)
            }
            WorkflowExecutionMode::Parallel => {
                tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;
                let result = serde_json::json!({
                    "workflow_id": workflow_id,
                    "execution_mode": "parallel",
                    "status": "completed",
                    "parallel_branches": 3,
                    "inputs_processed": workflow_inputs,
                    "results": {
                        "branch_1": "Data processing completed concurrently",
                        "branch_2": "Analysis completed concurrently",
                        "branch_3": "Validation completed concurrently"
                    },
                    "merge_result": "All branches merged successfully"
                });
                (result, 400u64)
            }
            WorkflowExecutionMode::Adaptive => {
                let complexity = workflow_inputs.len();
                let sleep_time = (complexity * 200).clamp(300, 1000) as u64;
                tokio::time::sleep(tokio::time::Duration::from_millis(sleep_time)).await;

                let result = serde_json::json!({
                    "workflow_id": workflow_id,
                    "execution_mode": "adaptive",
                    "status": "completed",
                    "adaptation_applied": true,
                    "inputs_processed": workflow_inputs,
                    "complexity_score": complexity,
                    "execution_strategy": if complexity > 5 { "parallel" } else { "sequential" },
                    "results": {
                        "adaptive_result": format!("Workflow adapted for {complexity} inputs"),
                        "performance": "Optimised for current load"
                    }
                });
                (result, sleep_time)
            }
        };

        let mut final_output = result.clone();
        if !output_mapping.is_empty() {
            let mut mapped_output = serde_json::Map::new();
            for (context_key, result_path) in output_mapping {
                if let Some(value) = result.get(result_path) {
                    mapped_output.insert(context_key.clone(), value.clone());
                } else if let Some(nested_value) = result.pointer(result_path) {
                    mapped_output.insert(context_key.clone(), nested_value.clone());
                }
            }
            final_output = Value::Object(mapped_output);
        }

        let end_time = chrono::Utc::now();
        let actual_duration = end_time
            .signed_duration_since(start_time)
            .num_milliseconds() as u64;

        let mut metadata = HashMap::new();
        metadata.insert(
            "execution_mode".to_string(),
            serde_json::to_value(execution_mode)?,
        );
        metadata.insert(
            "input_count".to_string(),
            Value::Number(serde_json::Number::from(workflow_inputs.len())),
        );
        metadata.insert(
            "output_mappings".to_string(),
            serde_json::to_value(output_mapping)?,
        );
        metadata.insert("simulated".to_string(), Value::Bool(true));

        Ok(WorkflowResult {
            workflow_id: workflow_id.to_string(),
            output: final_output,
            execution_time_ms: actual_duration,
            metadata,
        })
    }

    fn validate_workflow_invocation_input(
        &self,
        workflow_id: &str,
        input_mapping: &HashMap<String, String>,
        output_mapping: &HashMap<String, String>,
        execution_context: &ExecutionContext,
    ) -> AdapterResult<()> {
        if workflow_id.is_empty() {
            return Err(super::AdapterError::InvalidInput(
                "Workflow ID cannot be empty".to_string(),
            ));
        }

        if workflow_id.len() > 100 {
            return Err(super::AdapterError::InvalidInput(format!(
                "Workflow ID too long: '{workflow_id}' (max: 100 chars)"
            )));
        }

        if input_mapping.len() > 50 {
            return Err(super::AdapterError::InvalidInput(
                "Too many input mappings (max: 50)".to_string(),
            ));
        }

        for (workflow_param, context_key) in input_mapping {
            if workflow_param.is_empty() {
                return Err(super::AdapterError::InvalidInput(
                    "Workflow parameter name cannot be empty".to_string(),
                ));
            }
            if context_key.is_empty() {
                return Err(super::AdapterError::InvalidInput(
                    "Context key cannot be empty".to_string(),
                ));
            }
            if workflow_param.len() > 50 {
                return Err(super::AdapterError::InvalidInput(format!(
                    "Workflow parameter name too long: '{workflow_param}' (max: 50 chars)"
                )));
            }
            if context_key.len() > 50 {
                return Err(super::AdapterError::InvalidInput(format!(
                    "Context key too long: '{context_key}' (max: 50 chars)"
                )));
            }
        }

        if output_mapping.len() > 50 {
            return Err(super::AdapterError::InvalidInput(
                "Too many output mappings (max: 50)".to_string(),
            ));
        }

        for (context_key, result_path) in output_mapping {
            if context_key.is_empty() {
                return Err(super::AdapterError::InvalidInput(
                    "Output context key cannot be empty".to_string(),
                ));
            }
            if result_path.is_empty() {
                return Err(super::AdapterError::InvalidInput(
                    "Result path cannot be empty".to_string(),
                ));
            }
            if context_key.len() > 50 {
                return Err(super::AdapterError::InvalidInput(format!(
                    "Output context key too long: '{context_key}' (max: 50 chars)"
                )));
            }
            if result_path.len() > 100 {
                return Err(super::AdapterError::InvalidInput(format!(
                    "Result path too long: '{result_path}' (max: 100 chars)"
                )));
            }
        }

        InputValidator::validate_context_variables(
            &execution_context.variables,
            &self.validation_config,
        )?;

        if execution_context.session_id.is_empty() {
            return Err(super::AdapterError::InvalidInput(
                "Session ID cannot be empty".to_string(),
            ));
        }
        if execution_context.flow_id.is_empty() {
            return Err(super::AdapterError::InvalidInput(
                "Flow ID cannot be empty".to_string(),
            ));
        }
        if execution_context.block_id.is_empty() {
            return Err(super::AdapterError::InvalidInput(
                "Block ID cannot be empty".to_string(),
            ));
        }

        Ok(())
    }
}
