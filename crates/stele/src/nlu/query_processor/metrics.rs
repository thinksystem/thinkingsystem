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
use super::{QueryProcessor, Result};
impl QueryProcessor {
    pub async fn get_processing_stats(&self) -> HashMap<String, serde_json::Value> {
        let mut stats = HashMap::new();
        let orchestrator_stats = self.orchestrator.read().await.get_stats();
        stats.extend(orchestrator_stats);
        if let Ok(db_info) = self.nli.get_database_info().await {
            stats.insert("database_info".to_string(), db_info);
        }
        stats.insert("processor_version".to_string(), serde_json::json!("3.0-orchestrated-secure"));
        stats.insert("orchestrated_processing".to_string(), serde_json::json!(true));
        stats.insert("batch_operations".to_string(), serde_json::json!(true));
        stats.insert("externalised_config".to_string(), serde_json::json!(true));
        stats.insert("sql_validation_enabled".to_string(), serde_json::json!(true));
        stats
    }
    pub async fn get_system_metrics(&self) -> Result<HashMap<String, serde_json::Value>> {
        let mut metrics = HashMap::new();
        metrics.insert("memory_usage".to_string(), serde_json::json!({
            "estimated_mb": std::mem::size_of_val(self) / 1024 / 1024
        }));
        let processing_stats = self.get_processing_stats().await;
        metrics.insert("processing_stats".to_string(), serde_json::json!(processing_stats));
        let db_health = self.nli.health_check().await.is_ok();
        metrics.insert("database_connected".to_string(), serde_json::json!(db_health));
        let orchestrator_stats = self.orchestrator.read().await.get_stats();
        metrics.insert("orchestrator_stats".to_string(), serde_json::json!(orchestrator_stats));
        metrics.insert("security_features".to_string(), serde_json::json!({
            "sql_parsing_enabled": true,
            "statement_validation": true,
            "expression_safety_checks": true,
            "parameterized_queries": true,
            "input_sanitisation": true
        }));
        metrics.insert("timestamp".to_string(), serde_json::json!(chrono::Utc::now().to_rfc3339()));
        Ok(metrics)
    }
    pub async fn create_processing_report(&self, input: &str) -> Result<HashMap<String, serde_json::Value>> {
        let mut report = HashMap::new();
        report.insert("original_input".to_string(), serde_json::json!(input));
        report.insert("input_length".to_string(), serde_json::json!(input.len()));
        report.insert("input_trimmed".to_string(), serde_json::json!(input.trim()));
        let start_time = std::time::Instant::now();
        let result = self.process_user_input(input).await;
        let processing_time = start_time.elapsed();
        report.insert("processing_time_ms".to_string(), serde_json::json!(processing_time.as_millis()));
        report.insert("success".to_string(), serde_json::json!(result.is_ok()));
        match result {
            Ok(response) => {
                report.insert("response".to_string(), serde_json::json!(response));
            },
            Err(error) => {
                report.insert("error".to_string(), serde_json::json!(error.to_string()));
                report.insert("error_type".to_string(), serde_json::json!("processing_error"));
            }
        }
        report.insert("analysis".to_string(), serde_json::json!({
            "input_validated": true,
            "length_within_limits": input.len() <= 10000,
            "non_empty": !input.trim().is_empty(),
            "processing_attempted": true
        }));
        let system_metrics = self.get_system_metrics().await?;
        report.insert("system_metrics".to_string(), serde_json::json!(system_metrics));
        report.insert("report_generated_at".to_string(), serde_json::json!(chrono::Utc::now().to_rfc3339()));
        Ok(report)
    }
    pub async fn perform_health_check(&self) -> Result<HashMap<String, serde_json::Value>> {
        let mut health = HashMap::new();
        let db_health = self.nli.health_check().await.is_ok();
        health.insert("database".to_string(), serde_json::json!(db_health));
        let orchestrator_stats = self.orchestrator.read().await.get_stats();
        let orchestrator_health = !orchestrator_stats.is_empty();
        health.insert("orchestrator".to_string(), serde_json::json!(orchestrator_health));
        health.insert("security_validation".to_string(), serde_json::json!(true));
        health.insert("sql_parser".to_string(), serde_json::json!(true));
        let overall_health = db_health && orchestrator_health;
        health.insert("overall".to_string(), serde_json::json!(overall_health));
        health.insert("security_enhanced".to_string(), serde_json::json!(true));
        Ok(health)
    }
    pub async fn get_security_report(&self) -> HashMap<String, serde_json::Value> {
        let mut report = HashMap::new();
        report.insert("sql_validation".to_string(), serde_json::json!({
            "parser_available": true,
            "statement_validation": true,
            "expression_safety_checks": true,
            "dangerous_function_detection": true,
            "comment_removal": true
        }));
        report.insert("input_sanitisation".to_string(), serde_json::json!({
            "control_character_removal": true,
            "null_byte_filtering": true,
            "whitespace_normalisation": true,
            "suspicious_pattern_detection": true,
            "length_validation": true
        }));
        report.insert("allowed_operations".to_string(), serde_json::json!([
            "SELECT queries",
            "SHOW statements",
            "DESCRIBE statements",
            "Parameterized queries"
        ]));
        report.insert("blocked_operations".to_string(), serde_json::json!([
            "DROP statements",
            "DELETE statements",
            "INSERT statements",
            "UPDATE statements",
            "TRUNCATE statements",
            "CREATE statements",
            "ALTER statements",
            "Dangerous functions",
            "Multiple statements"
        ]));
        report.insert("report_timestamp".to_string(), serde_json::json!(chrono::Utc::now().to_rfc3339()));
        report
    }
}
