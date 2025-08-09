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

use crate::blocks::base::{evaluate_condition_str, BaseBlock};
use crate::blocks::rules::{BlockBehaviour, BlockError, BlockResult};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
const DEFAULT_OUTPUT_KEY: &str = "decision_result";
#[derive(Clone, Deserialize, Serialize)]
pub struct DecisionBlock {
    #[serde(flatten)]
    base: BaseBlock,
}
impl DecisionBlock {
    pub fn new(id: String, properties: HashMap<String, serde_json::Value>) -> Self {
        Self {
            base: BaseBlock::new(id, properties),
        }
    }
}
impl BlockBehaviour for DecisionBlock {
    fn id(&self) -> &str {
        &self.base.id
    }
    fn process<'life0, 'async_trait>(
        &'life0 self,
        state: &'life0 mut HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<BlockResult, BlockError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            let options = self.base.get_required_array("options")?;
            for option in options {
                let condition_value = option.get("condition");
                let condition_met = match condition_value {
                    Some(cond_val) if cond_val.is_string() => {
                        evaluate_condition_str(cond_val.as_str().unwrap(), state)
                    }
                    Some(cond_val) if cond_val.is_null() => true,
                    None => true,
                    _ => false,
                };
                if condition_met {
                    let target =
                        option
                            .get("target")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                BlockError::MissingProperty(
                                    "Missing or invalid 'target' string property in option"
                                        .to_string(),
                                )
                            })?;
                    let output_key = self
                        .base
                        .get_optional_string("output_key")?
                        .unwrap_or_else(|| DEFAULT_OUTPUT_KEY.to_string());
                    state.insert(output_key, serde_json::Value::String(target.to_string()));
                    state.insert(
                        "navigation_type".to_string(),
                        serde_json::Value::String("decision".to_string()),
                    );
                    state.insert(
                        "navigation_priority".to_string(),
                        serde_json::Value::Number(self.base.priority.into()),
                    );
                    state.insert(
                        "is_override".to_string(),
                        serde_json::Value::Bool(self.base.is_override),
                    );
                    return Ok(BlockResult::Navigate {
                        target: target.to_string(),
                        priority: self.base.priority,
                        is_override: self.base.is_override,
                    });
                }
            }
            let default_target = self.base.get_required_string("default_target")?;
            Ok(BlockResult::Navigate {
                target: default_target,
                priority: self.base.priority,
                is_override: self.base.is_override,
            })
        })
    }
    fn clone_box(&self) -> Box<dyn BlockBehaviour> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn validate(&self) -> Result<(), BlockError> {
        let options_array = self.base.get_required_array("options")?;
        for (index, option) in options_array.iter().enumerate() {
            let option_obj = option.as_object().ok_or_else(|| {
                BlockError::InvalidPropertyType(format!(
                    "Option at index {index} must be an object"
                ))
            })?;
            if option_obj.get("target").and_then(|v| v.as_str()).is_none() {
                return Err(BlockError::InvalidPropertyType(format!(
                    "Option at index {index} must have a string 'target' property"
                )));
            }
            if let Some(condition_val) = option_obj.get("condition") {
                if !condition_val.is_string() && !condition_val.is_null() {
                    return Err(BlockError::InvalidPropertyType(format!(
                        "Option at index {index} 'condition' property must be a string or null"
                    )));
                }
            }
        }
        self.base.get_required_string("default_target")?;
        self.base.get_optional_string("output_key")?;
        Ok(())
    }
}
