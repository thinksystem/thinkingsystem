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
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SecurityConfig {
    pub scoring: ScoringConfigSection,
    pub thresholds: ThresholdConfigSection,
    pub distribution: DistributionConfigSection,
    pub paths: PathConfigSection,
    pub llm: LlmConfigSection,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScoringConfigSection {
    pub length_bonus_8: f64,
    pub length_bonus_16: f64,
    pub all_digits_bonus: f64,
    pub all_digits_len_bonus_10: f64,
    pub alphanumeric_bonus: f64,
    pub mixed_case_bonus: f64,
    pub at_symbol_bonus: f64,
    pub hyphen_bonus: f64,
    pub slash_bonus: f64,
    pub uuid_like_bonus: f64,
    pub api_key_like_bonus: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ThresholdConfigSection {
    pub scribes_review_percentile: f64,
    pub absolute_threshold_override: f64,
    pub min_history_for_percentile: usize,
    pub llm_grey_area_low: f64,
    pub llm_grey_area_high: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DistributionConfigSection {
    pub max_history_size: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PathConfigSection {
    pub state_dir: String,
    pub state_file: String,
    pub training_data_file: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LlmConfigSection {
    pub api_endpoint: String,
    pub model: String,
    pub timeout_seconds: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct ScoringConfig {
    pub length_bonus_8: f64,
    pub length_bonus_16: f64,
    pub all_digits_bonus: f64,
    pub all_digits_len_bonus_10: f64,
    pub alphanumeric_bonus: f64,
    pub mixed_case_bonus: f64,
    pub at_symbol_bonus: f64,
    pub hyphen_bonus: f64,
    pub slash_bonus: f64,
    pub uuid_like_bonus: f64,
    pub api_key_like_bonus: f64,

    pub scribes_review_percentile: f64,

    pub absolute_threshold_override: f64,

    pub min_history_for_percentile: usize,
}

impl SecurityConfig {
    pub fn load_from_file(config_path: &Path) -> Result<Self> {
        let content = fs::read_to_string(config_path)?;
        let config: SecurityConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn default_config_path() -> PathBuf {
        PathBuf::from("config/messaging/security_config.toml")
    }

    pub fn load_or_default() -> Self {
        let config_path = Self::default_config_path();
        Self::load_from_file(&config_path).unwrap_or_else(|_| Self::default())
    }

    pub fn to_scoring_config(&self) -> ScoringConfig {
        ScoringConfig {
            length_bonus_8: self.scoring.length_bonus_8,
            length_bonus_16: self.scoring.length_bonus_16,
            all_digits_bonus: self.scoring.all_digits_bonus,
            all_digits_len_bonus_10: self.scoring.all_digits_len_bonus_10,
            alphanumeric_bonus: self.scoring.alphanumeric_bonus,
            mixed_case_bonus: self.scoring.mixed_case_bonus,
            at_symbol_bonus: self.scoring.at_symbol_bonus,
            hyphen_bonus: self.scoring.hyphen_bonus,
            slash_bonus: self.scoring.slash_bonus,
            uuid_like_bonus: self.scoring.uuid_like_bonus,
            api_key_like_bonus: self.scoring.api_key_like_bonus,
            scribes_review_percentile: self.thresholds.scribes_review_percentile,
            absolute_threshold_override: self.thresholds.absolute_threshold_override,
            min_history_for_percentile: self.thresholds.min_history_for_percentile,
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            scoring: ScoringConfigSection {
                length_bonus_8: 0.1,
                length_bonus_16: 0.1,
                all_digits_bonus: 0.4,
                all_digits_len_bonus_10: 0.2,
                alphanumeric_bonus: 0.2,
                mixed_case_bonus: 0.1,
                at_symbol_bonus: 0.6,
                hyphen_bonus: 0.2,
                slash_bonus: 0.2,
                uuid_like_bonus: 0.4,
                api_key_like_bonus: 0.3,
            },
            thresholds: ThresholdConfigSection {
                scribes_review_percentile: 0.85,
                absolute_threshold_override: 0.75,
                min_history_for_percentile: 20,
                llm_grey_area_low: 0.40,
                llm_grey_area_high: 0.75,
            },
            distribution: DistributionConfigSection {
                max_history_size: 1000,
            },
            paths: PathConfigSection {
                state_dir: "src/messaging/state".to_string(),
                state_file: "score_distribution.yml".to_string(),
                training_data_file: "training_data.jsonl".to_string(),
            },
            llm: LlmConfigSection {
                api_endpoint: "http://localhost:11434/api/generate".to_string(),
                model: "llama3".to_string(),
                timeout_seconds: 30,
            },
        }
    }
}

impl Default for ScoringConfig {
    fn default() -> Self {
        let config = SecurityConfig::load_or_default();
        config.to_scoring_config()
    }
}
