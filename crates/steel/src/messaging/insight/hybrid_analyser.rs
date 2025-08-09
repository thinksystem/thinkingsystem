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

use crate::messaging::insight::{
    analysis::{ContentAnalyser, ContentAnalysis},
    config::{ScoringConfig, SecurityConfig},
    distribution::ScoreDistribution,
    ner_analysis::{NerAnalyser, NerAnalysisResult, NerConfig},
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HybridContentAnalysis {
    pub syntactic_analysis: ContentAnalysis,

    pub ner_analysis: NerAnalysisResult,

    pub combined_risk_score: f64,

    pub requires_scribes_review: bool,

    pub analysis_method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridConfig {
    pub syntactic_weight: f64,
    pub ner_weight: f64,
    pub ner_boost_threshold: f64,
    pub min_combined_threshold: f64,
    pub enable_ner_boost: bool,
}

impl Default for HybridConfig {
    fn default() -> Self {
        Self {
            syntactic_weight: 0.6,
            ner_weight: 0.4,
            ner_boost_threshold: 0.8,
            min_combined_threshold: 0.5,
            enable_ner_boost: true,
        }
    }
}

pub struct HybridContentAnalyser {
    syntactic_analyser: ContentAnalyser,
    ner_analyser: NerAnalyser,
    hybrid_config: HybridConfig,
}

impl HybridContentAnalyser {
    pub fn new(
        scoring_config: ScoringConfig,
        ner_config: NerConfig,
        hybrid_config: HybridConfig,
    ) -> Self {
        Self {
            syntactic_analyser: ContentAnalyser::new(scoring_config),
            ner_analyser: NerAnalyser::new(ner_config),
            hybrid_config,
        }
    }

    pub fn from_security_config(security_config: &SecurityConfig) -> Self {
        Self::new(
            security_config.to_scoring_config(),
            NerConfig::default(),
            HybridConfig::default(),
        )
    }

    pub fn analyse_hybrid(
        &mut self,
        text: &str,
        distribution: &mut ScoreDistribution,
    ) -> Result<HybridContentAnalysis> {
        let syntactic_analysis = self.syntactic_analyser.analyse(text, distribution);

        let ner_analysis = self.ner_analyser.analyse_text(text)?;

        let (combined_risk_score, requires_scribes_review, analysis_method) =
            self.combine_analysis_results(&syntactic_analysis, &ner_analysis);

        Ok(HybridContentAnalysis {
            syntactic_analysis,
            ner_analysis,
            combined_risk_score,
            requires_scribes_review,
            analysis_method,
        })
    }

    fn combine_analysis_results(
        &self,
        syntactic: &ContentAnalysis,
        ner: &NerAnalysisResult,
    ) -> (f64, bool, String) {
        let syntactic_score = syntactic.overall_risk_score;
        let ner_score = ner.overall_ner_score;

        let base_combined_score = (syntactic_score * self.hybrid_config.syntactic_weight)
            + (ner_score * self.hybrid_config.ner_weight);

        let (final_score, method) = if self.hybrid_config.enable_ner_boost
            && ner_score >= self.hybrid_config.ner_boost_threshold
        {
            let boosted_score = (base_combined_score + ner_score * 0.3).min(1.0);
            (
                boosted_score,
                format!("Hybrid (NER-boosted: {ner_score:.3})"),
            )
        } else {
            (base_combined_score, "Hybrid (weighted)".to_string())
        };

        let needs_review = final_score >= self.hybrid_config.min_combined_threshold
            || syntactic.requires_scribes_review
            || self.has_high_risk_ner_entities(ner);

        (final_score, needs_review, method)
    }

    fn has_high_risk_ner_entities(&self, ner: &NerAnalysisResult) -> bool {
        ner.entities.iter().any(|entity| {
            matches!(
                entity.label.as_str(),
                "credit_card" | "ssn" | "email" | "phone"
            ) && entity.risk_score >= 0.7
        })
    }

    pub fn get_prioritised_entities(
        &self,
        analysis: &HybridContentAnalysis,
    ) -> Vec<EntityPriority> {
        let mut entities = Vec::new();

        for (token, score) in &analysis.syntactic_analysis.interesting_tokens {
            entities.push(EntityPriority {
                text: token.clone(),
                entity_type: "syntactic_pattern".to_string(),
                risk_score: *score,
                source: "Syntactic".to_string(),
                start: None,
                end: None,
                confidence: None,
            });
        }

        for entity in &analysis.ner_analysis.entities {
            entities.push(EntityPriority {
                text: entity.text.clone(),
                entity_type: entity.label.clone(),
                risk_score: entity.risk_score,
                source: "NER".to_string(),
                start: Some(entity.start),
                end: Some(entity.end),
                confidence: Some(entity.confidence),
            });
        }

        entities.sort_by(|a, b| b.risk_score.partial_cmp(&a.risk_score).unwrap());
        entities
    }

    pub fn update_hybrid_config(&mut self, config: HybridConfig) {
        self.hybrid_config = config;
    }

    pub fn update_ner_config(&mut self, config: NerConfig) {
        self.ner_analyser.update_config(config);
    }

    pub fn is_ner_ready(&self) -> bool {
        self.ner_analyser.is_ready()
    }

    pub fn initialise_ner(&mut self) -> Result<()> {
        self.ner_analyser.initialise_model()
    }

    pub fn get_hybrid_config(&self) -> &HybridConfig {
        &self.hybrid_config
    }

    pub fn get_ner_config(&self) -> &NerConfig {
        self.ner_analyser.get_config()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityPriority {
    pub text: String,
    pub entity_type: String,
    pub risk_score: f64,
    pub source: String,
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub confidence: Option<f64>,
}

impl Default for HybridContentAnalyser {
    fn default() -> Self {
        Self::new(
            ScoringConfig::default(),
            NerConfig::default(),
            HybridConfig::default(),
        )
    }
}

impl HybridContentAnalyser {
    #[cfg(test)]
    pub fn combine_analysis_results_test(
        &self,
        syntactic: &ContentAnalysis,
        ner: &NerAnalysisResult,
    ) -> (f64, bool, String) {
        self.combine_analysis_results(syntactic, ner)
    }

    #[cfg(test)]
    pub fn has_high_risk_ner_entities_test(&self, ner: &NerAnalysisResult) -> bool {
        self.has_high_risk_ner_entities(ner)
    }
}
