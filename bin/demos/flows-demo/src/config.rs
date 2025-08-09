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

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
pub struct FlowsDemoConfig {
    pub system: SystemInfo,
    pub engine: EngineConfig,
    pub llm: LLMConfig,
    pub generation: GenerationConfig,
    pub demo: DemoConfig,
    pub api_providers: ApiProviderConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApiProviderConfig {
    pub primary_provider: String,
    pub fallback_provider: String,
    pub anthropic: AnthropicConfig,
    pub ollama: OllamaConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AnthropicConfig {
    pub model: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub api_version: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OllamaConfig {
    pub endpoint: String,
    pub model: String,
    pub max_tokens: usize,
    pub temperature: f32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SystemConfig {
    pub system: SystemInfo,
    pub engine: EngineConfig,
    pub llm: LLMConfig,
    pub generation: GenerationConfig,
    pub demo: DemoConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SystemInfo {
    pub name: String,
    pub description: String,
    pub version: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EngineConfig {
    #[serde(rename = "type")]
    pub engine_type: String,
    pub backend: String,
    pub architecture: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LLMConfig {
    pub model: String,
    pub context_size: u32,
    pub temperature: f32,
    pub provider: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GenerationConfig {
    pub max_iterations: usize,
    pub validation_enabled: bool,
    pub fallback_enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DemoConfig {
    pub default_task: String,
    pub default_iterations: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PromptsConfig {
    pub flow_generation: FlowGenerationPrompts,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FlowGenerationPrompts {
    pub system_prompt: String,
    pub task_analysis_prompt: String,
    pub completion_evaluation_prompt: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MessagesConfig {
    pub error_messages: HashMap<String, String>,
    pub feedback_templates: HashMap<String, String>,
    pub status_messages: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FlowPattern {
    pub name: String,
    pub description: String,
    pub use_case: String,
    pub template_path: String,
    pub complexity: String,
}

#[derive(Debug, Clone)]
pub struct ConfigLoader {
    config_dir: String,
}

impl ConfigLoader {
    pub fn new(config_dir: &str) -> Self {
        Self {
            config_dir: config_dir.to_string(),
        }
    }

    pub fn load_flows_config(&self) -> anyhow::Result<FlowsDemoConfig> {
        let config = FlowsDemoConfig {
            system: SystemInfo {
                name: "Flows Demo".to_string(),
                description: "Advanced Flow Orchestration Demonstration".to_string(),
                version: "1.0.0".to_string(),
            },
            engine: EngineConfig {
                engine_type: "unified_flow_engine".to_string(),
                backend: "stele".to_string(),
                architecture: "async_tokio".to_string(),
            },
            llm: LLMConfig {
                model: std::env::var("LLM_MODEL")
                    .unwrap_or_else(|_| "claude-3-5-haiku-latest".to_string()),
                context_size: std::env::var("LLM_CONTEXT_SIZE")
                    .unwrap_or_else(|_| "8192".to_string())
                    .parse()
                    .unwrap_or(8192),
                temperature: std::env::var("LLM_TEMPERATURE")
                    .unwrap_or_else(|_| "0.2".to_string())
                    .parse()
                    .unwrap_or(0.2),
                provider: std::env::var("LLM_PROVIDER")
                    .ok()
                    .or_else(|| Some("anthropic".to_string())),
            },
            generation: GenerationConfig {
                max_iterations: 3,
                validation_enabled: true,
                fallback_enabled: true,
            },
            demo: DemoConfig {
                default_task: "API analysis and processing".to_string(),
                default_iterations: 2,
            },
            api_providers: ApiProviderConfig {
                primary_provider: std::env::var("PRIMARY_LLM_PROVIDER")
                    .unwrap_or_else(|_| "anthropic".to_string()),
                fallback_provider: std::env::var("FALLBACK_LLM_PROVIDER")
                    .unwrap_or_else(|_| "ollama".to_string()),
                anthropic: AnthropicConfig {
                    model: std::env::var("ANTHROPIC_MODEL")
                        .unwrap_or_else(|_| "claude-3-5-haiku-latest".to_string()),
                    max_tokens: std::env::var("ANTHROPIC_MAX_TOKENS")
                        .unwrap_or_else(|_| "8192".to_string())
                        .parse()
                        .unwrap_or(8192),
                    temperature: std::env::var("ANTHROPIC_TEMPERATURE")
                        .unwrap_or_else(|_| "0.2".to_string())
                        .parse()
                        .unwrap_or(0.2),
                    api_version: std::env::var("ANTHROPIC_API_VERSION")
                        .unwrap_or_else(|_| "2023-06-01".to_string()),
                },
                ollama: OllamaConfig {
                    endpoint: std::env::var("OLLAMA_ENDPOINT")
                        .unwrap_or_else(|_| "http://localhost:11434/api/generate".to_string()),
                    model: std::env::var("OLLAMA_MODEL")
                        .unwrap_or_else(|_| "llama3.2:latest".to_string()),
                    max_tokens: std::env::var("OLLAMA_MAX_TOKENS")
                        .unwrap_or_else(|_| "4096".to_string())
                        .parse()
                        .unwrap_or(4096),
                    temperature: std::env::var("OLLAMA_TEMPERATURE")
                        .unwrap_or_else(|_| "0.7".to_string())
                        .parse()
                        .unwrap_or(0.7),
                },
            },
        };

        Ok(config)
    }

    pub fn load_system_config(&self) -> anyhow::Result<SystemConfig> {
        let config_file = format!("{}/system.toml", self.config_dir);
        let content = std::fs::read_to_string(config_file)?;
        let config: SystemConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn load_prompts_config(&self) -> anyhow::Result<PromptsConfig> {
        let config_file = format!("{}/prompts.yml", self.config_dir);
        let content = std::fs::read_to_string(config_file)?;
        let config: PromptsConfig = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    pub fn load_messages_config(&self) -> anyhow::Result<MessagesConfig> {
        let config_file = format!("{}/messages.yml", self.config_dir);
        let content = std::fs::read_to_string(config_file)?;
        let config: MessagesConfig = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    pub fn load_flow_patterns(&self) -> anyhow::Result<serde_json::Value> {
        let config_file = format!("{}/flow_patterns.json", self.config_dir);
        let content = std::fs::read_to_string(config_file)?;
        let config: serde_json::Value = serde_json::from_str(&content)?;
        Ok(config)
    }

    pub fn load_block_schemas(&self) -> anyhow::Result<serde_json::Value> {
        let config_file = format!("{}/block_schemas.json", self.config_dir);
        let content = std::fs::read_to_string(config_file)?;
        let config: serde_json::Value = serde_json::from_str(&content)?;
        Ok(config)
    }
}
