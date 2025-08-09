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

use anyhow::Result;
use chrono::Utc;
use clap::{Arg, Command};
use serde_json::{json, Value};
use sleet::{
    agents::{AgentSystem, AgentSystemConfig},
    llm::LLMManager,
    tasks::{create_task_from_input, TaskSystem, TaskSystemConfig},
    workflows::{
        events, generate_complete_team, PlanningSession, PlanningSessionConfig,
        TeamGenerationConfig,
    },
};
use std::sync::Arc;
use tokio::sync::Mutex;

fn log_event(event_type: &str, data: Value) {
    println!(
        "{}",
        json!({ "timestamp": Utc::now().to_rfc3339(), "event": event_type, "data": data })
    );
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    let matches = Command::new("agent-orchestration-demo")
        .version("1.0.0")
        .about("Optimised Goal-Oriented Agent Planning Demo")
        .arg(
            Arg::new("goal")
                .long("goal")
                .short('g')
                .help("The goal for the agent team to achieve")
                .required(true),
        )
        .arg(
            Arg::new("team-size")
                .long("team-size")
                .short('s')
                .help("Number of specialist agents (1-10)")
                .default_value("3")
                .value_parser(clap::value_parser!(u8).range(1..=10)),
        )
        .arg(
            Arg::new("provider")
                .long("provider")
                .help("Primary LLM provider")
                .default_value("ollama")
                .value_parser(["ollama", "anthropic"]),
        )
        .arg(
            Arg::new("model")
                .long("model")
                .short('m')
                .help("Model for the primary provider")
                .default_value("llama3.2"),
        )
        .get_matches();

    let goal = matches
        .get_one::<String>("goal")
        .ok_or_else(|| anyhow::anyhow!("Goal argument is required"))?;
    let team_size = *matches
        .get_one::<u8>("team-size")
        .ok_or_else(|| anyhow::anyhow!("Team size argument is required"))?
        as usize;
    let provider = matches
        .get_one::<String>("provider")
        .ok_or_else(|| anyhow::anyhow!("Provider argument is required"))?;
    let mut model = matches
        .get_one::<String>("model")
        .ok_or_else(|| anyhow::anyhow!("Model argument is required"))?
        .clone();

    match provider.as_str() {
        "anthropic" => {
            if model.starts_with("llama") {
                println!("⚠️  Model '{model}' is not compatible with Anthropic provider. Using 'claude-3-5-haiku-latest' instead.");
                model = "claude-3-5-haiku-latest".to_string();
            }
        }
        "ollama" => {
            if model.starts_with("claude") || model.starts_with("gpt") {
                println!("⚠️  Model '{model}' is not compatible with Ollama provider. Using 'llama3.2' instead.");
                model = "llama3.2".to_string();
            }
        }
        _ => {}
    }

    log_event(
        events::DEMO_STARTUP,
        json!({ "goal": goal, "team_size": team_size, "provider": provider, "model": &model }),
    );

    let llm_config = sleet::llm::LLMManagerConfig {
        primary_provider: provider.to_string(),
        primary_model: model.clone(),
        preferred_provider: None,
        preferred_model: None,
        fallback_providers: vec![
            (
                "anthropic".to_string(),
                "claude-sonnet-4-20250514".to_string(),
            ),
            ("ollama".to_string(), "llama3.1".to_string()),
        ],
        retry_attempts: 3,
        enable_fallback: true,
    };
    let llm_manager = LLMManager::new(llm_config).await?;
    let agent_system = Arc::new(Mutex::new(AgentSystem::new(AgentSystemConfig::default())?));
    let _task_system = Arc::new(Mutex::new(TaskSystem::new(TaskSystemConfig::default())));

    let task = create_task_from_input(goal, &sleet::tasks::SimpleTaskAnalyser::new())?;

    let team_config = TeamGenerationConfig {
        provider: provider.to_string(),
        model: model.to_string(),
        ..Default::default()
    };
    let (specialists, arbiter) = generate_complete_team(goal, team_size, &team_config).await?;

    let session_config = PlanningSessionConfig::default();
    let mut session =
        PlanningSession::with_config(task, specialists, arbiter, llm_manager, session_config);
    let result = session.run().await?;

    log_event(
        events::DEMO_COMPLETED,
        json!({ "goal": goal, "result": result, "success": true }),
    );
    Ok(())
}
