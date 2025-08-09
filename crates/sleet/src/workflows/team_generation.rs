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

use crate::agents::{
    generator::{
        AgentGenerator, DiversityRequirements, GenerationConfig, ModelConfig,
        PerformanceExpectations, TaskRequirement,
    },
    Agent,
};
use crate::llm::UnifiedLLMAdapter;
use anyhow::Result;
use std::collections::HashMap;
use stele::nlu::llm_processor::LLMAdapter as LocalLLMAdapter;
use stele::nlu::llm_processor::LLMAdapter;

struct LLMAdapterBridge {
    inner: UnifiedLLMAdapter,
}

#[async_trait::async_trait]
impl stele::nlu::llm_processor::LLMAdapter for LLMAdapterBridge {
    async fn process_text(&self, input: &str) -> Result<String, Box<dyn std::error::Error>> {
        LocalLLMAdapter::generate_response(&self.inner, input)
            .await
            .map_err(|e| e as Box<dyn std::error::Error>)
    }

    async fn generate_response(&self, prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
        LocalLLMAdapter::generate_response(&self.inner, prompt)
            .await
            .map_err(|e| e as Box<dyn std::error::Error>)
    }

    async fn generate_structured_response(
        &self,
        system_prompt: &str,
        user_input: &str,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        LocalLLMAdapter::generate_structured_response(&self.inner, system_prompt, user_input)
            .await
            .map_err(|e| e as Box<dyn std::error::Error>)
    }
}

#[derive(Debug, Clone)]
pub struct TeamGenerationConfig {
    pub provider: String,
    pub model: String,
    pub specialist_temperature: f64,
    pub arbiter_temperature: f64,
    pub max_tokens: usize,
}

impl Default for TeamGenerationConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            model: "llama3.2".to_string(),
            specialist_temperature: 0.7,
            arbiter_temperature: 0.6,
            max_tokens: 4096,
        }
    }
}

pub async fn initialise_llm_adapter(provider: &str, model: &str) -> Result<Box<dyn LLMAdapter>> {
    let adapter = UnifiedLLMAdapter::with_preferences(provider, model)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to create LLM adapter with preferences {}/{}: {}",
                provider,
                model,
                e
            )
        })?;

    let bridge = LLMAdapterBridge { inner: adapter };
    Ok(Box::new(bridge))
}

pub async fn generate_specialist_team(
    goal: &str,
    team_size: usize,
    config: &TeamGenerationConfig,
) -> Result<Vec<Agent>> {
    let adapter = initialise_llm_adapter(&config.provider, &config.model).await?;
    let generator = AgentGenerator::new()?;

    let specialist_config = GenerationConfig {
        team_size,
        task_context: format!(
            "Create a team of {team_size} specialist agents to {goal}. Each agent should have unique expertise and complementary skills."
        ),
        requirements: vec![
            TaskRequirement {
                capability: "Problem Analysis".to_string(),
                min_proficiency: 0.7,
                critical: true,
                alternatives: vec!["Systems Thinking".to_string()],
            },
            TaskRequirement {
                capability: "Implementation".to_string(),
                min_proficiency: 0.8,
                critical: true,
                alternatives: vec!["Technical Execution".to_string()],
            },
        ],
        diversity_requirements: DiversityRequirements {
            min_specialisations: team_size.min(3),
            diverse_approach_styles: true,
            diverse_risk_tolerance: true,
            diverse_collaboration_styles: true,
            min_diversity_score: 0.7,
        },
        performance_expectations: PerformanceExpectations::default(),
        model_config: ModelConfig {
            model: config.model.clone(),
            temperature: config.specialist_temperature,
            max_tokens: config.max_tokens,
            custom_params: HashMap::new(),
        },
    };

    generator
        .generate_team_with_llm(specialist_config, adapter.as_ref())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to generate specialist team: {}", e))
}

pub async fn generate_arbiter_agent(
    goal: &str,
    specialists: &[Agent],
    config: &TeamGenerationConfig,
) -> Result<Agent> {
    let adapter = initialise_llm_adapter(&config.provider, &config.model).await?;
    let generator = AgentGenerator::new()?;

    let specialist_roles = specialists
        .iter()
        .map(|s| s.specialisation.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let specialist_names = specialists
        .iter()
        .map(|s| s.name.clone())
        .collect::<Vec<_>>()
        .join(", ");

    let arbiter_config = GenerationConfig {
        team_size: 1,
        task_context: format!(
            "Create 1 arbiter agent to evaluate and judge the quality of work for: {goal}. The arbiter should have expertise that complements the specialist team: [{specialist_roles}] and should be able to assess project completion."
        ),
        requirements: vec![
            TaskRequirement {
                capability: "Quality Assessment".to_string(),
                min_proficiency: 0.9,
                critical: true,
                alternatives: vec!["Evaluation".to_string(), "Review".to_string()],
            },
        ],
        diversity_requirements: DiversityRequirements::minimal(),
        performance_expectations: PerformanceExpectations::default(),
        model_config: ModelConfig {
            model: config.model.clone(),
            temperature: config.arbiter_temperature,
            max_tokens: config.max_tokens,
            custom_params: HashMap::new(),
        },
    };

    let mut arbiters = generator
        .generate_team_with_llm(arbiter_config, adapter.as_ref())
        .await?;
    let mut arbiter = arbiters
        .pop()
        .ok_or_else(|| anyhow::anyhow!("Failed to generate arbiter"))?;

    if specialists.iter().any(|s| s.name == arbiter.name) {
        tracing::warn!(
            "Duplicate agent name detected: {}, attempting regeneration",
            arbiter.name
        );

        let retry_config = GenerationConfig {
            team_size: 1,
            task_context: format!(
                "Create 1 arbiter agent with a completely different name from this list: [{specialist_names}]. The arbiter should evaluate and judge work quality for: {goal}. Focus on quality assessment and project evaluation expertise."
            ),
            requirements: vec![
                TaskRequirement {
                    capability: "Quality Assessment".to_string(),
                    min_proficiency: 0.9,
                    critical: true,
                    alternatives: vec!["Evaluation".to_string()],
                },
            ],
            diversity_requirements: DiversityRequirements::minimal(),
            performance_expectations: PerformanceExpectations::default(),
            model_config: ModelConfig {
                model: config.model.clone(),
                temperature: config.arbiter_temperature + 0.2,
                max_tokens: config.max_tokens,
                custom_params: HashMap::new(),
            },
        };

        let mut retry_arbiters = generator
            .generate_team_with_llm(retry_config, adapter.as_ref())
            .await?;
        if let Some(retry_arbiter) = retry_arbiters.pop() {
            arbiter = retry_arbiter;
        }
    }

    Ok(arbiter)
}

pub async fn generate_complete_team(
    goal: &str,
    team_size: usize,
    config: &TeamGenerationConfig,
) -> Result<(Vec<Agent>, Agent)> {
    let specialists = generate_specialist_team(goal, team_size, config).await?;
    let arbiter = generate_arbiter_agent(goal, &specialists, config).await?;

    let specialist_names = specialists
        .iter()
        .map(|s| s.name.clone())
        .collect::<Vec<_>>()
        .join(", ");

    tracing::info!(
        "Generated team: {} specialists and 1 arbiter",
        specialists.len()
    );
    tracing::info!("Specialists: {}", specialist_names);
    tracing::info!("Arbiter: {}", arbiter.name);

    Ok((specialists, arbiter))
}
