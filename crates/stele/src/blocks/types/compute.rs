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

use crate::blocks::base::BaseBlock;
use crate::blocks::rules::{BlockBehaviour, BlockError, BlockResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
const DEFAULT_OUTPUT_KEY: &str = "compute_result";
const DEFAULT_TARGET: &str = "default";
#[derive(Clone, Deserialize, Serialize)]
pub struct ComputeBlock {
    #[serde(flatten)]
    base: BaseBlock,
}
impl ComputeBlock {
    pub fn new(id: String, properties: HashMap<String, serde_json::Value>) -> Self {
        Self {
            base: BaseBlock::new(id, properties),
        }
    }
    fn evaluate_expression(
        &self,
        expression: &str,
        state: &HashMap<String, Value>,
    ) -> Result<Value, BlockError> {
        if expression.starts_with("state.") {
            self.evaluate_state_access(expression, state)
        } else if expression.contains("{{") && expression.contains("}}") {
            self.evaluate_template(expression, state)
        } else if self.is_mathematical_expression(expression) {
            self.evaluate_math_expression(expression, state)
        } else {
            self.parse_literal_value(expression)
        }
    }
    fn evaluate_state_access(
        &self,
        expression: &str,
        state: &HashMap<String, Value>,
    ) -> Result<Value, BlockError> {
        if let Some(path) = expression.strip_prefix("state.") {
            if let Some(value) = self.get_nested_value(state, path) {
                Ok(value.clone())
            } else {
                Ok(Value::Null)
            }
        } else {
            self.evaluate_math_expression(expression, state)
        }
    }
    fn evaluate_template(
        &self,
        template: &str,
        state: &HashMap<String, Value>,
    ) -> Result<Value, BlockError> {
        let mut result = template.to_string();
        while let Some(start) = result.find("{{") {
            if let Some(end) = result[start..].find("}}") {
                let end = start + end;
                let expr = &result[start + 2..end].trim();
                let value = if expr.starts_with("state.") {
                    let path = expr.strip_prefix("state.").unwrap();
                    self.get_nested_value(state, path)
                        .unwrap_or(&Value::Null)
                        .clone()
                } else {
                    Value::String(expr.to_string())
                };
                let replacement = match value {
                    Value::String(s) => s,
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => "null".to_string(),
                    _ => serde_json::to_string(&value).unwrap_or_default(),
                };
                result.replace_range(start..end + 2, &replacement);
            } else {
                break;
            }
        }
        Ok(Value::String(result))
    }
    fn evaluate_math_expression(
        &self,
        expression: &str,
        state: &HashMap<String, Value>,
    ) -> Result<Value, BlockError> {
        let substituted_expr = self.substitute_state_variables(expression, state)?;
        #[cfg(feature = "evalexpr")]
        {
            use evalexpr::eval;
            match eval(&substituted_expr) {
                Ok(evalexpr::Value::Float(f)) => Ok(Value::Number(
                    serde_json::Number::from_f64(f).unwrap_or_else(|| serde_json::Number::from(0)),
                )),
                Ok(evalexpr::Value::Int(i)) => Ok(Value::Number(serde_json::Number::from(i))),
                Ok(evalexpr::Value::Boolean(b)) => Ok(Value::Bool(b)),
                Ok(evalexpr::Value::String(s)) => Ok(Value::String(s)),
                Ok(_) => Ok(Value::Null),
                Err(e) => Err(BlockError::ProcessingError(format!(
                    "Math evaluation error: {e}"
                ))),
            }
        }
        #[cfg(not(feature = "evalexpr"))]
        {
            self.evaluate_simple_math(&substituted_expr)
        }
    }
    fn substitute_state_variables(
        &self,
        expression: &str,
        state: &HashMap<String, Value>,
    ) -> Result<String, BlockError> {
        let mut result = expression.to_string();
        let mut pos = 0;
        while let Some(start) = result[pos..].find("state.") {
            let actual_start = pos + start;
            let mut end = actual_start + 6;
            while end < result.len() {
                let ch = result.chars().nth(end).unwrap();
                if ch.is_alphanumeric() || ch == '_' || ch == '.' {
                    end += 1;
                } else {
                    break;
                }
            }
            let var_path = &result[actual_start + 6..end];
            let replacement = if let Some(value) = self.get_nested_value(state, var_path) {
                match value {
                    Value::Number(n) => n.to_string(),
                    Value::String(s) => format!("\"{}\"", s.replace('"', "\\\"")),
                    Value::Bool(b) => b.to_string(),
                    _ => "0".to_string(),
                }
            } else {
                "0".to_string()
            };
            result.replace_range(actual_start..end, &replacement);
            pos = actual_start + replacement.len();
        }
        Ok(result)
    }
    #[cfg(not(feature = "evalexpr"))]
    fn evaluate_simple_math(&self, expr: &str) -> Result<Value, BlockError> {
        let expr = expr.trim();
        if expr.starts_with('(') && expr.ends_with(')') {
            return self.evaluate_simple_math(&expr[1..expr.len() - 1]);
        }
        for (i, ch) in expr.char_indices().rev() {
            if (ch == '+' || ch == '-') && i > 0 && i < expr.len() - 1 {
                let left = self.evaluate_simple_math(expr[..i].trim())?;
                let right = self.evaluate_simple_math(expr[i + 1..].trim())?;
                return match ch {
                    '+' => self.add_values(left, right),
                    '-' => self.subtract_values(left, right),
                    _ => unreachable!(),
                };
            }
        }
        for (i, ch) in expr.char_indices().rev() {
            if (ch == '*' || ch == '/') && i > 0 && i < expr.len() - 1 {
                let left = self.evaluate_simple_math(expr[..i].trim())?;
                let right = self.evaluate_simple_math(expr[i + 1..].trim())?;
                return match ch {
                    '*' => self.multiply_values(left, right),
                    '/' => self.divide_values(left, right),
                    _ => unreachable!(),
                };
            }
        }
        self.parse_literal_value(expr)
    }
    fn get_nested_value<'a>(
        &self,
        state: &'a HashMap<String, Value>,
        path: &str,
    ) -> Option<&'a Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = state.get(parts[0])?;
        for part in &parts[1..] {
            current = current.get(part)?;
        }
        Some(current)
    }
    fn is_mathematical_expression(&self, expr: &str) -> bool {
        expr.contains(" + ")
            || expr.contains(" - ")
            || expr.contains(" * ")
            || expr.contains(" / ")
            || expr.contains("state.")
            || expr.contains('(')
            || expr.contains(')')
    }
    fn parse_literal_value(&self, value: &str) -> Result<Value, BlockError> {
        let value = value.trim();
        if value == "true" {
            return Ok(Value::Bool(true));
        }
        if value == "false" {
            return Ok(Value::Bool(false));
        }
        if let Ok(n) = value.parse::<i64>() {
            return Ok(Value::Number(n.into()));
        }
        if let Ok(n) = value.parse::<f64>() {
            if let Some(num) = serde_json::Number::from_f64(n) {
                return Ok(Value::Number(num));
            }
        }
        if (value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\''))
        {
            return Ok(Value::String(value[1..value.len() - 1].to_string()));
        }
        Ok(Value::String(value.to_string()))
    }
    #[cfg(not(feature = "evalexpr"))]
    fn add_values(&self, left: Value, right: Value) -> Result<Value, BlockError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                let result = a.as_f64().unwrap_or(0.0) + b.as_f64().unwrap_or(0.0);
                Ok(Value::Number(
                    serde_json::Number::from_f64(result)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ))
            }
            (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{a}{b}"))),
            (Value::String(a), Value::Number(b)) => Ok(Value::String(format!("{a}{b}"))),
            (Value::Number(a), Value::String(b)) => Ok(Value::String(format!("{a}{b}"))),
            _ => Err(BlockError::ProcessingError(
                "Cannot add these value types".into(),
            )),
        }
    }
    #[cfg(not(feature = "evalexpr"))]
    fn subtract_values(&self, left: Value, right: Value) -> Result<Value, BlockError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                let result = a.as_f64().unwrap_or(0.0) - b.as_f64().unwrap_or(0.0);
                Ok(Value::Number(
                    serde_json::Number::from_f64(result)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ))
            }
            _ => Err(BlockError::ProcessingError(
                "Cannot subtract non-numeric values".into(),
            )),
        }
    }
    #[cfg(not(feature = "evalexpr"))]
    fn multiply_values(&self, left: Value, right: Value) -> Result<Value, BlockError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                let result = a.as_f64().unwrap_or(0.0) * b.as_f64().unwrap_or(0.0);
                Ok(Value::Number(
                    serde_json::Number::from_f64(result)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ))
            }
            _ => Err(BlockError::ProcessingError(
                "Cannot multiply non-numeric values".into(),
            )),
        }
    }
    #[cfg(not(feature = "evalexpr"))]
    fn divide_values(&self, left: Value, right: Value) -> Result<Value, BlockError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                let b_val = b.as_f64().unwrap_or(0.0);
                if b_val == 0.0 {
                    return Err(BlockError::ProcessingError("Division by zero".into()));
                }
                let result = a.as_f64().unwrap_or(0.0) / b_val;
                Ok(Value::Number(
                    serde_json::Number::from_f64(result)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ))
            }
            _ => Err(BlockError::ProcessingError(
                "Cannot divide non-numeric values".into(),
            )),
        }
    }
}
impl BlockBehaviour for ComputeBlock {
    fn id(&self) -> &str {
        &self.base.id
    }
    fn process<'life0, 'async_trait>(
        &'life0 self,
        state: &'life0 mut HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<BlockResult, BlockError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            let expression = self.base.get_required_string("expression")?;
            if expression.starts_with("function:") {
                let function_name = expression.strip_prefix("function:").ok_or_else(|| {
                    BlockError::ProcessingError("Invalid function call syntax".into())
                })?;
                let args = if let Ok(Some(args_key)) = self.base.get_optional_string("args_key") {
                    if let Some(args_value) = state.get(&args_key) {
                        if let Some(args_array) = args_value.as_array() {
                            args_array.clone()
                        } else {
                            vec![args_value.clone()]
                        }
                    } else {
                        vec![]
                    }
                } else if let Ok(args_array) = self.base.get_required_array("args") {
                    args_array.clone()
                } else {
                    vec![]
                };
                let output_key = self
                    .base
                    .get_optional_string("output_key")?
                    .unwrap_or_else(|| DEFAULT_OUTPUT_KEY.to_string());
                let next_block = self
                    .base
                    .get_optional_string("next_block")?
                    .unwrap_or_else(|| DEFAULT_TARGET.to_string());
                return Ok(BlockResult::ExecuteFunction {
                    function_name: function_name.to_string(),
                    args,
                    output_key,
                    next_block,
                    priority: self.base.priority,
                    is_override: self.base.is_override,
                });
            }
            let result = self.evaluate_expression(&expression, state)?;
            let output_key = self
                .base
                .get_optional_string("output_key")?
                .unwrap_or_else(|| DEFAULT_OUTPUT_KEY.to_string());
            let next_block = self
                .base
                .get_optional_string("next_block")?
                .unwrap_or_else(|| DEFAULT_TARGET.to_string());
            state.insert(output_key, result);
            state.insert(
                "navigation_type".to_string(),
                serde_json::Value::String("compute".to_string()),
            );
            state.insert(
                "navigation_priority".to_string(),
                serde_json::Value::Number(self.base.priority.into()),
            );
            state.insert(
                "is_override".to_string(),
                serde_json::Value::Bool(self.base.is_override),
            );
            Ok(BlockResult::Navigate {
                target: next_block,
                priority: self.base.priority,
                is_override: self.base.is_override,
            })
        })
    }
    fn clone_box(&self) -> Box<dyn BlockBehaviour> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn validate(&self) -> Result<(), BlockError> {
        let expression = self.base.get_required_string("expression")?;
        if expression.trim().is_empty() {
            return Err(BlockError::InvalidPropertyType(
                "Expression cannot be empty".to_string(),
            ));
        }
        if expression.starts_with("function:") {
            let function_name = expression.strip_prefix("function:").ok_or_else(|| {
                BlockError::InvalidPropertyType("Invalid function call syntax".to_string())
            })?;
            if function_name.trim().is_empty() {
                return Err(BlockError::InvalidPropertyType(
                    "Function name cannot be empty".to_string(),
                ));
            }
            if self.base.properties.contains_key("args") {
                self.base.get_required_array("args")?;
            }
        }
        self.base.get_optional_string("output_key")?;
        self.base.get_optional_string("next_block")?;
        self.base.get_optional_string("args_key")?;
        self.base.get_optional_f64("priority")?;
        self.base.get_optional_bool("is_override")?;
        Ok(())
    }
}
