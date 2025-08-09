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

pub mod adapters;
pub mod context_manager;
pub mod coordinator;
pub mod event_system;
pub mod flow_scheduler;
pub mod resource_manager;
pub mod session_manager;

pub use context_manager::{ContextManager, ExecutionContext, SharedContext};
pub use coordinator::{OrchestrationConfig, OrchestrationCoordinator};
pub use event_system::{EventSubscriber, EventSystem, OrchestrationEvent};
pub use flow_scheduler::{ExecutionPlan, FlowScheduler, SchedulingStrategy};
pub use resource_manager::{
    AllocatedResources, AllocationStrategy, ResourceManager, ResourcePool, ResourceType,
    ResourceUsageTracker, ResourceUtilisation,
};
pub use session_manager::{OrchestrationSession, SessionManager, SessionStorage};

pub use adapters::{
    AgentAdapter, AgentSelector, ExecutionStrategy, InteractionType, LLMAdapter, TaskAdapter,
    WorkflowAdapter,
};

use crate::flows::definition::FlowDefinition;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationFlowDefinition {
    pub id: String,
    pub start_block_id: String,
    pub blocks: Vec<OrchestrationBlockDefinition>,
    pub participants: Vec<String>,
    pub permissions: HashMap<String, Vec<String>>,
    pub initial_state: Option<Value>,
    pub state_schema: Option<Value>,
    pub resource_requirements: ResourceRequirements,
    pub execution_config: ExecutionConfiguration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationBlockDefinition {
    pub id: String,
    pub block_type: OrchestrationBlockType,
    pub metadata: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrchestrationBlockType {
    Conditional {
        condition: String,
        true_block: String,
        false_block: String,
    },
    Compute {
        expression: String,
        output_key: String,
        next_block: String,
    },
    AwaitInput {
        interaction_id: String,
        agent_id: String,
        prompt: String,
        state_key: String,
        next_block: String,
    },
    ForEach {
        loop_id: String,
        array_path: String,
        iterator_var: String,
        loop_body_block_id: String,
        exit_block_id: String,
    },
    TryCatch {
        try_block_id: String,
        catch_block_id: String,
    },
    SubFlow {
        flow_id: String,
        input_map: HashMap<String, Value>,
        output_key: String,
        next_block: String,
    },
    Continue {
        loop_id: String,
    },
    Break {
        loop_id: String,
    },
    Terminate,

    AgentInteraction {
        agent_selector: AgentSelector,
        task_definition: TaskDefinition,
        interaction_type: InteractionType,
        timeout_secs: Option<u64>,
        retry_config: Option<RetryConfig>,
        next_block: String,
    },
    LLMProcessing {
        llm_config: LLMProcessingConfig,
        prompt_template: String,
        context_keys: Vec<String>,
        output_key: String,
        processing_options: LLMProcessingOptions,
        next_block: String,
    },
    TaskExecution {
        task_config: TaskExecutionConfig,
        resource_requirements: ResourceRequirement,
        execution_strategy: ExecutionStrategy,
        next_block: String,
    },
    WorkflowInvocation {
        workflow_id: String,
        input_mapping: HashMap<String, String>,
        output_mapping: HashMap<String, String>,
        execution_mode: WorkflowExecutionMode,
        next_block: String,
    },
    ResourceAllocation {
        resource_type: ResourceType,
        allocation_strategy: String,
        allocation_config: AllocationConfig,
        next_block: String,
    },
    ParallelExecution {
        branch_blocks: Vec<String>,
        merge_strategy: MergeStrategy,
        timeout_secs: Option<u64>,
        next_block: String,
    },
    ConditionalAgentSelection {
        selection_criteria: SelectionCriteria,
        agent_pool: Vec<String>,
        fallback_block: String,
        success_block: String,
    },
    EventTrigger {
        event_type: String,
        event_data: HashMap<String, Value>,
        subscribers: Vec<String>,
        next_block: String,
    },
    StateCheckpoint {
        checkpoint_id: String,
        state_keys: Vec<String>,
        next_block: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    pub task_type: String,
    pub parameters: HashMap<String, Value>,
    pub expected_output: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub backoff_strategy: BackoffStrategy,
    pub retry_conditions: Vec<RetryCondition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackoffStrategy {
    Linear { interval_ms: u64 },
    Exponential { initial_ms: u64, multiplier: f64 },
    Fixed { interval_ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryCondition {
    pub error_type: String,
    pub should_retry: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMProcessingConfig {
    pub provider: String,
    pub model: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub additional_params: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMProcessingOptions {
    pub streaming: bool,
    pub cache_results: bool,
    pub response_format: ResponseFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseFormat {
    Text,
    Json,
    Structured { schema: Value },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionConfig {
    pub task_id: String,
    pub priority: TaskPriority,
    pub dependencies: Vec<String>,
    pub completion_criteria: CompletionCriteria,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionCriteria {
    pub success_conditions: Vec<String>,
    pub failure_conditions: Vec<String>,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkflowExecutionMode {
    Sequential,
    Parallel,
    Adaptive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocationConfig {
    pub constraints: HashMap<String, Value>,
    pub preferences: HashMap<String, Value>,
    pub fallback_options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MergeStrategy {
    WaitAll,
    FirstComplete,
    Majority,
    Custom { strategy_name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionCriteria {
    pub required_capabilities: Vec<String>,
    pub preferred_capabilities: Vec<String>,
    pub performance_requirements: PerformanceRequirements,
    pub availability_requirements: AvailabilityRequirements,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceRequirements {
    pub min_response_time_ms: Option<u64>,
    pub max_response_time_ms: Option<u64>,
    pub min_throughput: Option<f64>,
    pub accuracy_requirements: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailabilityRequirements {
    pub immediate: bool,
    pub max_wait_time_secs: Option<u64>,
    pub backup_options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirement {
    pub cpu_cores: Option<u32>,
    pub memory_mb: Option<u64>,
    pub storage_mb: Option<u64>,
    pub network_bandwidth_mbps: Option<u64>,
    pub specialised_hardware: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    pub agents: HashMap<String, ResourceRequirement>,
    pub llm: HashMap<String, ResourceRequirement>,
    pub tasks: HashMap<String, ResourceRequirement>,
    pub workflows: HashMap<String, ResourceRequirement>,
    pub total_limits: ResourceRequirement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfiguration {
    pub max_parallel_blocks: u32,
    pub default_timeout_secs: u64,
    pub enable_checkpointing: bool,
    pub checkpoint_interval_secs: Option<u64>,
    pub enable_debugging: bool,
    pub performance_monitoring: bool,
}

#[derive(Error, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum OrchestrationError {
    #[error("Session error: {0}")]
    SessionError(String),
    #[error("Resource allocation failed: {0}")]
    ResourceAllocationError(String),
    #[error("Agent interaction failed: {0}")]
    AgentInteractionError(String),
    #[error("LLM processing failed: {0}")]
    LLMProcessingError(String),
    #[error("Task execution failed: {0}")]
    TaskExecutionError(String),
    #[error("Workflow execution failed: {0}")]
    WorkflowExecutionError(String),
    #[error("Adapter error: {0}")]
    AdapterError(String),
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Timeout error: {0}")]
    TimeoutError(String),
    #[error("Event error: {0}")]
    EventError(String),
}

impl From<adapters::AdapterError> for OrchestrationError {
    fn from(error: adapters::AdapterError) -> Self {
        match error {
            adapters::AdapterError::AgentOperationFailed(msg) => {
                OrchestrationError::AgentInteractionError(msg)
            }
            adapters::AdapterError::ServiceUnavailable(msg) => {
                OrchestrationError::ResourceAllocationError(msg)
            }
            adapters::AdapterError::ResourceNotFound(msg) => {
                OrchestrationError::ResourceAllocationError(msg)
            }
            _ => OrchestrationError::AdapterError(format!("{error:?}")),
        }
    }
}

impl From<crate::agents::AgentError> for OrchestrationError {
    fn from(error: crate::agents::AgentError) -> Self {
        OrchestrationError::AgentInteractionError(format!("{error}"))
    }
}

impl From<serde_json::Error> for OrchestrationError {
    fn from(error: serde_json::Error) -> Self {
        OrchestrationError::ValidationError(format!("JSON serialisation error: {error}"))
    }
}

impl From<FlowDefinition> for OrchestrationFlowDefinition {
    fn from(flow_def: FlowDefinition) -> Self {
        let orchestration_blocks = flow_def
            .blocks
            .into_iter()
            .map(|block| OrchestrationBlockDefinition {
                id: block.id,
                block_type: block.block_type.into(),
                metadata: None,
            })
            .collect();

        Self {
            id: flow_def.id,
            start_block_id: flow_def.start_block_id,
            blocks: orchestration_blocks,
            participants: flow_def.participants,
            permissions: flow_def.permissions,
            initial_state: flow_def.initial_state,
            state_schema: flow_def.state_schema,
            resource_requirements: ResourceRequirements {
                agents: HashMap::new(),
                llm: HashMap::new(),
                tasks: HashMap::new(),
                workflows: HashMap::new(),
                total_limits: ResourceRequirement {
                    cpu_cores: None,
                    memory_mb: None,
                    storage_mb: None,
                    network_bandwidth_mbps: None,
                    specialised_hardware: Vec::new(),
                },
            },
            execution_config: ExecutionConfiguration {
                max_parallel_blocks: 10,
                default_timeout_secs: 300,
                enable_checkpointing: false,
                checkpoint_interval_secs: None,
                enable_debugging: false,
                performance_monitoring: true,
            },
        }
    }
}

impl From<crate::flows::definition::BlockType> for OrchestrationBlockType {
    fn from(block_type: crate::flows::definition::BlockType) -> Self {
        match block_type {
            crate::flows::definition::BlockType::Conditional {
                condition,
                true_block,
                false_block,
            } => OrchestrationBlockType::Conditional {
                condition,
                true_block,
                false_block,
            },
            crate::flows::definition::BlockType::Compute {
                expression,
                output_key,
                next_block,
            } => OrchestrationBlockType::Compute {
                expression,
                output_key,
                next_block,
            },
            crate::flows::definition::BlockType::AwaitInput {
                interaction_id,
                agent_id,
                prompt,
                state_key,
                next_block,
            } => OrchestrationBlockType::AwaitInput {
                interaction_id,
                agent_id,
                prompt,
                state_key,
                next_block,
            },
            crate::flows::definition::BlockType::ForEach {
                loop_id,
                array_path,
                iterator_var,
                loop_body_block_id,
                exit_block_id,
            } => OrchestrationBlockType::ForEach {
                loop_id,
                array_path,
                iterator_var,
                loop_body_block_id,
                exit_block_id,
            },
            crate::flows::definition::BlockType::TryCatch {
                try_block_id,
                catch_block_id,
            } => OrchestrationBlockType::TryCatch {
                try_block_id,
                catch_block_id,
            },
            crate::flows::definition::BlockType::SubFlow {
                flow_id,
                input_map,
                output_key,
                next_block,
            } => OrchestrationBlockType::SubFlow {
                flow_id,
                input_map,
                output_key,
                next_block,
            },
            crate::flows::definition::BlockType::Continue { loop_id } => {
                OrchestrationBlockType::Continue { loop_id }
            }
            crate::flows::definition::BlockType::Break { loop_id } => {
                OrchestrationBlockType::Break { loop_id }
            }
            crate::flows::definition::BlockType::Terminate => OrchestrationBlockType::Terminate,
        }
    }
}

pub type OrchestrationResult<T> = Result<T, OrchestrationError>;
