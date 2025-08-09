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

use super::base_scribe::PerformanceMetrics;
use super::types::EmotionalState;
use crate::memory::memory_components::TimeScale;
use crate::memory::neural_models::{AttentionMechanism, RegularisationConfig, LSTM};
use crate::nlu::orchestrator::data_models::Action;
use rand::distributions::{Distribution, WeightedIndex};
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
const ACTION_EMBEDDING_DIM: usize = 4;
const LSTM_HIDDEN_DIM: usize = 32;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayBufferConfig {
    pub capacity: usize,
    pub priority_sample_ratio: f32,
    pub temporal_sample_ratio: f32,
}
impl Default for ReplayBufferConfig {
    fn default() -> Self {
        Self {
            capacity: 10000,
            priority_sample_ratio: 0.4,
            temporal_sample_ratio: 0.3,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryExperience {
    pub action_sequence: Vec<Action>,
    pub reward: f32,
    pub intrinsic_reward: f32,
    pub initial_metrics: PerformanceMetrics,
    pub final_metrics: PerformanceMetrics,
    pub initial_emotional_state: EmotionalState,
    pub final_emotional_state: EmotionalState,
    pub timestamp: u64,
    pub pattern_confidence: f32,
    pub embedding: Vec<f32>,
    pub timescale: TimeScale,
}
#[derive(Debug)]
pub struct ReplayBuffer {
    buffer: VecDeque<(usize, MemoryExperience)>,
    config: ReplayBufferConfig,
    priorities: HashMap<usize, f32>,
    temporal_weights: HashMap<usize, f32>,
    pattern_weights: HashMap<Vec<Action>, f32>,
    lstm_state_cache: HashMap<usize, (Vec<f32>, Vec<f32>)>,
    attention_scores: HashMap<usize, Vec<f32>>,
    next_id: usize,
    lstm: LSTM,
    attention: AttentionMechanism,
}
impl ReplayBuffer {
    pub fn new(config: ReplayBufferConfig) -> Self {
        Self {
            buffer: VecDeque::with_capacity(config.capacity),
            priorities: HashMap::new(),
            temporal_weights: HashMap::new(),
            pattern_weights: HashMap::new(),
            lstm_state_cache: HashMap::new(),
            attention_scores: HashMap::new(),
            next_id: 0,
            lstm: LSTM::new(
                ACTION_EMBEDDING_DIM,
                LSTM_HIDDEN_DIM,
                RegularisationConfig::default(),
            ),
            attention: AttentionMechanism::new(LSTM_HIDDEN_DIM),
            config,
        }
    }
    pub fn add(&mut self, experience: MemoryExperience) {
        if self.buffer.len() >= self.config.capacity {
            self.remove_oldest();
        }
        let id = self.next_id;
        self.next_id += 1;
        let priority = self.calculate_priority(&experience);
        self.priorities.insert(id, priority);
        self.update_temporal_weights();
        self.temporal_weights.insert(id, 1.0);
        let action_embeddings = embed_actions(&experience.action_sequence);
        let lstm_outputs = self.cache_lstm_state(id, &action_embeddings);
        self.update_attention_scores(id, &lstm_outputs);
        self.buffer.push_back((id, experience));
    }
    pub fn sample(&self, batch_size: usize) -> Vec<&MemoryExperience> {
        if self.buffer.is_empty() {
            return Vec::new();
        }
        let mut rng = rand::thread_rng();
        let mut sampled_ids = HashSet::new();
        let priority_count =
            (batch_size as f32 * self.config.priority_sample_ratio).round() as usize;
        let temporal_count =
            (batch_size as f32 * self.config.temporal_sample_ratio).round() as usize;
        let pattern_count = batch_size
            .saturating_sub(priority_count)
            .saturating_sub(temporal_count);
        sampled_ids.extend(self.priority_sampling(priority_count, &mut rng));
        sampled_ids.extend(self.temporal_sampling(temporal_count, &mut rng));
        sampled_ids.extend(self.pattern_sampling(pattern_count, &mut rng));
        let needed = batch_size.saturating_sub(sampled_ids.len());
        if needed > 0 {
            let random_samples: Vec<usize> = self.buffer.iter().map(|(id, _)| *id).collect();
            if !random_samples.is_empty() {
                sampled_ids.extend(random_samples.choose_multiple(&mut rng, needed).cloned());
            }
        }
        let experience_map: HashMap<usize, &MemoryExperience> =
            self.buffer.iter().map(|(id, exp)| (*id, exp)).collect();
        sampled_ids
            .iter()
            .filter_map(|id| experience_map.get(id).copied())
            .collect()
    }
    fn priority_sampling(&self, count: usize, rng: &mut impl Rng) -> Vec<usize> {
        let items: Vec<_> = self.priorities.iter().collect();
        if items.is_empty() || count == 0 {
            return Vec::new();
        }
        let Ok(dist) = WeightedIndex::new(items.iter().map(|(_, &p)| p)) else {
            return Vec::new();
        };
        dist.sample_iter(rng)
            .take(count)
            .map(|i| *items[i].0)
            .collect()
    }
    fn temporal_sampling(&self, count: usize, rng: &mut impl Rng) -> Vec<usize> {
        let items: Vec<_> = self.temporal_weights.iter().collect();
        if items.is_empty() || count == 0 {
            return Vec::new();
        }
        let Ok(dist) = WeightedIndex::new(items.iter().map(|(_, &w)| w)) else {
            return Vec::new();
        };
        dist.sample_iter(rng)
            .take(count)
            .map(|i| *items[i].0)
            .collect()
    }
    fn pattern_sampling(&self, count: usize, rng: &mut impl Rng) -> Vec<usize> {
        let pattern_items: Vec<_> = self.pattern_weights.iter().collect();
        if pattern_items.is_empty() || count == 0 {
            return Vec::new();
        }
        let Ok(dist) = WeightedIndex::new(pattern_items.iter().map(|(_, &w)| w)) else {
            return Vec::new();
        };
        let mut sampled_ids = Vec::with_capacity(count);
        for _ in 0..count {
            let chosen_pattern = pattern_items[dist.sample(rng)].0;
            let matching_experiences: Vec<usize> = self
                .buffer
                .iter()
                .filter(|(_, exp)| exp.action_sequence == *chosen_pattern)
                .map(|(id, _)| *id)
                .collect();
            if let Some(id) = matching_experiences.choose(rng) {
                sampled_ids.push(*id);
            }
        }
        sampled_ids
    }
    fn remove_oldest(&mut self) {
        if let Some((removed_id, removed_exp)) = self.buffer.pop_front() {
            self.priorities.remove(&removed_id);
            self.temporal_weights.remove(&removed_id);
            self.lstm_state_cache.remove(&removed_id);
            self.attention_scores.remove(&removed_id);
            if let Some(weight) = self.pattern_weights.get_mut(&removed_exp.action_sequence) {
                *weight -= 1.0;
                if *weight < 1.0 {
                    self.pattern_weights.remove(&removed_exp.action_sequence);
                }
            }
        }
    }
    fn calculate_priority(&self, experience: &MemoryExperience) -> f32 {
        experience.reward.abs() + (1.0 - experience.pattern_confidence) + 1e-6
    }
    fn update_temporal_weights(&mut self) {
        let decay_factor = 0.99;
        for weight in self.temporal_weights.values_mut() {
            *weight *= decay_factor;
        }
    }
    fn cache_lstm_state(&mut self, id: usize, embeddings: &[Vec<f32>]) -> Vec<Vec<f32>> {
        let mut hidden_state = vec![0.0; LSTM_HIDDEN_DIM];
        let mut cell_state = vec![0.0; LSTM_HIDDEN_DIM];
        let mut outputs = Vec::with_capacity(embeddings.len());
        for embedding in embeddings {
            (hidden_state, cell_state) = self.lstm.forward(embedding, (&hidden_state, &cell_state));
            outputs.push(hidden_state.clone());
        }
        self.lstm_state_cache.insert(id, (hidden_state, cell_state));
        outputs
    }
    fn update_attention_scores(&mut self, id: usize, lstm_outputs: &[Vec<f32>]) {
        let scores = self.attention.compute_scores(lstm_outputs);
        self.attention_scores.insert(id, scores);
    }
    pub fn len(&self) -> usize {
        self.buffer.len()
    }
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}
fn embed_actions(actions: &[Action]) -> Vec<Vec<f32>> {
    actions
        .iter()
        .map(|action| match action.verb.as_str() {
            "attack" => vec![1.0, 0.0, 0.0, 0.0],
            "defend" => vec![0.0, 1.0, 0.0, 0.0],
            "cooperate" => vec![0.0, 0.0, 1.0, 0.0],
            _ => vec![0.0, 0.0, 0.0, 1.0],
        })
        .collect()
}
