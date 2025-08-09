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

use serde::Deserialize;
use std::collections::HashMap;
#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    pub provider: String,
    pub capabilities: Vec<String>,
    pub speed_tier: String,
    pub cost_tier: String,
    pub max_tokens: usize,
    pub parallel_limit: usize,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}
#[derive(Debug, Clone, Deserialize)]
pub struct PromptTemplate {
    pub system_message: String,
    pub user_template: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ProcessingPolicy {
    pub name: String,
    pub priority: u8,
    pub conditions: HashMap<String, serde_yaml::Value>,
    pub strategy: ProcessingStrategy,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ProcessingStrategy {
    #[serde(rename = "type")]
    pub strategy_type: String,
    #[serde(default)]
    pub actions: Vec<TaskAction>,
    #[serde(default)]
    pub stages: Vec<ProcessingStage>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ProcessingStage {
    pub name: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub actions: Vec<TaskAction>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct TaskAction {
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub bundle: Vec<String>,
    pub model_capability: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub parallel_with: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}
fn default_timeout() -> u64 {
    30
}
#[derive(Debug, Clone, Deserialize)]
pub struct SqlValidationConfig {
    pub enabled: bool,
    pub use_sql_parser: bool,
    #[serde(default = "default_true")]
    pub block_dangerous_functions: bool,
    #[serde(default)]
    pub allow_multi_statements: bool,
    pub max_query_length: usize,
    #[serde(default = "default_true")]
    pub parameterized_queries_enabled: bool,
    pub statement_whitelist_mode: bool,
    #[serde(default)]
    pub allowed_operations: Vec<String>,
    #[serde(default)]
    pub allowed_functions: Vec<String>,
}
impl Default for SqlValidationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            use_sql_parser: true,
            block_dangerous_functions: true,
            allow_multi_statements: false,
            max_query_length: 10000,
            parameterized_queries_enabled: true,
            statement_whitelist_mode: true,
            allowed_operations: vec![
                "SELECT".to_string(),
                "SHOW".to_string(),
                "EXPLAIN".to_string(),
            ],
            allowed_functions: vec![
                "count".to_string(),
                "sum".to_string(),
                "avg".to_string(),
                "max".to_string(),
                "min".to_string(),
            ],
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct InputSanitisationConfig {
    pub enabled: bool,
    pub max_input_length: usize,
    pub remove_comments: bool,
    pub normalise_whitespace: bool,
    pub control_character_filtering: bool,
    pub null_byte_filtering: bool,
    #[serde(default = "default_true")]
    pub excessive_repetition_detection: bool,
    #[serde(default = "default_repetition_count")]
    pub max_repetition_count: usize,
    pub max_token_length: usize,
}
impl Default for InputSanitisationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_input_length: 10000,
            remove_comments: true,
            normalise_whitespace: true,
            control_character_filtering: true,
            null_byte_filtering: true,
            excessive_repetition_detection: true,
            max_repetition_count: 20,
            max_token_length: 100,
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct PathSecurityConfig {
    pub canonicalization_first: bool,
    pub base_directory_validation: bool,
    pub file_extension_validation: bool,
    pub file_size_validation: bool,
    pub max_config_file_size_mb: u64,
    #[serde(default = "default_max_path_length")]
    pub max_path_length: usize,
    pub allowed_extensions: Vec<String>,
    pub dangerous_path_patterns: Vec<String>,
}
impl Default for PathSecurityConfig {
    fn default() -> Self {
        Self {
            canonicalization_first: true,
            base_directory_validation: true,
            file_extension_validation: true,
            file_size_validation: true,
            max_config_file_size_mb: 10,
            max_path_length: 500,
            allowed_extensions: vec![
                "json".to_string(),
                "yml".to_string(),
                "yaml".to_string(),
                "toml".to_string(),
            ],
            dangerous_path_patterns: vec![
                "../".to_string(),
                "..\\".to_string(),
                "/etc/".to_string(),
                "/proc/".to_string(),
                "/sys/".to_string(),
                "~".to_string(),
                "$HOME".to_string(),
            ],
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct StructuralValidationConfig {
    pub use_ast_validation: bool,
    pub max_expression_depth: usize,
    pub max_json_depth: usize,
}
impl Default for StructuralValidationConfig {
    fn default() -> Self {
        Self {
            use_ast_validation: true,
            max_expression_depth: 10,
            max_json_depth: 10,
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct AuditLoggingConfig {
    pub security_events_enabled: bool,
    pub log_failed_validations: bool,
    pub log_path_traversal_attempts: bool,
    #[serde(rename = "log_security_attempts")]
    pub log_injection_attempts: bool,
    pub log_config_changes: bool,
    #[serde(default = "default_true")]
    pub redact_sensitive_info: bool,
}
impl Default for AuditLoggingConfig {
    fn default() -> Self {
        Self {
            security_events_enabled: true,
            log_failed_validations: true,
            log_path_traversal_attempts: true,
            log_injection_attempts: true,
            log_config_changes: true,
            redact_sensitive_info: true,
        }
    }
}
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SecurityConfig {
    #[serde(default)]
    pub sql_validation: SqlValidationConfig,
    #[serde(default)]
    pub input_sanitisation: InputSanitisationConfig,
    #[serde(default)]
    pub path_security: PathSecurityConfig,
    #[serde(default)]
    pub structural_validation: StructuralValidationConfig,
    #[serde(default)]
    pub audit_logging: AuditLoggingConfig,
    #[serde(default)]
    pub blocked_operations: Vec<String>,
    #[serde(default)]
    pub dangerous_functions: Vec<String>,
    #[serde(default)]
    pub injection_patterns: HashMap<String, Vec<String>>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct NLUConfig {
    pub models: Vec<ModelConfig>,
    pub selection_strategy: HashMap<String, serde_yaml::Value>,
    pub prompts: HashMap<String, PromptTemplate>,
    pub policies: Vec<ProcessingPolicy>,
    pub global_settings: HashMap<String, serde_yaml::Value>,
    pub tasks: HashMap<String, HashMap<String, serde_yaml::Value>>,
    #[serde(default)]
    pub security: SecurityConfig,
}
fn default_true() -> bool {
    true
}
fn default_repetition_count() -> usize {
    20
}
fn default_max_path_length() -> usize {
    500
}
fn default_temperature() -> f32 {
    0.3
}
