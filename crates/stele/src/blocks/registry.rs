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

use crate::blocks::rules::{BlockBehaviour, BlockError, BlockType};
use crate::blocks::{
    ComputeBlock, ConditionalBlock, DecisionBlock, DisplayBlock, ExternalDataBlock, GoToBlock,
    InputBlock, InteractiveBlock, RandomBlock, TerminalBlock,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::RwLock;
pub type BlockConstructor = fn(String, HashMap<String, Value>) -> Box<dyn BlockBehaviour>;
#[derive(Default)]
pub struct BlockRegistry {
    templates: RwLock<HashMap<String, Value>>,
    instances: RwLock<HashMap<String, Box<dyn BlockBehaviour>>>,
    constructors: RwLock<HashMap<String, BlockConstructor>>,
}
impl BlockRegistry {
    pub fn new() -> Self {
        Self {
            templates: RwLock::new(HashMap::new()),
            instances: RwLock::new(HashMap::new()),
            constructors: RwLock::new(HashMap::new()),
        }
    }
    pub fn with_standard_blocks() -> Result<Self, BlockError> {
        let registry = Self::new();
        registry.register("conditional", |id, props| {
            Box::new(ConditionalBlock::new(id, props))
        })?;
        registry.register("decision", |id, props| {
            Box::new(DecisionBlock::new(id, props))
        })?;
        registry.register("display", |id, props| {
            Box::new(DisplayBlock::new(id, props))
        })?;
        registry.register("external_data", |id, props| {
            Box::new(ExternalDataBlock::new(id, props))
        })?;
        registry.register("goto", |id, props| Box::new(GoToBlock::new(id, props)))?;
        registry.register("input", |id, props| Box::new(InputBlock::new(id, props)))?;
        registry.register("interactive", |id, props| {
            Box::new(InteractiveBlock::new(id, props))
        })?;
        registry.register("random", |id, props| Box::new(RandomBlock::new(id, props)))?;
        registry.register("compute", |id, props| {
            Box::new(ComputeBlock::new(id, props))
        })?;
        registry.register("terminal", |id, props| {
            Box::new(TerminalBlock::new(id, props))
        })?;
        Ok(registry)
    }
    pub fn register(
        &self,
        block_type_name: &str,
        constructor: BlockConstructor,
    ) -> Result<(), BlockError> {
        let mut constructors_map = self
            .constructors
            .write()
            .map_err(|_| BlockError::LockError)?;
        constructors_map.insert(block_type_name.to_string(), constructor);
        Ok(())
    }
    pub fn get_available_block_types(&self) -> Result<Vec<String>, BlockError> {
        let constructors_map = self
            .constructors
            .read()
            .map_err(|_| BlockError::LockError)?;
        Ok(constructors_map.keys().cloned().collect())
    }
    pub fn register_templates(&self, templates: Value) -> Result<(), BlockError> {
        let mut template_map = self.templates.write().map_err(|_| BlockError::LockError)?;
        if let Value::Object(template_obj) = templates {
            for (key, value) in template_obj {
                template_map.insert(key, value);
            }
            Ok(())
        } else {
            Err(BlockError::InvalidTemplate(
                "Templates must be an object".into(),
            ))
        }
    }
    pub fn create_block_from_type_name(
        &self,
        block_type_name: &str,
        id: String,
        mut properties: HashMap<String, Value>,
    ) -> Result<Box<dyn BlockBehaviour>, BlockError> {
        let templates_map = self.templates.read().map_err(|_| BlockError::LockError)?;
        if let Some(Value::Object(template_props)) = templates_map.get(block_type_name) {
            for (key, value) in template_props {
                properties
                    .entry(key.clone())
                    .or_insert_with(|| value.clone());
            }
        }
        let constructors_map = self
            .constructors
            .read()
            .map_err(|_| BlockError::LockError)?;
        let constructor = constructors_map
            .get(block_type_name)
            .ok_or_else(|| BlockError::UnsupportedBlockType(block_type_name.to_string()))?;
        let block = constructor(id.clone(), properties);
        block.validate()?;
        let mut instances_map = self.instances.write().map_err(|_| BlockError::LockError)?;
        instances_map.insert(id, block.clone_box());
        Ok(block)
    }
    pub fn create_block(
        &self,
        block_type: BlockType,
        id: String,
        properties: HashMap<String, Value>,
    ) -> Result<Box<dyn BlockBehaviour>, BlockError> {
        self.create_block_from_type_name(block_type.as_str(), id, properties)
    }
    pub fn get_block(&self, id: &str) -> Result<Box<dyn BlockBehaviour>, BlockError> {
        let instances_map = self.instances.read().map_err(|_| BlockError::LockError)?;
        instances_map
            .get(id)
            .map(|block| block.clone_box())
            .ok_or_else(|| BlockError::BlockNotFound(id.to_string()))
    }
    pub fn list_templates(&self) -> Result<Vec<String>, BlockError> {
        let templates_map = self.templates.read().map_err(|_| BlockError::LockError)?;
        Ok(templates_map.keys().cloned().collect())
    }
    pub fn clear_instances(&self) -> Result<(), BlockError> {
        let mut instances_map = self.instances.write().map_err(|_| BlockError::LockError)?;
        instances_map.clear();
        Ok(())
    }
    pub fn clear_templates(&self) -> Result<(), BlockError> {
        let mut templates_map = self.templates.write().map_err(|_| BlockError::LockError)?;
        templates_map.clear();
        Ok(())
    }
    pub fn clear_all(&self) -> Result<(), BlockError> {
        self.clear_instances()?;
        self.clear_templates()?;
        Ok(())
    }
}
