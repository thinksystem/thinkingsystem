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
use std::sync::Arc;
use stele::flows::dynamic_executor::strategy::EvalFn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionGraph {
    pub nodes: Vec<ExecNode>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecNode {
    RangeScan(RangeScanNode),
    SwitchScan(SwitchScanNode),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeScanNode {
    pub id: String,
    pub evaluator: String, 
    pub start: u64,
    pub end: u64,
    #[serde(default = "default_dense_cutoff")] pub prefer_dense_cutoff: u64,
    #[serde(default = "default_shards")] pub shards: u64,
    #[serde(default = "default_chunk")] pub chunk: u64,
    #[serde(default = "default_progress")] pub progress_log_interval: u64,
    #[serde(default)] pub early_stop_no_improve: Option<u64>,
    #[serde(default)] pub parity_rules: Option<serde_json::Value>, 
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchScanNode {
    pub id: String,
    
    pub evaluators: Vec<String>,
    pub start: u64,
    pub end: u64,
    #[serde(default = "default_dense_cutoff")] pub prefer_dense_cutoff: u64,
    #[serde(default = "default_shards")] pub shards: u64,
    #[serde(default = "default_chunk")] pub chunk: u64,
    #[serde(default = "default_progress")] pub progress_log_interval: u64,
    #[serde(default)] pub stage_advance_min_improve: Option<u32>, 
}

fn default_dense_cutoff() -> u64 { 120_000_000 }
fn default_shards() -> u64 { 64 }
fn default_chunk() -> u64 { 1_000_000 }
fn default_progress() -> u64 { 250_000 }


#[derive(Default)]
pub struct EvaluatorRegistry {
    map: HashMap<String, Arc<dyn EvalFn>>,
}
impl EvaluatorRegistry {
    pub fn register(&mut self, name: &str, eval: Arc<dyn EvalFn>) { self.map.insert(name.to_string(), eval); }
    pub fn get(&self, name: &str) -> Option<Arc<dyn EvalFn>> { self.map.get(name).cloned() }
}


