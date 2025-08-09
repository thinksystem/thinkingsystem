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

use crate::runtime::{FfiFunction, Value as RuntimeValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;


#[allow(dead_code)]
fn json_to_runtime_value(json_value: &serde_json::Value) -> RuntimeValue {
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


#[allow(dead_code)]
fn runtime_to_json_value(runtime_value: &RuntimeValue) -> serde_json::Value {
    match runtime_value {
        RuntimeValue::Integer(i) => serde_json::Value::Number(serde_json::Number::from(*i)),
        RuntimeValue::Boolean(b) => serde_json::Value::Bool(*b),
        RuntimeValue::String(s) => serde_json::Value::String(s.clone()),
        RuntimeValue::Null => serde_json::Value::Null,
        RuntimeValue::Json(j) => j.clone(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub role: String,
    pub specialisation: String,
    pub capabilities: AgentCapabilities,
    pub status: AgentStatus,
    pub metadata: AgentMetadata,
    pub custom_properties: HashMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentCapabilities {
    pub personality_traits: Vec<String>,
    pub strengths: Vec<String>,
    pub approach_style: String,
    pub competitive_edge: String,
    pub risk_tolerance: f64,
    pub collaboration_preference: String,
    pub technical_skills: Vec<TechnicalSkill>,
    pub performance_metrics: PerformanceMetrics,

    pub runtime_capabilities: RuntimeCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeCapabilities {
    pub can_create_subtasks: bool,
    pub can_modify_workflow: bool,
    pub can_access_external_apis: bool,
    pub can_delegate_tasks: bool,
    pub max_concurrent_executions: usize,
    pub trusted_execution_level: TrustLevel,
    pub ffi_permissions: Vec<String>,
    pub gas_limit: u64,
    pub execution_timeout_secs: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TrustLevel {
    Restricted,
    Standard,
    Elevated,
    Privileged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TechnicalSkill {
    pub name: String,
    pub proficiency: f64,
    pub experience_years: f64,
    pub domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerformanceMetrics {
    pub success_rate: f64,
    pub avg_completion_time: f64,
    pub quality_score: f64,
    pub collaboration_score: f64,
    pub tasks_completed: u64,
    pub innovation_score: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentStatus {
    Available,
    Busy { task_id: String, started_at: u64 },
    Offline { reason: String },
    Error { error: String, recoverable: bool },
    Maintenance,
    Deactivated,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentMetadata {
    pub created_at: u64,
    pub updated_at: u64,
    pub version: u32,
    pub created_by: String,
    pub generation_method: GenerationMethod,
    pub tags: Vec<String>,
    pub performance_tracking: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GenerationMethod {
    LLMGenerated {
        model: String,
        prompt_version: String,
    },
    Manual {
        creator: String,
    },
    Cloned {
        source_agent_id: String,
    },
    Template {
        template_id: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub generation_method: GenerationMethod,
    pub required_capabilities: Vec<String>,
    pub min_performance: PerformanceThresholds,
    pub collaboration_requirements: CollaborationRequirements,
    pub constraints: HashMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceThresholds {
    pub min_success_rate: f64,
    pub max_completion_time: f64,
    pub min_quality_score: f64,
    pub min_collaboration_score: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationRequirements {
    pub team_size_range: (usize, usize),
    pub preferred_style: CollaborationStyle,
    pub complementary_skills: Vec<String>,
    pub conflict_resolution: ConflictResolutionStyle,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CollaborationStyle {
    Consensus,
    Competitive,
    Independent,
    Hierarchical,
    Specialised,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConflictResolutionStyle {
    Compromise,
    Evidence,
    Democratic,
    Authoritative,
    Avoidance,
}
impl Agent {
    pub fn new(
        name: impl Into<String>,
        role: impl Into<String>,
        specialisation: impl Into<String>,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            role: role.into(),
            specialisation: specialisation.into(),
            capabilities: AgentCapabilities::default(),
            status: AgentStatus::Available,
            metadata: AgentMetadata {
                created_at: now,
                updated_at: now,
                version: 1,
                created_by: "system".to_string(),
                generation_method: GenerationMethod::Manual {
                    creator: "system".to_string(),
                },
                tags: Vec::new(),
                performance_tracking: true,
            },
            custom_properties: HashMap::new(),
        }
    }
    pub fn get_system_prompt(&self) -> String {
        format!(
            "You are {}, a {} specialising in {}.
Your personality traits: {}
Your key strengths: {}
Your approach style: {}
Your competitive edge: {}
Risk tolerance: {:.1}/10
Collaboration preference: {}
Runtime capabilities: can_create_subtasks={}, can_modify_workflow={}, trust_level={:?}
Execution constraints: gas_limit={}, timeout_secs={}, ffi_permissions={:?}
You are participating in a workflow execution environment where you must utilise your expertise to contribute effectively to task completion.
You have access to runtime tools and can execute dynamic workflows based on your trust level and permissions.
You think systematically, apply your specialised knowledge, and collaborate appropriately based on your preferences and the task requirements.
Always respond with valid JSON when structured output is requested, and demonstrate your expertise through detailed, thoughtful responses.",
            self.name,
            self.role,
            self.specialisation,
            self.capabilities.personality_traits.join(", "),
            self.capabilities.strengths.join(", "),
            self.capabilities.approach_style,
            self.capabilities.competitive_edge,
            self.capabilities.risk_tolerance,
            self.capabilities.collaboration_preference,
            self.capabilities.runtime_capabilities.can_create_subtasks,
            self.capabilities.runtime_capabilities.can_modify_workflow,
            self.capabilities.runtime_capabilities.trusted_execution_level,
            self.capabilities.runtime_capabilities.gas_limit,
            self.capabilities.runtime_capabilities.execution_timeout_secs,
            self.capabilities.runtime_capabilities.ffi_permissions
        )
    }

    pub fn create_agent_ffi_registry(&self) -> HashMap<String, FfiFunction> {
        let mut registry: HashMap<String, FfiFunction> = HashMap::new();

        registry.insert(
            "log_progress".to_string(),
            Arc::new(|args, _perms| {
                let message = args[0].as_str().unwrap_or("Progress update");
                tracing::info!("Agent progress: {}", message);
                Ok(RuntimeValue::Boolean(true))
            }),
        );

        for permission in &self.capabilities.runtime_capabilities.ffi_permissions {
            match permission.as_str() {
                "calculate" => {
                    registry.insert(
                        "calculate".to_string(),
                        Arc::new(|args, _perms| {
                            let expression = args[0].as_str().unwrap_or("0");
                            Ok(RuntimeValue::String(format!("calculated: {expression}")))
                        }),
                    );
                }
                "validate_data" => {
                    registry.insert(
                        "validate_data".to_string(),
                        Arc::new(|args, _perms| {
                            let _data = &args[0];
                            Ok(RuntimeValue::Boolean(true))
                        }),
                    );
                }
                "request_collaboration" => {
                    registry.insert(
                        "request_collaboration".to_string(),
                        Arc::new(|args, _perms| {
                            let request = args[0].as_str().unwrap_or("Help needed");
                            Ok(RuntimeValue::String(format!(
                                "collaboration_requested: {request}"
                            )))
                        }),
                    );
                }
                _ => {}
            }
        }

        registry
    }
    pub fn is_available(&self) -> bool {
        matches!(self.status, AgentStatus::Available)
    }
    pub fn get_skill_proficiency(&self, skill_name: &str) -> Option<f64> {
        self.capabilities
            .technical_skills
            .iter()
            .find(|skill| skill.name == skill_name)
            .map(|skill| skill.proficiency)
    }
    pub fn update_performance(&mut self, success: bool, completion_time: f64, quality: f64) {
        let metrics = &mut self.capabilities.performance_metrics;
        let total_tasks = metrics.tasks_completed as f64;
        let current_success_rate = metrics.success_rate * total_tasks;
        let new_success = if success { 1.0 } else { 0.0 };
        metrics.success_rate = (current_success_rate + new_success) / (total_tasks + 1.0);
        metrics.avg_completion_time =
            (metrics.avg_completion_time * total_tasks + completion_time) / (total_tasks + 1.0);
        metrics.quality_score =
            (metrics.quality_score * total_tasks + quality) / (total_tasks + 1.0);
        metrics.tasks_completed += 1;
        self.metadata.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.metadata.version += 1;
    }
}
impl Default for AgentCapabilities {
    fn default() -> Self {
        Self {
            personality_traits: vec!["analytical".into(), "focused".into()],
            strengths: vec!["problem-solving".into(), "attention-to-detail".into()],
            approach_style: "systematic".into(),
            competitive_edge: "thoroughness".into(),
            risk_tolerance: 5.0,
            collaboration_preference: "balanced".into(),
            technical_skills: Vec::new(),
            performance_metrics: PerformanceMetrics::default(),
            runtime_capabilities: RuntimeCapabilities::default(),
        }
    }
}

impl Default for RuntimeCapabilities {
    fn default() -> Self {
        Self {
            can_create_subtasks: false,
            can_modify_workflow: false,
            can_access_external_apis: false,
            can_delegate_tasks: false,
            max_concurrent_executions: 1,
            trusted_execution_level: TrustLevel::Restricted,
            ffi_permissions: vec!["log_progress".to_string()],
            gas_limit: 10000,
            execution_timeout_secs: 300,
        }
    }
}
impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            success_rate: 0.0,
            avg_completion_time: 0.0,
            quality_score: 0.0,
            collaboration_score: 0.0,
            tasks_completed: 0,
            innovation_score: 0.0,
        }
    }
}
impl Default for PerformanceThresholds {
    fn default() -> Self {
        Self {
            min_success_rate: 0.7,
            max_completion_time: 60.0,
            min_quality_score: 0.7,
            min_collaboration_score: 0.6,
        }
    }
}
impl AgentStatus {
    pub fn is_error(&self) -> bool {
        matches!(self, AgentStatus::Error { .. })
    }
    pub fn is_operational(&self) -> bool {
        matches!(self, AgentStatus::Available | AgentStatus::Busy { .. })
    }
    pub fn description(&self) -> String {
        match self {
            AgentStatus::Available => "Available for tasks".to_string(),
            AgentStatus::Busy { task_id, .. } => format!("Executing task: {task_id}"),
            AgentStatus::Offline { reason } => format!("Offline: {reason}"),
            AgentStatus::Error { error, recoverable } => {
                if *recoverable {
                    format!("Recoverable error: {error}")
                } else {
                    format!("Fatal error: {error}")
                }
            }
            AgentStatus::Maintenance => "Under maintenance".to_string(),
            AgentStatus::Deactivated => "Deactivated".to_string(),
        }
    }
}
