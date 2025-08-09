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

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use sqlparser::ast::{Expr, FunctionArg, FunctionArgExpr, FunctionArguments, ObjectName, Query, SelectItem, SetExpr, Statement, TableFactor, TableWithJoins};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use unicode_normalisation::UnicodeNormalisation;
use percent_encoding::percent_decode_str;
use once_cell::sync::Lazy;
use std::fs::File;
use super::{QueryProcessor, Result};
#[derive(Debug, Deserialize, Serialize, Clone)]
struct SecurityConfig {
    limits: LimitsConfig,
    path_security: PathSecurityConfig,
    sql_security: SqlSecurityConfig,
    input_security: InputSecurityConfig,
    encoding_security: EncodingSecurityConfig,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
struct LimitsConfig {
    input_max_length: usize,
    sql_max_length: usize,
    config_value_max_length: usize,
    log_truncate_length: usize,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
struct PathSecurityConfig {
    blocked_extensions: Vec<String>,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
struct SqlSecurityConfig {
    allowed_statement_types: Vec<String>,
    blocked_statement_types: Vec<String>,
    allowed_expression_types: Vec<String>,
    blocked_functions: Vec<String>,
    blocked_schemas: Vec<String>,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
struct InputSecurityConfig {
    dangerous_patterns: Vec<String>,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
struct EncodingSecurityConfig {
    normalise_unicode: bool,
    reject_null_bytes: bool,
    max_encoding_layers: u8,
}
static SECURITY_CONFIG: Lazy<Arc<RwLock<Option<SecurityConfig>>>> = Lazy::new(|| {
    Arc::new(RwLock::new(None))
});
static COMPILED_REGEXES: Lazy<Arc<RwLock<HashMap<String, regex::Regex>>>> = Lazy::new(|| {
    Arc::new(RwLock::new(HashMap::new()))
});
impl SecurityConfig {
    fn load() -> Result<Self> {
        let config_path = std::env::var("SECURITY_CONFIG_PATH")
            .unwrap_or_else(|_| "src/nlu/config/security.yml".to_string());
        let path = Path::new(&config_path);
        if !path.is_relative() || config_path.contains("..") {
            return Err(super::error::QueryProcessorError::security("Invalid config path: must be a relative path without '..' components"));
        }
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| super::error::QueryProcessorError::security(&format!("Failed to load security config '{config_path}': {e}")))?;
        let config: SecurityConfig = serde_yaml::from_str(&content)
            .map_err(|e| super::error::QueryProcessorError::security(&format!("Invalid security config format: {e}")))?;
        config.validate()?;
        Ok(config)
    }
    fn validate(&self) -> Result<()> {
        if self.limits.input_max_length == 0 || self.limits.input_max_length > 1_000_000 {
            return Err(super::error::QueryProcessorError::security("Invalid input_max_length in config: must be between 1 and 1,000,000"));
        }
        if self.limits.sql_max_length == 0 || self.limits.sql_max_length > 100_000 {
            return Err(super::error::QueryProcessorError::security("Invalid sql_max_length in config: must be between 1 and 100,000"));
        }
        if self.encoding_security.max_encoding_layers > 10 {
            return Err(super::error::QueryProcessorError::security("max_encoding_layers in config cannot exceed 10"));
        }
        for pattern in &self.input_security.dangerous_patterns {
            if pattern.len() > 1000 {
                return Err(super::error::QueryProcessorError::security("Regex pattern in config is too long (max 1000 chars)"));
            }
            regex::Regex::new(pattern)
                .map_err(|e| super::error::QueryProcessorError::security(&format!("Invalid regex pattern '{pattern}': {e}")))?;
        }
        Ok(())
    }
}
fn get_compiled_regex(pattern: &str) -> Result<regex::Regex> {
    let regexes = COMPILED_REGEXES.read()
        .map_err(|_| super::error::QueryProcessorError::security("Regex cache read lock poisoned"))?;
    if let Some(regex) = regexes.get(pattern) {
        return Ok(regex.clone());
    }
    drop(regexes);
    let regex = regex::Regex::new(pattern)
        .map_err(|e| super::error::QueryProcessorError::security(&format!("Invalid regex: {e}")))?;
    let mut regexes = COMPILED_REGEXES.write()
        .map_err(|_| super::error::QueryProcessorError::security("Regex cache write lock poisoned"))?;
    regexes.insert(pattern.to_string(), regex.clone());
    Ok(regex)
}
struct SqlAstVisitor<'a> {
    blocked_functions: &'a HashSet<String>,
    blocked_schemas: &'a HashSet<String>,
    allowed_expressions: &'a HashSet<String>,
}
impl<'a> SqlAstVisitor<'a> {
    fn new(blocked_functions: &'a HashSet<String>, blocked_schemas: &'a HashSet<String>, allowed_expressions: &'a HashSet<String>) -> Self {
        Self { blocked_functions, blocked_schemas, allowed_expressions }
    }
    fn get_expression_type_name(expr: &Expr) -> &'static str {
        match expr {
            Expr::BinaryOp { .. } => "BinaryOp",
            Expr::UnaryOp { .. } => "UnaryOp",
            Expr::Cast { .. } => "Cast",
            Expr::Case { .. } => "Case",
            Expr::Function(..) => "Function",
            Expr::Subquery(..) => "Subquery",
            Expr::InList { .. } => "InList",
            Expr::InSubquery { .. } => "InSubquery",
            Expr::Between { .. } => "Between",
            Expr::Identifier(..) => "Identifier",
            Expr::CompoundIdentifier(..) => "CompoundIdentifier",
            Expr::Value(..) => "Value",
            Expr::IsNull(..) => "IsNull",
            Expr::IsNotNull(..) => "IsNotNull",
            Expr::Like { .. } => "Like",
            Expr::ILike { .. } => "ILike",
            Expr::SimilarTo { .. } => "SimilarTo",
            Expr::Exists { .. } => "Exists",
            Expr::IsDistinctFrom(..) => "IsDistinctFrom",
            Expr::IsNotDistinctFrom(..) => "IsNotDistinctFrom",
            Expr::Nested(..) => "Nested",
            Expr::Extract { .. } => "Extract",
            Expr::Substring { .. } => "Substring",
            Expr::Trim { .. } => "Trim",
            Expr::Collate { .. } => "Collate",
            Expr::Tuple(..) => "Tuple",
            Expr::Interval(..) => "Interval",
            _ => "Unsupported",
        }
    }
    fn get_statement_type_name(stmt: &Statement) -> &'static str {
        match stmt {
            Statement::Query(..) => "Query",
            Statement::Insert { .. } => "Insert",
            Statement::Update { .. } => "Update",
            Statement::Delete { .. } => "Delete",
            Statement::CreateTable { .. } => "CreateTable",
            Statement::CreateIndex { .. } => "CreateIndex",
            Statement::CreateView { .. } => "CreateView",
            Statement::AlterTable { .. } => "AlterTable",
            Statement::Drop { .. } => "Drop",
            Statement::Truncate { .. } => "Truncate",
            Statement::Copy { .. } => "Copy",
            Statement::Call(..) => "Call",
            Statement::Execute { .. } => "Execute",
            Statement::Prepare { .. } => "Prepare",
            Statement::Deallocate { .. } => "Deallocate",
            Statement::ShowVariable { .. } => "ShowVariable",
            Statement::StartTransaction { .. } => "StartTransaction",
            Statement::Commit { .. } => "Commit",
            Statement::Rollback { .. } => "Rollback",
            _ => "Unsupported",
        }
    }
    fn walk_query(&self, query: &Query) -> Result<()> {
        if let Some(with) = &query.with {
            for cte in &with.cte_tables {
                self.walk_query(&cte.query)?;
            }
        }
        self.walk_set_expr(&query.body)?;
        Ok(())
    }
    fn walk_set_expr(&self, set_expr: &SetExpr) -> Result<()> {
        match set_expr {
            SetExpr::Select(select) => {
                for item in &select.projection {
                    match item {
                        SelectItem::UnnamedExpr(expr) => self.walk_expr(expr)?,
                        SelectItem::ExprWithAlias { expr, .. } => self.walk_expr(expr)?,
                        SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => {}
                    }
                }
                for table in &select.from {
                    self.walk_table_with_joins(table)?;
                }
                if let Some(selection) = &select.selection {
                    self.walk_expr(selection)?;
                }
                if let Some(having) = &select.having {
                    self.walk_expr(having)?;
                }
            }
            SetExpr::Query(query) => self.walk_query(query)?,
            SetExpr::SetOperation { left, right, .. } => {
                self.walk_set_expr(left)?;
                self.walk_set_expr(right)?;
            }
            SetExpr::Values(_) => {}
            _ => {
                return Err(super::error::QueryProcessorError::security("Only SELECT-like statements are allowed in queries and subqueries"));
            }
        }
        Ok(())
    }
    fn walk_table_with_joins(&self, t: &TableWithJoins) -> Result<()> {
        self.walk_table_factor(&t.relation)?;
        for join in &t.joins {
            self.walk_table_factor(&join.relation)?;
        }
        Ok(())
    }
    fn walk_table_factor(&self, t: &TableFactor) -> Result<()> {
        match t {
            TableFactor::Table { name, .. } => self.check_object_name(name),
            TableFactor::Derived { subquery, .. } => self.walk_query(subquery),
            TableFactor::NestedJoin { table_with_joins, .. } => self.walk_table_with_joins(table_with_joins),
            _ => Ok(()),
        }
    }
    fn walk_expr(&self, expr: &Expr) -> Result<()> {
        let expr_type = Self::get_expression_type_name(expr);
        if !self.allowed_expressions.contains(expr_type) {
            return Err(super::error::QueryProcessorError::security(&format!("Expression type '{expr_type}' is not allowed by security policy")));
        }
        match expr {
            Expr::BinaryOp { left, right, .. } => {
                self.walk_expr(left)?;
                self.walk_expr(right)?;
            }
            Expr::UnaryOp { expr, .. } => self.walk_expr(expr)?,
            Expr::Cast { expr, .. } => self.walk_expr(expr)?,
            Expr::Case { operand, conditions, else_result, .. } => {
                if let Some(op) = operand { self.walk_expr(op)?; }
                for case_when in conditions {
                    self.walk_expr(&case_when.condition)?;
                    self.walk_expr(&case_when.result)?;
                }
                if let Some(el) = else_result { self.walk_expr(el)?; }
            }
            Expr::Function(func) => {
                let name = func.name.to_string().to_lowercase();
                if self.blocked_functions.contains(&name) {
                    return Err(super::error::QueryProcessorError::security(&format!("Use of blocked function is not allowed: {name}")));
                }
                if let FunctionArguments::List(arg_list) = &func.args {
                    for arg in &arg_list.args {
                        match arg {
                            FunctionArg::Unnamed(FunctionArgExpr::Expr(expr)) => self.walk_expr(expr)?,
                            FunctionArg::Named { arg: FunctionArgExpr::Expr(expr), .. } => self.walk_expr(expr)?,
                            FunctionArg::Unnamed(FunctionArgExpr::Wildcard) => {},
                            _ => {}
                        }
                    }
                }
            }
            Expr::Subquery(query) => self.walk_query(query)?,
            Expr::InList { expr, list, .. } => {
                self.walk_expr(expr)?;
                for item in list { self.walk_expr(item)?; }
            }
            Expr::InSubquery { expr, subquery, .. } => {
                self.walk_expr(expr)?;
                self.walk_set_expr(subquery)?;
            }
            Expr::Between { expr, low, high, .. } => {
                self.walk_expr(expr)?;
                self.walk_expr(low)?;
                self.walk_expr(high)?;
            }
            Expr::Identifier(_) | Expr::CompoundIdentifier(_) | Expr::Value(_) => {
            }
            Expr::IsNull(expr) | Expr::IsNotNull(expr) => self.walk_expr(expr)?,
            Expr::Like { expr, pattern, .. } | Expr::ILike { expr, pattern, .. } |
            Expr::SimilarTo { expr, pattern, .. } => {
                self.walk_expr(expr)?;
                self.walk_expr(pattern)?;
            }
            Expr::Exists { subquery, .. } => self.walk_query(subquery)?,
            Expr::IsDistinctFrom(left, right) | Expr::IsNotDistinctFrom(left, right) => {
                self.walk_expr(left)?;
                self.walk_expr(right)?;
            }
            Expr::Nested(expr) => self.walk_expr(expr)?,
            Expr::Extract { expr, .. } => self.walk_expr(expr)?,
            Expr::Substring { expr, substring_from, substring_for, .. } => {
                self.walk_expr(expr)?;
                if let Some(from) = substring_from { self.walk_expr(from)?; }
                if let Some(for_expr) = substring_for { self.walk_expr(for_expr)?; }
            }
            Expr::Trim { expr, trim_what, .. } => {
                self.walk_expr(expr)?;
                if let Some(what) = trim_what { self.walk_expr(what)?; }
            }
            Expr::Collate { expr, .. } => self.walk_expr(expr)?,
            Expr::Tuple(exprs) => {
                for expr in exprs { self.walk_expr(expr)?; }
            }
            Expr::Interval(interval) => {
                self.walk_expr(&interval.value)?;
            }
            _ => {
                return Err(super::error::QueryProcessorError::security(&format!("Unsupported SQL expression type '{expr_type}' encountered during traversal")));
            }
        }
        Ok(())
    }
    fn check_object_name(&self, name: &ObjectName) -> Result<()> {
        if name.0.len() > 1 {
            let schema = name.0.first().unwrap().to_string().to_lowercase();
            if self.blocked_schemas.contains(&schema) {
                return Err(super::error::QueryProcessorError::security(&format!("Access to blocked schema is not allowed: {schema}")));
            }
        }
        Ok(())
    }
}
impl QueryProcessor {
    fn security_config(&self) -> Result<SecurityConfig> {
        {
            let config = SECURITY_CONFIG.read()
                .map_err(|_| super::error::QueryProcessorError::security("Config cache read lock poisoned"))?;
            if let Some(ref cached_config) = *config {
                return Ok(cached_config.clone());
            }
        }
        let new_config = SecurityConfig::load()?;
        {
            let mut config = SECURITY_CONFIG.write()
                .map_err(|_| super::error::QueryProcessorError::security("Config cache write lock poisoned"))?;
            *config = Some(new_config.clone());
        }
        Ok(new_config)
    }
    pub fn securely_open_file(&self, path: &str, base_dir: &Path) -> Result<File> {
        let canonical_path = self.validate_path_and_get_canonical(path, base_dir)?;
        File::open(&canonical_path)
            .map_err(|e| super::error::QueryProcessorError::security(&format!("Failed to open validated file '{}': {}", canonical_path.display(), e)))
    }
    pub(super) fn validate_path_and_get_canonical(&self, path: &str, base_dir: &Path) -> Result<PathBuf> {
        let config = self.security_config()?;
        let normalised = self.normalise_input(path)?;
        for pattern in &config.input_security.dangerous_patterns {
            let regex = get_compiled_regex(pattern)?;
            if regex.is_match(&normalised) {
                return Err(super::error::QueryProcessorError::security("Path contains a dangerous pattern"));
            }
        }
        let requested_path = Path::new(&normalised);
        let absolute_path = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            base_dir.join(requested_path)
        };
        let canonical_path = absolute_path.canonicalize()
            .map_err(|e| super::error::QueryProcessorError::security(&format!("Path resolution failed: {e}")))?;
        let canonical_base = base_dir.canonicalize()
            .map_err(|e| super::error::QueryProcessorError::security(&format!("Base directory is invalid: {e}")))?;
        if !canonical_path.starts_with(&canonical_base) {
            return Err(super::error::QueryProcessorError::security("Path traversal outside of allowed base directory is forbidden"));
        }
        if let Some(extension) = canonical_path.extension().and_then(|s| s.to_str()) {
            if config.path_security.blocked_extensions.contains(&extension.to_lowercase()) {
                return Err(super::error::QueryProcessorError::security(&format!("File type with extension '.{extension}' is not allowed")));
            }
        }
        Ok(canonical_path)
    }
    pub fn validate_input(&self, input: &str) -> Result<()> {
        let config = self.security_config()?;
        if input.len() > config.limits.input_max_length {
            return Err(super::error::QueryProcessorError::security(&format!("Input exceeds maximum length of {} characters", config.limits.input_max_length)));
        }
        if input.trim().is_empty() {
            return Err(super::error::QueryProcessorError::validation("Input cannot be empty or contain only whitespace"));
        }
        let normalised = self.normalise_input(input)?;
        for pattern in &config.input_security.dangerous_patterns {
            let regex = get_compiled_regex(pattern)?;
            if regex.is_match(&normalised) {
                return Err(super::error::QueryProcessorError::security("Input contains a dangerous pattern"));
            }
        }
        Ok(())
    }
    pub fn validate_sql_structure(&self, sql: &str) -> Result<()> {
        let config = self.security_config()?;
        if sql.len() > config.limits.sql_max_length {
            return Err(super::error::QueryProcessorError::security(&format!("SQL query exceeds maximum length of {} characters", config.limits.sql_max_length)));
        }
        if sql.trim().is_empty() {
            return Err(super::error::QueryProcessorError::validation("SQL query cannot be empty"));
        }
        let normalised = self.normalise_input(sql)?;
        let dialect = GenericDialect {};
        let ast = Parser::parse_sql(&dialect, &normalised)
            .map_err(|e| super::error::QueryProcessorError::security(&format!("Invalid SQL syntax: {e}")))?;
        if ast.len() > 1 {
            return Err(super::error::QueryProcessorError::security("Multiple SQL statements are not allowed in a single query"));
        }
        if ast.is_empty() {
            return Err(super::error::QueryProcessorError::validation("SQL query is empty after parsing"));
        }
        let stmt = &ast[0];
        let stmt_type = SqlAstVisitor::get_statement_type_name(stmt);
        if config.sql_security.blocked_statement_types.contains(&stmt_type.to_string()) {
            return Err(super::error::QueryProcessorError::security(&format!("{stmt_type} statements are blocked by security policy")));
        }
        if !config.sql_security.allowed_statement_types.contains(&stmt_type.to_string()) {
            return Err(super::error::QueryProcessorError::security(&format!("Statement type '{stmt_type}' is not in the allowed list")));
        }
        match stmt {
            Statement::Query(query) => {
                let blocked_functions: HashSet<String> = config.sql_security.blocked_functions
                    .iter()
                    .map(|s| s.to_lowercase())
                    .collect();
                let blocked_schemas: HashSet<String> = config.sql_security.blocked_schemas
                    .iter()
                    .map(|s| s.to_lowercase())
                    .collect();
                let allowed_expressions: HashSet<String> = config.sql_security.allowed_expression_types
                    .iter()
                    .cloned()
                    .collect();
                let visitor = SqlAstVisitor::new(&blocked_functions, &blocked_schemas, &allowed_expressions);
                visitor.walk_query(query)
            }
            _ => {
                Ok(())
            }
        }
    }
    pub fn sanitise_for_logging(&self, input: &str) -> String {
        let config = self.security_config().unwrap_or_else(|e| {
            eprintln!("Warning: Failed to load security config for logging, using default limits. Error: {}", e);
            SecurityConfig {
                limits: LimitsConfig {
                    log_truncate_length: 100,
                    input_max_length: 10000,
                    sql_max_length: 5000,
                    config_value_max_length: 1000
                },
                path_security: PathSecurityConfig { blocked_extensions: vec![] },
                sql_security: SqlSecurityConfig {
                    allowed_statement_types: vec!["Query".to_string()],
                    blocked_statement_types: vec![],
                    allowed_expression_types: vec![
                        "BinaryOp".to_string(), "UnaryOp".to_string(), "Cast".to_string(),
                        "Case".to_string(), "Function".to_string(), "Identifier".to_string(),
                        "CompoundIdentifier".to_string(), "Value".to_string()
                    ],
                    blocked_functions: vec![],
                    blocked_schemas: vec![]
                },
                input_security: InputSecurityConfig { dangerous_patterns: vec![] },
                encoding_security: EncodingSecurityConfig {
                    normalise_unicode: true,
                    reject_null_bytes: true,
                    max_encoding_layers: 2
                },
            }
        });
        input.chars()
            .map(|c| match c {
                '\n' | '\r' | '\t' => ' ',
                c if c.is_control() => '?',
                _ => c,
            })
            .take(config.limits.log_truncate_length)
            .collect()
    }
    pub fn validate_config_parameter(&self, key: &str, value: &str) -> Result<()> {
        let config = self.security_config()?;
        if key.is_empty() || key.len() > 100 {
            return Err(super::error::QueryProcessorError::security("Configuration key length is invalid"));
        }
        if !key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
            return Err(super::error::QueryProcessorError::security("Configuration key contains invalid characters"));
        }
        if value.len() > config.limits.config_value_max_length {
            return Err(super::error::QueryProcessorError::security("Configuration value exceeds maximum length"));
        }
        let normalised = self.normalise_input(value)?;
        for pattern in &config.input_security.dangerous_patterns {
            let regex = get_compiled_regex(pattern)?;
            if regex.is_match(&normalised) {
                return Err(super::error::QueryProcessorError::security("Configuration value contains a dangerous pattern"));
            }
        }
        Ok(())
    }
    fn normalise_input(&self, input: &str) -> Result<String> {
        let config = self.security_config()?;
        if config.encoding_security.reject_null_bytes && input.contains('\0') {
            return Err(super::error::QueryProcessorError::security("Input contains null bytes, which are not allowed"));
        }
        let mut result = input.to_string();
        if config.encoding_security.normalise_unicode {
            result = result.nfc().collect::<String>();
        }
        let mut decode_count = 0;
        let max_layers = config.encoding_security.max_encoding_layers;
        while decode_count < max_layers {
            let decoded = percent_decode_str(&result)
                .decode_utf8()
                .map_err(|e| super::error::QueryProcessorError::security(&format!("Invalid UTF-8 sequence in percent-encoded string: {e}")))?
                .into_owned();
            if decoded == result {
                break;
            }
            result = decoded;
            decode_count += 1;
        }
        if decode_count >= max_layers {
            return Err(super::error::QueryProcessorError::security(&format!("Input exceeds maximum of {max_layers} encoding layers")));
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    fn setup_real_config() {
        std::env::set_var("SECURITY_CONFIG_PATH", "src/nlu/config/security.yml");
    }
    #[test]
    fn test_security_config_load() {
        setup_real_config();
        let config = SecurityConfig::load();
        assert!(config.is_ok());
    }
    #[test]
    fn test_security_config_validation() {
        setup_real_config();
        let config = SecurityConfig::load().unwrap();
        assert!(config.validate().is_ok());
    }
    #[test]
    fn test_dangerous_pattern_regex() {
        setup_real_config();
        let regex = get_compiled_regex(r"javascript:").unwrap();
        assert!(regex.is_match("javascript:alert(1)"));
        assert!(!regex.is_match("normal text"));
    }
    #[test]
    fn test_path_traversal_pattern() {
        setup_real_config();
        let regex = get_compiled_regex(r"\.\.\/|\.\.\\").unwrap();
        assert!(regex.is_match("../etc/passwd"));
        assert!(regex.is_match("..\\windows\\system32"));
        assert!(!regex.is_match("normal/path"));
    }
    #[test]
    fn test_unicode_normalisation() {
        setup_real_config();
        let config = SecurityConfig::load().unwrap();
        let input = "café";
        let mut result = input.to_string();
        if config.encoding_security.normalise_unicode {
            result = result.nfc().collect::<String>();
        }
        assert!(!result.is_empty());
    }
    #[test]
    fn test_percent_encoding_decode() {
        let encoded = "hello%20world";
        let decoded = percent_decode_str(encoded)
            .decode_utf8()
            .unwrap()
            .into_owned();
        assert_eq!(decoded, "hello world");
    }
    #[test]
    fn test_real_config_values() {
        setup_real_config();
        let config = SecurityConfig::load().unwrap();
        assert_eq!(config.limits.input_max_length, 10000);
        assert_eq!(config.limits.sql_max_length, 5000);
        assert_eq!(config.limits.config_value_max_length, 1000);
        assert_eq!(config.limits.log_truncate_length, 100);
    }
    #[test]
    fn test_real_blocked_extensions() {
        setup_real_config();
        let config = SecurityConfig::load().unwrap();
        assert!(config.path_security.blocked_extensions.contains(&"exe".to_string()));
        assert!(config.path_security.blocked_extensions.contains(&"bat".to_string()));
        assert!(config.path_security.blocked_extensions.contains(&"cmd".to_string()));
        assert!(config.path_security.blocked_extensions.contains(&"sh".to_string()));
        assert!(config.path_security.blocked_extensions.contains(&"ps1".to_string()));
        assert!(config.path_security.blocked_extensions.contains(&"dll".to_string()));
        assert!(config.path_security.blocked_extensions.contains(&"so".to_string()));
    }
    #[test]
    fn test_real_blocked_functions() {
        setup_real_config();
        let config = SecurityConfig::load().unwrap();
        assert!(config.sql_security.blocked_functions.contains(&"load_file".to_string()));
        assert!(config.sql_security.blocked_functions.contains(&"into_outfile".to_string()));
        assert!(config.sql_security.blocked_functions.contains(&"copy".to_string()));
        assert!(config.sql_security.blocked_functions.contains(&"bulk_insert".to_string()));
        assert!(config.sql_security.blocked_functions.contains(&"xp_cmdshell".to_string()));
        assert!(config.sql_security.blocked_functions.contains(&"eval".to_string()));
    }
    #[test]
    fn test_real_blocked_schemas() {
        setup_real_config();
        let config = SecurityConfig::load().unwrap();
        assert!(config.sql_security.blocked_schemas.contains(&"information_schema".to_string()));
        assert!(config.sql_security.blocked_schemas.contains(&"mysql".to_string()));
        assert!(config.sql_security.blocked_schemas.contains(&"sys".to_string()));
        assert!(config.sql_security.blocked_schemas.contains(&"performance_schema".to_string()));
        assert!(config.sql_security.blocked_schemas.contains(&"pg_catalogue".to_string()));
        assert!(config.sql_security.blocked_schemas.contains(&"master".to_string()));
        assert!(config.sql_security.blocked_schemas.contains(&"msdb".to_string()));
    }
    #[test]
    fn test_real_allowed_statement_types() {
        setup_real_config();
        let config = SecurityConfig::load().unwrap();
        assert!(config.sql_security.allowed_statement_types.contains(&"Query".to_string()));
        assert_eq!(config.sql_security.allowed_statement_types.len(), 1);
    }
    #[test]
    fn test_real_dangerous_patterns() {
        setup_real_config();
        let config = SecurityConfig::load().unwrap();
        let patterns = &config.input_security.dangerous_patterns;
        assert!(patterns.iter().any(|p| p.contains("javascript:")));
        assert!(patterns.iter().any(|p| p.contains("<script")));
        assert!(patterns.iter().any(|p| p.contains("\\x00")));
        assert!(patterns.iter().any(|p| p.contains("\\.\\.")));
    }
    #[test]
    fn test_real_encoding_security() {
        setup_real_config();
        let config = SecurityConfig::load().unwrap();
        assert!(config.encoding_security.normalise_unicode);
        assert!(config.encoding_security.reject_null_bytes);
        assert_eq!(config.encoding_security.max_encoding_layers, 2);
    }
    #[test]
    fn test_regex_cache() {
        setup_real_config();
        let pattern = r"test\d+";
        let regex1 = get_compiled_regex(pattern).unwrap();
        let regex2 = get_compiled_regex(pattern).unwrap();
        assert!(regex1.is_match("test123"));
        assert!(regex2.is_match("test456"));
        assert!(!regex1.is_match("test"));
    }
    #[test]
    fn test_config_validation_logic() {
        let invalid_config = SecurityConfig {
            limits: LimitsConfig {
                input_max_length: 0,
                sql_max_length: 200000,
                config_value_max_length: 50,
                log_truncate_length: 20,
            },
            path_security: PathSecurityConfig {
                blocked_extensions: vec![],
            },
            sql_security: SqlSecurityConfig {
                allowed_statement_types: vec![],
                blocked_statement_types: vec![],
                allowed_expression_types: vec![],
                blocked_functions: vec![],
                blocked_schemas: vec![],
            },
            input_security: InputSecurityConfig {
                dangerous_patterns: vec![],
            },
            encoding_security: EncodingSecurityConfig {
                normalise_unicode: true,
                reject_null_bytes: true,
                max_encoding_layers: 15,
            },
        };
        let result = invalid_config.validate();
        assert!(result.is_err());
    }
    #[test]
    fn test_config_path_security_validation() {
        std::env::set_var("SECURITY_CONFIG_PATH", "/absolute/path/config.yml");
        let result = SecurityConfig::load();
        assert!(result.is_err());
        std::env::set_var("SECURITY_CONFIG_PATH", "../config.yml");
        let result = SecurityConfig::load();
        assert!(result.is_err());
    }
    #[test]
    fn test_invalid_regex_pattern_validation() {
        let config_with_invalid_regex = SecurityConfig {
            limits: LimitsConfig {
                input_max_length: 1000,
                sql_max_length: 5000,
                config_value_max_length: 500,
                log_truncate_length: 100,
            },
            path_security: PathSecurityConfig {
                blocked_extensions: vec![],
            },
            sql_security: SqlSecurityConfig {
                allowed_statement_types: vec!["Query".to_string()],
                blocked_statement_types: vec![],
                allowed_expression_types: vec![],
                blocked_functions: vec![],
                blocked_schemas: vec![],
            },
            input_security: InputSecurityConfig {
                dangerous_patterns: vec!["[invalid regex(".to_string()],
            },
            encoding_security: EncodingSecurityConfig {
                normalise_unicode: true,
                reject_null_bytes: true,
                max_encoding_layers: 2,
            },
        };
        let result = config_with_invalid_regex.validate();
        assert!(result.is_err());
    }
}
