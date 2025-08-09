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

use super::metrics::PerformanceMetrics;
use crate::blocks::rules::BlockError;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

type CompiledFunction = Arc<dyn Fn(&[Value]) -> Result<Value, BlockError> + Send + Sync>;

#[derive(Clone)]
pub struct DynamicFunction {
    pub compiled_fn: CompiledFunction,
    pub metadata: HashMap<String, Value>,
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub performance_metrics: Arc<RwLock<PerformanceMetrics>>,
    pub dependencies: Vec<String>,
    pub source_path: Option<String>,
    pub source_code: String,
}
impl DynamicFunction {
    pub fn new(compiled_fn: CompiledFunction, version: String, source_code: String) -> Self {
        Self {
            compiled_fn,
            metadata: HashMap::new(),
            version,
            created_at: Utc::now(),
            performance_metrics: Arc::new(RwLock::new(PerformanceMetrics::default())),
            dependencies: Vec::new(),
            source_path: None,
            source_code,
        }
    }
    pub fn execute(&self, args: &[Value]) -> Result<Value, BlockError> {
        let start = Instant::now();
        let result = (self.compiled_fn)(args);
        let duration = start.elapsed();
        if let Ok(mut metrics) = self.performance_metrics.write() {
            let success = result.is_ok();
            metrics.record_execution(duration, success);
        }
        result
    }
    pub async fn execute_with_timeout(
        &self,
        args: &[Value],
        timeout: Duration,
    ) -> Result<Value, BlockError> {
        let start = Instant::now();
        let compiled_fn_clone = self.compiled_fn.clone();
        let args_clone = args.to_vec();
        let blocking_task = tokio::task::spawn_blocking(move || (compiled_fn_clone)(&args_clone));
        let result = match tokio::time::timeout(timeout, blocking_task).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Err(BlockError::ProcessingError("Task panicked".to_string())),
            Err(_) => Err(BlockError::ProcessingError(
                "Function execution timeout".to_string(),
            )),
        };
        let duration = start.elapsed();
        if let Ok(mut metrics) = self.performance_metrics.write() {
            let success = result.is_ok();
            metrics.record_execution(duration, success);
        }
        result
    }
    pub fn execute_safe(&self, args: &[Value]) -> Value {
        match self.execute(args) {
            Ok(value) => value,
            Err(_) => Value::Number(serde_json::Number::from(0)),
        }
    }
    pub async fn execute_safe_with_timeout(&self, args: &[Value], timeout: Duration) -> Value {
        match self.execute_with_timeout(args, timeout).await {
            Ok(value) => value,
            Err(_) => Value::Number(serde_json::Number::from(0)),
        }
    }
    pub fn get_metadata(&self) -> &HashMap<String, Value> {
        &self.metadata
    }
    pub fn set_metadata(&mut self, key: String, value: Value) {
        self.metadata.insert(key, value);
    }
    pub fn get_version(&self) -> &str {
        &self.version
    }
    pub fn get_creation_time(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn get_source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }
    pub fn set_source_path(&mut self, path: Option<String>) {
        self.source_path = path;
    }
    pub fn get_dependencies(&self) -> &[String] {
        &self.dependencies
    }
    pub fn add_dependency(&mut self, dependency: String) {
        if !self.dependencies.contains(&dependency) {
            self.dependencies.push(dependency);
        }
    }
    pub fn remove_dependency(&mut self, dependency: &str) {
        self.dependencies.retain(|d| d != dependency);
    }
    pub fn has_dependency(&self, dependency: &str) -> bool {
        self.dependencies.contains(&dependency.to_string())
    }
    pub fn is_dependency_of(&self, other_function: &str) -> bool {
        self.dependencies.contains(&other_function.to_string())
    }
    pub fn get_performance_snapshot(&self) -> Option<PerformanceMetrics> {
        self.performance_metrics
            .read()
            .ok()
            .map(|metrics| metrics.clone())
    }
    pub fn reset_performance_metrics(&self) {
        if let Ok(mut metrics) = self.performance_metrics.write() {
            metrics.reset();
        }
    }
    pub fn get_call_count(&self) -> u64 {
        self.performance_metrics
            .read()
            .map(|metrics| metrics.total_calls)
            .unwrap_or(0)
    }
    pub fn get_error_count(&self) -> u64 {
        self.performance_metrics
            .read()
            .map(|metrics| metrics.error_count)
            .unwrap_or(0)
    }
    pub fn get_success_rate(&self) -> f64 {
        self.performance_metrics
            .read()
            .map(|metrics| metrics.success_rate)
            .unwrap_or(0.0)
    }
    pub fn get_average_execution_time(&self) -> Duration {
        self.performance_metrics
            .read()
            .map(|metrics| metrics.avg_execution_time)
            .unwrap_or_default()
    }
    pub fn get_last_executed(&self) -> Option<DateTime<Utc>> {
        self.performance_metrics.read().ok().and_then(|metrics| {
            if metrics.total_calls > 0 {
                Some(metrics.last_executed)
            } else {
                None
            }
        })
    }
    pub fn update_peak_memory_usage(&self, memory_usage: usize) {
        if let Ok(mut metrics) = self.performance_metrics.write() {
            metrics.record_memory_usage(memory_usage);
        }
    }
    pub fn get_peak_memory_usage(&self) -> usize {
        self.performance_metrics
            .read()
            .map(|metrics| metrics.peak_memory_usage)
            .unwrap_or(0)
    }
    pub fn clone_with_new_version(&self, new_version: String) -> Self {
        Self {
            compiled_fn: self.compiled_fn.clone(),
            metadata: self.metadata.clone(),
            version: new_version,
            created_at: Utc::now(),
            performance_metrics: Arc::new(RwLock::new(PerformanceMetrics::default())),
            dependencies: self.dependencies.clone(),
            source_path: self.source_path.clone(),
            source_code: self.source_code.clone(),
        }
    }
    pub fn get_error_statistics(&self) -> Option<(u64, u64, f64)> {
        self.performance_metrics.read().ok().map(|metrics| {
            (
                metrics.total_calls,
                metrics.error_count,
                metrics.success_rate,
            )
        })
    }
    pub fn has_failures(&self) -> bool {
        self.get_error_count() > 0
    }
    pub fn get_failure_rate(&self) -> f64 {
        1.0 - self.get_success_rate()
    }
}
