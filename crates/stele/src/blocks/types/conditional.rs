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

use crate::blocks::base::{
    evaluate_condition_str, evaluate_parsed_condition, parse_condition, BaseBlock, ConditionSet,
    ParsedCondition,
};
use crate::blocks::rules::{BlockBehaviour, BlockError, BlockResult};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
const DEFAULT_OUTPUT_KEY: &str = "condition_result";
#[derive(Clone, Deserialize, Serialize)]
pub struct ConditionalBlock {
    #[serde(flatten)]
    base: BaseBlock,
    #[serde(skip_serializing_if = "Option::is_none")]
    condition_set: Option<ConditionSet>,
    #[serde(skip)]
    parsed_condition: Option<ParsedCondition>,
}
impl ConditionalBlock {
    pub fn new(id: String, properties: HashMap<String, serde_json::Value>) -> Self {
        let parsed_condition = properties
            .get("condition")
            .and_then(|v| v.as_str())
            .and_then(|condition_str| parse_condition(condition_str).ok());
        Self {
            base: BaseBlock::new(id, properties),
            condition_set: None,
            parsed_condition,
        }
    }
    pub fn new_with_condition_set(
        id: String,
        properties: HashMap<String, serde_json::Value>,
        condition_set: ConditionSet,
    ) -> Self {
        Self {
            base: BaseBlock::new(id, properties),
            condition_set: Some(condition_set),
            parsed_condition: None,
        }
    }
    fn evaluate_conditions(&self, state: &HashMap<String, serde_json::Value>) -> bool {
        if let Some(ref condition_set) = self.condition_set {
            return condition_set.evaluate(state);
        }
        if let Some(ref parsed) = self.parsed_condition {
            return evaluate_parsed_condition(parsed, state);
        }
        if let Ok(Some(condition_str)) = self.base.get_optional_string("condition") {
            return evaluate_condition_str(&condition_str, state);
        }
        false
    }
}
impl BlockBehaviour for ConditionalBlock {
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
            let condition_result = self.evaluate_conditions(state);
            let target_prop_name = if condition_result {
                "true_block"
            } else {
                "false_block"
            };
            let target = self.base.get_required_string(target_prop_name)?;
            let output_key = self
                .base
                .get_optional_string("output_key")?
                .unwrap_or_else(|| DEFAULT_OUTPUT_KEY.to_string());
            state.insert(output_key, serde_json::Value::Bool(condition_result));
            state.insert(
                "navigation_type".to_string(),
                serde_json::Value::String("conditional".to_string()),
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
        let has_simple_condition = self.base.properties.contains_key("condition");
        let has_condition_set = self.condition_set.is_some();
        if !has_simple_condition && !has_condition_set {
            return Err(BlockError::MissingProperty(
                "Either 'condition' or 'condition_set' must be provided".to_string(),
            ));
        }
        if has_simple_condition {
            self.base.get_required_string("condition")?;
        }
        self.base.get_required_string("true_block")?;
        self.base.get_required_string("false_block")?;
        self.base.get_optional_string("output_key")?;
        Ok(())
    }
}
