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

pub mod core;
pub mod decision_making;
pub mod enhanced_memory;
pub mod memory_components;
pub mod neural_models;
pub mod persistence;
pub use core::MemorySystem;
pub use decision_making::StrategicDecisionMaker;
pub use enhanced_memory::{
    ContextBuilder, EmbeddingGenerator, MemoryIndex, RichContext, TfIdfEmbeddingGenerator,
};
pub use memory_components::{
    EpisodicMemory, Experience as MemoryExperience, MemoryConfig, PatternKnowledge, SemanticMemory,
    ShortTermMemory, TimeScale,
};
pub use neural_models::{
    AttentionMechanism, CausalDiscoveryModule, EmbeddingService, WorldModel, LSTM,
};
pub use persistence::{MemoryPersistence, MemorySnapshot};
