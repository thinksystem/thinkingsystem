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

use crate::tasks::schemas::{
    AgentContribution, FinalDeliverable, Task, TaskConfig, TaskExecution, TaskOutput, TaskPriority,
    TaskProposal, TaskStatus,
};
use crate::tasks::{SimpleTaskAnalyser, TaskAnalyser, TaskError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProposal {
    pub agent_id: String,
    pub task_id: String,
    pub proposal_data: Value,
    pub estimated_success: f64,
    pub resource_request: HashMap<String, i32>,
    pub timeline_estimate: u32,
    pub competitive_advantages: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecution {
    pub agent_id: String,
    pub task_id: String,
    pub execution_data: Value,
    pub resources_used: HashMap<String, i32>,
    pub progress_percentage: f64,
    pub current_step: String,
    pub competitive_position: String,
}
pub type TaskResult<T> = Result<T, TaskError>;
#[derive(Debug, Clone)]
pub struct TaskCompletionResult {
    pub success: bool,
    pub result: Option<String>,
    pub warnings: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSystemConfig {
    pub max_concurrent_tasks: usize,
    pub default_timeout_secs: u64,
    pub competitive_mode: bool,
    pub default_consensus_threshold: f64,
    pub resource_limits: HashMap<String, i32>,
}
#[derive(Debug)]
pub struct TaskSystem {
    tasks: HashMap<String, Task>,
    proposals: HashMap<String, Vec<TaskProposal>>,
    executions: HashMap<String, Vec<TaskExecution>>,
    results: HashMap<String, Vec<TaskCompletionResult>>,
    outputs: HashMap<String, TaskOutput>,
    config: TaskSystemConfig,
}
#[derive(Debug, Clone, Default)]
pub struct TaskManager {
    pub active_tasks: HashMap<String, Task>,
    pub proposals: HashMap<String, Vec<AgentProposal>>,
    pub executions: HashMap<String, Vec<AgentExecution>>,
    pub results: HashMap<String, Vec<TaskCompletionResult>>,
}
#[derive(Debug, Clone, Default)]
pub struct CompetitionManager {
    pub active_tasks: HashMap<String, Task>,
    pub proposals: HashMap<String, Vec<AgentProposal>>,
    pub executions: HashMap<String, Vec<AgentExecution>>,
    pub results: HashMap<String, Vec<TaskCompletionResult>>,
}
impl TaskSystem {
    pub fn new(config: TaskSystemConfig) -> Self {
        Self {
            tasks: HashMap::new(),
            proposals: HashMap::new(),
            executions: HashMap::new(),
            results: HashMap::new(),
            outputs: HashMap::new(),
            config,
        }
    }
    pub fn create_task(&mut self, config: TaskConfig) -> TaskResult<Task> {
        if self.tasks.len() >= self.config.max_concurrent_tasks {
            return Err(TaskError::ResourceError(
                "Maximum concurrent tasks reached".to_string(),
            ));
        }
        let task = Task::new(config);
        let task_id = task.id.clone();
        self.tasks.insert(task_id, task.clone());
        Ok(task)
    }
    pub fn get_task(&self, task_id: &str) -> TaskResult<&Task> {
        self.tasks
            .get(task_id)
            .ok_or_else(|| TaskError::TaskNotFound(task_id.to_string()))
    }
    pub fn update_task_status(&mut self, task_id: &str, status: TaskStatus) -> TaskResult<()> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| TaskError::TaskNotFound(task_id.to_string()))?;
        task.update_status(status);
        Ok(())
    }
    pub fn assign_agent(&mut self, task_id: &str, agent_id: String) -> TaskResult<()> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| TaskError::TaskNotFound(task_id.to_string()))?;
        task.assign_agent(agent_id);
        Ok(())
    }
    pub fn submit_proposal(&mut self, proposal: TaskProposal) -> TaskResult<()> {
        let task_id = proposal.task_id.clone();
        if !self.tasks.contains_key(&task_id) {
            return Err(TaskError::TaskNotFound(task_id));
        }
        self.proposals.entry(task_id).or_default().push(proposal);
        Ok(())
    }
    pub fn start_execution(
        &mut self,
        agent_id: String,
        task_id: String,
    ) -> TaskResult<TaskExecution> {
        if !self.tasks.contains_key(&task_id) {
            return Err(TaskError::TaskNotFound(task_id.clone()));
        }
        let execution = TaskExecution::new(agent_id, task_id.clone());
        let _execution_id = execution.id.clone();
        self.executions
            .entry(task_id)
            .or_default()
            .push(execution.clone());
        Ok(execution)
    }
    pub fn update_execution(
        &mut self,
        task_id: &str,
        agent_id: &str,
        progress: f64,
        step: String,
    ) -> TaskResult<()> {
        let executions = self
            .executions
            .get_mut(task_id)
            .ok_or_else(|| TaskError::TaskNotFound(task_id.to_string()))?;
        if let Some(execution) = executions.iter_mut().find(|e| e.agent_id == agent_id) {
            execution.update_progress(progress, step);
            Ok(())
        } else {
            Err(TaskError::ExecutionFailed(format!(
                "No execution found for agent {agent_id} on task {task_id}"
            )))
        }
    }
    pub fn add_contribution(&mut self, contribution: AgentContribution) -> TaskResult<()> {
        let task_id = contribution.task_id.clone();
        if !self.tasks.contains_key(&task_id) {
            return Err(TaskError::TaskNotFound(task_id.clone()));
        }
        let output = self
            .outputs
            .entry(task_id.clone())
            .or_insert_with(|| TaskOutput {
                task_id: task_id.clone(),
                description: String::new(),
                details: HashMap::new(),
                consensus_level: 0.0,
                contributors: Vec::new(),
                final_deliverable: None,
                quality_score: 0.0,
                completion_status: TaskStatus::InProgress,
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                finalised_at: None,
            });
        if let Some(existing) = output
            .contributors
            .iter_mut()
            .find(|c| c.agent_id == contribution.agent_id)
        {
            *existing = contribution;
        } else {
            output.contributors.push(contribution);
        }
        self.calculate_consensus(&task_id)?;
        Ok(())
    }
    pub fn calculate_consensus(&mut self, task_id: &str) -> TaskResult<f64> {
        let output = self
            .outputs
            .get_mut(task_id)
            .ok_or_else(|| TaskError::TaskNotFound(task_id.to_string()))?;
        if output.contributors.is_empty() {
            output.consensus_level = 0.0;
            return Ok(0.0);
        }
        let agreeing_count = output
            .contributors
            .iter()
            .filter(|c| c.agrees_with_output)
            .count();
        let consensus = agreeing_count as f64 / output.contributors.len() as f64;
        output.consensus_level = consensus;
        Ok(consensus)
    }
    pub fn complete_task(
        &mut self,
        task_id: &str,
        deliverable: FinalDeliverable,
    ) -> TaskResult<()> {
        self.update_task_status(task_id, TaskStatus::Completed)?;

        let mut metadata = HashMap::new();
        metadata.insert("task_id".to_string(), serde_json::json!(task_id));
        metadata.insert("agent_id".to_string(), serde_json::json!("system"));
        metadata.insert(
            "completion_time".to_string(),
            serde_json::json!(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()),
        );
        metadata.insert("quality_score".to_string(), serde_json::json!(1.0));

        let completion_result = TaskCompletionResult {
            success: true,
            result: Some(
                serde_json::to_string(&deliverable)
                    .unwrap_or_else(|_| "Failed to serialise deliverable".to_string()),
            ),
            warnings: Vec::new(),
            metadata,
        };
        self.results
            .entry(task_id.to_string())
            .or_default()
            .push(completion_result);

        if let Some(output) = self.outputs.get_mut(task_id) {
            output.final_deliverable = Some(deliverable);
            output.completion_status = TaskStatus::Completed;
            output.finalised_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            );
        }
        Ok(())
    }

    pub fn get_task_results(&self, task_id: &str) -> Option<&Vec<TaskCompletionResult>> {
        self.results.get(task_id)
    }

    pub fn get_all_results(&self) -> &HashMap<String, Vec<TaskCompletionResult>> {
        &self.results
    }

    pub fn get_statistics(&self) -> TaskSystemStatistics {
        let total_tasks = self.tasks.len();
        let completed_tasks = self
            .tasks
            .values()
            .filter(|t| t.status == TaskStatus::Completed)
            .count();
        let active_tasks = self
            .tasks
            .values()
            .filter(|t| matches!(t.status, TaskStatus::InProgress | TaskStatus::Planning))
            .count();
        TaskSystemStatistics {
            total_tasks,
            active_tasks,
            completed_tasks,
            total_proposals: self.proposals.values().map(|v| v.len()).sum(),
            total_executions: self.executions.values().map(|v| v.len()).sum(),
        }
    }
    pub fn list_tasks_by_status(&self, status: TaskStatus) -> Vec<&Task> {
        self.tasks.values().filter(|t| t.status == status).collect()
    }
    pub fn get_task_output(&self, task_id: &str) -> TaskResult<&TaskOutput> {
        self.outputs
            .get(task_id)
            .ok_or_else(|| TaskError::TaskNotFound(task_id.to_string()))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSystemStatistics {
    pub total_tasks: usize,
    pub active_tasks: usize,
    pub completed_tasks: usize,
    pub total_proposals: usize,
    pub total_executions: usize,
}
impl TaskManager {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add_task(&mut self, task: Task) {
        self.active_tasks.insert(task.id.clone(), task);
    }
    pub fn submit_proposal(&mut self, proposal: AgentProposal) {
        self.proposals
            .entry(proposal.task_id.clone())
            .or_default()
            .push(proposal);
    }
    pub fn update_execution(&mut self, execution: AgentExecution) {
        let executions = self
            .executions
            .entry(execution.task_id.clone())
            .or_default();
        if let Some(existing) = executions
            .iter_mut()
            .find(|e| e.agent_id == execution.agent_id)
        {
            *existing = execution;
        } else {
            executions.push(execution);
        }
    }
}
impl CompetitionManager {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add_task(&mut self, task: Task) {
        self.active_tasks.insert(task.id.clone(), task);
    }
    pub fn submit_proposal(&mut self, proposal: AgentProposal) {
        self.proposals
            .entry(proposal.task_id.clone())
            .or_default()
            .push(proposal);
    }
    pub fn update_execution(&mut self, execution: AgentExecution) {
        let executions = self
            .executions
            .entry(execution.task_id.clone())
            .or_default();
        if let Some(existing) = executions
            .iter_mut()
            .find(|e| e.agent_id == execution.agent_id)
        {
            *existing = execution;
        } else {
            executions.push(execution);
        }
    }
    pub fn get_competition_status(&self, task_id: &str) -> Vec<String> {
        self.executions
            .get(task_id)
            .map(|executions| {
                executions
                    .iter()
                    .map(|e| {
                        format!(
                            "{}: {:.1}% - {}",
                            e.agent_id, e.progress_percentage, e.current_step
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn evaluate_winner(&self, task_id: &str) -> Option<String> {
        self.executions
            .get(task_id)?
            .iter()
            .max_by(|a, b| {
                a.progress_percentage
                    .partial_cmp(&b.progress_percentage)
                    .unwrap()
            })
            .map(|execution| execution.agent_id.clone())
    }
    pub fn get_leaderboard(&self, task_id: &str) -> Vec<(String, f64)> {
        let mut leaderboard: Vec<(String, f64)> = self
            .executions
            .get(task_id)
            .map(|executions| {
                executions
                    .iter()
                    .map(|e| (e.agent_id.clone(), e.progress_percentage))
                    .collect()
            })
            .unwrap_or_default();
        leaderboard.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        leaderboard
    }
}

pub fn create_task_from_input(
    user_input: &str,
    analyser: &dyn TaskAnalyser,
) -> Result<Task, TaskError> {
    let analysis = analyser.analyse(user_input)?;

    let resource_requirements = analysis
        .resource_needs
        .iter()
        .map(
            |(resource_type, amount)| crate::tasks::schemas::ResourceRequirement {
                resource_type: resource_type.clone(),
                amount: *amount as i32,
                unit: "units".to_string(),
                required: *amount > 10.0,
            },
        )
        .collect();

    let config = TaskConfig {
        title: user_input
            .split_whitespace()
            .take(8)
            .collect::<Vec<_>>()
            .join(" "),
        description: user_input.to_string(),

        task_type: analysis.primary_task_type(),
        domain: analysis.primary_task_type().or_else(|| {
            analysis
                .tags
                .iter()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(tag, _)| tag.clone())
        }),

        priority: if analysis.complexity_score > 0.7 {
            TaskPriority::High
        } else if analysis.complexity_score > 0.4 {
            TaskPriority::Medium
        } else {
            TaskPriority::Low
        },
        max_duration_secs: Some(analysis.estimated_duration_secs),
        collaboration_required: analysis.requires_collaboration(0.6),
        success_criteria: vec![
            "Complete task with high quality.".to_string(),
            "Adhere to resource estimates.".to_string(),
            format!(
                "Achieve confidence level above {:.1}%",
                analysis.confidence * 100.0
            ),
        ],
        resource_requirements,
        metadata: HashMap::from([
            (
                "analysis".to_string(),
                serde_json::to_value(&analysis).unwrap(),
            ),
            (
                "analyser_type".to_string(),
                Value::String(analyser.analyser_type().to_string()),
            ),
            (
                "tags".to_string(),
                serde_json::to_value(&analysis.tags).unwrap(),
            ),
        ]),
    };
    Ok(Task::new(config))
}

pub fn create_task_from_input_simple(user_input: &str) -> Result<Task, TaskError> {
    let analyser = SimpleTaskAnalyser::new();
    create_task_from_input(user_input, &analyser)
}
pub fn detect_collaboration_context(input: &str) -> bool {
    let collaboration_keywords = [
        "team",
        "collaborate",
        "cooperation",
        "together",
        "group",
        "collective",
    ];
    let input_lower = input.to_lowercase();
    collaboration_keywords
        .iter()
        .any(|&keyword| input_lower.contains(keyword))
}
impl Default for TaskSystemConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 100,
            default_timeout_secs: 3600,
            competitive_mode: false,
            default_consensus_threshold: 0.7,
            resource_limits: HashMap::from([
                ("budget".to_string(), 10000),
                ("time_units".to_string(), 1000),
                ("compute_power".to_string(), 2000),
                ("data_access".to_string(), 1500),
            ]),
        }
    }
}
