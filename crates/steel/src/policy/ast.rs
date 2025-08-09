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

use serde_json::Value;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Equals(Box<Expression>, Box<Expression>),

    In(Box<Expression>, Vec<Expression>),

    Contains(Box<Expression>, String),

    Field(String),

    Value(Value),
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expression::Equals(left, right) => write!(f, "({left} == {right})"),
            Expression::In(field, values) => {
                write!(f, "{field} in [")?;
                for (i, value) in values.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{value}")?;
                }
                write!(f, "]")
            }
            Expression::Contains(field, value) => write!(f, "{field}.contains('{value}')"),
            Expression::Field(name) => write!(f, "{name}"),
            Expression::Value(value) => match value {
                Value::String(s) => write!(f, "'{s}'"),
                Value::Number(n) => write!(f, "{n}"),
                Value::Bool(b) => write!(f, "{b}"),
                _ => write!(f, "{value}"),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct EvaluationContext {
    pub data: Value,
}

impl EvaluationContext {
    pub fn new(data: Value) -> Self {
        Self { data }
    }

    pub fn get_field(&self, field_path: &str) -> Option<&Value> {
        if let Some(field_name) = field_path.strip_prefix("data.") {
            self.data.get(field_name)
        } else {
            self.data.get(field_path)
        }
    }

    pub fn contains_field(&self, field_name: &str) -> bool {
        self.data.get(field_name).is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EvaluationResult {
    Bool(bool),
    Error(String),
}

impl From<bool> for EvaluationResult {
    fn from(b: bool) -> Self {
        EvaluationResult::Bool(b)
    }
}

impl EvaluationResult {
    pub fn is_true(&self) -> bool {
        matches!(self, EvaluationResult::Bool(true))
    }

    pub fn is_false(&self) -> bool {
        matches!(self, EvaluationResult::Bool(false))
    }
}

pub fn evaluate(expr: &Expression, context: &EvaluationContext) -> EvaluationResult {
    match expr {
        Expression::Equals(left, right) => {
            let left_val = evaluate(left, context);
            let right_val = evaluate(right, context);

            match (left_val, right_val) {
                (EvaluationResult::Bool(false), _) => false.into(),
                (_, EvaluationResult::Bool(false)) => false.into(),
                _ => {
                    let left_value = extract_value(left, context);
                    let right_value = extract_value(right, context);

                    match (left_value, right_value) {
                        (Some(l), Some(r)) => (l == r).into(),
                        _ => false.into(),
                    }
                }
            }
        }

        Expression::In(field, values) => {
            if let Some(field_value) = extract_value(field, context) {
                for value_expr in values {
                    if let Some(test_value) = extract_value(value_expr, context) {
                        if field_value == test_value {
                            return true.into();
                        }
                    }
                }
            }
            false.into()
        }

        Expression::Contains(field, field_name) => {
            if let Some(target_value) = extract_value(field, context) {
                if let Value::Object(obj) = target_value {
                    obj.contains_key(field_name).into()
                } else if let Value::String(s) = target_value {
                    s.contains(field_name).into()
                } else {
                    false.into()
                }
            } else {
                context.contains_field(field_name).into()
            }
        }

        Expression::Field(_) => true.into(),

        Expression::Value(_) => true.into(),
    }
}

fn extract_value(expr: &Expression, context: &EvaluationContext) -> Option<Value> {
    match expr {
        Expression::Field(field_path) => context.get_field(field_path).cloned(),
        Expression::Value(value) => Some(value.clone()),
        _ => None,
    }
}
