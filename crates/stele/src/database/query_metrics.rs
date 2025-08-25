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
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, Default)]
pub struct Stat {
    pub count: u64,
    pub total_ms: u128,
    pub last_ms: u128,
}


pub type QueryMetricEntry = (String, Stat);
pub type QueryMetricsSnapshot = (Vec<QueryMetricEntry>, Vec<QueryMetricEntry>);

#[derive(Default)]
struct Registry {
    map: Mutex<HashMap<String, Stat>>,
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

fn registry() -> &'static Registry {
    REGISTRY.get_or_init(|| Registry { map: Mutex::new(HashMap::new()) })
}

pub fn record_query(label: &str, duration_ms: u128) {
    let reg = registry();
    let mut guard = reg.map.lock().expect("metrics mutex poisoned");
    let e = guard.entry(label.to_string()).or_default();
    e.count += 1;
    e.total_ms += duration_ms;
    e.last_ms = duration_ms;
}

pub fn snapshot_top_by_total(n: usize) -> Vec<QueryMetricEntry> {
    let reg = registry();
    let guard = reg.map.lock().expect("metrics mutex poisoned");
    let mut items: Vec<QueryMetricEntry> = guard.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    items.sort_by(|a, b| b.1.total_ms.cmp(&a.1.total_ms));
    items.truncate(n);
    items
}

pub fn snapshot_top_by_count(n: usize) -> Vec<QueryMetricEntry> {
    let reg = registry();
    let guard = reg.map.lock().expect("metrics mutex poisoned");
    let mut items: Vec<QueryMetricEntry> = guard.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    items.sort_by(|a, b| match b.1.count.cmp(&a.1.count) { Ordering::Equal => b.1.total_ms.cmp(&a.1.total_ms), other => other });
    items.truncate(n);
    items
}
