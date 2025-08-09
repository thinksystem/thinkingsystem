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

pub mod collaboration;
pub mod collaboration_workflows;
pub mod manager;
pub mod processor;
pub mod progress;
pub mod prompts;
pub mod unified_adapter;
pub mod utils;
pub mod validation;

pub use unified_adapter::UnifiedLLMAdapter;

pub use collaboration::{fields, json_utils, CollaborationPrompts};
pub use collaboration_workflows::{
    apply_breakout_strategy, assess_proposal_quality, distil_feedback, evaluate_progress_score,
    get_initial_proposal, get_specialist_feedback, refine_proposal,
};
pub use manager::{AdapterStrategy, LLMManager, LLMManagerConfig, LLMManagerStatus};
pub use processor::{
    generate_chat_response, stream_generate_response, stream_with_callback, ConversationConfig,
    ConversationEntry, LLMProcessor,
};
pub use progress::{
    analysis, ProgressAnalysis, ProgressConfig, ProgressEntry, ProgressPattern, ProgressTracker,
};
pub use prompts::{context_builders, PromptBuilder, PromptContext, PromptTemplate};
use thiserror::Error;
pub use utils::extract_json_from_text;
pub use validation::{validators, ResponseValidator, ValidationResult, ValueType};

pub type LLMResult<T> = Result<T, LLMError>;

#[derive(Error, Debug, Clone)]
pub enum LLMError {
    #[error("API error: {0}")]
    ApiError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Authentication failed: {0}")]
    AuthenticationError(String),

    #[error("JSON parsing error: {0}")]
    JsonError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Timeout error: request timed out after {seconds} seconds")]
    TimeoutError { seconds: u64 },

    #[error("Rate limit exceeded: {0}")]
    RateLimitError(String),

    #[error("Streaming error: {0}")]
    StreamError(String),
}

impl From<Box<dyn std::error::Error>> for LLMError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        LLMError::ApiError(err.to_string())
    }
}

impl LLMError {
    pub fn from_box_error(err: Box<dyn std::error::Error>) -> Self {
        LLMError::ApiError(err.to_string())
    }
}
