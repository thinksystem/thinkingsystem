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

use super::{
    assembly::AssemblyGenerator, dependency::DependencyManager, function::DynamicFunction,
    hot_reload::HotReloadManager, import_export::ImportExportManager, metrics::PerformanceMetrics,
};
use crate::blocks::rules::BlockError;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

type VersionHistory = Arc<RwLock<HashMap<String, HashMap<String, DateTime<Utc>>>>>;

pub struct DynamicExecutor {
    functions: Arc<RwLock<HashMap<String, Vec<DynamicFunction>>>>,
    function_compositions: Arc<RwLock<HashMap<String, Vec<String>>>>,
    version_history: VersionHistory,
    hot_reload_manager: Arc<RwLock<HotReloadManager>>,
    assembly_generator: AssemblyGenerator,
}
impl DynamicExecutor {
    pub fn new() -> Result<Self, BlockError> {
        let hot_reload_manager = HotReloadManager::new()?;
        let assembly_generator = AssemblyGenerator::new()?;
        Ok(Self {
            functions: Arc::new(RwLock::new(HashMap::new())),
            function_compositions: Arc::new(RwLock::new(HashMap::new())),
            version_history: Arc::new(RwLock::new(HashMap::new())),
            hot_reload_manager: Arc::new(RwLock::new(hot_reload_manager)),
            assembly_generator,
        })
    }
    pub fn compile_function(
        &self,
        wat_code: &str,
        exported_fn_name: &str,
    ) -> Result<DynamicFunction, BlockError> {
        let wasm_bytes = wat::parse_str(wat_code)
            .map_err(|e| BlockError::ProcessingError(format!("Invalid WAT format: {e}")))?;
        self.assembly_generator
            .compile_function(&wasm_bytes, exported_fn_name, wat_code)
    }
    pub fn register_function(&self, name: String, function: DynamicFunction) {
        let mut version_history = self.version_history.write().unwrap();
        let version_entry = version_history.entry(name.clone()).or_default();
        version_entry.insert(function.version.clone(), function.created_at);
        let mut functions = self.functions.write().unwrap();
        let functions_entry = functions.entry(name).or_default();
        functions_entry.push(function);
    }
    pub fn get_function(&self, name: &str, version: Option<&str>) -> Option<DynamicFunction> {
        let functions = self.functions.read().unwrap();
        let function_list = functions.get(name)?;
        match version {
            Some(v) => function_list.iter().find(|f| f.version == v).cloned(),
            None => function_list.last().cloned(),
        }
    }
    pub fn list_functions(&self) -> Vec<String> {
        let functions = self.functions.read().unwrap();
        functions.keys().cloned().collect()
    }
    pub fn get_function_count(&self) -> usize {
        let functions = self.functions.read().unwrap();
        functions.len()
    }
    pub fn get_total_versions(&self) -> usize {
        let functions = self.functions.read().unwrap();
        functions.values().map(|versions| versions.len()).sum()
    }
    pub fn compose_functions(
        &self,
        name: String,
        function_chain: Vec<String>,
    ) -> Result<(), BlockError> {
        let functions = self.functions.read().unwrap();
        DependencyManager::validate_function_chain(&functions, &function_chain)?;
        drop(functions);
        let mut compositions = self.function_compositions.write().unwrap();
        compositions.insert(name, function_chain);
        Ok(())
    }
    pub fn execute_composition(
        &self,
        composition_name: &str,
        initial_args: &[Value],
    ) -> Result<Value, BlockError> {
        let compositions = self.function_compositions.read().unwrap();
        let chain = compositions
            .get(composition_name)
            .ok_or_else(|| BlockError::ProcessingError("Composition not found".into()))?
            .clone();
        drop(compositions);
        let mut current_value = if initial_args.len() == 1 {
            initial_args[0].clone()
        } else {
            Value::Array(initial_args.to_vec())
        };
        for func_name in chain {
            if let Some(func) = self.get_function(&func_name, None) {
                let args = if current_value.is_array() {
                    current_value.as_array().unwrap().clone()
                } else {
                    vec![current_value]
                };
                current_value = func.execute(&args)?;
            } else {
                return Err(BlockError::ProcessingError(format!(
                    "Function {func_name} not found in composition"
                )));
            }
        }
        Ok(current_value)
    }
    pub fn list_compositions(&self) -> Vec<String> {
        let compositions = self.function_compositions.read().unwrap();
        compositions.keys().cloned().collect()
    }
    pub fn process_pending_events(&self) {
        let mut hot_reload_manager = self.hot_reload_manager.write().unwrap();
        let events = hot_reload_manager.get_pending_events();
        drop(hot_reload_manager);
        for event in events {
            if HotReloadManager::should_reload_file(&event) {
                for path in event.paths {
                    if let Some(function_name) =
                        HotReloadManager::extract_function_name_from_path(&path)
                    {
                        if let Ok(new_code) = std::fs::read_to_string(&path) {
                            println!("Reloading function: {function_name} from {path:?}");
                            let _ = self.reload_function_from_file(
                                &function_name,
                                &new_code,
                                Some(path.to_string_lossy().to_string()),
                            );
                        }
                    }
                }
            } else if HotReloadManager::is_new_file(&event) {
                println!("New function file detected: {:?}", event.paths);
            }
        }
    }
    pub fn hot_reload_function(
        &self,
        name: &str,
        new_code: &str,
        source_path: Option<String>,
    ) -> Result<(), BlockError> {
        let mut new_fn = self.compile_function(new_code, "execute")?;
        new_fn.source_path = source_path.clone();
        if let Some(ref path) = source_path {
            let mut hot_reload_manager = self.hot_reload_manager.write().unwrap();
            hot_reload_manager.watch_file(path)?;
        }
        let mut functions = self.functions.write().unwrap();
        if let Some(versions) = functions.get_mut(name) {
            versions.push(new_fn);
        } else {
            functions.insert(name.to_string(), vec![new_fn]);
        }
        Ok(())
    }
    fn reload_function_from_file(
        &self,
        name: &str,
        new_code: &str,
        source_path: Option<String>,
    ) -> Result<(), BlockError> {
        self.hot_reload_function(name, new_code, source_path)
    }
    pub fn get_version_history(
        &self,
        function_name: &str,
    ) -> Option<HashMap<String, DateTime<Utc>>> {
        let version_history = self.version_history.read().unwrap();
        version_history.get(function_name).cloned()
    }
    pub fn rollback_function(&self, name: &str, target_version: &str) -> Result<(), BlockError> {
        let mut functions = self.functions.write().unwrap();
        let function_list = functions
            .get_mut(name)
            .ok_or_else(|| BlockError::ProcessingError(format!("Function {name} not found")))?;
        let target_index = function_list
            .iter()
            .position(|f| f.version == target_version)
            .ok_or_else(|| {
                BlockError::ProcessingError(format!(
                    "Version {target_version} not found for function {name}"
                ))
            })?;
        let target_function = function_list.remove(target_index);
        function_list.push(target_function);
        Ok(())
    }
    pub fn get_performance_metrics(
        &self,
        function_name: &str,
        version: Option<&str>,
    ) -> Option<PerformanceMetrics> {
        let function = self.get_function(function_name, version)?;
        function.get_performance_snapshot()
    }
    pub fn benchmark_function(
        &self,
        name: &str,
        args: &[Value],
        iterations: usize,
    ) -> Result<Duration, BlockError> {
        let function = self
            .get_function(name, None)
            .ok_or_else(|| BlockError::ProcessingError(format!("Function {name} not found")))?;
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = function.execute_safe(args);
        }
        let total_time = start.elapsed();
        Ok(total_time / iterations as u32)
    }
    pub fn get_function_metadata(
        &self,
        name: &str,
        version: Option<&str>,
    ) -> Option<HashMap<String, Value>> {
        self.get_function(name, version).map(|f| f.metadata.clone())
    }
    pub fn update_function_metadata(
        &self,
        name: &str,
        version: Option<&str>,
        metadata: HashMap<String, Value>,
    ) -> Result<(), BlockError> {
        let mut functions = self.functions.write().unwrap();
        let function_list = functions
            .get_mut(name)
            .ok_or_else(|| BlockError::ProcessingError(format!("Function {name} not found")))?;
        let function = match version {
            Some(v) => function_list
                .iter_mut()
                .find(|f| f.version == v)
                .ok_or_else(|| BlockError::ProcessingError(format!("Version {v} not found")))?,
            None => function_list
                .last_mut()
                .ok_or_else(|| BlockError::ProcessingError("No versions found".into()))?,
        };
        function.metadata.extend(metadata);
        Ok(())
    }
    pub fn get_function_dependencies(&self, name: &str) -> Option<Vec<String>> {
        self.get_function(name, None)
            .map(|f| f.dependencies.clone())
    }
    pub fn add_function_dependency(
        &self,
        function_name: &str,
        dependency: String,
    ) -> Result<(), BlockError> {
        let mut functions = self.functions.write().unwrap();
        let function_list = functions.get_mut(function_name).ok_or_else(|| {
            BlockError::ProcessingError(format!("Function {function_name} not found"))
        })?;
        if let Some(current_function) = function_list.last_mut() {
            current_function.add_dependency(dependency);
        }
        Ok(())
    }
    pub fn remove_function_dependency(
        &self,
        function_name: &str,
        dependency: &str,
    ) -> Result<(), BlockError> {
        let mut functions = self.functions.write().unwrap();
        let function_list = functions.get_mut(function_name).ok_or_else(|| {
            BlockError::ProcessingError(format!("Function {function_name} not found"))
        })?;
        if let Some(current_function) = function_list.last_mut() {
            current_function.remove_dependency(dependency);
        }
        Ok(())
    }
    pub fn validate_all_dependencies(&self) -> Result<(), BlockError> {
        let functions = self.functions.read().unwrap();
        DependencyManager::validate_dependencies(&functions)
    }
    pub fn detect_circular_dependencies(&self) -> Result<(), BlockError> {
        let functions = self.functions.read().unwrap();
        DependencyManager::detect_circular_dependencies(&functions)
    }
    pub fn get_dependency_graph(&self) -> HashMap<String, Vec<String>> {
        let functions = self.functions.read().unwrap();
        DependencyManager::get_dependency_graph(&functions)
    }
    pub fn get_functions_by_dependency(&self, dependency: &str) -> Vec<String> {
        let functions = self.functions.read().unwrap();
        DependencyManager::get_functions_by_dependency(&functions, dependency)
    }
    pub fn remove_function(&self, name: &str) -> Result<(), BlockError> {
        let mut functions = self.functions.write().unwrap();
        functions
            .remove(name)
            .ok_or_else(|| BlockError::ProcessingError(format!("Function {name} not found")))?;
        drop(functions);
        let mut version_history = self.version_history.write().unwrap();
        version_history.remove(name);
        drop(version_history);
        let mut compositions = self.function_compositions.write().unwrap();
        let compositions_to_remove: Vec<String> = compositions
            .iter()
            .filter(|(_, chain)| chain.contains(&name.to_string()))
            .map(|(comp_name, _)| comp_name.clone())
            .collect();
        for comp_name in compositions_to_remove {
            compositions.remove(&comp_name);
        }
        Ok(())
    }
    pub fn clear_all_functions(&self) {
        let mut functions = self.functions.write().unwrap();
        functions.clear();
        drop(functions);
        let mut version_history = self.version_history.write().unwrap();
        version_history.clear();
        drop(version_history);
        let mut compositions = self.function_compositions.write().unwrap();
        compositions.clear();
    }
    pub fn cleanup_old_versions(&self, max_versions_per_function: usize) -> usize {
        let mut cleaned_count = 0;
        let mut functions = self.functions.write().unwrap();
        for function_list in functions.values_mut() {
            if function_list.len() > max_versions_per_function {
                let excess = function_list.len() - max_versions_per_function;
                function_list.drain(0..excess);
                cleaned_count += excess;
            }
        }
        cleaned_count
    }
    pub fn export_function(&self, name: &str, version: Option<&str>) -> Result<Value, BlockError> {
        let function = self
            .get_function(name, version)
            .ok_or_else(|| BlockError::ProcessingError(format!("Function {name} not found")))?;
        ImportExportManager::export_function(&function, name)
    }
    pub fn import_function(&self, export_data: Value) -> Result<String, BlockError> {
        let import_data = ImportExportManager::import_function_metadata(&export_data)?;
        let mut function = self.compile_function(&import_data.source_code, "execute")?;
        function.version = import_data.version;
        function.created_at = import_data.created_at;
        function.metadata = import_data.metadata;
        function.dependencies = import_data.dependencies;
        function.source_path = import_data.source_path;
        self.register_function(import_data.name.clone(), function);
        Ok(import_data.name)
    }
    pub fn get_executor_stats(&self) -> HashMap<String, Value> {
        let mut stats = HashMap::new();
        stats.insert(
            "total_functions".to_string(),
            Value::Number(self.get_function_count().into()),
        );
        stats.insert(
            "total_versions".to_string(),
            Value::Number(self.get_total_versions().into()),
        );
        let compositions = self.function_compositions.read().unwrap();
        stats.insert(
            "total_compositions".to_string(),
            Value::Number(compositions.len().into()),
        );
        stats
    }
    pub fn get_overall_error_statistics(&self) -> (u64, u64, f64) {
        let functions = self.functions.read().unwrap();
        let mut total_calls = 0;
        let mut total_errors = 0;
        for function_list in functions.values() {
            for function in function_list {
                if let Some(metrics) = function.get_performance_snapshot() {
                    total_calls += metrics.total_calls;
                    total_errors += metrics.error_count;
                }
            }
        }
        let success_rate = if total_calls > 0 {
            (total_calls - total_errors) as f64 / total_calls as f64
        } else {
            1.0
        };
        (total_calls, total_errors, success_rate)
    }
    pub fn get_functions_with_failures(&self) -> Vec<(String, String, u64)> {
        let functions = self.functions.read().unwrap();
        let mut failed_functions = Vec::new();
        for (name, versions) in functions.iter() {
            for function in versions {
                if function.has_failures() {
                    failed_functions.push((
                        name.clone(),
                        function.version.clone(),
                        function.get_error_count(),
                    ));
                }
            }
        }
        failed_functions
    }
    pub fn get_function_performance_report(&self) -> HashMap<String, Value> {
        let functions = self.functions.read().unwrap();
        let mut report = HashMap::new();
        for (name, versions) in functions.iter() {
            let version_reports: Vec<Value> = versions
                .iter()
                .map(|f| {
                    let metrics = f.get_performance_snapshot().unwrap_or_default();
                    serde_json::json!({
                        "version": f.version,
                        "total_calls": metrics.total_calls,
                        "error_count": metrics.error_count,
                        "success_rate": metrics.success_rate,
                        "avg_execution_time_ms": metrics.avg_execution_time.as_millis(),
                        "peak_memory_usage_bytes": metrics.peak_memory_usage,
                    })
                })
                .collect();
            report.insert(name.clone(), Value::Array(version_reports));
        }
        report
    }
    pub fn execute_function(&self, name: &str, args: &[Value]) -> Result<Value, BlockError> {
        let function = self
            .get_function(name, None)
            .ok_or_else(|| BlockError::ProcessingError(format!("Function '{name}' not found")))?;
        function.execute(args)
    }
    pub async fn execute_function_with_timeout(
        &self,
        name: &str,
        args: &[Value],
        timeout: Duration,
    ) -> Result<Value, BlockError> {
        let function = self
            .get_function(name, None)
            .ok_or_else(|| BlockError::ProcessingError(format!("Function '{name}' not found")))?;
        function.execute_with_timeout(args, timeout).await
    }
}
impl Clone for DynamicExecutor {
    fn clone(&self) -> Self {
        Self {
            functions: Arc::clone(&self.functions),
            function_compositions: Arc::clone(&self.function_compositions),
            version_history: Arc::clone(&self.version_history),
            hot_reload_manager: Arc::clone(&self.hot_reload_manager),
            assembly_generator: self.assembly_generator.clone(),
        }
    }
}
impl Default for DynamicExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create default DynamicExecutor")
    }
}
impl Drop for DynamicExecutor {
    fn drop(&mut self) {
        if let Ok(mut _manager) = self.hot_reload_manager.write() {}
    }
}
unsafe impl Send for DynamicExecutor {}
unsafe impl Sync for DynamicExecutor {}
