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

use super::types::{AllocatedResources, ResourceUtilisation};

pub struct ResourceUsageTracker {
    total_allocations: u64,
    total_deallocations: u64,
    peak_agent_usage: u32,
    peak_llm_usage: u32,
    peak_task_usage: u32,
    peak_workflow_usage: u32,
    current_usage: ResourceUtilisation,
}

impl ResourceUsageTracker {
    pub fn new() -> Self {
        Self {
            total_allocations: 0,
            total_deallocations: 0,
            peak_agent_usage: 0,
            peak_llm_usage: 0,
            peak_task_usage: 0,
            peak_workflow_usage: 0,
            current_usage: ResourceUtilisation::default(),
        }
    }

    pub fn record_allocation(&mut self, resources: &AllocatedResources) {
        self.total_allocations += 1;
        self.current_usage.active_agents += resources.agents.len() as u32;
        self.current_usage.active_llm_instances += resources.llm_instances.len() as u32;
        self.current_usage.active_tasks += resources.tasks.len() as u32;
        self.current_usage.active_workflows += resources.workflows.len() as u32;

        self.current_usage.total_allocations = self.total_allocations;

        self.peak_agent_usage = self.peak_agent_usage.max(self.current_usage.active_agents);
        self.peak_llm_usage = self
            .peak_llm_usage
            .max(self.current_usage.active_llm_instances);
        self.peak_task_usage = self.peak_task_usage.max(self.current_usage.active_tasks);
        self.peak_workflow_usage = self
            .peak_workflow_usage
            .max(self.current_usage.active_workflows);
    }

    pub fn record_deallocation(&mut self, resources: &AllocatedResources) {
        self.total_deallocations += 1;
        self.current_usage.active_agents = self
            .current_usage
            .active_agents
            .saturating_sub(resources.agents.len() as u32);
        self.current_usage.active_llm_instances = self
            .current_usage
            .active_llm_instances
            .saturating_sub(resources.llm_instances.len() as u32);
        self.current_usage.active_tasks = self
            .current_usage
            .active_tasks
            .saturating_sub(resources.tasks.len() as u32);
        self.current_usage.active_workflows = self
            .current_usage
            .active_workflows
            .saturating_sub(resources.workflows.len() as u32);

        self.current_usage.total_deallocations = self.total_deallocations;
    }

    pub fn get_utilisation(&self) -> ResourceUtilisation {
        self.current_usage.clone()
    }

    pub fn get_peak_usage(&self) -> PeakUsageStats {
        PeakUsageStats {
            peak_agents: self.peak_agent_usage,
            peak_llm_instances: self.peak_llm_usage,
            peak_tasks: self.peak_task_usage,
            peak_workflows: self.peak_workflow_usage,
        }
    }

    pub fn reset_statistics(&mut self) {
        self.total_allocations = 0;
        self.total_deallocations = 0;
        self.peak_agent_usage = 0;
        self.peak_llm_usage = 0;
        self.peak_task_usage = 0;
        self.peak_workflow_usage = 0;
        self.current_usage = ResourceUtilisation::default();
    }

    pub fn total_allocations(&self) -> u64 {
        self.total_allocations
    }

    pub fn total_deallocations(&self) -> u64 {
        self.total_deallocations
    }

    pub fn total_active_resources(&self) -> u32 {
        self.current_usage.active_agents
            + self.current_usage.active_llm_instances
            + self.current_usage.active_tasks
            + self.current_usage.active_workflows
    }
}

impl Default for ResourceUsageTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct PeakUsageStats {
    pub peak_agents: u32,
    pub peak_llm_instances: u32,
    pub peak_tasks: u32,
    pub peak_workflows: u32,
}
