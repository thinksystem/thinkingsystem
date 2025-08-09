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

use crate::memory::neural_models::RegularisationConfig;
use crate::nlu::orchestrator::data_models::Action;
use crate::scribes::{EmotionalState, InteractionOutcome, PerformanceMetrics};
use chrono::{DateTime, Utc};
use rand::distributions::{Distribution, WeightedIndex};
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TimeScale {
    ShortTerm,
    LongTerm,
}
#[derive(Debug, Clone)]
pub struct TimeScaleConfig {
    pub temporal_decay: f32,
    pub prune_aggressiveness: f32,
}
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    pub short_term_capacity: usize,
    pub episodic_capacity: usize,
    pub priority_sample_ratio: f32,
    pub temporal_sample_ratio: f32,
    pub pattern_sample_ratio: f32,
    pub priority_alpha: f32,
    pub intrinsic_reward_factor: f32,
    pub time_scales: HashMap<TimeScale, TimeScaleConfig>,
    pub meta_learning_error_threshold: f32,
    pub default_priority_sample_ratio: f32,
    pub regularisation: RegularisationConfig,
    pub meta_base_learning_rate: f32,
    pub meta_momentum: f32,
    pub meta_decay_factor: f32,
}
impl Default for MemoryConfig {
    fn default() -> Self {
        let mut time_scales = HashMap::new();
        time_scales.insert(
            TimeScale::ShortTerm,
            TimeScaleConfig {
                temporal_decay: 0.990,
                prune_aggressiveness: 1.0,
            },
        );
        time_scales.insert(
            TimeScale::LongTerm,
            TimeScaleConfig {
                temporal_decay: 0.999,
                prune_aggressiveness: 0.5,
            },
        );
        let default_priority_ratio = 0.4;
        Self {
            short_term_capacity: 50,
            episodic_capacity: 10000,
            priority_sample_ratio: default_priority_ratio,
            temporal_sample_ratio: 0.3,
            pattern_sample_ratio: 0.2,
            priority_alpha: 0.6,
            intrinsic_reward_factor: 0.1,
            time_scales,
            meta_learning_error_threshold: 0.8,
            default_priority_sample_ratio: default_priority_ratio,
            regularisation: RegularisationConfig::default(),
            meta_base_learning_rate: 0.01,
            meta_momentum: 0.9,
            meta_decay_factor: 0.99,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub action_sequence: Vec<Action>,
    pub reward: f32,
    pub intrinsic_reward: f32,
    pub initial_metrics: PerformanceMetrics,
    pub final_metrics: PerformanceMetrics,
    pub initial_emotional_state: EmotionalState,
    pub final_emotional_state: EmotionalState,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub timestamp: DateTime<Utc>,
    pub pattern_confidence: f32,
    pub embedding: Vec<f32>,
    pub timescale: TimeScale,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternKnowledge {
    pub frequency: u64,
    pub mean_reward: f32,
    pub mean_total_reward: f32,
    m2_reward: f32,
    pub mean_emotional_shift: (f32, f32),
}
impl PatternKnowledge {
    pub(crate) fn from_experience(exp: &Experience) -> Self {
        let emotional_shift = (
            exp.final_emotional_state.valence - exp.initial_emotional_state.valence,
            exp.final_emotional_state.arousal - exp.initial_emotional_state.arousal,
        );
        Self {
            frequency: 1,
            mean_reward: exp.reward,
            mean_total_reward: exp.reward + exp.intrinsic_reward,
            m2_reward: 0.0,
            mean_emotional_shift: emotional_shift,
        }
    }
    pub(crate) fn update(&mut self, exp: &Experience) {
        self.frequency += 1;
        let n = self.frequency as f32;
        let total_reward = exp.reward + exp.intrinsic_reward;
        let delta_reward = exp.reward - self.mean_reward;
        self.mean_reward += delta_reward / n;
        let delta_total_reward = total_reward - self.mean_total_reward;
        self.mean_total_reward += delta_total_reward / n;
        let delta2_total_reward = total_reward - self.mean_total_reward;
        self.m2_reward += delta_total_reward * delta2_total_reward;
        let emotional_shift = (
            exp.final_emotional_state.valence - exp.initial_emotional_state.valence,
            exp.final_emotional_state.arousal - exp.initial_emotional_state.arousal,
        );
        self.mean_emotional_shift.0 += (emotional_shift.0 - self.mean_emotional_shift.0) / n;
        self.mean_emotional_shift.1 += (emotional_shift.1 - self.mean_emotional_shift.1) / n;
    }
    pub fn get_reward_variance(&self) -> f32 {
        if self.frequency < 2 {
            0.0
        } else {
            self.m2_reward / (self.frequency - 1) as f32
        }
    }
}
#[derive(Debug)]
pub struct ShortTermMemory {
    history: VecDeque<InteractionOutcome>,
    capacity: usize,
}
impl ShortTermMemory {
    pub fn new(capacity: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(capacity),
            capacity,
        }
    }
    pub fn record(&mut self, outcome: InteractionOutcome) {
        if self.history.len() == self.capacity {
            self.history.pop_front();
        }
        self.history.push_back(outcome);
    }
    pub fn get_history(&self) -> &VecDeque<InteractionOutcome> {
        &self.history
    }
    pub fn clear(&mut self) {
        self.history.clear();
    }
}
#[derive(Debug, Default)]
pub struct SemanticMemory {
    patterns: HashMap<Vec<Action>, PatternKnowledge>,
    total_experiences: u64,
}
impl SemanticMemory {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn consolidate_experience(&mut self, experience: &Experience) {
        self.total_experiences += 1;
        self.patterns
            .entry(experience.action_sequence.clone())
            .and_modify(|k| k.update(experience))
            .or_insert_with(|| PatternKnowledge::from_experience(experience));
    }
    pub fn query(&self, pattern: &[Action]) -> Option<&PatternKnowledge> {
        self.patterns.get(pattern)
    }
    pub fn get_all_patterns(&self) -> &HashMap<Vec<Action>, PatternKnowledge> {
        &self.patterns
    }
    pub fn get_total_experiences(&self) -> u64 {
        self.total_experiences
    }
}
#[derive(Debug)]
pub struct EpisodicMemory {
    buffer: VecDeque<(usize, Experience)>,
    config: MemoryConfig,
    priorities: HashMap<usize, f32>,
    temporal_weights: HashMap<usize, f32>,
    next_id: usize,
}
impl EpisodicMemory {
    pub fn new(config: MemoryConfig) -> Self {
        Self {
            buffer: VecDeque::with_capacity(config.episodic_capacity),
            priorities: HashMap::new(),
            temporal_weights: HashMap::new(),
            next_id: 0,
            config,
        }
    }
    pub fn record(&mut self, experience: Experience, semantic_memory: &mut SemanticMemory) {
        if self.buffer.len() >= self.config.episodic_capacity {
            self.prune();
        }
        let id = self.next_id;
        self.next_id += 1;
        let priority = self.calculate_priority(&experience);
        semantic_memory.consolidate_experience(&experience);
        self.priorities.insert(id, priority);
        self.update_temporal_weights(&experience.timescale);
        self.temporal_weights.insert(id, 1.0);
        self.buffer.push_back((id, experience));
    }
    pub fn sample(&self, batch_size: usize, semantic_memory: &SemanticMemory) -> Vec<&Experience> {
        if self.buffer.is_empty() {
            return Vec::new();
        }
        let mut rng = rand::thread_rng();
        let mut sampled_ids = HashSet::new();
        let priority_count =
            (batch_size as f32 * self.config.priority_sample_ratio).round() as usize;
        let temporal_count =
            (batch_size as f32 * self.config.temporal_sample_ratio).round() as usize;
        let pattern_count = (batch_size as f32 * self.config.pattern_sample_ratio).round() as usize;
        sampled_ids.extend(self.priority_sampling(priority_count, &mut rng));
        sampled_ids.extend(self.temporal_sampling(temporal_count, &mut rng));
        sampled_ids.extend(self.pattern_sampling(pattern_count, semantic_memory, &mut rng));
        let needed = batch_size.saturating_sub(sampled_ids.len());
        if needed > 0 {
            let all_ids: Vec<usize> = self.buffer.iter().map(|(id, _)| *id).collect();
            let available_ids: Vec<usize> = all_ids
                .into_iter()
                .filter(|id| !sampled_ids.contains(id))
                .collect();
            if !available_ids.is_empty() {
                sampled_ids.extend(available_ids.choose_multiple(&mut rng, needed).cloned());
            }
        }
        let experience_map: HashMap<usize, &Experience> =
            self.buffer.iter().map(|(id, exp)| (*id, exp)).collect();
        sampled_ids
            .iter()
            .filter_map(|id| experience_map.get(id).copied())
            .collect()
    }
    pub fn sample_with_priority(&self, count: usize) -> Vec<&Experience> {
        if self.priorities.is_empty() {
            return Vec::new();
        }
        let mut sorted_priorities: Vec<_> = self.priorities.iter().collect();
        sorted_priorities.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
        let experience_map: HashMap<usize, &Experience> =
            self.buffer.iter().map(|(id, exp)| (*id, exp)).collect();
        sorted_priorities
            .iter()
            .take(count)
            .filter_map(|(id, _)| experience_map.get(id).copied())
            .collect()
    }
    pub fn get_buffer(&self) -> &VecDeque<(usize, Experience)> {
        &self.buffer
    }
    pub fn get_last_embedding(&self) -> Option<&Vec<f32>> {
        self.buffer.back().map(|(_, exp)| &exp.embedding)
    }
    pub fn get_next_id(&self) -> usize {
        self.next_id
    }
    fn prune(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        let mut min_value = f32::MAX;
        let mut index_to_remove = 0;
        for (i, (id, exp)) in self.buffer.iter().enumerate() {
            let priority = self.priorities.get(id).unwrap_or(&1e-6);
            let temporal_weight = self.temporal_weights.get(id).unwrap_or(&1e-6);
            let timescale_config = self.config.time_scales.get(&exp.timescale).unwrap();
            let value = priority * temporal_weight * timescale_config.prune_aggressiveness;
            if value < min_value {
                min_value = value;
                index_to_remove = i;
            }
        }
        if let Some((removed_id, _)) = self.buffer.remove(index_to_remove) {
            self.priorities.remove(&removed_id);
            self.temporal_weights.remove(&removed_id);
        }
    }
    fn calculate_priority(&self, experience: &Experience) -> f32 {
        let emotional_shift = (experience.final_emotional_state.valence
            - experience.initial_emotional_state.valence)
            .abs()
            + (experience.final_emotional_state.arousal
                - experience.initial_emotional_state.arousal)
                .abs();
        let embedding_l2_penalty =
            0.01 * experience.embedding.iter().map(|v| v.powi(2)).sum::<f32>();
        let base_priority = experience.reward.abs()
            + experience.intrinsic_reward
            + emotional_shift
            + (1.0 - experience.pattern_confidence)
            - embedding_l2_penalty;
        (base_priority.max(0.0)).powf(self.config.priority_alpha) + 1e-6
    }
    fn update_temporal_weights(&mut self, new_exp_timescale: &TimeScale) {
        let decay_rate = self
            .config
            .time_scales
            .get(new_exp_timescale)
            .unwrap()
            .temporal_decay;
        for weight in self.temporal_weights.values_mut() {
            *weight *= decay_rate;
        }
    }
    fn priority_sampling(&self, count: usize, rng: &mut impl Rng) -> Vec<usize> {
        if count == 0 || self.priorities.is_empty() {
            return Vec::new();
        }
        let items: Vec<_> = self.priorities.iter().collect();
        if let Ok(dist) = WeightedIndex::new(items.iter().map(|(_, &p)| p)) {
            dist.sample_iter(rng)
                .take(count)
                .map(|i| *items[i].0)
                .collect()
        } else {
            Vec::new()
        }
    }
    fn temporal_sampling(&self, count: usize, rng: &mut impl Rng) -> Vec<usize> {
        if count == 0 || self.temporal_weights.is_empty() {
            return Vec::new();
        }
        let items: Vec<_> = self.temporal_weights.iter().collect();
        if let Ok(dist) = WeightedIndex::new(items.iter().map(|(_, &w)| w)) {
            dist.sample_iter(rng)
                .take(count)
                .map(|i| *items[i].0)
                .collect()
        } else {
            Vec::new()
        }
    }
    fn pattern_sampling(
        &self,
        count: usize,
        semantic_memory: &SemanticMemory,
        rng: &mut impl Rng,
    ) -> Vec<usize> {
        let pattern_items: Vec<_> = semantic_memory.get_all_patterns().iter().collect();
        if count == 0 || pattern_items.is_empty() {
            return Vec::new();
        }
        let weights: Vec<f32> = pattern_items
            .iter()
            .map(|(_, knowledge)| {
                (1.0 / knowledge.frequency as f32) + knowledge.get_reward_variance() + 1e-6
            })
            .collect();
        if let Ok(dist) = WeightedIndex::new(&weights) {
            let mut sampled_ids = Vec::with_capacity(count);
            for _ in 0..count {
                let chosen_pattern = pattern_items[dist.sample(rng)].0;
                let matching_experiences: Vec<usize> = self
                    .buffer
                    .iter()
                    .filter(|(_, exp)| &exp.action_sequence == chosen_pattern)
                    .map(|(id, _)| *id)
                    .collect();
                if let Some(id) = matching_experiences.choose(rng) {
                    sampled_ids.push(*id);
                }
            }
            sampled_ids
        } else {
            Vec::new()
        }
    }
}
