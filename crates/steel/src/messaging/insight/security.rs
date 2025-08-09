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
use std::fs;
use std::path::{Path, PathBuf};

use crate::messaging::insight::analysis::{ContentAnalyser, ContentAnalysis};
use crate::messaging::insight::config::{ScoringConfig, SecurityConfig};
use crate::messaging::insight::distribution::ScoreDistribution;
use crate::messaging::insight::hybrid_analyser::{
    HybridConfig, HybridContentAnalyser, HybridContentAnalysis,
};
use crate::messaging::insight::ner_analysis::NerConfig;

#[derive(Debug, Clone)]
pub enum AnalysisMode {
    SyntacticOnly,

    Hybrid,
}

pub struct MessageSecurity {
    mode: AnalysisMode,
    syntactic_analyser: ContentAnalyser,
    hybrid_analyser: Option<HybridContentAnalyser>,
    pub distribution: ScoreDistribution,
    state_path: PathBuf,
}

impl MessageSecurity {
    pub fn new(state_dir: &Path) -> Self {
        let config = SecurityConfig::load_or_default();
        let distribution_path = state_dir.join(&config.paths.state_file);
        let distribution = ScoreDistribution::load_from_file(&distribution_path)
            .unwrap_or_else(|_| ScoreDistribution::default());

        Self {
            mode: AnalysisMode::SyntacticOnly,
            syntactic_analyser: ContentAnalyser::default(),
            hybrid_analyser: None,
            distribution,
            state_path: distribution_path,
        }
    }

    pub fn new_with_hybrid(
        state_dir: &Path,
        ner_config: Option<NerConfig>,
        hybrid_config: Option<HybridConfig>,
    ) -> Self {
        let config = SecurityConfig::load_or_default();
        let distribution_path = state_dir.join(&config.paths.state_file);
        let distribution = ScoreDistribution::load_from_file(&distribution_path)
            .unwrap_or_else(|_| ScoreDistribution::default());

        let hybrid_analyser = HybridContentAnalyser::new(
            config.to_scoring_config(),
            ner_config.unwrap_or_default(),
            hybrid_config.unwrap_or_default(),
        );

        Self {
            mode: AnalysisMode::Hybrid,
            syntactic_analyser: ContentAnalyser::default(),
            hybrid_analyser: Some(hybrid_analyser),
            distribution,
            state_path: distribution_path,
        }
    }

    pub fn new_with_config(config: ScoringConfig, state_dir: &Path) -> Self {
        let security_config = SecurityConfig::load_or_default();
        let distribution_path = state_dir.join(&security_config.paths.state_file);
        let distribution = ScoreDistribution::load_from_file(&distribution_path)
            .unwrap_or_else(|_| ScoreDistribution::default());
        Self {
            mode: AnalysisMode::SyntacticOnly,
            syntactic_analyser: ContentAnalyser::new(config),
            hybrid_analyser: None,
            distribution,
            state_path: distribution_path,
        }
    }

    pub fn new_with_state(
        config: ScoringConfig,
        distribution: ScoreDistribution,
        state_dir: &Path,
    ) -> Self {
        let security_config = SecurityConfig::load_or_default();
        Self {
            mode: AnalysisMode::SyntacticOnly,
            syntactic_analyser: ContentAnalyser::new(config),
            hybrid_analyser: None,
            distribution,
            state_path: state_dir.join(&security_config.paths.state_file),
        }
    }

    pub fn enable_hybrid_analysis(
        &mut self,
        ner_config: Option<NerConfig>,
        hybrid_config: Option<HybridConfig>,
    ) -> anyhow::Result<()> {
        let config = SecurityConfig::load_or_default();
        let mut hybrid_analyser = HybridContentAnalyser::new(
            config.to_scoring_config(),
            ner_config.unwrap_or_default(),
            hybrid_config.unwrap_or_default(),
        );

        hybrid_analyser.initialise_ner()?;

        self.mode = AnalysisMode::Hybrid;
        self.hybrid_analyser = Some(hybrid_analyser);
        Ok(())
    }

    pub fn disable_hybrid_analysis(&mut self) {
        self.mode = AnalysisMode::SyntacticOnly;
        self.hybrid_analyser = None;
    }

    pub fn assess_message_risk(&mut self, content: &str) -> MessageAnalysisResult {
        match &self.mode {
            AnalysisMode::SyntacticOnly => {
                let analysis = self
                    .syntactic_analyser
                    .analyse(content, &mut self.distribution);
                self.save_distribution_state();
                MessageAnalysisResult::Syntactic(analysis)
            }
            AnalysisMode::Hybrid => {
                if let Some(ref mut hybrid_analyser) = self.hybrid_analyser {
                    match hybrid_analyser.analyse_hybrid(content, &mut self.distribution) {
                        Ok(analysis) => {
                            self.save_distribution_state();
                            MessageAnalysisResult::Hybrid(analysis)
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Hybrid analysis failed, falling back to syntactic: {}",
                                e
                            );
                            let analysis = self
                                .syntactic_analyser
                                .analyse(content, &mut self.distribution);
                            self.save_distribution_state();
                            MessageAnalysisResult::Syntactic(analysis)
                        }
                    }
                } else {
                    let analysis = self
                        .syntactic_analyser
                        .analyse(content, &mut self.distribution);
                    self.save_distribution_state();
                    MessageAnalysisResult::Syntactic(analysis)
                }
            }
        }
    }

    pub fn requires_scribes_processing(&mut self, content: &str) -> bool {
        match self.assess_message_risk(content) {
            MessageAnalysisResult::Syntactic(analysis) => analysis.requires_scribes_review,
            MessageAnalysisResult::Hybrid(analysis) => analysis.requires_scribes_review,
        }
    }

    fn save_distribution_state(&self) {
        if let Err(e) = self.distribution.save_to_file(&self.state_path) {
            eprintln!("Failed to save score distribution: {e}");
        }
    }

    pub fn get_current_review_threshold(&mut self) -> f64 {
        match &self.mode {
            AnalysisMode::SyntacticOnly => self
                .distribution
                .get_percentile_threshold(self.syntactic_analyser.get_scribes_review_percentile())
                .unwrap_or(self.syntactic_analyser.get_absolute_threshold_override()),
            AnalysisMode::Hybrid => {
                if let Some(ref hybrid_analyser) = self.hybrid_analyser {
                    hybrid_analyser.get_hybrid_config().min_combined_threshold
                } else {
                    self.distribution
                        .get_percentile_threshold(
                            self.syntactic_analyser.get_scribes_review_percentile(),
                        )
                        .unwrap_or(self.syntactic_analyser.get_absolute_threshold_override())
                }
            }
        }
    }

    pub fn set_percentile_threshold(&mut self, percentile: f64) {
        self.syntactic_analyser
            .set_scribes_review_percentile(percentile);
    }

    pub fn set_absolute_threshold(&mut self, threshold: f64) {
        self.syntactic_analyser
            .set_absolute_threshold_override(threshold);
    }

    pub fn get_distribution_stats(&self) -> Option<(f64, f64, f64)> {
        self.distribution.get_stats()
    }

    pub fn get_analysis_mode(&self) -> &AnalysisMode {
        &self.mode
    }

    pub fn is_hybrid_ready(&self) -> bool {
        match &self.hybrid_analyser {
            Some(analyser) => analyser.is_ner_ready(),
            None => false,
        }
    }

    pub fn update_hybrid_config(&mut self, config: HybridConfig) -> Result<(), &'static str> {
        match &mut self.hybrid_analyser {
            Some(analyser) => {
                analyser.update_hybrid_config(config);
                Ok(())
            }
            None => Err("Hybrid analyser not available"),
        }
    }

    pub fn update_ner_config(&mut self, config: NerConfig) -> Result<(), &'static str> {
        match &mut self.hybrid_analyser {
            Some(analyser) => {
                analyser.update_ner_config(config);
                Ok(())
            }
            None => Err("Hybrid analyser not available"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageAnalysisResult {
    Syntactic(ContentAnalysis),
    Hybrid(HybridContentAnalysis),
}

impl MessageAnalysisResult {
    pub fn overall_risk_score(&self) -> f64 {
        match self {
            MessageAnalysisResult::Syntactic(analysis) => analysis.overall_risk_score,
            MessageAnalysisResult::Hybrid(analysis) => analysis.combined_risk_score,
        }
    }

    pub fn requires_scribes_review(&self) -> bool {
        match self {
            MessageAnalysisResult::Syntactic(analysis) => analysis.requires_scribes_review,
            MessageAnalysisResult::Hybrid(analysis) => analysis.requires_scribes_review,
        }
    }

    pub fn get_interesting_items(&self) -> Vec<String> {
        match self {
            MessageAnalysisResult::Syntactic(analysis) => analysis
                .interesting_tokens
                .iter()
                .map(|(token, _)| token.clone())
                .collect(),
            MessageAnalysisResult::Hybrid(analysis) => {
                let mut items = Vec::new();

                for (token, _) in &analysis.syntactic_analysis.interesting_tokens {
                    items.push(format!("Syntactic: {token}"));
                }

                for entity in &analysis.ner_analysis.entities {
                    items.push(format!("NER {}: {}", entity.label, entity.text));
                }

                items
            }
        }
    }
}

impl Default for MessageSecurity {
    fn default() -> Self {
        let config = SecurityConfig::load_or_default();
        let state_dir = PathBuf::from(&config.paths.state_dir);
        fs::create_dir_all(&state_dir).expect("Failed to create state directory");
        Self::new(&state_dir)
    }
}
