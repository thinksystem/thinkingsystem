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

use chrono::Utc;
use std::collections::HashMap;
use stele::memory::enhanced_memory::{MemoryIndex, RichContext};
use stele::memory::memory_components::Experience;
use stele::memory::TimeScale;
use stele::nlu::orchestrator::data_models::Action;
use stele::scribes::base_scribe::PerformanceMetrics;
use stele::scribes::{EmotionalState, InteractionOutcome};

#[test]
fn test_memory_index_operations() {
    let index = MemoryIndex::new();
    let emotional_state = EmotionalState {
        valence: 0.7,
        arousal: 0.5,
        dominance: 0.6,
        confidence: 0.8,
    };
    let experience = Experience {
        action_sequence: vec![],
        reward: 1.0,
        intrinsic_reward: 0.1,
        initial_metrics: PerformanceMetrics {
            accuracy: 0.0,
            response_time: 0.0,
            speed: "unknown".to_string(),
        },
        final_metrics: PerformanceMetrics {
            accuracy: 0.0,
            response_time: 0.0,
            speed: "unknown".to_string(),
        },
        initial_emotional_state: emotional_state.clone(),
        final_emotional_state: emotional_state.clone(),
        timestamp: chrono::Utc::now(),
        pattern_confidence: 0.8,
        embedding: vec![0.5; 32],
        timescale: TimeScale::ShortTerm,
    };
    let context = RichContext {
        action: Some(Action {
            temp_id: "test".to_string(),
            verb: "analyse".to_string(),
            confidence: 0.9,
            metadata: None,
        }),
        event: None,
        timestamp: Utc::now(),
        outcome: InteractionOutcome {
            success: true,
            quality_score: 0.85,
            feedback: "Good".to_string(),
            metadata: serde_json::json!({}),
        },
        semantic_embedding: vec![0.5; 32],
        metadata: HashMap::new(),
    };
    index.index_experience(0, &experience, &context);
    let found = index.find_by_action("analyse");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0], 0);
    let similar = index.find_by_emotional_state(&emotional_state, 0.2);
    assert!(similar.contains(&0));
}
