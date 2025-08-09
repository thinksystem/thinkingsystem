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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SpeedTier {
    Fast,
    Medium,
    Slow,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CostTier {
    Free,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

impl From<String> for SpeedTier {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "fast" => SpeedTier::Fast,
            "medium" => SpeedTier::Medium,
            "slow" => SpeedTier::Slow,
            _ => SpeedTier::Medium,
        }
    }
}

impl From<String> for CostTier {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "free" => CostTier::Free,
            "low" => CostTier::Low,
            "medium" => CostTier::Medium,
            "high" => CostTier::High,
            _ => CostTier::Medium,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub models: Vec<ModelDefinition>,
    pub selection_strategy: SelectionStrategy,
    pub providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub feedback: Option<FeedbackConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDefinition {
    pub name: String,
    pub provider: String,
    pub capabilities: Vec<String>,
    pub max_tokens: u32,

    #[serde(default)]
    pub speed_tier: Option<String>,
    #[serde(default)]
    pub cost_tier: Option<String>,
    #[serde(default)]
    pub parallel_limit: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,

    #[serde(default)]
    pub quality_score: Option<f64>,
    #[serde(default)]
    pub avg_response_ms: Option<u64>,
    #[serde(default)]
    pub avg_tokens_per_second: Option<u32>,
    #[serde(default)]
    pub cost_per_million_tokens: Option<CostPerMillionTokens>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostPerMillionTokens {
    pub input: f64,
    pub output: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionStrategy {
    #[serde(default)]
    pub primary_model: Option<String>,
    #[serde(default)]
    pub fallback_models: Vec<String>,
    #[serde(default)]
    pub prefer_speed: Option<bool>,
    #[serde(default)]
    pub prefer_free: Option<bool>,

    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub weights: HashMap<String, IntentWeights>,

    #[serde(default = "default_fallback_on_failure")]
    pub fallback_on_failure: bool,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentWeights {
    pub quality: f64,
    pub speed: f64,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackConfig {
    #[serde(default)]
    pub update_on_success: bool,
    #[serde(default)]
    pub performance_db_path: Option<String>,
    #[serde(default = "default_learning_rate")]
    pub learning_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub timeout_seconds: Option<u32>,
    pub max_retries: Option<u32>,
    pub authentication: AuthenticationConfig,
    pub rate_limits: Option<RateLimits>,
    #[serde(flatten)]
    pub provider_specific: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticationConfig {
    #[serde(rename = "type")]
    pub auth_type: String,
    pub header: Option<String>,
    pub version_header: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimits {
    pub requests_per_minute: Option<u32>,
    pub requests_per_hour: Option<u32>,
    pub concurrent_requests: Option<u32>,
}

fn default_fallback_on_failure() -> bool {
    true
}
fn default_max_retries() -> u32 {
    3
}
fn default_timeout_seconds() -> u64 {
    120
}
fn default_learning_rate() -> f64 {
    0.1
}

impl SelectionStrategy {
    pub fn is_v1_mode(&self) -> bool {
        self.primary_model.is_some() || !self.fallback_models.is_empty()
    }

    pub fn is_v2_mode(&self) -> bool {
        !self.is_v1_mode() && self.intent.is_some()
    }
}

impl ModelDefinition {
    pub fn has_v2_metrics(&self) -> bool {
        self.quality_score.is_some()
            || self.avg_response_ms.is_some()
            || self.cost_per_million_tokens.is_some()
    }

    pub fn get_quality_score(&self) -> f64 {
        if let Some(score) = self.quality_score {
            return score;
        }

        match self.speed_tier.as_deref() {
            Some("fast") => 0.6,
            Some("medium") => 0.7,
            Some("slow") => 0.8,
            _ => 0.5,
        }
    }

    pub fn get_cost_score(&self) -> f64 {
        if let Some(cost) = &self.cost_per_million_tokens {
            let total_cost = cost.input + cost.output;
            (total_cost / 100.0).min(1.0)
        } else {
            match self.cost_tier.as_deref() {
                Some("free") => 0.0,
                Some("low") => 0.2,
                Some("medium") => 0.5,
                Some("high") => 0.8,
                _ => 0.3,
            }
        }
    }

    pub fn get_speed_score(&self) -> f64 {
        if let Some(response_ms) = self.avg_response_ms {
            let normalised = 10000.0 / (response_ms as f64 + 100.0);
            normalised.min(1.0)
        } else {
            match self.speed_tier.as_deref() {
                Some("fast") => 0.9,
                Some("medium") => 0.6,
                Some("slow") => 0.3,
                _ => 0.5,
            }
        }
    }
}
