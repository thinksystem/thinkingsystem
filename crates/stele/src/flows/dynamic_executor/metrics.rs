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

use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
#[derive(Clone, Debug, Default)]
pub struct PerformanceMetrics {
    pub avg_execution_time: Duration,
    pub total_calls: u64,
    pub peak_memory_usage: usize,
    pub last_executed: DateTime<Utc>,
    pub error_count: u64,
    pub success_rate: f64,
}
impl PerformanceMetrics {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn record_execution(&mut self, duration: Duration, success: bool) {
        self.total_calls += 1;
        self.last_executed = Utc::now();
        if success {
            if self.total_calls == 1 {
                self.avg_execution_time = duration;
            } else {
                let alpha = 0.1;
                let new_avg_nanos = (1.0 - alpha) * self.avg_execution_time.as_nanos() as f64
                    + alpha * duration.as_nanos() as f64;
                self.avg_execution_time = Duration::from_nanos(new_avg_nanos as u64);
            }
        } else {
            self.error_count += 1;
        }
        if self.total_calls > 0 {
            self.success_rate =
                (self.total_calls - self.error_count) as f64 / self.total_calls as f64;
        } else {
            self.success_rate = 0.0;
        }
    }
    pub fn record_memory_usage(&mut self, memory_usage: usize) {
        if memory_usage > self.peak_memory_usage {
            self.peak_memory_usage = memory_usage;
        }
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn merge(&mut self, other: &PerformanceMetrics) {
        let total_calls = self.total_calls + other.total_calls;
        if total_calls > 0 {
            let self_weight = self.total_calls as f64 / total_calls as f64;
            let other_weight = other.total_calls as f64 / total_calls as f64;
            let new_avg_nanos = self_weight * self.avg_execution_time.as_nanos() as f64
                + other_weight * other.avg_execution_time.as_nanos() as f64;
            self.avg_execution_time = Duration::from_nanos(new_avg_nanos as u64);
            self.total_calls = total_calls;
            self.error_count += other.error_count;
            self.success_rate =
                (self.total_calls - self.error_count) as f64 / self.total_calls as f64;
            self.peak_memory_usage = self.peak_memory_usage.max(other.peak_memory_usage);
            self.last_executed = self.last_executed.max(other.last_executed);
        } else if self.total_calls == 0 {
            self.avg_execution_time = other.avg_execution_time;
            self.peak_memory_usage = other.peak_memory_usage;
            self.last_executed = other.last_executed;
        }
    }
    pub fn get_calls_per_second(&self, time_window: Duration) -> f64 {
        if time_window.is_zero() {
            return 0.0;
        }
        let now = Utc::now();
        let chrono_time_window = match chrono::Duration::from_std(time_window) {
            Ok(d) => d,
            Err(_) => return 0.0,
        };
        let window_start = now - chrono_time_window;
        if self.last_executed < window_start {
            0.0
        } else if time_window.as_secs_f64() > 0.0 {
            self.total_calls as f64 / time_window.as_secs_f64()
        } else {
            0.0
        }
    }
    pub fn to_stats_map(&self) -> HashMap<String, Value> {
        let mut stats = HashMap::new();
        stats.insert(
            "avg_execution_time_ms".to_string(),
            serde_json::Number::from_f64(self.avg_execution_time.as_millis() as f64)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        );
        stats.insert(
            "total_calls".to_string(),
            Value::Number(self.total_calls.into()),
        );
        stats.insert(
            "peak_memory_usage".to_string(),
            Value::Number(self.peak_memory_usage.into()),
        );
        stats.insert(
            "error_count".to_string(),
            Value::Number(self.error_count.into()),
        );
        stats.insert(
            "success_rate".to_string(),
            serde_json::Number::from_f64(self.success_rate)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        );
        if self.total_calls > 0 {
            stats.insert(
                "last_executed".to_string(),
                Value::String(self.last_executed.to_rfc3339()),
            );
        }
        stats
    }
}
