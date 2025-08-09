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

use crate::demo_processor::DemoDataProcessor;
use crate::identity::EnhancedIdentityVerifier;
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use stele::scribes::core::q_learning_core::QLearningCore;
use stele::scribes::specialists::KnowledgeScribe;
use tracing::{info, warn};

pub async fn test_enhanced_ecosystem_integration(
    enhanced_processor: &Arc<DemoDataProcessor>,
    enhanced_verifier: &Arc<EnhancedIdentityVerifier>,
    knowledge_scribe: &mut KnowledgeScribe,
    q_learning: &mut QLearningCore,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    info!("=== Testing Enhanced Ecosystem Integration ===");

    let integration_scenario = json!({
        "entities": ["autonomous_research_system", "ai_coordination", "enhanced_integration"],
        "text": "Demonstrating comprehensive integration of real LLM processing, database storage, IAM verification, and machine learning adaptation in a coordinated multi-agent cognitive system with full observability and logging.",
        "source_id": "urn:stele:log:1138",
        "urgency": 0.9
    });

    let start_time = Instant::now();
    let mut successful_operations = 0;
    let mut operation_results = Vec::new();

    info!("Enhanced identity verification phase");
    if let Ok(identity_result) = enhanced_verifier.verify_source(&integration_scenario).await {
        successful_operations += 1;
        operation_results.push(json!({
            "operation": "enhanced_identity_verification",
            "success": true,
            "result": identity_result
        }));
        info!("✓ Enhanced identity verification successful");
    } else {
        operation_results.push(json!({
            "operation": "enhanced_identity_verification",
            "success": false
        }));
        warn!("✗ Enhanced identity verification failed");
    }

    info!("Enhanced data processing phase");
    if let Ok(data_result) = enhanced_processor.process_data(&integration_scenario).await {
        successful_operations += 1;
        operation_results.push(json!({
            "operation": "enhanced_data_processing",
            "success": true,
            "result": data_result
        }));
        info!("✓ Enhanced data processing successful");

        if let Ok(storage_result) = enhanced_processor.store_extracted_data(&data_result).await {
            successful_operations += 1;
            operation_results.push(json!({
                "operation": "enhanced_data_storage",
                "success": true,
                "result": storage_result
            }));
            info!("✓ Enhanced data storage successful");
        }
    } else {
        operation_results.push(json!({
            "operation": "enhanced_data_processing",
            "success": false
        }));
        warn!("✗ Enhanced data processing failed");
    }

    info!("Knowledge graph integration phase");
    if let Ok(knowledge_result) = knowledge_scribe
        .link_data_to_graph(&integration_scenario)
        .await
    {
        successful_operations += 1;
        operation_results.push(json!({
            "operation": "knowledge_graph_integration",
            "success": true,
            "result": knowledge_result
        }));
        info!("✓ Knowledge graph integration successful");
    } else {
        operation_results.push(json!({
            "operation": "knowledge_graph_integration",
            "success": false
        }));
        warn!("✗ Knowledge graph integration failed");
    }

    info!("Q-learning adaptation phase");
    if successful_operations > 0 {
        let state = successful_operations as usize % 10;
        let action = q_learning.choose_action(state, &[0, 1, 2, 3]);
        let reward = if successful_operations >= 3 { 1.0 } else { 0.5 };
        let next_state = (state + 1) % 10;

        q_learning.add_experience(state, action, reward, next_state);
        q_learning.update_q_values();

        operation_results.push(json!({
            "operation": "q_learning_adaptation",
            "success": true,
            "state": state,
            "action": action,
            "reward": reward,
            "next_state": next_state
        }));

        info!(
            state = state,
            action = action,
            reward = reward,
            "✓ Q-learning adaptation completed based on {} successful operations",
            successful_operations
        );
    }

    let total_time = start_time.elapsed();

    info!(
        successful_operations = successful_operations,
        total_time_ms = total_time.as_millis(),
        "Enhanced ecosystem integration test completed"
    );

    Ok(json!({
        "total_time_ms": total_time.as_millis(),
        "successful_operations": successful_operations,
        "operation_results": operation_results,
        "integration_score": (successful_operations as f64 / 5.0) * 100.0,
        "enhanced_features": [
            "real_llm_processing",
            "comprehensive_logging",
            "database_storage",
            "iam_verification",
            "q_learning_adaptation",
            "knowledge_graph_integration",
            "entity_extraction",
            "structured_analysis"
        ],
        "demo_metadata": {
            "session_timestamp": Utc::now(),
            "integration_scenario": integration_scenario
        }
    }))
}
