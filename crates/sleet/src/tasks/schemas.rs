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
use serde_json::Value;
use std::collections::HashMap;
use std::time::SystemTime;
use uuid::Uuid;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum TaskPriority {
    Low,
    #[default]
    Medium,
    High,
    Critical,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Created,
    Planning,
    InProgress,
    Consensus,
    Completed,
    Failed,
    Cancelled,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirement {
    pub resource_type: String,
    pub amount: i32,
    pub unit: String,
    pub required: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub task_type: String,
    pub domain: String,
    pub priority: TaskPriority,
    pub status: TaskStatus,
    pub success_criteria: Vec<String>,
    pub resource_requirements: Vec<ResourceRequirement>,
    pub max_duration_secs: u64,
    pub difficulty_level: u8,
    pub collaboration_required: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub assigned_agents: Vec<String>,
    pub metadata: HashMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    pub title: String,
    pub description: String,
    pub task_type: Option<String>,
    pub domain: Option<String>,
    pub priority: TaskPriority,
    pub max_duration_secs: Option<u64>,
    pub collaboration_required: bool,
    pub success_criteria: Vec<String>,
    pub resource_requirements: Vec<ResourceRequirement>,
    pub metadata: HashMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProposal {
    pub id: String,
    pub agent_id: String,
    pub task_id: String,
    pub reasoning: String,
    pub strategy: String,
    pub resource_allocation: HashMap<String, i32>,
    pub timeline: Vec<String>,
    pub success_probability: f64,
    pub competitive_advantages: Vec<String>,
    pub risk_assessment: String,
    pub collaboration_strategy: String,
    pub estimated_completion_time: u64,
    pub confidence_score: f64,
    pub created_at: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecution {
    pub id: String,
    pub agent_id: String,
    pub task_id: String,
    pub execution_step: String,
    pub resource_usage: HashMap<String, i32>,
    pub progress_assessment: String,
    pub competitive_position: String,
    pub next_actions: Vec<String>,
    pub adaptation_strategy: String,
    pub success_indicators: Vec<String>,
    pub completion_percentage: f64,
    pub quality_metrics: HashMap<String, f64>,
    pub started_at: u64,
    pub last_updated: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalDeliverable {
    pub id: String,
    pub task_id: String,
    pub content: String,
    pub quality_score: f64,
    pub submitted_by: String,
    pub submitted_at: String,
    pub metadata: HashMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompetitiveTask {
    pub id: String,
    pub config: TaskConfig,
    pub competitors: Vec<String>,
    pub proposals: Vec<TaskProposal>,
    pub executions: Vec<TaskExecution>,
    pub final_deliverable: Option<FinalDeliverable>,
    pub status: TaskStatus,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}
impl CompetitiveTask {
    pub fn new(config: TaskConfig) -> Self {
        let now = SystemTime::now();
        Self {
            id: Uuid::new_v4().to_string(),
            config,
            competitors: Vec::new(),
            proposals: Vec::new(),
            executions: Vec::new(),
            final_deliverable: None,
            status: TaskStatus::Created,
            created_at: now,
            updated_at: now,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputFormat {
    TextDescription,
    StructuredData {
        fields: Vec<String>,
    },
    Presentation {
        slides: Vec<String>,
    },
    Code {
        language: String,
        files: Vec<String>,
    },
    Report {
        sections: Vec<String>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskOutputSchema {
    pub task_type: String,
    pub required_fields: Vec<String>,
    pub output_format: OutputFormat,
    pub consensus_threshold: f64,
    pub quality_requirements: HashMap<String, f64>,
    pub validation_criteria: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContribution {
    pub id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub task_id: String,
    pub contribution: String,
    pub contribution_type: String,
    pub agrees_with_output: bool,
    pub confidence_level: f64,
    pub quality_rating: f64,
    pub created_at: u64,
    pub metadata: HashMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskOutput {
    pub task_id: String,
    pub description: String,
    pub details: HashMap<String, String>,
    pub consensus_level: f64,
    pub contributors: Vec<AgentContribution>,
    pub final_deliverable: Option<FinalDeliverable>,
    pub quality_score: f64,
    pub completion_status: TaskStatus,
    pub created_at: u64,
    pub finalised_at: Option<u64>,
}
impl Task {
    pub fn new(config: TaskConfig) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Self {
            id: Uuid::new_v4().to_string(),
            title: config.title,
            description: config.description,
            task_type: config.task_type.unwrap_or_else(|| "general".to_string()),
            domain: config.domain.unwrap_or_else(|| "general".to_string()),
            priority: config.priority,
            status: TaskStatus::Created,
            success_criteria: config.success_criteria,
            resource_requirements: config.resource_requirements,
            max_duration_secs: config.max_duration_secs.unwrap_or(3600),
            difficulty_level: 5,
            collaboration_required: config.collaboration_required,
            created_at: now,
            updated_at: now,
            assigned_agents: Vec::new(),
            metadata: config.metadata,
        }
    }
    pub fn update_status(&mut self, status: TaskStatus) {
        self.status = status;
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
    pub fn assign_agent(&mut self, agent_id: String) {
        if !self.assigned_agents.contains(&agent_id) {
            self.assigned_agents.push(agent_id);
            self.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
        }
    }
    pub fn is_collaborative(&self) -> bool {
        self.collaboration_required || self.assigned_agents.len() > 1
    }
    pub fn complexity_score(&self) -> f64 {
        let base_score = self.difficulty_level as f64 / 10.0;
        let resource_complexity = self.resource_requirements.len() as f64 * 0.1;
        let collaboration_complexity = if self.is_collaborative() { 0.2 } else { 0.0 };
        (base_score + resource_complexity + collaboration_complexity).min(1.0)
    }
}

impl std::fmt::Display for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Task({}): {} [{}] - {}",
            self.id, self.title, self.task_type, self.status
        )
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Created => write!(f, "Created"),
            TaskStatus::Planning => write!(f, "Planning"),
            TaskStatus::InProgress => write!(f, "In Progress"),
            TaskStatus::Consensus => write!(f, "Consensus"),
            TaskStatus::Completed => write!(f, "Completed"),
            TaskStatus::Failed => write!(f, "Failed"),
            TaskStatus::Cancelled => write!(f, "Cancelled"),
        }
    }
}
impl TaskProposal {
    pub fn new(agent_id: String, task_id: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            agent_id,
            task_id,
            reasoning: String::new(),
            strategy: String::new(),
            resource_allocation: HashMap::new(),
            timeline: Vec::new(),
            success_probability: 0.0,
            competitive_advantages: Vec::new(),
            risk_assessment: String::new(),
            collaboration_strategy: String::new(),
            estimated_completion_time: 0,
            confidence_score: 0.0,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}
impl TaskExecution {
    pub fn new(agent_id: String, task_id: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Self {
            id: Uuid::new_v4().to_string(),
            agent_id,
            task_id,
            execution_step: "initialised".to_string(),
            resource_usage: HashMap::new(),
            progress_assessment: String::new(),
            competitive_position: String::new(),
            next_actions: Vec::new(),
            adaptation_strategy: String::new(),
            success_indicators: Vec::new(),
            completion_percentage: 0.0,
            quality_metrics: HashMap::new(),
            started_at: now,
            last_updated: now,
        }
    }
    pub fn update_progress(&mut self, percentage: f64, step: String) {
        self.completion_percentage = percentage.clamp(0.0, 100.0);
        self.execution_step = step;
        self.last_updated = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
}
impl AgentContribution {
    pub fn new(
        agent_id: String,
        agent_name: String,
        task_id: String,
        contribution: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            agent_id,
            agent_name,
            task_id,
            contribution,
            contribution_type: "general".to_string(),
            agrees_with_output: false,
            confidence_level: 0.0,
            quality_rating: 0.0,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            metadata: HashMap::new(),
        }
    }
}
pub fn create_task_output_schema(task_description: &str) -> TaskOutputSchema {
    let task_type = if task_description.to_lowercase().contains("design") {
        "design".to_string()
    } else if task_description.to_lowercase().contains("plan") {
        "planning".to_string()
    } else if task_description.to_lowercase().contains("code")
        || task_description.to_lowercase().contains("develop")
    {
        "development".to_string()
    } else if task_description.to_lowercase().contains("analysis")
        || task_description.to_lowercase().contains("analyse")
    {
        "analysis".to_string()
    } else {
        "general".to_string()
    };
    let (required_fields, output_format) = match task_type.as_str() {
        "design" => (
            vec![
                "name".to_string(),
                "description".to_string(),
                "key_features".to_string(),
                "target_audience".to_string(),
                "materials".to_string(),
            ],
            OutputFormat::StructuredData {
                fields: vec!["design_document".to_string(), "specifications".to_string()],
            },
        ),
        "development" => (
            vec!["solution".to_string(), "implementation".to_string()],
            OutputFormat::Code {
                language: "auto-detect".to_string(),
                files: vec!["main".to_string()],
            },
        ),
        "analysis" => (
            vec!["findings".to_string(), "recommendations".to_string()],
            OutputFormat::Report {
                sections: vec![
                    "executive_summary".to_string(),
                    "detailed_analysis".to_string(),
                ],
            },
        ),
        _ => (
            vec!["description".to_string()],
            OutputFormat::TextDescription,
        ),
    };
    TaskOutputSchema {
        task_type,
        required_fields,
        output_format,
        consensus_threshold: 0.7,
        quality_requirements: HashMap::from([
            ("clarity".to_string(), 0.8),
            ("completeness".to_string(), 0.7),
            ("accuracy".to_string(), 0.9),
        ]),
        validation_criteria: vec![
            "Meets success criteria".to_string(),
            "Resource usage within limits".to_string(),
            "Quality standards achieved".to_string(),
        ],
    }
}
impl Default for TaskConfig {
    fn default() -> Self {
        Self {
            title: "Untitled Task".to_string(),
            description: String::new(),
            task_type: None,
            domain: None,
            priority: TaskPriority::Medium,
            max_duration_secs: Some(3600),
            collaboration_required: false,
            success_criteria: vec!["Complete task successfully".to_string()],
            resource_requirements: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}
