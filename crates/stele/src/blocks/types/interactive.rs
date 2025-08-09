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
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
const DEFAULT_CHOICE_KEY: &str = "selected_option";
#[derive(Clone, Deserialize, Serialize)]
pub struct InteractiveBlock {
    #[serde(flatten)]
    base: BaseBlock,
}
impl InteractiveBlock {
    pub fn new(id: String, properties: HashMap<String, serde_json::Value>) -> Self {
        Self {
            base: BaseBlock::new(id, properties),
        }
    }
}
impl BlockBehaviour for InteractiveBlock {
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
            let choice_key = self
                .base
                .get_optional_string("choice_key")?
                .unwrap_or_else(|| DEFAULT_CHOICE_KEY.to_string());
            if let Some(selected_index_val) = state.get(&choice_key) {
                let selected_index = selected_index_val.as_u64().ok_or_else(|| {
                    BlockError::ProcessingError(format!(
                        "State key '{choice_key}' must be a u64 index for InteractiveBlock"
                    ))
                })?;
                let options = self.base.get_required_array("options")?;
                let selected_option_obj =
                    options.get(selected_index as usize).ok_or_else(|| {
                        BlockError::ProcessingError(format!(
                            "'selected_option' index {} is out of range for options array (len {})",
                            selected_index,
                            options.len()
                        ))
                    })?;
                let target = selected_option_obj.get("target")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| BlockError::MissingProperty(format!("Missing or invalid 'target' string property in selected option at index {selected_index}")))?;
                state.insert(
                    "navigation_type".to_string(),
                    serde_json::Value::String("interactive".to_string()),
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
                    target: target.to_string(),
                    priority: self.base.priority,
                    is_override: self.base.is_override,
                })
            } else {
                let question = self.base.get_required_string("question")?;
                let options = self.base.get_required_array("options")?.clone();
                Ok(BlockResult::AwaitChoice {
                    question,
                    options,
                    state_key: choice_key,
                })
            }
        })
    }
    fn clone_box(&self) -> Box<dyn BlockBehaviour> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn validate(&self) -> Result<(), BlockError> {
        self.base.get_required_string("question")?;
        let options_array = self.base.get_required_array("options")?;
        if options_array.is_empty() {
            return Err(BlockError::InvalidPropertyType(
                "options array cannot be empty for an interactive block".to_string(),
            ));
        }
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
            if option_obj.get("label").and_then(|v| v.as_str()).is_none()
                && option_obj.get("text").and_then(|v| v.as_str()).is_none()
            {
                return Err(BlockError::MissingProperty(format!("option at index {index} must have a string 'label' or 'text' property for display")));
            }
        }
        self.base.get_optional_string("choice_key")?;
        Ok(())
    }
}
