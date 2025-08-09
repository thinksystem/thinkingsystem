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

use crate::local_llm_interface::LocalLLMInterface;
use crate::scenario_generator::InteractiveScenarioGenerator;
use crate::ui::core::{ScribeUI, UIBridge};
use crate::ui::wrappers::*;
use crate::{
    cli::Args, demo_processor::DemoDataProcessor, identity::EnhancedIdentityVerifier,
    llm_logging::LLMLogger, logging_adapter::LoggingLLMAdapter,
};
use chrono::Utc;
use dotenvy::dotenv;
use eframe::egui;
use serde_json::json;
use std::sync::Arc;
use steel::IdentityProvider;
use stele::llm::unified_adapter::UnifiedLLMAdapter;
use stele::scribes::core::q_learning_core::QLearningCore;
use stele::scribes::specialists::KnowledgeScribe;
use surrealdb::{engine::any::Any, Surreal};
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

struct StartupArgs {
    args: Args,
    ui_bridge: Arc<UIBridge>,
    llm_logger: Arc<LLMLogger>,
}

pub struct ScribesDemoApp {
    ui: ScribeUI,
    core_logic_handle: Option<tokio::task::JoinHandle<()>>,
    runtime_handle: tokio::runtime::Handle,

    startup_args: Option<StartupArgs>,
}

impl ScribesDemoApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        args: Args,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        let (ui_bridge, message_receiver, llm_receiver) = UIBridge::new();
        let ui_bridge = Arc::new(ui_bridge);

        let session_id = format!("session_{}", Utc::now().format("%Y%m%d_%H%M%S"));
        let llm_logger = Arc::new(LLMLogger::new(
            "logs/llm_interactions.jsonl",
            session_id.clone(),
            true,
        ));

        let temp_unified_adapter = runtime_handle.block_on(async {
            UnifiedLLMAdapter::with_preferences("ollama", "llama3.2")
                .await
                .unwrap_or_else(|_| {
                    runtime_handle.block_on(async {
                        UnifiedLLMAdapter::with_defaults()
                            .await
                            .expect("Failed to create default unified adapter")
                    })
                })
        });
        let temp_local_llm_interface = Arc::new(Mutex::new(LocalLLMInterface::new(Arc::new(
            temp_unified_adapter,
        ))));
        let scenario_generator =
            Arc::new(InteractiveScenarioGenerator::new(temp_local_llm_interface));

        let ui = ScribeUI::new(
            message_receiver,
            llm_receiver,
            runtime_handle.clone(),
            scenario_generator.clone(),
        );

        let startup_args = StartupArgs {
            args,
            ui_bridge,
            llm_logger,
        };

        Self {
            ui,
            core_logic_handle: None,
            runtime_handle,
            startup_args: Some(startup_args),
        }
    }
}

impl eframe::App for ScribesDemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui.update(ctx);

        if let Some(scenario_data) = self.ui.should_start_demo() {
            if self.core_logic_handle.is_none() {
                if let Some(startup_args) = self.startup_args.take() {
                    info!("Starting core logic task...");

                    let core_logic_handle = self.runtime_handle.spawn(async move {
                        if let Err(e) = run_core_logic(startup_args, scenario_data).await {
                            error!("Core logic task failed: {}", e);
                        }
                    });

                    self.core_logic_handle = Some(core_logic_handle);
                }
            }
        }

        if let Some(handle) = &self.core_logic_handle {
            if handle.is_finished() {
                egui::Window::new("System Status")
                    .id(egui::Id::new("status_window"))
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label("Operations completed successfully.");
                        ui.label("All SCRIBE processes have been executed.");
                        ui.label("You can continue to observe the UI or close the application.");
                    });
            }
        }

        ctx.request_repaint();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        info!("Scribes application shutdown initiated.");

        if let Some(handle) = self.core_logic_handle.take() {
            handle.abort();
            info!("Aborted running core logic task.");
        }
    }
}

pub fn setup_logging(log_level_str: &str) -> tracing_appender::non_blocking::WorkerGuard {
    let file_appender = RollingFileAppender::new(Rotation::DAILY, "logs", "scribes-app.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let console_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level_str));
    let file_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level_str));

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .with_filter(console_filter),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_filter(file_filter),
        )
        .init();

    info!("Logging initialised with level: {}", log_level_str);
    guard
}

struct AppEnvironment {
    llm_adapter: Arc<LoggingLLMAdapter>,
    db: Surreal<Any>,
    iam_provider: Arc<IdentityProvider>,
}

async fn setup_app_environment(
    llm_logger: Arc<LLMLogger>,
) -> Result<AppEnvironment, Box<dyn std::error::Error + Send + Sync>> {
    info!("--- Initialising STELE Scribes Environment ---");
    dotenv().ok();

    let llm_adapter = match LoggingLLMAdapter::anthropic(Arc::clone(&llm_logger)) {
        Ok(adapter) => {
            info!("Successfully initialised Anthropic LLM adapter.");
            Arc::new(adapter)
        }
        Err(e) => {
            warn!("Anthropic adapter failed: {}, trying OpenAI.", e);
            match LoggingLLMAdapter::openai(Arc::clone(&llm_logger)) {
                Ok(adapter) => {
                    info!("Successfully initialised OpenAI LLM adapter.");
                    Arc::new(adapter)
                }
                Err(e2) => {
                    error!(
                        "All LLM adapters failed to initialise: Anthropic: {}, OpenAI: {}",
                        e, e2
                    );
                    return Err("No LLM provider available".into());
                }
            }
        }
    };

    let db = surrealdb::engine::any::connect("mem://").await?;
    db.use_ns("app_namespace").use_db("scribes").await?;
    info!("Successfully initialised in-memory SurrealDB.");

    let iam_provider = Arc::new(IdentityProvider::new().await?);
    info!("Successfully initialised STEEL IAM provider.");

    Ok(AppEnvironment {
        llm_adapter,
        db,
        iam_provider,
    })
}

async fn run_core_logic(
    startup_args: StartupArgs,
    scenario_data: Option<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let StartupArgs {
        ui_bridge,
        llm_logger,
        ..
    } = startup_args;

    let env = setup_app_environment(llm_logger.clone()).await?;

    let unified_llm_adapter = Arc::new(
        UnifiedLLMAdapter::with_preferences("ollama", "llama3.2")
            .await
            .expect("Failed to initialise unified LLM adapter"),
    );
    let local_llm_interface = Arc::new(Mutex::new(LocalLLMInterface::new(unified_llm_adapter)));

    let enhanced_data_processor = Arc::new(
        DemoDataProcessor::new(
            Arc::new(
                UnifiedLLMAdapter::with_preferences("ollama", "llama3.2")
                    .await
                    .expect("Failed to initialise unified LLM adapter for data processor"),
            ),
            Some(env.llm_adapter.clone()),
            local_llm_interface.clone(),
            Arc::new(env.db.clone()),
            llm_logger.clone(),
        )
        .await?
        .with_ui_bridge(ui_bridge.clone()),
    );
    let enhanced_identity_verifier = Arc::new(EnhancedIdentityVerifier::new(
        env.iam_provider.clone(),
        llm_logger.clone(),
    ));

    let knowledge_scribe = KnowledgeScribe::new("knowledge_specialist".to_string());
    let mut ui_knowledge_scribe = UIKnowledgeScribe::new(knowledge_scribe, ui_bridge.clone());
    let ui_data_processor =
        UIDataProcessor::new(enhanced_data_processor.clone(), ui_bridge.clone());
    let ui_identity_verifier =
        UIIdentityVerifier::new(enhanced_identity_verifier.clone(), ui_bridge.clone());

    let q_learning_core = QLearningCore::new(10, 4, 0.1, 0.99, 0.1, 1000);
    let mut ui_q_learning =
        crate::ui::wrappers::UIQLearning::new(q_learning_core, ui_bridge.clone());

    info!("All systems initialised. Awaiting scenario data to process.");

    if let Some(scenario_text) = scenario_data {
        info!("Processing scenario provided by user.");

        let scenarios = parse_scenarios_from_text(&scenario_text)?;
        info!(
            "=== Processing {} Scenarios with STELE Scribes ===",
            scenarios.len()
        );

        for (index, scenario) in scenarios.iter().enumerate() {
            let scenario_num = index + 1;
            info!(
                "--- Processing Scenario {} of {} ---",
                scenario_num,
                scenarios.len()
            );
            info!("Name: {}", scenario.name);
            info!("Description: {}", scenario.description);

            let content_to_process = extract_content_from_scenario(scenario);
            let delay = tokio::time::Duration::from_millis(300);

            let data_results = process_scenario_with_data_scribe(
                &ui_data_processor,
                scenario,
                &content_to_process,
            )
            .await?;
            tokio::time::sleep(delay).await;

            let knowledge_results = process_scenario_with_knowledge_scribe(
                &mut ui_knowledge_scribe,
                scenario,
                &content_to_process,
                &data_results,
            )
            .await?;
            tokio::time::sleep(delay).await;

            let identity_results =
                process_scenario_with_identity_scribe(&ui_identity_verifier, scenario).await?;
            tokio::time::sleep(delay).await;

            let coordination_results = coordinate_scribes_for_scenario(
                scenario,
                &data_results,
                &knowledge_results,
                &identity_results,
            )
            .await?;

            process_scenario_with_learning_system(
                &mut ui_q_learning,
                scenario,
                &coordination_results,
            )
            .await?;

            info!("Scenario '{}' processing complete.", scenario.name);
            if index < scenarios.len() - 1 {
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            }
        }
        info!(
            "=== All {} Scenarios Processing Complete ===",
            scenarios.len()
        );
    } else {
        warn!("Core logic started but no scenario data was provided. The system will remain idle.");
    }

    info!("=== STELE Scribes Core Logic Task Completed ===");
    info!(session_id = %llm_logger.session_id(), "Session processing complete.");

    Ok(())
}

#[derive(Debug, Clone)]
struct ParsedScenario {
    name: String,
    description: String,
    category: String,
    priority: String,
    data: serde_json::Value,
    expected_outcome: serde_json::Value,
}

fn parse_scenarios_from_text(text: &str) -> Result<Vec<ParsedScenario>, String> {
    let mut scenarios = Vec::new();
    let scenario_blocks: Vec<&str> = text.split("=== SCENARIO").collect();

    for (index, block) in scenario_blocks.iter().enumerate() {
        if index == 0 || block.trim().is_empty() {
            continue;
        }

        let mut name = "Unknown Scenario".to_string();
        let mut description = "No description".to_string();
        let mut category = "general".to_string();
        let mut priority = "medium".to_string();
        let mut data = serde_json::Value::Null;
        let mut expected_outcome = serde_json::Value::Null;

        let mut current_section = "";
        let mut json_buffer = String::new();
        let mut in_json = false;
        let mut brace_count = 0;

        for line in block.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if !in_json {
                if line.starts_with("Name:") {
                    name = line.trim_start_matches("Name:").trim().to_string();
                } else if line.starts_with("Description:") {
                    description = line.trim_start_matches("Description:").trim().to_string();
                } else if line.starts_with("Category:") {
                    category = line.trim_start_matches("Category:").trim().to_string();
                } else if line.starts_with("Priority:") {
                    priority = line.trim_start_matches("Priority:").trim().to_string();
                } else if line.starts_with("Data:") {
                    current_section = "data";
                } else if line.starts_with("Outcome:") {
                    current_section = "outcome";
                } else if line.starts_with('{') {
                    in_json = true;
                    brace_count = 0;
                    json_buffer.clear();
                }
            }
            if in_json {
                json_buffer.push_str(line);
                brace_count += line.chars().filter(|&c| c == '{').count();
                brace_count -= line.chars().filter(|&c| c == '}').count();

                if brace_count == 0 && !json_buffer.is_empty() {
                    if let Ok(parsed) = serde_json::from_str(&json_buffer) {
                        if current_section == "data" {
                            data = parsed;
                        } else if current_section == "outcome" {
                            expected_outcome = parsed;
                        }
                    } else {
                        warn!(
                            "Failed to parse JSON in section '{}': {}",
                            current_section, json_buffer
                        );
                    }
                    in_json = false;
                    json_buffer.clear();
                }
            }
        }
        scenarios.push(ParsedScenario {
            name,
            description,
            category,
            priority,
            data,
            expected_outcome,
        });
    }

    if scenarios.is_empty() {
        return Err("No valid scenarios found in the provided text.".to_string());
    }
    Ok(scenarios)
}

fn extract_content_from_scenario(scenario: &ParsedScenario) -> String {
    if let Some(text) = scenario.data.get("text").and_then(|v| v.as_str()) {
        text.to_string()
    } else if let Some(content) = scenario.data.get("content").and_then(|v| v.as_str()) {
        content.to_string()
    } else if let Some(entities) = scenario.data.get("entities").and_then(|v| v.as_array()) {
        let entity_names: Vec<String> = entities
            .iter()
            .filter_map(|e| e.as_str().map(String::from))
            .collect();
        format!("Analysis of entities: {}", entity_names.join(", "))
    } else {
        format!("{}: {}", scenario.name, scenario.description)
    }
}

fn extract_entities_from_text(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|word| {
            !word.is_empty() && word.chars().next().unwrap().is_uppercase() && word.len() > 2
        })
        .map(String::from)
        .collect()
}

async fn process_scenario_with_data_scribe(
    data_processor: &UIDataProcessor,
    scenario: &ParsedScenario,
    content: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let context = json!({"text": content, "source": scenario.name.clone()});
    let result = data_processor.process_data(&context).await?;
    Ok(result.to_string())
}

async fn process_scenario_with_knowledge_scribe(
    knowledge_scribe: &mut UIKnowledgeScribe,
    scenario: &ParsedScenario,
    content: &str,
    _data_results: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let entities = if let Some(arr) = scenario.data.get("entities").and_then(|v| v.as_array()) {
        arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    } else {
        extract_entities_from_text(content)
    };
    let context = json!({"entities": entities, "content": content});
    let result = knowledge_scribe.link_data_to_graph(&context).await?;
    Ok(result.to_string())
}

async fn process_scenario_with_identity_scribe(
    identity_verifier: &UIIdentityVerifier,
    scenario: &ParsedScenario,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let source_id = scenario
        .data
        .get("source_id")
        .and_then(|v| v.as_str())
        .unwrap_or("urn:user:interactive:session");
    let context = json!({"source_id": source_id, "content_hash": format!("{:x}", md5::compute(scenario.description.as_bytes()))});
    let result = identity_verifier.verify_source(&context).await?;
    Ok(result.to_string())
}

async fn coordinate_scribes_for_scenario(
    scenario: &ParsedScenario,
    data_results: &str,
    knowledge_results: &str,
    identity_results: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let final_analysis = format!(
        "Comprehensive Analysis of Scenario: '{}'\n  Data Insights: {}\n  Knowledge Extracted: {}\n  Identity Status: {}",
        scenario.name, data_results, knowledge_results, identity_results
    );
    info!("Coordination complete for scenario '{}'", scenario.name);
    Ok(final_analysis)
}

async fn process_scenario_with_learning_system(
    q_learning: &mut crate::ui::wrappers::UIQLearning,
    scenario: &ParsedScenario,
    coordination_results: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let state = match scenario.priority.as_str() {
        "low" => 0,
        "medium" => 1,
        "high" => 2,
        "critical" => 3,
        _ => 1,
    };

    let category_action = match scenario.category.as_str() {
        "data_processing" => 0,
        "knowledge_extraction" => 1,
        "identity_verification" => 2,
        "coordination" => 3,
        _ => 0,
    };

    let valid_actions = [0, 1, 2, 3];
    let chosen_action = q_learning.choose_action(state, &valid_actions);

    let reward = if coordination_results.contains("success") || coordination_results.len() > 50 {
        0.8
    } else {
        0.3
    };

    let next_state = if chosen_action == category_action {
        (state + 1) % 4
    } else {
        state
    };

    q_learning.add_experience(state, chosen_action, reward, next_state);

    q_learning.update_q_values();

    Ok(format!(
        "Learning System processed scenario '{}': state={}, action={}, reward={:.2}, next_state={}",
        scenario.name, state, chosen_action, reward, next_state
    ))
}
