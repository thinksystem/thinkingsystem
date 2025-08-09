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

pub mod allocation_strategies;
pub mod resource_pool;
pub mod resource_tracker;
pub mod types;

use crate::orchestration::{OrchestrationError, OrchestrationFlowDefinition, OrchestrationResult};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub use allocation_strategies::*;
pub use resource_pool::*;
pub use resource_tracker::*;
pub use types::*;

pub struct ResourceManager {
    resource_limits: super::coordinator::ResourceLimits,
    allocation_strategies: HashMap<String, Box<dyn AllocationStrategy>>,

    agent_pool: Arc<RwLock<ResourcePool<AgentResource>>>,
    llm_pool: Arc<RwLock<ResourcePool<LLMResource>>>,
    task_pool: Arc<RwLock<ResourcePool<TaskResource>>>,
    workflow_pool: Arc<RwLock<ResourcePool<WorkflowResource>>>,

    active_allocations: Arc<RwLock<HashMap<String, AllocatedResources>>>,

    usage_tracker: Arc<RwLock<ResourceUsageTracker>>,
}

impl ResourceManager {
    pub async fn new(
        resource_limits: super::coordinator::ResourceLimits,
    ) -> OrchestrationResult<Self> {
        let mut allocation_strategies: HashMap<String, Box<dyn AllocationStrategy>> =
            HashMap::new();

        allocation_strategies.insert(
            "round_robin".to_string(),
            Box::new(RoundRobinStrategy::new()),
        );
        allocation_strategies.insert(
            "capability_based".to_string(),
            Box::new(CapabilityBasedStrategy::new()),
        );
        allocation_strategies.insert(
            "load_balanced".to_string(),
            Box::new(LoadBalancedStrategy::new()),
        );
        allocation_strategies.insert(
            "priority_based".to_string(),
            Box::new(PriorityBasedStrategy::new()),
        );

        Ok(Self {
            resource_limits,
            allocation_strategies,
            agent_pool: Arc::new(RwLock::new(ResourcePool::new(ResourceType::Agent))),
            llm_pool: Arc::new(RwLock::new(ResourcePool::new(ResourceType::LLM))),
            task_pool: Arc::new(RwLock::new(ResourcePool::new(ResourceType::Task))),
            workflow_pool: Arc::new(RwLock::new(ResourcePool::new(ResourceType::Workflow))),
            active_allocations: Arc::new(RwLock::new(HashMap::new())),
            usage_tracker: Arc::new(RwLock::new(ResourceUsageTracker::new())),
        })
    }

    pub async fn allocate_for_flow(
        &mut self,
        flow_def: &OrchestrationFlowDefinition,
    ) -> OrchestrationResult<AllocatedResources> {
        let session_id = Uuid::new_v4().to_string();

        let requirements = self.analyse_flow_requirements(flow_def).await?;

        self.validate_resource_requirements(&requirements).await?;

        let agents = self
            .allocate_agents(&requirements.agent_requirements)
            .await?;
        let llm_instances = self
            .allocate_llm_instances(&requirements.llm_requirements)
            .await?;
        let tasks = self
            .allocate_task_resources(&requirements.task_requirements)
            .await?;
        let workflows = self
            .allocate_workflow_resources(&requirements.workflow_requirements)
            .await?;

        let allocated_resources = AllocatedResources {
            session_id: session_id.clone(),
            agents,
            llm_instances,
            tasks,
            workflows,
            allocated_at: chrono::Utc::now(),
        };

        {
            let mut allocations = self.active_allocations.write().await;
            allocations.insert(session_id, allocated_resources.clone());
        }

        {
            let mut usage_tracker = self.usage_tracker.write().await;
            usage_tracker.record_allocation(&allocated_resources);
        }

        Ok(allocated_resources)
    }

    pub async fn release_resources(&mut self, session_id: &str) -> OrchestrationResult<()> {
        let allocated_resources = {
            let mut allocations = self.active_allocations.write().await;
            allocations.remove(session_id).ok_or_else(|| {
                OrchestrationError::ResourceAllocationError(format!(
                    "No resources allocated for session: {session_id}"
                ))
            })?
        };

        self.release_agents(&allocated_resources.agents).await?;
        self.release_llm_instances(&allocated_resources.llm_instances)
            .await?;
        self.release_task_resources(&allocated_resources.tasks)
            .await?;
        self.release_workflow_resources(&allocated_resources.workflows)
            .await?;

        {
            let mut usage_tracker = self.usage_tracker.write().await;
            usage_tracker.record_deallocation(&allocated_resources);
        }

        Ok(())
    }

    pub async fn get_resource_utilisation(&self) -> ResourceUtilisation {
        let usage_tracker = self.usage_tracker.read().await;
        usage_tracker.get_utilisation()
    }

    pub fn register_allocation_strategy(
        &mut self,
        name: String,
        strategy: Box<dyn AllocationStrategy>,
    ) {
        self.allocation_strategies.insert(name, strategy);
    }

    pub async fn add_agent_resource(&self, agent: AgentResource) -> OrchestrationResult<()> {
        let mut agent_pool = self.agent_pool.write().await;
        agent_pool.add_resource(agent)?;
        Ok(())
    }

    pub async fn add_llm_resource(&self, llm: LLMResource) -> OrchestrationResult<()> {
        let mut llm_pool = self.llm_pool.write().await;
        llm_pool.add_resource(llm)?;
        Ok(())
    }

    pub async fn add_task_resource(&self, task: TaskResource) -> OrchestrationResult<()> {
        let mut task_pool = self.task_pool.write().await;
        task_pool.add_resource(task)?;
        Ok(())
    }

    pub async fn add_workflow_resource(
        &self,
        workflow: WorkflowResource,
    ) -> OrchestrationResult<()> {
        let mut workflow_pool = self.workflow_pool.write().await;
        workflow_pool.add_resource(workflow)?;
        Ok(())
    }

    pub async fn debug_pool_state(
        &self,
    ) -> (usize, usize, usize, usize, usize, usize, usize, usize) {
        let agent_pool = self.agent_pool.read().await;
        let llm_pool = self.llm_pool.read().await;
        let task_pool = self.task_pool.read().await;
        let workflow_pool = self.workflow_pool.read().await;

        (
            agent_pool.available_count(),
            agent_pool.allocated_count(),
            llm_pool.available_count(),
            llm_pool.allocated_count(),
            task_pool.available_count(),
            task_pool.allocated_count(),
            workflow_pool.available_count(),
            workflow_pool.allocated_count(),
        )
    }

    async fn release_agents(&self, agents: &[AgentResource]) -> OrchestrationResult<()> {
        let mut agent_pool = self.agent_pool.write().await;
        for agent in agents {
            agent_pool.release_resource(&agent.id)?;
        }
        Ok(())
    }

    async fn release_llm_instances(&self, llms: &[LLMResource]) -> OrchestrationResult<()> {
        let mut llm_pool = self.llm_pool.write().await;
        for llm in llms {
            llm_pool.release_resource(&llm.id)?;
        }
        Ok(())
    }

    async fn release_task_resources(&self, tasks: &[TaskResource]) -> OrchestrationResult<()> {
        let mut task_pool = self.task_pool.write().await;
        for task in tasks {
            task_pool.release_resource(&task.id)?;
        }
        Ok(())
    }

    async fn release_workflow_resources(
        &self,
        workflows: &[WorkflowResource],
    ) -> OrchestrationResult<()> {
        let mut workflow_pool = self.workflow_pool.write().await;
        for workflow in workflows {
            workflow_pool.release_resource(&workflow.id)?;
        }
        Ok(())
    }

    async fn analyse_flow_requirements(
        &self,
        flow_def: &OrchestrationFlowDefinition,
    ) -> OrchestrationResult<FlowResourceRequirements> {
        println!(
            "DEBUG: Analyzing flow requirements for {} blocks",
            flow_def.blocks.len()
        );

        let mut agent_requirements = Vec::new();
        let mut llm_requirements = Vec::new();
        let mut task_requirements = Vec::new();
        let mut workflow_requirements = Vec::new();

        for (i, block) in flow_def.blocks.iter().enumerate() {
            println!(
                "DEBUG: Block {}: id={}, type={:?}",
                i,
                block.id,
                std::mem::discriminant(&block.block_type)
            );

            match &block.block_type {
                super::OrchestrationBlockType::AgentInteraction {
                    agent_selector,
                    task_definition,
                    ..
                } => {
                    println!("DEBUG: Found AgentInteraction block: {}", block.id);
                    agent_requirements.push(AgentRequirement {
                        selector: agent_selector.clone(),
                        task_definition: task_definition.clone(),
                        capabilities: self.extract_required_capabilities(task_definition),
                    });
                }
                super::OrchestrationBlockType::LLMProcessing {
                    llm_config,
                    prompt_template,
                    context_keys,
                    ..
                } => {
                    println!(
                        "DEBUG: Found LLMProcessing block: {} with provider={}, model={}",
                        block.id, llm_config.provider, llm_config.model
                    );
                    llm_requirements.push(LLMRequirement {
                        config: llm_config.clone(),
                        estimated_tokens: self.estimate_token_usage(
                            llm_config,
                            prompt_template,
                            context_keys,
                        ),
                    });
                }
                super::OrchestrationBlockType::TaskExecution {
                    task_config,
                    resource_requirements,
                    ..
                } => {
                    println!("DEBUG: Found TaskExecution block: {}", block.id);
                    task_requirements.push(TaskRequirement {
                        config: task_config.clone(),
                        resource_requirements: resource_requirements.clone(),
                    });
                }
                super::OrchestrationBlockType::WorkflowInvocation { workflow_id, .. } => {
                    println!("DEBUG: Found WorkflowInvocation block: {}", block.id);
                    workflow_requirements.push(WorkflowRequirement {
                        workflow_id: workflow_id.clone(),
                        estimated_complexity: self.estimate_workflow_complexity(workflow_id).await,
                    });
                }
                _ => {
                    println!("DEBUG: Found other block type: {}", block.id);
                }
            }
        }

        println!("DEBUG: Analysis complete - Agent reqs: {}, LLM reqs: {}, Task reqs: {}, Workflow reqs: {}",
                 agent_requirements.len(), llm_requirements.len(), task_requirements.len(), workflow_requirements.len());

        Ok(FlowResourceRequirements {
            agent_requirements,
            llm_requirements,
            task_requirements,
            workflow_requirements,
        })
    }

    async fn validate_resource_requirements(
        &self,
        requirements: &FlowResourceRequirements,
    ) -> OrchestrationResult<()> {
        if requirements.agent_requirements.len()
            > self.resource_limits.max_agents_per_session as usize
        {
            return Err(OrchestrationError::ResourceAllocationError(format!(
                "Agent requirement exceeds limit: {} > {}",
                requirements.agent_requirements.len(),
                self.resource_limits.max_agents_per_session
            )));
        }

        if requirements.llm_requirements.len()
            > self.resource_limits.max_llm_instances_per_session as usize
        {
            return Err(OrchestrationError::ResourceAllocationError(format!(
                "LLM requirement exceeds limit: {} > {}",
                requirements.llm_requirements.len(),
                self.resource_limits.max_llm_instances_per_session
            )));
        }

        if requirements.task_requirements.len()
            > self.resource_limits.max_tasks_per_session as usize
        {
            return Err(OrchestrationError::ResourceAllocationError(format!(
                "Task requirement exceeds limit: {} > {}",
                requirements.task_requirements.len(),
                self.resource_limits.max_tasks_per_session
            )));
        }

        Ok(())
    }

    async fn allocate_agents(
        &self,
        requirements: &[AgentRequirement],
    ) -> OrchestrationResult<Vec<AgentResource>> {
        let mut allocated_agents = Vec::new();
        let mut agent_pool = self.agent_pool.write().await;

        for requirement in requirements {
            let strategy = self
                .allocation_strategies
                .get("capability_based")
                .ok_or_else(|| {
                    OrchestrationError::ConfigurationError(
                        "Capability-based strategy not found".to_string(),
                    )
                })?;

            let agent = strategy.allocate_agent(requirement, &agent_pool).await?;
            agent_pool.allocate_resource(&agent.id)?;
            allocated_agents.push(agent);
        }

        Ok(allocated_agents)
    }

    async fn allocate_llm_instances(
        &self,
        requirements: &[LLMRequirement],
    ) -> OrchestrationResult<Vec<LLMResource>> {
        println!(
            "DEBUG: allocate_llm_instances called with {} requirements",
            requirements.len()
        );

        if requirements.is_empty() {
            return Ok(Vec::new());
        }

        let mut allocated_llms = Vec::new();
        let llm_pool = self.llm_pool.read().await;

        println!(
            "DEBUG: LLM pool has {} available resources",
            llm_pool.available_count()
        );

        let strategy = self
            .allocation_strategies
            .get("load_balanced")
            .ok_or_else(|| {
                OrchestrationError::ConfigurationError(
                    "Load-balanced strategy not found".to_string(),
                )
            })?;

        let llm = strategy.allocate_llm(&requirements[0], &llm_pool).await?;

        println!(
            "DEBUG: Selected LLM instance: {} (provider={}, model={})",
            llm.id, llm.provider, llm.model
        );

        for i in 0..requirements.len() {
            allocated_llms.push(llm.clone());
            println!("DEBUG: Assigned LLM {} to requirement {}", llm.id, i);
        }

        println!(
            "DEBUG: Successfully allocated {} LLM instances (reusing same instance)",
            allocated_llms.len()
        );
        Ok(allocated_llms)
    }

    async fn allocate_task_resources(
        &self,
        requirements: &[TaskRequirement],
    ) -> OrchestrationResult<Vec<TaskResource>> {
        let mut allocated_tasks = Vec::new();
        let mut task_pool = self.task_pool.write().await;

        for requirement in requirements {
            let strategy = self
                .allocation_strategies
                .get("priority_based")
                .ok_or_else(|| {
                    OrchestrationError::ConfigurationError(
                        "Priority-based strategy not found".to_string(),
                    )
                })?;

            let task = strategy.allocate_task(requirement, &task_pool).await?;
            task_pool.allocate_resource(&task.id)?;
            allocated_tasks.push(task);
        }

        Ok(allocated_tasks)
    }

    async fn allocate_workflow_resources(
        &self,
        requirements: &[WorkflowRequirement],
    ) -> OrchestrationResult<Vec<WorkflowResource>> {
        let mut allocated_workflows = Vec::new();
        let mut workflow_pool = self.workflow_pool.write().await;

        for requirement in requirements {
            let strategy = self
                .allocation_strategies
                .get("round_robin")
                .ok_or_else(|| {
                    OrchestrationError::ConfigurationError(
                        "Round-robin strategy not found".to_string(),
                    )
                })?;

            let workflow = strategy
                .allocate_workflow(requirement, &workflow_pool)
                .await?;
            workflow_pool.allocate_resource(&workflow.id)?;
            allocated_workflows.push(workflow);
        }

        Ok(allocated_workflows)
    }

    fn extract_required_capabilities(
        &self,
        task_definition: &super::TaskDefinition,
    ) -> Vec<String> {
        vec![task_definition.task_type.clone()]
    }

    fn estimate_token_usage(
        &self,
        llm_config: &super::LLMProcessingConfig,
        prompt_template: &str,
        context_keys: &[String],
    ) -> u32 {
        let template_tokens = (prompt_template.len() as f64 / 4.0).ceil() as u32;
        let context_tokens = (context_keys.len() * 50) as u32;
        let max_output_tokens = llm_config.max_tokens.unwrap_or(1000);
        template_tokens + context_tokens + max_output_tokens
    }

    async fn estimate_workflow_complexity(&self, workflow_id: &str) -> f64 {
        if workflow_id.contains("simple") {
            1.0
        } else if workflow_id.contains("complex") {
            5.0
        } else {
            2.0
        }
    }
}
