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

use super::resource_pool::ResourcePool;
use super::types::*;
use crate::orchestration::{OrchestrationError, OrchestrationResult};
use std::sync::atomic::{AtomicUsize, Ordering};

#[async_trait::async_trait]
pub trait AllocationStrategy: Send + Sync {
    async fn allocate_agent(
        &self,
        requirement: &AgentRequirement,
        pool: &ResourcePool<AgentResource>,
    ) -> OrchestrationResult<AgentResource>;

    async fn allocate_llm(
        &self,
        requirement: &LLMRequirement,
        pool: &ResourcePool<LLMResource>,
    ) -> OrchestrationResult<LLMResource>;

    async fn allocate_task(
        &self,
        requirement: &TaskRequirement,
        pool: &ResourcePool<TaskResource>,
    ) -> OrchestrationResult<TaskResource>;

    async fn allocate_workflow(
        &self,
        requirement: &WorkflowRequirement,
        pool: &ResourcePool<WorkflowResource>,
    ) -> OrchestrationResult<WorkflowResource>;
}

pub struct RoundRobinStrategy {
    current_index: AtomicUsize,
}

impl RoundRobinStrategy {
    pub fn new() -> Self {
        Self {
            current_index: AtomicUsize::new(0),
        }
    }

    fn get_next_index(&self, pool_size: usize) -> usize {
        if pool_size == 0 {
            return 0;
        }
        self.current_index.fetch_add(1, Ordering::SeqCst) % pool_size
    }
}

impl Default for RoundRobinStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AllocationStrategy for RoundRobinStrategy {
    async fn allocate_agent(
        &self,
        _requirement: &AgentRequirement,
        pool: &ResourcePool<AgentResource>,
    ) -> OrchestrationResult<AgentResource> {
        let available = pool.get_available_resources();
        if available.is_empty() {
            return Err(OrchestrationError::ResourceAllocationError(
                "No agents available".to_string(),
            ));
        }

        let index = self.get_next_index(available.len());
        Ok(available[index].clone())
    }

    async fn allocate_llm(
        &self,
        _requirement: &LLMRequirement,
        pool: &ResourcePool<LLMResource>,
    ) -> OrchestrationResult<LLMResource> {
        let available = pool.get_available_resources();
        if available.is_empty() {
            return Err(OrchestrationError::ResourceAllocationError(
                "No LLM instances available".to_string(),
            ));
        }

        let index = self.get_next_index(available.len());
        Ok(available[index].clone())
    }

    async fn allocate_task(
        &self,
        _requirement: &TaskRequirement,
        pool: &ResourcePool<TaskResource>,
    ) -> OrchestrationResult<TaskResource> {
        let available = pool.get_available_resources();
        if available.is_empty() {
            return Err(OrchestrationError::ResourceAllocationError(
                "No task resources available".to_string(),
            ));
        }

        let index = self.get_next_index(available.len());
        Ok(available[index].clone())
    }

    async fn allocate_workflow(
        &self,
        _requirement: &WorkflowRequirement,
        pool: &ResourcePool<WorkflowResource>,
    ) -> OrchestrationResult<WorkflowResource> {
        let available = pool.get_available_resources();
        if available.is_empty() {
            return Err(OrchestrationError::ResourceAllocationError(
                "No workflow resources available".to_string(),
            ));
        }

        let index = self.get_next_index(available.len());
        Ok(available[index].clone())
    }
}

pub struct CapabilityBasedStrategy;

impl CapabilityBasedStrategy {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CapabilityBasedStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AllocationStrategy for CapabilityBasedStrategy {
    async fn allocate_agent(
        &self,
        requirement: &AgentRequirement,
        pool: &ResourcePool<AgentResource>,
    ) -> OrchestrationResult<AgentResource> {
        let available_agents = pool.get_available_resources();
        let suitable_agent = available_agents.iter().find(|agent| {
            requirement.capabilities.iter().all(|req_cap| {
                agent
                    .capabilities
                    .technical_skills
                    .iter()
                    .any(|skill| skill.name == *req_cap)
            })
        });

        suitable_agent.map(|agent| (*agent).clone()).ok_or_else(|| {
            OrchestrationError::ResourceAllocationError(format!(
                "No agent available with required capabilities: {:?}",
                requirement.capabilities
            ))
        })
    }

    async fn allocate_llm(
        &self,
        _requirement: &LLMRequirement,
        pool: &ResourcePool<LLMResource>,
    ) -> OrchestrationResult<LLMResource> {
        pool.get_available_resources()
            .first()
            .map(|r| (*r).clone())
            .ok_or_else(|| {
                OrchestrationError::ResourceAllocationError(
                    "No LLM instances available".to_string(),
                )
            })
    }

    async fn allocate_task(
        &self,
        _requirement: &TaskRequirement,
        pool: &ResourcePool<TaskResource>,
    ) -> OrchestrationResult<TaskResource> {
        pool.get_available_resources()
            .first()
            .map(|r| (*r).clone())
            .ok_or_else(|| {
                OrchestrationError::ResourceAllocationError(
                    "No task resources available".to_string(),
                )
            })
    }

    async fn allocate_workflow(
        &self,
        _requirement: &WorkflowRequirement,
        pool: &ResourcePool<WorkflowResource>,
    ) -> OrchestrationResult<WorkflowResource> {
        pool.get_available_resources()
            .first()
            .map(|r| (*r).clone())
            .ok_or_else(|| {
                OrchestrationError::ResourceAllocationError(
                    "No workflow resources available".to_string(),
                )
            })
    }
}

pub struct LoadBalancedStrategy;

impl LoadBalancedStrategy {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LoadBalancedStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AllocationStrategy for LoadBalancedStrategy {
    async fn allocate_agent(
        &self,
        _requirement: &AgentRequirement,
        pool: &ResourcePool<AgentResource>,
    ) -> OrchestrationResult<AgentResource> {
        pool.get_available_resources()
            .iter()
            .min_by_key(|r| r.performance_metrics.total_executions)
            .map(|r| (*r).clone())
            .ok_or_else(|| {
                OrchestrationError::ResourceAllocationError("No agents available".to_string())
            })
    }

    async fn allocate_llm(
        &self,
        _requirement: &LLMRequirement,
        pool: &ResourcePool<LLMResource>,
    ) -> OrchestrationResult<LLMResource> {
        pool.get_available_resources()
            .iter()
            .min_by_key(|r| r.performance_metrics.total_executions)
            .map(|r| (*r).clone())
            .ok_or_else(|| {
                OrchestrationError::ResourceAllocationError(
                    "No LLM instances available".to_string(),
                )
            })
    }

    async fn allocate_task(
        &self,
        _requirement: &TaskRequirement,
        pool: &ResourcePool<TaskResource>,
    ) -> OrchestrationResult<TaskResource> {
        pool.get_available_resources()
            .iter()
            .min_by_key(|r| r.performance_metrics.total_executions)
            .map(|r| (*r).clone())
            .ok_or_else(|| {
                OrchestrationError::ResourceAllocationError(
                    "No task resources available".to_string(),
                )
            })
    }

    async fn allocate_workflow(
        &self,
        _requirement: &WorkflowRequirement,
        pool: &ResourcePool<WorkflowResource>,
    ) -> OrchestrationResult<WorkflowResource> {
        pool.get_available_resources()
            .iter()
            .min_by_key(|r| r.performance_metrics.total_executions)
            .map(|r| (*r).clone())
            .ok_or_else(|| {
                OrchestrationError::ResourceAllocationError(
                    "No workflow resources available".to_string(),
                )
            })
    }
}

pub struct PriorityBasedStrategy;

impl PriorityBasedStrategy {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PriorityBasedStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AllocationStrategy for PriorityBasedStrategy {
    async fn allocate_agent(
        &self,
        _requirement: &AgentRequirement,
        pool: &ResourcePool<AgentResource>,
    ) -> OrchestrationResult<AgentResource> {
        pool.get_available_resources()
            .first()
            .map(|r| (*r).clone())
            .ok_or_else(|| {
                OrchestrationError::ResourceAllocationError("No agents available".to_string())
            })
    }

    async fn allocate_llm(
        &self,
        _requirement: &LLMRequirement,
        pool: &ResourcePool<LLMResource>,
    ) -> OrchestrationResult<LLMResource> {
        pool.get_available_resources()
            .first()
            .map(|r| (*r).clone())
            .ok_or_else(|| {
                OrchestrationError::ResourceAllocationError(
                    "No LLM instances available".to_string(),
                )
            })
    }

    async fn allocate_task(
        &self,
        requirement: &TaskRequirement,
        pool: &ResourcePool<TaskResource>,
    ) -> OrchestrationResult<TaskResource> {
        pool.get_available_resources()
            .iter()
            .find(|r| {
                let reqs = &requirement.resource_requirements;
                let res_reqs = &r.resource_requirements;
                reqs.cpu_cores.unwrap_or(0) <= res_reqs.cpu_cores.unwrap_or(u32::MAX)
                    && reqs.memory_mb.unwrap_or(0) <= res_reqs.memory_mb.unwrap_or(u64::MAX)
            })
            .map(|r| (*r).clone())
            .ok_or_else(|| {
                OrchestrationError::ResourceAllocationError(
                    "No suitable task resources available for the given requirements".to_string(),
                )
            })
    }

    async fn allocate_workflow(
        &self,
        _requirement: &WorkflowRequirement,
        pool: &ResourcePool<WorkflowResource>,
    ) -> OrchestrationResult<WorkflowResource> {
        pool.get_available_resources()
            .first()
            .map(|r| (*r).clone())
            .ok_or_else(|| {
                OrchestrationError::ResourceAllocationError(
                    "No workflow resources available".to_string(),
                )
            })
    }
}
