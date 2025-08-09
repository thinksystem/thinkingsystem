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

use super::core::MemorySystem;
use super::memory_components::Experience;
use crate::nlu::orchestrator::data_models::Action;
use crate::scribes::EmotionalState;
const CASE_BASED_REASONING_COUNT: usize = 3;
#[derive(Debug)]
pub struct StrategicDecisionMaker {
    exploration_factor: f32,
    uncertainty_penalty_factor: f32,
}
impl StrategicDecisionMaker {
    pub fn new(exploration_factor: f32, uncertainty_penalty_factor: f32) -> Self {
        Self {
            exploration_factor,
            uncertainty_penalty_factor,
        }
    }
    pub fn select_action<'a>(
        &self,
        available_actions: &[&'a Vec<Action>],
        memory_system: &'a MemorySystem,
        current_emotional_state: &EmotionalState,
    ) -> Option<&'a Vec<Action>> {
        if available_actions.is_empty() {
            return None;
        }
        let current_context = memory_system.generate_current_context();
        let total_plays = memory_system.semantic.get_total_experiences().max(1) as f32;
        let log_total_plays = (total_plays + 1e-6).ln();
        let similar_experiences = memory_system.retrieve_by_association(
            &current_context,
            current_emotional_state,
            CASE_BASED_REASONING_COUNT,
        );
        available_actions
            .iter()
            .max_by(|a, b| {
                let ucb_a = self.calculate_hybrid_ucb(
                    a,
                    &current_context,
                    memory_system,
                    log_total_plays,
                    &similar_experiences,
                );
                let ucb_b = self.calculate_hybrid_ucb(
                    b,
                    &current_context,
                    memory_system,
                    log_total_plays,
                    &similar_experiences,
                );
                ucb_a
                    .partial_cmp(&ucb_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied()
    }
    fn calculate_hybrid_ucb(
        &self,
        action_pattern: &[Action],
        context: &[f32],
        memory: &MemorySystem,
        log_total_plays: f32,
        similar_experiences: &[&Experience],
    ) -> f32 {
        let action_embedding = memory.embedding_service.embed_actions(action_pattern);
        let representative_action_embedding = action_embedding.last().unwrap_or(&vec![]).to_vec();
        let (predicted_reward, _) = memory
            .world_model
            .predict(context, &representative_action_embedding);
        let (semantic_reward, plays, variance) = match memory.semantic.query(action_pattern) {
            Some(knowledge) => (
                knowledge.mean_total_reward,
                knowledge.frequency as f32,
                knowledge.get_reward_variance(),
            ),
            None => (0.0, 1.0, 0.0),
        };
        let case_based_reward_bonus: f32 = similar_experiences
            .iter()
            .filter(|exp| exp.action_sequence == action_pattern)
            .map(|exp| exp.reward + exp.intrinsic_reward)
            .sum::<f32>()
            / CASE_BASED_REASONING_COUNT.max(1) as f32;
        let exploitation_term =
            (predicted_reward * 0.4) + (semantic_reward * 0.4) + (case_based_reward_bonus * 0.2);
        let exploration_term = self.exploration_factor * (log_total_plays / plays).sqrt();
        let uncertainty_penalty = self.uncertainty_penalty_factor * variance;
        exploitation_term + exploration_term - uncertainty_penalty
    }
}
