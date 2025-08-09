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
use stele::memory::memory_components::TimeScale;
use stele::memory::memory_components::{Experience, MemoryConfig};
use stele::memory::persistence::MemoryPersistence;
use stele::nlu::orchestrator::data_models::Action;
use stele::scribes::{EmotionalState, PerformanceMetrics};

#[test]
fn test_save_and_load() {
    let experiences = vec![(
        0,
        Experience {
            action_sequence: vec![Action {
                temp_id: "test".to_string(),
                verb: "test_action".to_string(),
                confidence: 0.9,
                metadata: None,
            }],
            reward: 1.0,
            intrinsic_reward: 0.1,
            initial_metrics: PerformanceMetrics {
                accuracy: 0.8,
                response_time: 150.0,
                speed: "fast".to_string(),
            },
            final_metrics: PerformanceMetrics {
                accuracy: 0.9,
                response_time: 120.0,
                speed: "fast".to_string(),
            },
            initial_emotional_state: EmotionalState {
                valence: 0.5,
                arousal: 0.5,
                dominance: 0.5,
                confidence: 0.8,
            },
            final_emotional_state: EmotionalState {
                valence: 0.6,
                arousal: 0.4,
                dominance: 0.5,
                confidence: 0.85,
            },
            timestamp: Utc::now(),
            pattern_confidence: 0.8,
            embedding: vec![0.1; 32],
            timescale: TimeScale::ShortTerm,
        },
    )];
    let patterns = vec![];
    let rich_contexts = vec![];
    let config = MemoryConfig::default();
    let temp_path = "/tmp/test_memory.json";
    MemoryPersistence::save_to_file(temp_path, &experiences, &patterns, &rich_contexts, &config)
        .unwrap();
    let loaded = MemoryPersistence::load_from_file(temp_path).unwrap();
    assert_eq!(loaded.experiences.len(), 1);
    assert_eq!(loaded.version, 1);
}
