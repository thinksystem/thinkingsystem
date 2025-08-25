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
use crate::{
    database::{data_interpreter::DatabaseInterface, dynamic_storage::DynamicStorage},
    nlu::orchestrator::data_models::{
        Entity, ExtractedData, InputSegment, KnowledgeNode, ProcessingMetadata, SegmentType,
        UnifiedNLUData,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
const DATA_SCRIBE_STATES: usize = 16;
const DATA_SCRIBE_ACTIONS: usize = 2;
use crate::nlu::orchestrator::NLUOrchestrator;
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessingMetrics {
    pub total_requests: u64,
    pub successful_extractions: u64,
    pub failed_extractions: u64,
    pub average_processing_time_ms: f64,
    pub total_cost: f64,
}
#[derive(Clone)]
pub struct DataScribe {
    pub id: String,
    base: BaseScribe,
    cognitive_core: QLearningCore,
    nlu_orchestrator: Arc<RwLock<NLUOrchestrator>>,
    storage: Arc<RwLock<Option<Arc<DynamicStorage>>>>,
    db_interface: Arc<RwLock<Option<DatabaseInterface>>>,
    metrics: Arc<RwLock<ProcessingMetrics>>,
    last_state_action: Option<(usize, usize)>,
}
impl DataScribe {
    pub async fn new(id: String, config_path: &str) -> Result<Self, String> {
        
        let orchestrator = match NLUOrchestrator::new(config_path).await {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!(error=?e, "Primary NLUOrchestrator::new failed; attempting unified adapter fallback");
                
                match crate::llm::unified_adapter::UnifiedLLMAdapter::with_defaults().await {
                    Ok(u) => match NLUOrchestrator::with_unified_adapter(config_path, Arc::new(u)).await {
                        Ok(o2) => o2,
                        Err(e2) => return Err(format!("DataScribe orchestrator init error (primary: {e:?}; unified fallback: {e2:?})")),
                    },
                    Err(eu) => return Err(format!("DataScribe orchestrator init error (primary: {e:?}; unified adapter create failed: {eu})")),
                }
            }
        };
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
            storage: Arc::new(RwLock::new(None)),
            db_interface: Arc::new(RwLock::new(None)),
            metrics: Arc::new(RwLock::new(ProcessingMetrics::default())),
            last_state_action: None,
        })
    }
    
    pub async fn metrics_snapshot(&self) -> ProcessingMetrics {
        self.metrics.read().await.clone()
    }
    #[tracing::instrument(skip(self, context), fields(id = %self.id))]
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
        let nlu_result: Result<Value, String> = match action {
            0 => {
                tracing::info!(target: "stele::data_scribe", text_len = text.len(), "NLU orchestrator starting");
                let r = orchestrator
                    .process_input(text)
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|u| serde_json::to_value(&u).map_err(|e| e.to_string()));
                tracing::info!(target: "stele::data_scribe", "NLU orchestrator finished");
                r
            }
            1 => self.run_fast_fallback(text).await,
            _ => unreachable!(),
        };
        self.update_metrics(&nlu_result, start_time.elapsed()).await;
        nlu_result
    }
    #[tracing::instrument(skip(self, data), fields(id = %self.id))]
    pub async fn store_extracted_data(&self, data: &Value) -> Result<Value, String> {
        tracing::info!(target: "stele::data_scribe", "Persisting NLU result to database");

        let unified: UnifiedNLUData = match serde_json::from_value::<UnifiedNLUData>(data.clone()) {
            Ok(u) => u,
            Err(_) => self
                .convert_fallback_to_unified(data)
                .ok_or_else(|| "Unsupported data shape for persistence".to_string())?,
        };

        {
            let mut iface_guard = self.db_interface.write().await;
            if iface_guard.is_none() {
                let iface = DatabaseInterface::new()
                    .await
                    .map_err(|e| format!("Failed to initialise database interface: {e}"))?;
                *iface_guard = Some(iface);
            }
        }

        let storage: Arc<DynamicStorage> = {
            let mut storage_guard = self.storage.write().await;
            if storage_guard.is_none() {
                let client = {
                    let iface_guard = self.db_interface.read().await;
                    let iface = iface_guard
                        .as_ref()
                        .ok_or("Database interface not initialised")?;
                    iface.get_client()
                };
                *storage_guard = Some(Arc::new(DynamicStorage::new(client)));
            }
            storage_guard
                .as_ref()
                .cloned()
                .ok_or("Dynamic storage not initialised")?
        };

        let raw_text = unified.get_raw_text();

        let (user_id, channel) = (
            data.get("metadata")
                .and_then(|m| m.get("user_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("anonymous"),
            data.get("metadata")
                .and_then(|m| m.get("channel"))
                .and_then(|v| v.as_str())
                .unwrap_or("default"),
        );

        let result = storage
            .store_llm_output(user_id, channel, &raw_text, &unified)
            .await?;
        tracing::info!(target: "stele::data_scribe", "Persistence complete");
        Ok(result)
    }
    #[tracing::instrument(skip(self, context), fields(id = %self.id, user_id = %user_id, channel = %channel))]
    pub async fn process_and_store(
        &mut self,
        context: &Value,
        user_id: &str,
        channel: &str,
    ) -> Result<Value, String> {
        let processed = self.process_data(context).await?;

        let mut enriched = processed.clone();
        if let Value::Object(ref mut m) = enriched {
            let mut meta = serde_json::Map::new();
            meta.insert("user_id".to_string(), Value::String(user_id.to_string()));
            meta.insert("channel".to_string(), Value::String(channel.to_string()));
            m.insert("metadata".to_string(), Value::Object(meta));
        }
        self.store_extracted_data(&enriched).await
    }
    fn convert_fallback_to_unified(&self, data: &Value) -> Option<UnifiedNLUData> {
        if data.get("method")?.as_str()? != "fast_fallback" {
            return None;
        }
        let keywords = data
            .get("keywords")?
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>();
        if keywords.is_empty() {
            return None;
        }
        let text = keywords.join(" ");
        let segment = InputSegment::new(
            text.clone(),
            SegmentType::Statement {
                intent: "fallback_extraction".to_string(),
            },
        );
        let nodes: Vec<KnowledgeNode> = keywords
            .iter()
            .enumerate()
            .map(|(i, k)| {
                KnowledgeNode::Entity(Entity {
                    temp_id: format!("kw_{i}"),
                    name: k.to_string(),
                    entity_type: "Keyword".to_string(),
                    confidence: 0.3,
                    metadata: None,
                })
            })
            .collect();
        let extracted = ExtractedData {
            nodes,
            relationships: Vec::new(),
        };
        let cost = data.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let processing_metadata = ProcessingMetadata {
            strategy_used: "fast_fallback".to_string(),
            total_cost_estimate: cost,
            ..Default::default()
        };
        let unified = UnifiedNLUData {
            segments: vec![segment],
            extracted_data: extracted,
            processing_metadata,
        };
        Some(unified)
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

impl std::fmt::Debug for DataScribe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataScribe").field("id", &self.id).finish()
    }
}
