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

use super::function::DynamicFunction;
use crate::blocks::rules::BlockError;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
pub struct ImportExportManager;
impl ImportExportManager {
    pub fn export_function(function: &DynamicFunction, name: &str) -> Result<Value, BlockError> {
        let export_data = serde_json::json!({
            "name": name,
            "version": function.version,
            "created_at": function.created_at,
            "metadata": function.metadata,
            "dependencies": function.dependencies,
            "source_path": function.source_path,
            "source_code": function.source_code
        });
        Ok(export_data)
    }
    pub fn export_function_with_metrics(
        function: &DynamicFunction,
        name: &str,
    ) -> Result<Value, BlockError> {
        let mut export_data = Self::export_function(function, name)?;
        if let Some(metrics) = function.get_performance_snapshot() {
            export_data["performance_metrics"] = serde_json::json!({
                "total_calls": metrics.total_calls,
                "error_count": metrics.error_count,
                "success_rate": metrics.success_rate,
                "avg_execution_time_ms": metrics.avg_execution_time.as_millis(),
                "peak_memory_usage": metrics.peak_memory_usage,
                "last_executed": metrics.last_executed
            });
        }
        Ok(export_data)
    }
    pub fn import_function_metadata(export_data: &Value) -> Result<FunctionImportData, BlockError> {
        let name = export_data["name"].as_str().ok_or_else(|| {
            BlockError::ProcessingError("Missing function name in export data".into())
        })?;
        let version = export_data["version"]
            .as_str()
            .ok_or_else(|| BlockError::ProcessingError("Missing version in export data".into()))?;
        let source_code = export_data["source_code"].as_str().ok_or_else(|| {
            BlockError::ProcessingError("Missing source_code in export data".into())
        })?;
        let created_at = export_data["created_at"]
            .as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        let metadata: HashMap<String, Value> = export_data["metadata"]
            .as_object()
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        let dependencies: Vec<String> = export_data["dependencies"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let source_path = export_data["source_path"].as_str().map(String::from);
        Ok(FunctionImportData {
            name: name.to_string(),
            version: version.to_string(),
            created_at,
            metadata,
            dependencies,
            source_path,
            source_code: source_code.to_string(),
        })
    }
    pub fn export_multiple_functions(
        functions: &HashMap<String, DynamicFunction>,
    ) -> Result<Value, BlockError> {
        let mut exported_functions = Vec::new();
        for (name, function) in functions {
            let export_data = Self::export_function(function, name)?;
            exported_functions.push(export_data);
        }
        Ok(serde_json::json!({
            "version": "1.0",
            "exported_at": Utc::now(),
            "functions": exported_functions
        }))
    }
    pub fn import_multiple_functions(
        export_data: &Value,
    ) -> Result<Vec<FunctionImportData>, BlockError> {
        let functions_array = export_data["functions"].as_array().ok_or_else(|| {
            BlockError::ProcessingError("Invalid export format: missing functions array".into())
        })?;
        let mut imported_functions = Vec::new();
        for function_data in functions_array {
            let import_data = Self::import_function_metadata(function_data)?;
            imported_functions.push(import_data);
        }
        Ok(imported_functions)
    }
    pub fn validate_export_data(export_data: &Value) -> Result<(), BlockError> {
        let required_fields = ["name", "version"];
        for field in &required_fields {
            if export_data.get(field).and_then(|v| v.as_str()).is_none() {
                return Err(BlockError::ProcessingError(format!(
                    "Missing required field: {field}"
                )));
            }
        }
        if let Some(deps) = export_data.get("dependencies") {
            if !deps.is_array() {
                return Err(BlockError::ProcessingError(
                    "Dependencies must be an array".into(),
                ));
            }
        }
        if let Some(metadata) = export_data.get("metadata") {
            if !metadata.is_object() {
                return Err(BlockError::ProcessingError(
                    "Metadata must be an object".into(),
                ));
            }
        }
        Ok(())
    }
    pub fn create_backup_export(
        functions: &HashMap<String, Vec<DynamicFunction>>,
    ) -> Result<Value, BlockError> {
        let mut backup_data = HashMap::new();
        for (name, versions) in functions {
            let mut version_exports = Vec::new();
            for function in versions {
                let export_data = Self::export_function_with_metrics(function, name)?;
                version_exports.push(export_data);
            }
            backup_data.insert(name.clone(), version_exports);
        }
        Ok(serde_json::json!({
            "backup_version": "1.0",
            "created_at": Utc::now(),
            "total_functions": functions.len(),
            "total_versions": functions.values().map(|v| v.len()).sum::<usize>(),
            "functions": backup_data
        }))
    }
}
#[derive(Debug, Clone)]
pub struct FunctionImportData {
    pub name: String,
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub metadata: HashMap<String, Value>,
    pub dependencies: Vec<String>,
    pub source_path: Option<String>,
    pub source_code: String,
}
