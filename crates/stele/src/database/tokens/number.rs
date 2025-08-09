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

use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
#[derive(Debug, Clone)]
pub struct NumberToken {
    pub value: NumberValue,
    pub raw_text: String,
}
#[derive(Debug, Clone)]
pub enum NumberValue {
    Integer(i64),
    Float(f64),
    Decimal(Decimal),
}
impl NumberToken {
    pub fn new(value: NumberValue, raw_text: String) -> Self {
        Self { value, raw_text }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.validate_decimal_suffix()?;
        Ok(())
    }
    fn validate_decimal_suffix(&self) -> Result<(), String> {
        if !self.raw_text.ends_with("dec") {
            return Ok(());
        }
        let num_str = self.raw_text.trim_end_matches("dec");
        match Decimal::from_str(num_str) {
            Ok(d) => {
                if d.scale() > 28 {
                    Err("Decimal precision exceeds 28 digits".to_string())
                } else {
                    Ok(())
                }
            }
            Err(_) => Err("Invalid decimal number format".to_string()),
        }
    }
    pub fn normalise_float_suffix(mut self) -> Self {
        if self.raw_text.ends_with('f') {
            let num_str = self.raw_text.trim_end_matches('f');
            if let Ok(float_val) = num_str.parse::<f64>() {
                self.value = NumberValue::Float(float_val);
            }
        }
        self
    }
}
impl PartialOrd for NumberToken {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use NumberValue::*;
        match (&self.value, &other.value) {
            (Integer(a), Integer(b)) => a.partial_cmp(b),
            (Float(a), Float(b)) => a.partial_cmp(b),
            (Decimal(a), Decimal(b)) => a.partial_cmp(b),
            (Integer(a), Decimal(b)) => rust_decimal::Decimal::from(*a).partial_cmp(b),
            (Decimal(a), Integer(b)) => a.partial_cmp(&rust_decimal::Decimal::from(*b)),
            (Float(a), Decimal(b)) => rust_decimal::Decimal::from_f64(*a)?.partial_cmp(b),
            (Decimal(a), Float(b)) => a.partial_cmp(&rust_decimal::Decimal::from_f64(*b)?),
            (Integer(a), Float(b)) => (*a as f64).partial_cmp(b),
            (Float(a), Integer(b)) => a.partial_cmp(&(*b as f64)),
        }
    }
}
impl PartialEq for NumberToken {
    fn eq(&self, other: &Self) -> bool {
        self.partial_cmp(other) == Some(std::cmp::Ordering::Equal)
    }
}
impl Eq for NumberToken {}
impl Hash for NumberToken {
    fn hash<H: Hasher>(&self, state: &mut H) {
        use NumberValue::*;
        match self.value {
            Integer(i) => i.hash(state),
            Float(f) if f.fract() == 0.0 && f.is_finite() => (f as i64).hash(state),
            Decimal(d) if d.is_integer() => d.to_i64().unwrap().hash(state),
            Float(f) => f.to_bits().hash(state),
            Decimal(d) => d.to_f64().unwrap_or(f64::NAN).to_bits().hash(state),
        }
    }
}
impl std::fmt::Display for NumberToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.raw_text)
    }
}
