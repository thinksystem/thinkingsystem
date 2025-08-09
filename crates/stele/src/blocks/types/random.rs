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

use crate::blocks::base::BaseBlock;
use crate::blocks::rules::{BlockBehaviour, BlockError, BlockResult};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
const DEFAULT_OUTPUT_KEY: &str = "random_result";
#[derive(Debug)]
struct ParsedOption {
    target: String,
    weight: f64,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct RandomBlock {
    #[serde(flatten)]
    base: BaseBlock,
}
impl RandomBlock {
    pub fn new(id: String, properties: HashMap<String, serde_json::Value>) -> Self {
        Self {
            base: BaseBlock::new(id, properties),
        }
    }
    fn parse_options_from_value(
        raw_options: &[serde_json::Value],
    ) -> Result<Vec<ParsedOption>, BlockError> {
        raw_options
            .iter()
            .map(|opt_val| {
                let obj = opt_val.as_object().ok_or_else(|| {
                    BlockError::InvalidPropertyType(
                        "Option in 'options' array must be an object.".to_string(),
                    )
                })?;
                let target = obj
                    .get("target")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        BlockError::MissingProperty(
                            "Option object missing 'target' string property.".to_string(),
                        )
                    })?
                    .to_string();
                let weight = obj.get("weight").and_then(|v| v.as_f64()).ok_or_else(|| {
                    BlockError::MissingProperty(
                        "Option object missing 'weight' number property.".to_string(),
                    )
                })?;
                if weight < 0.0 {
                    return Err(BlockError::InvalidPropertyType(
                        "Option 'weight' cannot be negative.".to_string(),
                    ));
                }
                Ok(ParsedOption { target, weight })
            })
            .collect()
    }
    fn calculate_weighted_choice(&self, options: &[ParsedOption]) -> Result<String, BlockError> {
        if options.is_empty() {
            return Err(BlockError::ProcessingError(
                "Options array cannot be empty for weighted choice.".to_string(),
            ));
        }
        let mut rng = rand::thread_rng();
        let total_weight: f64 = options.iter().map(|opt| opt.weight).sum();
        if total_weight <= 0.0 {
            return Err(BlockError::ProcessingError(
                "Total weight of options must be positive.".to_string(),
            ));
        }
        let mut random_target_value = rng.gen_range(0.0..total_weight);
        for option in options {
            if option.weight > 0.0 {
                random_target_value -= option.weight;
                if random_target_value <= 0.0 {
                    return Ok(option.target.clone());
                }
            }
        }
        if let Some(first_option) = options.iter().find(|opt| opt.weight > 0.0) {
            return Ok(first_option.target.clone());
        }
        Err(BlockError::ProcessingError("No valid option found after weighted selection. Ensure at least one option has positive weight.".to_string()))
    }
}
impl BlockBehaviour for RandomBlock {
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
            let raw_options_array = self.base.get_required_array("options")?;
            let parsed_options = Self::parse_options_from_value(raw_options_array)?;
            let target = self.calculate_weighted_choice(&parsed_options)?;
            let output_key = self
                .base
                .get_optional_string("output_key")?
                .unwrap_or_else(|| DEFAULT_OUTPUT_KEY.to_string());
            state.insert(output_key, serde_json::Value::String(target.clone()));
            state.insert(
                "navigation_type".to_string(),
                serde_json::Value::String("random".to_string()),
            );
            state.insert(
                "navigation_priority".to_string(),
                serde_json::Value::Number(self.base.priority.into()),
            );
            state.insert(
                "is_override".to_string(),
                serde_json::Value::Bool(self.base.is_override),
            );
            Ok(BlockResult::Navigate {
                target,
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
        if options_array.is_empty() {
            return Err(BlockError::InvalidPropertyType(
                "options array cannot be empty for RandomBlock".to_string(),
            ));
        }
        let mut has_positive_weight = false;
        for (index, option) in options_array.iter().enumerate() {
            let option_obj = option.as_object().ok_or_else(|| {
                BlockError::InvalidPropertyType(format!(
                    "option at index {index} must be an object"
                ))
            })?;
            if option_obj.get("target").and_then(|v| v.as_str()).is_none() {
                return Err(BlockError::InvalidPropertyType(format!(
                    "option at index {index} must have a string 'target' property"
                )));
            }
            let weight_val = option_obj.get("weight").ok_or_else(|| {
                BlockError::MissingProperty(format!(
                    "option at index {index} must have a 'weight' property"
                ))
            })?;
            let weight = weight_val.as_f64().ok_or_else(|| {
                BlockError::InvalidPropertyType(format!(
                    "option at index {index} 'weight' property must be a number"
                ))
            })?;
            if weight < 0.0 {
                return Err(BlockError::InvalidPropertyType(format!(
                    "option at index {index} 'weight' cannot be negative"
                )));
            }
            if weight > 0.0 {
                has_positive_weight = true;
            }
        }
        if !has_positive_weight {
            return Err(BlockError::InvalidPropertyType(
                "At least one option must have a positive weight".to_string(),
            ));
        }
        self.base.get_optional_string("output_key")?;
        Ok(())
    }
}
