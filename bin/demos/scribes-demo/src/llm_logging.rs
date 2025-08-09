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
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::Instant;
use tracing::{debug, error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMCallLog {
    pub timestamp: DateTime<Utc>,
    pub call_id: String,
    pub session_id: String,
    pub component: String,
    pub operation: String,
    pub provider: String,
    pub model: String,
    pub request: LLMRequest,
    pub response: Option<LLMResponse>,
    pub error: Option<String>,
    pub timing: LLMTiming,
    pub cost_estimate: Option<f64>,
    pub tokens: Option<TokenCount>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMRequest {
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub temperature: f32,
    pub max_tokens: usize,
    pub raw_payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMResponse {
    pub content: String,
    pub raw_response: Value,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMTiming {
    pub request_start: DateTime<Utc>,
    pub response_received: DateTime<Utc>,
    pub total_duration_ms: u64,
    pub network_latency_ms: Option<u64>,
    pub processing_time_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCount {
    pub prompt_tokens: Option<usize>,
    pub completion_tokens: Option<usize>,
    pub total_tokens: Option<usize>,
}

#[derive(Debug)]
pub struct LLMLogger {
    log_file_path: String,
    session_id: String,
    verbose_console: bool,
}

impl LLMLogger {
    pub fn new(log_file_path: &str, session_id: String, verbose_console: bool) -> Self {
        if let Some(parent) = Path::new(log_file_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }

        Self {
            log_file_path: log_file_path.to_string(),
            session_id,
            verbose_console,
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn generate_call_id() -> String {
        format!(
            "llm_{}",
            &uuid::Uuid::new_v4().to_string().replace('-', "")[..16]
        )
    }

    pub fn start_call(
        &self,
        component: &str,
        operation: &str,
        provider: &str,
        model: &str,
        request: LLMRequest,
    ) -> LLMCallTracker {
        let call_id = Self::generate_call_id();
        let start_time = Utc::now();

        info!(
            call_id = %call_id,
            component = %component,
            operation = %operation,
            provider = %provider,
            model = %model,
            "Starting LLM call"
        );

        if self.verbose_console {
            debug!(
                call_id = %call_id,
                prompt_length = request.prompt.len(),
                temperature = request.temperature,
                max_tokens = request.max_tokens,
                "LLM Request details"
            );
            debug!(call_id = %call_id, prompt = %request.prompt, "LLM Request prompt");
            if let Some(ref system) = request.system_prompt {
                debug!(call_id = %call_id, system_prompt = %system, "LLM System prompt");
            }
        }

        LLMCallTracker {
            logger: self.clone(),
            call_id: call_id.clone(),
            component: component.to_string(),
            operation: operation.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            request,
            start_time,
            start_instant: Instant::now(),
        }
    }

    fn write_log(&self, log_entry: &LLMCallLog) {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file_path)
        {
            if let Ok(json_line) = serde_json::to_string(log_entry) {
                if let Err(e) = writeln!(file, "{json_line}") {
                    error!("Failed to write to LLM log file: {}", e);
                }
            }
        } else {
            error!("Failed to open LLM log file: {}", self.log_file_path);
        }
    }
}

impl Clone for LLMLogger {
    fn clone(&self) -> Self {
        Self {
            log_file_path: self.log_file_path.clone(),
            session_id: self.session_id.clone(),
            verbose_console: self.verbose_console,
        }
    }
}

pub struct LLMCallTracker {
    logger: LLMLogger,
    call_id: String,
    component: String,
    operation: String,
    provider: String,
    model: String,
    request: LLMRequest,
    start_time: DateTime<Utc>,
    start_instant: Instant,
}

impl LLMCallTracker {
    pub fn complete_success(
        self,
        response: LLMResponse,
        tokens: Option<TokenCount>,
        cost_estimate: Option<f64>,
        metadata: Value,
    ) {
        let end_time = Utc::now();
        let duration = self.start_instant.elapsed();

        info!(
            call_id = %self.call_id,
            duration_ms = duration.as_millis(),
            response_length = response.content.len(),
            "LLM call completed successfully"
        );

        if self.logger.verbose_console {
            debug!(
                call_id = %self.call_id,
                response = %response.content,
                "LLM Response content"
            );
        }

        if let Some(ref token_count) = tokens {
            debug!(
                call_id = %self.call_id,
                prompt_tokens = ?token_count.prompt_tokens,
                completion_tokens = ?token_count.completion_tokens,
                total_tokens = ?token_count.total_tokens,
                "LLM Token usage"
            );
        }

        if let Some(cost) = cost_estimate {
            debug!(call_id = %self.call_id, cost_usd = cost, "LLM Cost estimate");
        }

        let log_entry = LLMCallLog {
            timestamp: self.start_time,
            call_id: self.call_id,
            session_id: self.logger.session_id.clone(),
            component: self.component,
            operation: self.operation,
            provider: self.provider,
            model: self.model,
            request: self.request,
            response: Some(response),
            error: None,
            timing: LLMTiming {
                request_start: self.start_time,
                response_received: end_time,
                total_duration_ms: duration.as_millis() as u64,
                network_latency_ms: None,
                processing_time_ms: None,
            },
            cost_estimate,
            tokens,
            metadata,
        };

        self.logger.write_log(&log_entry);
    }

    pub fn complete_error(self, error: String, metadata: Value) {
        let end_time = Utc::now();
        let duration = self.start_instant.elapsed();

        error!(
            call_id = %self.call_id,
            duration_ms = duration.as_millis(),
            error = %error,
            "LLM call failed"
        );

        let log_entry = LLMCallLog {
            timestamp: self.start_time,
            call_id: self.call_id,
            session_id: self.logger.session_id.clone(),
            component: self.component,
            operation: self.operation,
            provider: self.provider,
            model: self.model,
            request: self.request,
            response: None,
            error: Some(error),
            timing: LLMTiming {
                request_start: self.start_time,
                response_received: end_time,
                total_duration_ms: duration.as_millis() as u64,
                network_latency_ms: None,
                processing_time_ms: None,
            },
            cost_estimate: None,
            tokens: None,
            metadata,
        };

        self.logger.write_log(&log_entry);
    }

    pub fn call_id(&self) -> &str {
        &self.call_id
    }
}

pub fn estimate_cost(provider: &str, model: &str, tokens: &TokenCount) -> Option<f64> {
    let total_tokens = tokens.total_tokens? as f64;
    let prompt_tokens = tokens.prompt_tokens.unwrap_or(0) as f64;
    let completion_tokens = tokens.completion_tokens.unwrap_or(0) as f64;

    match provider {
        "anthropic" => match model {
            "claude-3-5-haiku-latest" | "claude-3-haiku-20240307" => {
                Some((prompt_tokens * 0.00025 + completion_tokens * 0.00125) / 1000.0)
            }
            "claude-3-5-sonnet-latest" | "claude-3-sonnet-20240229" => {
                Some((prompt_tokens * 0.003 + completion_tokens * 0.015) / 1000.0)
            }
            "claude-3-opus-20240229" => {
                Some((prompt_tokens * 0.015 + completion_tokens * 0.075) / 1000.0)
            }
            _ => Some(total_tokens * 0.002 / 1000.0),
        },
        "openai" => match model {
            "gpt-4" => Some((prompt_tokens * 0.03 + completion_tokens * 0.06) / 1000.0),
            "gpt-4-turbo" => Some((prompt_tokens * 0.01 + completion_tokens * 0.03) / 1000.0),
            "gpt-3.5-turbo" => Some((prompt_tokens * 0.0015 + completion_tokens * 0.002) / 1000.0),
            _ => Some(total_tokens * 0.002 / 1000.0),
        },
        _ => Some(total_tokens * 0.001 / 1000.0),
    }
}

pub fn estimate_tokens(text: &str) -> usize {
    (text.len() as f64 / 4.0).ceil() as usize
}
