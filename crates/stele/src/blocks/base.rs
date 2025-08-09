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

use crate::blocks::rules::BlockError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BaseBlock {
    pub id: String,
    pub properties: HashMap<String, serde_json::Value>,
    pub priority: i32,
    pub is_override: bool,
}
impl BaseBlock {
    pub fn new(id: String, properties: HashMap<String, serde_json::Value>) -> Self {
        let priority = properties
            .get("priority")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        let is_override = properties
            .get("is_override")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        Self {
            id,
            properties,
            priority,
            is_override,
        }
    }
    pub fn get_property(&self, key: &str) -> Result<&serde_json::Value, BlockError> {
        self.properties
            .get(key)
            .ok_or_else(|| BlockError::MissingProperty(format!("'{}' on block '{}'", key, self.id)))
    }
    pub fn get_required_string(&self, key: &str) -> Result<String, BlockError> {
        self.get_property(key)?
            .as_str()
            .map(String::from)
            .ok_or_else(|| {
                BlockError::InvalidPropertyType(format!(
                    "Property '{}' on block '{}' must be a string",
                    key, self.id
                ))
            })
    }
    pub fn get_required_array(&self, key: &str) -> Result<&Vec<serde_json::Value>, BlockError> {
        self.get_property(key)?.as_array().ok_or_else(|| {
            BlockError::InvalidPropertyType(format!(
                "Property '{}' on block '{}' must be an array",
                key, self.id
            ))
        })
    }
    pub fn get_required_f64(&self, key: &str) -> Result<f64, BlockError> {
        self.get_property(key)?.as_f64().ok_or_else(|| {
            BlockError::InvalidPropertyType(format!(
                "Property '{}' on block '{}' must be a number",
                key, self.id
            ))
        })
    }
    pub fn get_required_bool(&self, key: &str) -> Result<bool, BlockError> {
        self.get_property(key)?.as_bool().ok_or_else(|| {
            BlockError::InvalidPropertyType(format!(
                "Property '{}' on block '{}' must be a boolean",
                key, self.id
            ))
        })
    }
    pub fn get_optional_string(&self, key: &str) -> Result<Option<String>, BlockError> {
        match self.properties.get(key) {
            None => Ok(None),
            Some(val) => val.as_str().map(|s| Some(s.to_string())).ok_or_else(|| {
                BlockError::InvalidPropertyType(format!(
                    "Property '{}' on block '{}' must be a string if present",
                    key, self.id
                ))
            }),
        }
    }
    pub fn get_optional_f64(&self, key: &str) -> Result<Option<f64>, BlockError> {
        match self.properties.get(key) {
            None => Ok(None),
            Some(val) => val.as_f64().map(Some).ok_or_else(|| {
                BlockError::InvalidPropertyType(format!(
                    "Property '{}' on block '{}' must be a number if present",
                    key, self.id
                ))
            }),
        }
    }
    pub fn get_optional_bool(&self, key: &str) -> Result<Option<bool>, BlockError> {
        match self.properties.get(key) {
            None => Ok(None),
            Some(val) => val.as_bool().map(Some).ok_or_else(|| {
                BlockError::InvalidPropertyType(format!(
                    "Property '{}' on block '{}' must be a boolean if present",
                    key, self.id
                ))
            }),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParsedCondition {
    Equals {
        var_name: String,
        value: ConditionValue,
    },
    NotEquals {
        var_name: String,
        value: ConditionValue,
    },
    GreaterThan {
        var_name: String,
        value: f64,
    },
    LessThan {
        var_name: String,
        value: f64,
    },
    GreaterThanOrEqual {
        var_name: String,
        value: f64,
    },
    LessThanOrEqual {
        var_name: String,
        value: f64,
    },
    Contains {
        var_name: String,
        value: String,
    },
    StateKeyExists(String),
    AlwaysTrue,
    AlwaysFalse,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConditionValue {
    String(String),
    Number(f64),
    Boolean(bool),
}
impl ConditionValue {
    fn from_str(s: &str) -> Self {
        if let Ok(b) = s.parse::<bool>() {
            return ConditionValue::Boolean(b);
        }
        if let Ok(n) = s.parse::<f64>() {
            return ConditionValue::Number(n);
        }
        ConditionValue::String(s.to_string())
    }
    fn matches_json_value(&self, json_val: &serde_json::Value) -> bool {
        match (self, json_val) {
            (ConditionValue::String(s), serde_json::Value::String(js)) => s == js,
            (ConditionValue::Number(n), serde_json::Value::Number(jn)) => {
                if let Some(jn_f64) = jn.as_f64() {
                    (n - jn_f64).abs() < f64::EPSILON
                } else {
                    false
                }
            }
            (ConditionValue::Boolean(b), serde_json::Value::Bool(jb)) => b == jb,
            (ConditionValue::String(s), _) => {
                let json_str = match json_val {
                    serde_json::Value::String(js) => js.clone(),
                    other => other.to_string().trim_matches('"').to_string(),
                };
                s == &json_str
            }
            (ConditionValue::Number(n), _) => json_val
                .as_f64()
                .is_some_and(|jn| (n - jn).abs() < f64::EPSILON),
            (ConditionValue::Boolean(b), _) => json_val.as_bool().is_some_and(|jb| b == &jb),
        }
    }
}
#[derive(Debug)]
pub enum ConditionParseError {
    InvalidFormat { message: String, condition: String },
    UnsupportedOperator(String),
    InvalidNumericValue(String),
}
impl std::fmt::Display for ConditionParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConditionParseError::InvalidFormat { message, condition } => {
                write!(
                    f,
                    "Invalid condition '{condition}': {message}. Expected format: 'variable operator value'"
                )
            }
            ConditionParseError::UnsupportedOperator(op) => {
                write!(
                    f,
                    "Unsupported operator '{op}'. Supported: ==, !=, >, <, >=, <=, contains"
                )
            }
            ConditionParseError::InvalidNumericValue(val) => {
                write!(
                    f,
                    "Invalid numeric value '{val}'. Use decimal format like '42' or '3.14'"
                )
            }
        }
    }
}
impl std::error::Error for ConditionParseError {}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionSet {
    pub conditions: Vec<ParsedCondition>,
    pub logic: ConditionLogic,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConditionLogic {
    And,
    Or,
    Not,
}
impl ConditionSet {
    pub fn new(logic: ConditionLogic) -> Self {
        Self {
            conditions: Vec::new(),
            logic,
        }
    }
    pub fn add_condition_str(&mut self, condition_str: &str) -> Result<(), ConditionParseError> {
        let parsed = parse_condition(condition_str)?;
        self.conditions.push(parsed);
        Ok(())
    }
    pub fn add_condition(&mut self, condition: ParsedCondition) {
        self.conditions.push(condition);
    }
    pub fn evaluate(&self, state: &HashMap<String, serde_json::Value>) -> bool {
        if self.conditions.is_empty() {
            return true;
        }
        match self.logic {
            ConditionLogic::And => self
                .conditions
                .iter()
                .all(|condition| evaluate_parsed_condition(condition, state)),
            ConditionLogic::Or => self
                .conditions
                .iter()
                .any(|condition| evaluate_parsed_condition(condition, state)),
            ConditionLogic::Not => {
                if self.conditions.len() == 1 {
                    !evaluate_parsed_condition(&self.conditions[0], state)
                } else {
                    !self
                        .conditions
                        .iter()
                        .all(|condition| evaluate_parsed_condition(condition, state))
                }
            }
        }
    }
    pub fn is_empty(&self) -> bool {
        self.conditions.is_empty()
    }
    pub fn len(&self) -> usize {
        self.conditions.len()
    }
}
pub fn parse_condition(condition_str: &str) -> Result<ParsedCondition, ConditionParseError> {
    let condition_str = condition_str.trim();
    if condition_str.is_empty() || condition_str.eq_ignore_ascii_case("true") {
        return Ok(ParsedCondition::AlwaysTrue);
    }
    if condition_str.eq_ignore_ascii_case("false") {
        return Ok(ParsedCondition::AlwaysFalse);
    }
    if !condition_str.contains(' ')
        && !condition_str.contains(">=")
        && !condition_str.contains("<=")
        && !condition_str.contains("!=")
        && !condition_str.contains("==")
        && !condition_str.contains("contains")
        && !condition_str.contains('>')
        && !condition_str.contains('<')
    {
        return Ok(ParsedCondition::StateKeyExists(condition_str.to_string()));
    }
    let (var_name, operator, value_str) = parse_condition_parts(condition_str)?;
    match operator {
        "==" => Ok(ParsedCondition::Equals {
            var_name: var_name.to_string(),
            value: ConditionValue::from_str(value_str),
        }),
        "!=" => Ok(ParsedCondition::NotEquals {
            var_name: var_name.to_string(),
            value: ConditionValue::from_str(value_str),
        }),
        ">" => {
            let num_val = value_str
                .parse::<f64>()
                .map_err(|_| ConditionParseError::InvalidNumericValue(value_str.to_string()))?;
            Ok(ParsedCondition::GreaterThan {
                var_name: var_name.to_string(),
                value: num_val,
            })
        }
        "<" => {
            let num_val = value_str
                .parse::<f64>()
                .map_err(|_| ConditionParseError::InvalidNumericValue(value_str.to_string()))?;
            Ok(ParsedCondition::LessThan {
                var_name: var_name.to_string(),
                value: num_val,
            })
        }
        ">=" => {
            let num_val = value_str
                .parse::<f64>()
                .map_err(|_| ConditionParseError::InvalidNumericValue(value_str.to_string()))?;
            Ok(ParsedCondition::GreaterThanOrEqual {
                var_name: var_name.to_string(),
                value: num_val,
            })
        }
        "<=" => {
            let num_val = value_str
                .parse::<f64>()
                .map_err(|_| ConditionParseError::InvalidNumericValue(value_str.to_string()))?;
            Ok(ParsedCondition::LessThanOrEqual {
                var_name: var_name.to_string(),
                value: num_val,
            })
        }
        "contains" => Ok(ParsedCondition::Contains {
            var_name: var_name.to_string(),
            value: value_str.to_string(),
        }),
        _ => Err(ConditionParseError::UnsupportedOperator(
            operator.to_string(),
        )),
    }
}
pub fn evaluate_parsed_condition(
    parsed_condition: &ParsedCondition,
    state: &HashMap<String, serde_json::Value>,
) -> bool {
    match parsed_condition {
        ParsedCondition::Equals { var_name, value } => state
            .get(var_name)
            .is_some_and(|state_val| value.matches_json_value(state_val)),
        ParsedCondition::NotEquals { var_name, value } => state
            .get(var_name)
            .is_none_or(|state_val| !value.matches_json_value(state_val)),
        ParsedCondition::GreaterThan { var_name, value } => state
            .get(var_name)
            .and_then(|v| v.as_f64())
            .is_some_and(|state_val| state_val > *value),
        ParsedCondition::LessThan { var_name, value } => state
            .get(var_name)
            .and_then(|v| v.as_f64())
            .is_some_and(|state_val| state_val < *value),
        ParsedCondition::GreaterThanOrEqual { var_name, value } => state
            .get(var_name)
            .and_then(|v| v.as_f64())
            .is_some_and(|state_val| state_val >= *value),
        ParsedCondition::LessThanOrEqual { var_name, value } => state
            .get(var_name)
            .and_then(|v| v.as_f64())
            .is_some_and(|state_val| state_val <= *value),
        ParsedCondition::Contains { var_name, value } => state
            .get(var_name)
            .and_then(|v| v.as_str())
            .is_some_and(|state_val| state_val.contains(value)),
        ParsedCondition::StateKeyExists(key) => state.get(key).is_some_and(|v| match v {
            serde_json::Value::Bool(b) => *b,
            serde_json::Value::Null => false,
            _ => true,
        }),
        ParsedCondition::AlwaysTrue => true,
        ParsedCondition::AlwaysFalse => false,
    }
}
pub fn evaluate_condition_str(condition: &str, state: &HashMap<String, serde_json::Value>) -> bool {
    match parse_condition(condition) {
        Ok(parsed) => evaluate_parsed_condition(&parsed, state),
        Err(_) => false,
    }
}
fn parse_condition_parts(condition: &str) -> Result<(&str, &str, &str), ConditionParseError> {
    let operators = [">=", "<=", "!=", "==", "contains", ">", "<"];
    for op in &operators {
        let pattern = format!(" {op} ");
        if let Some(op_pos) = condition.find(&pattern) {
            let var_part = condition[..op_pos].trim();
            let value_part = condition[op_pos + pattern.len()..].trim();
            if var_part.is_empty() || value_part.is_empty() {
                continue;
            }
            return Ok((var_part, op, value_part));
        }
        if let Some(op_pos) = condition.find(op) {
            let var_part = condition[..op_pos].trim();
            let value_part = condition[op_pos + op.len()..].trim();
            if !var_part.is_empty() && !value_part.is_empty() {
                return Ok((var_part, op, value_part));
            }
        }
    }
    Err(ConditionParseError::InvalidFormat {
        message: "No valid operator found".to_string(),
        condition: condition.to_string(),
    })
}
