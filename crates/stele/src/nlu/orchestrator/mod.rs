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
use chrono::{Datelike, Duration, Timelike, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use steel::messaging::insight::ner_analysis::{NerAnalyser, NerAnalysisResult};
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
                "ollama" => Arc::new(Self::create_ollama_adapter(model)?),
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
    fn create_ollama_adapter(model: &ModelConfig) -> Result<CustomLLMAdapter, OrchestratorError> {
        CustomLLMAdapter::ollama(model.name.clone())
            .map_err(|e| OrchestratorError::new(format!("Failed to create ollama adapter: {e}")))
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

        
        let ner_enabled = std::env::var("STELE_ENABLE_NATIVE_NER")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if ner_enabled {
            match Self::run_native_ner(original_input) {
                Ok(ner) => {
                    if ner.overall_ner_score > 0.0 {
                        let mut added_entities = 0usize;
                        for e in ner.entities {
                            let label = e.label.to_lowercase();
                            if (label == "person" || label == "location") && e.confidence >= 0.6 {
                                if !Self::has_entity(&unified_extracted_data, &e.text, &label) {
                                    unified_extracted_data.nodes.push(KnowledgeNode::Entity(
                                        Entity {
                                            temp_id: format!("ner_{label}_{added_entities}"),
                                            name: e.text.clone(),
                                            entity_type: label.clone(),
                                            confidence: e.confidence as f32,
                                            metadata: Some(serde_json::json!({
                                                "source": "native_ner",
                                                "risk_score": e.risk_score
                                            })),
                                        },
                                    ));
                                    added_entities += 1;
                                }
                            } else if label == "date" && e.confidence >= 0.5 {
                                
                                let resolved = Self::resolve_temporal_text(&e.text);
                                unified_extracted_data.nodes.push(KnowledgeNode::Temporal(
                                    TemporalMarker {
                                        temp_id: format!("ner_temporal_{added_entities}"),
                                        date_text: e.text.clone(),
                                        resolved_date: resolved,
                                        confidence: e.confidence as f32,
                                        metadata: Some(serde_json::json!({
                                            "source": "native_ner"
                                        })),
                                    },
                                ));
                                added_entities += 1;
                            }
                        }
                        if added_entities > 0 {
                            debug!(added = added_entities, "Native NER enriched extracted data");
                        }
                    }
                }
                Err(e) => {
                    warn!("Native NER unavailable or failed: {}", e);
                }
            }
        } else {
            debug!("Native NER enrichment disabled");
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
        
        let mut unified = UnifiedNLUData {
            segments,
            extracted_data: unified_extracted_data,
            processing_metadata,
        };
        if let Err(e) =
            futures::executor::block_on(self.resolve_temporals_llm(&mut unified, original_input))
        {
            warn!("Temporal resolution step failed: {}", e);
        }
        Ok(unified)
    }

    async fn resolve_temporals_llm(
        &self,
        data: &mut UnifiedNLUData,
        original_input: &str,
    ) -> Result<(), OrchestratorError> {
        use chrono::Utc;
        let now_iso = Utc::now().to_rfc3339();
        let adapter = match self.get_default_adapter() {
            Some(a) => a,
            None => return Ok(()),
        };
        
        let per_call_timeout = std::time::Duration::from_millis(1500);
        for node in data.extracted_data.nodes.iter_mut() {
            if let data_models::KnowledgeNode::Temporal(t) = node {
                if t.resolved_date.is_some() || t.date_text.trim().is_empty() {
                    continue;
                }
                let prompt = format!(
                    concat!(
                        "You are a temporal normalizer.\n",
                        "- Current UTC datetime: {now}\n",
                        "- Original input: \"{input}\"\n",
                        "- Temporal phrase to normalize: \"{phrase}\"\n",
                        "Return a strict JSON object with an ISO8601 UTC datetime for the most likely next occurrence.\n",
                        "If only a day of week is given (e.g., 'tue', 'wed'), choose the next such day from now.\n",
                        "If time missing, default to 09:00:00Z. If ambiguous, prefer the future.\n",
                        "Respond ONLY as JSON: {{\"iso\": \"YYYY-MM-DDTHH:MM:SSZ\"}} or {{\"iso\": null}} if not resolvable."
                    ),
                    now = now_iso,
                    input = original_input,
                    phrase = t.date_text
                );
                let fut = adapter.process_text(&prompt);
                match tokio::time::timeout(per_call_timeout, fut).await {
                    Ok(Ok(resp_str)) => {
                        
                        let json_str =
                            Self::extract_json_inline(&resp_str).unwrap_or(resp_str.clone());
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                            if let Some(iso) = val.get("iso").and_then(|v| v.as_str()) {
                                if !iso.is_empty() {
                                    t.resolved_date = Some(iso.to_string());
                                }
                            }
                        }
                    }
                    Ok(Err(e)) => warn!("LLM temporal normalization failed: {}", e),
                    Err(_) => warn!("LLM temporal normalization timed out"),
                }
            }
        }
        Ok(())
    }
}

impl NLUOrchestrator {
    #[allow(dead_code)]
    fn fallback_normalize_weekday(phrase: &str) -> Option<String> {
        use chrono::{Datelike, TimeZone, Utc, Weekday};
        let lower = phrase.trim().to_lowercase();
        let map = [
            ("mon", Weekday::Mon),
            ("monday", Weekday::Mon),
            ("tue", Weekday::Tue),
            ("tues", Weekday::Tue),
            ("tuesday", Weekday::Tue),
            ("wed", Weekday::Wed),
            ("wednesday", Weekday::Wed),
            ("thu", Weekday::Thu),
            ("thur", Weekday::Thu),
            ("thurs", Weekday::Thu),
            ("thursday", Weekday::Thu),
            ("fri", Weekday::Fri),
            ("friday", Weekday::Fri),
            ("sat", Weekday::Sat),
            ("saturday", Weekday::Sat),
            ("sun", Weekday::Sun),
            ("sunday", Weekday::Sun),
        ];
        let target = match map.iter().find(|(k, _)| lower.starts_with(*k)) {
            Some((_, wd)) => *wd,
            None => return None,
        };
        let now = Utc::now();
        let current_wd = now.weekday();
        let mut days_ahead = (target.num_days_from_monday() as i32
            - current_wd.num_days_from_monday() as i32)
            .rem_euclid(7);
        if days_ahead == 0 {
            days_ahead = 7;
        }
        let next = now + chrono::Duration::days(days_ahead as i64);
        let dt = Utc
            .with_ymd_and_hms(next.year(), next.month(), next.day(), 9, 0, 0)
            .single()?;
        Some(dt.to_rfc3339())
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

impl NLUOrchestrator {
    fn extract_json_inline(text: &str) -> Option<String> {
        if let Some(start) = text.find("```json") {
            let content_start = start + 7;
            if let Some(end) = text[content_start..].find("```") {
                let json_content = &text[content_start..content_start + end];
                return Some(json_content.trim().to_string());
            }
        }
        let trimmed = text.trim();
        if (trimmed.starts_with('{') && trimmed.ends_with('}'))
            || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        {
            return Some(trimmed.to_string());
        }
        None
    }
    fn run_native_ner(text: &str) -> anyhow::Result<NerAnalysisResult> {
        let mut analyser = NerAnalyser::default();
        analyser.analyse_text(text)
    }

    fn has_entity(data: &ExtractedData, name: &str, entity_type: &str) -> bool {
        let nl = name.to_lowercase();
        let tl = entity_type.to_lowercase();
        data.entities()
            .any(|e| e.name.to_lowercase() == nl && e.entity_type.to_lowercase() == tl)
    }

    
    fn resolve_temporal_text(text: &str) -> Option<String> {
        let now = Utc::now();
        let lower = text.trim().to_lowercase();

        
        let mut hour: Option<u32> = None;
        let mut minute: u32 = 0;
        
        let time_re = regex::Regex::new(r"(?i)\b(\d{1,2})(?::(\d{2}))?\s*(am|pm)?\b").ok();
        if let Some(re) = &time_re {
            if let Some(caps) = re.captures(&lower) {
                if let Some(h) = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok()) {
                    hour = Some(h);
                }
                if let Some(m) = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok()) {
                    minute = m;
                }
                if let Some(ap) = caps.get(3).map(|m| m.as_str().to_lowercase()) {
                    if let Some(h) = hour.as_mut() {
                        if ap == "pm" && *h < 12 {
                            *h += 12;
                        }
                        if ap == "am" && *h == 12 {
                            *h = 0;
                        }
                    }
                }
            }
        }

        
        let mut date = if lower.contains("tomorrow") {
            now + Duration::days(1)
        } else if lower.contains("yesterday") {
            now - Duration::days(1)
        } else if lower.contains("today") {
            now
        } else {
            
            let weekdays = [
                ("monday", chrono::Weekday::Mon),
                ("tuesday", chrono::Weekday::Tue),
                ("wednesday", chrono::Weekday::Wed),
                ("thursday", chrono::Weekday::Thu),
                ("friday", chrono::Weekday::Fri),
                ("saturday", chrono::Weekday::Sat),
                ("sunday", chrono::Weekday::Sun),
            ];
            let mut chosen: Option<chrono::Weekday> = None;
            for (name, wd) in weekdays.iter() {
                if lower.contains(&format!("next {name}")) || lower.contains(*name) {
                    chosen = Some(*wd);
                    break;
                }
            }
            if let Some(target) = chosen {
                let mut d = now.date_naive();
                let mut add_days = (7 + target.num_days_from_monday() as i64
                    - d.weekday().num_days_from_monday() as i64)
                    % 7;
                if add_days == 0 {
                    add_days = 7;
                }
                d = d + chrono::naive::Days::new(add_days as u64);
                d.and_time(now.time()).and_utc()
            } else {
                now
            }
        };

        
        if let Some(h) = hour {
            date = date
                .with_hour(h)
                .and_then(|d| d.with_minute(minute))
                .unwrap_or(date);
            
            if date < now && !lower.contains("yesterday") {
                date += Duration::days(1);
            }
        } else {
            date = date
                .with_hour(9)
                .and_then(|d| d.with_minute(0))
                .unwrap_or(date);
        }

        Some(date.to_rfc3339())
    }
}
