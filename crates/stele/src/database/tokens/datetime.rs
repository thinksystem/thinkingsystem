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

use chrono::{DateTime, NaiveDate, Utc};
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
#[derive(Debug, Clone, Eq)]
pub struct DateTimeToken {
    value: DateTime<Utc>,
    precision: String,
}
impl DateTimeToken {
    pub fn new(timestamp_str: &str) -> Result<Self, String> {
        if let Ok(dt) = DateTime::parse_from_rfc3339(timestamp_str) {
            return Ok(Self {
                value: dt.with_timezone(&Utc),
                precision: "ns".to_string(),
            });
        }
        if let Ok(naive_date) = NaiveDate::parse_from_str(timestamp_str, "%Y-%m-%d") {
            return Ok(Self {
                value: naive_date
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_local_timezone(Utc)
                    .unwrap(),
                precision: "s".to_string(),
            });
        }
        Err(format!(
            "Could not parse '{timestamp_str}' as a valid ISO-8601 datetime or YYYY-MM-DD date."
        ))
    }
    pub fn with_precision(mut self, precision: String) -> Result<Self, String> {
        match precision.as_str() {
            "ns" | "us" | "ms" | "s" => {
                self.precision = precision;
                Ok(self)
            }
            _ => Err(format!(
                "Invalid precision: {precision}. Expected ns, us, ms, or s"
            )),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        Ok(())
    }
    pub fn to_rfc3339(&self) -> String {
        match self.precision.as_str() {
            "s" => self
                .value
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "ms" => self
                .value
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "us" => self
                .value
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
            "ns" => self
                .value
                .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            _ => self
                .value
                .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
        }
    }
}
impl PartialEq for DateTimeToken {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
impl Hash for DateTimeToken {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}
impl PartialOrd for DateTimeToken {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.value.partial_cmp(&other.value)
    }
}
impl std::fmt::Display for DateTimeToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_rfc3339())
    }
}
