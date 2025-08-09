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
use serde_json::Value;
use std::collections::{HashMap, HashSet};
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Binder {
    connections: HashMap<String, String>,
    weights: HashMap<String, f64>,
    metadata: HashMap<String, Value>,
    start_block: String,
    conditional_routes: HashMap<String, HashMap<String, String>>,
}
impl Binder {
    pub fn set_start_block(&mut self, block_id: String) -> &mut Self {
        self.start_block = block_id;
        self
    }
    pub fn get_next_block_with_priority(
        &self,
        current_block: &str,
        priority_target: Option<(&str, i32)>,
    ) -> Option<String> {
        if let Some((target, priority)) = priority_target {
            if priority > 0 {
                return Some(target.to_string());
            }
        }
        self.get_next_block(current_block, None)
    }
    pub fn get_start_block(&self) -> &str {
        &self.start_block
    }
    pub fn add_connection(&mut self, from: String, to: String) -> &mut Self {
        self.connections.insert(from, to);
        self
    }
    pub fn add_conditional_route(
        &mut self,
        from: String,
        condition: String,
        to: String,
    ) -> &mut Self {
        self.conditional_routes
            .entry(from)
            .or_default()
            .insert(condition, to);
        self
    }
    pub fn add_weight(&mut self, block_id: String, weight: f64) -> &mut Self {
        self.weights.insert(block_id, weight);
        self
    }
    pub fn add_metadata(&mut self, key: String, value: Value) -> &mut Self {
        self.metadata.insert(key, value);
        self
    }
    pub fn get_weight(&self, block_id: &str) -> Option<f64> {
        self.weights.get(block_id).copied()
    }
    pub fn get_next_block(&self, current_block: &str, condition: Option<&str>) -> Option<String> {
        if let Some(condition) = condition {
            if let Some(routes) = self.conditional_routes.get(current_block) {
                if let Some(next) = routes.get(condition) {
                    return Some(next.clone());
                }
            }
        }
        self.connections.get(current_block).cloned()
    }
    pub fn validate_connections(&self) -> bool {
        let mut visited = HashSet::new();
        let mut current = Some(&self.start_block);
        while let Some(block_id) = current {
            if visited.contains(block_id) {
                return false;
            }
            visited.insert(block_id);
            current = self.connections.get(block_id);
        }
        true
    }
    pub fn optimise_weights(&mut self) {
        let total_weight: f64 = self.weights.values().sum();
        if total_weight > 0.0 {
            for weight in self.weights.values_mut() {
                *weight /= total_weight;
            }
        }
    }
    pub fn get_metadata(&self, block_id: &str) -> Option<&Value> {
        self.metadata.get(block_id)
    }
    pub fn validate_path(&self, path: &[String]) -> bool {
        if path.is_empty() {
            return false;
        }
        for window in path.windows(2) {
            if let Some(next) = self.connections.get(&window[0]) {
                if next != &window[1] {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }
}
pub trait FlowNavigator {
    fn get_next_block(&self, current_block: &str, condition: Option<&str>) -> Option<String>;
    fn calculate_path(&self, from: &str, to: &str) -> Vec<String>;
    fn process_flow_progression(&self, start_block: &str) -> Vec<String>;
}
#[derive(Default)]
pub struct Flowgorithm {
    binders: HashMap<String, Binder>,
}
impl Flowgorithm {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register_binder(&mut self, flow_id: String, binder: Binder) {
        self.binders.insert(flow_id, binder);
    }
    pub fn get_binder(&self, flow_id: &str) -> Option<&Binder> {
        self.binders.get(flow_id)
    }
    pub fn get_binder_mut(&mut self, flow_id: &str) -> Option<&mut Binder> {
        self.binders.get_mut(flow_id)
    }
    pub fn clear_flow(&mut self, flow_id: &str) {
        self.binders.remove(flow_id);
    }
    pub fn validate_all_flows(&self) -> bool {
        self.binders
            .values()
            .all(|binder| binder.validate_connections())
    }
    pub fn get_flow_progression(&self, flow_id: &str) -> Option<Vec<String>> {
        self.binders.get(flow_id).map(|binder| {
            let mut progression = Vec::new();
            let mut current = binder.get_start_block().to_string();
            while !current.is_empty() {
                progression.push(current.clone());
                if let Some(next) = binder.get_next_block(&current, None) {
                    current = next;
                } else {
                    break;
                }
            }
            progression
        })
    }
}
impl FlowNavigator for Flowgorithm {
    fn get_next_block(&self, current_block: &str, condition: Option<&str>) -> Option<String> {
        for binder in self.binders.values() {
            if let Some(next) = binder.get_next_block(current_block, condition) {
                return Some(next);
            }
        }
        None
    }
    fn calculate_path(&self, from: &str, to: &str) -> Vec<String> {
        let mut path = Vec::new();
        let mut current = from.to_string();
        path.push(current.clone());
        while let Some(next_block) = self.get_next_block(&current, None) {
            path.push(next_block.clone());
            if next_block == to {
                return path;
            }
            current = next_block;
        }
        Vec::new()
    }
    fn process_flow_progression(&self, start_block: &str) -> Vec<String> {
        let mut progression = Vec::new();
        let mut current = start_block.to_string();
        while let Some(next_block) = self.get_next_block(&current, None) {
            progression.push(next_block.clone());
            current = next_block;
        }
        progression
    }
}
