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

use crate::cli::Args;
use crate::identity::EnhancedIdentityVerifier;
use crate::setup::{initialise_system, setup_logging, SystemConfig};
use crate::tests::coordination::test_multi_specialist_coordination;
use crate::tests::data::test_data_specialist;
use crate::tests::enhanced::{
    test_enhanced_data_processor, test_enhanced_identity_verifier,
    test_enhanced_multi_specialist_coordination,
};
use crate::tests::identity::test_identity_specialist;
use crate::tests::integration::test_enhanced_ecosystem_integration;
use crate::tests::knowledge::test_knowledge_specialist;
use crate::tests::learning::{test_learning_system, test_q_learning_api};
use dotenvy::dotenv;
use std::sync::Arc;
use stele::scribes::core::q_learning_core::QLearningCore;
use stele::scribes::replay_buffer::{ReplayBuffer, ReplayBufferConfig};
use stele::scribes::scriptorium::learning_system::LearningSystem;
use stele::scribes::specialists::{DataScribe, IdentityScribe, KnowledgeScribe};
use tracing::info;

pub fn run_with_gui(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    use crate::ui::ScribesDemoApp;

    dotenv().ok();

    let _log_guard = setup_logging(args.log_level.clone(), args.trace)?;

    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

    let handle = runtime.handle().clone();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };

    let result = eframe::run_native(
        "STELE Scribes Demo with UI",
        options,
        Box::new(move |cc| Ok(Box::new(ScribesDemoApp::new(cc, args, handle)))),
    );

    drop(runtime);

    result.map_err(|e| format!("Failed to run egui app: {e}").into())
}

pub async fn run_without_gui(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let _log_guard = setup_logging(args.log_level.clone(), args.trace)?;

    let SystemConfig {
        session_id,
        llm_logger: _llm_logger,
        llm_adapter,
        logging_adapter: _logging_adapter,
        iam_provider,
        local_llm_interface: _local_llm_interface,
        enhanced_data_processor,
    } = initialise_system().await?;

    let enhanced_identity_verifier = Arc::new(EnhancedIdentityVerifier::new(
        Arc::clone(&iam_provider),
        Arc::clone(&_llm_logger),
    ));

    info!("--- Initialising Specialist Scribes ---");
    let mut knowledge_scribe = KnowledgeScribe::new("enhanced_knowledge_specialist".to_string());
    let _data_scribe = DataScribe::new("enhanced_data_specialist".to_string(), "").await?;
    let _identity_scribe = IdentityScribe::new("enhanced_identity_specialist".to_string());

    info!("--- Initialising Learning Systems ---");
    let replay_config = ReplayBufferConfig {
        capacity: 1000,
        priority_sample_ratio: 0.4,
        temporal_sample_ratio: 0.3,
    };
    let mut replay_buffer = ReplayBuffer::new(replay_config);
    let mut q_learning = QLearningCore::new(10, 4, 0.1, 0.99, 0.1, 1000);
    let mut learning_system = LearningSystem::new();

    info!("All enhanced systems initialised successfully");

    info!("=== Running Enhanced Demo Tests ===");

    let knowledge_results = test_knowledge_specialist(&mut knowledge_scribe).await?;
    let data_results = test_data_specialist(&enhanced_data_processor).await?;
    let identity_results = test_identity_specialist(&enhanced_identity_verifier).await?;
    let coordination_results = test_multi_specialist_coordination(
        &mut knowledge_scribe,
        &enhanced_data_processor,
        &enhanced_identity_verifier,
    )
    .await?;
    let enhanced_data_results = test_enhanced_data_processor(&enhanced_data_processor).await?;
    let enhanced_identity_results =
        test_enhanced_identity_verifier(&enhanced_identity_verifier).await?;
    let enhanced_coordination_results = test_enhanced_multi_specialist_coordination(
        &mut knowledge_scribe,
        &enhanced_data_processor,
        &enhanced_identity_verifier,
    )
    .await?;
    let learning_results = test_q_learning_api(&mut q_learning, &mut replay_buffer).await?;
    let system_results = test_learning_system(&mut learning_system).await?;
    let enhanced_integration_results = test_enhanced_ecosystem_integration(
        &enhanced_data_processor,
        &enhanced_identity_verifier,
        &mut knowledge_scribe,
        &mut q_learning,
    )
    .await?;

    info!("=== Enhanced STELE Scribes Demo Completed Successfully ===");
    info!(?knowledge_results, "Knowledge Specialist Results");
    info!(?data_results, "Data Specialist Results (Legacy)");
    info!(?identity_results, "Identity Specialist Results (Legacy)");
    info!(
        ?coordination_results,
        "Multi-Specialist Coordination Results (Legacy)"
    );
    info!(?enhanced_data_results, "Enhanced Data Processor Results");
    info!(
        ?enhanced_identity_results,
        "Enhanced Identity Verifier Results"
    );
    info!(
        ?enhanced_coordination_results,
        "Enhanced Coordination Results"
    );
    info!(?learning_results, "Learning System Results");
    info!(?system_results, "System Integration Results");
    info!(
        ?enhanced_integration_results,
        "Enhanced Integration Results"
    );

    info!(session_id = %session_id, "Demo session completed successfully");
    info!("LLM interaction logs saved to: logs/llm_interactions.jsonl");
    info!("System logs saved to: logs/scribes-demo.log.*");

    Ok(())
}
