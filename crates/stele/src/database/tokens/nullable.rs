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
#[derive(Debug, Clone, Eq)]
pub struct NullableToken {
    pub field_name: String,
    pub is_none: bool,
    pub is_null: bool,
}
impl NullableToken {
    pub fn new(field_name: String) -> Self {
        Self {
            field_name,
            is_none: false,
            is_null: false,
        }
    }
    pub fn none() -> Self {
        Self {
            field_name: String::new(),
            is_none: true,
            is_null: false,
        }
    }
    pub fn null() -> Self {
        Self {
            field_name: String::new(),
            is_none: false,
            is_null: true,
        }
    }
    pub fn with_none(mut self) -> Self {
        self.is_none = true;
        self
    }
    pub fn with_null(mut self) -> Self {
        self.is_null = true;
        self
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.is_none && self.is_null {
            return Err("Token cannot be both NONE and NULL".to_string());
        }
        if self.field_name.is_empty() && !self.is_none && !self.is_null {
            return Err("Field name required for non-NONE/NULL values".to_string());
        }
        Ok(())
    }
}
impl PartialEq for NullableToken {
    fn eq(&self, other: &Self) -> bool {
        self.is_none == other.is_none
            && self.is_null == other.is_null
            && self.field_name == other.field_name
    }
}
impl PartialOrd for NullableToken {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self.is_none, other.is_none, self.is_null, other.is_null) {
            (true, true, _, _) => Some(Ordering::Equal),
            (true, false, _, _) => Some(Ordering::Less),
            (false, true, _, _) => Some(Ordering::Greater),
            (false, false, true, true) => Some(Ordering::Equal),
            (false, false, true, false) => Some(Ordering::Less),
            (false, false, false, true) => Some(Ordering::Greater),
            (false, false, false, false) => Some(self.field_name.cmp(&other.field_name)),
        }
    }
}
impl std::hash::Hash for NullableToken {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.field_name.hash(state);
        self.is_none.hash(state);
        self.is_null.hash(state);
    }
}
impl std::fmt::Display for NullableToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.is_none, self.is_null) {
            (true, _) => write!(f, "NONE"),
            (_, true) => write!(f, "NULL"),
            _ => write!(f, "{}", self.field_name),
        }
    }
}
