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

use super::memory_components::Experience;
use crate::nlu::orchestrator::data_models::Action;
use crate::scribes::{EmotionalState, InteractionOutcome};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventInfo {
    pub id: String,
    pub name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub metadata: HashMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RichContext {
    pub action: Option<Action>,
    pub event: Option<EventInfo>,
    pub timestamp: DateTime<Utc>,
    pub outcome: InteractionOutcome,
    pub semantic_embedding: Vec<f32>,
    pub metadata: HashMap<String, serde_json::Value>,
}
#[derive(Debug)]
pub struct MemoryIndex {
    temporal_index: Arc<RwLock<BTreeMap<DateTime<Utc>, Vec<usize>>>>,
    action_index: Arc<RwLock<HashMap<String, HashSet<usize>>>>,
    emotional_index: Arc<RwLock<HashMap<EmotionalStateKey, Vec<usize>>>>,
    outcome_index: Arc<RwLock<HashMap<bool, HashSet<usize>>>>,
    pattern_cache: Arc<RwLock<HashMap<Vec<Action>, PatternStats>>>,
}
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct EmotionalStateKey {
    valence_bucket: i8,
    arousal_bucket: i8,
}
impl From<&EmotionalState> for EmotionalStateKey {
    fn from(state: &EmotionalState) -> Self {
        Self {
            valence_bucket: (state.valence * 10.0).round() as i8,
            arousal_bucket: (state.arousal * 10.0).round() as i8,
        }
    }
}
#[derive(Debug, Clone)]
pub struct PatternStats {
    frequency: u64,
    mean_reward: f32,
    variance: f32,
    last_updated: DateTime<Utc>,
}

impl PatternStats {
    pub fn new() -> Self {
        Self {
            frequency: 0,
            mean_reward: 0.0,
            variance: 0.0,
            last_updated: Utc::now(),
        }
    }

    pub fn update_with_reward(&mut self, reward: f32) {
        let old_mean = self.mean_reward;
        self.frequency += 1;

        self.mean_reward += (reward - self.mean_reward) / self.frequency as f32;
        if self.frequency > 1 {
            self.variance = ((self.frequency - 1) as f32 * self.variance
                + (reward - old_mean) * (reward - self.mean_reward))
                / self.frequency as f32;
        }
        self.last_updated = Utc::now();
    }

    pub fn get_confidence_score(&self) -> f32 {
        if self.frequency == 0 {
            return 0.0;
        }

        let frequency_factor = (self.frequency as f32).ln().max(1.0);
        let variance_factor = 1.0 / (1.0 + self.variance);
        frequency_factor * variance_factor
    }
}

impl Default for PatternStats {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryIndex {
    pub fn new() -> Self {
        Self {
            temporal_index: Arc::new(RwLock::new(BTreeMap::new())),
            action_index: Arc::new(RwLock::new(HashMap::new())),
            emotional_index: Arc::new(RwLock::new(HashMap::new())),
            outcome_index: Arc::new(RwLock::new(HashMap::new())),
            pattern_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    pub fn index_experience(
        &self,
        experience_id: usize,
        experience: &Experience,
        context: &RichContext,
    ) {
        {
            let mut temporal = self.temporal_index.write();
            temporal
                .entry(context.timestamp)
                .or_default()
                .push(experience_id);
        }
        if let Some(action) = &context.action {
            let mut action_idx = self.action_index.write();
            action_idx
                .entry(action.verb.clone())
                .or_default()
                .insert(experience_id);
        }
        {
            let mut emotional = self.emotional_index.write();
            let key = EmotionalStateKey::from(&experience.initial_emotional_state);
            emotional.entry(key).or_default().push(experience_id);
        }
        {
            let mut outcome = self.outcome_index.write();
            outcome
                .entry(context.outcome.success)
                .or_default()
                .insert(experience_id);
        }
        if !experience.action_sequence.is_empty() {
            let mut cache = self.pattern_cache.write();
            cache.remove(&experience.action_sequence);
        }
    }
    pub fn find_by_time_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<usize> {
        let temporal = self.temporal_index.read();
        temporal
            .range(start..=end)
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect()
    }
    pub fn find_by_action(&self, verb: &str) -> Vec<usize> {
        let action_idx = self.action_index.read();
        action_idx
            .get(verb)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }
    pub fn find_by_emotional_state(&self, state: &EmotionalState, tolerance: f32) -> Vec<usize> {
        let emotional = self.emotional_index.read();
        let target_key = EmotionalStateKey::from(state);
        let mut results = Vec::new();
        let bucket_range = (tolerance * 10.0).ceil() as i8;
        for v_offset in -bucket_range..=bucket_range {
            for a_offset in -bucket_range..=bucket_range {
                let key = EmotionalStateKey {
                    valence_bucket: target_key.valence_bucket + v_offset,
                    arousal_bucket: target_key.arousal_bucket + a_offset,
                };
                if let Some(ids) = emotional.get(&key) {
                    results.extend(ids.iter().copied());
                }
            }
        }
        results
    }
    pub fn get_pattern_stats(&self, pattern: &[Action]) -> Option<PatternStats> {
        let cache = self.pattern_cache.read();
        cache.get(pattern).cloned()
    }
    pub fn update_pattern_stats(&self, pattern: Vec<Action>, stats: PatternStats) {
        let mut cache = self.pattern_cache.write();
        cache.insert(pattern, stats);
    }
}

impl Default for MemoryIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
pub struct ContextBuilder {
    action: Option<Action>,
    event: Option<EventInfo>,
    outcome: Option<InteractionOutcome>,
    metadata: HashMap<String, serde_json::Value>,
}
impl ContextBuilder {
    pub fn with_action(mut self, action: Action) -> Self {
        self.action = Some(action);
        self
    }
    pub fn with_event(mut self, event: EventInfo) -> Self {
        self.event = Some(event);
        self
    }
    pub fn with_outcome(mut self, outcome: InteractionOutcome) -> Self {
        self.outcome = Some(outcome);
        self
    }
    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
    pub fn build(self, embedding_service: &impl EmbeddingGenerator) -> RichContext {
        let semantic_embedding = if let Some(ref action) = self.action {
            embedding_service.generate_embedding(action)
        } else {
            let outcome = self.outcome.as_ref();
            let mut embedding = vec![0.0; 32];
            if let Some(o) = outcome {
                embedding[0] = if o.success { 1.0 } else { -1.0 };
                embedding[1] = o.quality_score;
            }
            embedding
        };
        RichContext {
            action: self.action,
            event: self.event,
            timestamp: Utc::now(),
            outcome: self.outcome.unwrap_or_else(|| InteractionOutcome {
                success: false,
                quality_score: 0.0,
                feedback: String::new(),
                metadata: serde_json::json!({}),
            }),
            semantic_embedding,
            metadata: self.metadata,
        }
    }
}
pub trait EmbeddingGenerator {
    fn generate_embedding(&self, action: &Action) -> Vec<f32>;
}
#[derive(Debug)]
pub struct TfIdfEmbeddingGenerator {
    vocab: HashMap<String, usize>,
    idf_weights: Vec<f32>,
    pub embedding_dim: usize,
}
impl TfIdfEmbeddingGenerator {
    pub fn new(embedding_dim: usize) -> Self {
        Self {
            vocab: HashMap::new(),
            idf_weights: vec![1.0; embedding_dim],
            embedding_dim,
        }
    }
    pub fn update_vocab(&mut self, actions: &[Action]) {
        for action in actions {
            let words = action.verb.split_whitespace();
            for word in words {
                let len = self.vocab.len();
                self.vocab
                    .entry(word.to_lowercase())
                    .or_insert(len % self.embedding_dim);
            }
        }
    }
}
impl EmbeddingGenerator for TfIdfEmbeddingGenerator {
    fn generate_embedding(&self, action: &Action) -> Vec<f32> {
        let mut embedding = vec![0.0; self.embedding_dim];
        let words: Vec<_> = action.verb.split_whitespace().collect();
        let word_count = words.len() as f32;
        for word in words {
            if let Some(&idx) = self.vocab.get(&word.to_lowercase()) {
                embedding[idx] += 1.0 / word_count;
            }
        }
        for (i, idf_weight) in self.idf_weights.iter().enumerate().take(self.embedding_dim) {
            embedding[i] *= idf_weight;
        }
        if self.embedding_dim > 0 {
            embedding[0] += action.confidence * 0.5;
        }
        let norm = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut embedding {
                *val /= norm;
            }
        }
        embedding
    }
}
