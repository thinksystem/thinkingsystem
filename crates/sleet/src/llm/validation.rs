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

use crate::llm::{LLMError, LLMResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub enum ValueType {
    String,
    Number,
    Boolean,
    Array,
    Object,
    Any,
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub issues: Vec<String>,
    pub fixed_value: Option<Value>,
}

impl ValidationResult {
    pub fn valid() -> Self {
        Self {
            is_valid: true,
            issues: Vec::new(),
            fixed_value: None,
        }
    }

    pub fn invalid(issue: String) -> Self {
        Self {
            is_valid: false,
            issues: vec![issue],
            fixed_value: None,
        }
    }

    pub fn fixed(issue: String, fixed_value: Value) -> Self {
        Self {
            is_valid: false,
            issues: vec![issue],
            fixed_value: Some(fixed_value),
        }
    }
}

pub type CustomValidation = Box<dyn Fn(&Value) -> ValidationResult + Send + Sync>;

#[derive(Default)]
pub struct ResponseValidator {
    pub required_fields: Vec<String>,

    pub field_types: HashMap<String, ValueType>,

    pub default_values: HashMap<String, Value>,

    pub custom_validations: HashMap<String, CustomValidation>,

    pub auto_fix: bool,
}

impl ResponseValidator {
    pub fn new() -> Self {
        Self {
            auto_fix: true,
            ..Default::default()
        }
    }

    pub fn require_field(mut self, field: impl Into<String>) -> Self {
        self.required_fields.push(field.into());
        self
    }

    pub fn require_fields(mut self, fields: Vec<String>) -> Self {
        self.required_fields.extend(fields);
        self
    }

    pub fn expect_type(mut self, field: impl Into<String>, value_type: ValueType) -> Self {
        self.field_types.insert(field.into(), value_type);
        self
    }

    pub fn with_default(mut self, field: impl Into<String>, default: Value) -> Self {
        self.default_values.insert(field.into(), default);
        self
    }

    pub fn with_custom_validation<F>(mut self, field: impl Into<String>, validation: F) -> Self
    where
        F: Fn(&Value) -> ValidationResult + Send + Sync + 'static,
    {
        self.custom_validations
            .insert(field.into(), Box::new(validation));
        self
    }

    pub fn auto_fix(mut self, enabled: bool) -> Self {
        self.auto_fix = enabled;
        self
    }

    pub fn validate_and_fix(&self, mut response: Value) -> LLMResult<Value> {
        let mut issues = Vec::new();
        let mut was_fixed = false;

        if !response.is_object() {
            if self.auto_fix {
                warn!("Response is not an object, wrapping in object");
                response = json!({ "response": response });
                was_fixed = true;
            } else {
                return Err(LLMError::JsonError(
                    "Response must be a JSON object".to_string(),
                ));
            }
        }

        let response_obj = response.as_object_mut().unwrap();

        for field in &self.required_fields {
            if !response_obj.contains_key(field) {
                if let Some(default) = self.default_values.get(field) {
                    debug!("Adding default value for missing field: {}", field);
                    response_obj.insert(field.clone(), default.clone());
                    was_fixed = true;
                } else if self.auto_fix {
                    let default = self.get_type_default(field);
                    warn!("Field '{}' missing, adding default: {:?}", field, default);
                    response_obj.insert(field.clone(), default);
                    was_fixed = true;
                } else {
                    issues.push(format!("Required field '{field}' is missing"));
                }
            }
        }

        for (field, expected_type) in &self.field_types {
            if let Some(value) = response_obj.get_mut(field) {
                let validation_result = self.validate_field_type(field, value, expected_type);
                if !validation_result.is_valid {
                    issues.extend(validation_result.issues);
                    if let Some(fixed_value) = validation_result.fixed_value {
                        if self.auto_fix {
                            *value = fixed_value;
                            was_fixed = true;
                        }
                    }
                }
            }
        }

        for (field, validation) in &self.custom_validations {
            if let Some(value) = response_obj.get_mut(field) {
                let validation_result = validation(value);
                if !validation_result.is_valid {
                    issues.extend(validation_result.issues);
                    if let Some(fixed_value) = validation_result.fixed_value {
                        if self.auto_fix {
                            *value = fixed_value;
                            was_fixed = true;
                        }
                    }
                }
            }
        }

        if !issues.is_empty() && !self.auto_fix {
            return Err(LLMError::JsonError(format!(
                "Validation failed: {}",
                issues.join(", ")
            )));
        }

        if was_fixed {
            debug!("Response was automatically fixed");
        }

        Ok(response)
    }

    fn validate_field_type(
        &self,
        field: &str,
        value: &Value,
        expected_type: &ValueType,
    ) -> ValidationResult {
        match expected_type {
            ValueType::String => {
                if value.is_string() {
                    ValidationResult::valid()
                } else {
                    ValidationResult::fixed(
                        format!("Field '{field}' should be string"),
                        json!(value.to_string()),
                    )
                }
            }
            ValueType::Number => {
                if value.is_number() {
                    ValidationResult::valid()
                } else if value.is_string() {
                    if let Ok(num) = value.as_str().unwrap().parse::<f64>() {
                        ValidationResult::fixed(
                            format!("Field '{field}' converted from string to number"),
                            json!(num),
                        )
                    } else {
                        ValidationResult::invalid(format!(
                            "Field '{field}' cannot be converted to number"
                        ))
                    }
                } else {
                    ValidationResult::invalid(format!("Field '{field}' should be number"))
                }
            }
            ValueType::Boolean => {
                if value.is_boolean() {
                    ValidationResult::valid()
                } else if value.is_string() {
                    let str_val = value.as_str().unwrap().to_lowercase();
                    match str_val.as_str() {
                        "true" | "yes" | "1" => ValidationResult::fixed(
                            format!("Field '{field}' converted from string to boolean"),
                            json!(true),
                        ),
                        "false" | "no" | "0" => ValidationResult::fixed(
                            format!("Field '{field}' converted from string to boolean"),
                            json!(false),
                        ),
                        _ => ValidationResult::invalid(format!(
                            "Field '{field}' cannot be converted to boolean"
                        )),
                    }
                } else {
                    ValidationResult::invalid(format!("Field '{field}' should be boolean"))
                }
            }
            ValueType::Array => {
                if value.is_array() {
                    ValidationResult::valid()
                } else {
                    ValidationResult::fixed(
                        format!("Field '{field}' wrapped in array"),
                        json!([value]),
                    )
                }
            }
            ValueType::Object => {
                if value.is_object() {
                    ValidationResult::valid()
                } else {
                    ValidationResult::invalid(format!("Field '{field}' should be object"))
                }
            }
            ValueType::Any => ValidationResult::valid(),
        }
    }

    fn get_type_default(&self, field: &str) -> Value {
        if let Some(expected_type) = self.field_types.get(field) {
            match expected_type {
                ValueType::String => json!(""),
                ValueType::Number => json!(0),
                ValueType::Boolean => json!(false),
                ValueType::Array => json!([]),
                ValueType::Object => json!({}),
                ValueType::Any => json!(null),
            }
        } else {
            json!(null)
        }
    }
}

pub fn agent_assessment_validator() -> ResponseValidator {
    ResponseValidator::new()
        .require_fields(vec![
            "goal_achieved".to_string(),
            "confidence".to_string(),
            "reasoning".to_string(),
        ])
        .expect_type("goal_achieved", ValueType::Boolean)
        .expect_type("confidence", ValueType::Number)
        .expect_type("reasoning", ValueType::String)
        .expect_type("missing_elements", ValueType::Array)
        .with_default("goal_achieved", json!(false))
        .with_default("confidence", json!(0.0))
        .with_default("reasoning", json!("No reasoning provided"))
        .with_default("missing_elements", json!([]))
        .with_custom_validation("confidence", |value| {
            if let Some(num) = value.as_f64() {
                if (0.0..=1.0).contains(&num) {
                    ValidationResult::valid()
                } else {
                    let clamped = num.clamp(0.0, 1.0);
                    ValidationResult::fixed(
                        "Confidence value clamped to 0.0-1.0 range".to_string(),
                        json!(clamped),
                    )
                }
            } else {
                ValidationResult::invalid("Confidence must be a number".to_string())
            }
        })
}

pub fn proposal_validator() -> ResponseValidator {
    ResponseValidator::new()
        .require_fields(vec![
            "concept".to_string(),
            "details".to_string(),
            "rationale".to_string(),
        ])
        .expect_type("concept", ValueType::String)
        .expect_type("details", ValueType::Object)
        .expect_type("rationale", ValueType::String)
        .expect_type("changes_made", ValueType::Array)
        .with_default("concept", json!("Undefined concept"))
        .with_default("details", json!({}))
        .with_default("rationale", json!("No rationale provided"))
        .with_default("changes_made", json!([]))
}

pub fn feedback_validator() -> ResponseValidator {
    ResponseValidator::new()
        .require_fields(vec![
            "strengths".to_string(),
            "concerns".to_string(),
            "suggestions".to_string(),
        ])
        .expect_type("strengths", ValueType::Array)
        .expect_type("concerns", ValueType::Array)
        .expect_type("suggestions", ValueType::Array)
        .with_default("strengths", json!([]))
        .with_default("concerns", json!([]))
        .with_default("suggestions", json!([]))
}

pub fn progress_evaluation_validator() -> ResponseValidator {
    ResponseValidator::new()
        .require_fields(vec!["score".to_string(), "reasoning".to_string()])
        .expect_type("score", ValueType::Number)
        .expect_type("reasoning", ValueType::String)
        .with_default("score", json!(5))
        .with_default("reasoning", json!("No reasoning provided"))
        .with_custom_validation("score", |value| {
            if let Some(num) = value.as_f64() {
                let score = num.round() as i32;
                if (1..=10).contains(&score) {
                    if score as f64 != num {
                        ValidationResult::fixed(
                            "Score rounded to integer".to_string(),
                            json!(score),
                        )
                    } else {
                        ValidationResult::valid()
                    }
                } else {
                    let clamped = score.clamp(1, 10);
                    ValidationResult::fixed(
                        "Score clamped to 1-10 range".to_string(),
                        json!(clamped),
                    )
                }
            } else {
                ValidationResult::invalid("Score must be a number".to_string())
            }
        })
}

pub mod validators {
    use super::*;

    pub fn validate_arbiter_assessment(response: Value) -> Value {
        agent_assessment_validator()
            .validate_and_fix(response)
            .unwrap_or_else(|_| {
                warn!("Failed to validate arbiter assessment, using fallback");
                json!({
                    "goal_achieved": false,
                    "confidence": 0.0,
                    "reasoning": "Validation failed, using fallback response",
                    "missing_elements": ["Invalid response format"]
                })
            })
    }

    pub fn validate_proposal_response(response: Value) -> Value {
        proposal_validator()
            .validate_and_fix(response)
            .unwrap_or_else(|_| {
                warn!("Failed to validate proposal, using fallback");
                json!({
                    "concept": "Invalid proposal format",
                    "details": {},
                    "rationale": "Validation failed, using fallback response",
                    "changes_made": []
                })
            })
    }

    pub fn validate_feedback_response(response: Value) -> Value {
        feedback_validator()
            .validate_and_fix(response)
            .unwrap_or_else(|_| {
                warn!("Failed to validate feedback, using fallback");
                json!({
                    "strengths": [],
                    "concerns": ["Invalid feedback format"],
                    "suggestions": []
                })
            })
    }

    pub fn ensure_boolean_field(response: &mut Value, field: &str, default: bool) {
        if let Some(obj) = response.as_object_mut() {
            if !obj.contains_key(field) {
                obj.insert(field.to_string(), json!(default));
            } else if let Some(value) = obj.get_mut(field) {
                if !value.is_boolean() {
                    if let Some(str_val) = value.as_str() {
                        let bool_val = match str_val.to_lowercase().as_str() {
                            "true" | "yes" | "1" => true,
                            "false" | "no" | "0" => false,
                            _ => default,
                        };
                        *value = json!(bool_val);
                    } else {
                        *value = json!(default);
                    }
                }
            }
        }
    }

    pub fn ensure_confidence_range(response: &mut Value, field: &str) {
        if let Some(obj) = response.as_object_mut() {
            if let Some(value) = obj.get_mut(field) {
                if let Some(num) = value.as_f64() {
                    let clamped = num.clamp(0.0, 1.0);
                    if clamped != num {
                        *value = json!(clamped);
                    }
                } else {
                    *value = json!(0.0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_validation() {
        let validator = ResponseValidator::new()
            .require_field("name")
            .expect_type("name", ValueType::String);

        let response = json!({ "name": "test" });
        let result = validator.validate_and_fix(response).unwrap();
        assert_eq!(result["name"], "test");
    }

    #[test]
    fn test_auto_fix_missing_field() {
        let validator = ResponseValidator::new()
            .require_field("missing")
            .with_default("missing", json!("default_value"));

        let response = json!({});
        let result = validator.validate_and_fix(response).unwrap();
        assert_eq!(result["missing"], "default_value");
    }

    #[test]
    fn test_type_conversion() {
        let validator = ResponseValidator::new().expect_type("score", ValueType::Number);

        let response = json!({ "score": "42" });
        let result = validator.validate_and_fix(response).unwrap();
        assert_eq!(result["score"], 42.0);
    }

    #[test]
    fn test_agent_assessment_validator() {
        let response = json!({
            "goal_achieved": "true",
            "confidence": 1.5,
            "reasoning": "Good work"
        });

        let result = validators::validate_arbiter_assessment(response);
        assert_eq!(result["goal_achieved"], true);
        assert_eq!(result["confidence"], 1.0);
    }
}
