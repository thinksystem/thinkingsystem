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
    adapters::{AgentAdapter, LLMAdapter, TaskAdapter, WorkflowAdapter},
    context_manager::ContextManager,
    event_system::{EventSystem, OrchestrationEvent},
    flow_scheduler::FlowScheduler,
    resource_manager::ResourceManager,
    session_manager::{OrchestrationSession, SessionManager},
    OrchestrationError, OrchestrationFlowDefinition, OrchestrationResult,
};
use crate::{
    runtime::{ExecutionStatus, Value as RuntimeValue},
    transpiler::FlowTranspiler,
    AgentSystem, LLMProcessor, TaskSystem,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

fn convert_to_runtime_value(json_value: &serde_json::Value) -> RuntimeValue {
    match json_value {
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                RuntimeValue::Integer(i)
            } else {
                RuntimeValue::Integer(0)
            }
        }
        serde_json::Value::Bool(b) => RuntimeValue::Boolean(*b),
        serde_json::Value::String(s) => RuntimeValue::String(s.clone()),
        serde_json::Value::Null => RuntimeValue::Null,
        _ => RuntimeValue::Null,
    }
}

fn convert_string_to_runtime_value(s: &str) -> RuntimeValue {
    RuntimeValue::String(s.to_string())
}

fn convert_from_runtime_value(runtime_value: &RuntimeValue) -> serde_json::Value {
    match runtime_value {
        RuntimeValue::Integer(i) => serde_json::Value::Number(serde_json::Number::from(*i)),
        RuntimeValue::Boolean(b) => serde_json::Value::Bool(*b),
        RuntimeValue::String(s) => serde_json::Value::String(s.clone()),
        RuntimeValue::Null => serde_json::Value::Null,
        RuntimeValue::Json(j) => j.clone(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationConfig {
    pub max_concurrent_sessions: u32,
    pub default_gas_limit: u64,
    pub session_timeout_secs: u64,
    pub enable_persistence: bool,
    pub storage_config: StorageConfig,
    pub resource_limits: ResourceLimits,
    pub monitoring_config: MonitoringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub session_storage_path: Option<String>,
    pub checkpoint_storage_path: Option<String>,
    pub log_storage_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_agents_per_session: u32,
    pub max_llm_instances_per_session: u32,
    pub max_tasks_per_session: u32,
    pub max_memory_mb_per_session: u64,
    pub max_cpu_cores_per_session: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub enable_performance_tracking: bool,
    pub enable_resource_monitoring: bool,
    pub enable_detailed_logging: bool,
    pub metrics_collection_interval_secs: u64,
}

impl Default for OrchestrationConfig {
    fn default() -> Self {
        Self {
            max_concurrent_sessions: 100,
            default_gas_limit: 1_000_000,
            session_timeout_secs: 3600,
            enable_persistence: true,
            storage_config: StorageConfig {
                session_storage_path: Some("./orchestration/sessions".to_string()),
                checkpoint_storage_path: Some("./orchestration/checkpoints".to_string()),
                log_storage_path: Some("./orchestration/logs".to_string()),
            },
            resource_limits: ResourceLimits {
                max_agents_per_session: 50,
                max_llm_instances_per_session: 10,
                max_tasks_per_session: 100,
                max_memory_mb_per_session: 8192,
                max_cpu_cores_per_session: 8,
            },
            monitoring_config: MonitoringConfig {
                enable_performance_tracking: true,
                enable_resource_monitoring: true,
                enable_detailed_logging: false,
                metrics_collection_interval_secs: 60,
            },
        }
    }
}

pub struct OrchestrationCoordinator {
    config: OrchestrationConfig,
    session_manager: Arc<RwLock<SessionManager>>,
    resource_manager: Arc<RwLock<ResourceManager>>,
    flow_scheduler: Arc<RwLock<FlowScheduler>>,
    context_manager: Arc<RwLock<ContextManager>>,
    event_system: Arc<RwLock<EventSystem>>,

    agent_adapter: Arc<RwLock<AgentAdapter>>,
    llm_adapter: Arc<RwLock<LLMAdapter>>,
    task_adapter: Arc<RwLock<TaskAdapter>>,
    workflow_adapter: Arc<RwLock<WorkflowAdapter>>,

    _transpiler: FlowTranspiler,

    active_sessions: Arc<RwLock<HashMap<String, Arc<RwLock<OrchestrationSession>>>>>,
}

impl OrchestrationCoordinator {
    pub async fn new(config: OrchestrationConfig) -> OrchestrationResult<Self> {
        let session_manager = Arc::new(RwLock::new(
            SessionManager::new(config.storage_config.clone()).await?,
        ));

        let resource_manager = Arc::new(RwLock::new(
            ResourceManager::new(config.resource_limits.clone()).await?,
        ));

        let flow_scheduler = Arc::new(RwLock::new(
            FlowScheduler::new(config.max_concurrent_sessions).await?,
        ));

        let context_manager = Arc::new(RwLock::new(ContextManager::new().await?));

        let event_system = Arc::new(RwLock::new(EventSystem::new().await?));

        let agent_config = crate::agents::AgentSystemConfig::default();
        let agent_system = Arc::new(RwLock::new(AgentSystem::new(agent_config)?));
        let agent_adapter = Arc::new(RwLock::new(AgentAdapter::new(agent_system)));

        let llm_adapter = Arc::new(RwLock::new(LLMAdapter::new().await?));

        let task_adapter = Arc::new(RwLock::new(TaskAdapter::new().await?));

        let workflow_adapter = Arc::new(RwLock::new(WorkflowAdapter::new().await?));

        let transpiler = FlowTranspiler;

        Ok(Self {
            config,
            session_manager,
            resource_manager,
            flow_scheduler,
            context_manager,
            event_system,
            agent_adapter,
            llm_adapter,
            task_adapter,
            workflow_adapter,
            _transpiler: transpiler,
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn initialise(
        &mut self,
        agent_system: Option<AgentSystem>,
        llm_processor: Option<LLMProcessor>,
        task_system: Option<TaskSystem>,
    ) -> OrchestrationResult<()> {
        if let Some(agent_sys) = agent_system {
            let agents = agent_sys.list_active_agents().map_err(|e| {
                OrchestrationError::ConfigurationError(format!("Failed to list agents: {e}"))
            })?;

            let resource_manager = self.resource_manager.clone();
            for agent in agents {
                let agent_resource = super::resource_manager::AgentResource {
                    id: agent.id.clone(),
                    agent: agent.clone(),
                    capabilities: agent.capabilities.clone(),
                    availability_status: super::resource_manager::AvailabilityStatus::Available,
                    performance_metrics: super::resource_manager::PerformanceMetrics::default(),
                };

                resource_manager
                    .write()
                    .await
                    .add_agent_resource(agent_resource)
                    .await
                    .map_err(|e| {
                        OrchestrationError::ConfigurationError(format!(
                            "Failed to add agent resource: {e}"
                        ))
                    })?;
            }

            let mut agent_adapter = self.agent_adapter.write().await;

            *agent_adapter = crate::orchestration::adapters::agent_adapter::AgentAdapter::new(
                Arc::new(RwLock::new(agent_sys)),
            );
        }

        if let Some(llm_proc) = llm_processor {
            let llm_config = llm_proc.get_adapter_config().clone();
            let llm_resource = super::resource_manager::LLMResource {
                id: format!("llm_{}", llm_config.model_name),
                config: llm_config.clone(),
                provider: "unified".to_string(),
                model: llm_config.model_name.clone(),
                availability_status: super::resource_manager::AvailabilityStatus::Available,
                performance_metrics: super::resource_manager::PerformanceMetrics::default(),
            };

            let resource_manager = self.resource_manager.clone();
            resource_manager
                .write()
                .await
                .add_llm_resource(llm_resource)
                .await
                .map_err(|e| {
                    OrchestrationError::ConfigurationError(format!(
                        "Failed to add LLM resource: {e}"
                    ))
                })?;

            let mut llm_adapter = self.llm_adapter.write().await;
            llm_adapter.set_llm_processor(llm_proc).await?;
        }

        if let Some(task_sys) = task_system {
            let mut task_adapter = self.task_adapter.write().await;
            task_adapter.set_task_system(task_sys).await?;
        }

        if self.config.monitoring_config.enable_performance_tracking {
            self.start_performance_monitoring().await?;
        }

        let mut event_system = self.event_system.write().await;
        event_system
            .emit(OrchestrationEvent::CoordinatorInitialised {
                config: self.config.clone(),
                timestamp: chrono::Utc::now(),
            })
            .await?;

        Ok(())
    }

    pub async fn execute_flow(
        &self,
        flow_def: OrchestrationFlowDefinition,
        gas_limit: Option<u64>,
    ) -> OrchestrationResult<ExecutionStatus> {
        let session_id = Uuid::new_v4().to_string();
        let gas_limit = gas_limit.unwrap_or(self.config.default_gas_limit);

        let session = self
            .create_session(session_id.clone(), flow_def.clone(), gas_limit)
            .await?;

        {
            let mut active_sessions = self.active_sessions.write().await;
            active_sessions.insert(session_id.clone(), Arc::new(RwLock::new(session)));
        }

        {
            let mut event_system = self.event_system.write().await;
            event_system
                .emit(OrchestrationEvent::SessionStarted {
                    session_id: session_id.clone(),
                    flow_id: flow_def.id.clone(),
                    timestamp: chrono::Utc::now(),
                })
                .await?;
            
            {
                let prov = stele::provenance::context::global();
                prov.set_session(&session_id, Some(&flow_def.id)).await;
            }
        }

        let result = self.execute_session(&session_id).await;

        let should_remove_session = !matches!(&result, Ok(ExecutionStatus::AwaitingInput { .. }));

        if should_remove_session {
            let mut active_sessions = self.active_sessions.write().await;
            active_sessions.remove(&session_id);
        }

        {
            let mut event_system = self.event_system.write().await;
            let event = match &result {
                Ok(ExecutionStatus::AwaitingInput { .. }) => {
                    return result;
                }
                Ok(status) => OrchestrationEvent::SessionCompleted {
                    session_id: session_id.clone(),
                    final_result: serde_json::to_value(status).unwrap_or_default(),
                    timestamp: chrono::Utc::now(),
                },
                Err(error) => OrchestrationEvent::ErrorOccurred {
                    session_id: session_id.clone(),
                    error: error.clone(),
                    timestamp: chrono::Utc::now(),
                },
            };
            event_system.emit(event).await?;
        }

        result
    }

    async fn create_session(
        &self,
        session_id: String,
        flow_def: OrchestrationFlowDefinition,
        gas_limit: u64,
    ) -> OrchestrationResult<OrchestrationSession> {
        self.validate_flow_definition(&flow_def).await?;

        let allocated_resources = {
            let mut resource_manager = self.resource_manager.write().await;
            resource_manager.allocate_for_flow(&flow_def).await?
        };

        let execution_context = {
            let mut context_manager = self.context_manager.write().await;
            context_manager
                .create_context(&session_id, &flow_def)
                .await?
        };

        let execution_plan = {
            let mut flow_scheduler = self.flow_scheduler.write().await;
            flow_scheduler.create_execution_plan(&flow_def).await?
        };

        let mut session_manager = self.session_manager.write().await;
        session_manager
            .create_session(
                session_id,
                flow_def,
                execution_context,
                allocated_resources,
                execution_plan,
                gas_limit,
            )
            .await
    }

    async fn execute_session(&self, session_id: &str) -> OrchestrationResult<ExecutionStatus> {
        let session = {
            let active_sessions = self.active_sessions.read().await;
            active_sessions
                .get(session_id)
                .ok_or_else(|| {
                    OrchestrationError::SessionError(format!("Session not found: {session_id}"))
                })?
                .clone()
        };

        let current_status;

        loop {
            let (current_block_id, final_result) = {
                let session_guard = session.read().await;
                let block_id = session_guard.get_current_block_id();
                let result = session_guard.get_final_result();
                (block_id, result)
            };

            if current_block_id.is_none() {
                current_status =
                    ExecutionStatus::Completed(convert_to_runtime_value(&final_result));
                break;
            }

            let current_block_id = current_block_id.unwrap();

            match self.execute_block(session_id, &current_block_id).await? {
                ExecutionStatus::Running => {
                    continue;
                }
                ExecutionStatus::AwaitingInput {
                    session_id,
                    interaction_id,
                    agent_id,
                    prompt,
                } => {
                    {
                        let mut session_guard = session.write().await;
                        session_guard.set_awaiting_input(
                            interaction_id.clone(),
                            agent_id.clone(),
                            convert_from_runtime_value(&prompt),
                        );
                    }

                    current_status = ExecutionStatus::AwaitingInput {
                        session_id: session_id.clone(),
                        interaction_id: interaction_id.clone(),
                        agent_id: agent_id.clone(),
                        prompt,
                    };
                    break;
                }
                ExecutionStatus::Completed(result) => {
                    current_status = ExecutionStatus::Completed(result);
                    break;
                }
            }
        }

        Ok(current_status)
    }

    async fn execute_block(
        &self,
        session_id: &str,
        block_id: &str,
    ) -> OrchestrationResult<ExecutionStatus> {
        println!("DEBUG: Executing block: {block_id}");

        {
            let mut event_system = self.event_system.write().await;
            event_system
                .emit(OrchestrationEvent::BlockExecutionStarted {
                    session_id: session_id.to_string(),
                    block_id: block_id.to_string(),
                    timestamp: chrono::Utc::now(),
                })
                .await?;
        }
        
        stele::provenance::context::global()
            .push_block(block_id)
            .await;

        let session = {
            let active_sessions = self.active_sessions.read().await;
            active_sessions
                .get(session_id)
                .ok_or_else(|| {
                    OrchestrationError::SessionError(format!("Session not found: {session_id}"))
                })?
                .clone()
        };

        let block_definition = {
            let session_guard = session.read().await;
            session_guard
                .get_block_definition(block_id)
                .ok_or_else(|| {
                    OrchestrationError::ValidationError(format!("Block not found: {block_id}"))
                })?
                .clone()
        };

        let result = match &block_definition.block_type {
            super::OrchestrationBlockType::AgentInteraction {
                agent_selector,
                task_definition,
                interaction_type: _,
                timeout_secs,
                retry_config,
                next_block,
            } => {
                let execution_context = {
                    let session_guard = session.read().await;
                    session_guard.get_execution_context().clone()
                };

                let criteria = match agent_selector {
                    super::adapters::AgentSelector::ByCapability(caps) => {
                        super::adapters::agent_adapter::AgentSelectionCriteria {
                            required_capabilities: caps.clone(),
                            preferred_tags: Vec::new(),
                            exclude_busy: true,
                            max_concurrent_tasks: Some(10),
                        }
                    }
                    super::adapters::AgentSelector::ByTag(tags) => {
                        super::adapters::agent_adapter::AgentSelectionCriteria {
                            required_capabilities: Vec::new(),
                            preferred_tags: tags.clone(),
                            exclude_busy: true,
                            max_concurrent_tasks: Some(10),
                        }
                    }
                    super::adapters::AgentSelector::ById(id) => {
                        super::adapters::agent_adapter::AgentSelectionCriteria {
                            required_capabilities: Vec::new(),
                            preferred_tags: vec![format!("id:{id}")],
                            exclude_busy: false,
                            max_concurrent_tasks: None,
                        }
                    }
                    _ => super::adapters::agent_adapter::AgentSelectionCriteria {
                        required_capabilities: Vec::new(),
                        preferred_tags: Vec::new(),
                        exclude_busy: true,
                        max_concurrent_tasks: Some(10),
                    },
                };

                let options = super::adapters::InteractionOptions {
                    timeout_seconds: *timeout_secs,
                    retry_attempts: retry_config.as_ref().map(|r| r.max_attempts).unwrap_or(3),
                    priority: super::adapters::Priority::Normal,
                    execution_mode: super::adapters::ExecutionMode::Synchronous,
                };

                let agent_result = {
                    let adapter_context = super::adapters::ExecutionContext {
                        session_id: execution_context.session_id.clone(),
                        flow_id: session_id.to_string(),
                        block_id: block_id.to_string(),
                        variables: execution_context.variables.clone(),
                        metadata: execution_context
                            .metadata
                            .iter()
                            .map(|(k, v)| (k.clone(), v.to_string()))
                            .collect(),
                    };

                    let agent_adapter = self.agent_adapter.read().await;
                    agent_adapter
                        .interact_with_agent(
                            &criteria,
                            &serde_json::to_value(task_definition)?,
                            &options,
                            &adapter_context,
                        )
                        .await?
                };
                {
                    let mut session_guard = session.write().await;
                    session_guard
                        .update_context_with_agent_result(&agent_result)
                        .await?;
                    session_guard.set_next_block(next_block.clone());
                }

                ExecutionStatus::Running
            }

            super::OrchestrationBlockType::LLMProcessing {
                llm_config,
                prompt_template,
                context_keys,
                output_key,
                processing_options,
                next_block,
            } => {
                let llm_result = {
                    let llm_adapter = self.llm_adapter.read().await;
                    let session_guard = session.read().await;
                    let execution_context = session_guard.get_execution_context();

                    let adapter_context = super::adapters::ExecutionContext::from_context_manager(
                        execution_context,
                        session_guard.flow_definition.id.clone(),
                        block_id.to_string(),
                    );

                    llm_adapter
                        .process_llm_request(
                            llm_config,
                            prompt_template,
                            context_keys,
                            processing_options,
                            &adapter_context,
                        )
                        .await?
                };

                {
                    let mut session_guard = session.write().await;
                    session_guard
                        .update_context_value(output_key, llm_result.result)
                        .await?;
                    session_guard.set_next_block(next_block.clone());
                }

                ExecutionStatus::Running
            }

            super::OrchestrationBlockType::TaskExecution {
                task_config,
                resource_requirements,
                execution_strategy,
                next_block,
            } => {
                let task_result = {
                    let task_adapter = self.task_adapter.read().await;
                    let session_guard = session.read().await;
                    let execution_context = session_guard.get_execution_context();

                    let adapter_context = super::adapters::ExecutionContext::from_context_manager(
                        execution_context,
                        session_guard.flow_definition.id.clone(),
                        block_id.to_string(),
                    );

                    task_adapter
                        .execute_task(
                            task_config,
                            resource_requirements,
                            execution_strategy,
                            &adapter_context,
                        )
                        .await?
                };

                {
                    let mut session_guard = session.write().await;
                    let task_result_converted: Result<Value, crate::tasks::TaskError> =
                        Ok(task_result.result.clone());
                    session_guard
                        .update_context_with_task_result(&task_result_converted)
                        .await?;
                    session_guard.set_next_block(next_block.clone());
                }

                ExecutionStatus::Running
            }

            super::OrchestrationBlockType::WorkflowInvocation {
                workflow_id,
                input_mapping,
                output_mapping,
                execution_mode,
                next_block,
            } => {
                let workflow_result = {
                    let workflow_adapter = self.workflow_adapter.read().await;
                    let session_guard = session.read().await;
                    let execution_context = session_guard.get_execution_context();

                    let adapter_context = super::adapters::ExecutionContext::from_context_manager(
                        execution_context,
                        session_guard.flow_definition.id.clone(),
                        block_id.to_string(),
                    );

                    workflow_adapter
                        .invoke_workflow(
                            workflow_id,
                            input_mapping,
                            output_mapping,
                            execution_mode,
                            &adapter_context,
                        )
                        .await?
                };

                {
                    let mut session_guard = session.write().await;
                    session_guard
                        .update_context_with_workflow_result(&workflow_result)
                        .await?;
                    session_guard.set_next_block(next_block.clone());
                }

                ExecutionStatus::Running
            }

            super::OrchestrationBlockType::ParallelExecution {
                branch_blocks,
                merge_strategy,
                timeout_secs,
                next_block,
            } => {
                let parallel_result = self
                    .execute_parallel_branches(
                        session_id,
                        branch_blocks,
                        merge_strategy,
                        Some(timeout_secs.unwrap_or(300)),
                    )
                    .await?;

                {
                    let mut session_guard = session.write().await;

                    let parallel_execution_result =
                        crate::orchestration::session_manager::ParallelExecutionResult {
                            branch_results: std::collections::HashMap::from([(
                                "result".to_string(),
                                parallel_result,
                            )]),
                            execution_time_ms: chrono::Utc::now()
                                .signed_duration_since(chrono::Utc::now())
                                .num_milliseconds()
                                as u64,
                            successful_branches: vec!["default".to_string()],
                            failed_branches: vec![],
                        };
                    session_guard
                        .update_context_with_parallel_result(&parallel_execution_result)
                        .await?;
                    session_guard.set_next_block(next_block.clone());
                }

                ExecutionStatus::Running
            }

            super::OrchestrationBlockType::AwaitInput {
                interaction_id,
                agent_id,
                prompt,
                state_key,
                next_block,
            } => {
                let has_input_data = {
                    let session_guard = session.read().await;
                    session_guard.get_execution_context().has_input_data()
                };

                if has_input_data {
                    let input_data = {
                        let mut session_guard = session.write().await;
                        session_guard
                            .get_execution_context_mut()
                            .consume_input_data()
                    };

                    if let Some(input) = input_data {
                        {
                            let mut session_guard = session.write().await;
                            session_guard
                                .get_execution_context_mut()
                                .set_variable(state_key.clone(), input);
                            session_guard.set_next_block(next_block.clone());
                        }

                        ExecutionStatus::Running
                    } else {
                        ExecutionStatus::AwaitingInput {
                            session_id: session_id.to_string(),
                            interaction_id: interaction_id.clone(),
                            agent_id: agent_id.clone(),
                            prompt: convert_string_to_runtime_value(prompt),
                        }
                    }
                } else {
                    ExecutionStatus::AwaitingInput {
                        session_id: session_id.to_string(),
                        interaction_id: interaction_id.clone(),
                        agent_id: agent_id.clone(),
                        prompt: convert_string_to_runtime_value(prompt),
                    }
                }
            }

            super::OrchestrationBlockType::ForEach {
                loop_id,
                array_path,
                iterator_var,
                loop_body_block_id,
                exit_block_id,
            } => {
                let idx_key = format!("__loop_{loop_id}_index");
                let arr_key = format!("__loop_{loop_id}_array");

                let (current_index, array_snapshot_opt, has_snapshot) = {
                    let session_guard = session.read().await;
                    let ec = session_guard.get_execution_context();

                    let idx = ec
                        .variables
                        .get(&idx_key)
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as usize;

                    let snapshot = ec.variables.get(&arr_key).cloned();
                    let has_snapshot = snapshot.is_some();
                    let array_val = snapshot.or_else(|| ec.get_context_for_path(array_path));
                    (idx, array_val, has_snapshot)
                };

                {
                    let mut session_guard = session.write().await;
                    let ec = session_guard.get_execution_context_mut();

                    let next_block =
                        if let Some(serde_json::Value::Array(arr)) = array_snapshot_opt.clone() {
                            if !has_snapshot {
                                ec.set_value(&arr_key, serde_json::Value::Array(arr.clone()));
                            }
                            if current_index < arr.len() {
                                let item = arr[current_index].clone();
                                ec.set_value(iterator_var, item);
                                loop_body_block_id.clone()
                            } else {
                                ec.set_value(&idx_key, serde_json::Value::Null);
                                ec.set_value(&arr_key, serde_json::Value::Null);
                                exit_block_id.clone()
                            }
                        } else {
                            ec.set_value(&idx_key, serde_json::Value::Null);
                            ec.set_value(&arr_key, serde_json::Value::Null);
                            exit_block_id.clone()
                        };

                    session_guard.set_next_block(next_block);
                }

                ExecutionStatus::Running
            }
            super::OrchestrationBlockType::Continue { loop_id } => {
                let (foreach_block_id, idx_key) = {
                    let session_guard = session.read().await;
                    let foreach = session_guard
                        .flow_definition
                        .blocks
                        .iter()
                        .find(|b| matches!(
                            &b.block_type,
                            super::OrchestrationBlockType::ForEach { loop_id: lid, .. } if lid == loop_id
                        ))
                        .ok_or_else(|| OrchestrationError::ValidationError(format!(
                            "Continue references unknown loop_id '{loop_id}'",
                        )))?;
                    (foreach.id.clone(), format!("__loop_{loop_id}_index"))
                };

                {
                    let mut session_guard = session.write().await;
                    let ec = session_guard.get_execution_context_mut();
                    let cur = ec
                        .variables
                        .get(&idx_key)
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    ec.set_value(&idx_key, serde_json::Value::from(cur + 1));
                    session_guard.set_next_block(foreach_block_id);
                }

                ExecutionStatus::Running
            }
            super::OrchestrationBlockType::Break { loop_id } => {
                let (exit_block_id, idx_key, arr_key) = {
                    let session_guard = session.read().await;
                    let foreach = session_guard
                        .flow_definition
                        .blocks
                        .iter()
                        .find(|b| matches!(
                            &b.block_type,
                            super::OrchestrationBlockType::ForEach { loop_id: lid, .. } if lid == loop_id
                        ))
                        .ok_or_else(|| OrchestrationError::ValidationError(format!(
                            "Break references unknown loop_id '{loop_id}'",
                        )))?;
                    if let super::OrchestrationBlockType::ForEach { exit_block_id, .. } =
                        &foreach.block_type
                    {
                        (
                            exit_block_id.clone(),
                            format!("__loop_{loop_id}_index"),
                            format!("__loop_{loop_id}_array"),
                        )
                    } else {
                        unreachable!("Matched ForEach variant expected");
                    }
                };

                {
                    let mut session_guard = session.write().await;
                    let ec = session_guard.get_execution_context_mut();
                    ec.set_value(&idx_key, serde_json::Value::Null);
                    ec.set_value(&arr_key, serde_json::Value::Null);
                    session_guard.set_next_block(exit_block_id);
                }

                ExecutionStatus::Running
            }

            _ => {
                self.execute_standard_block(session_id, &block_definition)
                    .await?
            }
        };

        {
            let mut event_system = self.event_system.write().await;
            event_system
                .emit(OrchestrationEvent::BlockExecutionCompleted {
                    session_id: session_id.to_string(),
                    block_id: block_id.to_string(),
                    result: serde_json::to_value(&result).unwrap_or_default(),
                    timestamp: chrono::Utc::now(),
                })
                .await?;
        }

        Ok(result)
    }

    async fn execute_standard_block(
        &self,
        session_id: &str,
        block_definition: &super::OrchestrationBlockDefinition,
    ) -> OrchestrationResult<ExecutionStatus> {
        let standard_block = crate::flows::definition::BlockDefinition {
            id: block_definition.id.clone(),
            block_type: match &block_definition.block_type {
                super::OrchestrationBlockType::Conditional {
                    condition,
                    true_block,
                    false_block,
                } => crate::flows::definition::BlockType::Conditional {
                    condition: condition.clone(),
                    true_block: true_block.clone(),
                    false_block: false_block.clone(),
                },
                super::OrchestrationBlockType::Compute {
                    expression,
                    output_key,
                    next_block,
                } => crate::flows::definition::BlockType::Compute {
                    expression: expression.clone(),
                    output_key: output_key.clone(),
                    next_block: next_block.clone(),
                },
                super::OrchestrationBlockType::Terminate => {
                    crate::flows::definition::BlockType::Terminate
                }
                _ => {
                    return Err(OrchestrationError::ValidationError(
                        "Cannot convert orchestration block to standard block".to_string(),
                    ));
                }
            },
        };

        let session = {
            let active_sessions = self.active_sessions.read().await;
            active_sessions
                .get(session_id)
                .ok_or_else(|| {
                    OrchestrationError::SessionError(format!("Session not found: {session_id}"))
                })?
                .clone()
        };

        let execution_result = {
            let session_guard = session.read().await;
            let context = session_guard.get_execution_context();

            match &standard_block.block_type {
                crate::flows::definition::BlockType::Conditional {
                    condition,
                    true_block,
                    false_block,
                } => {
                    let condition_result = if condition.contains("==") {
                        let parts: Vec<&str> = condition.split("==").map(|s| s.trim()).collect();
                        if parts.len() == 2 {
                            let var_name = parts[0].trim();
                            let expected_value = parts[1].trim().trim_matches('"');

                            if let Some(Value::String(s)) = context.variables.get(var_name) {
                                let user_input = s.to_lowercase();
                                let expected = expected_value.to_lowercase();

                                if expected.contains("final report") {
                                    user_input.contains("1")
                                        || user_input.contains("final")
                                        || user_input.contains("report")
                                } else if expected.contains("refinement") {
                                    user_input.contains("2") || user_input.contains("refine")
                                } else if expected.contains("start over") {
                                    user_input.contains("3")
                                        || user_input.contains("start")
                                        || user_input.contains("over")
                                } else {
                                    user_input == expected
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        context
                            .variables
                            .get(condition)
                            .unwrap_or(&Value::Bool(false))
                            .as_bool()
                            .unwrap_or(false)
                    };

                    drop(session_guard);
                    let mut session_guard = session.write().await;

                    if condition_result {
                        info!(
                            "Condition '{}' is true, setting next block: {}",
                            condition, true_block
                        );
                        session_guard.set_next_block(true_block.clone());
                    } else {
                        info!(
                            "Condition '{}' is false, setting next block: {}",
                            condition, false_block
                        );
                        session_guard.set_next_block(false_block.clone());
                    }
                    ExecutionStatus::Running
                }
                crate::flows::definition::BlockType::Compute {
                    expression,
                    output_key,
                    next_block,
                } => {
                    drop(session_guard);
                    let mut session_guard = session.write().await;

                    let computed_value = match expression.as_str() {
                        "true" => Value::Bool(true),
                        "false" => Value::Bool(false),
                        expr => serde_json::from_str(expr)
                            .unwrap_or_else(|_| Value::String(expr.to_string())),
                    };

                    session_guard
                        .get_execution_context_mut()
                        .set_value(output_key, computed_value);
                    session_guard.set_next_block(next_block.clone());
                    ExecutionStatus::Running
                }
                crate::flows::definition::BlockType::Terminate => {
                    ExecutionStatus::Completed(RuntimeValue::Integer(0))
                }
                _ => ExecutionStatus::Running,
            }
        };

        Ok(execution_result)
    }

    async fn execute_parallel_branches(
        &self,
        session_id: &str,
        branch_blocks: &[String],
        merge_strategy: &super::MergeStrategy,
        _timeout_secs: Option<u64>,
    ) -> OrchestrationResult<Value> {
        let mut branch_results = std::collections::HashMap::new();

        for block_id in branch_blocks {
            let _session_clone = {
                let active_sessions = self.active_sessions.read().await;
                active_sessions
                    .get(session_id)
                    .ok_or_else(|| {
                        OrchestrationError::SessionError(format!("Session not found: {session_id}"))
                    })?
                    .clone()
            };

            let block_id = block_id.clone();
            let _coordinator = self as *const Self;

            let result = Box::pin(self.execute_block(session_id, &block_id)).await?;
            branch_results.insert(block_id, serde_json::to_value(&result).unwrap_or_default());
        }

        let merged_result = match merge_strategy {
            super::MergeStrategy::WaitAll => {
                serde_json::to_value(&branch_results).unwrap_or_default()
            }
            super::MergeStrategy::FirstComplete => branch_results
                .values()
                .next()
                .cloned()
                .unwrap_or(Value::Null),
            super::MergeStrategy::Majority => {
                serde_json::to_value(&branch_results).unwrap_or_default()
            }
            super::MergeStrategy::Custom { strategy_name: _ } => {
                serde_json::to_value(&branch_results).unwrap_or_default()
            }
        };

        Ok(merged_result)
    }

    async fn validate_flow_definition(
        &self,
        flow_def: &OrchestrationFlowDefinition,
    ) -> OrchestrationResult<()> {
        if flow_def.blocks.is_empty() {
            return Err(OrchestrationError::ValidationError(
                "Flow definition must contain at least one block".to_string(),
            ));
        }

        if !flow_def
            .blocks
            .iter()
            .any(|b| b.id == flow_def.start_block_id)
        {
            return Err(OrchestrationError::ValidationError(format!(
                "Start block '{}' not found in flow definition",
                flow_def.start_block_id
            )));
        }

        let block_ids: std::collections::HashSet<_> =
            flow_def.blocks.iter().map(|b| &b.id).collect();

        for block in &flow_def.blocks {
            match &block.block_type {
                super::OrchestrationBlockType::Conditional {
                    true_block,
                    false_block,
                    ..
                } => {
                    if !block_ids.contains(true_block) {
                        return Err(OrchestrationError::ValidationError(format!(
                            "Block '{}' references non-existent true_block '{}'",
                            block.id, true_block
                        )));
                    }
                    if !block_ids.contains(false_block) {
                        return Err(OrchestrationError::ValidationError(format!(
                            "Block '{}' references non-existent false_block '{}'",
                            block.id, false_block
                        )));
                    }
                }
                super::OrchestrationBlockType::Compute { next_block, .. }
                | super::OrchestrationBlockType::AgentInteraction { next_block, .. }
                | super::OrchestrationBlockType::LLMProcessing { next_block, .. }
                | super::OrchestrationBlockType::TaskExecution { next_block, .. }
                | super::OrchestrationBlockType::WorkflowInvocation { next_block, .. }
                | super::OrchestrationBlockType::ResourceAllocation { next_block, .. }
                | super::OrchestrationBlockType::ParallelExecution { next_block, .. }
                | super::OrchestrationBlockType::EventTrigger { next_block, .. }
                | super::OrchestrationBlockType::StateCheckpoint { next_block, .. } => {
                    if !next_block.is_empty() && !block_ids.contains(next_block) {
                        return Err(OrchestrationError::ValidationError(format!(
                            "Block '{}' references non-existent next_block '{}'",
                            block.id, next_block
                        )));
                    }
                }
                _ => {}
            }
        }

        let total_agents = flow_def.resource_requirements.agents.len() as u32;
        let total_llms = flow_def.resource_requirements.llm.len() as u32;
        let total_tasks = flow_def.resource_requirements.tasks.len() as u32;

        if total_agents > self.config.resource_limits.max_agents_per_session {
            return Err(OrchestrationError::ValidationError(format!(
                "Flow requires {} agents, but limit is {}",
                total_agents, self.config.resource_limits.max_agents_per_session
            )));
        }

        if total_llms > self.config.resource_limits.max_llm_instances_per_session {
            return Err(OrchestrationError::ValidationError(format!(
                "Flow requires {} LLM instances, but limit is {}",
                total_llms, self.config.resource_limits.max_llm_instances_per_session
            )));
        }

        if total_tasks > self.config.resource_limits.max_tasks_per_session {
            return Err(OrchestrationError::ValidationError(format!(
                "Flow requires {} tasks, but limit is {}",
                total_tasks, self.config.resource_limits.max_tasks_per_session
            )));
        }

        Ok(())
    }

    async fn start_performance_monitoring(&self) -> OrchestrationResult<()> {
        let interval = std::time::Duration::from_secs(
            self.config
                .monitoring_config
                .metrics_collection_interval_secs,
        );

        tokio::spawn({
            let active_sessions = self.active_sessions.clone();
            let config = self.config.clone();

            async move {
                let mut interval_timer = tokio::time::interval(interval);

                loop {
                    interval_timer.tick().await;

                    if config.monitoring_config.enable_performance_tracking {
                        let sessions = active_sessions.read().await;
                        let session_count = sessions.len();

                        log::info!("Performance Monitor: {session_count} active sessions");

                        for (session_id, session) in sessions.iter() {
                            let session_guard = session.read().await;
                            let resource_usage = session_guard.get_resource_usage();

                            if config.monitoring_config.enable_detailed_logging {
                                log::debug!(
                                    "Session {}: CPU: {:.1}%, Memory: {} MB, Agents: {}, LLMs: {}, Tasks: {}",
                                    session_id,
                                    resource_usage.cpu_usage_percent,
                                    resource_usage.memory_usage_mb,
                                    resource_usage.active_agents,
                                    resource_usage.active_llm_instances,
                                    resource_usage.active_tasks
                                );
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn get_session_status(&self, session_id: &str) -> OrchestrationResult<SessionStatus> {
        let active_sessions = self.active_sessions.read().await;
        if let Some(session) = active_sessions.get(session_id) {
            let session_guard = session.read().await;
            Ok(SessionStatus {
                session_id: session_id.to_string(),
                status: session_guard.get_status(),
                current_block: session_guard.get_current_block_id(),
                progress: session_guard.get_progress(),
                resource_usage: session_guard.get_resource_usage(),
            })
        } else {
            Err(OrchestrationError::SessionError(format!(
                "Session not found: {session_id}"
            )))
        }
    }

    pub async fn resume_session(
        &self,
        session_id: &str,
        input_data: Value,
    ) -> OrchestrationResult<ExecutionStatus> {
        let session = {
            let active_sessions = self.active_sessions.read().await;
            active_sessions
                .get(session_id)
                .ok_or_else(|| {
                    OrchestrationError::SessionError(format!("Session not found: {session_id}"))
                })?
                .clone()
        };

        {
            let mut session_guard = session.write().await;
            session_guard.resume_with_input(input_data).await?;
        }

        let result = self.execute_session(session_id).await;

        let should_remove_session = !matches!(&result, Ok(ExecutionStatus::AwaitingInput { .. }));

        if should_remove_session {
            let mut active_sessions = self.active_sessions.write().await;
            active_sessions.remove(session_id);

            let mut event_system = self.event_system.write().await;
            let event = match &result {
                Ok(status) => OrchestrationEvent::SessionCompleted {
                    session_id: session_id.to_string(),
                    final_result: serde_json::to_value(status).unwrap_or_default(),
                    timestamp: chrono::Utc::now(),
                },
                Err(error) => OrchestrationEvent::ErrorOccurred {
                    session_id: session_id.to_string(),
                    error: error.clone(),
                    timestamp: chrono::Utc::now(),
                },
            };
            event_system.emit(event).await?;
        }

        result
    }

    pub async fn debug_resource_state(&self) {
        let resource_manager = self.resource_manager.read().await;
        let utilization = resource_manager.get_resource_utilisation().await;
        println!("Resource Manager Debug State:");
        println!("  Active agents: {}", utilization.active_agents);
        println!(
            "  Active LLM instances: {}",
            utilization.active_llm_instances
        );
        println!("  Active tasks: {}", utilization.active_tasks);
        println!("  Active workflows: {}", utilization.active_workflows);
        println!("  CPU usage: {:.2}%", utilization.cpu_usage_percent);
        println!("  Memory usage: {} MB", utilization.memory_usage_mb);
        println!("  Total allocations: {}", utilization.total_allocations);
        println!("  Total deallocations: {}", utilization.total_deallocations);

        let (
            agent_avail,
            agent_alloc,
            llm_avail,
            llm_alloc,
            task_avail,
            task_alloc,
            wf_avail,
            wf_alloc,
        ) = resource_manager.debug_pool_state().await;

        println!("Resource Pool Details:");
        println!(
            "  Agent Pool - Available: {}, Allocated: {}, Total: {}",
            agent_avail,
            agent_alloc,
            agent_avail + agent_alloc
        );
        println!(
            "  LLM Pool - Available: {}, Allocated: {}, Total: {}",
            llm_avail,
            llm_alloc,
            llm_avail + llm_alloc
        );
        println!(
            "  Task Pool - Available: {}, Allocated: {}, Total: {}",
            task_avail,
            task_alloc,
            task_avail + task_alloc
        );
        println!(
            "  Workflow Pool - Available: {}, Allocated: {}, Total: {}",
            wf_avail,
            wf_alloc,
            wf_avail + wf_alloc
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStatus {
    pub session_id: String,
    pub status: String,
    pub current_block: Option<String>,
    pub progress: f64,
    pub resource_usage: ResourceUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: u64,
    pub active_agents: u32,
    pub active_llm_instances: u32,
    pub active_tasks: u32,
}
