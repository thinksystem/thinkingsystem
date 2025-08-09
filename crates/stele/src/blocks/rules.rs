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
use std::{any::Any, collections::HashMap, future::Future, pin::Pin};
use thiserror::Error;

#[derive(Debug)]
pub enum BlockResult {
    Success(serde_json::Value),
    Failure(String),
    Navigate {
        target: String,
        priority: i32,
        is_override: bool,
    },
    FetchExternalData {
        url: String,
        data_path: String,
        output_key: String,
        next_block: String,
        priority: i32,
        is_override: bool,
    },

    FetchExternalDataEnhanced {
        url: String,
        data_path: String,
        output_key: String,
        next_block: String,
        priority: i32,
        is_override: bool,
        enable_path_discovery: bool,
    },

    ApiResponseAnalysis {
        url: String,
        response_structure: serde_json::Value,
        discovered_paths: Vec<String>,
        suggested_path: Option<String>,
        original_path: String,
        next_block: String,
    },
    ExecuteFunction {
        function_name: String,
        args: Vec<serde_json::Value>,
        output_key: String,
        next_block: String,
        priority: i32,
        is_override: bool,
    },
    AwaitInput {
        prompt: String,
        state_key: String,
    },
    AwaitChoice {
        question: String,
        options: Vec<serde_json::Value>,
        state_key: String,
    },
    Move(String),
    Terminate,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponseMetadata {
    pub url: String,
    pub original_path: String,
    pub used_path: Option<String>,
    pub discovered_paths: Vec<String>,
    pub structure_analysis: std::collections::HashMap<String, serde_json::Value>,
    pub path_discovery_used: bool,
    pub extraction_method: String,
}

#[derive(Error, Debug)]
pub enum BlockError {
    #[error("block processing error: {0}")]
    ProcessingError(String),
    #[error("validation error: {0}")]
    ValidationError(String),
    #[error("Lock acquisition failed")]
    LockError,
    #[error("Block not found: {0}")]
    BlockNotFound(String),
    #[error("Template not found: {0}")]
    TemplateNotFound(String),
    #[error("Invalid template: {0}")]
    InvalidTemplate(String),
    #[error("Missing required property: {0}")]
    MissingProperty(String),
    #[error("Invalid property type: {0}")]
    InvalidPropertyType(String),
    #[error("Unsupported block type: {0}")]
    UnsupportedBlockType(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("API request failed: {0}")]
    ApiRequestError(String),
    #[error("JSON parsing error: {0}")]
    JsonParseError(String),
    #[error("Data path not found in JSON response: {0}")]
    DataPathNotFound(String),

    #[error("Data path extraction failed with discovery: {message}")]
    DataPathNotFoundWithDiscovery {
        message: String,
        metadata: Box<ApiResponseMetadata>,
    },
    #[error("Security violation: {0}")]
    SecurityViolation(String),
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub enum BlockType {
    Conditional,
    Decision,
    Display,
    ExternalData,
    GoTo,
    Input,
    Interactive,
    Random,
    Compute,
    Terminal,
    APIExplorer,
    NativeFunction,
    LLMContentGenerator,
    LLMContentAnalyser,
    StandardProcessor,
    IntelligentDecision,
    OutputAggregator,
    DynamicFunction(String, String),
}
impl BlockType {
    pub fn as_str(&self) -> &str {
        match self {
            BlockType::Conditional => "conditional",
            BlockType::Decision => "decision",
            BlockType::Display => "display",
            BlockType::ExternalData => "external_data",
            BlockType::GoTo => "goto",
            BlockType::Input => "input",
            BlockType::Interactive => "interactive",
            BlockType::Random => "random",
            BlockType::Compute => "compute",
            BlockType::Terminal => "terminal",
            BlockType::APIExplorer => "api_explorer",
            BlockType::NativeFunction => "native_function",
            BlockType::LLMContentGenerator => "llm_content_generator",
            BlockType::LLMContentAnalyser => "llm_content_analyser",
            BlockType::StandardProcessor => "standard_processor",
            BlockType::IntelligentDecision => "intelligent_decision",
            BlockType::OutputAggregator => "output_aggregator",
            BlockType::DynamicFunction(_, _) => "dynamic_function",
        }
    }
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BlockInput {
    pub text: String,
    pub metadata: HashMap<String, serde_json::Value>,
}
pub trait BlockBehaviour: Any + Send + Sync {
    fn id(&self) -> &str;
    fn process<'life0, 'async_trait>(
        &'life0 self,
        state: &'life0 mut HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<BlockResult, BlockError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait;
    fn clone_box(&self) -> Box<dyn BlockBehaviour>;
    fn as_any(&self) -> &dyn Any;
    fn validate(&self) -> Result<(), BlockError>;
}
