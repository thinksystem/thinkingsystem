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

use serde::{Deserialize, Serialize};

use crate::messaging::insight::config::ScoringConfig;
use crate::messaging::insight::distribution::ScoreDistribution;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ContentAnalysis {
    pub overall_risk_score: f64,
    pub interesting_tokens: Vec<(String, f64)>,
    pub requires_scribes_review: bool,
}

pub struct ContentAnalyser {
    config: ScoringConfig,
}

impl ContentAnalyser {
    pub fn new(config: ScoringConfig) -> Self {
        Self { config }
    }

    pub fn analyse(&self, text: &str, distribution: &mut ScoreDistribution) -> ContentAnalysis {
        let tokens = text.split_whitespace();
        let mut interesting_tokens = Vec::new();
        let mut max_score = 0.0;

        for token_str in tokens {
            let token = token_str.trim_matches(|c: char| !c.is_alphanumeric());
            if token.is_empty() {
                continue;
            }

            let score = self.score_token(token);
            if score > 0.3 {
                interesting_tokens.push((token.to_string(), score));
            }
            if score > max_score {
                max_score = score;
            }
        }

        interesting_tokens.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        distribution.add_score(max_score);

        let requires_review = self.requires_scribes_review(max_score, distribution);

        ContentAnalysis {
            overall_risk_score: max_score,
            interesting_tokens,
            requires_scribes_review: requires_review,
        }
    }

    fn requires_scribes_review(&self, score: f64, distribution: &mut ScoreDistribution) -> bool {
        if score >= self.config.absolute_threshold_override {
            return true;
        }

        if distribution.score_history.len() < self.config.min_history_for_percentile {
            return false;
        }

        if let Some(percentile_threshold) =
            distribution.get_percentile_threshold(self.config.scribes_review_percentile)
        {
            score >= percentile_threshold
        } else {
            score >= self.config.absolute_threshold_override
        }
    }

    fn score_token(&self, token: &str) -> f64 {
        let len = token.len();
        if len < 4 {
            return 0.0;
        }

        let mut score: f64 = 0.0;
        let has_digits = token.chars().any(|c| c.is_ascii_digit());
        let has_letters = token.chars().any(|c| c.is_ascii_alphabetic());
        let has_uppercase = token.chars().any(|c| c.is_ascii_uppercase());
        let has_lowercase = token.chars().any(|c| c.is_ascii_lowercase());
        let has_hyphen = token.contains('-');
        let has_at_symbol = token.contains('@');
        let has_slash = token.contains('/');

        if len > 8 {
            score += self.config.length_bonus_8;
        }
        if len > 16 {
            score += self.config.length_bonus_16;
        }

        if has_digits && !has_letters {
            score += self.config.all_digits_bonus;
            if len > 10 {
                score += self.config.all_digits_len_bonus_10;
            }
        } else if has_digits && has_letters {
            score += self.config.alphanumeric_bonus;
            if has_uppercase && has_lowercase {
                score += self.config.mixed_case_bonus;
            }
        }

        if has_at_symbol {
            score += self.config.at_symbol_bonus;
        }
        if has_hyphen && has_digits {
            score += self.config.hyphen_bonus;
        }
        if has_slash && has_digits {
            score += self.config.slash_bonus;
        }

        if len > 30 && has_hyphen {
            score += self.config.uuid_like_bonus;
        }
        if len > 6
            && token
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            score += self.config.api_key_like_bonus;
        }

        score.min(1.0)
    }

    #[cfg(test)]
    pub fn score_token_test(&self, token: &str) -> f64 {
        self.score_token(token)
    }

    #[cfg(test)]
    pub fn requires_scribes_review_test(
        &self,
        score: f64,
        distribution: &mut ScoreDistribution,
    ) -> bool {
        self.requires_scribes_review(score, distribution)
    }

    pub fn get_scribes_review_percentile(&self) -> f64 {
        self.config.scribes_review_percentile
    }

    pub fn set_scribes_review_percentile(&mut self, percentile: f64) {
        self.config.scribes_review_percentile = percentile.clamp(0.0, 1.0);
    }

    pub fn get_absolute_threshold_override(&self) -> f64 {
        self.config.absolute_threshold_override
    }

    pub fn set_absolute_threshold_override(&mut self, threshold: f64) {
        self.config.absolute_threshold_override = threshold.clamp(0.0, 1.0);
    }
}

impl Default for ContentAnalyser {
    fn default() -> Self {
        Self::new(ScoringConfig::default())
    }
}
