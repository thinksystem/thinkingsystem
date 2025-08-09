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

use std::cmp::Ordering;
use std::hash::Hash;
#[derive(Debug)]
pub struct StringToken {
    pub value: String,
    pub prefix: Option<StringPrefix>,
    pub contains_unicode: bool,
    pub multiline: bool,
}
#[derive(Debug, Clone)]
pub enum StringPrefix {
    String,
    Record,
    DateTime,
    UUID,
    Bytes,
    FilePath,
}
impl StringToken {
    pub fn new(value: String) -> Self {
        Self {
            value,
            prefix: None,
            contains_unicode: false,
            multiline: false,
        }
    }
    pub fn with_prefix(mut self, prefix: StringPrefix) -> Self {
        self.prefix = Some(prefix);
        self
    }
    pub fn with_multiline(mut self, is_multiline: bool) -> Self {
        self.multiline = is_multiline;
        self
    }
    pub fn validate(&self) -> Result<(), String> {
        self.validate_multiline()?;
        self.validate_prefix()
    }
    fn validate_multiline(&self) -> Result<(), String> {
        if self.multiline && !self.value.contains('\n') {
            return Err("Multiline string must contain newlines".to_string());
        }
        Ok(())
    }
    fn validate_prefix(&self) -> Result<(), String> {
        if let Some(prefix) = &self.prefix {
            match prefix {
                StringPrefix::DateTime => self.validate_datetime(),
                StringPrefix::UUID => self.validate_uuid(),
                StringPrefix::Record => self.validate_record_id(),
                StringPrefix::Bytes => self.validate_bytes(),
                StringPrefix::FilePath => self.validate_filepath(),
                StringPrefix::String => Ok(()),
            }
        } else {
            Ok(())
        }
    }
    fn validate_datetime(&self) -> Result<(), String> {
        let parts: Vec<&str> = self.value.split('T').collect();
        if parts.len() != 2 {
            return Err("DateTime must contain ISO 8601 'T' separator".to_string());
        }
        let (date, time) = (parts[0], parts[1]);
        if date.len() != 10 || date.matches('-').count() != 2 {
            return Err("Invalid date format - expected YYYY-MM-DD".to_string());
        }
        if !time.contains(':') {
            return Err("Invalid time format - expected HH:mm:ss".to_string());
        }
        if !(time.contains('Z') || time.contains('+') || time.contains('-')) {
            return Err("DateTime must specify timezone (Z, +HH:mm, or -HH:mm)".to_string());
        }
        Ok(())
    }
    fn validate_uuid(&self) -> Result<(), String> {
        if self.value.len() != 36 || self.value.matches('-').count() != 4 {
            return Err("Invalid UUID format".to_string());
        }
        if !self
            .value
            .chars()
            .all(|c| c == '-' || c.is_ascii_hexdigit())
        {
            return Err(
                "UUID must contain only hexadecimal characters (0-9, a-f, A-F)".to_string(),
            );
        }
        Ok(())
    }
    fn validate_record_id(&self) -> Result<(), String> {
        if !self.value.contains(':') {
            return Err("Record ID must contain table name and ID separated by ':'".to_string());
        }
        let parts: Vec<&str> = self.value.split(':').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err("Invalid record ID format - expected table_name:record_id".to_string());
        }
        Ok(())
    }
    fn validate_bytes(&self) -> Result<(), String> {
        if self.value.len() % 2 != 0 {
            return Err(
                "Byte string must have an even number of hexadecimal characters".to_string(),
            );
        }
        if !self.value.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(
                "Byte string must contain only hexadecimal characters (0-9, a-f, A-F)".to_string(),
            );
        }
        Ok(())
    }
    fn validate_filepath(&self) -> Result<(), String> {
        if !self.value.contains(":/") {
            return Err("File path must contain a bucket and key separated by ':/'".to_string());
        }
        let parts: Vec<&str> = self.value.splitn(2, ":/").collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err("Invalid file path format - expected bucket:/key".to_string());
        }
        Ok(())
    }
    pub fn detect_unicode(&mut self) {
        self.contains_unicode = !self.value.is_ascii();
    }
}
impl Clone for StringToken {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            prefix: self.prefix.clone(),
            contains_unicode: self.contains_unicode,
            multiline: self.multiline,
        }
    }
}
impl PartialEq for StringToken {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
impl Eq for StringToken {}
impl PartialOrd for StringToken {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.value.cmp(&other.value))
    }
}
impl Hash for StringToken {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}
impl std::fmt::Display for StringToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}
