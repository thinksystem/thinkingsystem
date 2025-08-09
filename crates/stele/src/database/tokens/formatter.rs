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

use chrono::{DateTime, NaiveDateTime, Utc};
use std::collections::HashSet;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FormatterToken {
    pub format_type: FormatType,
    pub specifier: String,
    pub description: String,
    pub example: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FormatType {
    Date,
    Time,
    Timezone,
    DateTime,
    Other,
}
impl FormatterToken {
    pub fn new(format_type: FormatType, specifier: String, description: String) -> Self {
        Self {
            format_type,
            specifier,
            description,
            example: None,
        }
    }
    pub fn with_example(mut self, example: String) -> Self {
        self.example = Some(example);
        self
    }
    pub fn validate(&self) -> Result<(), String> {
        let valid_specifiers = self.get_valid_specifiers();
        if !valid_specifiers.contains(self.specifier.as_str()) {
            return Err(format!(
                "Invalid format specifier: '{}' for type {:?}",
                self.specifier, self.format_type
            ));
        }
        Ok(())
    }
    fn get_valid_specifiers(&self) -> HashSet<&'static str> {
        match self.format_type {
            FormatType::Date => [
                "%Y", "%C", "%y", "%m", "%b", "%B", "%h", "%d", "%e", "%a", "%A", "%w", "%u", "%U",
                "%W", "%G", "%g", "%V", "%j", "%D", "%x", "%F", "%v",
            ]
            .into_iter()
            .collect(),
            FormatType::Time => [
                "%H", "%k", "%I", "%l", "%P", "%p", "%M", "%S", "%f", "%.f", "%.3f", "%.6f",
                "%.9f", "%3f", "%6f", "%9f", "%R", "%T", "%X", "%r",
            ]
            .into_iter()
            .collect(),
            FormatType::Timezone => ["%Z", "%z", "%:z"].into_iter().collect(),
            FormatType::DateTime => ["%c", "%+", "%s"].into_iter().collect(),
            FormatType::Other => ["%t", "%n", "%%"].into_iter().collect(),
        }
    }
    pub fn parse_datetime(&self, input: &str) -> Result<DateTime<Utc>, String> {
        let parse_result = match self.format_type {
            FormatType::DateTime => NaiveDateTime::parse_from_str(input, &self.specifier)
                .map_err(|e| e.to_string())?
                .and_utc(),
            FormatType::Date => {
                let naive_date = chrono::NaiveDate::parse_from_str(input, &self.specifier)
                    .map_err(|e| e.to_string())?;
                naive_date.and_hms_opt(0, 0, 0).unwrap().and_utc()
            }
            FormatType::Time => {
                let today = chrono::Utc::now().date_naive();
                let naive_time = chrono::NaiveTime::parse_from_str(input, &self.specifier)
                    .map_err(|e| e.to_string())?;
                today.and_time(naive_time).and_utc()
            }
            _ => return Err("Unsupported format type for parsing".to_string()),
        };
        Ok(parse_result)
    }
    pub fn format_datetime(&self, dt: &DateTime<Utc>) -> Result<String, String> {
        match self.format_type {
            FormatType::DateTime | FormatType::Date | FormatType::Time | FormatType::Timezone => {
                Ok(dt.format(&self.specifier).to_string())
            }
            FormatType::Other => {
                Err("Cannot format datetime with a special format specifier".to_string())
            }
        }
    }
}
impl PartialOrd for FormatterToken {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.specifier.cmp(&other.specifier))
    }
}
impl std::fmt::Display for FormatterToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.specifier)
    }
}
