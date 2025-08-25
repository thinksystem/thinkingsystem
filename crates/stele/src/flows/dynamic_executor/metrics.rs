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

use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
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


#[derive(Debug)]
pub struct FunctionMetrics {
    total_calls: AtomicU64,
    error_count: AtomicU64,
    avg_execution_time_ns: AtomicU64,
    last_executed_ms: AtomicU64,
    peak_memory_usage: AtomicUsize,
}

impl Default for FunctionMetrics {
    fn default() -> Self {
        Self {
            total_calls: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            avg_execution_time_ns: AtomicU64::new(0),
            last_executed_ms: AtomicU64::new(0),
            peak_memory_usage: AtomicUsize::new(0),
        }
    }
}

impl FunctionMetrics {
    pub fn record_execution(&self, duration: Duration, success: bool) {
        let calls_before = self.total_calls.fetch_add(1, Ordering::Relaxed);
        if !success {
            self.error_count.fetch_add(1, Ordering::Relaxed);
        } else {
            
            let dur_ns = duration.as_nanos() as u64;
            self.avg_execution_time_ns
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                    let new_val = if calls_before == 0 || current == 0 {
                        dur_ns
                    } else {
                        
                        let old_f = current as f64;
                        let new_f = (1.0 - 0.1) * old_f + 0.1 * dur_ns as f64;
                        new_f as u64
                    };
                    Some(new_val)
                })
                .ok();
        }
        
        let now_ms = Utc::now().timestamp_millis() as u64;
        self.last_executed_ms.store(now_ms, Ordering::Relaxed);
    }
    pub fn record_memory_usage(&self, memory_usage: usize) {
        
        let mut current = self.peak_memory_usage.load(Ordering::Relaxed);
        while memory_usage > current {
            match self.peak_memory_usage.compare_exchange(
                current,
                memory_usage,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }
    pub fn reset(&self) {
        self.total_calls.store(0, Ordering::Relaxed);
        self.error_count.store(0, Ordering::Relaxed);
        self.avg_execution_time_ns.store(0, Ordering::Relaxed);
        self.last_executed_ms.store(0, Ordering::Relaxed);
        self.peak_memory_usage.store(0, Ordering::Relaxed);
    }
    pub fn snapshot(&self) -> PerformanceMetrics {
        let total_calls = self.total_calls.load(Ordering::Relaxed);
        let error_count = self.error_count.load(Ordering::Relaxed);
        let avg_ns = self.avg_execution_time_ns.load(Ordering::Relaxed);
        let last_ms = self.last_executed_ms.load(Ordering::Relaxed);
        let peak = self.peak_memory_usage.load(Ordering::Relaxed);
        let success_rate = if total_calls > 0 {
            (total_calls - error_count) as f64 / total_calls as f64
        } else {
            0.0
        };
        let last_executed = if last_ms > 0 {
            let secs = (last_ms / 1000) as i64;
            let millis_part = (last_ms % 1000) as u32;
            let opt = Utc.timestamp_opt(secs, millis_part * 1_000_000);
            opt.single().unwrap_or_else(Utc::now)
        } else {
            Utc::now()
        };
        PerformanceMetrics {
            avg_execution_time: Duration::from_nanos(avg_ns),
            total_calls,
            peak_memory_usage: peak,
            last_executed,
            error_count,
            success_rate,
        }
    }
    pub fn to_stats_map(&self) -> HashMap<String, Value> {
        self.snapshot().to_stats_map()
    }
    
    pub fn calls_per(&self, window: Duration) -> f64 {
        if window.is_zero() {
            return 0.0;
        }
        let snap = self.snapshot();
        let now = Utc::now();
        if now - chrono::Duration::from_std(window).unwrap_or_default() > snap.last_executed {
            0.0
        } else {
            snap.total_calls as f64 / window.as_secs_f64().max(1e-9)
        }
    }
}

impl PerformanceMetrics {
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
            "peak_memory_usage_bytes".to_string(),
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
