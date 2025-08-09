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

use crate::data_loader::DataLoader;
use crate::demo_processor::DemoDataProcessor;
use crate::identity::EnhancedIdentityVerifier;
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use stele::scribes::specialists::KnowledgeScribe;
use tracing::{debug, error, info, warn};

pub async fn test_enhanced_data_processor(
    enhanced_processor: &Arc<DemoDataProcessor>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    info!("=== Testing Enhanced Data Processor ===");

    let test_scenarios = DataLoader::load_enhanced_data_processor_scenarios()?;

    let mut total_records_processed = 0;
    let mut total_processing_time = 0u128;
    let mut llm_calls_made = 0;
    let start_time = Instant::now();

    for (i, scenario) in test_scenarios.iter().enumerate() {
        info!(scenario_index = i, "Processing test scenario");

        let scenario_start = Instant::now();
        match enhanced_processor.process_data(scenario).await {
            Ok(result) => {
                let scenario_time = scenario_start.elapsed();
                total_processing_time += scenario_time.as_millis();
                total_records_processed += 1;

                if result["method"].as_str() == Some("llm_structured_analysis") {
                    llm_calls_made += 1;
                }

                info!(
                    scenario_index = i,
                    processing_time_ms = scenario_time.as_millis(),
                    method = %result["method"].as_str().unwrap_or("unknown"),
                    "Scenario processed successfully"
                );

                if let Some(text) = scenario["text"].as_str() {
                    match enhanced_processor.extract_entities(text).await {
                        Ok(entities) => {
                            debug!(scenario_index = i, entities = ?entities, "Entities extracted");
                        }
                        Err(e) => {
                            warn!(scenario_index = i, error = %e, "Entity extraction failed");
                        }
                    }
                }

                if let Ok(storage_result) = enhanced_processor.store_extracted_data(&result).await {
                    debug!(scenario_index = i, storage_result = ?storage_result, "Data stored successfully");
                }
            }
            Err(e) => {
                error!(scenario_index = i, error = %e, "Scenario processing failed");
            }
        }
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
            "fallback_analysis"
        ]
    }))
}

pub async fn test_enhanced_identity_verifier(
    enhanced_verifier: &Arc<EnhancedIdentityVerifier>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    info!("=== Testing Enhanced Identity Verifier ===");

    let test_contexts = DataLoader::load_identity_verifier_contexts()?;

    let mut identities_verified = 0;
    let mut trust_scores = Vec::new();

    for (i, context) in test_contexts.iter().enumerate() {
        info!(context_index = i, context = ?context, "Testing identity verification");

        match enhanced_verifier.verify_source(context).await {
            Ok(result) => {
                identities_verified += 1;

                if let Some(trust_score) = result["trust_score"].as_f64() {
                    trust_scores.push(trust_score);
                }

                info!(
                    context_index = i,
                    status = %result["status"].as_str().unwrap_or("unknown"),
                    trust_score = result["trust_score"].as_f64().unwrap_or(0.0),
                    roles = ?result["roles"],
                    "Identity verification completed"
                );
            }
            Err(e) => {
                error!(context_index = i, error = %e, "Identity verification failed");
            }
        }
    }

    let link_context = json!({
        "source_id": "enhanced_system_id_1",
        "target_id": "enhanced_system_id_2"
    });

    let mut links_created = 0;
    match enhanced_verifier.link_identities(&link_context).await {
        Ok(result) => {
            links_created += 1;
            info!(result = ?result, "Identity linking completed");
        }
        Err(e) => {
            error!(error = %e, "Identity linking failed");
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
        "features_tested": [
            "admin_bootstrapping",
            "user_creation",
            "role_assignment",
            "token_creation",
            "token_verification",
            "identity_linking"
        ]
    }))
}

pub async fn test_enhanced_multi_specialist_coordination(
    knowledge_scribe: &mut KnowledgeScribe,
    enhanced_processor: &Arc<DemoDataProcessor>,
    enhanced_verifier: &Arc<EnhancedIdentityVerifier>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    info!("=== Testing Enhanced Multi-Specialist Coordination ===");

    let coordination_scenario = json!({
        "text": "Advanced cognitive architectures enable multi-agent systems to exhibit emergent intelligence through coordinated interaction patterns and distributed decision-making processes.",
        "source_id": "urn:stele:log:1138",
        "urgency": 0.7
    });

    let mut handoffs_completed = 0;
    let start_time = Instant::now();

    info!("Step 1: Enhanced identity verification");
    if let Ok(identity_result) = enhanced_verifier
        .verify_source(&coordination_scenario)
        .await
    {
        handoffs_completed += 1;
        info!(identity_result = ?identity_result, "Identity verification successful");

        info!("Step 2: Enhanced data processing");
        if let Ok(data_result) = enhanced_processor
            .process_data(&coordination_scenario)
            .await
        {
            handoffs_completed += 1;
            info!(processing_method = %data_result["method"].as_str().unwrap_or("unknown"), "Data processing successful");

            info!("Step 3: Knowledge graph integration");
            let knowledge_context = json!({
                "entities": ["cognitive_architectures", "multi_agent_systems", "emergent_intelligence"],
                "content": coordination_scenario["text"]
            });

            if let Ok(knowledge_result) = knowledge_scribe
                .link_data_to_graph(&knowledge_context)
                .await
            {
                handoffs_completed += 1;
                info!(knowledge_result = ?knowledge_result, "Knowledge linking successful");

                if let Ok(_) = enhanced_processor
                    .store_extracted_data(&json!({
                        "coordination_scenario": coordination_scenario,
                        "identity_result": identity_result,
                        "data_result": data_result,
                        "knowledge_result": knowledge_result,
                        "coordination_timestamp": Utc::now()
                    }))
                    .await
                {
                    handoffs_completed += 1;
                    info!("Coordination results stored successfully");
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
        "coordination_features": [
            "identity_verification",
            "llm_data_processing",
            "knowledge_graph_integration",
            "result_storage",
            "comprehensive_logging"
        ]
    }))
}
