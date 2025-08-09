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
use regex::Regex;
use std::collections::HashMap;
use std::hash::Hash;
#[derive(Debug, Clone, PartialEq)]
pub struct RecordIdToken {
    pub table: String,
    pub identifier: RecordIdentifier,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordIdentifier {
    Text(String),
    Number(i64),
    Object(HashMap<String, SurrealToken>),
    Array(Vec<SurrealToken>),
    Generated(String),
}
impl RecordIdToken {
    pub fn new(table: String, identifier: RecordIdentifier) -> Self {
        Self { table, identifier }
    }
    pub fn from_string(s: String) -> Result<Self, String> {
        let identifier = if let Ok(num) = s.parse::<i64>() {
            RecordIdentifier::Text(num.to_string())
        } else if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            RecordIdentifier::Text(format!("⟨{s}⟩"))
        } else {
            RecordIdentifier::Text(s)
        };
        Ok(Self {
            table: String::new(),
            identifier,
        })
    }
    pub fn from_range(
        start: Option<Vec<SurrealToken>>,
        end: Option<Vec<SurrealToken>>,
    ) -> Result<Self, String> {
        let identifier = match (start, end) {
            (Some(s), Some(e)) if s.len() == e.len() => RecordIdentifier::Array(s),
            (Some(s), None) => RecordIdentifier::Array(s),
            (None, Some(e)) => RecordIdentifier::Array(e),
            _ => return Err("At least one range bound must be specified".to_string()),
        };
        let token = Self {
            table: String::new(),
            identifier,
        };
        token.validate_array_range()?;
        Ok(token)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.validate_identifier()?;
        self.validate_generated()?;
        self.validate_array_range()?;
        self.validate_nested_links()
    }
    pub fn validate_identifier(&self) -> Result<(), String> {
        match &self.identifier {
            RecordIdentifier::Object(map) => {
                let keys: Vec<_> = map.keys().collect();
                if keys.windows(2).any(|w| w[0] > w[1]) {
                    return Err(
                        "Object keys in record ID must be sorted alphabetically".to_string()
                    );
                }
                Ok(())
            }
            RecordIdentifier::Text(text) => {
                let is_valid = text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    || (text.starts_with('⟨') && text.ends_with('⟩'));
                if !is_valid {
                    return Err("Text record ID must contain only letters, numbers, and underscores, or be enclosed in ⟨⟩".to_string());
                }
                Ok(())
            }
            RecordIdentifier::Number(_) => Ok(()),
            _ => Ok(()),
        }
    }
    pub fn validate_traversal(&self, max_depth: usize) -> Result<(), String> {
        let mut depth = 0;
        let mut current = self;
        while let Some(next) = current.get_next_reference() {
            depth += 1;
            if depth > max_depth {
                return Err(format!(
                    "Record traversal depth {depth} exceeds maximum allowed depth {max_depth}"
                ));
            }
            current = next;
        }
        Ok(())
    }
    pub fn validate_circular_refs(&self, visited: &mut Vec<String>) -> Result<(), String> {
        let record_path = format!("{}{}", self.table, self.identifier);
        if visited.contains(&record_path) {
            return Err(format!(
                "Circular reference detected in path: {}",
                visited.join(" -> ")
            ));
        }
        visited.push(record_path);
        if let RecordIdentifier::Object(fields) = &self.identifier {
            for value in fields.values() {
                if let SurrealToken::RecordId(record) = value {
                    record.validate_circular_refs(visited)?;
                }
            }
        }
        visited.pop();
        Ok(())
    }
    pub fn get_next_reference(&self) -> Option<&RecordIdToken> {
        if let RecordIdentifier::Object(fields) = &self.identifier {
            for value in fields.values() {
                if let SurrealToken::RecordId(record) = value {
                    return Some(record);
                }
            }
        }
        None
    }
    fn validate_generated(&self) -> Result<(), String> {
        if let RecordIdentifier::Generated(id_str) = &self.identifier {
            match id_str.split('(').next().unwrap_or("") {
                "rand" if !is_valid_rand(id_str) => {
                    Err("rand() ID must be 20 characters long using [a-z0-9]".to_string())
                }
                "ulid" if !is_valid_ulid(id_str) => {
                    Err("ulid() must be 26 characters in Crockford's Base32".to_string())
                }
                "uuid" if !is_valid_uuid(id_str) => {
                    Err("uuid() must be a valid UUIDv7 format".to_string())
                }
                "" => Err("Unknown ID generation function".to_string()),
                _ => Ok(()),
            }
        } else {
            Ok(())
        }
    }
    fn validate_array_range(&self) -> Result<(), String> {
        if let RecordIdentifier::Array(elements) = &self.identifier {
            validate_array_elements(elements)?;
            validate_array_sorting(elements)?;
        }
        Ok(())
    }
    fn validate_nested_links(&self) -> Result<(), String> {
        match &self.identifier {
            RecordIdentifier::Array(elements) => {
                for value in elements {
                    if let SurrealToken::RecordId(record) = value {
                        record.validate_nested_links()?;
                    }
                }
            }
            RecordIdentifier::Object(map) => {
                for value in map.values() {
                    if let SurrealToken::RecordId(record) = value {
                        record.validate_nested_links()?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
fn is_valid_rand(id: &str) -> bool {
    id.len() == 20
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}
fn is_valid_ulid(id: &str) -> bool {
    id.len() == 26 && id.chars().all(|c| c.is_ascii_alphanumeric())
}
fn is_valid_uuid(id: &str) -> bool {
    let uuid_pattern =
        Regex::new(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
            .unwrap();
    uuid_pattern.is_match(id)
}
fn validate_array_elements(elements: &[SurrealToken]) -> Result<(), String> {
    for element in elements {
        match element {
            SurrealToken::String(_)
            | SurrealToken::DateTime(_)
            | SurrealToken::NullableValue(_) => continue,
            _ => return Err("Array range elements must be string, datetime, or NONE".to_string()),
        }
    }
    Ok(())
}
fn validate_array_sorting(elements: &[SurrealToken]) -> Result<(), String> {
    if elements.windows(2).any(|w| w[0] > w[1]) {
        return Err("Array elements must maintain natural sorting order".to_string());
    }
    Ok(())
}
impl Eq for RecordIdToken {}
impl std::fmt::Display for RecordIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(s) => write!(f, "{s}"),
            Self::Number(n) => write!(f, "{n}"),
            Self::Object(o) => write!(f, "{o:?}"),
            Self::Array(a) => write!(f, "{a:?}"),
            Self::Generated(g) => write!(f, "{g}"),
        }
    }
}
impl Hash for RecordIdToken {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.table.hash(state);
        self.identifier.hash(state);
    }
}
impl Hash for RecordIdentifier {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Self::Text(s) => {
                0_u8.hash(state);
                s.hash(state);
            }
            Self::Number(n) => {
                1_u8.hash(state);
                n.hash(state);
            }
            Self::Object(map) => {
                2_u8.hash(state);
                let mut keys: Vec<_> = map.keys().collect();
                keys.sort();
                for key in keys {
                    key.hash(state);
                    map.get(key).unwrap().hash(state);
                }
            }
            Self::Array(arr) => {
                3_u8.hash(state);
                for item in arr {
                    item.hash(state);
                }
            }
            Self::Generated(s) => {
                4_u8.hash(state);
                s.hash(state);
            }
        }
    }
}
