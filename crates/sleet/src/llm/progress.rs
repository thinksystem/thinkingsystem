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
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressConfig {
    pub fast_ema_alpha: f64,

    pub slow_ema_alpha: f64,

    pub momentum_threshold: f64,

    pub plateau_threshold: u8,

    pub min_history_for_plateau: usize,

    pub max_history_size: usize,
}

impl Default for ProgressConfig {
    fn default() -> Self {
        Self {
            fast_ema_alpha: 0.3,
            slow_ema_alpha: 0.1,
            momentum_threshold: 0.01,
            plateau_threshold: 3,
            min_history_for_plateau: 5,
            max_history_size: 50,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEntry {
    pub iteration: u32,
    pub score: u8,
    pub momentum: f64,
    pub timestamp: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ProgressTracker {
    config: ProgressConfig,
    ema_fast: f64,
    ema_slow: f64,
    momentum: f64,
    plateau_count: u8,
    history: VecDeque<ProgressEntry>,
}

impl ProgressTracker {
    pub fn new() -> Self {
        Self::with_config(ProgressConfig::default())
    }

    pub fn with_config(config: ProgressConfig) -> Self {
        Self {
            config,
            ema_fast: 0.0,
            ema_slow: 0.0,
            momentum: 0.0,
            plateau_count: 0,
            history: VecDeque::new(),
        }
    }

    pub fn update(&mut self, iteration: u32, progress_score: u8) {
        self.update_with_metadata(iteration, progress_score, None);
    }

    pub fn update_with_metadata(
        &mut self,
        iteration: u32,
        progress_score: u8,
        metadata: Option<serde_json::Value>,
    ) {
        let score_f64 = progress_score as f64;

        if self.history.is_empty() {
            self.ema_fast = score_f64;
            self.ema_slow = score_f64;
        } else {
            self.ema_fast = self.config.fast_ema_alpha * score_f64
                + (1.0 - self.config.fast_ema_alpha) * self.ema_fast;
            self.ema_slow = self.config.slow_ema_alpha * score_f64
                + (1.0 - self.config.slow_ema_alpha) * self.ema_slow;
        }

        self.momentum = self.ema_fast - self.ema_slow;

        if self.momentum.abs() < self.config.momentum_threshold {
            self.plateau_count = self.plateau_count.saturating_add(1);
        } else {
            self.plateau_count = 0;
        }

        let entry = ProgressEntry {
            iteration,
            score: progress_score,
            momentum: self.momentum,
            timestamp: Utc::now(),
            metadata,
        };

        self.history.push_back(entry);

        while self.history.len() > self.config.max_history_size {
            self.history.pop_front();
        }

        debug!(
            iteration = iteration,
            score = progress_score,
            momentum = self.momentum,
            plateau_count = self.plateau_count,
            "Progress updated"
        );
    }

    pub fn needs_strategy_change(&self) -> bool {
        self.plateau_count >= self.config.plateau_threshold
            && self.history.len() >= self.config.min_history_for_plateau
    }

    pub fn get_momentum(&self) -> f64 {
        self.momentum
    }

    pub fn get_plateau_count(&self) -> u8 {
        self.plateau_count
    }

    pub fn get_fast_ema(&self) -> f64 {
        self.ema_fast
    }

    pub fn get_slow_ema(&self) -> f64 {
        self.ema_slow
    }

    pub fn get_history(&self) -> &VecDeque<ProgressEntry> {
        &self.history
    }

    pub fn get_latest(&self) -> Option<&ProgressEntry> {
        self.history.back()
    }

    pub fn get_recent(&self, n: usize) -> Vec<&ProgressEntry> {
        self.history.iter().rev().take(n).collect()
    }

    pub fn get_recent_average(&self, n: usize) -> Option<f64> {
        if self.history.is_empty() {
            return None;
        }

        let recent_entries: Vec<_> = self.history.iter().rev().take(n).collect();
        if recent_entries.is_empty() {
            return None;
        }

        let sum: u32 = recent_entries.iter().map(|e| e.score as u32).sum();
        Some(sum as f64 / recent_entries.len() as f64)
    }

    pub fn is_improving(&self, lookback: usize) -> bool {
        if self.history.len() < 2 {
            return false;
        }

        let recent: Vec<_> = self.history.iter().rev().take(lookback).collect();
        if recent.len() < 2 {
            return false;
        }

        let first_score = recent.last().unwrap().score;
        let last_score = recent.first().unwrap().score;

        last_score > first_score && self.momentum > 0.0
    }

    pub fn is_declining(&self, lookback: usize) -> bool {
        if self.history.len() < 2 {
            return false;
        }

        let recent: Vec<_> = self.history.iter().rev().take(lookback).collect();
        if recent.len() < 2 {
            return false;
        }

        let first_score = recent.last().unwrap().score;
        let last_score = recent.first().unwrap().score;

        last_score < first_score && self.momentum < -self.config.momentum_threshold
    }

    pub fn get_status_summary(&self) -> String {
        if self.history.is_empty() {
            return "No progress data available".to_string();
        }

        let latest = self.get_latest().unwrap();
        let momentum_desc = if self.momentum.abs() < self.config.momentum_threshold {
            "stagnant"
        } else if self.momentum > 0.0 {
            "improving"
        } else {
            "declining"
        };

        let plateau_status = if self.needs_strategy_change() {
            " (PLATEAU DETECTED - strategy change recommended)"
        } else {
            ""
        };

        format!(
            "Iteration {}: Score {}/10, Momentum {:.2} ({}), Trend: {}{}",
            latest.iteration,
            latest.score,
            self.momentum,
            momentum_desc,
            if self.is_improving(3) {
                "improving"
            } else if self.is_declining(3) {
                "declining"
            } else {
                "stable"
            },
            plateau_status
        )
    }

    pub fn reset(&mut self) {
        self.ema_fast = 0.0;
        self.ema_slow = 0.0;
        self.momentum = 0.0;
        self.plateau_count = 0;
        self.history.clear();
        debug!("Progress tracker reset");
    }

    pub fn update_config(&mut self, config: ProgressConfig) {
        self.config = config;
        debug!("Progress tracker configuration updated");
    }

    pub fn get_config(&self) -> &ProgressConfig {
        &self.config
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}

pub mod analysis {
    use super::*;

    pub fn analyse_progress_pattern(tracker: &ProgressTracker) -> ProgressAnalysis {
        let history = tracker.get_history();

        if history.len() < 3 {
            return ProgressAnalysis {
                pattern: ProgressPattern::Insufficient,
                recommendation: "Need more data points for analysis".to_string(),
                confidence: 0.0,
            };
        }

        let recent_avg = tracker.get_recent_average(3).unwrap_or(0.0);
        let overall_avg =
            history.iter().map(|e| e.score as f64).sum::<f64>() / history.len() as f64;

        let pattern = if tracker.needs_strategy_change() {
            ProgressPattern::Plateau
        } else if tracker.is_improving(5) {
            ProgressPattern::Improving
        } else if tracker.is_declining(5) {
            ProgressPattern::Declining
        } else if recent_avg > overall_avg + 1.0 {
            ProgressPattern::RecentImprovement
        } else {
            ProgressPattern::Stable
        };

        let (recommendation, confidence) = match pattern {
            ProgressPattern::Plateau => (
                "Consider changing strategy, approach, or introducing new perspectives".to_string(),
                0.9
            ),
            ProgressPattern::Improving => (
                "Current approach is working well, continue with minor optimisations".to_string(),
                0.8
            ),
            ProgressPattern::Declining => (
                "Review recent changes and consider reverting or adjusting approach".to_string(),
                0.8
            ),
            ProgressPattern::RecentImprovement => (
                "Recent improvements show promise, monitor closely and reinforce successful elements".to_string(),
                0.7
            ),
            ProgressPattern::Stable => (
                "Progress is steady, consider small experiments to find improvement opportunities".to_string(),
                0.6
            ),
            ProgressPattern::Insufficient => (
                "Insufficient data".to_string(),
                0.0
            ),
        };

        ProgressAnalysis {
            pattern,
            recommendation,
            confidence,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProgressPattern {
    Improving,
    Declining,
    Plateau,
    Stable,
    RecentImprovement,
    Insufficient,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressAnalysis {
    pub pattern: ProgressPattern,
    pub recommendation: String,
    pub confidence: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_tracker_basic() {
        let mut tracker = ProgressTracker::new();

        tracker.update(1, 5);
        assert_eq!(tracker.get_latest().unwrap().score, 5);
        assert_eq!(tracker.get_momentum(), 0.0);

        tracker.update(2, 7);
        assert!(tracker.get_momentum() > 0.0);
    }

    #[test]
    fn test_plateau_detection() {
        let config = ProgressConfig {
            plateau_threshold: 2,
            momentum_threshold: 0.1,
            min_history_for_plateau: 3,
            ..Default::default()
        };

        let mut tracker = ProgressTracker::with_config(config);
        tracker.update(1, 5);
        tracker.update(2, 5);
        tracker.update(3, 5);

        assert!(tracker.needs_strategy_change());
    }

    #[test]
    fn test_improving_trend() {
        let mut tracker = ProgressTracker::new();

        tracker.update(1, 3);
        tracker.update(2, 5);
        tracker.update(3, 7);
        tracker.update(4, 8);

        assert!(tracker.is_improving(4));
        assert!(tracker.get_momentum() > 0.0);
    }

    #[test]
    fn test_declining_trend() {
        let mut tracker = ProgressTracker::new();

        tracker.update(1, 8);
        tracker.update(2, 6);
        tracker.update(3, 4);
        tracker.update(4, 3);

        assert!(tracker.is_declining(4));
        assert!(tracker.get_momentum() < 0.0);
    }

    #[test]
    fn test_recent_average() {
        let mut tracker = ProgressTracker::new();

        tracker.update(1, 2);
        tracker.update(2, 4);
        tracker.update(3, 6);

        assert_eq!(tracker.get_recent_average(2).unwrap(), 5.0);
        assert_eq!(tracker.get_recent_average(3).unwrap(), 4.0);
    }
}
