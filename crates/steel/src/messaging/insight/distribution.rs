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

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use crate::messaging::insight::config::SecurityConfig;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScoreDistribution {
    pub score_history: Vec<f64>,

    pub max_history_size: usize,

    #[serde(skip)]
    pub cached_percentiles: std::collections::HashMap<String, f64>,
}

impl ScoreDistribution {
    pub fn new(max_history_size: usize) -> Self {
        Self {
            score_history: Vec::new(),
            max_history_size,
            cached_percentiles: std::collections::HashMap::new(),
        }
    }

    pub fn add_score(&mut self, score: f64) {
        let insert_pos = self
            .score_history
            .binary_search_by(|probe| probe.partial_cmp(&score).unwrap())
            .unwrap_or_else(|e| e);
        self.score_history.insert(insert_pos, score);

        if self.score_history.len() > self.max_history_size {
            self.score_history.remove(0);
        }

        self.cached_percentiles.clear();
    }

    pub fn get_percentile_threshold(&mut self, percentile: f64) -> Option<f64> {
        if self.score_history.is_empty() {
            return None;
        }

        let cache_key = format!("{percentile:.3}");
        if let Some(&cached_value) = self.cached_percentiles.get(&cache_key) {
            return Some(cached_value);
        }

        let index = ((self.score_history.len() as f64 - 1.0) * percentile).floor() as usize;
        let threshold = self.score_history[index.min(self.score_history.len() - 1)];

        self.cached_percentiles.insert(cache_key, threshold);
        Some(threshold)
    }

    pub fn get_stats(&self) -> Option<(f64, f64, f64)> {
        if self.score_history.is_empty() {
            return None;
        }

        let min = *self.score_history.first().unwrap();
        let max = *self.score_history.last().unwrap();
        let mean = self.score_history.iter().sum::<f64>() / self.score_history.len() as f64;

        Some((min, max, mean))
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let yaml_data = serde_yaml::to_string(self)?;
        let mut file = File::create(path)?;
        file.write_all(yaml_data.as_bytes())?;
        Ok(())
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let mut yaml_data = String::new();
        file.read_to_string(&mut yaml_data)?;
        let distribution: ScoreDistribution = serde_yaml::from_str(&yaml_data)?;
        Ok(distribution)
    }
}

impl Default for ScoreDistribution {
    fn default() -> Self {
        let config = SecurityConfig::load_or_default();
        Self::new(config.distribution.max_history_size)
    }
}
