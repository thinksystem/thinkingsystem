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

use super::{ArrayToken, NumberValue, ObjectToken};
use crate::database::*;
use std::collections::HashMap;
#[derive(Debug, Clone, PartialEq)]
pub struct LiteralToken {
    pub variants: Vec<LiteralVariant>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum LiteralVariant {
    String(String),
    Number(f64),
    Object(HashMap<String, SurrealToken>),
    Array(Vec<SurrealToken>),
    Duration(String),
}
impl LiteralToken {
    pub fn new(variants: Vec<LiteralVariant>) -> Self {
        Self { variants }
    }
    pub fn with_variant(mut self, variant: LiteralVariant) -> Self {
        self.variants.push(variant);
        self
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.variants.is_empty() {
            return Err("Literal token must have at least one variant".to_string());
        }
        Ok(())
    }
    pub fn validate_value(&self, value: &SurrealToken) -> Result<(), String> {
        for variant in &self.variants {
            if self.variant_matches_value(variant, value).is_ok() {
                return Ok(());
            }
        }
        Err(format!(
            "Found '{}' but expected {}",
            value,
            self.format_expected_variants()
        ))
    }
    fn variant_matches_value(
        &self,
        variant: &LiteralVariant,
        value: &SurrealToken,
    ) -> Result<(), String> {
        match (variant, value) {
            (LiteralVariant::Number(n), SurrealToken::Number(num)) => match num.value {
                NumberValue::Integer(i) if *n == i as f64 => Ok(()),
                NumberValue::Float(f) if *n == f => Ok(()),
                _ => Err("Number does not match".to_string()),
            },
            (LiteralVariant::String(s), SurrealToken::String(str_token)) => {
                if s == &str_token.value {
                    Ok(())
                } else {
                    Err("String does not match".to_string())
                }
            }
            (LiteralVariant::Object(schema), SurrealToken::Object(obj)) => {
                self.validate_object_schema(schema, obj)
            }
            (LiteralVariant::Array(schema), SurrealToken::Array(arr)) => {
                self.validate_array_schema(schema, arr)
            }
            (LiteralVariant::Duration(d), SurrealToken::DateTime(dt)) => {
                if !d.is_empty() && !dt.to_string().is_empty() {
                    Ok(())
                } else {
                    Err("Duration does not match".to_string())
                }
            }
            _ => Err("Variant type and value type do not align".to_string()),
        }
    }
    pub fn validate_as_schema_definition(&self, table: &str, field: &str) -> Result<(), String> {
        for variant in &self.variants {
            if let LiteralVariant::Object(schema) = variant {
                if schema.contains_key("error") {
                    continue;
                }
                return Err(format!("Invalid literal schema for field '{field}' on table '{table}'. Object variants must be structured to be distinguishable, for example by having a unique key like 'error'."));
            }
        }
        Ok(())
    }
    fn validate_object_schema(
        &self,
        schema: &HashMap<String, SurrealToken>,
        obj: &ObjectToken,
    ) -> Result<(), String> {
        if schema.len() != obj.fields.len() {
            return Err(format!(
                "Object has a different number of fields. Expected {}, got {}.",
                schema.len(),
                obj.fields.len()
            ));
        }
        for (key, expected_type) in schema {
            match obj.fields.get(key) {
                Some(field_value) => {
                    if let SurrealToken::Literal(literal) = expected_type {
                        literal.validate_value(field_value)?;
                    } else if std::mem::discriminant(expected_type)
                        != std::mem::discriminant(field_value)
                    {
                        return Err(format!("Mismatched type for field '{key}'"));
                    }
                }
                None => return Err(format!("Missing required field '{key}'")),
            }
        }
        Ok(())
    }
    fn validate_array_schema(
        &self,
        schema: &[SurrealToken],
        arr: &ArrayToken,
    ) -> Result<(), String> {
        if schema.len() != arr.elements.len() {
            return Err(format!(
                "Array has a different number of elements. Expected {}, got {}.",
                schema.len(),
                arr.elements.len()
            ));
        }
        for (expected_type, actual_value) in schema.iter().zip(arr.elements.iter()) {
            if let SurrealToken::Literal(literal) = expected_type {
                literal.validate_value(actual_value)?;
            } else if std::mem::discriminant(expected_type) != std::mem::discriminant(actual_value)
            {
                return Err(format!(
                    "Mismatched type in array. Expected a variant of {expected_type:?}, but got a variant of {actual_value:?}."
                ));
            }
        }
        Ok(())
    }
    fn format_expected_variants(&self) -> String {
        self.variants
            .iter()
            .map(|v| match v {
                LiteralVariant::Number(n) => n.to_string(),
                LiteralVariant::String(s) => format!("'{s}'"),
                LiteralVariant::Object(_) => "{ ... }".to_string(),
                LiteralVariant::Array(_) => "[...]".to_string(),
                LiteralVariant::Duration(d) => d.clone(),
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}
impl std::fmt::Display for LiteralToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_expected_variants())
    }
}
impl Eq for LiteralToken {}
impl Eq for LiteralVariant {}
impl std::hash::Hash for LiteralToken {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.variants.hash(state);
    }
}
impl std::hash::Hash for LiteralVariant {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            LiteralVariant::String(s) => {
                0u8.hash(state);
                s.hash(state);
            }
            LiteralVariant::Number(n) => {
                1u8.hash(state);
                n.to_bits().hash(state);
            }
            LiteralVariant::Object(obj) => {
                2u8.hash(state);
                let mut entries: Vec<_> = obj.iter().collect();
                entries.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
                for (k, v) in entries {
                    k.hash(state);
                    v.hash(state);
                }
            }
            LiteralVariant::Array(arr) => {
                3u8.hash(state);
                for item in arr {
                    item.hash(state);
                }
            }
            LiteralVariant::Duration(d) => {
                4u8.hash(state);
                d.hash(state);
            }
        }
    }
}
