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

use crate::ui::wrappers::{UIDataProcessor, UIIdentityVerifier, UIKnowledgeScribe, UIQLearning};
use serde_json::{json, Value};
use std::time::Instant;
use stele::scribes::replay_buffer::ReplayBuffer;

pub async fn test_knowledge_specialist_ui(
    knowledge_scribe: &mut UIKnowledgeScribe,
) -> Result<Value, String> {
    tracing::debug!("Testing KnowledgeScribe API with UI");
    let test_scenarios = [
        json!({"entities": ["A", "B"], "content": "A relates to B"}),
        json!({"entities": ["C", "D"], "content": "C relates to D"}),
    ];
    let mut entities_processed = 0;
    for scenario in test_scenarios.iter() {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if let Ok(result) = knowledge_scribe.link_data_to_graph(scenario).await {
            tracing::debug!("link_data_to_graph result: {}", result);
            entities_processed += 1;
        }
    }
    Ok(json!({ "entities_processed": entities_processed }))
}

pub async fn test_data_specialist_ui(
    enhanced_processor: &UIDataProcessor,
) -> Result<Value, String> {
    tracing::debug!("Testing Enhanced DataProcessor with UI");
    let test_context = json!({"text": "Some data to process.", "urgency": 0.5});
    let mut records_processed = 0;
    let start_time = Instant::now();

    if let Ok(result) = enhanced_processor.process_data(&test_context).await {
        tracing::debug!("process_data result: {}", result);
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if let Ok(_storage_result) = enhanced_processor.store_extracted_data(&result).await {
            records_processed += 1;
        }
    }
    let processing_time_ms = start_time.elapsed().as_millis();
    Ok(json!({ "records_processed": records_processed, "processing_time_ms": processing_time_ms }))
}

pub async fn test_identity_specialist_ui(
    enhanced_verifier: &UIIdentityVerifier,
) -> Result<Value, String> {
    tracing::debug!("Testing Enhanced IdentityVerifier with UI");
    let context_to_verify = json!({"source_id": "urn:stele:log:1138"});
    let mut identities_verified = 0;

    if let Ok(_result) = enhanced_verifier.verify_source(&context_to_verify).await {
        identities_verified += 1;
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let link_context = json!({"source_id": "id_1", "target_id": "id_2"});
    if let Ok(_result) = enhanced_verifier.link_identities(&link_context).await {}
    Ok(json!({ "identities_verified": identities_verified }))
}

pub async fn test_q_learning_api_ui(
    q_learning: &mut UIQLearning,
    _replay_buffer: &mut ReplayBuffer,
) -> Result<Value, Box<dyn std::error::Error>> {
    tracing::debug!("Testing QLearningCore API with UI");
    let state = 1;
    let valid_actions = vec![0, 1, 2, 3];
    let reward = 1.0;
    let next_state = 2;
    let mut updates_performed = 0;

    let action = q_learning.choose_action(state, &valid_actions);
    tracing::debug!(
        "API call: choose_action({}, {:?}) -> {}",
        state,
        valid_actions,
        action
    );

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    q_learning.add_experience(state, action, reward, next_state);
    tracing::debug!(
        "API call: add_experience({}, {}, {}, {})",
        state,
        action,
        reward,
        next_state
    );

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    q_learning.update_q_values();
    tracing::debug!("API call: update_q_values()");
    updates_performed += 1;

    Ok(json!({ "q_learning_updates_performed": updates_performed }))
}
