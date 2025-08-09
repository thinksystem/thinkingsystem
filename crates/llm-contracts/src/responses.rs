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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMResponse {
    pub id: Uuid,
    pub request_id: Uuid,
    pub content: String,
    pub model_used: String,
    pub provider_used: String,
    pub usage: Usage,
    pub metadata: ResponseMetadata,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResponse {
    pub content: String,
    pub model: String,
    pub usage: Usage,
    pub finish_reason: Option<String>,
    pub raw_response: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMetadata {
    pub processing_time_ms: u64,
    pub model_selection_reason: String,
    pub security_checks_passed: bool,
    pub cached: bool,
    pub retry_count: u32,
    pub cost_estimate: Option<f64>,
    pub additional_data: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub id: Uuid,
    pub request_id: Uuid,
    pub content_delta: String,
    pub is_final: bool,
    pub usage: Option<Usage>,
}

impl Default for ResponseMetadata {
    fn default() -> Self {
        Self {
            processing_time_ms: 0,
            model_selection_reason: "default".to_string(),
            security_checks_passed: true,
            cached: false,
            retry_count: 0,
            cost_estimate: None,
            additional_data: HashMap::new(),
        }
    }
}
