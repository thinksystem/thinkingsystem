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
use std::cmp::Ordering;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RangeToken {
    pub start: Option<Box<SurrealToken>>,
    pub end: Option<Box<SurrealToken>>,
    pub inclusive_end: bool,
    pub exclusive_start: bool,
}
impl RangeToken {
    pub fn new(
        start: Option<Box<SurrealToken>>,
        end: Option<Box<SurrealToken>>,
        inclusive_end: bool,
        exclusive_start: bool,
    ) -> Self {
        Self {
            start,
            end,
            inclusive_end,
            exclusive_start,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if let (Some(start), Some(end)) = (&self.start, &self.end) {
            if start > end {
                return Err("Range start bound must be less than or equal to end bound".to_string());
            }
        }
        Ok(())
    }
    pub fn contains(&self, value: &SurrealToken) -> bool {
        let after_start = match &self.start {
            Some(start) => {
                if self.exclusive_start {
                    value > start
                } else {
                    value >= start
                }
            }
            None => true,
        };
        let before_end = match &self.end {
            Some(end) => {
                if self.inclusive_end {
                    value <= end
                } else {
                    value < end
                }
            }
            None => true,
        };
        after_start && before_end
    }
    pub fn is_empty(&self) -> bool {
        if let (Some(start), Some(end)) = (&self.start, &self.end) {
            if start > end {
                return true;
            }
            if start == end {
                return !self.inclusive_end || self.exclusive_start;
            }
        }
        false
    }
    pub fn is_infinite(&self) -> bool {
        self.start.is_none() && self.end.is_none()
    }
    pub fn overlaps(&self, other: &RangeToken) -> bool {
        let self_ends_before_other_starts = match (&self.end, &other.start) {
            (Some(self_end), Some(other_start)) => {
                if self.inclusive_end && !other.exclusive_start {
                    self_end < other_start
                } else {
                    self_end <= other_start
                }
            }
            _ => false,
        };
        let other_ends_before_self_starts = match (&other.end, &self.start) {
            (Some(other_end), Some(self_start)) => {
                if other.inclusive_end && !self.exclusive_start {
                    other_end < self_start
                } else {
                    other_end <= self_start
                }
            }
            _ => false,
        };
        !self_ends_before_other_starts && !other_ends_before_self_starts
    }
}
impl PartialOrd for RangeToken {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (&self.start, &other.start) {
            (Some(a), Some(b)) => a.partial_cmp(b),
            (None, Some(_)) => Some(Ordering::Less),
            (Some(_), None) => Some(Ordering::Greater),
            (None, None) => Some(Ordering::Equal),
        }
    }
}
impl std::fmt::Display for RangeToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(start) = &self.start {
            write!(f, "{start}")?;
        }
        if self.exclusive_start {
            write!(f, ">")?;
        }
        write!(f, "..")?;
        if self.inclusive_end {
            write!(f, "=")?;
        }
        if let Some(end) = &self.end {
            write!(f, "{end}")?;
        }
        Ok(())
    }
}
