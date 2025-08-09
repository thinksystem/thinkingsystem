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

pub async fn test_high_volume_data_processing_ui(
    enhanced_processor: &UIDataProcessor,
) -> Result<Value, Box<dyn std::error::Error>> {
    tracing::info!("Testing high-volume data processing scenarios with UI");

    let stress_scenarios = [
        json!({
            "text": "Large-scale distributed machine learning systems process terabytes of data across multiple cloud regions, utilising advanced neural networks, transformer architectures, and real-time inference pipelines for autonomous decision-making in complex multi-agent environments.",
            "entities": ["machine_learning", "terabytes", "neural_networks", "transformers", "cloud", "multi_agent"],
            "complexity": "high",
            "scenario_type": "volume_stress"
        }),
        json!({
            "text": "Quantum computing algorithms, blockchain consensus mechanisms, distributed ledger technologies, cryptographic hash functions, zero-knowledge proofs, homomorphic encryption, federated learning protocols, and edge computing architectures converge to create next-generation decentralised AI systems.",
            "entities": ["quantum_computing", "blockchain", "cryptography", "federated_learning", "edge_computing"],
            "complexity": "extreme",
            "scenario_type": "complexity_stress"
        }),
    ];

    let mut results = Vec::new();
    let start_time = Instant::now();

    for (idx, scenario) in stress_scenarios.iter().enumerate() {
        let scenario_start = Instant::now();
        match enhanced_processor.process_data(scenario).await {
            Ok(result) => {
                let processing_time = scenario_start.elapsed();
                results.push(json!({
                    "scenario_id": idx,
                    "success": true,
                    "processing_time_ms": processing_time.as_millis(),
                    "entities_extracted": result.get("entities").map(|e| e.as_array().map(|a| a.len()).unwrap_or(0)),
                    "complexity": scenario["complexity"],
                    "scenario_type": scenario["scenario_type"]
                }));
                tracing::info!(
                    "✓ High-volume scenario {} with UI completed in {}ms",
                    idx,
                    processing_time.as_millis()
                );
            }
            Err(e) => {
                results.push(json!({
                    "scenario_id": idx,
                    "success": false,
                    "error": e.to_string(),
                    "scenario_type": scenario["scenario_type"]
                }));
                tracing::warn!("✗ High-volume scenario {} failed: {}", idx, e);
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    let total_time = start_time.elapsed();
    let success_rate = results
        .iter()
        .filter(|r| r["success"].as_bool().unwrap_or(false))
        .count() as f64
        / results.len() as f64;

    Ok(json!({
        "test_type": "high_volume_data_processing_ui",
        "total_scenarios": results.len(),
        "success_rate": success_rate,
        "total_time_ms": total_time.as_millis(),
        "avg_time_per_scenario_ms": total_time.as_millis() / results.len() as u128,
        "ui_enabled": true,
        "results": results
    }))
}

pub async fn test_edge_case_scenarios_ui(
    enhanced_processor: &UIDataProcessor,
    enhanced_verifier: &UIIdentityVerifier,
) -> Result<Value, Box<dyn std::error::Error>> {
    tracing::info!("Testing edge case scenarios with UI");

    let edge_cases = [
        json!({
            "text": "",
            "entities": [],
            "source_id": "urn:empty:test",
            "case_type": "empty_data"
        }),
        json!({
            "text": "AI系统处理数据 🤖 with émoticons and специальные symbols ñ € ∑ ∆",
            "entities": ["AI", "data", "symbols"],
            "source_id": "urn:unicode:test",
            "case_type": "unicode_text"
        }),
    ];

    let mut results = Vec::new();

    for (idx, case) in edge_cases.iter().enumerate() {
        let case_start = Instant::now();
        let mut case_result = json!({
            "case_id": idx,
            "case_type": case["case_type"],
            "data_processing": null,
            "identity_verification": null
        });

        match enhanced_processor.process_data(case).await {
            Ok(result) => {
                case_result["data_processing"] = json!({
                    "success": true,
                    "entities_found": result.get("entities").map(|e| e.as_array().map(|a| a.len()).unwrap_or(0)),
                    "processing_time_ms": case_start.elapsed().as_millis()
                });
            }
            Err(e) => {
                case_result["data_processing"] = json!({
                    "success": false,
                    "error": e.to_string()
                });
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        if case.get("source_id").is_some() {
            match enhanced_verifier.verify_source(case).await {
                Ok(result) => {
                    case_result["identity_verification"] = json!({
                        "success": true,
                        "trust_score": result.get("trust_score"),
                        "status": result.get("status")
                    });
                }
                Err(e) => {
                    case_result["identity_verification"] = json!({
                        "success": false,
                        "error": e.to_string()
                    });
                }
            }
        }

        results.push(case_result);
        tracing::info!(
            "Edge case {} ({}) completed with UI",
            idx,
            case["case_type"]
        );

        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    }

    Ok(json!({
        "test_type": "edge_case_scenarios_ui",
        "total_cases": results.len(),
        "ui_enabled": true,
        "results": results
    }))
}
