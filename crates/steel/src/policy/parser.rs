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

use crate::policy::ast::Expression;
use serde_json::Value;

pub struct ConditionParser;

impl ConditionParser {
    pub fn parse(condition: &str) -> Result<Expression, String> {
        let condition = condition.trim();

        if condition.contains(" in [") {
            return Self::parse_in_expression(condition);
        }

        if condition.contains(" == ") {
            return Self::parse_equals_expression(condition);
        }

        if condition.contains(".contains(") {
            return Self::parse_contains_expression(condition);
        }

        Err(format!("Unsupported condition format: {condition}"))
    }

    fn parse_in_expression(condition: &str) -> Result<Expression, String> {
        let parts: Vec<&str> = condition.splitn(2, " in [").collect();
        if parts.len() != 2 {
            return Err("Invalid 'in' expression format".to_string());
        }

        let field = parts[0].trim();
        let values_part = parts[1].trim();

        let values_part = values_part
            .strip_suffix(']')
            .ok_or("Missing closing bracket in 'in' expression")?;

        let mut values = Vec::new();
        for value in values_part.split(',') {
            let value = value.trim();
            if let Some(string_value) = Self::parse_string_literal(value) {
                values.push(Expression::Value(Value::String(string_value)));
            } else {
                return Err(format!("Unsupported value format: {value}"));
            }
        }

        Ok(Expression::In(
            Box::new(Expression::Field(field.to_string())),
            values,
        ))
    }

    fn parse_equals_expression(condition: &str) -> Result<Expression, String> {
        let parts: Vec<&str> = condition.splitn(2, " == ").collect();
        if parts.len() != 2 {
            return Err("Invalid equality expression format".to_string());
        }

        let left = parts[0].trim();
        let right = parts[1].trim();

        let left_expr = Expression::Field(left.to_string());
        let right_expr = if let Some(string_value) = Self::parse_string_literal(right) {
            Expression::Value(Value::String(string_value))
        } else {
            return Err(format!("Unsupported value format: {right}"));
        };

        Ok(Expression::Equals(
            Box::new(left_expr),
            Box::new(right_expr),
        ))
    }

    fn parse_contains_expression(condition: &str) -> Result<Expression, String> {
        if let Some(start) = condition.find(".contains('") {
            let field_part = &condition[..start];
            let remaining = &condition[start + 11..];

            if let Some(end) = remaining.find("')") {
                let field_name = &remaining[..end];
                return Ok(Expression::Contains(
                    Box::new(Expression::Field(field_part.to_string())),
                    field_name.to_string(),
                ));
            }
        }

        Err("Invalid contains expression format".to_string())
    }

    fn parse_string_literal(value: &str) -> Option<String> {
        if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
            Some(value[1..value.len() - 1].to_string())
        } else {
            None
        }
    }
}
