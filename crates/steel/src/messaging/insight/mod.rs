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

pub mod analysis;
pub mod config;
pub mod distribution;
pub mod feedback;
pub mod generators;
pub mod hybrid_analyser;
pub mod llm_integration;
pub mod metrics;
pub mod ner_analysis;
pub mod optimiser;
pub mod scribe_bridge;
pub mod security;

pub use analysis::{ContentAnalyser, ContentAnalysis};
pub use config::{ScoringConfig, SecurityConfig};
pub use distribution::ScoreDistribution;
pub use feedback::{FeedbackLoop, TrainingStats};
pub use generators::*;
pub use hybrid_analyser::{
    EntityPriority, HybridConfig, HybridContentAnalyser, HybridContentAnalysis,
};
pub use llm_integration::{HybridAnalyser, LlmClient, LlmPiiResponse};
pub use metrics::{ModelPerformance, TrainingExample};
pub use ner_analysis::{DetectedEntity, NerAnalyser, NerAnalysisResult, NerConfig};
pub use optimiser::ModelOptimiser;
pub use scribe_bridge::{
    BridgeMode, ScribeSecurityBridge, ScribeSecurityEvent, ScribeSecurityResponse,
};
pub use security::{AnalysisMode, MessageAnalysisResult, MessageSecurity};
