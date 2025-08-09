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

use serde_json::json;
use stele::scribes::core::q_learning_core::QLearningCore;
use stele::scribes::replay_buffer::ReplayBuffer;
use stele::scribes::scriptorium::learning_system::LearningSystem;
use tracing::debug;

pub async fn test_q_learning_api(
    q_learning: &mut QLearningCore,
    _replay_buffer: &mut ReplayBuffer,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    debug!("Testing QLearningCore API");
    let state = 1;
    let valid_actions = vec![0, 1, 2, 3];
    let reward = 1.0;
    let next_state = 2;
    let mut updates_performed = 0;
    let action = q_learning.choose_action(state, &valid_actions);
    debug!(
        "API call: choose_action({}, {:?}) -> {}",
        state, valid_actions, action
    );
    q_learning.add_experience(state, action, reward, next_state);
    debug!(
        "API call: add_experience({}, {}, {}, {})",
        state, action, reward, next_state
    );
    q_learning.update_q_values();
    debug!("API call: update_q_values()");
    updates_performed += 1;
    Ok(json!({ "q_learning_updates_performed": updates_performed }))
}

pub async fn test_learning_system(
    learning_system: &mut LearningSystem,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    debug!("Testing LearningSystem API");
    use stele::scribes::discourse::{Inscription, Testament};
    use stele::scribes::StrategyVector;

    let current_strategy = StrategyVector {
        aggressiveness: 0.5,
        cooperativeness: 0.7,
    };

    let real_testament = Testament {
        canon_invoked: "CanonOfIngestionAndVerification".to_string(),
        participants: vec![
            "data_scribe_001".to_string(),
            "knowledge_scribe_001".to_string(),
            "identity_scribe_001".to_string(),
        ],
        was_successful: true,
        final_product: json!({
            "source_text": "LLM integration with comprehensive database persistence and real-time analytics",
            "verified_knowledge": "Established relationships between AI agents, LLM processing, and database systems",
            "source_trust_score": 0.95,
            "entities_extracted": ["LLM", "Database", "AI Agent", "Analytics"],
            "sentiment_analysis": {
                "sentiment": "positive",
                "confidence": 0.85
            }
        }),
        chronicle: vec![
            Inscription {
                scribe_id: "data_scribe_001".to_string(),
                action: "process_data".to_string(),
                result: Ok(json!({
                    "entities": ["LLM", "Database", "AI Agent"],
                    "relationships": ["integrates_with", "processes", "analyses"],
                    "processing_time_ms": 1250
                })),
            },
            Inscription {
                scribe_id: "knowledge_scribe_001".to_string(),
                action: "link_to_graph".to_string(),
                result: Ok(json!({
                    "graph_nodes_added": 5,
                    "relationships_created": 8,
                    "knowledge_confidence": 0.88
                })),
            },
            Inscription {
                scribe_id: "identity_scribe_001".to_string(),
                action: "verify_source".to_string(),
                result: Ok(json!({
                    "status": "Verified",
                    "trust_score": 0.95,
                    "verification_method": "enhanced_iam"
                })),
            },
        ],
    };

    let evolved_strategy = learning_system.evolve_strategy(current_strategy, &real_testament);
    debug!(
        "API call: evolve_strategy({:?}, ...) -> {:?}",
        current_strategy, evolved_strategy
    );

    Ok(json!({
        "strategy_evolution_tested": true,
        "original_strategy": {
            "aggressiveness": current_strategy.aggressiveness,
            "cooperativeness": current_strategy.cooperativeness
        },
        "evolved_strategy": {
            "aggressiveness": evolved_strategy.aggressiveness,
            "cooperativeness": evolved_strategy.cooperativeness
        },
        "testament_success": real_testament.was_successful,
        "participants_count": real_testament.participants.len(),
        "inscriptions_count": real_testament.chronicle.len()
    }))
}
