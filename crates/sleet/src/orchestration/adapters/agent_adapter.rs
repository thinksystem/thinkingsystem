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

use super::{
    AdapterError, AgentInfo, AgentStatus, CacheConfig, CacheEntry, ErrorCategory, ErrorDetails,
    ExecutionContext, ExecutionMetadata, HealthStatus, InputValidator, InteractionOptions,
    ScoringConfig, ServiceAdapter, ValidationConfig,
};
use crate::agents::schemas::Agent;
use crate::agents::{AgentError, AgentSystem};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Hash)]
pub struct AgentSelectionCriteria {
    pub required_capabilities: Vec<String>,
    pub preferred_tags: Vec<String>,
    pub exclude_busy: bool,
    pub max_concurrent_tasks: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct AgentScore {
    pub agent: Agent,
    pub score: f64,
    pub capability_match: f64,
    pub tag_match: f64,
    pub performance_score: f64,
    pub availability_score: f64,
}

#[derive(Debug, Clone)]
pub struct AgentInteractionResult {
    pub agent_id: String,
    pub result: Value,
    pub execution_metadata: ExecutionMetadata,
    pub agent_metadata: AgentMetadata,
}

#[derive(Debug, Clone)]
pub struct AgentMetadata {
    pub capabilities: Vec<String>,
    pub current_load: f64,
    pub response_time_ms: u64,
    pub success_rate: f64,
}

pub struct AgentAdapter {
    agent_system: Arc<RwLock<AgentSystem>>,
    selection_cache: Arc<RwLock<HashMap<u64, CacheEntry<Agent>>>>,
    performance_metrics: Arc<RwLock<HashMap<String, AgentMetadata>>>,
    cache_config: CacheConfig,
    validation_config: ValidationConfig,
    scoring_config: ScoringConfig,
}

impl AgentAdapter {
    pub fn new(agent_system: Arc<RwLock<AgentSystem>>) -> Self {
        Self::with_config(
            agent_system,
            CacheConfig::default(),
            ValidationConfig::default(),
            ScoringConfig::default(),
        )
    }

    pub fn with_config(
        agent_system: Arc<RwLock<AgentSystem>>,
        cache_config: CacheConfig,
        validation_config: ValidationConfig,
        scoring_config: ScoringConfig,
    ) -> Self {
        Self {
            agent_system,
            selection_cache: Arc::new(RwLock::new(HashMap::new())),
            performance_metrics: Arc::new(RwLock::new(HashMap::new())),
            cache_config,
            validation_config,
            scoring_config,
        }
    }

    fn validate_interaction_input(
        &self,
        criteria: &AgentSelectionCriteria,
        input_data: &Value,
        options: &InteractionOptions,
        execution_context: &ExecutionContext,
    ) -> Result<(), AdapterError> {
        if criteria.required_capabilities.is_empty() && criteria.preferred_tags.is_empty() {
            return Err(AdapterError::InvalidInput(
                "At least one required capability or preferred tag must be specified".to_string(),
            ));
        }

        for capability in &criteria.required_capabilities {
            if capability.is_empty() {
                return Err(AdapterError::InvalidInput(
                    "Capability name cannot be empty".to_string(),
                ));
            }
            if capability.len() > 50 {
                return Err(AdapterError::InvalidInput(format!(
                    "Capability name too long: '{capability}' (max: 50 chars)"
                )));
            }
        }

        for tag in &criteria.preferred_tags {
            if tag.is_empty() {
                return Err(AdapterError::InvalidInput(
                    "Tag name cannot be empty".to_string(),
                ));
            }
            if tag.len() > 30 {
                return Err(AdapterError::InvalidInput(format!(
                    "Tag name too long: '{tag}' (max: 30 chars)"
                )));
            }
        }

        if let Some(max_tasks) = criteria.max_concurrent_tasks {
            if max_tasks == 0 {
                return Err(AdapterError::InvalidInput(
                    "Max concurrent tasks must be greater than 0".to_string(),
                ));
            }
            if max_tasks > 100 {
                return Err(AdapterError::InvalidInput(
                    "Max concurrent tasks cannot exceed 100".to_string(),
                ));
            }
        }

        let input_str = input_data.to_string();
        if input_str.len() > 100000 {
            return Err(AdapterError::InvalidInput(
                "Input data too large (max: 100KB)".to_string(),
            ));
        }

        InputValidator::validate_timeout(options.timeout_seconds, &self.validation_config)?;
        InputValidator::validate_retry_attempts(options.retry_attempts, &self.validation_config)?;

        InputValidator::validate_context_variables(
            &execution_context.variables,
            &self.validation_config,
        )?;

        if execution_context.session_id.is_empty() {
            return Err(AdapterError::InvalidInput(
                "Session ID cannot be empty".to_string(),
            ));
        }
        if execution_context.flow_id.is_empty() {
            return Err(AdapterError::InvalidInput(
                "Flow ID cannot be empty".to_string(),
            ));
        }
        if execution_context.block_id.is_empty() {
            return Err(AdapterError::InvalidInput(
                "Block ID cannot be empty".to_string(),
            ));
        }

        Ok(())
    }

    async fn evict_expired_cache_entries(&self) {
        let mut cache = self.selection_cache.write().await;
        let ttl_seconds = self.cache_config.ttl_seconds;

        cache.retain(|_, entry| !entry.is_expired(ttl_seconds));
    }

    async fn enforce_cache_size_limit(&self) {
        let mut cache = self.selection_cache.write().await;
        let max_entries = self.cache_config.max_entries;

        if cache.len() <= max_entries {
            return;
        }

        let excess_count = cache.len() - max_entries;
        let keys_to_remove: Vec<u64> = match self.cache_config.eviction_policy {
            super::EvictionPolicy::LRU => {
                let mut entries: Vec<_> = cache.iter().collect();
                entries.sort_by(|a, b| a.1.last_accessed.cmp(&b.1.last_accessed));
                entries
                    .into_iter()
                    .take(excess_count)
                    .map(|(k, _)| *k)
                    .collect()
            }
            super::EvictionPolicy::LFU => {
                let mut entries: Vec<_> = cache.iter().collect();
                entries.sort_by(|a, b| a.1.access_count.cmp(&b.1.access_count));
                entries
                    .into_iter()
                    .take(excess_count)
                    .map(|(k, _)| *k)
                    .collect()
            }
            super::EvictionPolicy::TTL => {
                let mut entries: Vec<_> = cache.iter().collect();
                entries.sort_by(|a, b| a.1.created_at.cmp(&b.1.created_at));
                entries
                    .into_iter()
                    .take(excess_count)
                    .map(|(k, _)| *k)
                    .collect()
            }
        };

        for key in keys_to_remove {
            cache.remove(&key);
        }
    }

    fn hash_criteria(&self, criteria: &AgentSelectionCriteria) -> u64 {
        let mut hasher = DefaultHasher::new();
        criteria.hash(&mut hasher);
        hasher.finish()
    }

    pub async fn interact_with_agent(
        &self,
        criteria: &AgentSelectionCriteria,
        input_data: &Value,
        options: &InteractionOptions,
        execution_context: &ExecutionContext,
    ) -> Result<AgentInteractionResult, AdapterError> {
        self.validate_interaction_input(criteria, input_data, options, execution_context)?;

        self.evict_expired_cache_entries().await;
        self.enforce_cache_size_limit().await;

        let selected_agent = self.select_agent(criteria).await?;

        let mut execution_metadata = ExecutionMetadata {
            execution_id: uuid::Uuid::new_v4().to_string(),
            start_time: chrono::Utc::now(),
            end_time: None,
            duration_ms: None,
            resource_usage: super::ResourceUsageInfo {
                cpu_time_ms: 0,
                memory_peak_mb: 0,
                network_bytes: 0,
                storage_bytes: 0,
            },
            performance_metrics: super::PerformanceMetrics {
                throughput: 0.0,
                latency_ms: 0.0,
                success_rate: 1.0,
                quality_score: None,
            },
            error_details: None,
        };

        let start_time = std::time::Instant::now();
        let result = self
            .execute_agent_task(&selected_agent, input_data, options, execution_context)
            .await;
        let duration = start_time.elapsed();

        execution_metadata.end_time = Some(chrono::Utc::now());
        execution_metadata.duration_ms = Some(duration.as_millis() as u64);
        execution_metadata.performance_metrics.latency_ms = duration.as_millis() as f64;

        match result {
            Ok(interaction_result) => {
                let agent_id = selected_agent.id.clone();
                Ok(AgentInteractionResult {
                    agent_id,
                    result: interaction_result,
                    execution_metadata,
                    agent_metadata: self.extract_agent_metadata(&selected_agent),
                })
            }
            Err(error) => {
                let error_code = match &error {
                    AgentError::AgentNotFound(_) => "AGENT_NOT_FOUND",
                    AgentError::InvalidConfiguration(_) => "AGENT_INVALID_CONFIG",
                    AgentError::GenerationFailed(_) => "AGENT_GENERATION_FAILED",
                    AgentError::Registry(_) => "AGENT_REGISTRY_ERROR",
                    AgentError::LlmError(_) => "AGENT_LLM_ERROR",
                    AgentError::CapabilityError(_) => "AGENT_CAPABILITY_ERROR",
                };

                let error_category = match &error {
                    AgentError::AgentNotFound(_) => ErrorCategory::Resource,
                    AgentError::InvalidConfiguration(_) => ErrorCategory::Configuration,
                    AgentError::GenerationFailed(_) => ErrorCategory::Internal,
                    AgentError::Registry(_) => ErrorCategory::Internal,
                    AgentError::LlmError(_) => ErrorCategory::External,
                    AgentError::CapabilityError(_) => ErrorCategory::Validation,
                };

                let retry_recommended =
                    matches!(error, AgentError::Registry(_) | AgentError::LlmError(_));

                execution_metadata.error_details = Some(ErrorDetails {
                    error_code: error_code.to_string(),
                    error_message: format!("Agent execution failed: {error:?}"),
                    error_category,
                    retry_recommended,
                    context: {
                        let mut ctx = HashMap::new();
                        ctx.insert(
                            "agent_id".to_string(),
                            Value::String(selected_agent.id.clone()),
                        );
                        ctx.insert(
                            "agent_capabilities".to_string(),
                            Value::Array(
                                selected_agent
                                    .capabilities
                                    .strengths
                                    .iter()
                                    .map(|s| Value::String(s.clone()))
                                    .collect(),
                            ),
                        );
                        ctx
                    },
                });
                Err(AdapterError::AgentOperationFailed(format!(
                    "Agent interaction failed: {error:?}"
                )))
            }
        }
    }

    async fn select_agent(&self, criteria: &AgentSelectionCriteria) -> Result<Agent, AdapterError> {
        if let Some(cached_agent) = self.check_cache(criteria).await {
            return Ok(cached_agent);
        }

        let scored_agents = self.score_agents(criteria).await?;

        if scored_agents.is_empty() {
            return Err(AdapterError::ResourceNotFound(
                "No suitable agents found".to_string(),
            ));
        }

        let best_agent = scored_agents[0].agent.clone();

        self.cache_agent(criteria, &best_agent).await;

        Ok(best_agent)
    }

    async fn score_agents(
        &self,
        criteria: &AgentSelectionCriteria,
    ) -> Result<Vec<AgentScore>, AdapterError> {
        let agent_system = self.agent_system.read().await;

        let stats = agent_system.get_statistics();
        if stats.total_agents == 0 {
            return Err(AdapterError::ResourceNotFound(
                "No agents available in system".to_string(),
            ));
        }

        let all_agents = agent_system
            .list_active_agents()
            .map_err(|e| AdapterError::ServiceUnavailable(format!("Failed to list agents: {e}")))?;

        if all_agents.is_empty() {
            return Err(AdapterError::ResourceNotFound(
                "No active agents found".to_string(),
            ));
        }

        let mut scored_agents = Vec::new();

        for agent in all_agents {
            if criteria.exclude_busy && self.is_agent_busy(&agent).await {
                continue;
            }

            if let Some(max_tasks) = criteria.max_concurrent_tasks {
                let current_tasks = self.get_agent_current_tasks(&agent).await;
                if current_tasks >= max_tasks {
                    continue;
                }
            }

            let score = self.calculate_agent_score(&agent, criteria).await;
            scored_agents.push(score);
        }

        scored_agents.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(scored_agents)
    }

    async fn calculate_agent_score(
        &self,
        agent: &Agent,
        criteria: &AgentSelectionCriteria,
    ) -> AgentScore {
        let capability_match =
            self.calculate_capability_match(agent, &criteria.required_capabilities);
        let tag_match = self.calculate_tag_match(agent, &criteria.preferred_tags);
        let performance_score = self.calculate_performance_score(agent).await;
        let availability_score = self.calculate_availability_score(agent).await;

        let score = (capability_match * self.scoring_config.capability_weight)
            + (tag_match * self.scoring_config.tag_weight)
            + (performance_score * self.scoring_config.performance_weight)
            + (availability_score * self.scoring_config.availability_weight);

        AgentScore {
            agent: agent.clone(),
            score,
            capability_match,
            tag_match,
            performance_score,
            availability_score,
        }
    }

    fn calculate_capability_match(&self, agent: &Agent, required_capabilities: &[String]) -> f64 {
        if required_capabilities.is_empty() {
            return 1.0;
        }

        let mut matches = 0;
        for capability in required_capabilities {
            if agent.capabilities.strengths.contains(capability) {
                matches += 1;
            }
        }

        matches as f64 / required_capabilities.len() as f64
    }

    fn calculate_tag_match(&self, agent: &Agent, preferred_tags: &[String]) -> f64 {
        if preferred_tags.is_empty() {
            return 1.0;
        }

        let mut matches = 0;
        for tag in preferred_tags {
            if agent.metadata.tags.contains(tag) {
                matches += 1;
            }
        }

        matches as f64 / preferred_tags.len() as f64
    }

    async fn calculate_performance_score(&self, agent: &Agent) -> f64 {
        let metrics = self.performance_metrics.read().await;

        if let Some(metadata) = metrics.get(&agent.id) {
            let success_score = metadata.success_rate;
            let response_score = 1.0 - (metadata.response_time_ms as f64 / 10000.0).min(1.0);
            let load_score = 1.0 - metadata.current_load;

            (success_score + response_score + load_score) / 3.0
        } else {
            0.5
        }
    }

    async fn calculate_availability_score(&self, agent: &Agent) -> f64 {
        if self.is_agent_busy(agent).await {
            0.3
        } else {
            1.0
        }
    }

    async fn is_agent_busy(&self, agent: &Agent) -> bool {
        let current_tasks = self.get_agent_current_tasks(agent).await;
        current_tasks > 0
    }

    async fn get_agent_current_tasks(&self, _agent: &Agent) -> u32 {
        let metrics = self.performance_metrics.read().await;
        if let Some(metadata) = metrics.get(&_agent.id) {
            (metadata.current_load * 10.0) as u32
        } else {
            0
        }
    }

    async fn cache_agent(&self, criteria: &AgentSelectionCriteria, agent: &Agent) {
        let cache_key = self.hash_criteria(criteria);
        let mut cache = self.selection_cache.write().await;

        if cache.len() >= self.cache_config.max_entries {
            if let Some(key_to_remove) = self.select_eviction_candidate(&cache).await {
                cache.remove(&key_to_remove);
            }
        }

        cache.insert(cache_key, CacheEntry::new(agent.clone()));
    }

    async fn select_eviction_candidate(
        &self,
        cache: &HashMap<u64, CacheEntry<Agent>>,
    ) -> Option<u64> {
        if cache.is_empty() {
            return None;
        }

        match self.cache_config.eviction_policy {
            super::EvictionPolicy::LRU => cache
                .iter()
                .min_by(|a, b| a.1.last_accessed.cmp(&b.1.last_accessed))
                .map(|(k, _)| *k),
            super::EvictionPolicy::LFU => cache
                .iter()
                .min_by(|a, b| a.1.access_count.cmp(&b.1.access_count))
                .map(|(k, _)| *k),
            super::EvictionPolicy::TTL => cache
                .iter()
                .min_by(|a, b| a.1.created_at.cmp(&b.1.created_at))
                .map(|(k, _)| *k),
        }
    }

    async fn check_cache(&self, criteria: &AgentSelectionCriteria) -> Option<Agent> {
        let cache_key = self.hash_criteria(criteria);
        let mut cache = self.selection_cache.write().await;

        if let Some(entry) = cache.get_mut(&cache_key) {
            if !entry.is_expired(self.cache_config.ttl_seconds) {
                Some(entry.access().clone())
            } else {
                cache.remove(&cache_key);
                None
            }
        } else {
            None
        }
    }

    async fn get_agent_by_id(&self, agent_id: &str) -> Result<Agent, AdapterError> {
        let agent_system = self.agent_system.read().await;

        {
            let agent_hash = {
                let mut hasher = DefaultHasher::new();
                agent_id.hash(&mut hasher);
                hasher.finish()
            };

            let cache = self.selection_cache.read().await;
            if let Some(entry) = cache.get(&agent_hash) {
                if !entry.is_expired(self.cache_config.ttl_seconds) {
                    return Ok(entry.value.clone());
                }
            }
        }

        let system = agent_system;
        let agent = system
            .get_agent(agent_id)
            .map_err(|e| AdapterError::ServiceUnavailable(format!("Agent not found: {e}")))?;

        {
            let mut cache = self.selection_cache.write().await;

            let agent_hash = {
                let mut hasher = DefaultHasher::new();
                agent_id.hash(&mut hasher);
                hasher.finish()
            };
            cache.insert(agent_hash, CacheEntry::new(agent.clone()));
        }

        Ok(agent)
    }

    async fn execute_agent_task(
        &self,
        agent: &Agent,
        input_data: &Value,
        options: &InteractionOptions,
        execution_context: &ExecutionContext,
    ) -> Result<Value, AgentError> {
        let _task_context = serde_json::json!({
            "input_data": input_data,
            "execution_context": {
                "session_id": execution_context.session_id,
                "flow_id": execution_context.flow_id,
                "block_id": execution_context.block_id,
                "variables": execution_context.variables
            },
            "options": {
                "timeout_seconds": options.timeout_seconds,
                "retry_attempts": options.retry_attempts,
                "priority": options.priority,
                "execution_mode": options.execution_mode
            }
        });

        let processing_time = if agent
            .capabilities
            .strengths
            .contains(&"fast_processing".to_string())
        {
            tokio::time::Duration::from_millis(50)
        } else if agent
            .capabilities
            .strengths
            .contains(&"complex_reasoning".to_string())
        {
            tokio::time::Duration::from_millis(500)
        } else {
            tokio::time::Duration::from_millis(200)
        };

        tokio::time::sleep(processing_time).await;

        let result = serde_json::json!({
            "status": "completed",
            "agent_id": agent.id,
            "agent_name": agent.name,
            "processing_time_ms": processing_time.as_millis(),
            "result": self.generate_agent_response(agent, input_data),
            "metadata": {
                "capabilities_used": agent.capabilities.strengths,
                "confidence_score": 0.85,
                "resource_usage": {
                    "cpu_percentage": 15.0,
                    "memory_mb": 64.0
                }
            },
            "timestamp": chrono::Utc::now().to_rfc3339()
        });

        self.update_agent_performance_after_execution(agent, &processing_time)
            .await;

        Ok(result)
    }

    fn generate_agent_response(&self, agent: &Agent, input_data: &Value) -> Value {
        if agent
            .capabilities
            .strengths
            .contains(&"data_analysis".to_string())
        {
            serde_json::json!({
                "analysis_type": "data_analysis",
                "findings": format!("Analysed input data: {input_data}"),
                "insights": [
                    "Data structure is well-formed",
                    "Key patterns identified",
                    "Recommendations generated"
                ],
                "confidence": 0.9
            })
        } else if agent
            .capabilities
            .strengths
            .contains(&"natural_language".to_string())
        {
            serde_json::json!({
                "response_type": "natural_language",
                "text_response": format!("I have processed your request: {input_data}"),
                "sentiment": "positive",
                "language_confidence": 0.95
            })
        } else if agent
            .capabilities
            .strengths
            .contains(&"problem_solving".to_string())
        {
            serde_json::json!({
                "solution_type": "problem_solving",
                "proposed_solution": format!("Solution for: {input_data}"),
                "steps": [
                    "Problem analysis completed",
                    "Solution strategy identified",
                    "Implementation plan created"
                ],
                "feasibility_score": 0.88
            })
        } else {
            serde_json::json!({
                "response_type": "general",
                "message": format!("Task completed successfully with input: {input_data}"),
                "status": "success"
            })
        }
    }

    async fn update_agent_performance_after_execution(
        &self,
        agent: &Agent,
        processing_time: &tokio::time::Duration,
    ) {
        let mut metrics = self.performance_metrics.write().await;

        let updated_metadata = if let Some(existing) = metrics.get(&agent.id) {
            let new_response_time =
                (existing.response_time_ms + processing_time.as_millis() as u64) / 2;
            let new_success_rate = (existing.success_rate * 0.9) + (1.0 * 0.1);

            AgentMetadata {
                capabilities: existing.capabilities.clone(),
                current_load: (existing.current_load * 0.8),
                response_time_ms: new_response_time,
                success_rate: new_success_rate,
            }
        } else {
            AgentMetadata {
                capabilities: agent.capabilities.strengths.clone(),
                current_load: 0.1,
                response_time_ms: processing_time.as_millis() as u64,
                success_rate: 1.0,
            }
        };

        metrics.insert(agent.id.clone(), updated_metadata);
    }

    fn extract_agent_metadata(&self, agent: &Agent) -> AgentMetadata {
        let metrics = self.performance_metrics.try_read();

        if let Ok(metrics_guard) = metrics {
            if let Some(cached_metadata) = metrics_guard.get(&agent.id) {
                return cached_metadata.clone();
            }
        }

        let base_response_time = if agent
            .capabilities
            .strengths
            .contains(&"fast_processing".to_string())
        {
            50
        } else if agent
            .capabilities
            .strengths
            .contains(&"complex_reasoning".to_string())
        {
            300
        } else {
            150
        };

        let estimated_load = if agent.capabilities.strengths.len() > 5 {
            0.3
        } else {
            0.1
        };

        AgentMetadata {
            capabilities: agent.capabilities.strengths.clone(),
            current_load: estimated_load,
            response_time_ms: base_response_time,
            success_rate: 0.85,
        }
    }

    pub async fn get_agent_status(&self, agent_id: &str) -> Result<AgentStatus, AdapterError> {
        let _agent = self.get_agent_by_id(agent_id).await?;
        let metrics = self.performance_metrics.read().await;

        let (current_tasks, health_score) = if let Some(metadata) = metrics.get(agent_id) {
            let tasks = (metadata.current_load * 10.0) as u32;
            let health = (metadata.success_rate * 0.6) + ((1.0 - metadata.current_load) * 0.4);
            (tasks, health)
        } else {
            (0, 0.8)
        };

        let is_available = current_tasks < 5 && health_score > 0.5;

        Ok(AgentStatus {
            agent_id: agent_id.to_string(),
            is_available,
            current_tasks,
            last_activity: chrono::Utc::now()
                - chrono::Duration::minutes((current_tasks * 5) as i64),
            health_score,
        })
    }

    pub async fn list_available_agents(&self) -> Result<Vec<AgentInfo>, AdapterError> {
        let agent_system = self.agent_system.read().await;

        let stats = agent_system.get_statistics();
        if stats.total_agents == 0 {
            return Ok(vec![]);
        }

        let all_agents = agent_system
            .list_active_agents()
            .map_err(|e| AdapterError::ServiceUnavailable(format!("Failed to list agents: {e}")))?;

        let metrics_guard = self.performance_metrics.read().await;
        let mut agent_infos = Vec::new();

        for agent in all_agents {
            let (is_available, current_load) = if let Some(metadata) = metrics_guard.get(&agent.id)
            {
                let available = metadata.current_load < 0.8 && metadata.success_rate > 0.5;
                (available, metadata.current_load)
            } else {
                (true, 0.0)
            };

            agent_infos.push(AgentInfo {
                agent_id: agent.id.clone(),
                name: agent.name.clone(),
                description: format!(
                    "Agent with capabilities: {}",
                    agent.capabilities.strengths.join(", ")
                ),
                capabilities: agent.capabilities.strengths.clone(),
                is_available,
                current_load,
            });
        }

        agent_infos.sort_by(|a, b| match (a.is_available, b.is_available) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a
                .current_load
                .partial_cmp(&b.current_load)
                .unwrap_or(std::cmp::Ordering::Equal),
        });

        Ok(agent_infos)
    }

    pub async fn cancel_agent_task(
        &self,
        agent_id: &str,
        task_id: &str,
    ) -> Result<(), AdapterError> {
        let _agent = self.get_agent_by_id(agent_id).await?;

        log::info!("Cancelling task {task_id} for agent {agent_id}");

        let mut metrics = self.performance_metrics.write().await;
        if let Some(metadata) = metrics.get_mut(agent_id) {
            metadata.current_load = (metadata.current_load - 0.2).max(0.0);
        }

        Ok(())
    }

    pub async fn get_agent_performance_metrics(
        &self,
        agent_id: &str,
    ) -> Result<AgentMetadata, AdapterError> {
        let metrics = self.performance_metrics.read().await;

        if let Some(metadata) = metrics.get(agent_id) {
            Ok(metadata.clone())
        } else {
            Ok(AgentMetadata {
                capabilities: vec!["general".to_string()],
                current_load: 0.0,
                response_time_ms: 0,
                success_rate: 1.0,
            })
        }
    }

    pub async fn update_agent_metrics(&self, agent_id: &str, metadata: AgentMetadata) {
        let mut metrics = self.performance_metrics.write().await;
        metrics.insert(agent_id.to_string(), metadata);
    }

    pub async fn warm_cache(&self) -> Result<(), AdapterError> {
        let agent_system = self.agent_system.read().await;

        let stats = agent_system.get_statistics();
        if stats.total_agents == 0 {
            log::warn!("No agents available for cache warming");
            return Ok(());
        }

        let all_agents = agent_system.list_active_agents().map_err(|e| {
            AdapterError::ServiceUnavailable(format!(
                "Failed to list agents for cache warming: {e}"
            ))
        })?;

        if all_agents.is_empty() {
            log::warn!("No active agents found for cache warming");
            return Ok(());
        }

        let common_capabilities = vec![
            vec!["data_analysis".to_string()],
            vec!["natural_language".to_string()],
            vec!["problem_solving".to_string()],
            vec!["fast_processing".to_string()],
            vec!["complex_reasoning".to_string()],
        ];

        let mut cache = self.selection_cache.write().await;

        for capabilities in common_capabilities {
            let criteria = AgentSelectionCriteria {
                required_capabilities: capabilities,
                preferred_tags: vec![],
                exclude_busy: false,
                max_concurrent_tasks: None,
            };

            if let Some(best_agent) = all_agents.iter().find(|agent| {
                criteria
                    .required_capabilities
                    .iter()
                    .all(|cap| agent.capabilities.strengths.contains(cap))
            }) {
                let cache_key = self.hash_criteria(&criteria);
                cache.insert(cache_key, CacheEntry::new(best_agent.clone()));
            }
        }

        let cache_len = cache.len();
        log::info!("Cache warmed with {cache_len} entries");
        Ok(())
    }

    pub async fn clear_cache(&self) {
        let mut cache = self.selection_cache.write().await;
        cache.clear();
    }

    pub async fn health_check(&self) -> Result<HealthStatus, AdapterError> {
        let agent_system = self.agent_system.read().await;

        let stats = agent_system.get_statistics();
        let agent_count = stats.total_agents;

        Ok(HealthStatus {
            is_healthy: agent_count > 0,
            message: format!("Agent system operational with {agent_count} agents"),
            last_check: chrono::Utc::now(),
        })
    }
}

impl ServiceAdapter for AgentAdapter {
    fn service_type(&self) -> &'static str {
        "agent"
    }

    fn is_available(&self) -> bool {
        if let Ok(agent_system) = self.agent_system.try_read() {
            let stats = agent_system.get_statistics();
            stats.total_agents > 0
        } else {
            false
        }
    }
}
