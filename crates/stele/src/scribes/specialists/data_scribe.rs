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

use crate::scribes::base_scribe::{
    BaseScribe, CostPerRequest, DataHandling, PerformanceMetrics, ProviderMetadata,
};
use crate::scribes::core::q_learning_core::QLearningCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
const DATA_SCRIBE_STATES: usize = 16;
const DATA_SCRIBE_ACTIONS: usize = 2;
#[derive(Debug, Clone)]
pub struct NLUOrchestrator;
impl NLUOrchestrator {
    pub async fn new(_config_path: &str) -> Result<Self, String> {
        Ok(Self)
    }
    pub async fn process_input(&self, _text: &str) -> Result<Value, String> {
        Ok(
            json!({ "method": "full_pipeline", "entities": ["STELE"], "cost": 0.05, "latency_ms": 250 }),
        )
    }
}
#[derive(Debug, Clone)]
pub struct DatabaseInterface;
impl Default for DatabaseInterface {
    fn default() -> Self {
        Self::new()
    }
}

impl DatabaseInterface {
    pub fn new() -> Self {
        Self
    }
    pub async fn store_data(&self, _data: &Value) -> Result<(), String> {
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessingMetrics {
    pub total_requests: u64,
    pub successful_extractions: u64,
    pub failed_extractions: u64,
    pub average_processing_time_ms: f64,
    pub total_cost: f64,
}
#[derive(Clone, Debug)]
pub struct DataScribe {
    pub id: String,
    base: BaseScribe,
    cognitive_core: QLearningCore,
    nlu_orchestrator: Arc<RwLock<NLUOrchestrator>>,
    database: Arc<RwLock<DatabaseInterface>>,
    metrics: Arc<RwLock<ProcessingMetrics>>,
    last_state_action: Option<(usize, usize)>,
}
impl DataScribe {
    pub async fn new(id: String, config_path: &str) -> Result<Self, String> {
        let orchestrator = NLUOrchestrator::new(config_path).await?;
        Ok(Self {
            id,
            base: BaseScribe::new(DATA_SCRIBE_STATES, DATA_SCRIBE_ACTIONS),
            cognitive_core: QLearningCore::new(
                DATA_SCRIBE_STATES,
                DATA_SCRIBE_ACTIONS,
                0.95,
                0.1,
                0.1,
                16,
            ),
            nlu_orchestrator: Arc::new(RwLock::new(orchestrator)),
            database: Arc::new(RwLock::new(DatabaseInterface::new())),
            metrics: Arc::new(RwLock::new(ProcessingMetrics::default())),
            last_state_action: None,
        })
    }
    pub async fn process_data(&mut self, context: &Value) -> Result<Value, String> {
        let _current_state = self.base.state();

        let text = context["text"].as_str().unwrap_or("");
        let urgency = context["urgency"].as_f64().unwrap_or(0.0) as f32;
        let state = self.calculate_state(text, urgency);

        self.base.set_state(state);

        let valid_actions: Vec<usize> = (0..DATA_SCRIBE_ACTIONS).collect();
        let action = self.cognitive_core.choose_action(state, &valid_actions);
        self.last_state_action = Some((state, action));
        let start_time = std::time::Instant::now();
        let orchestrator = self.nlu_orchestrator.read().await;
        let nlu_result = match action {
            0 => orchestrator.process_input(text).await,
            1 => self.run_fast_fallback(text).await,
            _ => unreachable!(),
        };
        self.update_metrics(&nlu_result, start_time.elapsed()).await;
        nlu_result
    }
    pub async fn store_extracted_data(&self, data: &Value) -> Result<Value, String> {
        self.database.read().await.store_data(data).await?;
        Ok(json!({"status": "stored"}))
    }
    pub fn record_reward(&mut self, reward: f32) {
        if let Some((last_state, last_action)) = self.last_state_action {
            let next_state = self.calculate_state("", 0.0);
            self.cognitive_core
                .add_experience(last_state, last_action, reward, next_state);
            self.cognitive_core.update_q_values();
        }
        self.last_state_action = None;
    }
    pub fn modulate_core(&mut self, aggressiveness: f32) {
        let base_exploration = 0.1;
        let modulated_exploration = base_exploration + (aggressiveness - 0.5) * 0.15;
        self.cognitive_core
            .set_modulated_exploration_rate(modulated_exploration);
    }
    fn calculate_state(&self, text: &str, urgency: f32) -> usize {
        let metrics = self.metrics.try_read().unwrap();
        let complexity_bin = if text.len() < 100 { 0 } else { 1 };
        let urgency_bin = if urgency < 0.5 { 0 } else { 1 };
        let cost_bin = if metrics.total_cost / (metrics.total_requests.max(1) as f64) < 0.02 {
            0
        } else {
            1
        };
        let latency_bin = if metrics.average_processing_time_ms < 150.0 {
            0
        } else {
            1
        };
        complexity_bin * 8 + urgency_bin * 4 + cost_bin * 2 + latency_bin
    }
    async fn update_metrics(&self, result: &Result<Value, String>, duration: std::time::Duration) {
        let mut metrics = self.metrics.write().await;
        metrics.total_requests += 1;
        if let Ok(data) = result {
            metrics.successful_extractions += 1;
            metrics.total_cost += data["cost"].as_f64().unwrap_or(0.0);
            let total_reqs = metrics.successful_extractions as f64;
            metrics.average_processing_time_ms = (metrics.average_processing_time_ms
                * (total_reqs - 1.0)
                + duration.as_millis() as f64)
                / total_reqs;
        } else {
            metrics.failed_extractions += 1;
        }
    }
    async fn run_fast_fallback(&self, text: &str) -> Result<Value, String> {
        Ok(json!({
            "method": "fast_fallback",
            "keywords": text.split_whitespace().take(5).collect::<Vec<&str>>(),
            "cost": 0.001,
            "latency_ms": 30,
        }))
    }
    pub async fn get_performance_summary(&self) -> HashMap<String, Value> {
        let metrics = self.metrics.read().await;
        let mut summary = HashMap::new();
        let success_rate = if metrics.total_requests > 0 {
            metrics.successful_extractions as f64 / metrics.total_requests as f64
        } else {
            0.0
        };
        summary.insert("total_requests".to_string(), json!(metrics.total_requests));
        summary.insert("success_rate".to_string(), json!(success_rate));
        summary.insert(
            "average_processing_time_ms".to_string(),
            json!(metrics.average_processing_time_ms),
        );
        summary.insert("total_cost".to_string(), json!(metrics.total_cost));
        summary
    }
    pub fn get_provider_metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            name: "DataScribe".to_string(),
            provider_type: vec!["Internal".to_string(), "Processing".to_string()],
            supported_content_types: vec!["text/plain".to_string()],
            cost_per_request: CostPerRequest {
                amount: 0.0,
                currency: "USD".to_string(),
            },
            copyright_ownership: "Client".to_string(),
            data_reproduction_rights: "Limited".to_string(),
            data_handling: DataHandling {
                storage_duration: "Ephemeral".to_string(),
                usage_policy: "Internal processing only".to_string(),
            },
            performance_metrics: PerformanceMetrics {
                accuracy: 0.95,
                response_time: 150.0,
                speed: "Dynamic".to_string(),
            },
        }
    }
}
