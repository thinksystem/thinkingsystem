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

use crate::ui::wrappers::{UIDataProcessor, UIIdentityVerifier, UIKnowledgeScribe};
use serde_json::{json, Value};

pub async fn test_multi_specialist_coordination_ui(
    knowledge_scribe: &mut UIKnowledgeScribe,
    enhanced_processor: &UIDataProcessor,
    enhanced_verifier: &UIIdentityVerifier,
) -> Result<Value, Box<dyn std::error::Error>> {
    tracing::debug!("Testing multi-specialist coordination with UI");
    let mut handoffs_completed = 0;
    let research_data = json!({
        "text": "Emergent Behaviours in Multi-Agent Systems",
        "source_id": "urn:stele:log:1138",
        "urgency": 0.6
    });

    if enhanced_verifier
        .verify_source(&research_data)
        .await
        .is_ok()
    {
        handoffs_completed += 1;
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        if let Ok(_data_result) = enhanced_processor.process_data(&research_data).await {
            handoffs_completed += 1;
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

            let knowledge_context = json!({
                "entities": ["multi_agent_systems"],
                "content": research_data["text"]
            });
            if knowledge_scribe
                .link_data_to_graph(&knowledge_context)
                .await
                .is_ok()
            {
                handoffs_completed += 1;
            }
        }
    }
    Ok(json!({ "handoffs_completed": handoffs_completed }))
}

pub async fn test_enhanced_multi_specialist_coordination_ui(
    knowledge_scribe: &mut UIKnowledgeScribe,
    enhanced_processor: &UIDataProcessor,
    enhanced_verifier: &UIIdentityVerifier,
) -> Result<Value, Box<dyn std::error::Error>> {
    tracing::info!("=== Testing Enhanced Multi-Specialist Coordination with UI ===");

    let coordination_scenario = json!({
        "text": "Advanced cognitive architectures enable multi-agent systems to exhibit emergent intelligence through coordinated interaction patterns and distributed decision-making processes.",
        "source_id": "urn:stele:log:1138",
        "urgency": 0.7
    });

    let mut handoffs_completed = 0;
    let start_time = std::time::Instant::now();

    tracing::info!("Step 1: Enhanced identity verification with UI");
    if let Ok(identity_result) = enhanced_verifier
        .verify_source(&coordination_scenario)
        .await
    {
        handoffs_completed += 1;
        tracing::info!(identity_result = ?identity_result, "Identity verification successful with UI");

        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        tracing::info!("Step 2: Enhanced data processing with UI");
        if let Ok(data_result) = enhanced_processor
            .process_data(&coordination_scenario)
            .await
        {
            handoffs_completed += 1;
            tracing::info!(processing_method = %data_result["method"].as_str().unwrap_or("unknown"), "Data processing successful with UI");

            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

            tracing::info!("Step 3: Knowledge graph integration with UI");
            let knowledge_context = json!({
                "entities": ["cognitive_architectures", "multi_agent_systems", "emergent_intelligence"],
                "content": coordination_scenario["text"]
            });

            if let Ok(knowledge_result) = knowledge_scribe
                .link_data_to_graph(&knowledge_context)
                .await
            {
                handoffs_completed += 1;
                tracing::info!(knowledge_result = ?knowledge_result, "Knowledge linking successful with UI");

                tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

                if let Ok(_) = enhanced_processor
                    .store_extracted_data(&json!({
                        "coordination_scenario": coordination_scenario,
                        "identity_result": identity_result,
                        "data_result": data_result,
                        "knowledge_result": knowledge_result,
                        "coordination_timestamp": chrono::Utc::now()
                    }))
                    .await
                {
                    handoffs_completed += 1;
                    tracing::info!("Coordination results stored successfully with UI");
                }
            }
        }
    }

    let total_time = start_time.elapsed();

    Ok(json!({
        "handoffs_completed": handoffs_completed,
        "max_possible_handoffs": 4,
        "coordination_success_rate": (handoffs_completed as f64 / 4.0) * 100.0,
        "total_coordination_time_ms": total_time.as_millis(),
        "enhanced_backends": true,
        "ui_integration": true,
        "coordination_features": [
            "identity_verification",
            "llm_data_processing",
            "knowledge_graph_integration",
            "result_storage",
            "comprehensive_logging",
            "ui_visualisation"
        ]
    }))
}
