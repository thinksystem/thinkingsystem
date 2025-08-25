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


use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct StateMetrics {
    pub data_size_bytes: usize,
    pub metadata_size_bytes: usize,
    pub access_count: u64,
    pub modification_count: u64,
    pub last_access: DateTime<Utc>,
    pub last_modification: DateTime<Utc>,
    pub serialization_count: u64,
    pub deserialization_count: u64,
}

impl Default for StateMetrics {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            data_size_bytes: 0,
            metadata_size_bytes: 0,
            access_count: 0,
            modification_count: 0,
            last_access: now,
            last_modification: now,
            serialization_count: 0,
            deserialization_count: 0,
        }
    }
}

impl StateMetrics {
    pub fn recalc_sizes(
        &mut self,
        data: &HashMap<String, Value>,
        metadata: &HashMap<String, Value>,
    ) {
        self.data_size_bytes = approximate_size(data);
        self.metadata_size_bytes = approximate_size(metadata);
    }
}

fn approximate_size(map: &HashMap<String, Value>) -> usize {
    map.iter()
        .map(|(k, v)| k.len() + serde_json::to_vec(v).map(|b| b.len()).unwrap_or(0))
        .sum()
}
