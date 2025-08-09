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

use crate::database::query_builder::{RelateQuery, SelectQuery};
use crate::database::surreal_token::SurrealTokenParser;
use crate::database::tokens::{IdiomPart, IdiomToken};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, warn};
#[derive(Debug)]
pub enum QueryError {
    TooManyConditions,
    UnsupportedQueryType,
    InvalidOperator,
    InvalidFieldType,
    InvalidTable,
    TooManyAttempts,
    InvalidQuery(String),
}
#[derive(Debug)]
pub enum ValidatedQuery {
    Select(Box<SelectQuery>),
    Relate(Box<RelateQuery>),
}
pub struct QueryValidator {
    rules: QueryRules,
}
#[derive(Deserialize, Serialize, Clone)]
pub struct QueryRules {
    pub allowed_tables: Vec<String>,
    pub max_conditions: u32,
    pub allowed_operators: Vec<String>,
    pub field_types: HashMap<String, FieldType>,
    pub relationships: Vec<RelationRule>,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum FieldType {
    String,
    Number,
    Boolean,
    DateTime,
}
#[derive(Deserialize, Serialize, Clone)]
pub struct RelationRule {
    pub from_table: String,
    pub edge_table: String,
    pub to_table: String,
    pub allowed_fields: Vec<String>,
}
#[derive(Deserialize)]
pub struct LlmQueryBatch {
    pub queries: Vec<QueryIntent>,
    pub relationships: Vec<RelationIntent>,
}
#[derive(Deserialize)]
pub struct QueryIntent {
    query_type: String,
    target: String,
    conditions: Vec<Condition>,
    expected_result: String,
}
#[derive(Debug, Deserialize, Clone)]
pub struct Condition {
    pub field: String,
    pub operator: String,
    pub value: serde_json::Value,
}
#[derive(Deserialize)]
pub struct RelationIntent {
    from: String,
    edge: String,
    to: String,
    content: Option<serde_json::Value>,
}
pub struct QueryNegotiator {
    rules: QueryRules,
    validator: QueryValidator,
}
impl QueryNegotiator {
    pub fn new(rules: QueryRules) -> Self {
        Self {
            rules: rules.clone(),
            validator: QueryValidator::new(rules),
        }
    }
    pub fn negotiate_with_feedback<F>(
        &self,
        natural_query: &str,
        feedback_handler: F,
    ) -> Result<Vec<ValidatedQuery>, QueryError>
    where
        F: Fn(&ValidationFeedback) -> Option<String>,
    {
        let mut current_query = natural_query.to_string();
        let mut attempts = 0;
        const MAX_ATTEMPTS: u8 = 3;
        while attempts < MAX_ATTEMPTS {
            match self.negotiate_batch(&current_query) {
                Ok(queries) => return Ok(queries),
                Err(e) => {
                    let feedback = ValidationFeedback {
                        valid: false,
                        errors: vec![e.to_string()],
                        suggested_fixes: self.generate_fixes(&e),
                    };
                    if let Some(revised_query) = feedback_handler(&feedback) {
                        current_query = revised_query;
                        attempts += 1;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        Err(QueryError::TooManyAttempts)
    }
    fn generate_fixes(&self, error: &QueryError) -> Vec<String> {
        match error {
            QueryError::InvalidTable => self
                .rules
                .allowed_tables
                .iter()
                .map(|t| format!("Valid table option: {t}"))
                .collect(),
            QueryError::InvalidOperator => self
                .rules
                .allowed_operators
                .iter()
                .map(|op| format!("Valid operator: {op}"))
                .collect(),
            QueryError::InvalidFieldType => self
                .rules
                .field_types
                .iter()
                .map(|(field, type_)| format!("Field '{field}' expects type {type_:?}"))
                .collect(),
            QueryError::TooManyConditions => {
                vec![format!(
                    "Maximum allowed conditions: {}",
                    self.rules.max_conditions
                )]
            }
            _ => vec!["Please revise your query syntax".to_string()],
        }
    }
    fn negotiate_batch(&self, _natural_query: &str) -> Result<Vec<ValidatedQuery>, QueryError> {
        let batch = self.get_llm_query_batch(_natural_query)?;
        let mut validated_queries = Vec::new();
        let mut feedback = Vec::new();

        for relation in &batch.relationships {
            debug!(
                "Processing relationship: {} -[{}]-> {} with content: {:?}",
                relation.from, relation.edge, relation.to, relation.content
            );
        }

        for intent in batch.queries {
            match self.validate_query_intent(&intent) {
                Ok(query) => validated_queries.push(query),
                Err(e) => {
                    let detailed_feedback = self.validator.generate_feedback(&intent);
                    feedback.push(format!(
                        "Query '{}' invalid: {} - Errors: {}",
                        intent.expected_result,
                        e,
                        detailed_feedback.errors.join(", ")
                    ));
                }
            }
        }

        if !feedback.is_empty() {
            if let Ok(revision_request) = self.request_query_revision(&feedback) {
                warn!(
                    "Query validation failed, revision suggested: {}",
                    revision_request
                );
            }
        }

        Ok(validated_queries)
    }
    pub fn validate_token_structure(&self, input: &str) -> Result<(), QueryError> {
        if input.is_empty() {
            return Err(QueryError::InvalidTable);
        }
        let valid_chars = input.chars().all(|c| {
            c.is_alphanumeric()
                || matches!(c, '@' | '.' | '[' | ']' | '{' | '}' | '-' | '>' | '_' | '?')
        });
        if !valid_chars {
            return Err(QueryError::InvalidOperator);
        }
        Ok(())
    }
    pub fn validate_parsed_token(&self, token: &IdiomToken) -> Result<(), QueryError> {
        for part in &token.parts {
            match part {
                IdiomPart::Field(field) => {
                    if !self.rules.field_types.contains_key(field) && field != "*" && field != "$" {
                        return Err(QueryError::InvalidFieldType);
                    }
                }
                IdiomPart::Graph(relation) => {
                    if !self.rules.allowed_tables.contains(relation) {
                        return Err(QueryError::InvalidTable);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn validate_query_intent(&self, intent: &QueryIntent) -> Result<ValidatedQuery, QueryError> {
        self.validator.validate_table(&intent.target)?;
        self.validator.validate_conditions(&intent.conditions)?;
        match intent.query_type.as_str() {
            "select" => {
                let query = SelectQuery::new().from(vec![intent.target.clone()]);
                Ok(ValidatedQuery::Select(Box::new(query)))
            }
            "relate" => Ok(ValidatedQuery::Relate(Box::new(
                self.build_relate_query(intent)?,
            ))),
            _ => Err(QueryError::UnsupportedQueryType),
        }
    }
    fn build_relate_query(&self, intent: &QueryIntent) -> Result<RelateQuery, QueryError> {
        let default_edge = format!("{}_edge", intent.target);
        Ok(RelateQuery::new(
            intent.target.clone(),
            default_edge,
            intent.target.clone(),
        ))
    }
    fn get_llm_query_batch(&self, _natural_query: &str) -> Result<LlmQueryBatch, QueryError> {
        unimplemented!()
    }
    fn request_query_revision(&self, feedback: &[String]) -> Result<String, QueryError> {
        if feedback.is_empty() {
            return Err(QueryError::InvalidQuery(
                "No feedback provided for revision".to_string(),
            ));
        }

        let feedback_summary = feedback.join("; ");
        Ok(format!(
            "Please revise your query addressing these issues: {feedback_summary}"
        ))
    }
}
#[derive(Serialize)]
pub struct ValidationFeedback {
    valid: bool,
    errors: Vec<String>,
    suggested_fixes: Vec<String>,
}
impl QueryValidator {
    pub fn new(rules: QueryRules) -> Self {
        Self { rules }
    }
    pub fn validate_conditions(&self, conditions: &[Condition]) -> Result<(), QueryError> {
        if conditions.len() > self.rules.max_conditions as usize {
            return Err(QueryError::TooManyConditions);
        }
        for condition in conditions {
            self.validate_operator(&condition.operator)?;
            self.validate_field_type(&condition.field, &condition.value)?;
        }
        Ok(())
    }
    pub fn validate_table(&self, table: &str) -> Result<(), QueryError> {
        if self.rules.allowed_tables.contains(&table.to_string()) {
            Ok(())
        } else {
            Err(QueryError::InvalidTable)
        }
    }
    pub fn validate_operator(&self, operator: &str) -> Result<(), QueryError> {
        if self.rules.allowed_operators.contains(&operator.to_string()) {
            Ok(())
        } else {
            Err(QueryError::InvalidOperator)
        }
    }
    pub fn validate_field_type(
        &self,
        field: &str,
        value: &serde_json::Value,
    ) -> Result<(), QueryError> {
        if let Some(field_type) = self.rules.field_types.get(field) {
            match field_type {
                FieldType::String => {
                    if value.is_string() {
                        Ok(())
                    } else {
                        Err(QueryError::InvalidFieldType)
                    }
                }
                FieldType::Number => {
                    if value.is_number() {
                        Ok(())
                    } else {
                        Err(QueryError::InvalidFieldType)
                    }
                }
                FieldType::Boolean => {
                    if value.is_boolean() {
                        Ok(())
                    } else {
                        Err(QueryError::InvalidFieldType)
                    }
                }
                FieldType::DateTime => {
                    if value.is_string() {
                        Ok(())
                    } else {
                        Err(QueryError::InvalidFieldType)
                    }
                }
            }
        } else {
            Err(QueryError::InvalidFieldType)
        }
    }
    fn generate_feedback(&self, query: &QueryIntent) -> ValidationFeedback {
        let mut errors = Vec::new();
        let mut suggested_fixes = Vec::new();

        if query.target.is_empty() {
            errors.push("Query target cannot be empty".to_string());
            suggested_fixes.push("Specify a valid table or record target".to_string());
        }

        if query.conditions.is_empty() && query.query_type != "select" {
            errors.push("Query conditions are required for this operation".to_string());
            suggested_fixes.push("Add WHERE conditions to specify what to modify".to_string());
        }

        let valid_types = ["select", "create", "update", "delete", "relate"];
        if !valid_types.contains(&query.query_type.as_str()) {
            errors.push(format!("Invalid query type: {}", query.query_type));
            suggested_fixes.push(format!("Use one of: {}", valid_types.join(", ")));
        }

        ValidationFeedback {
            valid: errors.is_empty(),
            errors,
            suggested_fixes,
        }
    }
    pub fn validate_token_batch(
        &self,
        tokens: Vec<IdiomToken>,
    ) -> Vec<Result<ValidatedQuery, QueryError>> {
        tokens
            .into_par_iter()
            .map(|token| self.validate_token(token))
            .collect()
    }
    fn validate_token_part(&self, part: &IdiomPart) -> Result<(), QueryError> {
        match part {
            IdiomPart::Field(field) => {
                if !self.rules.field_types.contains_key(field) && field != "*" && field != "$" {
                    return Err(QueryError::InvalidFieldType);
                }
            }
            IdiomPart::Graph(relation) => {
                if !self.rules.allowed_tables.contains(relation) {
                    return Err(QueryError::InvalidTable);
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn validate_token(&self, token: IdiomToken) -> Result<ValidatedQuery, QueryError> {
        let validation_results: Vec<_> = token
            .parts
            .par_iter()
            .map(|part| self.validate_token_part(part))
            .collect();
        if let Some(err) = validation_results.into_iter().find(|r| r.is_err()) {
            return Err(err.unwrap_err());
        }
        let query = SurrealTokenParser::convert_idiom_to_select_query(&token);
        Ok(ValidatedQuery::Select(Box::new(query)))
    }
}
impl ValidationFeedback {
    pub fn get_suggested_fixes(&self) -> Vec<String> {
        vec![]
    }
}
impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryError::TooManyConditions => write!(f, "Too many conditions"),
            QueryError::UnsupportedQueryType => write!(f, "Unsupported query type"),
            QueryError::InvalidOperator => write!(f, "Invalid operator"),
            QueryError::InvalidFieldType => write!(f, "Invalid field type"),
            QueryError::InvalidTable => write!(f, "Invalid table"),
            QueryError::TooManyAttempts => write!(f, "Exceeded maximum validation attempts"),
            QueryError::InvalidQuery(msg) => write!(f, "Invalid query: {msg}"),
        }
    }
}
