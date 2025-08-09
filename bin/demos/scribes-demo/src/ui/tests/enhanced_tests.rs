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

use crate::ui::wrappers::{UIDataProcessor, UIIdentityVerifier};
use serde_json::{json, Value};
use std::time::Instant;

pub async fn test_enhanced_data_processor_ui(
    enhanced_processor: &UIDataProcessor,
) -> Result<Value, Box<dyn std::error::Error>> {
    tracing::info!("=== Testing Enhanced Data Processor with UI ===");

    let test_scenarios = vec![
        json!({
            "text": "The multi-agent cognitive system demonstrates emergent behaviours through coordinated decision-making processes involving distributed neural networks and reinforcement learning algorithms.",
            "urgency": 0.8
        }),
        json!({
            "text": "Advanced natural language processing capabilities enable sophisticated understanding of complex technical documentation and research papers.",
            "urgency": 0.6
        }),
        json!({
            "text": "Integration of large language models with knowledge graphs facilitates enhanced information retrieval and semantic reasoning.",
            "urgency": 0.4
        }),
    ];

    let mut total_records_processed = 0;
    let mut total_processing_time = 0u128;
    let mut llm_calls_made = 0;
    let start_time = Instant::now();

    for (i, scenario) in test_scenarios.iter().enumerate() {
        tracing::info!(scenario_index = i, "Processing test scenario with UI");

        let scenario_start = Instant::now();
        match enhanced_processor.process_data(scenario).await {
            Ok(result) => {
                let scenario_time = scenario_start.elapsed();
                total_processing_time += scenario_time.as_millis();
                total_records_processed += 1;

                if result["method"].as_str() == Some("llm_structured_analysis") {
                    llm_calls_made += 1;
                }

                tracing::info!(
                    scenario_index = i,
                    processing_time_ms = scenario_time.as_millis(),
                    method = %result["method"].as_str().unwrap_or("unknown"),
                    "Scenario processed successfully with UI"
                );

                if let Some(text) = scenario["text"].as_str() {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    match enhanced_processor.extract_entities(text).await {
                        Ok(entities) => {
                            tracing::debug!(scenario_index = i, entities = ?entities, "Entities extracted with UI");
                        }
                        Err(e) => {
                            tracing::warn!(scenario_index = i, error = %e, "Entity extraction failed");
                        }
                    }
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                if let Ok(_storage_result) = enhanced_processor.store_extracted_data(&result).await
                {
                    tracing::debug!(scenario_index = i, "Data stored successfully with UI");
                }
            }
            Err(e) => {
                tracing::error!(scenario_index = i, error = %e, "Scenario processing failed");
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    }

    let total_time = start_time.elapsed();

    Ok(json!({
        "total_records_processed": total_records_processed,
        "total_processing_time_ms": total_processing_time,
        "average_processing_time_ms": if total_records_processed > 0 { total_processing_time / total_records_processed as u128 } else { 0 },
        "llm_calls_made": llm_calls_made,
        "scenarios_tested": test_scenarios.len(),
        "success_rate": (total_records_processed as f64 / test_scenarios.len() as f64) * 100.0,
        "total_demo_time_ms": total_time.as_millis(),
        "enhanced_features": [
            "real_llm_processing",
            "structured_analysis",
            "entity_extraction",
            "comprehensive_logging",
            "fallback_analysis",
            "ui_visualisation"
        ]
    }))
}

pub async fn test_enhanced_identity_verifier_ui(
    enhanced_verifier: &UIIdentityVerifier,
) -> Result<Value, Box<dyn std::error::Error>> {
    tracing::info!("=== Testing Enhanced Identity Verifier with UI ===");

    let test_contexts = vec![
        json!({"source_id": "urn:stele:log:1138"}),
        json!({"source_id": "unknown_source_123"}),
        json!({"source_id": "test_user_456"}),
    ];

    let mut identities_verified = 0;
    let mut trust_scores = Vec::new();

    for (i, context) in test_contexts.iter().enumerate() {
        tracing::info!(context_index = i, context = ?context, "Testing identity verification with UI");

        match enhanced_verifier.verify_source(context).await {
            Ok(result) => {
                identities_verified += 1;

                if let Some(trust_score) = result["trust_score"].as_f64() {
                    trust_scores.push(trust_score);
                }

                tracing::info!(
                    context_index = i,
                    status = %result["status"].as_str().unwrap_or("unknown"),
                    trust_score = result["trust_score"].as_f64().unwrap_or(0.0),
                    roles = ?result["roles"],
                    "Identity verification completed with UI"
                );
            }
            Err(e) => {
                tracing::error!(context_index = i, error = %e, "Identity verification failed");
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    let link_context = json!({
        "source_id": "enhanced_system_id_1",
        "target_id": "enhanced_system_id_2"
    });

    let mut links_created = 0;
    match enhanced_verifier.link_identities(&link_context).await {
        Ok(result) => {
            links_created += 1;
            tracing::info!(result = ?result, "Identity linking completed with UI");
        }
        Err(e) => {
            tracing::error!(error = %e, "Identity linking failed");
        }
    }

    let average_trust_score = if !trust_scores.is_empty() {
        trust_scores.iter().sum::<f64>() / trust_scores.len() as f64
    } else {
        0.0
    };

    Ok(json!({
        "identities_verified": identities_verified,
        "links_created": links_created,
        "contexts_tested": test_contexts.len(),
        "average_trust_score": average_trust_score,
        "trust_scores": trust_scores,
        "enhanced_iam": true,
        "ui_integration": true,
        "features_tested": [
            "admin_bootstrapping",
            "user_creation",
            "role_assignment",
            "token_creation",
            "token_verification",
            "identity_linking",
            "ui_visualisation"
        ]
    }))
}
