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

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenExtractor {
    #[serde(default)]
    pub rules: HashMap<String, String>,
    #[serde(default)]
    pub config: PromptsConfig,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptsConfig {
    #[serde(default = "default_max_length")]
    pub max_length: usize,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_top_p")]
    pub top_p: f32,
}

fn default_max_length() -> usize {
    2048
}
fn default_temperature() -> f32 {
    0.7
}
fn default_top_p() -> f32 {
    0.9
}

impl TokenExtractor {
    pub fn with_config(config: PromptsConfig) -> Self {
        Self {
            rules: HashMap::new(),
            config,
        }
    }
    pub fn add_rule(&mut self, key: String, value: String) {
        self.rules.insert(key, value);
    }
    pub fn extract(&self, input: &str) -> Vec<String> {
        input.split_whitespace().map(|s| s.to_string()).collect()
    }
}
