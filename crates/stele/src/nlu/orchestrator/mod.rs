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

use crate::llm::unified_adapter::UnifiedLLMAdapter;
use crate::nlu::llm_processor::{CustomLLMAdapter, LLMAdapter};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};
pub mod adapter;
pub mod analyser;
pub mod config;
pub mod data_models;
pub mod error;
pub mod executor;
pub mod planner;
pub use adapter::*;
pub use analyser::InputAnalysis;
pub use config::*;
pub use data_models::*;
pub use error::OrchestratorError;
pub use planner::ProcessingPlan;
pub struct NLUOrchestrator {
    config: NLUConfig,
    llm_adapters: HashMap<String, Arc<dyn LLMAdapter + Send + Sync>>,
    prompt_cache: HashMap<String, String>,
}
impl NLUOrchestrator {
    #[instrument(skip(config_path), name = "nlu_orchestrator_new")]
    pub async fn new(config_path: &str) -> Result<Self, OrchestratorError> {
        let config = Self::load_config(config_path).await?;
        let llm_adapters = Self::initialise_adapters(&config).await?;
        Ok(Self {
            config,
            llm_adapters,
            prompt_cache: HashMap::new(),
        })
    }

    #[instrument(
        skip(config_path, unified_adapter),
        name = "nlu_orchestrator_with_adapter"
    )]
    pub async fn with_unified_adapter(
        config_path: &str,
        unified_adapter: Arc<UnifiedLLMAdapter>,
    ) -> Result<Self, OrchestratorError> {
        let config = Self::load_config(config_path).await?;

        let mut llm_adapters = HashMap::new();
        for model in &config.models {
            llm_adapters.insert(
                model.name.clone(),
                unified_adapter.clone() as Arc<dyn LLMAdapter + Send + Sync>,
            );
        }

        info!(
            "Initialised NLU orchestrator with shared unified LLM adapter for {} models",
            llm_adapters.len()
        );
        Ok(Self {
            config,
            llm_adapters,
            prompt_cache: HashMap::new(),
        })
    }
    async fn load_config(config_path: &str) -> Result<NLUConfig, OrchestratorError> {
        let models_content =
            tokio::fs::read_to_string(&format!("{config_path}/llm_models.yml")).await?;
        let models_config: serde_yaml::Value = serde_yaml::from_str(&models_content)?;
        let prompts_content =
            tokio::fs::read_to_string(&format!("{config_path}/prompts.yml")).await?;
        let prompts_config: HashMap<String, PromptTemplate> =
            serde_yaml::from_str(&prompts_content)?;
        let rules_content = tokio::fs::read_to_string(&format!("{config_path}/rules.yml")).await?;
        let rules_config: serde_yaml::Value = serde_yaml::from_str(&rules_content)?;
        let security_config =
            match tokio::fs::read_to_string(&format!("{config_path}/security.yml")).await {
                Ok(content) => {
                    let security_value: serde_yaml::Value = serde_yaml::from_str(&content)?;
                    serde_yaml::from_value(security_value["security"].clone())?
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    warn!("security.yml not found, using default security settings.");
                    SecurityConfig::default()
                }
                Err(e) => return Err(e.into()),
            };
        let config = NLUConfig {
            models: serde_yaml::from_value(models_config["models"].clone())?,
            selection_strategy: serde_yaml::from_value(
                models_config["selection_strategy"].clone(),
            )?,
            prompts: prompts_config,
            policies: serde_yaml::from_value(rules_config["policies"].clone())?,
            global_settings: serde_yaml::from_value(rules_config["global_settings"].clone())?,
            tasks: serde_yaml::from_value(rules_config["tasks"].clone())?,
            security: security_config,
        };
        info!(
            "Loaded NLU config with {} models, {} prompts, {} policies",
            config.models.len(),
            config.prompts.len(),
            config.policies.len()
        );
        Ok(config)
    }
    async fn initialise_adapters(
        config: &NLUConfig,
    ) -> Result<HashMap<String, Arc<dyn LLMAdapter + Send + Sync>>, OrchestratorError> {
        let mut adapters = HashMap::new();
        for model in &config.models {
            let adapter: Arc<dyn LLMAdapter + Send + Sync> = match model.provider.as_str() {
                "anthropic" => Arc::new(Self::create_anthropic_adapter(model)?),
                "openai" => Arc::new(Self::create_openai_adapter(model)?),
                "ollama" => {
                    let unified_adapter =
                        UnifiedLLMAdapter::with_defaults().await.map_err(|e| {
                            OrchestratorError::new(format!("Failed to create unified adapter: {e}"))
                        })?;
                    Arc::new(unified_adapter)
                }
                _ => {
                    return Err(OrchestratorError::new(format!(
                        "Unsupported provider: {}",
                        model.provider
                    )))
                }
            };
            adapters.insert(model.name.clone(), adapter);
        }
        info!("Initialised {} LLM adapters", adapters.len());
        Ok(adapters)
    }
    fn create_anthropic_adapter(
        model: &ModelConfig,
    ) -> Result<CustomLLMAdapter, OrchestratorError> {
        let adapter =
            CustomLLMAdapter::new(model.name.clone(), model.max_tokens, model.temperature);
        Ok(adapter)
    }
    fn create_openai_adapter(model: &ModelConfig) -> Result<CustomLLMAdapter, OrchestratorError> {
        let adapter =
            CustomLLMAdapter::new(model.name.clone(), model.max_tokens, model.temperature);
        Ok(adapter)
    }
    #[instrument(skip(self))]
    pub async fn process_input(&self, input: &str) -> Result<UnifiedNLUData, OrchestratorError> {
        let start_time = std::time::Instant::now();
        let analysis = analyser::analyse(input);
        debug!("Input analysis: {:?}", analysis);
        let policy = self.select_policy(&analysis)?;
        info!("Selected policy: {}", policy.name);
        let plan = planner::create_plan(policy, &self.config, input)?;
        debug!("Created plan with {} tasks", plan.tasks.len());
        let task_results = executor::execute(&plan, &self.llm_adapters, input).await?;
        let unified_data =
            self.consolidate_results(task_results, &policy.name, start_time, input)?;
        info!(
            "Processing completed in {}ms",
            unified_data.processing_metadata.execution_time_ms
        );
        Ok(unified_data)
    }
    fn select_policy(
        &self,
        analysis: &InputAnalysis,
    ) -> Result<&ProcessingPolicy, OrchestratorError> {
        let mut matching_policies: Vec<&ProcessingPolicy> = self
            .config
            .policies
            .iter()
            .filter(|p| self.policy_matches(p, analysis))
            .collect();
        if matching_policies.is_empty() {
            return Err(OrchestratorError::new(
                "No matching policy found".to_string(),
            ));
        }
        matching_policies.sort_by(|a, b| b.priority.cmp(&a.priority));
        Ok(matching_policies[0])
    }
    fn policy_matches(&self, policy: &ProcessingPolicy, analysis: &InputAnalysis) -> bool {
        policy.conditions.iter().all(|(condition, value)| {
            let is_match = match condition.as_str() {
                "input_length" => self.check_numerical_condition(analysis.length as f64, value),
                "word_count" => self.check_numerical_condition(analysis.word_count as f64, value),
                "complexity_score" => {
                    self.check_numerical_condition(analysis.complexity_score, value)
                }
                "contains_question_words" => {
                    self.check_boolean_condition(analysis.contains_question_words, value)
                }
                "ends_with_question_mark" => {
                    self.check_boolean_condition(analysis.ends_with_question_mark, value)
                }
                _ => {
                    warn!(
                        "Unknown condition in policy '{}': {}",
                        policy.name, condition
                    );
                    true
                }
            };
            is_match
        })
    }
    fn check_boolean_condition(
        &self,
        analysis_value: bool,
        condition_value: &serde_yaml::Value,
    ) -> bool {
        condition_value.as_bool() == Some(analysis_value)
    }
    fn check_numerical_condition(
        &self,
        analysis_value: f64,
        condition_value: &serde_yaml::Value,
    ) -> bool {
        if let Some(map) = condition_value.as_mapping() {
            for (op, val) in map {
                if let (Some(op_str), Some(limit)) = (op.as_str(), val.as_f64()) {
                    match op_str {
                        "lt" => {
                            if analysis_value >= limit {
                                return false;
                            }
                        }
                        "lte" => {
                            if analysis_value > limit {
                                return false;
                            }
                        }
                        "gt" => {
                            if analysis_value <= limit {
                                return false;
                            }
                        }
                        "gte" => {
                            if analysis_value < limit {
                                return false;
                            }
                        }
                        "eq" => {
                            if analysis_value != limit {
                                return false;
                            }
                        }
                        _ => {
                            warn!("Unknown numerical operator: {}", op_str);
                            return false;
                        }
                    }
                } else {
                    return false;
                }
            }
            true
        } else if let Some(limit) = condition_value.as_f64() {
            analysis_value == limit
        } else {
            false
        }
    }
    fn consolidate_results(
        &self,
        task_results: Vec<TaskOutput>,
        strategy_name: &str,
        start_time: std::time::Instant,
        original_input: &str,
    ) -> Result<UnifiedNLUData, OrchestratorError> {
        let mut unified_extracted_data = ExtractedData::default();
        let mut segments = Vec::new();
        let mut models_used = Vec::new();
        let mut confidence_scores = HashMap::new();
        let mut total_cost_estimate = 0.0;
        let mut primary_intent = "unknown".to_string();
        let topics = Vec::new();
        let sentiment_score = 0.0_f32;
        for result in task_results {
            if !result.success {
                warn!("Task {} failed: {:?}", result.task_name, result.error);
                continue;
            }
            debug!("Consolidating task result: {}", result.task_name);
            models_used.push(result.model_used.clone());
            total_cost_estimate += self.estimate_task_cost(&result);
            match adapter::transform_llm_output(&result.data) {
                Ok((mut extracted_data_part, intent_opt)) => {
                    unified_extracted_data
                        .nodes
                        .append(&mut extracted_data_part.nodes);
                    unified_extracted_data
                        .relationships
                        .append(&mut extracted_data_part.relationships);
                    if let Some(intent) = intent_opt {
                        primary_intent = intent;
                        confidence_scores.insert("intent".to_string(), 1.0);
                    }
                }
                Err(e) => {
                    warn!(
                        "Adapter failed to transform LLM output for task {}: {}",
                        result.task_name, e
                    );
                }
            }
            let task_type = result.task_name.split('_').next().unwrap_or("");
            if task_type == "segmentation" {
                if let Some(seg_data) = result.data.get("segments") {
                    if let Ok(parsed) =
                        serde_json::from_value::<Vec<InputSegment>>(seg_data.clone())
                    {
                        debug!("Successfully parsed {} segments from task.", parsed.len());
                        segments.extend(parsed);
                    } else {
                        warn!("Failed to deserialise segments from: {:?}", seg_data);
                    }
                }
            }
        }
        if segments.is_empty() {
            segments.push(InputSegment {
                text: original_input.to_string(),
                segment_type: SegmentType::Statement {
                    intent: primary_intent,
                },
                priority: 100,
                ..Default::default()
            });
        }
        let execution_time_ms = start_time.elapsed().as_millis() as u64;
        let processing_metadata = ProcessingMetadata {
            strategy_used: strategy_name.to_string(),
            models_used: models_used
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect(),
            execution_time_ms,
            total_cost_estimate,
            confidence_scores,
            topics,
            sentiment_score,
        };
        Ok(UnifiedNLUData {
            segments,
            extracted_data: unified_extracted_data,
            processing_metadata,
        })
    }

    pub fn validate_configuration(&self) -> Result<(), OrchestratorError> {
        let prompt_categories = vec!["extraction", "analysis", "classification"];
        for category in prompt_categories {
            let prompts = self.get_prompts_by_category(category);
            for (name, template) in prompts {
                self.validate_prompt(&name, &template)?;
            }
        }

        let sample_inputs = vec![
            "test input",
            "SELECT * FROM users",
            "<script>alert('xss')</script>",
        ];
        for input in sample_inputs {
            if let Err(e) = self.validate_input_security(input) {
                warn!(
                    "Input security validation failed for sample: {} - {}",
                    input, e
                );
            }
        }

        if let Err(e) = self.check_rate_limit("test_user") {
            warn!("Rate limit check failed: {}", e);
        }

        Ok(())
    }

    fn estimate_task_cost(&self, result: &TaskOutput) -> f64 {
        let base_cost = match result.model_used.as_str() {
            name if name.contains("claude") => 0.01,
            name if name.contains("gpt-4") => 0.03,
            name if name.contains("gpt-3.5") => 0.002,
            _ => 0.005,
        };
        let execution_factor = result.execution_time.as_secs_f64() / 1.0;
        base_cost * execution_factor.max(0.1)
    }
    fn get_prompts_by_category(&self, _category: &str) -> HashMap<String, String> {
        self.config
            .prompts
            .iter()
            .map(|(name, template)| (name.clone(), template.system_message.clone()))
            .collect()
    }
    fn validate_prompt(&self, task_name: &str, template: &str) -> Result<(), OrchestratorError> {
        let required_placeholders = match task_name {
            "entity_extraction" | "temporal_extraction" | "numerical_extraction" => vec!["{input}"],
            "bundled_extraction" => vec!["{input}", "{current_time}"],
            _ => vec!["{input}"],
        };
        for placeholder in required_placeholders {
            if !template.contains(placeholder) {
                return Err(OrchestratorError::new(format!(
                    "Template for {task_name} missing required placeholder: {placeholder}"
                )));
            }
        }
        Ok(())
    }
    pub fn select_model_for_task(
        &self,
        task_name: &str,
        required_capabilities: &[&str],
    ) -> Option<&ModelConfig> {
        if let Some(task_config) = self.config.tasks.get(task_name) {
            if let Some(preferred_model) =
                task_config.get("preferred_model").and_then(|v| v.as_str())
            {
                if let Some(model) = self
                    .config
                    .models
                    .iter()
                    .find(|m| m.name == preferred_model)
                {
                    return Some(model);
                }
            }
        }
        let selection_method = self
            .config
            .selection_strategy
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("capability_based");
        match selection_method {
            "capability_based" => self
                .config
                .models
                .iter()
                .filter(|model| {
                    required_capabilities
                        .iter()
                        .all(|cap| model.capabilities.contains(&cap.to_string()))
                })
                .min_by_key(|model| match model.cost_tier.as_str() {
                    "low" => 1,
                    "medium" => 2,
                    "high" => 3,
                    _ => 2,
                }),
            "cost_optimised" => {
                self.config
                    .models
                    .iter()
                    .min_by_key(|model| match model.cost_tier.as_str() {
                        "low" => 1,
                        "medium" => 2,
                        "high" => 3,
                        _ => 2,
                    })
            }
            "performance_optimised" => self
                .config
                .models
                .iter()
                .max_by_key(|model| model.max_tokens),
            "round_robin" => self.config.models.first(),
            _ => self.config.models.first(),
        }
    }
    pub fn cache_prompt(&mut self, key: String, prompt: String) {
        self.prompt_cache.insert(key, prompt);
    }
    pub fn get_cached_prompt(&self, key: &str) -> Option<&String> {
        self.prompt_cache.get(key)
    }
    pub fn clear_cache(&mut self) {
        self.prompt_cache.clear();
    }
    fn validate_input_security(&self, input: &str) -> Result<(), OrchestratorError> {
        if input.len() > self.config.security.input_sanitisation.max_input_length {
            return Err(OrchestratorError::new(format!(
                "Input exceeds maximum length of {} characters",
                self.config.security.input_sanitisation.max_input_length
            )));
        }
        for pattern in &self.config.security.blocked_operations {
            if input.to_lowercase().contains(&pattern.to_lowercase()) {
                return Err(OrchestratorError::new(format!(
                    "Input contains blocked pattern: {pattern}"
                )));
            }
        }
        if self.config.security.input_sanitisation.enabled {
            let suspicious_indicators = [
                "system:",
                "prompt:",
                "ignore previous",
                "override",
                "jailbreak",
            ];
            let input_lower = input.to_lowercase();
            for indicator in &suspicious_indicators {
                if input_lower.contains(indicator) {
                    warn!(
                        "Potentially suspicious input detected: contains '{}'",
                        indicator
                    );
                }
            }
        }
        Ok(())
    }
    fn check_rate_limit(&self, _user_id: &str) -> Result<(), OrchestratorError> {
        if self.config.security.audit_logging.security_events_enabled {
            debug!("Security features are enabled but rate limiting not yet implemented");
        }
        Ok(())
    }
    pub fn get_available_models(&self) -> &[ModelConfig] {
        &self.config.models
    }
    pub fn get_model(&self, name: &str) -> Option<&ModelConfig> {
        self.config.models.iter().find(|m| m.name == name)
    }
    pub fn get_policies(&self) -> &[ProcessingPolicy] {
        &self.config.policies
    }
    pub async fn health_check(&self) -> Result<HashMap<String, bool>, OrchestratorError> {
        let mut health_status = HashMap::new();
        for (name, adapter) in &self.llm_adapters {
            match timeout(
                std::time::Duration::from_secs(5),
                adapter.process_text("test"),
            )
            .await
            {
                Ok(Ok(_)) => {
                    health_status.insert(format!("adapter_{name}"), true);
                }
                _ => {
                    health_status.insert(format!("adapter_{name}"), false);
                }
            }
        }
        health_status.insert("config_valid".to_string(), !self.config.models.is_empty());
        health_status.insert(
            "prompts_loaded".to_string(),
            !self.config.prompts.is_empty(),
        );
        Ok(health_status)
    }
    pub fn get_default_adapter(&self) -> Option<Arc<dyn LLMAdapter + Send + Sync>> {
        self.llm_adapters.values().next().cloned()
    }
    pub fn get_adapter(&self, name: &str) -> Option<Arc<dyn LLMAdapter + Send + Sync>> {
        self.llm_adapters.get(name).cloned()
    }
    pub async fn process_with_adapter(
        &self,
        text: &str,
        adapter_name: &str,
    ) -> Result<UnifiedNLUData, OrchestratorError> {
        if let Some(_adapter) = self.get_adapter(adapter_name) {
            self.process_input(text).await
        } else {
            Err(OrchestratorError::new(format!(
                "Adapter '{adapter_name}' not found"
            )))
        }
    }
}
async fn timeout<T>(
    duration: std::time::Duration,
    future: impl std::future::Future<Output = T>,
) -> Result<T, tokio::time::error::Elapsed> {
    tokio::time::timeout(duration, future).await
}
pub use data_models::{ExtractedData, InputSegment, SegmentType, TaskOutput, UnifiedNLUData};
