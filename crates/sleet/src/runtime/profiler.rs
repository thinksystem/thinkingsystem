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

use std::collections::HashMap;

pub struct ExecutionProfiler {
    execution_counts: HashMap<String, u64>,
}

impl Default for ExecutionProfiler {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionProfiler {
    pub fn new() -> Self {
        Self {
            execution_counts: HashMap::new(),
        }
    }

    pub fn record_execution(&mut self, hash: &str) {
        *self.execution_counts.entry(hash.to_string()).or_insert(0) += 1;
    }

    pub fn get_execution_count(&self, hash: &str) -> u64 {
        *self.execution_counts.get(hash).unwrap_or(&0)
    }

    pub fn clear(&mut self) {
        self.execution_counts.clear();
    }

    pub fn get_all_counts(&self) -> Vec<(String, u64)> {
        self.execution_counts
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    }
}

pub struct Profiler {
    hot_paths: HashMap<u64, u32>,
}

unsafe impl Send for Profiler {}
unsafe impl Sync for Profiler {}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Profiler {
    pub fn new() -> Self {
        Self {
            hot_paths: HashMap::new(),
        }
    }

    pub fn record(&mut self, bytecode_hash: u64) {
        *self.hot_paths.entry(bytecode_hash).or_insert(0) += 1;
    }

    pub fn is_hot(&self, bytecode_hash: u64) -> bool {
        self.hot_paths.get(&bytecode_hash).unwrap_or(&0) > &2
    }

    pub fn get_execution_count(&self, bytecode_hash: u64) -> u32 {
        *self.hot_paths.get(&bytecode_hash).unwrap_or(&0)
    }
}
