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

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
pub type InteractionLogicFn =
    Arc<dyn Fn(&mut Value, &Value) -> Result<(), String> + Send + Sync>;
#[derive(Clone, Default)]
pub struct FunctionRegistry {
    functions: HashMap<String, InteractionLogicFn>,
}
impl FunctionRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(&mut self, name: String, func: InteractionLogicFn) {
        self.functions.insert(name, func);
    }
    pub fn get(&self, name: &str) -> Option<&InteractionLogicFn> {
        self.functions.get(name)
    }
}
