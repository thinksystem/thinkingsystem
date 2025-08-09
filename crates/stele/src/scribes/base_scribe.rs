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

use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BaseScribe {
    state: usize,
    q_table: Vec<Vec<f32>>,
    provider_metadata: Vec<ProviderMetadata>,
    goals: Vec<(String, u32)>,
    delegate: Delegate,
}
impl BaseScribe {
    pub fn new(num_states: usize, num_actions: usize) -> Self {
        Self {
            state: 0,
            q_table: vec![vec![0.0; num_actions]; num_states],
            provider_metadata: Vec::new(),
            goals: Vec::new(),
            delegate: Delegate::default(),
        }
    }
    pub fn state(&self) -> usize {
        self.state
    }
    pub fn set_state(&mut self, new_state: usize) {
        if new_state < self.q_table.len() {
            self.state = new_state;
        }
    }
    pub fn q_table(&self) -> &Vec<Vec<f32>> {
        &self.q_table
    }
    pub fn q_table_mut(&mut self) -> &mut Vec<Vec<f32>> {
        &mut self.q_table
    }
    pub fn goals(&self) -> &[(String, u32)] {
        &self.goals
    }
    pub fn delegate(&self) -> &Delegate {
        &self.delegate
    }
    pub fn delegate_mut(&mut self) -> &mut Delegate {
        &mut self.delegate
    }
    pub fn provider_metadata(&self) -> &[ProviderMetadata] {
        &self.provider_metadata
    }
    pub fn provider_metadata_mut(&mut self) -> &mut Vec<ProviderMetadata> {
        &mut self.provider_metadata
    }
    pub fn add_provider_metadata(&mut self, metadata: ProviderMetadata) {
        self.provider_metadata.push(metadata);
    }
    pub fn add_goal(&mut self, goal: String, priority: u32) {
        self.goals.push((goal, priority));
        self.goals.sort_by_key(|&(_, p)| p);
    }
    pub fn get_best_action(&self, state: usize) -> usize {
        if state >= self.q_table.len() || self.q_table[state].is_empty() {
            return 0;
        }
        self.q_table[state]
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(index, _)| index)
            .unwrap_or(0)
    }
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderMetadata {
    pub name: String,
    pub provider_type: Vec<String>,
    pub supported_content_types: Vec<String>,
    pub cost_per_request: CostPerRequest,
    pub copyright_ownership: String,
    pub data_reproduction_rights: String,
    pub data_handling: DataHandling,
    pub performance_metrics: PerformanceMetrics,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CostPerRequest {
    pub amount: f32,
    pub currency: String,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DataHandling {
    pub storage_duration: String,
    pub usage_policy: String,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PerformanceMetrics {
    pub accuracy: f32,
    pub response_time: f32,
    pub speed: String,
}
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Delegate {
    pub tasks: Vec<String>,
    pub status: DelegateStatus,
    pub performance_history: Vec<TaskPerformance>,
}
impl Delegate {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn assign_task(&mut self, task: String) {
        self.tasks.push(task);
        self.status = DelegateStatus::Busy;
    }
    pub fn get_tasks(&self) -> &[String] {
        &self.tasks
    }
    pub fn get_performance_history(&self) -> &[TaskPerformance] {
        &self.performance_history
    }
    pub fn add_performance_record(&mut self, performance: TaskPerformance) {
        self.performance_history.push(performance);
    }
    pub fn complete_task(&mut self, task_id: &str, completion_time: f32, success_rate: f32) {
        self.add_performance_record(TaskPerformance {
            task_id: task_id.to_string(),
            completion_time,
            success_rate,
        });
        self.tasks.retain(|task| task != task_id);
        if self.tasks.is_empty() {
            self.status = DelegateStatus::Available;
        }
    }
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub enum DelegateStatus {
    #[default]
    Available,
    Busy,
    Offline,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskPerformance {
    pub task_id: String,
    pub completion_time: f32,
    pub success_rate: f32,
}
