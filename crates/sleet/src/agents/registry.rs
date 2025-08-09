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

use crate::agents::schemas::AgentStatus;
use crate::agents::{Agent, RegistryConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;
use tokio::fs as async_fs;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StatusCategory {
    Available,
    Busy,
    Offline,
    Error,
    Maintenance,
    Deactivated,
}
#[derive(Error, Debug)]
pub enum RegistryError {
    #[error("Agent with ID '{id}' already exists")]
    AgentAlreadyExists { id: String },
    #[error("Agent with ID '{id}' not found")]
    AgentNotFound { id: String },
    #[error("Registry storage error: {reason}")]
    StorageError { reason: String },
    #[error("Registry capacity exceeded: {current}/{max}")]
    CapacityExceeded { current: usize, max: usize },
    #[error("Invalid query parameters: {reason}")]
    InvalidQuery { reason: String },
    #[error("Serialisation error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
#[derive(Debug)]
pub struct AgentRegistry {
    agents: HashMap<String, Agent>,
    config: RegistryConfig,
    status_index: HashMap<StatusCategory, Vec<String>>,
    skill_index: HashMap<String, Vec<String>>,
    performance_index: PerformanceIndex,
    dirty: bool,
}
#[derive(Debug, Clone)]
struct PerformanceIndex {
    by_success_rate: Vec<(String, f64)>,
    by_quality: Vec<(String, f64)>,
    by_collaboration: Vec<(String, f64)>,
    by_innovation: Vec<(String, f64)>,
    by_completion_time: Vec<(String, f64)>,
    by_task_count: Vec<(String, u64)>,
    needs_rebuild: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentQuery {
    pub status: Option<Vec<AgentStatus>>,
    pub required_skills: Option<Vec<SkillRequirement>>,
    pub min_performance: Option<PerformanceThresholds>,
    pub specialisations: Option<Vec<String>>,
    pub roles: Option<Vec<String>>,
    pub sort_by: Option<SortCriteria>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRequirement {
    pub skill: String,
    pub min_proficiency: f64,
    pub required: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceThresholds {
    pub min_success_rate: Option<f64>,
    pub min_quality_score: Option<f64>,
    pub min_collaboration_score: Option<f64>,
    pub min_innovation_score: Option<f64>,
    pub max_avg_completion_time: Option<f64>,
    pub min_tasks_completed: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SortCriteria {
    SuccessRate,
    Quality,
    Collaboration,
    Innovation,
    CompletionTime,
    TaskCount,
    CreatedAt,
    UpdatedAt,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    pub agents: Vec<Agent>,
    pub total_count: usize,
    pub query: AgentQuery,
    pub execution_time_ms: u64,
}
impl AgentRegistry {
    pub fn new(config: RegistryConfig) -> Result<Self, RegistryError> {
        let mut registry = Self {
            agents: HashMap::new(),
            config,
            status_index: HashMap::new(),
            skill_index: HashMap::new(),
            performance_index: PerformanceIndex::new(),
            dirty: false,
        };
        if registry.config.persistent {
            registry.load_from_storage_sync()?;
        }
        Ok(registry)
    }
    pub fn register_agent(&mut self, agent: Agent) -> Result<(), RegistryError> {
        if self.agents.contains_key(&agent.id) {
            return Err(RegistryError::AgentAlreadyExists { id: agent.id });
        }
        if self.agents.len() >= self.config.max_in_memory {
            return Err(RegistryError::CapacityExceeded {
                current: self.agents.len(),
                max: self.config.max_in_memory,
            });
        }
        self.update_indices_for_agent(&agent, true);
        self.agents.insert(agent.id.clone(), agent);
        self.dirty = true;
        if self.config.persistent {
            self.save_to_storage_sync()?;
        }
        Ok(())
    }
    pub fn get_agent(&self, agent_id: &str) -> Result<Agent, RegistryError> {
        self.agents
            .get(agent_id)
            .cloned()
            .ok_or(RegistryError::AgentNotFound {
                id: agent_id.to_string(),
            })
    }
    pub fn update_agent(&mut self, agent: Agent) -> Result<(), RegistryError> {
        let agent_id = &agent.id;
        if !self.agents.contains_key(agent_id) {
            return Err(RegistryError::AgentNotFound {
                id: agent_id.clone(),
            });
        }
        if let Some(old_agent) = self.agents.get(agent_id).cloned() {
            self.update_indices_for_agent(&old_agent, false);
        }
        self.update_indices_for_agent(&agent, true);
        self.agents.insert(agent_id.clone(), agent);
        self.dirty = true;
        if self.config.persistent {
            self.save_to_storage_sync()?;
        }
        Ok(())
    }
    pub fn update_agent_status(
        &mut self,
        agent_id: &str,
        status: AgentStatus,
    ) -> Result<(), RegistryError> {
        let mut agent = self.get_agent(agent_id)?;
        self.remove_from_status_index(&agent.status, agent_id);
        agent.status = status.clone();
        agent.metadata.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.add_to_status_index(&status, agent_id);
        self.agents.insert(agent_id.to_string(), agent);
        self.dirty = true;
        Ok(())
    }
    pub fn remove_agent(&mut self, agent_id: &str) -> Result<(), RegistryError> {
        let agent = self.get_agent(agent_id)?;
        self.update_indices_for_agent(&agent, false);
        self.agents.remove(agent_id);
        self.dirty = true;
        if self.config.persistent {
            self.save_to_storage_sync()?;
        }
        Ok(())
    }
    pub fn search_agents(&self, query: AgentQuery) -> Result<SearchResults, RegistryError> {
        let start_time = std::time::Instant::now();
        let mut candidates: Vec<&Agent> = self.agents.values().collect();
        if let Some(statuses) = &query.status {
            candidates.retain(|agent| {
                statuses.iter().any(|status| {
                    StatusCategory::from_agent_status(&agent.status)
                        == StatusCategory::from_agent_status(status)
                })
            });
        }
        if let Some(skill_reqs) = &query.required_skills {
            candidates.retain(|agent| {
                skill_reqs.iter().all(|req| {
                    if req.required {
                        agent.capabilities.technical_skills.iter().any(|skill| {
                            skill.name == req.skill && skill.proficiency >= req.min_proficiency
                        })
                    } else {
                        true
                    }
                })
            });
        }
        if let Some(perf_thresholds) = &query.min_performance {
            candidates.retain(|agent| {
                let metrics = &agent.capabilities.performance_metrics;
                if let Some(min_success) = perf_thresholds.min_success_rate {
                    if metrics.success_rate < min_success {
                        return false;
                    }
                }
                if let Some(min_quality) = perf_thresholds.min_quality_score {
                    if metrics.quality_score < min_quality {
                        return false;
                    }
                }
                if let Some(min_collab) = perf_thresholds.min_collaboration_score {
                    if metrics.collaboration_score < min_collab {
                        return false;
                    }
                }
                if let Some(min_innovation) = perf_thresholds.min_innovation_score {
                    if metrics.innovation_score < min_innovation {
                        return false;
                    }
                }
                if let Some(max_time) = perf_thresholds.max_avg_completion_time {
                    if metrics.avg_completion_time > max_time {
                        return false;
                    }
                }
                if let Some(min_tasks) = perf_thresholds.min_tasks_completed {
                    if metrics.tasks_completed < min_tasks {
                        return false;
                    }
                }
                true
            });
        }
        if let Some(specialisations) = &query.specialisations {
            candidates.retain(|agent| {
                specialisations
                    .iter()
                    .any(|spec| agent.specialisation.contains(spec))
            });
        }
        if let Some(roles) = &query.roles {
            candidates.retain(|agent| roles.contains(&agent.role));
        }
        let total_count = candidates.len();
        if let Some(sort_criteria) = &query.sort_by {
            candidates.sort_by(|a, b| match sort_criteria {
                SortCriteria::SuccessRate => b
                    .capabilities
                    .performance_metrics
                    .success_rate
                    .partial_cmp(&a.capabilities.performance_metrics.success_rate)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortCriteria::Quality => b
                    .capabilities
                    .performance_metrics
                    .quality_score
                    .partial_cmp(&a.capabilities.performance_metrics.quality_score)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortCriteria::Collaboration => b
                    .capabilities
                    .performance_metrics
                    .collaboration_score
                    .partial_cmp(&a.capabilities.performance_metrics.collaboration_score)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortCriteria::Innovation => b
                    .capabilities
                    .performance_metrics
                    .innovation_score
                    .partial_cmp(&a.capabilities.performance_metrics.innovation_score)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortCriteria::CompletionTime => a
                    .capabilities
                    .performance_metrics
                    .avg_completion_time
                    .partial_cmp(&b.capabilities.performance_metrics.avg_completion_time)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortCriteria::TaskCount => b
                    .capabilities
                    .performance_metrics
                    .tasks_completed
                    .cmp(&a.capabilities.performance_metrics.tasks_completed),
                SortCriteria::CreatedAt => b.metadata.created_at.cmp(&a.metadata.created_at),
                SortCriteria::UpdatedAt => b.metadata.updated_at.cmp(&a.metadata.updated_at),
            });
        }
        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(usize::MAX);
        let result_agents: Vec<Agent> = candidates
            .into_iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();
        let execution_time = start_time.elapsed().as_millis() as u64;
        Ok(SearchResults {
            agents: result_agents,
            total_count,
            query,
            execution_time_ms: execution_time,
        })
    }
    pub fn list_active_agents(&self) -> Result<Vec<Agent>, RegistryError> {
        let query = AgentQuery {
            status: Some(vec![AgentStatus::Available]),
            ..Default::default()
        };
        let results = self.search_agents(query)?;
        Ok(results.agents)
    }
    pub fn get_agents_by_skill(&self, skill_name: &str, min_proficiency: f64) -> Vec<Agent> {
        self.skill_index
            .get(skill_name)
            .map(|agent_ids| {
                agent_ids
                    .iter()
                    .filter_map(|id| self.agents.get(id))
                    .filter(|agent| {
                        agent.capabilities.technical_skills.iter().any(|skill| {
                            skill.name == skill_name && skill.proficiency >= min_proficiency
                        })
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn total_agent_count(&self) -> usize {
        self.agents.len()
    }
    pub fn active_agent_count(&self) -> usize {
        self.status_index
            .get(&StatusCategory::Available)
            .map(|agents| agents.len())
            .unwrap_or(0)
    }
    pub fn available_agent_count(&self) -> usize {
        self.active_agent_count()
    }

    pub fn get_top_performing_agents(
        &mut self,
        criteria: SortCriteria,
        limit: usize,
    ) -> Vec<Agent> {
        let top_ids = self
            .performance_index
            .get_top_performers(&self.agents, &criteria, limit);
        top_ids
            .into_iter()
            .filter_map(|id| self.agents.get(&id).cloned())
            .collect()
    }

    pub async fn persist_to_storage(&self) -> Result<(), RegistryError> {
        self.save_to_storage().await
    }

    pub async fn restore_from_storage(&mut self) -> Result<(), RegistryError> {
        self.load_from_storage().await
    }

    fn update_indices_for_agent(&mut self, agent: &Agent, add: bool) {
        if add {
            self.add_to_status_index(&agent.status, &agent.id);
            for skill in &agent.capabilities.technical_skills {
                self.skill_index
                    .entry(skill.name.clone())
                    .or_default()
                    .push(agent.id.clone());
            }
            self.performance_index.needs_rebuild = true;
        } else {
            self.remove_from_status_index(&agent.status, &agent.id);
            for skill in &agent.capabilities.technical_skills {
                if let Some(agent_list) = self.skill_index.get_mut(&skill.name) {
                    agent_list.retain(|id| id != &agent.id);
                    if agent_list.is_empty() {
                        self.skill_index.remove(&skill.name);
                    }
                }
            }
            self.performance_index.needs_rebuild = true;
        }
    }
    fn add_to_status_index(&mut self, status: &AgentStatus, agent_id: &str) {
        let status_category = StatusCategory::from_agent_status(status);
        self.status_index
            .entry(status_category)
            .or_default()
            .push(agent_id.to_string());
    }

    fn remove_from_status_index(&mut self, status: &AgentStatus, agent_id: &str) {
        let status_category = StatusCategory::from_agent_status(status);
        if let Some(agent_list) = self.status_index.get_mut(&status_category) {
            agent_list.retain(|id| id != agent_id);
            if agent_list.is_empty() {
                self.status_index.remove(&status_category);
            }
        }
    }
    async fn save_to_storage(&self) -> Result<(), RegistryError> {
        if let Some(storage_path) = &self.config.storage_path {
            let path = Path::new(storage_path);
            if let Some(parent) = path.parent() {
                async_fs::create_dir_all(parent).await?;
            }
            let data = serde_json::to_string_pretty(&self.agents)?;
            async_fs::write(path, data).await?;
        }
        Ok(())
    }

    fn save_to_storage_sync(&self) -> Result<(), RegistryError> {
        if let Some(storage_path) = &self.config.storage_path {
            let path = Path::new(storage_path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let data = serde_json::to_string_pretty(&self.agents)?;
            std::fs::write(path, data)?;
        }
        Ok(())
    }
    async fn load_from_storage(&mut self) -> Result<(), RegistryError> {
        if let Some(storage_path) = &self.config.storage_path {
            let path = Path::new(storage_path);
            if path.exists() {
                let data = async_fs::read_to_string(path).await?;
                let agents: HashMap<String, Agent> = serde_json::from_str(&data)?;
                for agent in agents.values() {
                    self.update_indices_for_agent(agent, true);
                }
                self.agents = agents;
            }
        }
        Ok(())
    }

    fn load_from_storage_sync(&mut self) -> Result<(), RegistryError> {
        if let Some(storage_path) = &self.config.storage_path {
            let path = Path::new(storage_path);
            if path.exists() {
                let data = std::fs::read_to_string(path)?;
                let agents: HashMap<String, Agent> = serde_json::from_str(&data)?;
                for agent in agents.values() {
                    self.update_indices_for_agent(agent, true);
                }
                self.agents = agents;
            }
        }
        Ok(())
    }
}

impl PerformanceIndex {
    fn new() -> Self {
        Self {
            by_success_rate: Vec::new(),
            by_quality: Vec::new(),
            by_collaboration: Vec::new(),
            by_innovation: Vec::new(),
            by_completion_time: Vec::new(),
            by_task_count: Vec::new(),
            needs_rebuild: false,
        }
    }

    fn rebuild(&mut self, agents: &HashMap<String, Agent>) {
        if !self.needs_rebuild {
            return;
        }

        self.by_success_rate.clear();
        self.by_quality.clear();
        self.by_collaboration.clear();
        self.by_innovation.clear();
        self.by_completion_time.clear();
        self.by_task_count.clear();

        for (id, agent) in agents {
            let metrics = &agent.capabilities.performance_metrics;
            self.by_success_rate
                .push((id.clone(), metrics.success_rate));
            self.by_quality.push((id.clone(), metrics.quality_score));
            self.by_collaboration
                .push((id.clone(), metrics.collaboration_score));
            self.by_innovation
                .push((id.clone(), metrics.innovation_score));
            self.by_completion_time
                .push((id.clone(), metrics.avg_completion_time));
            self.by_task_count
                .push((id.clone(), metrics.tasks_completed));
        }

        self.by_success_rate
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        self.by_quality
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        self.by_collaboration
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        self.by_innovation
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        self.by_completion_time
            .sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        self.by_task_count.sort_by(|a, b| b.1.cmp(&a.1));

        self.needs_rebuild = false;
    }

    fn get_top_performers(
        &mut self,
        agents: &HashMap<String, Agent>,
        criteria: &SortCriteria,
        limit: usize,
    ) -> Vec<String> {
        self.rebuild(agents);

        match criteria {
            SortCriteria::TaskCount => self
                .by_task_count
                .iter()
                .take(limit)
                .map(|(id, _)| id.clone())
                .collect(),
            _ => {
                let source = match criteria {
                    SortCriteria::SuccessRate => &self.by_success_rate,
                    SortCriteria::Quality => &self.by_quality,
                    SortCriteria::Collaboration => &self.by_collaboration,
                    SortCriteria::Innovation => &self.by_innovation,
                    SortCriteria::CompletionTime => &self.by_completion_time,
                    _ => return Vec::new(),
                };

                source
                    .iter()
                    .take(limit)
                    .map(|(id, _)| id.clone())
                    .collect()
            }
        }
    }
}

impl StatusCategory {
    fn from_agent_status(status: &AgentStatus) -> Self {
        match status {
            AgentStatus::Available => StatusCategory::Available,
            AgentStatus::Busy { .. } => StatusCategory::Busy,
            AgentStatus::Offline { .. } => StatusCategory::Offline,
            AgentStatus::Error { .. } => StatusCategory::Error,
            AgentStatus::Maintenance => StatusCategory::Maintenance,
            AgentStatus::Deactivated => StatusCategory::Deactivated,
        }
    }
}

impl PartialEq for AgentStatus {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}
impl Eq for AgentStatus {}
impl std::hash::Hash for AgentStatus {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
    }
}
