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

use crate::scribes::discourse::Testament;
use crate::scribes::StrategyVector;
use rand_distr::{Distribution, Normal};
use std::collections::VecDeque;
#[derive(Debug)]
struct MetaLearner {
    learning_rate: f32,
    exploration_factor: f32,
    performance_history: VecDeque<f32>,
}
impl MetaLearner {
    fn new() -> Self {
        Self {
            learning_rate: 0.1,
            exploration_factor: 0.05,
            performance_history: VecDeque::with_capacity(20),
        }
    }
    fn update(&mut self, performance_score: f32) {
        if self.performance_history.len() == 20 {
            self.performance_history.pop_front();
        }
        self.performance_history.push_back(performance_score);
        if self.performance_history.is_empty() {
            return;
        }
        let trend =
            self.performance_history.iter().sum::<f32>() / self.performance_history.len() as f32;
        let adjustment_factor = (-(trend - 0.75) * 2.0).exp();
        self.learning_rate = (self.learning_rate * adjustment_factor).clamp(0.01, 0.2);
        self.exploration_factor = (self.exploration_factor * adjustment_factor).clamp(0.01, 0.15);
    }
}
#[derive(Debug)]
pub struct LearningSystem {
    meta_learner: MetaLearner,
}
impl LearningSystem {
    pub fn new() -> Self {
        Self {
            meta_learner: MetaLearner::new(),
        }
    }
    pub fn evolve_strategy(
        &mut self,
        current_strategy: StrategyVector,
        testament: &Testament,
    ) -> StrategyVector {
        let mut new_strategy = current_strategy;
        let (gradient, performance_score) = self.gradient_from_testament(testament);
        self.meta_learner.update(performance_score);
        new_strategy.apply_gradient(gradient, self.meta_learner.learning_rate);
        self.apply_exploration(&mut new_strategy);
        new_strategy
    }
    fn gradient_from_testament(&self, testament: &Testament) -> ((f32, f32), f32) {
        let performance_score = if testament.was_successful { 1.0 } else { 0.0 };
        let (agg_delta, coop_delta) = if testament.was_successful {
            (-0.1, 0.2)
        } else {
            (0.2, -0.1)
        };
        ((agg_delta, coop_delta), performance_score)
    }
    fn apply_exploration(&self, strategy: &mut StrategyVector) {
        let mut rng = rand::thread_rng();
        let noise_dist = Normal::new(0.0, self.meta_learner.exploration_factor).unwrap();
        strategy.aggressiveness =
            (strategy.aggressiveness + noise_dist.sample(&mut rng) as f32).clamp(0.0, 1.0);
        strategy.cooperativeness =
            (strategy.cooperativeness + noise_dist.sample(&mut rng) as f32).clamp(0.0, 1.0);
    }
}

impl Default for LearningSystem {
    fn default() -> Self {
        Self::new()
    }
}
