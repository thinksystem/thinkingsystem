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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileToken {
    pub raw_text: String,
    pub bucket: String,
    pub path: String,
}
impl FileToken {
    pub fn new(bucket: &str, path: &str) -> Self {
        let raw_text = format!("f\"{}:/{}\"", bucket, path.trim_start_matches('/'));
        Self {
            raw_text,
            bucket: bucket.to_string(),
            path: path.to_string(),
        }
    }
    pub fn from_str(s: &str) -> Result<Self, String> {
        if !s.starts_with("f\"") || !s.ends_with("\"") {
            return Err(format!(
                "Invalid file pointer format: '{}'. Must start with f\" and end with \".",
                s
            ));
        }
        let inner = &s[2..s.len() - 1];
        if let Some((bucket, path)) = inner.split_once(":/") {
            if bucket.is_empty() {
                return Err("Bucket name in file pointer cannot be empty.".to_string());
            }
            Ok(Self {
                raw_text: s.to_string(),
                bucket: bucket.to_string(),
                path: path.to_string(),
            })
        } else {
            Err("File pointer must contain a ':/' separator between the bucket and the path.".to_string())
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.bucket.is_empty() {
            return Err("FileToken has an empty bucket name.".to_string());
        }
        if !self.raw_text.starts_with("f\"") || !self.raw_text.ends_with("\"") {
            return Err("FileToken raw_text is malformed.".to_string());
        }
        Ok(())
    }
}
impl PartialOrd for FileToken {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for FileToken {
    fn cmp(&self, other: &Self) -> Ordering {
        self.raw_text.cmp(&other.raw_text)
    }
}
impl std::fmt::Display for FileToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.raw_text)
    }
}
