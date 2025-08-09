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
use serde_json::Value;
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
const DEFAULT_INPUT_KEY: &str = "input_result";
#[derive(Clone, Deserialize, Serialize)]
pub struct InputBlock {
    #[serde(flatten)]
    base: BaseBlock,
}
impl InputBlock {
    pub fn new(id: String, properties: HashMap<String, Value>) -> Self {
        Self {
            base: BaseBlock::new(id, properties),
        }
    }
}
impl BlockBehaviour for InputBlock {
    fn id(&self) -> &str {
        &self.base.id
    }
    fn process<'life0, 'async_trait>(
        &'life0 self,
        state: &'life0 mut HashMap<String, Value>,
    ) -> Pin<Box<dyn Future<Output = Result<BlockResult, BlockError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            let prompt = self.base.get_required_string("prompt")?;
            let state_key = self
                .base
                .get_optional_string("input_key")?
                .unwrap_or_else(|| DEFAULT_INPUT_KEY.to_string());
            state.insert(
                "navigation_type".to_string(),
                Value::String("input_await".to_string()),
            );
            state.insert(
                "navigation_priority".to_string(),
                Value::Number(self.base.priority.into()),
            );
            state.insert(
                "is_override".to_string(),
                Value::Bool(self.base.is_override),
            );
            Ok(BlockResult::AwaitInput { prompt, state_key })
        })
    }
    fn clone_box(&self) -> Box<dyn BlockBehaviour> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn validate(&self) -> Result<(), BlockError> {
        self.base.get_required_string("prompt")?;
        self.base.get_optional_string("input_key")?;
        self.base.get_optional_f64("priority")?;
        self.base.get_optional_bool("is_override")?;
        Ok(())
    }
}
