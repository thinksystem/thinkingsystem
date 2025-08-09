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

use crate::{Agent, AgentCapabilities};
use serde::{Deserialize, Serialize};
use stele::LLMConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResourceType {
    Agent,
    LLM,
    Task,
    Workflow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocatedResources {
    pub session_id: String,
    pub agents: Vec<AgentResource>,
    pub llm_instances: Vec<LLMResource>,
    pub tasks: Vec<TaskResource>,
    pub workflows: Vec<WorkflowResource>,
    pub allocated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResource {
    pub id: String,
    pub agent: Agent,
    pub capabilities: AgentCapabilities,
    pub availability_status: AvailabilityStatus,
    pub performance_metrics: PerformanceMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMResource {
    pub id: String,
    pub config: LLMConfig,
    pub provider: String,
    pub model: String,
    pub availability_status: AvailabilityStatus,
    pub performance_metrics: PerformanceMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResource {
    pub id: String,
    pub executor_id: String,
    pub resource_requirements: crate::orchestration::ResourceRequirement,
    pub availability_status: AvailabilityStatus,
    pub performance_metrics: PerformanceMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowResource {
    pub id: String,
    pub workflow_id: String,
    pub executor_type: String,
    pub availability_status: AvailabilityStatus,
    pub performance_metrics: PerformanceMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AvailabilityStatus {
    Available,
    Busy,
    Maintenance,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub average_response_time_ms: f64,
    pub success_rate: f64,
    pub total_executions: u64,
    pub last_execution: Option<chrono::DateTime<chrono::Utc>>,
    pub error_count: u64,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            average_response_time_ms: 0.0,
            success_rate: 1.0,
            total_executions: 0,
            last_execution: None,
            error_count: 0,
        }
    }
}

pub trait HasId {
    fn get_id(&self) -> String;
}

impl HasId for AgentResource {
    fn get_id(&self) -> String {
        self.id.clone()
    }
}

impl HasId for LLMResource {
    fn get_id(&self) -> String {
        self.id.clone()
    }
}

impl HasId for TaskResource {
    fn get_id(&self) -> String {
        self.id.clone()
    }
}

impl HasId for WorkflowResource {
    fn get_id(&self) -> String {
        self.id.clone()
    }
}

#[derive(Debug, Clone)]
pub struct FlowResourceRequirements {
    pub agent_requirements: Vec<AgentRequirement>,
    pub llm_requirements: Vec<LLMRequirement>,
    pub task_requirements: Vec<TaskRequirement>,
    pub workflow_requirements: Vec<WorkflowRequirement>,
}

#[derive(Debug, Clone)]
pub struct AgentRequirement {
    pub selector: crate::orchestration::AgentSelector,
    pub task_definition: crate::orchestration::TaskDefinition,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LLMRequirement {
    pub config: crate::orchestration::LLMProcessingConfig,
    pub estimated_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct TaskRequirement {
    pub config: crate::orchestration::TaskExecutionConfig,
    pub resource_requirements: crate::orchestration::ResourceRequirement,
}

#[derive(Debug, Clone)]
pub struct WorkflowRequirement {
    pub workflow_id: String,
    pub estimated_complexity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUtilisation {
    pub active_agents: u32,
    pub active_llm_instances: u32,
    pub active_tasks: u32,
    pub active_workflows: u32,
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: u64,
    pub total_allocations: u64,
    pub total_deallocations: u64,
}

impl Default for ResourceUtilisation {
    fn default() -> Self {
        Self {
            active_agents: 0,
            active_llm_instances: 0,
            active_tasks: 0,
            active_workflows: 0,
            cpu_usage_percent: 0.0,
            memory_usage_mb: 0,
            total_allocations: 0,
            total_deallocations: 0,
        }
    }
}
