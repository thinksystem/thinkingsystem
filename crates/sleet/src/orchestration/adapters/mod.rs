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

pub mod agent_adapter;
pub mod llm_adapter;
pub mod task_adapter;
pub mod workflow_adapter;

pub use agent_adapter::{AgentAdapter, AgentInteractionResult};
pub use llm_adapter::{LLMAdapter, LLMProcessingResult};
pub use task_adapter::{TaskAdapter, TaskExecutionResult};
pub use workflow_adapter::WorkflowAdapter;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("Service not available: {0}")]
    ServiceUnavailable(String),
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Timeout: operation timed out after {timeout_secs} seconds")]
    TimeoutError { timeout_secs: u64 },
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Resource error: {0}")]
    ResourceError(String),
    #[error("Agent operation failed: {0}")]
    AgentOperationFailed(String),
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),
    #[error("Event error: {0}")]
    EventError(String),
    #[error("Task execution failed: {0}")]
    TaskExecutionFailed(String),
    #[error("LLM processing failed: {0}")]
    LLMProcessingFailed(String),
}

impl From<serde_json::Error> for AdapterError {
    fn from(error: serde_json::Error) -> Self {
        AdapterError::ConfigurationError(format!("JSON serialisation error: {error}"))
    }
}

pub type AdapterResult<T> = Result<T, AdapterError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentSelector {
    ById(String),
    ByCapability(Vec<String>),
    ByTag(Vec<String>),
    Dynamic(SelectionCriteria),
    BestAvailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionCriteria {
    pub required_capabilities: Vec<String>,
    pub preferred_capabilities: Vec<String>,
    pub performance_threshold: Option<f64>,
    pub availability_requirement: AvailabilityRequirement,
    pub cost_constraints: Option<CostConstraints>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AvailabilityRequirement {
    Immediate,
    WithinSeconds(u64),
    Flexible,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostConstraints {
    pub max_cost_per_operation: Option<f64>,
    pub total_budget: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InteractionType {
    Query {
        expect_response: bool,
        response_format: ResponseFormat,
    },
    Command {
        expect_confirmation: bool,
        timeout_secs: Option<u64>,
    },
    Collaboration {
        other_agents: Vec<String>,
        coordination_strategy: CoordinationStrategy,
    },
    Analysis {
        analysis_type: AnalysisType,
        depth_level: AnalysisDepth,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseFormat {
    Text,
    Json,
    Structured { schema: Value },
    Binary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CoordinationStrategy {
    Sequential,
    Parallel,
    Consensus,
    Competition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnalysisType {
    Sentiment,
    Classification,
    Extraction,
    Summarization,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnalysisDepth {
    Surface,
    Moderate,
    Deep,
    Exhaustive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionMode {
    Synchronous,
    Asynchronous,
    Background,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionOptions {
    pub timeout_seconds: Option<u64>,
    pub retry_attempts: u32,
    pub priority: Priority,
    pub execution_mode: ExecutionMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStrategy {
    Immediate,
    Scheduled {
        start_time: chrono::DateTime<chrono::Utc>,
    },
    Queued {
        priority: TaskPriority,
    },
    Batch {
        batch_size: u32,
    },
    Distributed {
        node_count: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    pub execution_id: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub duration_ms: Option<u64>,
    pub resource_usage: ResourceUsageInfo,
    pub performance_metrics: PerformanceMetrics,
    pub error_details: Option<ErrorDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsageInfo {
    pub cpu_time_ms: u64,
    pub memory_peak_mb: u64,
    pub network_bytes: u64,
    pub storage_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub throughput: f64,
    pub latency_ms: f64,
    pub success_rate: f64,
    pub quality_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetails {
    pub error_code: String,
    pub error_message: String,
    pub error_category: ErrorCategory,
    pub retry_recommended: bool,
    pub context: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorCategory {
    Configuration,
    Resource,
    Network,
    Timeout,
    Validation,
    Internal,
    External,
}

impl Default for ExecutionMetadata {
    fn default() -> Self {
        Self {
            execution_id: uuid::Uuid::new_v4().to_string(),
            start_time: chrono::Utc::now(),
            end_time: None,
            duration_ms: None,
            resource_usage: ResourceUsageInfo {
                cpu_time_ms: 0,
                memory_peak_mb: 0,
                network_bytes: 0,
                storage_bytes: 0,
            },
            performance_metrics: PerformanceMetrics {
                throughput: 0.0,
                latency_ms: 0.0,
                success_rate: 1.0,
                quality_score: None,
            },
            error_details: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub session_id: String,
    pub flow_id: String,
    pub block_id: String,
    pub variables: HashMap<String, serde_json::Value>,
    pub metadata: HashMap<String, String>,
}

impl ExecutionContext {
    pub fn get_variable(&self, key: &str) -> Option<&serde_json::Value> {
        self.variables.get(key)
    }

    pub fn set_variable(&mut self, key: String, value: serde_json::Value) {
        self.variables.insert(key, value);
    }

    pub fn from_context_manager(
        ctx: &crate::orchestration::context_manager::ExecutionContext,
        flow_id: String,
        block_id: String,
    ) -> Self {
        Self {
            session_id: ctx.session_id.clone(),
            flow_id,
            block_id,
            variables: ctx.variables.clone(),
            metadata: ctx
                .metadata
                .iter()
                .map(|(k, v)| (k.clone(), v.to_string()))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: f64,
    pub network_io_kb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub agent_id: String,
    pub is_available: bool,
    pub current_tasks: u32,
    pub last_activity: chrono::DateTime<chrono::Utc>,
    pub health_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub agent_id: String,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub is_available: bool,
    pub current_load: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub is_healthy: bool,
    pub message: String,
    pub last_check: chrono::DateTime<chrono::Utc>,
}

pub trait ServiceAdapter {
    fn service_type(&self) -> &'static str;
    fn is_available(&self) -> bool;
}

pub const DEFAULT_CACHE_MAX_ENTRIES: usize = 1000;
pub const DEFAULT_CACHE_TTL_SECONDS: u64 = 300;
pub const DEFAULT_MAX_PROMPT_LENGTH: usize = 50000;
pub const DEFAULT_MAX_RETRY_ATTEMPTS: u32 = 3;
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 30;

pub const CAPABILITY_WEIGHT: f64 = 0.4;
pub const TAG_WEIGHT: f64 = 0.2;
pub const PERFORMANCE_WEIGHT: f64 = 0.3;
pub const AVAILABILITY_WEIGHT: f64 = 0.1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub max_entries: usize,
    pub ttl_seconds: u64,
    pub eviction_policy: EvictionPolicy,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: DEFAULT_CACHE_MAX_ENTRIES,
            ttl_seconds: DEFAULT_CACHE_TTL_SECONDS,
            eviction_policy: EvictionPolicy::LRU,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvictionPolicy {
    LRU,
    LFU,
    TTL,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    pub max_prompt_length: usize,
    pub max_retry_attempts: u32,
    pub default_timeout_seconds: u64,
    pub max_context_variables: usize,
    pub max_dependency_depth: u32,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_prompt_length: DEFAULT_MAX_PROMPT_LENGTH,
            max_retry_attempts: DEFAULT_MAX_RETRY_ATTEMPTS,
            default_timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
            max_context_variables: 100,
            max_dependency_depth: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringConfig {
    pub capability_weight: f64,
    pub tag_weight: f64,
    pub performance_weight: f64,
    pub availability_weight: f64,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            capability_weight: CAPABILITY_WEIGHT,
            tag_weight: TAG_WEIGHT,
            performance_weight: PERFORMANCE_WEIGHT,
            availability_weight: AVAILABILITY_WEIGHT,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheEntry<T> {
    pub value: T,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_accessed: chrono::DateTime<chrono::Utc>,
    pub access_count: u64,
}

impl<T> CacheEntry<T> {
    pub fn new(value: T) -> Self {
        let now = chrono::Utc::now();
        Self {
            value,
            created_at: now,
            last_accessed: now,
            access_count: 1,
        }
    }

    pub fn access(&mut self) -> &T {
        self.last_accessed = chrono::Utc::now();
        self.access_count += 1;
        &self.value
    }

    pub fn is_expired(&self, ttl_seconds: u64) -> bool {
        let now = chrono::Utc::now();
        let age = now.signed_duration_since(self.created_at);
        age.num_seconds() as u64 > ttl_seconds
    }
}

pub struct InputValidator;

impl InputValidator {
    pub fn validate_prompt(prompt: &str, config: &ValidationConfig) -> Result<(), AdapterError> {
        if prompt.is_empty() {
            return Err(AdapterError::InvalidInput(
                "Prompt cannot be empty".to_string(),
            ));
        }

        if prompt.len() > config.max_prompt_length {
            return Err(AdapterError::InvalidInput(format!(
                "Prompt too long: {} characters (max: {})",
                prompt.len(),
                config.max_prompt_length
            )));
        }

        let harmful_patterns = ["<script", "javascript:", "data:text/html"];
        for pattern in &harmful_patterns {
            if prompt.to_lowercase().contains(pattern) {
                return Err(AdapterError::InvalidInput(
                    "Prompt contains potentially harmful content".to_string(),
                ));
            }
        }

        Ok(())
    }

    pub fn validate_context_variables(
        variables: &HashMap<String, Value>,
        config: &ValidationConfig,
    ) -> Result<(), AdapterError> {
        if variables.len() > config.max_context_variables {
            return Err(AdapterError::InvalidInput(format!(
                "Too many context variables: {} (max: {})",
                variables.len(),
                config.max_context_variables
            )));
        }

        for (key, value) in variables {
            if key.is_empty() {
                return Err(AdapterError::InvalidInput(
                    "Context variable key cannot be empty".to_string(),
                ));
            }

            if key.len() > 100 {
                return Err(AdapterError::InvalidInput(format!(
                    "Context variable key too long: '{}' ({} chars, max: 100)",
                    key,
                    key.len()
                )));
            }

            if let Value::String(s) = value {
                if s.len() > 10000 {
                    return Err(AdapterError::InvalidInput(format!(
                        "Context variable '{}' value too long ({} chars, max: 10000)",
                        key,
                        s.len()
                    )));
                }
            }
        }

        Ok(())
    }

    pub fn validate_timeout(
        timeout_seconds: Option<u64>,
        config: &ValidationConfig,
    ) -> Result<u64, AdapterError> {
        match timeout_seconds {
            Some(timeout) => {
                if timeout == 0 {
                    return Err(AdapterError::InvalidInput(
                        "Timeout must be greater than 0".to_string(),
                    ));
                }
                if timeout > 3600 {
                    return Err(AdapterError::InvalidInput(
                        "Timeout cannot exceed 1 hour (3600 seconds)".to_string(),
                    ));
                }
                Ok(timeout)
            }
            None => Ok(config.default_timeout_seconds),
        }
    }

    pub fn validate_retry_attempts(
        attempts: u32,
        config: &ValidationConfig,
    ) -> Result<(), AdapterError> {
        if attempts > config.max_retry_attempts {
            return Err(AdapterError::InvalidInput(format!(
                "Too many retry attempts: {} (max: {})",
                attempts, config.max_retry_attempts
            )));
        }
        Ok(())
    }

    pub fn validate_task_dependencies(
        dependencies: &[String],
        config: &ValidationConfig,
    ) -> Result<(), AdapterError> {
        if dependencies.len() as u32 > config.max_dependency_depth {
            return Err(AdapterError::InvalidInput(format!(
                "Too many task dependencies: {} (max: {})",
                dependencies.len(),
                config.max_dependency_depth
            )));
        }

        let mut unique_deps = std::collections::HashSet::new();
        for dep in dependencies {
            if dep.is_empty() {
                return Err(AdapterError::InvalidInput(
                    "Dependency ID cannot be empty".to_string(),
                ));
            }
            if !unique_deps.insert(dep) {
                return Err(AdapterError::InvalidInput(format!(
                    "Duplicate dependency found: '{dep}'"
                )));
            }
        }

        Ok(())
    }
}
