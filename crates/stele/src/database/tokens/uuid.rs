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

use lazy_static::lazy_static;
use rand::Rng;
use regex::Regex;
use std::cmp::Ordering;
use time::OffsetDateTime;
lazy_static! {
    static ref UUID_V4_PATTERN: Regex =
        Regex::new(r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
            .unwrap();
    static ref UUID_V7_PATTERN: Regex =
        Regex::new(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
            .unwrap();
}
#[derive(Debug, Clone, PartialEq)]
pub struct UUIDToken {
    pub value: String,
    pub version: UUIDVersion,
}
#[derive(Debug, Clone, Copy, PartialEq, Hash)]
pub enum UUIDVersion {
    V4,
    V7,
}
impl UUIDToken {
    pub fn new(value: String, version: UUIDVersion) -> Self {
        Self { value, version }
    }
    pub fn from_string(s: &str) -> Result<Self, String> {
        let uuid_str = if let Some(stripped) = s.strip_prefix('u') {
            stripped.to_string()
        } else {
            s.to_string()
        };
        let version = if uuid_str.chars().nth(14) == Some('4') {
            UUIDVersion::V4
        } else if uuid_str.chars().nth(14) == Some('7') {
            UUIDVersion::V7
        } else {
            return Err("Invalid UUID version".to_string());
        };
        let token = Self::new(uuid_str, version);
        token.validate()?;
        Ok(token)
    }
    pub fn from_cast(value: &str) -> Result<Self, String> {
        if !value.starts_with("<uuid>") {
            return Err("Invalid UUID cast syntax".to_string());
        }
        Self::from_string(value.trim_start_matches("<uuid>").trim())
    }
    pub fn validate(&self) -> Result<(), String> {
        let uuid_str = self.value.to_lowercase();
        match self.version {
            UUIDVersion::V4 => {
                if !UUID_V4_PATTERN.is_match(&uuid_str) {
                    return Err("Invalid UUIDv4 format".to_string());
                }
            }
            UUIDVersion::V7 => {
                if !UUID_V7_PATTERN.is_match(&uuid_str) {
                    return Err("Invalid UUIDv7 format".to_string());
                }
            }
        }
        Ok(())
    }
    pub fn validate_timestamp_order(&self, previous: &UUIDToken) -> Result<(), String> {
        if self.version != UUIDVersion::V7 || previous.version != UUIDVersion::V7 {
            return Err("Timestamp ordering only applies to UUIDv7".to_string());
        }
        let current_ts = self.extract_timestamp_v7()?;
        let previous_ts = previous.extract_timestamp_v7()?;
        if current_ts < previous_ts {
            return Err("UUIDv7 timestamps must be monotonically increasing".to_string());
        }
        Ok(())
    }
    pub fn generate_v4() -> Self {
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 16];
        rng.fill(&mut bytes);
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let uuid_str = format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15]
        );
        Self::new(uuid_str, UUIDVersion::V4)
    }
    pub fn generate_v7() -> Self {
        let timestamp = OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
        let mut rng = rand::thread_rng();
        let uuid_str = format!(
            "{:08x}-{:04x}-7{:03x}-{:04x}-{:012x}",
            (timestamp >> 16) & 0xFFFFFFFF,
            (timestamp & 0xFFFF),
            rng.gen::<u16>() & 0x0FFF,
            (0x8000 | (rng.gen::<u16>() & 0x3FFF)),
            rng.gen::<u64>()
        );
        Self::new(uuid_str, UUIDVersion::V7)
    }
    pub fn generate_ordered_v7(count: usize) -> Vec<Self> {
        let mut uuids = Vec::with_capacity(count);
        let mut last_ts = None;
        for _ in 0..count {
            let mut uuid = Self::generate_v7();
            if let Some(prev_ts) = last_ts {
                let current_ts = uuid.extract_timestamp_v7().unwrap();
                if current_ts <= prev_ts {
                    uuid = Self::generate_v7_with_timestamp(prev_ts + 1);
                }
            }
            last_ts = uuid.extract_timestamp_v7().ok();
            uuids.push(uuid);
        }
        uuids
    }
    pub fn extract_timestamp_v7(&self) -> Result<i64, String> {
        if self.version != UUIDVersion::V7 {
            return Err("Timestamp can only be extracted from UUIDv7".to_string());
        }
        let uuid_str = self.value.replace("-", "");
        let timestamp_hex = &uuid_str[0..12];
        i64::from_str_radix(timestamp_hex, 16)
            .map_err(|_| "Failed to parse UUIDv7 timestamp".to_string())
    }
    fn generate_v7_with_timestamp(timestamp_ms: i64) -> Self {
        let mut rng = rand::thread_rng();
        let uuid_str = format!(
            "{:08x}-{:04x}-7{:03x}-{:04x}-{:012x}",
            (timestamp_ms >> 16) & 0xFFFFFFFF,
            (timestamp_ms & 0xFFFF),
            rng.gen::<u16>() & 0x0FFF,
            (0x8000 | (rng.gen::<u16>() & 0x3FFF)),
            rng.gen::<u64>()
        );
        Self::new(uuid_str, UUIDVersion::V7)
    }
}
impl Eq for UUIDToken {}
impl PartialOrd for UUIDToken {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if let (UUIDVersion::V7, UUIDVersion::V7) = (self.version, other.version) {
            let self_ts = self.extract_timestamp_v7().ok()?;
            let other_ts = other.extract_timestamp_v7().ok()?;
            if self_ts != other_ts {
                return Some(self_ts.cmp(&other_ts));
            }
        }
        Some(self.value.cmp(&other.value))
    }
}
impl std::fmt::Display for UUIDToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}
impl std::hash::Hash for UUIDToken {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
        self.version.hash(state);
    }
}
