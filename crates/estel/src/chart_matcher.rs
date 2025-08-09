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

use crate::api_graph::{ApiGraph, ArgSpec, ChartNode, DataType};
use crate::data_profiler::DimensionProfile;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use thiserror::Error;
#[derive(Error, Debug)]
pub enum MatchingError {
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}
#[derive(Debug, Clone)]
pub struct MatchingConfig {
    pub min_quality_score: f64,
    pub max_suggestions_per_chart: usize,
    pub include_partial_matches: bool,
    pub prefer_high_dimensionality: bool,
}
impl Default for MatchingConfig {
    fn default() -> Self {
        Self {
            min_quality_score: 0.3,
            max_suggestions_per_chart: 10,
            include_partial_matches: true,
            prefer_high_dimensionality: false,
        }
    }
}
impl MatchingConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(0.0..=1.0).contains(&self.min_quality_score) {
            return Err("min_quality_score must be between 0.0 and 1.0".to_string());
        }
        if self.max_suggestions_per_chart == 0 {
            return Err("max_suggestions_per_chart must be greater than 0".to_string());
        }
        if self.max_suggestions_per_chart > 100 {
            return Err(
                "max_suggestions_per_chart should not exceed 100 for performance reasons"
                    .to_string(),
            );
        }
        Ok(())
    }
    pub fn for_performance() -> Self {
        Self {
            min_quality_score: 0.5,
            max_suggestions_per_chart: 5,
            include_partial_matches: false,
            ..Default::default()
        }
    }
    pub fn for_exploration() -> Self {
        Self {
            min_quality_score: 0.2,
            max_suggestions_per_chart: 15,
            include_partial_matches: true,
            prefer_high_dimensionality: true,
        }
    }
    pub fn for_presentation() -> Self {
        Self {
            min_quality_score: 0.6,
            max_suggestions_per_chart: 8,
            include_partial_matches: false,
            prefer_high_dimensionality: false,
        }
    }
}
#[derive(Debug, Clone)]
pub struct DomainHints {
    pub prefer_simple_charts: bool,
    pub strict_quality_threshold: bool,
    pub preferred_library: Option<String>,
    pub domain_context: Option<String>,
}
impl Default for DomainHints {
    fn default() -> Self {
        Self {
            prefer_simple_charts: false,
            strict_quality_threshold: false,
            preferred_library: Some("plotly".to_string()),
            domain_context: None,
        }
    }
}
#[derive(Debug, Clone)]
pub struct RenderSpec {
    pub chart_name: String,
    pub library: String,
    pub description: String,
    pub mappings: HashMap<String, String>,
    pub quality_score: f64,
    pub dimensions_used: usize,
    pub complete: bool,
    pub detailed_score: Option<ChartScore>,
}
#[derive(Debug, Clone)]
pub struct ChartScore {
    pub technical_feasibility: f64,
    pub semantic_appropriateness: f64,
    pub visual_effectiveness: f64,
    pub data_utilisation: f64,
    pub complexity_match: f64,
    pub overall_score: f64,
}
impl ChartScore {
    pub fn calculate_overall(&mut self, weights: &ScoringWeights) {
        self.overall_score = self.technical_feasibility * weights.technical_weight
            + self.semantic_appropriateness * weights.semantic_weight
            + self.visual_effectiveness * weights.visual_weight
            + self.data_utilisation * weights.utilisation_weight
            + self.complexity_match * weights.complexity_weight;
    }
}
#[derive(Debug, Clone)]
pub struct ScoringWeights {
    pub technical_weight: f64,
    pub semantic_weight: f64,
    pub visual_weight: f64,
    pub utilisation_weight: f64,
    pub complexity_weight: f64,
}
impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            technical_weight: 0.3,
            semantic_weight: 0.25,
            visual_weight: 0.25,
            utilisation_weight: 0.1,
            complexity_weight: 0.1,
        }
    }
}
#[derive(Debug, Clone)]
pub enum ScoringProfile {
    Exploratory,
    Presentation,
    Analytical,
    Balanced,
}
impl ScoringProfile {
    pub fn get_weights(&self) -> ScoringWeights {
        match self {
            ScoringProfile::Exploratory => ScoringWeights {
                technical_weight: 0.2,
                semantic_weight: 0.1,
                visual_weight: 0.2,
                utilisation_weight: 0.3,
                complexity_weight: 0.2,
            },
            ScoringProfile::Presentation => ScoringWeights {
                technical_weight: 0.3,
                semantic_weight: 0.2,
                visual_weight: 0.4,
                utilisation_weight: 0.05,
                complexity_weight: 0.05,
            },
            ScoringProfile::Analytical => ScoringWeights {
                technical_weight: 0.25,
                semantic_weight: 0.4,
                visual_weight: 0.25,
                utilisation_weight: 0.05,
                complexity_weight: 0.05,
            },
            ScoringProfile::Balanced => ScoringWeights::default(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct ChartExplanation {
    pub total_dimensions: usize,
    pub dimension_types: String,
    pub chart_explanations: Vec<SingleChartExplanation>,
}
#[derive(Debug, Clone)]
pub struct SingleChartExplanation {
    pub chart_name: String,
    pub can_render: bool,
    pub missing_requirements: Vec<String>,
    pub available_mappings: Vec<String>,
    pub quality_issues: Vec<String>,
}
mod internal {
    use super::*;
    pub mod scoring_weights {
        pub const FINAL_SCORE_UTILISATION_BONUS: f64 = 0.1;
        pub const FINAL_SCORE_COMPLETENESS_BONUS: f64 = 0.05;
        pub const MAPPING_QUALITY_SEMANTIC_BONUS_X_TEMPORAL: f64 = 0.2;
        pub const MAPPING_QUALITY_SEMANTIC_BONUS_Y_NUMERIC: f64 = 0.1;
        pub const MAPPING_QUALITY_SEMANTIC_BONUS_COLOR_CATEGORICAL: f64 = 0.15;
        pub const MAPPING_QUALITY_SEMANTIC_BONUS_SIZE_NUMERIC: f64 = 0.1;
        pub const CARDINALITY_PENALTY_COLOR: f64 = 0.3;
        pub const CARDINALITY_PENALTY_X_AXIS: f64 = 0.2;
        pub const SEMANTIC_PENALTY_SIMILAR_MEASURES: f64 = 0.5;
        pub const SEMANTIC_BONUS_COMPLEMENTARY_MEASURES: f64 = 0.2;
        pub const SEMANTIC_BONUS_TEMPORAL_LINE: f64 = 0.4;
        pub const SEMANTIC_BONUS_NUMERIC_SCATTER: f64 = 0.2;
        pub const HIGH_CARDINALITY_THRESHOLD: usize = 20;
        pub const PIE_CHART_MAX_CATEGORIES: usize = 8;
        pub const COLOR_MAX_CATEGORIES: usize = 10;
    }
    pub(super) struct DimensionIndex<'a> {
        by_type: HashMap<DataType, Vec<&'a DimensionProfile>>,
        by_name: HashMap<String, &'a DimensionProfile>,
        sorted_profiles: Vec<&'a DimensionProfile>,
    }
    impl<'a> DimensionIndex<'a> {
        pub fn new(profiles: &'a [DimensionProfile]) -> Self {
            let mut by_type: HashMap<DataType, Vec<&'a DimensionProfile>> = HashMap::new();
            let mut by_name = HashMap::new();
            let mut sorted_profiles: Vec<&'a DimensionProfile> = profiles.iter().collect();
            sorted_profiles.sort_unstable_by(|a, b| {
                b.quality_score
                    .partial_cmp(&a.quality_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            for p in profiles {
                by_name.insert(p.name.clone(), p);
                by_type.entry(p.data_type.clone()).or_default().push(p);
            }
            Self {
                by_type,
                by_name,
                sorted_profiles,
            }
        }
        pub fn get_by_type(&self, data_type: &DataType) -> &[&'a DimensionProfile] {
            self.by_type.get(data_type).map_or(&[], |v| v.as_slice())
        }
        pub fn get_by_name(&self, name: &str) -> Option<&&'a DimensionProfile> {
            self.by_name.get(name)
        }
    }
    struct SpecBuilder<'a, 'b> {
        chart: &'a ChartNode,
        dimension_index: &'a DimensionIndex<'a>,
        matcher: &'a ChartMatcher<'a>,
        detailed_score: Option<&'b ChartScore>,
        mappings: HashMap<String, String>,
        used_profiles: HashSet<String>,
    }
    impl<'a, 'b> SpecBuilder<'a, 'b> {
        fn new(
            chart: &'a ChartNode,
            index: &'a DimensionIndex<'a>,
            matcher: &'a ChartMatcher<'a>,
            score: Option<&'b ChartScore>,
        ) -> Self {
            Self {
                chart,
                dimension_index: index,
                matcher,
                detailed_score: score,
                mappings: HashMap::new(),
                used_profiles: HashSet::new(),
            }
        }
        fn try_map_required(&mut self) -> bool {
            let required_args = self.chart.required_args();
            for (arg_name, arg_spec) in &required_args {
                if let Some(profile) = self.find_best_match(arg_spec) {
                    self.mappings
                        .insert(arg_name.to_string(), profile.name.clone());
                    self.used_profiles.insert(profile.name.clone());
                } else {
                    return false;
                }
            }
            true
        }
        fn validate_mappings(&self) -> bool {
            for (arg_name, col_name) in &self.mappings {
                if let Some(profile) = self.dimension_index.get_by_name(col_name) {
                    if arg_name == "y"
                        && self.chart.name == "bar"
                        && !matches!(profile.data_type, DataType::Categorical)
                    {
                        return false;
                    }
                }
            }
            true
        }
        fn build(&self) -> Option<RenderSpec> {
            if !self.validate_mappings() {
                return None;
            }
            let spec = RenderSpec {
                chart_name: self.chart.name.clone(),
                library: self.chart.library.clone(),
                description: self.chart.description.clone(),
                quality_score: self.calculate_mapping_quality(),
                dimensions_used: self.mappings.len(),
                complete: self.mappings.len() >= self.chart.required_args().len(),
                mappings: self.mappings.clone(),
                detailed_score: self.detailed_score.cloned(),
            };
            Some(spec)
        }
        fn find_best_match(&self, arg_spec: &ArgSpec) -> Option<&'a DimensionProfile> {
            let mut compatible: Vec<_> = self
                .dimension_index
                .sorted_profiles
                .iter()
                .filter(|p| {
                    !self.used_profiles.contains(&p.name)
                        && arg_spec
                            .data_type
                            .accepted_types()
                            .contains(&(&p.data_type))
                })
                .cloned()
                .collect();
            if compatible.is_empty() {
                return None;
            }
            if (self.chart.name == "treemap" || self.chart.name == "sunburst")
                && arg_spec.data_type.accepts(&DataType::Numeric)
            {
                compatible.sort_by(|a, b| {
                    let a_score = self.calculate_hierarchical_suitability(a);
                    let b_score = self.calculate_hierarchical_suitability(b);
                    b_score
                        .partial_cmp(&a_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            compatible.first().cloned()
        }
        fn calculate_hierarchical_suitability(&self, profile: &DimensionProfile) -> f64 {
            let mut score = profile.quality_score;
            if let Some(stats) = &profile.numeric_stats {
                if let Some(std_dev) = stats.std {
                    if std_dev < 1e-6 {
                        score -= 0.8;
                    }
                }
                if let (Some(min), Some(max)) = (stats.min, stats.max) {
                    let range = max - min;
                    if range > 0.0 {
                        score += (range.ln() / 10.0).clamp(0.0, 0.2);
                    }
                }
            }
            if profile.name.to_lowercase().contains("year")
                || profile.name.to_lowercase().contains("date")
            {
                score -= 0.3;
            }
            if let Some(cardinality) = profile.cardinality {
                if cardinality < 5 {
                    score -= 0.4;
                }
            }
            score
        }
        fn calculate_mapping_quality(&self) -> f64 {
            if self.mappings.is_empty() {
                return 0.0;
            }

            let _matcher_config = &self.matcher;

            let total_quality: f64 =
                self.mappings
                    .iter()
                    .map(|(arg_name, col_name)| {
                        if let Some(profile) = self.dimension_index.get_by_name(col_name) {
                            let mut quality = profile.quality_score;
                            quality += match (arg_name.as_str(), &profile.data_type) {
                        ("x", DataType::Temporal) if self.chart.name.contains("line") =>
                            scoring_weights::MAPPING_QUALITY_SEMANTIC_BONUS_X_TEMPORAL,
                        ("y", DataType::Numeric) =>
                            scoring_weights::MAPPING_QUALITY_SEMANTIC_BONUS_Y_NUMERIC,
                        ("colour", DataType::Categorical) =>
                            scoring_weights::MAPPING_QUALITY_SEMANTIC_BONUS_COLOR_CATEGORICAL,
                        ("size", DataType::Numeric) =>
                            scoring_weights::MAPPING_QUALITY_SEMANTIC_BONUS_SIZE_NUMERIC,
                        _ => 0.0,
                    };
                            if let (DataType::Categorical, Some(card)) =
                                (&profile.data_type, profile.cardinality)
                            {
                                quality -= match arg_name.as_str() {
                                    "colour" if card > scoring_weights::COLOR_MAX_CATEGORIES => {
                                        scoring_weights::CARDINALITY_PENALTY_COLOR
                                    }
                                    "x" if card > scoring_weights::HIGH_CARDINALITY_THRESHOLD => {
                                        scoring_weights::CARDINALITY_PENALTY_X_AXIS
                                    }
                                    _ => 0.0,
                                };
                            }
                            quality.max(0.0)
                        } else {
                            0.0
                        }
                    })
                    .sum();
            (total_quality / self.mappings.len() as f64).clamp(0.0, 1.0)
        }
    }
    #[derive(Debug, Clone)]
    pub(super) struct ChartCandidate<'a> {
        pub chart: &'a ChartNode,
        pub score: f64,
        pub detailed_score: Option<ChartScore>,
    }
    #[derive(Debug, Clone)]
    pub(super) struct InternalConfig {
        pub min_quality_score: f64,
        pub stage1_threshold: f64,
        pub stage2_threshold: f64,
        pub stage1_max_candidates: usize,
        pub stage2_max_candidates: usize,
        pub final_max_results: usize,
        pub prefer_high_dimensionality: bool,
    }
    impl InternalConfig {
        pub fn new(config: &MatchingConfig, characteristics: &DatasetCharacteristics) -> Self {
            let complexity_factor = characteristics.complexity_score;
            Self {
                min_quality_score: config.min_quality_score,
                stage1_threshold: match complexity_factor {
                    x if x > 0.7 => 0.15,
                    x if x > 0.4 => 0.25,
                    _ => 0.35,
                },
                stage2_threshold: config.min_quality_score * 0.8,
                stage1_max_candidates: (config.max_suggestions_per_chart * 4).min(25),
                stage2_max_candidates: (config.max_suggestions_per_chart * 2).min(15),
                final_max_results: config.max_suggestions_per_chart,
                prefer_high_dimensionality: config.prefer_high_dimensionality,
            }
        }
    }
    #[derive(Debug, Clone)]
    pub(super) struct DatasetCharacteristics {
        pub dimensionality: usize,
        pub avg_cardinality: f64,
        pub complexity_score: f64,
        pub has_temporal: bool,
        pub numeric_count: usize,
        pub categorical_count: usize,
    }
    impl DatasetCharacteristics {
        pub fn from_profiles(profiles: &[DimensionProfile]) -> Self {
            let dimensionality = profiles.len();
            if dimensionality == 0 {
                return Self {
                    dimensionality: 0,
                    avg_cardinality: 0.0,
                    complexity_score: 0.0,
                    has_temporal: false,
                    numeric_count: 0,
                    categorical_count: 0,
                };
            }
            let mut total_cardinality = 0.0;
            let mut cardinality_count = 0;
            let mut type_counts: HashMap<DataType, usize> = HashMap::new();
            for p in profiles {
                if let Some(c) = p.cardinality {
                    total_cardinality += c as f64;
                    cardinality_count += 1;
                }
                *type_counts.entry(p.data_type.clone()).or_insert(0) += 1;
            }
            let avg_cardinality = if cardinality_count > 0 {
                total_cardinality / cardinality_count as f64
            } else {
                0.0
            };
            let numeric_count = *type_counts.get(&DataType::Numeric).unwrap_or(&0);
            let categorical_count = *type_counts.get(&DataType::Categorical).unwrap_or(&0);
            let temporal_count = *type_counts.get(&DataType::Temporal).unwrap_or(&0);
            let complexity_score = {
                let dim_factor = (dimensionality as f64 / 10.0).min(1.0);
                let card_factor = (avg_cardinality / 100.0).min(1.0);
                let type_diversity = type_counts.keys().len() as f64 / 3.0;
                (dim_factor * 0.4 + card_factor * 0.4 + type_diversity * 0.2).min(1.0)
            };
            Self {
                dimensionality,
                avg_cardinality,
                complexity_score,
                has_temporal: temporal_count > 0,
                numeric_count,
                categorical_count,
            }
        }
    }
    pub(super) struct ChartMatcher<'a> {
        api_graph: &'a ApiGraph,
        config: InternalConfig,
        characteristics: DatasetCharacteristics,
        scoring_weights: ScoringWeights,
        dimension_index: DimensionIndex<'a>,
        profile_names: Vec<&'a str>,
        _domain_hints: Option<&'a DomainHints>,
    }
    impl<'a> ChartMatcher<'a> {
        pub fn new(
            profiles: &'a [DimensionProfile],
            api_graph: &'a ApiGraph,
            config: &'a MatchingConfig,
            hints: Option<&'a DomainHints>,
        ) -> Self {
            let characteristics = DatasetCharacteristics::from_profiles(profiles);
            let internal_config = InternalConfig::new(config, &characteristics);
            let scoring_weights = Self::get_adaptive_scoring_weights(&characteristics);
            let dimension_index = DimensionIndex::new(profiles);
            let profile_names = profiles.iter().map(|p| p.name.as_str()).collect();
            Self {
                api_graph,
                config: internal_config,
                characteristics,
                scoring_weights,
                dimension_index,
                profile_names,
                _domain_hints: hints,
            }
        }
        fn get_adaptive_scoring_weights(
            characteristics: &DatasetCharacteristics,
        ) -> ScoringWeights {
            let mut weights = ScoringWeights::default();
            if characteristics.complexity_score > 0.7 {
                weights.technical_weight = 0.4;
                weights.semantic_weight = 0.3;
                weights.visual_weight = 0.2;
            } else if characteristics.complexity_score < 0.3 {
                weights.visual_weight = 0.4;
            }
            if characteristics.dimensionality > 10 {
                weights.utilisation_weight = 0.05;
            }
            weights
        }
        pub fn find_charts(&self) -> Vec<RenderSpec> {
            if self.profile_names.is_empty() {
                return Vec::new();
            }
            let stage1_candidates = self.stage1_fast_filter();
            if stage1_candidates.is_empty() {
                return Vec::new();
            }
            let stage2_candidates = self.stage2_semantic_analysis(stage1_candidates);
            if stage2_candidates.is_empty() {
                return Vec::new();
            }
            let mut final_results = self.stage3_full_analysis(&stage2_candidates);
            final_results.sort_unstable_by(|a, b| {
                self.calculate_final_ranking_score(b)
                    .partial_cmp(&self.calculate_final_ranking_score(a))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            final_results.truncate(self.config.final_max_results);
            final_results
        }
        fn stage1_fast_filter(&self) -> Vec<ChartCandidate<'a>> {
            let mut candidates: Vec<_> = self
                .api_graph
                .get_all_charts()
                .iter()
                .filter(|chart| chart.library == "plotly")
                .map(|chart| {
                    let detailed_score = self.calculate_detailed_scores(chart);
                    let score = detailed_score.overall_score;
                    (chart, score, detailed_score)
                })
                .filter(|(_, score, _)| *score >= self.config.stage1_threshold)
                .map(|(chart, score, detailed_score)| ChartCandidate {
                    chart,
                    score,
                    detailed_score: Some(detailed_score),
                })
                .collect();
            candidates.sort_unstable_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            candidates.truncate(self.config.stage1_max_candidates);
            candidates
        }
        fn stage2_semantic_analysis(
            &self,
            candidates: Vec<ChartCandidate<'a>>,
        ) -> Vec<ChartCandidate<'a>> {
            let mut qualified_candidates: Vec<_> = candidates
                .into_iter()
                .filter(|c| c.score >= self.config.stage2_threshold)
                .collect();
            qualified_candidates.sort_unstable_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            qualified_candidates.truncate(self.config.stage2_max_candidates);
            qualified_candidates
        }
        fn stage3_full_analysis(&self, candidates: &[ChartCandidate<'a>]) -> Vec<RenderSpec> {
            candidates
                .par_iter()
                .flat_map(|candidate| {
                    let mut builder = SpecBuilder::new(
                        candidate.chart,
                        &self.dimension_index,
                        self,
                        candidate.detailed_score.as_ref(),
                    );
                    let mut specs = Vec::new();
                    if builder.try_map_required() {
                        if let Some(spec) = builder.build() {
                            specs.push(spec);
                        }
                    }
                    specs
                })
                .filter(|spec| spec.quality_score >= self.config.min_quality_score)
                .collect()
        }
        pub fn calculate_detailed_scores(&self, chart: &ChartNode) -> ChartScore {
            let mut score = ChartScore {
                technical_feasibility: self.calculate_technical_feasibility(chart),
                semantic_appropriateness: self.calculate_semantic_score(chart),
                visual_effectiveness: self.calculate_visual_effectiveness(chart),
                data_utilisation: self.calculate_data_utilisation(chart),
                complexity_match: self.calculate_complexity_match(chart),
                overall_score: 0.0,
            };
            score.calculate_overall(&self.scoring_weights);
            score
        }
        fn calculate_technical_feasibility(&self, chart: &ChartNode) -> f64 {
            let required_args = chart.required_args();
            if required_args.is_empty() {
                return 1.0;
            }
            let total_score: f64 = required_args
                .iter()
                .map(|(_, arg)| self.calculate_arg_compatibility(arg))
                .sum();
            (total_score / required_args.len() as f64).min(1.0)
        }
        fn calculate_arg_compatibility(&self, arg_spec: &ArgSpec) -> f64 {
            let compatible_profiles: Vec<_> = arg_spec
                .data_type
                .accepted_types()
                .iter()
                .flat_map(|dt| self.dimension_index.get_by_type(dt))
                .collect();
            if compatible_profiles.is_empty() {
                return 0.0;
            }
            let mut score = 0.6;
            let best_quality = compatible_profiles
                .iter()
                .map(|p| p.quality_score)
                .fold(0.0, f64::max);
            score += best_quality * 0.3;
            score += (compatible_profiles.len() as f64 / 5.0).min(0.1);
            score.min(1.0)
        }
        fn calculate_semantic_score(&self, chart: &ChartNode) -> f64 {
            let mut score = 0.5;
            score += match chart.name.as_str() {
                "line" => self.line_chart_semantic_score(),
                "scatter" => self.scatter_chart_semantic_score(),
                "histogram" => self.histogram_semantic_score(),
                "bar" => self.bar_chart_semantic_score(),
                "box" => self.box_chart_semantic_score(),
                "pie" => self.pie_chart_semantic_score(),
                _ => 0.0,
            };
            score.clamp(0.0, 1.0)
        }
        fn line_chart_semantic_score(&self) -> f64 {
            let mut score = 0.0;
            if self.characteristics.has_temporal {
                score += scoring_weights::SEMANTIC_BONUS_TEMPORAL_LINE;
            }
            if self.has_similar_statistical_measures() {
                score -= scoring_weights::SEMANTIC_PENALTY_SIMILAR_MEASURES;
            }
            if self.has_sequential_data() {
                score += 0.3;
            }
            if !self.characteristics.has_temporal
                && self.characteristics.categorical_count > 0
                && self.characteristics.numeric_count == 0
            {
                score -= 0.2;
            }
            score
        }
        fn scatter_chart_semantic_score(&self) -> f64 {
            let mut score = 0.1;
            if self.characteristics.numeric_count >= 2 {
                score += scoring_weights::SEMANTIC_BONUS_NUMERIC_SCATTER;
            }
            if self.has_complementary_measures() {
                score += scoring_weights::SEMANTIC_BONUS_COMPLEMENTARY_MEASURES;
            }
            if self.characteristics.numeric_count >= 2
                && self.characteristics.categorical_count >= 1
            {
                score += 0.2;
            }
            score
        }
        fn histogram_semantic_score(&self) -> f64 {
            let mut score = 0.2;
            if self.characteristics.numeric_count >= 1 {
                score += 0.3;
            }
            if self.characteristics.dimensionality > 5 {
                score -= 0.1;
            }
            score
        }
        fn bar_chart_semantic_score(&self) -> f64 {
            let mut score = 0.2;
            if self.characteristics.categorical_count >= 1
                && self.characteristics.numeric_count >= 1
            {
                score += 0.3;
            }
            if self.has_ranking_potential() {
                score += 0.2;
            }
            score
        }
        fn box_chart_semantic_score(&self) -> f64 {
            let mut score = 0.1;
            if self.characteristics.categorical_count >= 1
                && self.characteristics.numeric_count >= 1
            {
                score += 0.4;
            }
            if self.has_potential_outliers() {
                score += 0.2;
            }
            score
        }
        fn pie_chart_semantic_score(&self) -> f64 {
            let mut score = 0.0;
            if self.characteristics.categorical_count >= 1
                && self.characteristics.numeric_count >= 1
            {
                score += 0.3;
            }
            if self.has_high_cardinality_categorical() {
                score -= 0.4;
            }
            score
        }
        fn calculate_visual_effectiveness(&self, chart: &ChartNode) -> f64 {
            let mut score = 0.7;
            if let Some(cardinality_penalty) = self.calculate_cardinality_penalty(chart) {
                score -= cardinality_penalty;
            }
            let required_dims = chart.required_args().len();
            let available_dims = self.characteristics.dimensionality;
            if required_dims > available_dims {
                score -= 0.3;
            } else if required_dims == available_dims {
                score += 0.1;
            }
            score += self.calculate_readability_score(chart);
            score.clamp(0.0, 1.0)
        }
        fn calculate_cardinality_penalty(&self, chart: &ChartNode) -> Option<f64> {
            let max_penalty = self
                .dimension_index
                .by_name
                .values()
                .filter_map(|p| p.cardinality)
                .map(|cardinality| match chart.name.as_str() {
                    "pie" if cardinality > scoring_weights::PIE_CHART_MAX_CATEGORIES => 0.4,
                    "bar" if cardinality > scoring_weights::HIGH_CARDINALITY_THRESHOLD => 0.3,
                    "line" if cardinality > 50 => 0.2,
                    _ if cardinality > 100 => 0.1,
                    _ => 0.0,
                })
                .fold(0.0, f64::max);
            if max_penalty > 0.0 {
                Some(max_penalty)
            } else {
                None
            }
        }
        fn calculate_readability_score(&self, chart: &ChartNode) -> f64 {
            match chart.name.as_str() {
                "scatter" | "box" | "violin" => 0.1,
                "line" if self.characteristics.has_temporal => 0.2,
                "histogram" => 0.15,
                _ => 0.0,
            }
        }
        fn calculate_data_utilisation(&self, chart: &ChartNode) -> f64 {
            let required_args = chart.required_args().len();
            if self.characteristics.dimensionality == 0 {
                return 0.0;
            }
            let utilisation_ratio =
                required_args as f64 / self.characteristics.dimensionality as f64;
            match utilisation_ratio {
                r if r <= 0.3 => 0.5,
                r if r <= 0.7 => 1.0,
                r if r <= 1.0 => 0.8,
                _ => 0.3,
            }
        }
        fn calculate_complexity_match(&self, chart: &ChartNode) -> f64 {
            let chart_complexity = (chart.args.len() as f64 / 10.0).min(1.0);
            1.0 - (self.characteristics.complexity_score - chart_complexity).abs()
        }
        fn has_similar_statistical_measures(&self) -> bool {
            for i in 0..self.profile_names.len() {
                for j in (i + 1)..self.profile_names.len() {
                    if self.are_similar_statistical_measures(
                        self.profile_names[i],
                        self.profile_names[j],
                    ) {
                        return true;
                    }
                }
            }
            false
        }
        fn has_complementary_measures(&self) -> bool {
            for i in 0..self.profile_names.len() {
                for j in (i + 1)..self.profile_names.len() {
                    if self.are_complementary_measures(self.profile_names[i], self.profile_names[j])
                    {
                        return true;
                    }
                }
            }
            false
        }
        fn has_sequential_data(&self) -> bool {
            self.characteristics.has_temporal
                || self.profile_names.iter().any(|name| {
                    let lower = name.to_lowercase();
                    lower.contains("sequence") || lower.contains("order") || lower.contains("index")
                })
        }
        fn has_ranking_potential(&self) -> bool {
            self.profile_names.iter().any(|name| {
                let lower = name.to_lowercase();
                lower.contains("rank") || lower.contains("score") || lower.contains("rating")
            })
        }
        fn has_potential_outliers(&self) -> bool {
            self.dimension_index.by_name.values().any(|p| {
                p.numeric_stats
                    .as_ref()
                    .is_some_and(|s| s.outlier_count > 0)
            })
        }
        fn has_high_cardinality_categorical(&self) -> bool {
            self.dimension_index.by_name.values().any(|p| {
                matches!(p.data_type, DataType::Categorical)
                    && p.cardinality
                        .is_some_and(|c| c > scoring_weights::HIGH_CARDINALITY_THRESHOLD)
            })
        }
        fn are_similar_statistical_measures(&self, name1: &str, name2: &str) -> bool {
            let n1 = name1.to_lowercase();
            let n2 = name2.to_lowercase();
            let p1 = n1.contains("percentile") || n1.contains("quartile");
            let p2 = n2.contains("percentile") || n2.contains("quartile");
            if p1 && p2 {
                return true;
            }
            let keywords = ["mean", "median", "average", "std", "variance", "min", "max"];
            let s1 = keywords.iter().any(|k| n1.contains(k));
            let s2 = keywords.iter().any(|k| n2.contains(k));
            s1 && s2
        }
        fn are_complementary_measures(&self, name1: &str, name2: &str) -> bool {
            let n1 = name1.to_lowercase();
            let n2 = name2.to_lowercase();
            let pairs = [
                ("revenue", "cost"),
                ("income", "expense"),
                ("profit", "loss"),
                ("sales", "returns"),
                ("actual", "budget"),
                ("actual", "forecast"),
                ("price", "quantity"),
                ("width", "height"),
                ("latitude", "longitude"),
                ("start", "end"),
                ("before", "after"),
            ];
            pairs.iter().any(|(w1, w2)| {
                (n1.contains(w1) && n2.contains(w2)) || (n1.contains(w2) && n2.contains(w1))
            })
        }
        fn calculate_final_ranking_score(&self, spec: &RenderSpec) -> f64 {
            let mut score = spec.quality_score;
            if !score.is_finite() {
                score = 0.0;
            }
            if self.config.prefer_high_dimensionality {
                let utilisation =
                    spec.dimensions_used as f64 / self.characteristics.dimensionality as f64;
                if utilisation.is_finite() {
                    score += utilisation * scoring_weights::FINAL_SCORE_UTILISATION_BONUS;
                }
            }
            if spec.complete {
                score += scoring_weights::FINAL_SCORE_COMPLETENESS_BONUS;
            }
            if score.is_finite() {
                score
            } else {
                0.0
            }
        }
    }
}
pub fn find_qualified_charts(
    profiles: &[DimensionProfile],
    api_graph: &ApiGraph,
    config: &MatchingConfig,
) -> Vec<RenderSpec> {
    internal::ChartMatcher::new(profiles, api_graph, config, None).find_charts()
}
pub fn find_qualified_charts_validated(
    profiles: &[DimensionProfile],
    api_graph: &ApiGraph,
    config: &MatchingConfig,
) -> Result<Vec<RenderSpec>, MatchingError> {
    config.validate().map_err(MatchingError::InvalidConfig)?;
    if profiles.is_empty() {
        return Ok(Vec::new());
    }
    let matcher = internal::ChartMatcher::new(profiles, api_graph, config, None);
    Ok(matcher.find_charts())
}
pub fn find_qualified_charts_with_hints(
    profiles: &[DimensionProfile],
    api_graph: &ApiGraph,
    config: &MatchingConfig,
    hints: &DomainHints,
) -> Vec<RenderSpec> {
    let mut adjusted_config = config.clone();
    if hints.strict_quality_threshold {
        adjusted_config.min_quality_score = adjusted_config.min_quality_score.max(0.7);
    }
    if hints.prefer_simple_charts {
        adjusted_config.prefer_high_dimensionality = false;
    }
    let mut results =
        internal::ChartMatcher::new(profiles, api_graph, &adjusted_config, Some(hints))
            .find_charts();
    if let Some(lib) = &hints.preferred_library {
        results.retain(|spec| &spec.library == lib);
    }
    results
}
pub fn find_best_chart(profiles: &[DimensionProfile], api_graph: &ApiGraph) -> Option<RenderSpec> {
    find_qualified_charts(
        profiles,
        api_graph,
        &MatchingConfig {
            max_suggestions_per_chart: 1,
            ..Default::default()
        },
    )
    .into_iter()
    .next()
}
pub fn find_high_dimensional_charts(
    profiles: &[DimensionProfile],
    api_graph: &ApiGraph,
) -> Vec<RenderSpec> {
    let config = MatchingConfig {
        prefer_high_dimensionality: true,
        min_quality_score: 0.5,
        ..Default::default()
    };
    find_qualified_charts(profiles, api_graph, &config)
        .into_iter()
        .filter(|spec| spec.dimensions_used >= profiles.len().saturating_sub(1))
        .collect()
}
pub fn find_charts_by_library(
    profiles: &[DimensionProfile],
    api_graph: &ApiGraph,
    library: &str,
) -> Vec<RenderSpec> {
    find_qualified_charts(profiles, api_graph, &MatchingConfig::default())
        .into_iter()
        .filter(|spec| spec.library == library)
        .collect()
}
struct ExplanationDimensionIndex<'a> {
    by_type: HashMap<DataType, Vec<&'a DimensionProfile>>,
    all_dims: &'a [DimensionProfile],
}
impl<'a> ExplanationDimensionIndex<'a> {
    fn new(profiles: &'a [DimensionProfile]) -> Self {
        let mut by_type: HashMap<DataType, Vec<&'a DimensionProfile>> = HashMap::new();
        for p in profiles {
            by_type.entry(p.data_type.clone()).or_default().push(p);
        }
        Self {
            by_type,
            all_dims: profiles,
        }
    }
    fn get_compatible_dimensions(&self, spec: &ArgSpec) -> Vec<&'a DimensionProfile> {
        spec.data_type
            .accepted_types()
            .iter()
            .flat_map(|dt| self.by_type.get(dt).map_or(&[][..], |v| v.as_slice()))
            .cloned()
            .collect()
    }
}
pub fn explain_chart_suggestions(
    profiles: &[DimensionProfile],
    api_graph: &ApiGraph,
) -> ChartExplanation {
    let dimensions = ExplanationDimensionIndex::new(profiles);
    let explanations = api_graph
        .get_all_charts()
        .iter()
        .filter(|chart| chart.library == "plotly")
        .map(|chart| explain_single_chart(chart, &dimensions))
        .collect();
    ChartExplanation {
        total_dimensions: dimensions.all_dims.len(),
        dimension_types: format_dimension_summary(&dimensions),
        chart_explanations: explanations,
    }
}
fn explain_single_chart(
    chart: &ChartNode,
    dimensions: &ExplanationDimensionIndex,
) -> SingleChartExplanation {
    let mut missing_requirements = Vec::new();
    let mut available_mappings = Vec::new();
    for (arg_name, arg_spec) in chart.required_args() {
        let compatible_dims = dimensions.get_compatible_dimensions(arg_spec);
        if compatible_dims.is_empty() {
            missing_requirements.push(format!(
                "No compatible data for required argument '{}' (needs {})",
                arg_name,
                format_data_types(arg_spec.data_type.accepted_types())
            ));
        } else {
            available_mappings.push(format!(
                "'{}' can map to: {}",
                arg_name,
                compatible_dims
                    .iter()
                    .map(|d| d.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    SingleChartExplanation {
        chart_name: chart.name.clone(),
        can_render: missing_requirements.is_empty(),
        missing_requirements,
        available_mappings,
        quality_issues: Vec::new(),
    }
}
fn format_dimension_summary(dimensions: &ExplanationDimensionIndex) -> String {
    let mut counts = HashMap::new();
    for p in dimensions.all_dims {
        *counts.entry(p.data_type.clone()).or_insert(0) += 1;
    }
    format!(
        "{} numeric, {} categorical, {} temporal",
        counts.get(&DataType::Numeric).unwrap_or(&0),
        counts.get(&DataType::Categorical).unwrap_or(&0),
        counts.get(&DataType::Temporal).unwrap_or(&0)
    )
}
fn format_data_types(data_types: Vec<&DataType>) -> String {
    data_types
        .iter()
        .map(|dt| format!("{dt:?}").to_lowercase())
        .collect::<Vec<_>>()
        .join(" or ")
}
pub mod advanced {
    use super::*;

    #[derive(Debug, Clone)]
    pub struct MatchingPerformanceMetrics {
        pub total_time_ms: u64,
        pub charts_evaluated: usize,
        pub results_generated: usize,
        pub avg_quality_score: f64,
        pub dimensionality_coverage: f64,
    }
    pub fn get_matching_performance_metrics(
        profiles: &[DimensionProfile],
        api_graph: &ApiGraph,
        config: &MatchingConfig,
    ) -> MatchingPerformanceMetrics {
        let start_time = Instant::now();
        let results = find_qualified_charts(profiles, api_graph, config);
        let total_time = start_time.elapsed();
        MatchingPerformanceMetrics {
            total_time_ms: total_time.as_millis() as u64,
            charts_evaluated: api_graph.get_all_charts().len(),
            results_generated: results.len(),
            avg_quality_score: if results.is_empty() {
                0.0
            } else {
                results.iter().map(|r| r.quality_score).sum::<f64>() / results.len() as f64
            },
            dimensionality_coverage: if profiles.is_empty() || results.is_empty() {
                0.0
            } else {
                results.iter().map(|r| r.dimensions_used).max().unwrap_or(0) as f64
                    / profiles.len() as f64
            },
        }
    }
    pub fn batch_process_datasets(
        datasets: &[(String, Vec<DimensionProfile>)],
        api_graph: &ApiGraph,
        config: &MatchingConfig,
    ) -> HashMap<String, Vec<RenderSpec>> {
        datasets
            .par_iter()
            .map(|(name, profiles)| {
                (
                    name.clone(),
                    find_qualified_charts(profiles, api_graph, config),
                )
            })
            .collect()
    }
    pub fn validate_render_spec(
        spec: &RenderSpec,
        profiles: &[DimensionProfile],
    ) -> Result<(), String> {
        let profile_names: HashSet<_> = profiles.iter().map(|p| &p.name).collect();
        for (arg, col) in &spec.mappings {
            if !profile_names.contains(col) {
                return Err(format!(
                    "Column '{col}' for argument '{arg}' not in dataset"
                ));
            }
        }
        Ok(())
    }
    pub fn suggest_charts_with_heuristics(
        profiles: &[DimensionProfile],
        api_graph: &ApiGraph,
        hints: &DomainHints,
    ) -> Vec<RenderSpec> {
        find_qualified_charts_with_hints(profiles, api_graph, &MatchingConfig::default(), hints)
    }
    #[derive(Debug, Clone)]
    pub struct AdvancedMatchingConfig {
        pub base_config: MatchingConfig,
        pub enable_adaptive_thresholds: bool,
        pub enable_performance_learning: bool,
        pub max_processing_time_ms: u64,
        pub semantic_weight: f64,
        pub quality_weight: f64,
        pub performance_weight: f64,
        pub scoring_profile: ScoringProfile,
    }
    impl Default for AdvancedMatchingConfig {
        fn default() -> Self {
            Self {
                base_config: MatchingConfig::default(),
                enable_adaptive_thresholds: true,
                enable_performance_learning: false,
                max_processing_time_ms: 1000,
                semantic_weight: 0.3,
                quality_weight: 0.5,
                performance_weight: 0.2,
                scoring_profile: ScoringProfile::Balanced,
            }
        }
    }
    pub fn find_charts_advanced(
        profiles: &[DimensionProfile],
        api_graph: &ApiGraph,
        config: &AdvancedMatchingConfig,
    ) -> Vec<RenderSpec> {
        let mut adjusted_config = config.base_config.clone();
        if config.enable_adaptive_thresholds {
            let characteristics =
                crate::chart_matcher::internal::DatasetCharacteristics::from_profiles(profiles);
            if characteristics.complexity_score > 0.7 {
                adjusted_config.min_quality_score *= 0.8;
            }
        }
        find_qualified_charts(profiles, api_graph, &adjusted_config)
    }
    pub fn get_detailed_scoring_breakdown(
        profiles: &[DimensionProfile],
        api_graph: &ApiGraph,
        config: &MatchingConfig,
    ) -> HashMap<String, ChartScore> {
        let matcher =
            crate::chart_matcher::internal::ChartMatcher::new(profiles, api_graph, config, None);
        let mut breakdown = HashMap::new();
        for chart in api_graph.get_all_charts() {
            if chart.library == "plotly" {
                let score = matcher.calculate_detailed_scores(chart);
                breakdown.insert(chart.name.clone(), score);
            }
        }
        breakdown
    }
    pub fn find_charts_for_use_case(
        profiles: &[DimensionProfile],
        api_graph: &ApiGraph,
        use_case: &str,
    ) -> Vec<RenderSpec> {
        let config = match use_case {
            "exploration" => MatchingConfig {
                min_quality_score: 0.2,
                max_suggestions_per_chart: 15,
                include_partial_matches: true,
                prefer_high_dimensionality: true,
            },
            "presentation" => MatchingConfig {
                min_quality_score: 0.6,
                max_suggestions_per_chart: 5,
                include_partial_matches: false,
                prefer_high_dimensionality: false,
            },
            "analysis" => MatchingConfig {
                min_quality_score: 0.4,
                max_suggestions_per_chart: 8,
                include_partial_matches: true,
                prefer_high_dimensionality: false,
            },
            _ => MatchingConfig::default(),
        };
        find_qualified_charts(profiles, api_graph, &config)
    }
    pub fn filter_charts_by_quality(
        render_specs: Vec<RenderSpec>,
        min_technical_score: f64,
        min_semantic_score: f64,
        min_visual_score: f64,
    ) -> (Vec<RenderSpec>, Vec<String>) {
        let mut filtered = Vec::new();
        let mut rejections = Vec::new();
        for spec in render_specs {
            if let Some(detailed_score) = &spec.detailed_score {
                let mut rejected = false;
                if detailed_score.technical_feasibility < min_technical_score {
                    rejections.push(format!(
                        "{}: Technical feasibility too low ({:.2})",
                        spec.chart_name, detailed_score.technical_feasibility
                    ));
                    rejected = true;
                }
                if detailed_score.semantic_appropriateness < min_semantic_score {
                    rejections.push(format!(
                        "{}: Semantic appropriateness too low ({:.2})",
                        spec.chart_name, detailed_score.semantic_appropriateness
                    ));
                    rejected = true;
                }
                if detailed_score.visual_effectiveness < min_visual_score {
                    rejections.push(format!(
                        "{}: Visual effectiveness too low ({:.2})",
                        spec.chart_name, detailed_score.visual_effectiveness
                    ));
                    rejected = true;
                }
                if !rejected {
                    filtered.push(spec);
                }
            } else {
                let min_overall = min_technical_score
                    .max(min_semantic_score)
                    .max(min_visual_score);
                if spec.quality_score >= min_overall {
                    filtered.push(spec);
                } else {
                    rejections.push(format!(
                        "{}: Overall quality too low ({:.2})",
                        spec.chart_name, spec.quality_score
                    ));
                }
            }
        }
        (filtered, rejections)
    }
    pub fn recommend_charts_with_reasoning(
        profiles: &[DimensionProfile],
        api_graph: &ApiGraph,
        config: &MatchingConfig,
    ) -> Vec<(RenderSpec, String)> {
        let specs = find_qualified_charts(profiles, api_graph, config);
        specs
            .into_iter()
            .map(|spec| {
                let reasoning = if let Some(detailed_score) = &spec.detailed_score {
                    format!(
                        "Technical: {:.1}%, Semantic: {:.1}%, Visual: {:.1}%, Data Use: {:.1}%",
                        detailed_score.technical_feasibility * 100.0,
                        detailed_score.semantic_appropriateness * 100.0,
                        detailed_score.visual_effectiveness * 100.0,
                        detailed_score.data_utilisation * 100.0
                    )
                } else {
                    format!("Overall quality: {:.1}%", spec.quality_score * 100.0)
                };
                (spec, reasoning)
            })
            .collect()
    }
    pub struct ChartRecommendationCache {
        cache: HashMap<String, Vec<RenderSpec>>,
    }
    impl Default for ChartRecommendationCache {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ChartRecommendationCache {
        pub fn new() -> Self {
            Self {
                cache: HashMap::new(),
            }
        }
        pub fn get(&self, key: &str) -> Option<&Vec<RenderSpec>> {
            self.cache.get(key)
        }
        pub fn insert(&mut self, key: String, specs: Vec<RenderSpec>) {
            self.cache.insert(key, specs);
        }
        pub fn clear(&mut self) {
            self.cache.clear();
        }
    }
    pub struct CacheStats {
        pub hits: usize,
        pub misses: usize,
        pub entries: usize,
    }
    impl CacheStats {
        pub fn hit_ratio(&self) -> f64 {
            if self.hits + self.misses == 0 {
                0.0
            } else {
                self.hits as f64 / (self.hits + self.misses) as f64
            }
        }
    }
}
pub fn get_recommendation_summary(
    profiles: &[DimensionProfile],
    api_graph: &ApiGraph,
    config: &MatchingConfig,
) -> String {
    let characteristics = internal::DatasetCharacteristics::from_profiles(profiles);
    let specs = find_qualified_charts(profiles, api_graph, config);
    let mut summary = String::new();
    summary.push_str(&format!(
        "Dataset: {} dimensions ({} numeric, {} categorical, {} temporal)\n",
        characteristics.dimensionality,
        characteristics.numeric_count,
        characteristics.categorical_count,
        if characteristics.has_temporal { 1 } else { 0 }
    ));
    summary.push_str(&format!(
        "Complexity: {:.1}%, Average cardinality: {:.1}\n",
        characteristics.complexity_score * 100.0,
        characteristics.avg_cardinality
    ));
    summary.push_str(&format!("Found {} suitable charts:\n", specs.len()));
    for spec in specs.iter().take(5) {
        if let Some(detailed_score) = &spec.detailed_score {
            summary.push_str(&format!(
                "  • {}: {:.1}% (T:{:.1}% S:{:.1}% V:{:.1}%)\n",
                spec.chart_name,
                spec.quality_score * 100.0,
                detailed_score.technical_feasibility * 100.0,
                detailed_score.semantic_appropriateness * 100.0,
                detailed_score.visual_effectiveness * 100.0
            ));
        } else {
            summary.push_str(&format!(
                "  • {}: {:.1}%\n",
                spec.chart_name,
                spec.quality_score * 100.0
            ));
        }
    }
    if specs.len() > 5 {
        summary.push_str(&format!("  ... and {} more\n", specs.len() - 5));
    }
    summary
}
pub fn get_quick_recommendations(profiles: &[DimensionProfile]) -> Vec<&'static str> {
    let characteristics = internal::DatasetCharacteristics::from_profiles(profiles);
    let mut recommendations = Vec::new();
    if characteristics.numeric_count >= 2 {
        recommendations.push("scatter");
    }
    if characteristics.has_temporal && characteristics.numeric_count >= 1 {
        recommendations.push("line");
    }
    if characteristics.categorical_count >= 1 && characteristics.numeric_count >= 1 {
        recommendations.push("bar");
    }
    if characteristics.numeric_count >= 1 {
        recommendations.push("histogram");
    }
    if characteristics.categorical_count >= 1 && characteristics.numeric_count >= 1 {
        recommendations.push("box");
    }
    if characteristics.categorical_count >= 1
        && characteristics.numeric_count >= 1
        && characteristics.avg_cardinality
            <= internal::scoring_weights::PIE_CHART_MAX_CATEGORIES as f64
    {
        recommendations.push("pie");
    }
    recommendations
}
