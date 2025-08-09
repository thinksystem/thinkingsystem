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
    context_manager::ExecutionContext, flow_scheduler::ExecutionPlan,
    resource_manager::AllocatedResources, OrchestrationBlockDefinition, OrchestrationError,
    OrchestrationFlowDefinition, OrchestrationResult,
};
use crate::orchestration::adapters::AgentInteractionResult;
use crate::tasks::TaskResult;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;

pub struct SessionManager {
    storage_config: super::coordinator::StorageConfig,
    sessions: HashMap<String, OrchestrationSession>,
    storage: Option<Box<dyn SessionStorage>>,
}

impl SessionManager {
    pub async fn new(
        storage_config: super::coordinator::StorageConfig,
    ) -> OrchestrationResult<Self> {
        let storage = if storage_config.session_storage_path.is_some() {
            Some(
                Box::new(FileSessionStorage::new(storage_config.clone()).await?)
                    as Box<dyn SessionStorage>,
            )
        } else {
            None
        };

        Ok(Self {
            storage_config,
            sessions: HashMap::new(),
            storage,
        })
    }

    pub async fn create_session(
        &mut self,
        session_id: String,
        flow_def: OrchestrationFlowDefinition,
        execution_context: ExecutionContext,
        allocated_resources: AllocatedResources,
        execution_plan: ExecutionPlan,
        gas_limit: u64,
    ) -> OrchestrationResult<OrchestrationSession> {
        let session = OrchestrationSession::new(
            session_id.clone(),
            flow_def,
            execution_context,
            allocated_resources,
            execution_plan,
            gas_limit,
        );

        if let Some(storage) = &mut self.storage {
            storage.store_session(&session).await?;
        }

        self.sessions.insert(session_id.clone(), session.clone());
        Ok(session)
    }

    pub async fn get_session(&self, session_id: &str) -> Option<&OrchestrationSession> {
        self.sessions.get(session_id)
    }

    pub async fn get_session_mut(&mut self, session_id: &str) -> Option<&mut OrchestrationSession> {
        self.sessions.get_mut(session_id)
    }

    pub async fn remove_session(&mut self, session_id: &str) -> OrchestrationResult<()> {
        if let Some(storage) = &mut self.storage {
            storage.remove_session(session_id).await?;
        }
        self.sessions.remove(session_id);
        Ok(())
    }

    pub async fn list_sessions(&self) -> Vec<String> {
        self.sessions.keys().cloned().collect()
    }

    pub fn get_storage_config(&self) -> &super::coordinator::StorageConfig {
        &self.storage_config
    }

    pub async fn checkpoint_session(&mut self, session_id: &str) -> OrchestrationResult<()> {
        if let Some(session) = self.sessions.get(session_id) {
            if let Some(storage) = &mut self.storage {
                storage.checkpoint_session(session).await?;
            }
        }
        Ok(())
    }

    pub async fn restore_session(
        &mut self,
        session_id: &str,
    ) -> OrchestrationResult<OrchestrationSession> {
        if let Some(storage) = &mut self.storage {
            let session = storage.restore_session(session_id).await?;
            self.sessions
                .insert(session_id.to_string(), session.clone());
            Ok(session)
        } else {
            Err(OrchestrationError::SessionError(
                "No storage configured for session restoration".to_string(),
            ))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationSession {
    pub id: String,
    pub flow_definition: OrchestrationFlowDefinition,
    pub execution_context: ExecutionContext,
    pub allocated_resources: AllocatedResources,
    pub execution_plan: ExecutionPlan,
    pub gas_limit: u64,
    pub gas_consumed: u64,
    pub status: SessionStatus,
    pub current_block_id: Option<String>,
    pub execution_history: Vec<ExecutionEvent>,
    pub checkpoints: Vec<SessionCheckpoint>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl OrchestrationSession {
    pub fn new(
        id: String,
        flow_definition: OrchestrationFlowDefinition,
        execution_context: ExecutionContext,
        allocated_resources: AllocatedResources,
        execution_plan: ExecutionPlan,
        gas_limit: u64,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            current_block_id: Some(flow_definition.start_block_id.clone()),
            id,
            flow_definition,
            execution_context,
            allocated_resources,
            execution_plan,
            gas_limit,
            gas_consumed: 0,
            status: SessionStatus::Running,
            execution_history: Vec::new(),
            checkpoints: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn get_current_block_id(&self) -> Option<String> {
        self.current_block_id.clone()
    }

    pub fn set_next_block(&mut self, block_id: String) {
        self.current_block_id = Some(block_id);
        self.updated_at = chrono::Utc::now();
    }

    pub fn complete(&mut self, final_result: Value) {
        self.status = SessionStatus::Completed;
        self.current_block_id = None;
        self.execution_context.set_final_result(final_result);
        self.updated_at = chrono::Utc::now();
    }

    pub fn set_awaiting_input(&mut self, interaction_id: String, agent_id: String, prompt: Value) {
        self.status = SessionStatus::AwaitingInput {
            interaction_id,
            agent_id,
            prompt,
        };
        self.updated_at = chrono::Utc::now();
    }

    pub async fn resume_with_input(&mut self, input_data: Value) -> OrchestrationResult<()> {
        if let SessionStatus::AwaitingInput { .. } = &self.status {
            self.execution_context.add_input_data(input_data);
            self.status = SessionStatus::Running;
            self.updated_at = chrono::Utc::now();
            Ok(())
        } else {
            Err(OrchestrationError::SessionError(
                "Session is not awaiting input".to_string(),
            ))
        }
    }

    pub fn get_block_definition(&self, block_id: &str) -> Option<&OrchestrationBlockDefinition> {
        self.flow_definition
            .blocks
            .iter()
            .find(|b| b.id == block_id)
    }

    pub fn get_execution_context(&self) -> &ExecutionContext {
        &self.execution_context
    }

    pub fn get_execution_context_mut(&mut self) -> &mut ExecutionContext {
        &mut self.execution_context
    }

    pub async fn update_context_with_agent_result(
        &mut self,
        result: &AgentInteractionResult,
    ) -> OrchestrationResult<()> {
        self.execution_context.add_agent_result(result.clone());
        self.add_execution_event(ExecutionEvent::AgentInteractionCompleted {
            agent_id: result.agent_id.clone(),
            result: result.result.clone(),
            timestamp: chrono::Utc::now(),
        });
        self.updated_at = chrono::Utc::now();
        Ok(())
    }

    pub async fn update_context_value(
        &mut self,
        key: &str,
        value: Value,
    ) -> OrchestrationResult<()> {
        self.execution_context.set_value(key, value.clone());
        self.add_execution_event(ExecutionEvent::ContextUpdated {
            key: key.to_string(),
            value,
            timestamp: chrono::Utc::now(),
        });
        self.updated_at = chrono::Utc::now();
        Ok(())
    }

    pub async fn update_context_with_task_result(
        &mut self,
        result: &TaskResult<Value>,
    ) -> OrchestrationResult<()> {
        match result {
            Ok(value) => {
                self.execution_context
                    .add_task_result("default_task".to_string(), value.clone());
                self.add_execution_event(ExecutionEvent::TaskCompleted {
                    result: value.clone(),
                    timestamp: chrono::Utc::now(),
                });
            }
            Err(error) => {
                self.add_execution_event(ExecutionEvent::TaskFailed {
                    error: error.to_string(),
                    timestamp: chrono::Utc::now(),
                });
                return Err(OrchestrationError::TaskExecutionError(error.to_string()));
            }
        }
        self.updated_at = chrono::Utc::now();
        Ok(())
    }

    pub async fn update_context_with_workflow_result(
        &mut self,
        result: &WorkflowResult,
    ) -> OrchestrationResult<()> {
        self.execution_context.add_workflow_result(result.clone());
        self.add_execution_event(ExecutionEvent::WorkflowCompleted {
            workflow_id: result.workflow_id.clone(),
            result: result.output.clone(),
            timestamp: chrono::Utc::now(),
        });
        self.updated_at = chrono::Utc::now();
        Ok(())
    }

    pub async fn update_context_with_parallel_result(
        &mut self,
        result: &ParallelExecutionResult,
    ) -> OrchestrationResult<()> {
        self.execution_context.add_parallel_result(result.clone());
        self.add_execution_event(ExecutionEvent::ParallelExecutionCompleted {
            branch_results: result.branch_results.clone(),
            timestamp: chrono::Utc::now(),
        });
        self.updated_at = chrono::Utc::now();
        Ok(())
    }

    pub fn consume_gas(&mut self, amount: u64) -> OrchestrationResult<()> {
        if self.gas_consumed + amount > self.gas_limit {
            Err(OrchestrationError::ResourceAllocationError(
                "Out of gas".to_string(),
            ))
        } else {
            self.gas_consumed += amount;
            Ok(())
        }
    }

    pub fn get_status(&self) -> String {
        match &self.status {
            SessionStatus::Running => "running".to_string(),
            SessionStatus::AwaitingInput { .. } => "awaiting_input".to_string(),
            SessionStatus::Completed => "completed".to_string(),
            SessionStatus::Failed(_) => "failed".to_string(),
        }
    }

    pub fn get_progress(&self) -> f64 {
        let total_blocks = self.flow_definition.blocks.len() as f64;
        let executed_blocks = self.execution_history.len() as f64;
        if total_blocks > 0.0 {
            (executed_blocks / total_blocks).min(1.0)
        } else {
            0.0
        }
    }

    pub fn get_resource_usage(&self) -> super::coordinator::ResourceUsage {
        let mut rng = rand::thread_rng();
        super::coordinator::ResourceUsage {
            cpu_usage_percent: rng.gen_range(5.0..15.0),
            memory_usage_mb: rng.gen_range(256..384),
            active_agents: self.allocated_resources.agents.len() as u32,
            active_llm_instances: self.allocated_resources.llm_instances.len() as u32,
            active_tasks: self.allocated_resources.tasks.len() as u32,
        }
    }

    pub fn get_final_result(&self) -> Value {
        self.execution_context
            .get_final_result()
            .unwrap_or(Value::Null)
    }

    pub fn create_checkpoint(&mut self) -> SessionCheckpoint {
        let checkpoint = SessionCheckpoint {
            id: Uuid::new_v4().to_string(),
            session_id: self.id.clone(),
            execution_context: self.execution_context.clone(),
            current_block_id: self.current_block_id.clone(),
            gas_consumed: self.gas_consumed,
            created_at: chrono::Utc::now(),
        };
        self.checkpoints.push(checkpoint.clone());
        checkpoint
    }

    pub fn restore_from_checkpoint(
        &mut self,
        checkpoint: &SessionCheckpoint,
    ) -> OrchestrationResult<()> {
        if checkpoint.session_id != self.id {
            return Err(OrchestrationError::SessionError(
                "Checkpoint does not belong to this session".to_string(),
            ));
        }

        self.execution_context = checkpoint.execution_context.clone();
        self.current_block_id = checkpoint.current_block_id.clone();
        self.gas_consumed = checkpoint.gas_consumed;
        self.status = SessionStatus::Running;
        self.updated_at = chrono::Utc::now();
        Ok(())
    }

    fn add_execution_event(&mut self, event: ExecutionEvent) {
        self.execution_history.push(event);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStatus {
    Running,
    AwaitingInput {
        interaction_id: String,
        agent_id: String,
        prompt: Value,
    },
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionEvent {
    BlockStarted {
        block_id: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    BlockCompleted {
        block_id: String,
        result: Value,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    AgentInteractionCompleted {
        agent_id: String,
        result: Value,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    TaskCompleted {
        result: Value,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    TaskFailed {
        error: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    WorkflowCompleted {
        workflow_id: String,
        result: Value,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    ParallelExecutionCompleted {
        branch_results: HashMap<String, Value>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    ContextUpdated {
        key: String,
        value: Value,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    CheckpointCreated {
        checkpoint_id: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCheckpoint {
    pub id: String,
    pub session_id: String,
    pub execution_context: ExecutionContext,
    pub current_block_id: Option<String>,
    pub gas_consumed: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowResult {
    pub workflow_id: String,
    pub output: Value,
    pub execution_time_ms: u64,
    pub metadata: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelExecutionResult {
    pub branch_results: HashMap<String, Value>,
    pub execution_time_ms: u64,
    pub successful_branches: Vec<String>,
    pub failed_branches: Vec<String>,
}

#[async_trait::async_trait]
pub trait SessionStorage: Send + Sync {
    async fn store_session(&mut self, session: &OrchestrationSession) -> OrchestrationResult<()>;
    async fn restore_session(
        &mut self,
        session_id: &str,
    ) -> OrchestrationResult<OrchestrationSession>;
    async fn remove_session(&mut self, session_id: &str) -> OrchestrationResult<()>;
    async fn list_sessions(&self) -> OrchestrationResult<Vec<String>>;
    async fn checkpoint_session(
        &mut self,
        session: &OrchestrationSession,
    ) -> OrchestrationResult<()>;
}

pub struct FileSessionStorage {
    session_dir: PathBuf,
    checkpoint_dir: PathBuf,
}

impl FileSessionStorage {
    pub async fn new(config: super::coordinator::StorageConfig) -> OrchestrationResult<Self> {
        let session_dir = PathBuf::from(config.session_storage_path.as_ref().ok_or_else(|| {
            OrchestrationError::ConfigurationError(
                "Session storage path not configured".to_string(),
            )
        })?);

        let checkpoint_dir = PathBuf::from(
            config
                .checkpoint_storage_path
                .as_ref()
                .unwrap_or(&format!("{}/checkpoints", session_dir.display())),
        );

        fs::create_dir_all(&session_dir).await.map_err(|e| {
            OrchestrationError::ConfigurationError(format!(
                "Failed to create session directory: {e}"
            ))
        })?;

        fs::create_dir_all(&checkpoint_dir).await.map_err(|e| {
            OrchestrationError::ConfigurationError(format!(
                "Failed to create checkpoint directory: {e}"
            ))
        })?;

        Ok(Self {
            session_dir,
            checkpoint_dir,
        })
    }
}

#[async_trait::async_trait]
impl SessionStorage for FileSessionStorage {
    async fn store_session(&mut self, session: &OrchestrationSession) -> OrchestrationResult<()> {
        let session_file = self.session_dir.join(format!("{}.json", session.id));
        let session_json = serde_json::to_string_pretty(session).map_err(|e| {
            OrchestrationError::SessionError(format!("Failed to serialise session: {e}"))
        })?;

        fs::write(session_file, session_json).await.map_err(|e| {
            OrchestrationError::SessionError(format!("Failed to write session file: {e}"))
        })?;

        Ok(())
    }

    async fn restore_session(
        &mut self,
        session_id: &str,
    ) -> OrchestrationResult<OrchestrationSession> {
        let session_file = self.session_dir.join(format!("{session_id}.json"));
        let session_json = fs::read_to_string(session_file).await.map_err(|e| {
            OrchestrationError::SessionError(format!("Failed to read session file: {e}"))
        })?;

        let session: OrchestrationSession = serde_json::from_str(&session_json).map_err(|e| {
            OrchestrationError::SessionError(format!("Failed to deserialise session: {e}"))
        })?;

        Ok(session)
    }

    async fn remove_session(&mut self, session_id: &str) -> OrchestrationResult<()> {
        let session_file = self.session_dir.join(format!("{session_id}.json"));
        if session_file.exists() {
            fs::remove_file(session_file).await.map_err(|e| {
                OrchestrationError::SessionError(format!("Failed to remove session file: {e}"))
            })?;
        }
        Ok(())
    }

    async fn list_sessions(&self) -> OrchestrationResult<Vec<String>> {
        let mut entries = fs::read_dir(&self.session_dir).await.map_err(|e| {
            OrchestrationError::SessionError(format!("Failed to read session directory: {e}"))
        })?;

        let mut sessions = Vec::new();
        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            OrchestrationError::SessionError(format!("Failed to read directory entry: {e}"))
        })? {
            if let Some(file_name) = entry.file_name().to_str() {
                if file_name.ends_with(".json") {
                    let session_id = file_name.trim_end_matches(".json");
                    sessions.push(session_id.to_string());
                }
            }
        }

        Ok(sessions)
    }

    async fn checkpoint_session(
        &mut self,
        session: &OrchestrationSession,
    ) -> OrchestrationResult<()> {
        let checkpoint = session
            .checkpoints
            .last()
            .ok_or_else(|| OrchestrationError::SessionError("No checkpoint to save".to_string()))?;

        let checkpoint_file = self
            .checkpoint_dir
            .join(format!("{}_{}.json", session.id, checkpoint.id));
        let checkpoint_json = serde_json::to_string_pretty(checkpoint).map_err(|e| {
            OrchestrationError::SessionError(format!("Failed to serialise checkpoint: {e}"))
        })?;

        fs::write(checkpoint_file, checkpoint_json)
            .await
            .map_err(|e| {
                OrchestrationError::SessionError(format!("Failed to write checkpoint file: {e}"))
            })?;

        Ok(())
    }
}
