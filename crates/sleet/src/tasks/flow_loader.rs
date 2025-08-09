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

use crate::flows::definition::{BlockDefinition, BlockType, FlowDefinition};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
pub type FlowLoaderResult<T> = Result<T, FlowLoaderError>;
#[derive(Debug, Clone)]
pub enum FlowLoaderError {
    IoError(String),
    JsonError(String),
    ValidationError(String),
    ConversionError(String),
}
impl std::fmt::Display for FlowLoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlowLoaderError::IoError(msg) => write!(f, "IO error: {msg}"),
            FlowLoaderError::JsonError(msg) => write!(f, "JSON error: {msg}"),
            FlowLoaderError::ValidationError(msg) => write!(f, "Validation error: {msg}"),
            FlowLoaderError::ConversionError(msg) => write!(f, "Conversion error: {msg}"),
        }
    }
}
impl std::error::Error for FlowLoaderError {}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFlowDefinition {
    pub id: String,
    pub start_block_id: String,
    pub blocks: Vec<JsonBlockDefinition>,
    pub participants: Vec<String>,
    pub permissions: HashMap<String, Vec<String>>,
    pub initial_state: Option<Value>,
    pub state_schema: Option<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonBlockDefinition {
    pub id: String,
    #[serde(flatten)]
    pub block_type: JsonBlockType,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum JsonBlockType {
    Conditional {
        condition: String,
        true_block: String,
        false_block: String,
    },
    Compute {
        expression: String,
        output_key: String,
        next_block: String,
    },
    AwaitInput {
        interaction_id: String,
        agent_id: String,
        prompt: String,
        state_key: String,
        next_block: String,
    },
    ForEach {
        loop_id: String,
        array_path: String,
        iterator_var: String,
        loop_body_block_id: String,
        exit_block_id: String,
    },
    TryCatch {
        try_block_id: String,
        catch_block_id: String,
    },
    Continue {
        loop_id: String,
    },
    Break {
        loop_id: String,
    },
    Terminate,
}
pub struct FlowLoader {
    strict_validation: bool,
}
impl FlowLoader {
    pub fn new() -> Self {
        Self {
            strict_validation: true,
        }
    }
    pub fn with_config(strict_validation: bool) -> Self {
        Self { strict_validation }
    }
    pub fn load_from_json<P: AsRef<Path>>(&self, path: P) -> FlowLoaderResult<FlowDefinition> {
        let content =
            fs::read_to_string(path).map_err(|e| FlowLoaderError::IoError(e.to_string()))?;
        self.parse_json(&content)
    }
    pub fn parse_json(&self, json_str: &str) -> FlowLoaderResult<FlowDefinition> {
        let json_flow: JsonFlowDefinition = serde_json::from_str(json_str)
            .map_err(|e| FlowLoaderError::JsonError(e.to_string()))?;
        self.convert_json_flow(json_flow)
    }
    fn convert_json_flow(&self, json_flow: JsonFlowDefinition) -> FlowLoaderResult<FlowDefinition> {
        let mut blocks = Vec::new();
        for json_block in &json_flow.blocks {
            blocks.push(self.convert_block_definition(json_block.clone())?);
        }
        if self.strict_validation {
            self.validate_flow(&json_flow, &blocks)?;
        }
        Ok(FlowDefinition {
            id: json_flow.id,
            start_block_id: json_flow.start_block_id,
            blocks,
            participants: json_flow.participants,
            permissions: json_flow.permissions,
            initial_state: json_flow.initial_state,
            state_schema: json_flow.state_schema,
        })
    }
    fn convert_block_definition(
        &self,
        json_block: JsonBlockDefinition,
    ) -> FlowLoaderResult<BlockDefinition> {
        let block_type = match json_block.block_type {
            JsonBlockType::Conditional {
                condition,
                true_block,
                false_block,
            } => BlockType::Conditional {
                condition,
                true_block,
                false_block,
            },
            JsonBlockType::Compute {
                expression,
                output_key,
                next_block,
            } => BlockType::Compute {
                expression,
                output_key,
                next_block,
            },
            JsonBlockType::AwaitInput {
                interaction_id,
                agent_id,
                prompt,
                state_key,
                next_block,
            } => BlockType::AwaitInput {
                interaction_id,
                agent_id,
                prompt,
                state_key,
                next_block,
            },
            JsonBlockType::ForEach {
                loop_id,
                array_path,
                iterator_var,
                loop_body_block_id,
                exit_block_id,
            } => BlockType::ForEach {
                loop_id,
                array_path,
                iterator_var,
                loop_body_block_id,
                exit_block_id,
            },
            JsonBlockType::TryCatch {
                try_block_id,
                catch_block_id,
            } => BlockType::TryCatch {
                try_block_id,
                catch_block_id,
            },
            JsonBlockType::Continue { loop_id } => BlockType::Continue { loop_id },
            JsonBlockType::Break { loop_id } => BlockType::Break { loop_id },
            JsonBlockType::Terminate => BlockType::Terminate,
        };
        Ok(BlockDefinition::new(json_block.id, block_type))
    }
    fn validate_flow(
        &self,
        json_flow: &JsonFlowDefinition,
        blocks: &[BlockDefinition],
    ) -> FlowLoaderResult<()> {
        if !blocks.iter().any(|b| b.id == json_flow.start_block_id) {
            return Err(FlowLoaderError::ValidationError(format!(
                "Start block '{}' not found in flow",
                json_flow.start_block_id
            )));
        }
        for block in blocks {
            self.validate_block_references(block, blocks)?;
        }
        Ok(())
    }
    fn validate_block_references(
        &self,
        block: &BlockDefinition,
        all_blocks: &[BlockDefinition],
    ) -> FlowLoaderResult<()> {
        let check_block_exists = |block_id: &str| -> FlowLoaderResult<()> {
            if !all_blocks.iter().any(|b| b.id == block_id) {
                return Err(FlowLoaderError::ValidationError(format!(
                    "Referenced block '{block_id}' not found in flow"
                )));
            }
            Ok(())
        };
        match &block.block_type {
            BlockType::Conditional {
                true_block,
                false_block,
                ..
            } => {
                check_block_exists(true_block)?;
                check_block_exists(false_block)?;
            }
            BlockType::Compute { next_block, .. } => {
                check_block_exists(next_block)?;
            }
            BlockType::AwaitInput { next_block, .. } => {
                check_block_exists(next_block)?;
            }
            BlockType::ForEach {
                loop_body_block_id,
                exit_block_id,
                ..
            } => {
                check_block_exists(loop_body_block_id)?;
                check_block_exists(exit_block_id)?;
            }
            BlockType::TryCatch {
                try_block_id,
                catch_block_id,
            } => {
                check_block_exists(try_block_id)?;
                check_block_exists(catch_block_id)?;
            }
            _ => {}
        }
        Ok(())
    }
}
impl Default for FlowLoader {
    fn default() -> Self {
        Self::new()
    }
}
pub fn load_flow_from_json<P: AsRef<Path>>(path: P) -> FlowLoaderResult<FlowDefinition> {
    let loader = FlowLoader::new();
    loader.load_from_json(path)
}
pub fn parse_flow_json(json_str: &str) -> FlowLoaderResult<FlowDefinition> {
    let loader = FlowLoader::new();
    loader.parse_json(json_str)
}
pub struct FlowLoaderBuilder {
    strict_validation: bool,
}
impl FlowLoaderBuilder {
    pub fn new() -> Self {
        Self {
            strict_validation: true,
        }
    }
    pub fn with_strict_validation(mut self, strict: bool) -> Self {
        self.strict_validation = strict;
        self
    }
    pub fn build(self) -> FlowLoader {
        FlowLoader::with_config(self.strict_validation)
    }
}
impl Default for FlowLoaderBuilder {
    fn default() -> Self {
        Self::new()
    }
}
pub fn validate_flow_structure(flow: &FlowDefinition) -> FlowLoaderResult<Vec<String>> {
    let mut warnings = Vec::new();
    let mut referenced_blocks = std::collections::HashSet::new();
    referenced_blocks.insert(flow.start_block_id.clone());
    for block in &flow.blocks {
        match &block.block_type {
            BlockType::Conditional {
                true_block,
                false_block,
                ..
            } => {
                referenced_blocks.insert(true_block.clone());
                referenced_blocks.insert(false_block.clone());
            }
            BlockType::Compute { next_block, .. } => {
                referenced_blocks.insert(next_block.clone());
            }
            BlockType::AwaitInput { next_block, .. } => {
                referenced_blocks.insert(next_block.clone());
            }
            BlockType::ForEach {
                loop_body_block_id,
                exit_block_id,
                ..
            } => {
                referenced_blocks.insert(loop_body_block_id.clone());
                referenced_blocks.insert(exit_block_id.clone());
            }
            BlockType::TryCatch {
                try_block_id,
                catch_block_id,
            } => {
                referenced_blocks.insert(try_block_id.clone());
                referenced_blocks.insert(catch_block_id.clone());
            }
            _ => {}
        }
    }
    for block in &flow.blocks {
        if !referenced_blocks.contains(&block.id) {
            warnings.push(format!(
                "Block '{}' is not referenced by any other block",
                block.id
            ));
        }
    }
    Ok(warnings)
}
pub fn create_test_flow() -> FlowDefinition {
    let mut flow = FlowDefinition::new("test_flow", "start");
    flow.add_block(BlockDefinition::new(
        "start",
        BlockType::Compute {
            expression: "1 + 1".to_string(),
            output_key: "result".to_string(),
            next_block: "end".to_string(),
        },
    ));
    flow.add_block(BlockDefinition::new("end", BlockType::Terminate));
    flow
}
