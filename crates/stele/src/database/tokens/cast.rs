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

use crate::database::*;
#[derive(Debug)]
pub struct CastToken {
    pub target_type: String,
    pub value: Box<SurrealToken>,
    pub nested_types: Vec<String>,
}
impl PartialEq for CastToken {
    fn eq(&self, other: &Self) -> bool {
        self.target_type == other.target_type
            && self.nested_types == other.nested_types
            && self.value == other.value
    }
}
impl Eq for CastToken {}
impl Clone for CastToken {
    fn clone(&self) -> Self {
        Self {
            target_type: self.target_type.clone(),
            nested_types: self.nested_types.clone(),
            value: self.value.clone(),
        }
    }
}
impl std::hash::Hash for CastToken {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.target_type.hash(state);
        self.nested_types.hash(state);
        self.value.hash(state);
    }
}
impl std::fmt::Display for CastToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<{}", self.target_type)?;
        if !self.nested_types.is_empty() {
            write!(f, "<{}>", self.nested_types.join("|"))?;
        }
        write!(f, ">{}", self.value)
    }
}
impl CastToken {
    pub fn new(target_type: String, value: SurrealToken) -> Self {
        Self {
            target_type,
            value: Box::new(value),
            nested_types: Vec::new(),
        }
    }
    pub fn with_nested_types(mut self, types: Vec<String>) -> Self {
        self.nested_types = types;
        self
    }
    pub fn validate(&self) -> Result<(), String> {
        match self.target_type.as_str() {
            "array" | "set" => self.validate_collection_cast(),
            "record" => self.validate_record_cast(),
            "decimal" | "number" => self.validate_decimal_cast(),
            "datetime" => self.validate_datetime_cast(),
            "duration" => self.validate_duration_cast(),
            "float" | "int" => self.validate_numeric_cast(),
            "bool" | "string" | "uuid" => Ok(()),
            "regex" => self.validate_regex_cast(),
            _ => Err(format!("Unknown cast target type: {}", self.target_type)),
        }
    }
    fn validate_collection_cast(&self) -> Result<(), String> {
        if self.nested_types.is_empty() {
            return Err(format!(
                "{} cast requires at least one nested type",
                self.target_type
            ));
        }
        for nested_type in &self.nested_types {
            match nested_type.as_str() {
                "bool" | "string" | "float" | "int" | "decimal" | "datetime" | "duration"
                | "uuid" | "record" => continue,
                _ => return Err(format!("Invalid nested type: {nested_type}")),
            }
        }
        Ok(())
    }
    fn validate_record_cast(&self) -> Result<(), String> {
        match &*self.value {
            SurrealToken::String(s) => {
                if !s.value.contains(':') {
                    return Err("Record cast requires table:id format".to_string());
                }
                Ok(())
            }
            _ => Err("Record cast requires string input".to_string()),
        }
    }
    fn validate_decimal_cast(&self) -> Result<(), String> {
        match &*self.value {
            SurrealToken::Number(n) => {
                if n.raw_text.contains('E') || n.raw_text.contains('e') {
                    return Err("Scientific notation not allowed in decimal cast".to_string());
                }
                Ok(())
            }
            SurrealToken::String(s) => {
                if !s
                    .value
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
                {
                    return Err("Invalid decimal string format".to_string());
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    fn validate_datetime_cast(&self) -> Result<(), String> {
        match &*self.value {
            SurrealToken::String(s) => {
                if !s.value.contains('T') && !s.value.contains('-') {
                    return Err("Invalid datetime format".to_string());
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    fn validate_duration_cast(&self) -> Result<(), String> {
        match &*self.value {
            SurrealToken::String(s) => {
                if !s.value.chars().any(|c| "hmsd".contains(c)) {
                    return Err("Invalid duration format".to_string());
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    fn validate_numeric_cast(&self) -> Result<(), String> {
        match &*self.value {
            SurrealToken::String(s) => {
                if !s
                    .value
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
                {
                    return Err("Invalid numeric string format".to_string());
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    fn validate_regex_cast(&self) -> Result<(), String> {
        match &*self.value {
            SurrealToken::String(_) => Ok(()),
            _ => Err("Regex cast requires a string pattern as input".to_string()),
        }
    }
}
