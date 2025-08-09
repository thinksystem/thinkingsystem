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

use super::{OrchestrationFlowDefinition, OrchestrationResult};
use crate::orchestration::adapters::AgentInteractionResult;
use crate::orchestration::session_manager::{ParallelExecutionResult, WorkflowResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ContextManager {
    contexts: Arc<RwLock<HashMap<String, ExecutionContext>>>,
}

impl ContextManager {
    pub async fn new() -> OrchestrationResult<Self> {
        Ok(Self {
            contexts: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn create_context(
        &mut self,
        session_id: &str,
        flow_def: &OrchestrationFlowDefinition,
    ) -> OrchestrationResult<ExecutionContext> {
        let context = ExecutionContext::new(session_id.to_string(), flow_def);

        {
            let mut contexts = self.contexts.write().await;
            contexts.insert(session_id.to_string(), context.clone());
        }

        Ok(context)
    }

    pub async fn get_context(&self, session_id: &str) -> Option<ExecutionContext> {
        let contexts = self.contexts.read().await;
        contexts.get(session_id).cloned()
    }

    pub async fn update_context(
        &self,
        session_id: &str,
        updater: impl FnOnce(&mut ExecutionContext),
    ) -> OrchestrationResult<()> {
        let mut contexts = self.contexts.write().await;
        if let Some(context) = contexts.get_mut(session_id) {
            updater(context);
        }
        Ok(())
    }

    pub async fn remove_context(&self, session_id: &str) -> OrchestrationResult<()> {
        let mut contexts = self.contexts.write().await;
        contexts.remove(session_id);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub session_id: String,
    pub shared_state: Value,
    pub agent_contexts: HashMap<String, AgentContext>,
    pub llm_contexts: HashMap<String, LLMContext>,
    pub task_contexts: HashMap<String, TaskContext>,
    pub workflow_contexts: HashMap<String, WorkflowContext>,
    pub variables: HashMap<String, Value>,
    pub final_result: Option<Value>,
    pub metadata: HashMap<String, Value>,
}

impl ExecutionContext {
    pub fn new(session_id: String, flow_def: &OrchestrationFlowDefinition) -> Self {
        Self {
            session_id,
            shared_state: flow_def
                .initial_state
                .clone()
                .unwrap_or(Value::Object(serde_json::Map::new())),
            agent_contexts: HashMap::new(),
            llm_contexts: HashMap::new(),
            task_contexts: HashMap::new(),
            workflow_contexts: HashMap::new(),
            variables: HashMap::new(),
            final_result: None,
            metadata: HashMap::new(),
        }
    }

    pub fn get_shared_state(&self) -> Option<&Value> {
        Some(&self.shared_state)
    }

    pub fn set_shared_state(&mut self, state: Value) {
        self.shared_state = state;
    }

    pub fn get_variable(&self, key: &str) -> Option<&Value> {
        self.variables.get(key)
    }

    pub fn set_value(&mut self, key: &str, value: Value) {
        self.variables.insert(key.to_string(), value);
    }

    pub fn add_agent_result(&mut self, result: AgentInteractionResult) {
        let execution_metadata = super::adapters::ExecutionMetadata {
            execution_id: format!("agent-{}", result.agent_id),
            start_time: result.execution_metadata.start_time,
            end_time: result.execution_metadata.end_time,
            duration_ms: result.execution_metadata.duration_ms,
            resource_usage: super::adapters::ResourceUsageInfo {
                cpu_time_ms: 0,
                memory_peak_mb: 0,
                network_bytes: 0,
                storage_bytes: 0,
            },
            performance_metrics: super::adapters::PerformanceMetrics {
                throughput: 0.0,
                latency_ms: result.execution_metadata.duration_ms.unwrap_or(0) as f64,
                success_rate: 1.0,
                quality_score: None,
            },
            error_details: result.execution_metadata.error_details,
        };

        let context = AgentContext {
            agent_id: result.agent_id.clone(),
            last_result: Some(result.result),
            execution_metadata,
            agent_metadata: {
                let mut metadata = HashMap::new();
                metadata.insert(
                    "capabilities".to_string(),
                    serde_json::to_value(&result.agent_metadata.capabilities).unwrap_or_default(),
                );
                metadata.insert(
                    "current_load".to_string(),
                    serde_json::to_value(result.agent_metadata.current_load).unwrap_or_default(),
                );
                metadata.insert(
                    "response_time_ms".to_string(),
                    serde_json::to_value(result.agent_metadata.response_time_ms)
                        .unwrap_or_default(),
                );
                metadata.insert(
                    "success_rate".to_string(),
                    serde_json::to_value(result.agent_metadata.success_rate).unwrap_or_default(),
                );
                metadata
            },
        };
        self.agent_contexts.insert(result.agent_id, context);
    }

    pub fn add_task_result(&mut self, task_id: String, result: Value) {
        let context = TaskContext {
            task_id: task_id.clone(),
            last_result: Some(result),
            execution_metadata: HashMap::new(),
        };
        self.task_contexts.insert(task_id, context);
    }

    pub fn add_workflow_result(&mut self, result: WorkflowResult) {
        let context = WorkflowContext {
            workflow_id: result.workflow_id.clone(),
            last_result: Some(result.output),
            execution_time_ms: result.execution_time_ms,
            metadata: result.metadata,
        };
        self.workflow_contexts.insert(result.workflow_id, context);
    }

    pub fn add_parallel_result(&mut self, result: ParallelExecutionResult) {
        if let Value::Object(ref mut obj) = &mut self.shared_state {
            obj.insert(
                "parallel_results".to_string(),
                serde_json::to_value(&result.branch_results).unwrap_or_default(),
            );
        }
    }

    pub fn add_input_data(&mut self, input_data: Value) {
        self.variables.insert("input".to_string(), input_data);
    }

    pub fn has_input_data(&self) -> bool {
        self.variables.contains_key("input")
    }

    pub fn consume_input_data(&mut self) -> Option<Value> {
        self.variables.remove("input")
    }

    pub fn set_variable(&mut self, key: String, value: Value) {
        self.variables.insert(key, value);
    }

    pub fn set_final_result(&mut self, result: Value) {
        self.final_result = Some(result);
    }

    pub fn get_final_result(&self) -> Option<Value> {
        self.final_result.clone()
    }

    pub fn get_context_for_path(&self, path: &str) -> Option<Value> {
        if let Some(value) = self.variables.get(path) {
            return Some(value.clone());
        }
        self.shared_state.pointer(path).cloned()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContext {
    pub agent_id: String,
    pub last_result: Option<Value>,
    pub execution_metadata: super::adapters::ExecutionMetadata,
    pub agent_metadata: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMContext {
    pub llm_id: String,
    pub last_result: Option<Value>,
    pub processing_metadata: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub task_id: String,
    pub last_result: Option<Value>,
    pub execution_metadata: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowContext {
    pub workflow_id: String,
    pub last_result: Option<Value>,
    pub execution_time_ms: u64,
    pub metadata: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedContext {
    pub global_variables: HashMap<String, Value>,
    pub system_state: Value,
    pub configuration: HashMap<String, Value>,
}

impl Default for SharedContext {
    fn default() -> Self {
        Self {
            global_variables: HashMap::new(),
            system_state: Value::Object(serde_json::Map::new()),
            configuration: HashMap::new(),
        }
    }
}
