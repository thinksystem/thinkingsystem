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
use std::str::FromStr;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BooleanToken {
    pub value: bool,
    pub raw_text: String,
}
impl BooleanToken {
    pub fn new(value: bool) -> Self {
        let raw_text = if value { "true" } else { "false" }.to_string();
        Self { value, raw_text }
    }
    pub fn parse_from_str(s: &str) -> Result<Self, String> {
        let lowercase = s.to_lowercase();
        match lowercase.as_str() {
            "true" => Ok(Self {
                value: true,
                raw_text: s.to_string(),
            }),
            "false" => Ok(Self {
                value: false,
                raw_text: s.to_string(),
            }),
            _ => Err(format!("Invalid boolean value: {s}")),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !["true", "false", "TRUE", "FALSE", "True", "False"].contains(&self.raw_text.as_str()) {
            return Err(format!("Invalid boolean format: {}", self.raw_text));
        }
        Ok(())
    }
}
impl PartialOrd for BooleanToken {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.value.cmp(&other.value))
    }
}
impl Ord for BooleanToken {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}
impl std::fmt::Display for BooleanToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.raw_text)
    }
}

impl FromStr for BooleanToken {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_from_str(s)
    }
}
