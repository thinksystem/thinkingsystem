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

pub mod base_scribe;
pub mod canonical;
pub mod core;
pub mod discourse;
pub mod embeddings;
pub mod replay_buffer;
pub mod runtime; 
pub mod scriptorium;
pub mod specialists;
pub mod types;
pub use base_scribe::{
    BaseScribe, CostPerRequest, DataHandling, Delegate, PerformanceMetrics, ProviderMetadata,
    TaskPerformance,
};
pub use core::q_learning_core::Experience;
pub use replay_buffer::{MemoryExperience, ReplayBuffer, ReplayBufferConfig};
use serde::{Deserialize, Serialize};
pub use specialists::{DataScribe, IdentityScribe, KnowledgeScribe, Scribe, ScribeId};
pub use types::{EmotionalState, InteractionOutcome};
pub use runtime::{ScribeRuntimeManager, PreparedTask};
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct StrategyVector {
    pub aggressiveness: f32,
    pub cooperativeness: f32,
}
impl StrategyVector {
    pub fn apply_gradient(&mut self, gradient: (f32, f32), learning_rate: f32) {
        self.aggressiveness = (self.aggressiveness + gradient.0 * learning_rate).clamp(0.0, 1.0);
        self.cooperativeness = (self.cooperativeness + gradient.1 * learning_rate).clamp(0.0, 1.0);
    }
}
#[derive(Clone, Debug)]
pub struct ScribeState {
    pub specialist: Scribe,
    pub strategy: StrategyVector,
    pub interactions_completed: u32,
}
impl ScribeState {
    pub fn new(specialist: Scribe) -> Self {
        Self {
            specialist,
            strategy: StrategyVector {
                aggressiveness: 0.5,
                cooperativeness: 0.5,
            },
            interactions_completed: 0,
        }
    }
}
