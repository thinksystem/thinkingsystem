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

#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

mod config;
mod llm;
mod modules;

use anyhow::Result;
use clap::{Arg, Command};
use config::ConfigLoader;
use llm::AdaptiveFlowOrchestrator;
use modules::FlowOrchestrator;
use std::sync::Arc;
use stele::llm::unified_adapter::UnifiedLLMAdapter;
use tracing::{error, info, warn, Level};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .with_target(false)
        .with_thread_ids(true)
        .init();

    dotenvy::dotenv().ok();

    let config_loader = ConfigLoader::new("config");

    let flows_config = config_loader
        .load_flows_config()
        .map_err(|e| anyhow::anyhow!("Failed to load flows configuration: {}", e))?;

    info!("Loaded flows configuration with LLM selection engine");

    
    let llm_adapter = Arc::new(UnifiedLLMAdapter::with_defaults().await.map_err(|e| {
        anyhow::anyhow!(
            "Failed to initialise LLM adapter with selection engine: {}",
            e
        )
    })?);

    info!("Unified LLM adapter initialised with dynamic model selection (local-first, fallback enabled)");

    let matches = Command::new("flows-demo")
        .version("1.0.0")
        .author("ThinkingSystem Team")
        .about("Demonstrates core flows and blocks orchestration using the stele engine")
        .arg(
            Arg::new("name")
                .short('n')
                .long("name")
                .value_name("NAME")
                .help("Name to analyse using real external APIs")
                .required(false)
                .default_value("kim jong moon"),
        )
        .arg(
            Arg::new("demo-count")
                .short('c')
                .long("demo-count")
                .value_name("NUMBER")
                .help("Number of different API demonstrations to run")
                .default_value("2"),
        )
        .arg(
            Arg::new("endpoint")
                .short('e')
                .long("endpoint")
                .value_name("URL")
                .help("API endpoint to explore and analyse")
                .required(false),
        )
        .arg(
            Arg::new("processing-goal")
                .long("processing-goal")
                .value_name("GOAL")
                .help(
                    "Enable LLM-powered flow negotiation with specific goal (requires --endpoint)",
                )
                .required(false),
        )
        .arg(
            Arg::new("adaptive")
                .long("adaptive")
                .action(clap::ArgAction::SetTrue)
                .help("Enable adaptive flow execution with LLM-powered error recovery")
                .required(false),
        )
        .get_matches();

    let name = matches.get_one::<String>("name").unwrap();
    let demo_count: usize = matches
        .get_one::<String>("demo-count")
        .unwrap()
        .parse()
        .unwrap_or(2);
    let endpoint = matches.get_one::<String>("endpoint");
    let processing_goal = matches.get_one::<String>("processing-goal");
    let adaptive_mode = matches.get_flag("adaptive");

    info!("Name to analyse: {}", name);
    info!("API demonstrations: {}", demo_count);

    if let Some(endpoint_url) = endpoint {
        info!("Exploring endpoint: {}", endpoint_url);

        if let Some(goal) = processing_goal {
            if adaptive_mode {
                info!(
                    "Adaptive LLM-powered flow execution enabled with goal: {}",
                    goal
                );
                run_adaptive_flow_demo(
                    endpoint_url,
                    goal,
                    config_loader,
                    llm_adapter,
                    flows_config,
                )
                .await?;
            } else {
                info!("LLM-powered flow negotiation enabled with goal: {}", goal);
                run_intelligent_flow_demo(
                    endpoint_url,
                    goal,
                    config_loader,
                    llm_adapter,
                    flows_config,
                )
                .await?;
            }
        } else {
            run_api_exploration_demo(endpoint_url, config_loader).await?;
        }
    } else {
        run_real_api_demo(name, demo_count, config_loader).await?;
    }
    Ok(())
}

async fn run_real_api_demo(
    name: &str,
    demo_count: usize,
    config_loader: ConfigLoader,
) -> Result<()> {
    let mut orchestrator = match FlowOrchestrator::new(config_loader).await {
        Ok(orchestrator) => {
            info!("Flow Orchestrator initialised successfully");
            orchestrator
        }
        Err(e) => {
            error!("Failed to initialise orchestrator: {}", e);
            return Err(e);
        }
    };

    orchestrator.show_system_info()?;

    info!("Starting real external API demonstrations");

    let api_types = ["nationality", "weather"];

    for (i, api_type) in api_types.iter().take(demo_count).enumerate() {
        info!(
            "=== API Demo {} of {} ===",
            i + 1,
            demo_count.min(api_types.len())
        );

        match orchestrator.execute_api_demo(api_type, name, i + 1).await {
            Ok(result) => {
                info!("API demo completed successfully");
                info!(
                    "Result: {}",
                    serde_json::to_string_pretty(&result).unwrap_or_default()
                );
            }
            Err(e) => {
                error!("API demo failed: {}", e);
            }
        }

        if i < demo_count.min(api_types.len()) - 1 {
            info!("Waiting before next demo...");
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }

    info!("Real external API demonstrations completed!");
    Ok(())
}

async fn run_api_exploration_demo(endpoint_url: &str, config_loader: ConfigLoader) -> Result<()> {
    let mut orchestrator = match FlowOrchestrator::new(config_loader).await {
        Ok(orchestrator) => {
            info!("Flow Orchestrator initialised successfully");
            orchestrator
        }
        Err(e) => {
            error!("Failed to initialise orchestrator: {}", e);
            return Err(e);
        }
    };

    orchestrator.show_system_info()?;

    info!("Starting API exploration demonstration");
    info!("Target endpoint: {}", endpoint_url);

    match orchestrator.execute_api_exploration(endpoint_url).await {
        Ok(result) => {
            info!("API exploration completed successfully");
            info!(
                "Result: {}",
                serde_json::to_string_pretty(&result).unwrap_or_default()
            );
        }
        Err(e) => {
            error!("API exploration failed: {}", e);
        }
    }

    info!("API exploration demonstration completed!");
    Ok(())
}

async fn run_intelligent_flow_demo(
    endpoint_url: &str,
    processing_goal: &str,
    config_loader: ConfigLoader,
    llm_adapter: Arc<UnifiedLLMAdapter>,
    flows_config: config::FlowsDemoConfig,
) -> Result<()> {
    info!("Starting LLM-Enhanced Flow Negotiation Demo");
    info!("Goal: {}", processing_goal);
    info!("Endpoint: {}", endpoint_url);

    let mut enhanced_orchestrator = match llm::LLMEnhancedOrchestrator::new_with_unified_adapter(
        config_loader,
        llm_adapter,
        flows_config,
    )
    .await
    {
        Ok(orchestrator) => {
            info!("LLM-Enhanced Orchestrator initialised successfully");
            orchestrator
        }
        Err(e) => {
            error!("Failed to initialise LLM-enhanced orchestrator: {}", e);
            return Err(e);
        }
    };

    enhanced_orchestrator.show_enhanced_system_info()?;

    info!("Performing comprehensive health check...");
    match enhanced_orchestrator.comprehensive_health_check().await {
        Ok(health_report) => {
            info!(
                "Health Report: {}",
                serde_json::to_string_pretty(&health_report)?
            );
        }
        Err(e) => {
            warn!("Health check issues detected: {}", e);
        }
    }

    info!("Executing flow negotiation...");
    match enhanced_orchestrator
        .complete_flow_cycle(endpoint_url, processing_goal)
        .await
    {
        Ok(result) => {
            info!("Complete flow cycle completed successfully!");
            info!("FULL RESULTS:");
            info!("{}", serde_json::to_string_pretty(&result)?);
        }
        Err(e) => {
            error!("Complete flow cycle failed: {}", e);
        }
    }

    info!("LLM-Enhanced Flow Negotiation Demo completed!");
    Ok(())
}

async fn run_adaptive_flow_demo(
    endpoint_url: &str,
    processing_goal: &str,
    config_loader: ConfigLoader,
    llm_adapter: Arc<UnifiedLLMAdapter>,
    flows_config: config::FlowsDemoConfig,
) -> Result<()> {
    info!("Starting Adaptive Flow Execution Demo");
    info!("Goal: {}", processing_goal);
    info!("Endpoint: {}", endpoint_url);

    let mut adaptive_orchestrator = match AdaptiveFlowOrchestrator::new_with_unified_adapter(
        config_loader,
        llm_adapter,
        flows_config,
    )
    .await
    {
        Ok(orchestrator) => {
            info!("Adaptive Flow Orchestrator initialised successfully");
            orchestrator
        }
        Err(e) => {
            error!("Failed to initialise adaptive orchestrator: {}", e);
            return Err(e);
        }
    };

    adaptive_orchestrator.show_system_info()?;

    info!("Performing adaptive orchestrator health check...");
    match adaptive_orchestrator.health_check().await {
        Ok(health_report) => {
            info!(
                "Health Report: {}",
                serde_json::to_string_pretty(&health_report)?
            );
        }
        Err(e) => {
            warn!("Health check issues detected: {}", e);
        }
    }

    info!("Starting adaptive flow execution with error recovery...");
    match adaptive_orchestrator
        .adaptive_flow_execution(endpoint_url, processing_goal)
        .await
    {
        Ok(result) => {
            info!("Adaptive flow execution completed successfully!");
            info!("FULL RESULTS:");
            info!("{}", serde_json::to_string_pretty(&result)?);
        }
        Err(e) => {
            error!("Adaptive flow execution failed: {}", e);
        }
    }

    info!("Adaptive Flow Execution Demo completed!");
    Ok(())
}
