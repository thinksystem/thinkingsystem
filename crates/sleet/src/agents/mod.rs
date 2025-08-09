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

pub mod capabilities;
pub mod generator;
pub mod registry;
pub mod schemas;
pub use capabilities::{
    CapabilityMatch, CapabilityMatcher, CapabilityMatcherConfig, SkillRequirement,
};
pub use generator::{AgentGenerator, AgentTeamResponse, FallbackAgentConfig, GenerationConfig};
pub use registry::{AgentRegistry, RegistryError};
pub use schemas::{
    Agent, AgentCapabilities, AgentConfig, AgentMetadata, AgentStatus, RuntimeCapabilities,
    TrustLevel,
};
use serde::{Deserialize, Serialize};
use stele::nlu::llm_processor::LLMAdapter;
use thiserror::Error;
#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
    #[error("Invalid agent configuration")]
    InvalidConfiguration(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("Agent generation failed")]
    GenerationFailed(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("Registry error")]
    Registry(#[from] RegistryError),
    #[error("LLM integration error")]
    LlmError(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("Capability evaluation error")]
    CapabilityError(#[source] Box<dyn std::error::Error + Send + Sync>),
}
pub type AgentResult<T> = Result<T, AgentError>;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSystemConfig {
    pub generation: GenerationConfig,
    pub registry: RegistryConfig,
    pub max_concurrent_agents: usize,
    pub default_timeout_secs: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    pub persistent: bool,
    pub storage_path: Option<String>,
    pub max_in_memory: usize,
}
pub struct AgentSystem {
    generator: AgentGenerator,
    registry: AgentRegistry,
    llm_adapter: Option<Box<dyn LLMAdapter>>,
    #[allow(dead_code)]
    config: AgentSystemConfig,
}
impl AgentSystem {
    pub fn new(config: AgentSystemConfig) -> AgentResult<Self> {
        let generator =
            AgentGenerator::new().map_err(|e| AgentError::GenerationFailed(Box::new(e)))?;
        let registry_config = config.registry.clone();
        let registry = AgentRegistry::new(registry_config)?;
        Ok(Self {
            generator,
            registry,
            llm_adapter: None,
            config,
        })
    }
    pub async fn initialise(&mut self) -> AgentResult<()> {
        Ok(())
    }

    pub fn set_llm_adapter(&mut self, adapter: Box<dyn LLMAdapter>) {
        self.llm_adapter = Some(adapter);
    }

    pub fn get_llm_adapter(&self) -> Option<&dyn LLMAdapter> {
        self.llm_adapter.as_ref().map(|a| a.as_ref())
    }
    pub async fn generate_agent(&mut self, config: AgentConfig) -> AgentResult<Agent> {
        let agent = self
            .generator
            .generate_agent(config)
            .await
            .map_err(|e| AgentError::GenerationFailed(Box::new(e)))?;
        self.registry.register_agent(agent.clone())?;
        Ok(agent)
    }
    pub async fn generate_team(
        &mut self,
        task_description: String,
        team_size: usize,
    ) -> AgentResult<Vec<Agent>> {
        let config = GenerationConfig {
            team_size,
            task_context: task_description,
            ..Default::default()
        };

        let team = if let Some(adapter) = &self.llm_adapter {
            self.generator
                .generate_team_with_llm(config, adapter.as_ref())
                .await
        } else {
            self.generator.generate_team(config).await
        }
        .map_err(|e| AgentError::GenerationFailed(Box::new(e)))?;

        for agent in &team {
            self.registry.register_agent(agent.clone())?;
        }
        Ok(team)
    }
    pub async fn find_agents(
        &self,
        requirements: CapabilityMatcher,
    ) -> AgentResult<Vec<(Agent, CapabilityMatch)>> {
        let all_agents = self.registry.list_active_agents()?;
        let mut matches = Vec::new();
        for agent in all_agents {
            let match_result = requirements.evaluate_match(&agent.capabilities);
            if match_result.required_skills_met {
                matches.push((agent, match_result));
            }
        }
        matches.sort_by(|a, b| {
            b.1.score
                .partial_cmp(&a.1.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(matches)
    }
    pub fn get_agent(&self, agent_id: &str) -> AgentResult<Agent> {
        Ok(self.registry.get_agent(agent_id)?)
    }
    pub fn register_agent(&mut self, agent: Agent) -> AgentResult<()> {
        Ok(self.registry.register_agent(agent)?)
    }
    pub fn update_agent_status(&mut self, agent_id: &str, status: AgentStatus) -> AgentResult<()> {
        Ok(self.registry.update_agent_status(agent_id, status)?)
    }
    pub fn remove_agent(&mut self, agent_id: &str) -> AgentResult<()> {
        Ok(self.registry.remove_agent(agent_id)?)
    }
    pub fn get_statistics(&self) -> AgentSystemStatistics {
        AgentSystemStatistics {
            total_agents: self.registry.total_agent_count(),
            active_agents: self.registry.active_agent_count(),
            available_agents: self.registry.available_agent_count(),
        }
    }
    pub fn list_active_agents(&self) -> AgentResult<Vec<Agent>> {
        Ok(self.registry.list_active_agents()?)
    }
    pub async fn shutdown(&mut self) -> AgentResult<()> {
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSystemStatistics {
    pub total_agents: usize,
    pub active_agents: usize,
    pub available_agents: usize,
}
impl Default for AgentSystemConfig {
    fn default() -> Self {
        Self {
            generation: GenerationConfig::default(),
            registry: RegistryConfig {
                persistent: false,
                storage_path: None,
                max_in_memory: 1000,
            },
            max_concurrent_agents: 50,
            default_timeout_secs: 300,
        }
    }
}
