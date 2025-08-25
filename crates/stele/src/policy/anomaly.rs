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



use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct AnomalyFlag {
    pub predicate: String,
    pub subject_key: Option<String>,
    pub window_secs: u64,
    pub count: usize,
    pub baseline_avg: f64,
    pub ratio: f64,
}

#[derive(Default)]
struct Series {
    events: VecDeque<Instant>,
    baseline_avg: f64,
    samples: usize,
}

static STATE: Lazy<Mutex<HashMap<String, Series>>> = Lazy::new(|| Mutex::new(HashMap::new()));

fn key(predicate: &str, subject: Option<&str>) -> String {
    format!("{}::{}", predicate, subject.unwrap_or("*"))
}

pub struct AnomalyConfig {
    pub window: Duration,
    pub min_events: usize,
    pub burst_ratio: f64,
}
impl Default for AnomalyConfig {
    fn default() -> Self {
        Self {
            window: Duration::from_secs(30),
            min_events: 5,
            burst_ratio: 4.0,
        }
    }
}


pub fn record_fact(
    predicate: &str,
    subject_key: Option<&str>,
    cfg: &AnomalyConfig,
) -> Option<AnomalyFlag> {
    let mut guard = STATE.lock();
    let k = key(predicate, subject_key);
    let entry = guard.entry(k.clone()).or_default();
    let now = Instant::now();
    entry.events.push_back(now);
    
    while let Some(front) = entry.events.front() {
        if now.duration_since(*front) > cfg.window {
            entry.events.pop_front();
        } else {
            break;
        }
    }
    let count = entry.events.len();
    
    let prev_baseline = entry.baseline_avg;
    if count >= cfg.min_events && prev_baseline > 0.0 {
        let ratio = (count as f64) / prev_baseline.max(1.0);
        if ratio >= cfg.burst_ratio {
            
            entry.samples += 1;
            let beta = 0.2; 
            entry.baseline_avg = if entry.samples == 1 {
                count as f64
            } else {
                prev_baseline * (1.0 - beta) + (count as f64) * beta
            };
            return Some(AnomalyFlag {
                predicate: predicate.to_string(),
                subject_key: subject_key.map(|s| s.to_string()),
                window_secs: cfg.window.as_secs(),
                count,
                baseline_avg: entry.baseline_avg,
                ratio,
            });
        }
    }
    
    entry.samples += 1;
    let beta = 0.2; 
    entry.baseline_avg = if entry.samples == 1 {
        count as f64
    } else {
        prev_baseline * (1.0 - beta) + (count as f64) * beta
    };
    None
}


pub fn reset() {
    STATE.lock().clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_burst() {
        reset();
        let cfg = AnomalyConfig {
            window: Duration::from_millis(100),
            min_events: 3,
            burst_ratio: 2.0,
        };
        assert!(record_fact("EMPLOYS", Some("orgA"), &cfg).is_none());
        assert!(record_fact("EMPLOYS", Some("orgA"), &cfg).is_none());
        
        let flag = record_fact("EMPLOYS", Some("orgA"), &cfg);
        assert!(flag.is_some());
    }
}
