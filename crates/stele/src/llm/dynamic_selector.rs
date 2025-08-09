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

use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tracing::{debug, info, warn};

#[derive(Error, Debug)]
pub enum SelectorError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("YAML parsing error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Lock acquisition failed: {0}")]
    LockFailed(String),
    #[error("No models found with capability: {0}")]
    NoModelsForCapability(String),
    #[error("Intent '{0}' not found in weights configuration")]
    IntentNotFound(String),
    #[error("Bypass failed: Model '{0}' not found in configuration")]
    BypassModelNotFound(String),
    #[error("All candidate models are currently unavailable (circuit is open)")]
    AllModelsUnavailable,
    #[error("System time is before UNIX EPOCH")]
    SystemTimeError(#[from] std::time::SystemTimeError),
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
pub enum OperatingMode {
    #[default]
    Dynamic,
    Static,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub models: Vec<Model>,
    pub selection_strategy: SelectionStrategy,
    pub feedback: Feedback,
    pub providers: HashMap<String, ProviderConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Model {
    pub name: String,
    pub provider: String,
    pub capabilities: Vec<String>,
    pub quality_score: f64,
    pub max_tokens: u32,
    #[serde(default)]
    pub cost_tier: Option<String>,
    #[serde(default)]
    pub speed_tier: Option<String>,
    #[serde(default)]
    pub parallel_limit: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct Cost {
    pub input: f64,
    pub output: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SelectionStrategy {
    pub intent: String,
    pub weights: HashMap<String, Weights>,
    #[serde(default)]
    pub capability_profiles: HashMap<String, CapabilityProfile>,
    #[serde(default = "default_exploration_rate")]
    pub exploration_rate: f64,
    #[serde(default)]
    pub operating_mode: OperatingMode,
    #[serde(default)]
    pub dynamic_scoring: DynamicScoringConfig,
    #[serde(default)]
    pub static_scoring_weights: StaticScoringWeights,
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct DynamicScoringConfig {
    pub cost_normalisation_factor: f64,
    pub speed_ms_normalisation_max: f64,
    pub speed_tps_normalisation_max: f64,
    pub reliability_weight: f64,
    pub load_penalty_factor: f64,
    pub context_scoring: ContextScoringConfig,
}

impl Default for DynamicScoringConfig {
    fn default() -> Self {
        Self {
            cost_normalisation_factor: 0.10,
            speed_ms_normalisation_max: 5000.0,
            speed_tps_normalisation_max: 250.0,
            reliability_weight: 0.2,
            load_penalty_factor: 0.1,
            context_scoring: ContextScoringConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(default)]
pub struct ContextScoringConfig {
    pub base_weight: f64,
    pub preference_boosts: PreferenceBoosts,
    pub use_case_boosts: HashMap<String, f64>,
    pub priority_boosts: HashMap<String, f64>,
    pub domain_boosts: HashMap<String, f64>,
    pub length_boosts: HashMap<String, f64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct PreferenceBoosts {
    pub provider: f64,
    pub model: f64,
}

impl Default for PreferenceBoosts {
    fn default() -> Self {
        Self {
            provider: 0.4,
            model: 0.8,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct StaticScoringWeights {
    pub base_score: f64,
    pub high_availability_bonus: f64,
    pub good_availability_bonus: f64,
    pub availability_threshold_high: u32,
    pub availability_threshold_good: u32,
    pub speed_tier_bonuses: HashMap<String, f64>,
    pub speed_mismatch_penalty: f64,
    pub cost_tier_bonuses: HashMap<String, f64>,
}

impl Default for StaticScoringWeights {
    fn default() -> Self {
        Self {
            base_score: 10.0,
            high_availability_bonus: 2.0,
            good_availability_bonus: 1.0,
            availability_threshold_high: 5,
            availability_threshold_good: 2,
            speed_tier_bonuses: HashMap::from([
                ("Fast".to_string(), 5.0),
                ("Medium".to_string(), 3.0),
                ("Slow".to_string(), 1.0),
            ]),
            speed_mismatch_penalty: -2.0,
            cost_tier_bonuses: HashMap::from([
                ("Free".to_string(), 5.0),
                ("Low".to_string(), 3.0),
                ("Medium".to_string(), 1.0),
                ("High".to_string(), -1.0),
            ]),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct CircuitBreakerConfig {
    pub threshold: u32,
    pub cooldown_seconds: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            threshold: 3,
            cooldown_seconds: 300,
        }
    }
}

fn default_exploration_rate() -> f64 {
    0.05
}

#[derive(Debug, Deserialize, Clone)]
pub struct CapabilityProfile {
    pub weights: Weights,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub struct Weights {
    pub quality: f64,
    pub speed: f64,
    pub cost: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Feedback {
    pub update_on_success: bool,
    pub performance_db_path: String,
    pub learning_rate: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProviderConfig {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PerformanceData {
    pub avg_response_ms: f64,
    pub avg_tokens_per_second: f64,
    pub success_count: u64,
    pub failure_count: u64,
    pub failure_rate: f64,
    pub cost_estimation_multiplier: f64,
    pub active_requests: u32,
    pub circuit_breaker_state: CircuitBreakerState,
    pub last_failure_time: Option<SystemTime>,
    pub consecutive_failures: u32,
}

impl Default for PerformanceData {
    fn default() -> Self {
        Self {
            avg_response_ms: 1500.0,
            avg_tokens_per_second: 50.0,
            success_count: 0,
            failure_count: 0,
            failure_rate: 0.0,
            cost_estimation_multiplier: 1.0,
            active_requests: 0,
            circuit_breaker_state: CircuitBreakerState::Closed,
            last_failure_time: None,
            consecutive_failures: 0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PerformanceDb {
    data: HashMap<String, PerformanceData>,
}

impl PerformanceDb {
    fn load(path: &Path) -> Result<Self, SelectorError> {
        if !path.exists() {
            return Ok(PerformanceDb::default());
        }
        let file = fs::File::open(path)?;
        serde_json::from_reader(file).map_err(Into::into)
    }

    fn save(&self, path: &Path) -> Result<(), SelectorError> {
        let file = fs::File::create(path)?;
        serde_json::to_writer_pretty(file, self).map_err(Into::into)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SelectionRequest {
    pub capability: String,
    pub estimated_input_tokens: u32,
    pub estimated_output_tokens: u32,
    pub preferred_provider: Option<String>,
    pub preferred_model: Option<String>,
    pub available_providers: Option<Vec<String>>,
    pub bypass_model_name: Option<String>,
    pub exploration_rate: Option<f64>,
    pub context_metadata: Option<HashMap<String, String>>,
}

impl SelectionRequest {
    pub fn new(capability: &str) -> Self {
        Self {
            capability: capability.to_string(),
            estimated_input_tokens: 500,
            estimated_output_tokens: 200,
            ..Default::default()
        }
    }
    pub fn bypass_model(mut self, model_name: &str) -> Self {
        self.bypass_model_name = Some(model_name.to_string());
        self
    }
    pub fn exploration_rate(mut self, rate: f64) -> Self {
        self.exploration_rate = Some(rate);
        self
    }
    pub fn with_context(mut self, key: &str, value: &str) -> Self {
        self.context_metadata
            .get_or_insert_with(HashMap::new)
            .insert(key.to_string(), value.to_string());
        self
    }
    pub fn with_available_providers(mut self, providers: Vec<String>) -> Self {
        self.available_providers = Some(providers);
        self
    }
    pub fn with_preferences(mut self, provider: &str, model: &str) -> Self {
        self.preferred_provider = Some(provider.to_string());
        self.preferred_model = Some(model.to_string());
        self
    }
    pub fn with_token_estimates(mut self, input_tokens: u32, output_tokens: u32) -> Self {
        self.estimated_input_tokens = input_tokens;
        self.estimated_output_tokens = output_tokens;
        self
    }
}

#[derive(Debug, Clone)]
pub struct ModelSelection {
    pub model: Model,
    pub score: f64,
    pub reason: String,
}

#[derive(Debug)]
struct ScoredModel<'a> {
    model: &'a Model,
    score: f64,
    reason: String,
}

#[derive(Clone, Debug)]
pub struct DynamicModelSelector {
    config: Arc<Config>,
    performance_db: Arc<RwLock<PerformanceDb>>,
    rng: Arc<Mutex<rand::rngs::StdRng>>,
}

impl DynamicModelSelector {
    pub fn from_config_path(path: &str) -> Result<Self, SelectorError> {
        info!("Loading configuration from: {}", path);
        let config_str = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&config_str)?;
        let performance_db = PerformanceDb::load(Path::new(&config.feedback.performance_db_path))?;
        let seed = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        Ok(Self {
            config: Arc::new(config),
            performance_db: Arc::new(RwLock::new(performance_db)),
            rng: Arc::new(Mutex::new(rand::rngs::StdRng::seed_from_u64(seed))),
        })
    }

    pub fn get_models(&self) -> &[Model] {
        &self.config.models
    }

    pub fn select_model(
        &self,
        request: &SelectionRequest,
    ) -> Result<ModelSelection, SelectorError> {
        if let Some(ref bypass_name) = request.bypass_model_name {
            info!("Bypass active: selecting '{}'", bypass_name);
            let model = self
                .config
                .models
                .iter()
                .find(|m| &m.name == bypass_name)
                .cloned()
                .ok_or_else(|| SelectorError::BypassModelNotFound(bypass_name.clone()))?;
            return Ok(ModelSelection {
                model,
                score: 100.0,
                reason: format!("Bypass for '{bypass_name}'"),
            });
        }

        if self.config.selection_strategy.operating_mode == OperatingMode::Dynamic {
            self.update_all_circuit_breakers()?;
        }

        let mut scored_models = self.score_and_sort_candidates(request)?;
        if scored_models.is_empty() {
            return Err(SelectorError::AllModelsUnavailable);
        }

        let exploration_rate = request
            .exploration_rate
            .unwrap_or(self.config.selection_strategy.exploration_rate);
        let should_explore = self.is_exploration_enabled()
            && self
                .rng
                .lock()
                .map_err(|e| SelectorError::LockFailed(e.to_string()))?
                .gen_bool(exploration_rate);

        let selected = if should_explore && scored_models.len() > 1 {
            let random_index = self
                .rng
                .lock()
                .map_err(|e| SelectorError::LockFailed(e.to_string()))?
                .gen_range(1..scored_models.len());
            info!("EXPLORATION: Picking a sub-optimal model for discovery.");
            scored_models.remove(random_index)
        } else {
            info!("EXPLOITATION: Picking the best model.");
            scored_models.remove(0)
        };

        info!(
            "Selected '{}' with score {:.2}",
            selected.model.name, selected.score
        );
        debug!("Reason: {}", selected.reason);

        if self.config.selection_strategy.operating_mode == OperatingMode::Dynamic {
            self.increment_active_requests(&selected.model.name)?;
        }

        Ok(ModelSelection {
            model: selected.model.clone(),
            score: selected.score,
            reason: selected.reason,
        })
    }

    pub fn select_multiple_models(
        &self,
        request: &SelectionRequest,
        count: usize,
    ) -> Result<Vec<ModelSelection>, SelectorError> {
        if count == 0 {
            return Ok(vec![]);
        }

        if self.config.selection_strategy.operating_mode == OperatingMode::Dynamic {
            self.update_all_circuit_breakers()?;
        }

        let scored_models = self.score_and_sort_candidates(request)?;

        let selected_count = count.min(scored_models.len());
        let mut final_selections = Vec::with_capacity(selected_count);

        for scored in scored_models.into_iter().take(selected_count) {
            if self.config.selection_strategy.operating_mode == OperatingMode::Dynamic {
                self.increment_active_requests(&scored.model.name)?;
            }
            final_selections.push(ModelSelection {
                model: scored.model.clone(),
                score: scored.score,
                reason: scored.reason,
            });
        }

        info!(
            "Selected {} models for multi-model request",
            final_selections.len()
        );
        Ok(final_selections)
    }

    pub fn update_performance(
        &self,
        model_name: &str,
        response_time: Duration,
        tokens_generated: Option<u32>,
        actual_cost: Option<f64>,
        estimated_cost: Option<f64>,
        success: bool,
    ) -> Result<(), SelectorError> {
        if self.config.selection_strategy.operating_mode == OperatingMode::Static {
            return Ok(());
        }
        if !self.config.feedback.update_on_success && success {
            return Ok(());
        }
        let mut perf_db = self
            .performance_db
            .write()
            .map_err(|e| SelectorError::LockFailed(e.to_string()))?;
        let entry = perf_db.data.entry(model_name.to_string()).or_default();

        entry.active_requests = entry.active_requests.saturating_sub(1);
        let alpha = self.config.feedback.learning_rate;
        entry.failure_rate =
            (alpha * if success { 0.0 } else { 1.0 }) + ((1.0 - alpha) * entry.failure_rate);

        if success {
            entry.success_count += 1;
            entry.consecutive_failures = 0;
            entry.avg_response_ms = (alpha * response_time.as_millis() as f64)
                + ((1.0 - alpha) * entry.avg_response_ms);
            if let Some(tokens) = tokens_generated {
                let response_time_secs = response_time.as_secs_f64();
                if response_time_secs > 0.0 {
                    let current_tps = tokens as f64 / response_time_secs;
                    entry.avg_tokens_per_second =
                        (alpha * current_tps) + ((1.0 - alpha) * entry.avg_tokens_per_second);
                }
            }
            if let (Some(actual), Some(estimated)) = (actual_cost, estimated_cost) {
                if estimated > 0.0 {
                    let ratio = actual / estimated;
                    entry.cost_estimation_multiplier =
                        (alpha * ratio) + ((1.0 - alpha) * entry.cost_estimation_multiplier);
                }
            }
            if entry.circuit_breaker_state == CircuitBreakerState::HalfOpen {
                info!(
                    "Circuit for '{}' is now CLOSED after successful recovery.",
                    model_name
                );
                entry.circuit_breaker_state = CircuitBreakerState::Closed;
            }
        } else {
            entry.failure_count += 1;
            entry.consecutive_failures += 1;
            entry.last_failure_time = Some(SystemTime::now());

            if entry.circuit_breaker_state == CircuitBreakerState::HalfOpen {
                warn!(
                    "Circuit for '{}' is now OPEN again after failed recovery.",
                    model_name
                );
                entry.circuit_breaker_state = CircuitBreakerState::Open;
            }
        }
        info!(
            "Performance updated for '{}'. Success: {}, TPS: {:.1}, Active: {}",
            model_name, success, entry.avg_tokens_per_second, entry.active_requests
        );
        self.save_performance_db_locked(&perf_db)?;
        Ok(())
    }

    fn score_and_sort_candidates<'a>(
        &'a self,
        request: &SelectionRequest,
    ) -> Result<Vec<ScoredModel<'a>>, SelectorError> {
        let candidates: Vec<&'a Model> = self
            .config
            .models
            .iter()
            .filter(|m| m.capabilities.contains(&request.capability))
            .filter(|m| {
                request
                    .available_providers
                    .as_ref()
                    .is_none_or(|providers| providers.contains(&m.provider))
            })
            .collect();

        if candidates.is_empty() {
            return Err(SelectorError::NoModelsForCapability(
                request.capability.clone(),
            ));
        }

        let perf_db = self
            .performance_db
            .read()
            .map_err(|e| SelectorError::LockFailed(e.to_string()))?;
        let viable_candidates: Vec<&'a Model> = candidates
            .into_iter()
            .filter(|model| {
                if self.config.selection_strategy.operating_mode == OperatingMode::Dynamic {
                    Self::is_model_available_locked(&perf_db, model.name.as_str())
                } else {
                    true
                }
            })
            .collect();

        if viable_candidates.is_empty() {
            return Ok(vec![]);
        }

        let weights = self.get_weights_for_request(request)?;
        let mut scored_models: Vec<ScoredModel<'a>> = viable_candidates
            .iter()
            .map(|model| {
                let (score, reason) = self.calculate_score(model, request, &perf_db, weights);
                ScoredModel {
                    model,
                    score,
                    reason,
                }
            })
            .collect();

        scored_models.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(scored_models)
    }

    fn calculate_score(
        &self,
        model: &Model,
        request: &SelectionRequest,
        perf_db: &PerformanceDb,
        weights: &Weights,
    ) -> (f64, String) {
        match self.config.selection_strategy.operating_mode {
            OperatingMode::Dynamic => {
                self.calculate_dynamic_score(model, request, perf_db, weights)
            }
            OperatingMode::Static => self.calculate_static_score(model, request),
        }
    }

    fn calculate_dynamic_score(
        &self,
        model: &Model,
        request: &SelectionRequest,
        perf_db: &PerformanceDb,
        weights: &Weights,
    ) -> (f64, String) {
        let perf = perf_db.data.get(&model.name).cloned().unwrap_or_default();
        let scoring_cfg = &self.config.selection_strategy.dynamic_scoring;

        
        let granular_cost_score = if model.provider == "ollama" {
            1.0 
        } else {
            
            let estimated_total_tokens =
                request.estimated_input_tokens + request.estimated_output_tokens;
            if estimated_total_tokens > 0 {
                
                
                match model.name.as_str() {
                    "claude-3-5-haiku-latest" => 0.3,
                    "gpt-4o-mini" => 0.25,
                    "claude-3-5-sonnet-latest" => 0.1,
                    "claude-3-7-sonnet-latest" => 0.05,
                    "gpt-4o" => 0.05,
                    "claude-sonnet-4-20250514" => 0.02,
                    _ => 0.1,
                }
            } else {
                0.1 
            }
        };
        let live_speed_score = {
            let norm_ms =
                1.0 - (perf.avg_response_ms / scoring_cfg.speed_ms_normalisation_max).min(1.0);
            let norm_tps =
                (perf.avg_tokens_per_second / scoring_cfg.speed_tps_normalisation_max).min(1.0);
            (norm_ms + norm_tps) / 2.0
        };
        let reliability_score = 1.0 - perf.failure_rate;
        let context_score_boost = self.calculate_context_score(model, request);
        let load_penalty = perf.active_requests as f64 * scoring_cfg.load_penalty_factor;

        let final_score = (model.quality_score * weights.quality)
            + (granular_cost_score * weights.cost)
            + (live_speed_score * weights.speed)
            + (reliability_score * scoring_cfg.reliability_weight)
            + (context_score_boost * scoring_cfg.context_scoring.base_weight)
            - load_penalty;

        let reason = format!(
            "Dynamic(Q:{:.2} C:{:.2} S:{:.2}) + Reliability({:.2}) + Context({:.2}) - Load({:.2})",
            model.quality_score * weights.quality,
            granular_cost_score * weights.cost,
            live_speed_score * weights.speed,
            reliability_score * scoring_cfg.reliability_weight,
            context_score_boost * scoring_cfg.context_scoring.base_weight,
            load_penalty
        );
        (final_score, reason)
    }

    fn calculate_static_score(&self, model: &Model, _request: &SelectionRequest) -> (f64, String) {
        let weights = &self.config.selection_strategy.static_scoring_weights;
        let mut score = weights.base_score;
        let mut reasons = vec!["base score".to_string()];

        if let Some(model_speed_str) = &model.speed_tier {
            if let Some(bonus) = weights.speed_tier_bonuses.get(model_speed_str) {
                score += bonus;
                reasons.push(format!("speed tier ({model_speed_str})"));
            }
        }

        if let Some(cost_tier_str) = &model.cost_tier {
            if let Some(bonus) = weights.cost_tier_bonuses.get(cost_tier_str) {
                score += bonus;
                reasons.push(format!("cost tier ({cost_tier_str})"));
            }
        }

        if let Some(parallel_limit) = model.parallel_limit {
            if parallel_limit > weights.availability_threshold_high {
                score += weights.high_availability_bonus;
                reasons.push("high availability".to_string());
            } else if parallel_limit > weights.availability_threshold_good {
                score += weights.good_availability_bonus;
                reasons.push("good availability".to_string());
            }
        }

        (score, reasons.join(", "))
    }

    fn calculate_context_score(&self, model: &Model, request: &SelectionRequest) -> f64 {
        let mut score_boost = 0.0;
        let context_cfg = &self
            .config
            .selection_strategy
            .dynamic_scoring
            .context_scoring;

        if let Some(metadata) = &request.context_metadata {
            if let Some(use_case) = metadata.get("use_case") {
                score_boost += context_cfg
                    .use_case_boosts
                    .get(use_case)
                    .cloned()
                    .unwrap_or(0.0);
            }
            if let Some(priority) = metadata.get("priority") {
                score_boost += context_cfg
                    .priority_boosts
                    .get(priority)
                    .cloned()
                    .unwrap_or(0.0);
            }
            if let Some(domain) = metadata.get("domain") {
                score_boost += context_cfg
                    .domain_boosts
                    .get(domain)
                    .cloned()
                    .unwrap_or(0.0);
            }
            if let Some(length) = metadata.get("response_length") {
                score_boost += context_cfg
                    .length_boosts
                    .get(length)
                    .cloned()
                    .unwrap_or(0.0);
            }
        }

        if let Some(pref_provider) = &request.preferred_provider {
            if &model.provider == pref_provider {
                score_boost += context_cfg.preference_boosts.provider;
            }
        }
        if let Some(pref_model) = &request.preferred_model {
            if &model.name == pref_model {
                score_boost += context_cfg.preference_boosts.model;
            }
        }
        score_boost
    }

    fn update_all_circuit_breakers(&self) -> Result<(), SelectorError> {
        let mut perf_db = self
            .performance_db
            .write()
            .map_err(|e| SelectorError::LockFailed(e.to_string()))?;
        let cb_config = &self.config.selection_strategy.circuit_breaker;
        let now = SystemTime::now();
        for (name, perf) in perf_db.data.iter_mut() {
            if perf.circuit_breaker_state == CircuitBreakerState::Open {
                if let Some(last_failure) = perf.last_failure_time {
                    if now
                        .duration_since(last_failure)
                        .is_ok_and(|d| d.as_secs() >= cb_config.cooldown_seconds)
                    {
                        info!(
                            "Circuit for '{}' is now HALF-OPEN. Attempting recovery.",
                            name
                        );
                        perf.circuit_breaker_state = CircuitBreakerState::HalfOpen;
                    }
                }
            } else if perf.circuit_breaker_state == CircuitBreakerState::Closed
                && perf.consecutive_failures >= cb_config.threshold
            {
                warn!(
                    "Circuit for '{}' is now OPEN. Failures: {}",
                    name, perf.consecutive_failures
                );
                perf.circuit_breaker_state = CircuitBreakerState::Open;
                perf.last_failure_time = Some(now);
            }
        }
        Ok(())
    }

    fn is_model_available_locked(perf_db: &PerformanceDb, model_name: &str) -> bool {
        perf_db
            .data
            .get(model_name)
            .is_none_or(|perf| perf.circuit_breaker_state != CircuitBreakerState::Open)
    }

    fn is_exploration_enabled(&self) -> bool {
        self.config.selection_strategy.operating_mode == OperatingMode::Dynamic
            && self.config.selection_strategy.exploration_rate > 0.0
    }

    fn increment_active_requests(&self, model_name: &str) -> Result<(), SelectorError> {
        self.performance_db
            .write()
            .map_err(|e| SelectorError::LockFailed(e.to_string()))?
            .data
            .entry(model_name.to_string())
            .or_default()
            .active_requests += 1;
        Ok(())
    }

    fn get_weights_for_request<'a>(
        &'a self,
        request: &SelectionRequest,
    ) -> Result<&'a Weights, SelectorError> {
        if let Some(profile) = self
            .config
            .selection_strategy
            .capability_profiles
            .get(&request.capability)
        {
            Ok(&profile.weights)
        } else {
            self.config
                .selection_strategy
                .weights
                .get(&self.config.selection_strategy.intent)
                .ok_or_else(|| {
                    SelectorError::IntentNotFound(self.config.selection_strategy.intent.clone())
                })
        }
    }

    fn save_performance_db_locked(&self, perf_db: &PerformanceDb) -> Result<(), SelectorError> {
        perf_db
            .save(Path::new(&self.config.feedback.performance_db_path))
            .map_err(|e| {
                warn!("Failed to save performance DB: {}", e);
                e
            })
    }

    pub fn get_model_performance(
        &self,
        model_name: &str,
    ) -> Result<Option<PerformanceData>, SelectorError> {
        Ok(self
            .performance_db
            .read()
            .map_err(|e| SelectorError::LockFailed(e.to_string()))?
            .data
            .get(model_name)
            .cloned())
    }

    pub fn get_models_for_capability(&self, capability: &str) -> Vec<Model> {
        self.config
            .models
            .iter()
            .filter(|m| m.capabilities.contains(&capability.to_string()))
            .cloned()
            .collect()
    }

    pub fn get_system_health(&self) -> Result<HashMap<String, String>, SelectorError> {
        let perf_db = self
            .performance_db
            .read()
            .map_err(|e| SelectorError::LockFailed(e.to_string()))?;
        let mut health = HashMap::new();
        let total_models = self.config.models.len();
        let mut available_models = 0;
        let mut circuit_open_models = 0;

        for model in &self.config.models {
            if let Some(perf) = perf_db.data.get(&model.name) {
                if self.config.selection_strategy.operating_mode == OperatingMode::Dynamic
                    && perf.circuit_breaker_state == CircuitBreakerState::Open
                {
                    circuit_open_models += 1;
                } else {
                    available_models += 1;
                }
            } else {
                available_models += 1;
            }
        }
        health.insert("total_models".to_string(), total_models.to_string());
        health.insert("available_models".to_string(), available_models.to_string());
        health.insert(
            "circuit_open_models".to_string(),
            circuit_open_models.to_string(),
        );
        let health_pct = if total_models > 0 {
            (available_models as f64 / total_models as f64) * 100.0
        } else {
            100.0
        };
        health.insert(
            "system_health_percentage".to_string(),
            format!("{health_pct:.1}"),
        );
        Ok(health)
    }

    pub fn reset_circuit_breaker(&self, model_name: &str) -> Result<(), SelectorError> {
        let mut perf_db = self
            .performance_db
            .write()
            .map_err(|e| SelectorError::LockFailed(e.to_string()))?;
        if let Some(perf) = perf_db.data.get_mut(model_name) {
            perf.circuit_breaker_state = CircuitBreakerState::Closed;
            perf.consecutive_failures = 0;
            perf.last_failure_time = None;
            info!("Circuit breaker reset for model '{}'", model_name);
            self.save_performance_db_locked(&perf_db)?;
            Ok(())
        } else {
            Err(SelectorError::BypassModelNotFound(model_name.to_string()))
        }
    }
}
