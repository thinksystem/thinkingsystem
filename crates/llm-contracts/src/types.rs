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
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Classification,
    Sentiment,
    MultiStepAnalysis,
    FullExtraction,
    FastExtraction,
    Segmentation,
    Tokenization,
    ComplexReasoning,
    CodeGeneration,
    Reasoning,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeedTier {
    Fast,
    Medium,
    Slow,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostTier {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Anthropic,
    OpenAI,
    Ollama,
    Custom(String),
}

#[derive(Debug, Error)]
pub enum LLMError {
    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Rate limit exceeded")]
    RateLimit,

    #[error("Network error: {0}")]
    Network(String),

    #[error("Serialisation error: {0}")]
    Serialisation(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Timeout error")]
    Timeout,

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type LLMResult<T> = Result<T, LLMError>;

impl From<String> for Capability {
    fn from(s: String) -> Self {
        match s.as_str() {
            "classification" => Capability::Classification,
            "sentiment" => Capability::Sentiment,
            "multi_step_analysis" => Capability::MultiStepAnalysis,
            "full_extraction" => Capability::FullExtraction,
            "fast_extraction" => Capability::FastExtraction,
            "segmentation" => Capability::Segmentation,
            "tokenization" => Capability::Tokenization,
            "complex_reasoning" => Capability::ComplexReasoning,
            "code_generation" => Capability::CodeGeneration,
            "reasoning" => Capability::Reasoning,
            _ => Capability::Custom(s),
        }
    }
}

impl From<String> for SpeedTier {
    fn from(s: String) -> Self {
        match s.as_str() {
            "fast" => SpeedTier::Fast,
            "medium" => SpeedTier::Medium,
            "slow" => SpeedTier::Slow,
            _ => SpeedTier::Medium,
        }
    }
}

impl From<String> for CostTier {
    fn from(s: String) -> Self {
        match s.as_str() {
            "low" => CostTier::Low,
            "medium" => CostTier::Medium,
            "high" => CostTier::High,
            _ => CostTier::Medium,
        }
    }
}

impl From<String> for Provider {
    fn from(s: String) -> Self {
        match s.as_str() {
            "anthropic" => Provider::Anthropic,
            "openai" => Provider::OpenAI,
            "ollama" => Provider::Ollama,
            _ => Provider::Custom(s),
        }
    }
}
