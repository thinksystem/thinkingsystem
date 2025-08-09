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

use super::enhanced_memory::{ContextBuilder, MemoryIndex, RichContext, TfIdfEmbeddingGenerator};
use super::memory_components::{
    EpisodicMemory, Experience, MemoryConfig, PatternKnowledge, SemanticMemory, ShortTermMemory,
};
use super::neural_models::{
    AttentionMechanism, CausalDiscoveryModule, EmbeddingService, WorldModel, LSTM,
};
use crate::scribes::{EmotionalState, InteractionOutcome};
use std::collections::HashMap;
const LSTM_HIDDEN_DIM: usize = 64;
const CONSOLIDATION_BATCH_SIZE: usize = 128;
const META_LEARNING_UPDATE_INTERVAL: u64 = 100;
const CAUSAL_DISCOVERY_SIGNIFICANCE: f64 = 0.05;
const CAUSAL_DISCOVERY_WINDOW_SECS: i64 = 10;
#[derive(Debug)]
pub struct MetaLearningController {
    experiences_since_last_update: u64,
    adjustment_momentum: f32,
    base_learning_rate: f32,
    momentum_factor: f32,
    decay_factor: f32,
}
impl MetaLearningController {
    pub fn new(config: &MemoryConfig) -> Self {
        Self {
            experiences_since_last_update: 0,
            adjustment_momentum: 0.0,
            base_learning_rate: config.meta_base_learning_rate,
            momentum_factor: config.meta_momentum,
            decay_factor: config.meta_decay_factor,
        }
    }
    fn calculate_adaptive_step(&mut self, error_delta: f32) -> f32 {
        let gradient = error_delta;
        let update =
            self.base_learning_rate * gradient + self.momentum_factor * self.adjustment_momentum;
        self.adjustment_momentum = update;
        update
    }
    fn update_learning_rates(&mut self) {
        self.base_learning_rate *= self.decay_factor;
        self.adjustment_momentum *= self.decay_factor;
    }
    pub fn update_strategy_if_needed(
        &mut self,
        config: &mut MemoryConfig,
        world_model: &WorldModel,
    ) {
        self.experiences_since_last_update += 1;
        if self.experiences_since_last_update >= META_LEARNING_UPDATE_INTERVAL {
            self.experiences_since_last_update = 0;
            let avg_error = world_model.get_average_prediction_error();
            let error_delta = avg_error - config.meta_learning_error_threshold;
            if error_delta.abs() > 0.05 {
                let step = self.calculate_adaptive_step(error_delta);
                if error_delta > 0.0 {
                    let new_ratio = (config.priority_sample_ratio + step).clamp(0.1, 0.9);
                    println!(
                        "[Meta-Learning] High error ({avg_error:.3}). Adjusting priority ratio to {new_ratio:.3}"
                    );
                    config.priority_sample_ratio = new_ratio;
                } else {
                    let new_ratio = (config.priority_sample_ratio + step)
                        .max(config.default_priority_sample_ratio);
                    println!("[Meta-Learning] Low error ({avg_error:.3}). Reverting to balanced sampling ratio: {new_ratio:.3}");
                    config.priority_sample_ratio = new_ratio;
                }
            }
            self.update_learning_rates();
        }
    }
}
#[derive(Debug)]
pub struct MemorySystem {
    pub short_term: ShortTermMemory,
    pub episodic: EpisodicMemory,
    pub semantic: SemanticMemory,
    pub world_model: WorldModel,
    pub causal_module: CausalDiscoveryModule,
    pub config: MemoryConfig,
    lstm: LSTM,
    attention: AttentionMechanism,
    pub embedding_service: EmbeddingService,
    meta_learning_controller: MetaLearningController,
    memory_index: MemoryIndex,
    context_embedder: TfIdfEmbeddingGenerator,
    rich_contexts: HashMap<usize, RichContext>,
}
impl MemorySystem {
    pub fn new(config: MemoryConfig) -> Self {
        let embedding_dim = EmbeddingService::get_embedding_dim();
        let reg_config = config.regularisation;
        Self {
            short_term: ShortTermMemory::new(config.short_term_capacity),
            episodic: EpisodicMemory::new(config.clone()),
            semantic: SemanticMemory::new(),
            world_model: WorldModel::new(LSTM_HIDDEN_DIM, embedding_dim, reg_config),
            causal_module: CausalDiscoveryModule::new(
                CAUSAL_DISCOVERY_SIGNIFICANCE,
                CAUSAL_DISCOVERY_WINDOW_SECS,
            ),
            lstm: LSTM::new(embedding_dim, LSTM_HIDDEN_DIM, reg_config),
            attention: AttentionMechanism::new(LSTM_HIDDEN_DIM),
            embedding_service: EmbeddingService::new(&reg_config),
            meta_learning_controller: MetaLearningController::new(&config),
            memory_index: MemoryIndex::new(),
            context_embedder: TfIdfEmbeddingGenerator::new(embedding_dim),
            rich_contexts: HashMap::new(),
            config,
        }
    }
    pub fn generate_current_context(&self) -> Vec<f32> {
        let recent_contexts: Vec<_> = self
            .rich_contexts
            .iter()
            .filter_map(|(_id, context)| {
                let now = chrono::Utc::now();
                let age = now.signed_duration_since(context.timestamp).num_seconds();
                if age < 10 {
                    Some(context)
                } else {
                    None
                }
            })
            .collect();
        if !recent_contexts.is_empty() {
            let mut weighted_embedding = vec![0.0; EmbeddingService::get_embedding_dim()];
            let now = chrono::Utc::now();
            for ctx in &recent_contexts {
                let age = now.signed_duration_since(ctx.timestamp).num_seconds() as f32;
                let weight = (-age / 5.0).exp();
                for (i, &val) in ctx.semantic_embedding.iter().enumerate() {
                    if i < weighted_embedding.len() {
                        weighted_embedding[i] += val * weight;
                    }
                }
            }
            let (hidden, _) = self.lstm.forward(
                &weighted_embedding,
                (&vec![0.0; LSTM_HIDDEN_DIM], &vec![0.0; LSTM_HIDDEN_DIM]),
            );
            hidden
        } else {
            let recent_outcomes = self.short_term.get_history();
            if recent_outcomes.is_empty() {
                return vec![0.0; LSTM_HIDDEN_DIM];
            }
            let mut context_embedding = vec![0.0; LSTM_HIDDEN_DIM];
            for (i, outcome) in recent_outcomes.iter().enumerate() {
                if i >= LSTM_HIDDEN_DIM {
                    break;
                }
                context_embedding[i % LSTM_HIDDEN_DIM] += if outcome.success { 1.0 } else { -0.5 };
                context_embedding[(i + 1) % LSTM_HIDDEN_DIM] += outcome.quality_score;
            }
            let norm = context_embedding
                .iter()
                .map(|x| x.powi(2))
                .sum::<f32>()
                .sqrt();
            if norm > 0.0 {
                for val in context_embedding.iter_mut() {
                    *val /= norm;
                }
            }
            context_embedding
        }
    }
    pub fn record_interaction(&mut self, outcome: InteractionOutcome) {
        self.short_term.record(outcome);
    }
    pub fn record_rich_interaction(&mut self, builder: ContextBuilder) {
        let context = builder.build(&self.context_embedder);
        let context_id = self.rich_contexts.len();
        self.short_term.record(context.outcome.clone());
        self.rich_contexts.insert(context_id, context);
    }
    pub fn record_experience(&mut self, mut experience: Experience) {
        let action_embeddings = self
            .embedding_service
            .embed_actions(&experience.action_sequence);
        let lstm_outputs = self.process_sequence_with_lstm(&action_embeddings);
        experience.embedding = if let Some(final_hidden_state) = lstm_outputs.last() {
            self.attention
                .compute_context_vector(&lstm_outputs, final_hidden_state)
        } else {
            vec![0.0; LSTM_HIDDEN_DIM]
        };
        if let (Some(initial_embedding), Some(action_embedding)) =
            (self.episodic.get_last_embedding(), action_embeddings.last())
        {
            let (predicted_reward, predicted_next_state) = self
                .world_model
                .predict(initial_embedding, action_embedding);
            let reward_error = (experience.reward - predicted_reward).powi(2);
            let state_error = predicted_next_state
                .iter()
                .zip(experience.embedding.iter())
                .map(|(p, a)| (a - p).powi(2))
                .sum::<f32>()
                / predicted_next_state.len().max(1) as f32;
            experience.intrinsic_reward = (reward_error + state_error).sqrt();
            let total_reward_for_learning = experience.reward
                + experience.intrinsic_reward * self.config.intrinsic_reward_factor;
            self.world_model.train(
                initial_embedding,
                action_embedding,
                &experience.embedding,
                total_reward_for_learning,
            );
        }
        let experience_id = self.episodic.get_next_id();
        self.episodic.record(experience.clone(), &mut self.semantic);
        if let Some(recent_context) = self.rich_contexts.values().last() {
            self.memory_index
                .index_experience(experience_id, &experience, recent_context);
        }
        self.meta_learning_controller
            .update_strategy_if_needed(&mut self.config, &self.world_model);
    }
    pub fn perform_offline_consolidation(&mut self) {
        println!("[Consolidation] Starting offline consolidation (sleep)...");
        let experiences = self
            .episodic
            .sample_with_priority(CONSOLIDATION_BATCH_SIZE * 2);
        for experience in &experiences {
            if let Some(action_embedding) = self
                .embedding_service
                .embed_actions(&experience.action_sequence)
                .last()
            {
                let prev_state_proxy = &experience.embedding;
                let total_reward = experience.reward + experience.intrinsic_reward;
                self.world_model.train(
                    prev_state_proxy,
                    action_embedding,
                    &experience.embedding,
                    total_reward,
                );
            }
        }
        println!("[Consolidation] Analysing for significant causal links...");
        self.causal_module.discover_patterns(&experiences);
        println!("[Consolidation] Offline consolidation complete.");
    }
    pub fn sample_for_learning(&self, batch_size: usize) -> Vec<&Experience> {
        self.episodic.sample(batch_size, &self.semantic)
    }
    pub fn find_experiences_by_action(&self, verb: &str) -> Vec<&Experience> {
        let experience_ids = self.memory_index.find_by_action(verb);
        self.episodic
            .get_buffer()
            .iter()
            .filter_map(|(id, exp)| {
                if experience_ids.contains(id) {
                    Some(exp)
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn find_recent_experiences(&self, seconds: i64) -> Vec<&Experience> {
        let now = chrono::Utc::now();
        let start = now - chrono::Duration::seconds(seconds);
        let experience_ids = self.memory_index.find_by_time_range(start, now);
        self.episodic
            .get_buffer()
            .iter()
            .filter_map(|(id, exp)| {
                if experience_ids.contains(id) {
                    Some(exp)
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn retrieve_by_association(
        &self,
        context_embedding: &[f32],
        current_emotional_state: &EmotionalState,
        count: usize,
    ) -> Vec<&Experience> {
        if context_embedding.is_empty() || count == 0 {
            return Vec::new();
        }
        let mut scored_experiences: Vec<(f32, &Experience)> = self
            .episodic
            .get_buffer()
            .iter()
            .map(|(_, exp)| {
                let semantic_sim = cosine_similarity(&exp.embedding, context_embedding);
                let valence_dist =
                    (exp.initial_emotional_state.valence - current_emotional_state.valence).powi(2);
                let arousal_dist =
                    (exp.initial_emotional_state.arousal - current_emotional_state.arousal).powi(2);
                let emotional_dist = (valence_dist + arousal_dist).sqrt();
                let emotional_sim = 1.0 / (1.0 + emotional_dist);
                let combined_score = semantic_sim * 0.7 + emotional_sim * 0.3;
                (-combined_score, exp)
            })
            .collect();
        scored_experiences
            .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        scored_experiences
            .into_iter()
            .take(count)
            .map(|(_, exp)| exp)
            .collect()
    }
    fn process_sequence_with_lstm(&self, embeddings: &[Vec<f32>]) -> Vec<Vec<f32>> {
        let mut hidden_state = vec![0.0; LSTM_HIDDEN_DIM];
        let mut cell_state = vec![0.0; LSTM_HIDDEN_DIM];
        let mut outputs = Vec::with_capacity(embeddings.len());
        for embedding in embeddings {
            (hidden_state, cell_state) = self.lstm.forward(embedding, (&hidden_state, &cell_state));
            outputs.push(hidden_state.clone());
        }
        outputs
    }
    pub fn save_to_file<P: AsRef<std::path::Path>>(
        &self,
        path: P,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use super::persistence::MemoryPersistence;
        let experiences: Vec<(usize, Experience)> = self
            .episodic
            .get_buffer()
            .iter()
            .map(|(id, exp)| (*id, exp.clone()))
            .collect();
        let patterns: Vec<(Vec<String>, PatternKnowledge)> = self
            .semantic
            .get_all_patterns()
            .iter()
            .map(|(actions, knowledge)| {
                let action_strings: Vec<String> = actions.iter().map(|a| a.verb.clone()).collect();
                (action_strings, knowledge.clone())
            })
            .collect();
        let rich_contexts: Vec<(usize, RichContext)> = self
            .rich_contexts
            .iter()
            .map(|(id, ctx)| (*id, ctx.clone()))
            .collect();
        MemoryPersistence::save_to_file(path, &experiences, &patterns, &rich_contexts, &self.config)
    }
    pub fn load_from_file<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use super::persistence::MemoryPersistence;
        let snapshot = MemoryPersistence::load_from_file(path)?;
        self.rich_contexts.clear();
        for (_id, experience) in snapshot.experiences {
            self.episodic.record(experience, &mut self.semantic);
        }
        for (id, context) in snapshot.rich_contexts {
            if let Some(experience) = self
                .episodic
                .get_buffer()
                .iter()
                .find(|(exp_id, _)| *exp_id == id)
            {
                self.memory_index
                    .index_experience(id, &experience.1, &context);
            }
            self.rich_contexts.insert(id, context);
        }
        let all_actions: Vec<crate::nlu::orchestrator::data_models::Action> = self
            .episodic
            .get_buffer()
            .iter()
            .flat_map(|(_, exp)| exp.action_sequence.clone())
            .collect();
        self.context_embedder.update_vocab(&all_actions);
        Ok(())
    }
}
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let dot_product = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum::<f32>();
    let norm_a = a.iter().map(|x| x.powi(2)).sum::<f32>().sqrt();
    let norm_b = b.iter().map(|x| x.powi(2)).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
}
