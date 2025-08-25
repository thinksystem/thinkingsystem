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


#![allow(dead_code)]


use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration; 

use super::engine::EngineMetrics;

#[derive(Debug, Default)]
#[allow(dead_code)] 
pub struct EngineMetricsAtomic {
    processing_time_ns: AtomicU64,
    blocks_processed: AtomicUsize,
    memory_usage: AtomicUsize,
    function_calls: RwLock<HashMap<String, usize>>,
    version_history: RwLock<Vec<String>>,
    last_reload: RwLock<DateTime<Utc>>,
}

#[allow(dead_code)]
impl EngineMetricsAtomic {
    pub fn new() -> Self {
        Self::default()
    }
    #[inline]
    pub fn add_processing_time(&self, dur: Duration) {
        self.processing_time_ns
            .fetch_add(dur.as_nanos() as u64, Ordering::Relaxed);
    }
    #[inline]
    pub fn increment_blocks_processed(&self, count: usize) {
        if count > 0 {
            self.blocks_processed.fetch_add(count, Ordering::Relaxed);
        }
    }
    #[inline]
    pub fn set_memory_usage(&self, bytes: usize) {
        self.memory_usage.store(bytes, Ordering::Relaxed);
    }
    #[inline]
    pub fn increment_function_call(&self, function_name: &str) {
        let mut map = self.function_calls.write();
        *map.entry(function_name.to_string()).or_insert(0) += 1;
    }
    #[inline]
    pub fn push_version_history(&self, version_id: String) {
        self.version_history.write().push(version_id);
    }
    #[inline]
    pub fn update_last_reload(&self) {
        *self.last_reload.write() = Utc::now();
    }
    pub fn snapshot(&self) -> EngineMetrics {
        EngineMetrics {
            processing_time: Duration::from_nanos(self.processing_time_ns.load(Ordering::Relaxed)),
            blocks_processed: self.blocks_processed.load(Ordering::Relaxed),
            memory_usage: self.memory_usage.load(Ordering::Relaxed),
            function_calls: self.function_calls.read().clone(),
            version_history: self.version_history.read().clone(),
            last_reload: *self.last_reload.read(),
        }
    }
}
