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

use crate::flows::dynamic_executor::registry::FunctionRegistry;
use crate::blocks::rules::{Block, BlockResult, BlockError};
use serde_json::Value;
use async_trait::async_trait;
pub struct NativeFunctionBlock {
    function_name: String,
    registry: FunctionRegistry,
    next_block: String,
}
#[async_trait]
impl Block for NativeFunctionBlock {
    async fn process(&self, state: &mut Value) -> Result<BlockResult, BlockError> {
        let func = self.registry.get(&self.function_name).ok_or_else(|| {
            BlockError::ConfigurationError(format!("Function '{}' not found in registry", self.function_name))
        })?;
        let game_state = state.pointer_mut("/game_state").ok_or_else(|| BlockError::ProcessingError("Missing 'game_state'".to_string()))?;
        let current_move = state.pointer("/current_move").ok_or_else(|| BlockError::ProcessingError("Missing 'current_move'".to_string()))?;
        func(game_state, current_move).map_err(BlockError::ProcessingError)?;
        Ok(BlockResult::Next(self.next_block.clone()))
    }
}
