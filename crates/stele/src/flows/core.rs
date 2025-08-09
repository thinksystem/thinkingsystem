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

use crate::blocks::rules::{BlockError, BlockType};
use crate::flows::flowgorithm::Binder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
#[derive(Serialize, Deserialize)]
pub struct ChannelState {
    pub user_id: String,
    pub operator_id: String,
    pub channel_id: String,
    pub flow: Option<Value>,
    pub skill: Option<Value>,
    pub block_id: Option<String>,
    pub data: HashMap<String, Value>,
    pub extra: HashMap<String, Value>,
    pub binder: Option<Binder>,
}
impl ChannelState {
    pub fn new(user_id: String, operator_id: String, channel_id: String) -> Self {
        Self {
            user_id,
            operator_id,
            channel_id,
            flow: None,
            skill: None,
            block_id: None,
            data: HashMap::new(),
            extra: HashMap::new(),
            binder: None,
        }
    }
}
impl FlowStateManager for ChannelState {
    fn update_state(&mut self, data: HashMap<String, Value>) {
        self.data = data;
    }
    fn get_flow_state(&self) -> HashMap<String, Value> {
        self.data.clone()
    }
}
#[derive(Deserialize, Serialize, Clone)]
pub struct FlowDefinition {
    pub id: String,
    pub name: String,
    pub start_block_id: String,
    pub blocks: Vec<BlockDefinition>,
}
pub trait FlowValidation {
    fn validate_flow_properties(&self, blocks: &[BlockDefinition]) -> Result<(), BlockError>;
}
struct FlowValidationRules {
    required_props: &'static [&'static str],
    target_props: &'static [&'static str],
    array_props: &'static [&'static str],
}
impl FlowValidationRules {
    fn for_block_type(block_type: &BlockType) -> Self {
        match block_type {
            BlockType::GoTo => Self {
                required_props: &["target"],
                target_props: &["target"],
                array_props: &[],
            },
            BlockType::Conditional => Self {
                required_props: &["condition", "true_block", "false_block"],
                target_props: &["true_block", "false_block"],
                array_props: &[],
            },
            BlockType::Decision => Self {
                required_props: &["options", "default"],
                target_props: &["default"],
                array_props: &["options"],
            },
            BlockType::Random => Self {
                required_props: &["options", "weights"],
                target_props: &[],
                array_props: &["options", "weights"],
            },
            BlockType::Interactive => Self {
                required_props: &["prompt", "options"],
                target_props: &[],
                array_props: &["options"],
            },
            _ => Self {
                required_props: &[],
                target_props: &[],
                array_props: &[],
            },
        }
    }
    fn validate(
        &self,
        block: &BlockDefinition,
        blocks: &[BlockDefinition],
    ) -> Result<(), BlockError> {
        for prop in self.required_props {
            if !block.properties.contains_key(*prop) {
                return Err(BlockError::MissingProperty((*prop).to_string()));
            }
        }
        for prop in self.target_props {
            if let Some(target) = block.properties.get(*prop).and_then(|v| v.as_str()) {
                if !blocks.iter().any(|b| b.id == target) {
                    return Err(BlockError::ValidationError(format!(
                        "Target block {target} not found"
                    )));
                }
            }
        }
        for prop in self.array_props {
            if let Some(value) = block.properties.get(*prop) {
                if !value.is_array() {
                    return Err(BlockError::InvalidPropertyType(format!(
                        "{prop} must be an array"
                    )));
                }
            }
        }
        Ok(())
    }
}
impl FlowValidation for BlockDefinition {
    fn validate_flow_properties(&self, blocks: &[BlockDefinition]) -> Result<(), BlockError> {
        let validation_rules = FlowValidationRules::for_block_type(&self.block_type);
        validation_rules.validate(self, blocks)
    }
}
impl FlowDefinition {
    pub fn validate(&self) -> Result<(), BlockError> {
        if self.blocks.is_empty() {
            return Err(BlockError::ValidationError("Empty flow".into()));
        }
        if !self.blocks.iter().any(|b| b.id == self.start_block_id) {
            return Err(BlockError::ValidationError("Invalid start block".into()));
        }
        for block in &self.blocks {
            block.validate_flow_properties(&self.blocks)?;
        }
        Ok(())
    }
}
#[derive(Deserialize, Serialize, Clone)]
pub struct BlockDefinition {
    pub id: String,
    pub block_type: BlockType,
    pub properties: HashMap<String, Value>,
}
pub trait FlowStateManager {
    fn update_state(&mut self, data: HashMap<String, Value>);
    fn get_flow_state(&self) -> HashMap<String, Value>;
}
#[derive(Default)]
pub struct FlowBuilder {
    id: String,
    name: String,
    start_block_id: String,
    blocks: Vec<BlockDefinition>,
}
impl FlowBuilder {
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            start_block_id: String::new(),
            blocks: Vec::new(),
        }
    }
    pub fn set_start_block(&mut self, block_id: String) -> &mut Self {
        self.start_block_id = block_id;
        self
    }
    pub fn add_block(
        &mut self,
        id: String,
        block_type: BlockType,
        properties: HashMap<String, Value>,
    ) -> &mut Self {
        self.blocks.push(BlockDefinition {
            id,
            block_type,
            properties,
        });
        self
    }
    pub fn build(self) -> Result<FlowDefinition, BlockError> {
        if self.start_block_id.is_empty() {
            return Err(BlockError::ValidationError("Start block ID not set".into()));
        }
        let flow = FlowDefinition {
            id: self.id,
            name: self.name,
            start_block_id: self.start_block_id,
            blocks: self.blocks,
        };
        flow.validate()?;
        Ok(flow)
    }
}
