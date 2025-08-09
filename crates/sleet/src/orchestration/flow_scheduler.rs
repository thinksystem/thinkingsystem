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
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub struct FlowScheduler {
    max_concurrent_sessions: u32,
    scheduling_strategies: HashMap<String, Box<dyn SchedulingStrategy>>,
}

impl FlowScheduler {
    pub async fn new(max_concurrent_sessions: u32) -> OrchestrationResult<Self> {
        Ok(Self {
            max_concurrent_sessions,
            scheduling_strategies: HashMap::new(),
        })
    }

    pub fn register_strategy(&mut self, flow_type: String, strategy: Box<dyn SchedulingStrategy>) {
        self.scheduling_strategies.insert(flow_type, strategy);
    }

    pub async fn create_execution_plan(
        &mut self,
        flow_def: &OrchestrationFlowDefinition,
    ) -> OrchestrationResult<ExecutionPlan> {
        let mut execution_order = Vec::new();
        let mut parallel_groups = Vec::new();
        let mut visited = std::collections::HashSet::new();

        Self::build_execution_order(
            &flow_def.start_block_id,
            flow_def,
            &mut execution_order,
            &mut parallel_groups,
            &mut visited,
        );

        if let Some(strategy) = self.scheduling_strategies.get(&flow_def.id) {
            tracing::debug!("Applying scheduling strategy for flow: {}", flow_def.id);

            return strategy.schedule(flow_def);
        }

        let mut resource_requirements = HashMap::new();
        let mut total_agents = 0u32;
        let mut total_llms = 0u32;
        let mut total_tasks = 0u32;

        for block in &flow_def.blocks {
            match &block.block_type {
                super::OrchestrationBlockType::AgentInteraction { .. } => {
                    total_agents += 1;
                }
                super::OrchestrationBlockType::LLMProcessing { .. } => {
                    total_llms += 1;
                }
                super::OrchestrationBlockType::TaskExecution { .. } => {
                    total_tasks += 1;
                }
                super::OrchestrationBlockType::ParallelExecution { branch_blocks, .. } => {
                    let group = ParallelGroup {
                        group_id: format!("parallel_{}", block.id),
                        block_ids: branch_blocks.clone(),
                        merge_strategy: "wait_all".to_string(),
                    };
                    parallel_groups.push(group);
                }
                _ => {}
            }
        }

        resource_requirements.insert("agents".to_string(), total_agents);
        resource_requirements.insert("llms".to_string(), total_llms);
        resource_requirements.insert("tasks".to_string(), total_tasks);

        if total_agents > self.max_concurrent_sessions {
            tracing::warn!(
                "Flow requires {} agents but max concurrent sessions is {}",
                total_agents,
                self.max_concurrent_sessions
            );
        }

        let estimated_duration_secs = (flow_def.blocks.len() as u64 * 30) + 60;

        Ok(ExecutionPlan {
            flow_id: flow_def.id.clone(),
            execution_order,
            parallel_groups,
            estimated_duration_secs,
            resource_requirements,
        })
    }

    fn build_execution_order(
        current_block_id: &str,
        flow_def: &OrchestrationFlowDefinition,
        execution_order: &mut Vec<String>,
        parallel_groups: &mut Vec<ParallelGroup>,
        visited: &mut std::collections::HashSet<String>,
    ) {
        if visited.contains(current_block_id) {
            return;
        }

        visited.insert(current_block_id.to_string());
        execution_order.push(current_block_id.to_string());

        if let Some(block) = flow_def.blocks.iter().find(|b| b.id == current_block_id) {
            match &block.block_type {
                super::OrchestrationBlockType::Conditional {
                    true_block,
                    false_block,
                    ..
                } => {
                    Self::build_execution_order(
                        true_block,
                        flow_def,
                        execution_order,
                        parallel_groups,
                        visited,
                    );
                    Self::build_execution_order(
                        false_block,
                        flow_def,
                        execution_order,
                        parallel_groups,
                        visited,
                    );
                }
                super::OrchestrationBlockType::ParallelExecution {
                    branch_blocks,
                    next_block,
                    ..
                } => {
                    let group = ParallelGroup {
                        group_id: format!("parallel_{}", block.id),
                        block_ids: branch_blocks.clone(),
                        merge_strategy: "wait_all".to_string(),
                    };
                    parallel_groups.push(group);

                    Self::build_execution_order(
                        next_block,
                        flow_def,
                        execution_order,
                        parallel_groups,
                        visited,
                    );
                }

                super::OrchestrationBlockType::Compute { next_block, .. }
                | super::OrchestrationBlockType::AgentInteraction { next_block, .. }
                | super::OrchestrationBlockType::LLMProcessing { next_block, .. }
                | super::OrchestrationBlockType::TaskExecution { next_block, .. }
                | super::OrchestrationBlockType::WorkflowInvocation { next_block, .. }
                | super::OrchestrationBlockType::ResourceAllocation { next_block, .. }
                | super::OrchestrationBlockType::EventTrigger { next_block, .. }
                | super::OrchestrationBlockType::StateCheckpoint { next_block, .. } => {
                    Self::build_execution_order(
                        next_block,
                        flow_def,
                        execution_order,
                        parallel_groups,
                        visited,
                    );
                }
                super::OrchestrationBlockType::Terminate => {}
                _ => {}
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub flow_id: String,
    pub execution_order: Vec<String>,
    pub parallel_groups: Vec<ParallelGroup>,
    pub estimated_duration_secs: u64,
    pub resource_requirements: HashMap<String, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelGroup {
    pub group_id: String,
    pub block_ids: Vec<String>,
    pub merge_strategy: String,
}

pub trait SchedulingStrategy: Send + Sync {
    fn schedule(
        &self,
        flow_def: &OrchestrationFlowDefinition,
    ) -> OrchestrationResult<ExecutionPlan>;
}
