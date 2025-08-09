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

use crate::api_graph::DataType;
use anyhow::Result;
use chrono::{DateTime, NaiveDate, NaiveDateTime};
use polars::prelude::QuantileMethod;
use polars::prelude::*;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::Path;
#[derive(Debug, thiserror::Error)]
pub enum ProfilerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Polars error: {0}")]
    Polars(#[from] polars::error::PolarsError),
    #[error("Parsing error: {0}")]
    Parsing(String),
    #[error("Unsupported data type: {0}")]
    UnsupportedType(String),
    #[error("Configuration error: {0}")]
    Config(String),
}
#[derive(Debug, Clone)]
pub struct ProfilingConfig {
    pub max_sample_values: usize,
    pub type_confidence_threshold: f64,
    pub max_categorical_cardinality: usize,
    pub quality_weights: QualityWeights,
    pub temporal_formats: Vec<String>,
    pub enable_advanced_stats: bool,
}
#[derive(Debug, Clone)]
pub struct QualityWeights {
    pub null_penalty: f64,
    pub cardinality_penalty: f64,
    pub type_confidence_bonus: f64,
    pub outlier_penalty: f64,
    pub interaction_factor: f64,
}
impl Default for QualityWeights {
    fn default() -> Self {
        Self {
            null_penalty: 0.3,
            cardinality_penalty: 0.1,
            type_confidence_bonus: 0.2,
            outlier_penalty: 0.15,
            interaction_factor: 0.1,
        }
    }
}
impl Default for ProfilingConfig {
    fn default() -> Self {
        Self {
            max_sample_values: 100,
            type_confidence_threshold: 0.8,
            max_categorical_cardinality: 50,
            quality_weights: QualityWeights::default(),
            temporal_formats: vec![
                "%Y-%m-%d".to_string(),
                "%Y-%m-%d %H:%M:%S".to_string(),
                "%Y-%m-%dT%H:%M:%S".to_string(),
                "%Y-%m-%dT%H:%M:%SZ".to_string(),
                "%m/%d/%Y".to_string(),
                "%d/%m/%Y".to_string(),
                "%Y%m%d".to_string(),
            ],
            enable_advanced_stats: false,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionProfile {
    pub name: String,
    pub data_type: DataType,
    pub cardinality: Option<usize>,
    pub total_count: usize,
    pub null_count: usize,
    pub null_percentage: f64,
    pub sample_values: Vec<String>,
    pub numeric_stats: Option<NumericStats>,
    pub temporal_stats: Option<TemporalStats>,
    pub quality_score: f64,
    pub type_confidence: f64,
    pub issues: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericStats {
    pub mean: Option<f64>,
    pub median: Option<f64>,
    pub std: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub q25: Option<f64>,
    pub q75: Option<f64>,
    pub skewness: Option<f64>,
    pub kurtosis: Option<f64>,
    pub mad: Option<f64>,
    pub outlier_count: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalStats {
    pub min_date: Option<String>,
    pub max_date: Option<String>,
    pub date_range_days: Option<i64>,
    pub inferred_frequency: Option<String>,
    pub has_time_component: bool,
    pub unique_count: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetSummary {
    pub total_dimensions: usize,
    pub numeric_count: usize,
    pub categorical_count: usize,
    pub temporal_count: usize,
    pub avg_quality_score: f64,
    pub total_issues: usize,
    pub chart_readiness_score: f64,
}
pub struct DataProfiler {
    config: ProfilingConfig,
}
impl DataProfiler {
    pub fn new() -> Self {
        Self {
            config: ProfilingConfig::default(),
        }
    }
    pub fn with_config(config: ProfilingConfig) -> Self {
        Self { config }
    }
    pub fn profile_csv<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<Vec<DimensionProfile>, ProfilerError> {
        let file = File::open(path)?;
        let df = CsvReader::new(file).finish()?;
        self.profile_dataframe(&df)
    }
    pub fn profile_parquet<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<Vec<DimensionProfile>, ProfilerError> {
        let file = File::open(path)?;
        let df = ParquetReader::new(file).finish()?;
        self.profile_dataframe(&df)
    }
    pub fn profile_json<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<Vec<DimensionProfile>, ProfilerError> {
        let file = File::open(path)?;
        let df = JsonReader::new(file).finish()?;
        self.profile_dataframe(&df)
    }
    pub fn profile_dataframe(
        &self,
        df: &DataFrame,
    ) -> Result<Vec<DimensionProfile>, ProfilerError> {
        let total_rows = df.height();
        df.get_columns()
            .par_iter()
            .map(|column| {
                self.profile_column(
                    column.as_series().expect("Column should contain a series"),
                    total_rows,
                )
            })
            .collect()
    }
    fn profile_column(
        &self,
        column: &Series,
        total_rows: usize,
    ) -> Result<DimensionProfile, ProfilerError> {
        let name = column.name().to_string();
        let null_count = column.null_count();
        let null_percentage = if total_rows > 0 {
            null_count as f64 / total_rows as f64
        } else {
            0.0
        };
        let (data_type, type_confidence) = self.detect_data_type(column)?;
        let mut numeric_stats = None;
        let mut temporal_stats = None;
        let mut cardinality = None;
        match data_type {
            DataType::Numeric => {
                let s_float = column.cast(&polars::prelude::DataType::Float64)?;
                numeric_stats = Some(self.calculate_numeric_stats(&s_float)?);
            }
            DataType::Temporal => {
                let s_str = column.cast(&polars::prelude::DataType::String)?;
                let str_chunked = s_str.str()?;
                let values: Vec<Option<&str>> = str_chunked.into_iter().collect();
                temporal_stats = Some(self.calculate_temporal_stats_simple(&values)?);
            }
            DataType::Categorical => {
                cardinality = Some(column.n_unique()?);
            }
        }
        let sample_values = self.get_sample_values(column)?;
        let issues = self.detect_quality_issues(
            &data_type,
            null_percentage,
            cardinality,
            &numeric_stats,
            total_rows,
        );
        let quality_score = self.calculate_quality_score(
            null_percentage,
            type_confidence,
            cardinality,
            &issues,
            &numeric_stats,
        );
        Ok(DimensionProfile {
            name,
            data_type,
            cardinality,
            total_count: total_rows,
            null_count,
            null_percentage,
            sample_values,
            numeric_stats,
            temporal_stats,
            quality_score,
            type_confidence,
            issues,
        })
    }
    fn detect_data_type(&self, column: &Series) -> Result<(DataType, f64), ProfilerError> {
        let non_null_count = column.len() - column.null_count();
        if non_null_count == 0 {
            return Ok((DataType::Categorical, 0.0));
        }
        if matches!(
            column.dtype(),
            polars::prelude::DataType::Float64
                | polars::prelude::DataType::Int64
                | polars::prelude::DataType::Float32
                | polars::prelude::DataType::Int32
        ) {
            return Ok((DataType::Numeric, 1.0));
        }
        if let Ok(s_float) = column.cast(&polars::prelude::DataType::Float64) {
            let successful_casts = s_float.len() - s_float.null_count();
            let confidence = successful_casts as f64 / non_null_count as f64;
            if confidence >= self.config.type_confidence_threshold {
                if let Ok(s_str) = column.cast(&polars::prelude::DataType::String) {
                    let str_ca = s_str.str()?;
                    let unique_count = str_ca.unique()?.len();
                    if unique_count == 1 {
                        return Ok((DataType::Categorical, 0.9));
                    }
                }
                return Ok((DataType::Numeric, confidence));
            }
        }
        if let Ok(s_str) = column.cast(&polars::prelude::DataType::String) {
            let str_ca = s_str.str()?;
            let values: Vec<Option<&str>> = str_ca.into_iter().collect();
            let temporal_confidence = self.test_temporal_parsing_simple(&values)?;
            if temporal_confidence >= self.config.type_confidence_threshold {
                return Ok((DataType::Temporal, temporal_confidence));
            }
        }
        Ok((DataType::Categorical, 0.8))
    }
    fn calculate_numeric_stats(&self, s: &Series) -> Result<NumericStats, ProfilerError> {
        let s_f64 = s.f64()?;
        if s_f64.is_empty() {
            return Ok(NumericStats {
                mean: None,
                median: None,
                std: None,
                min: None,
                max: None,
                q25: None,
                q75: None,
                skewness: None,
                kurtosis: None,
                mad: None,
                outlier_count: 0,
            });
        }
        let q25 = s_f64.quantile(0.25, QuantileMethod::Linear).ok().flatten();
        let q75 = s_f64.quantile(0.75, QuantileMethod::Linear).ok().flatten();
        let outlier_count = if let (Some(q25_val), Some(q75_val)) = (q25, q75) {
            let iqr = q75_val - q25_val;
            if iqr > 0.0 {
                let lower_bound = q25_val - 1.5 * iqr;
                let upper_bound = q75_val + 1.5 * iqr;
                s_f64
                    .into_iter()
                    .filter(|opt_v| opt_v.is_some_and(|v| v < lower_bound || v > upper_bound))
                    .count()
            } else {
                0
            }
        } else {
            0
        };
        let mad = if let Some(median) = s_f64.median() {
            let deviations: Vec<f64> = s_f64
                .into_iter()
                .filter_map(|opt_v| opt_v.map(|v| (v - median).abs()))
                .collect();
            if !deviations.is_empty() {
                let mad_series = Series::new("mad".into(), deviations);
                mad_series.f64().ok().and_then(|s| s.median())
            } else {
                None
            }
        } else {
            None
        };
        let kurtosis = if self.config.enable_advanced_stats {
            if let (Some(mean), Some(std_dev)) = (s_f64.mean(), s_f64.std(1)) {
                if std_dev > 1e-9 {
                    let n = s_f64.len() as f64;
                    let fourth_moment: f64 = s_f64
                        .into_iter()
                        .filter_map(|opt_v| opt_v.map(|v| ((v - mean) / std_dev).powi(4)))
                        .sum();
                    Some((fourth_moment / n) - 3.0)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        let skewness = None;
        Ok(NumericStats {
            mean: s_f64.mean(),
            median: s_f64.median(),
            std: s_f64.std(1),
            min: s_f64.min(),
            max: s_f64.max(),
            q25,
            q75,
            skewness,
            kurtosis,
            mad,
            outlier_count,
        })
    }
    fn get_sample_values(&self, series: &Series) -> Result<Vec<String>, ProfilerError> {
        let unique = series.unique()?;
        let sample = unique.head(Some(self.config.max_sample_values));
        let str_series = sample.cast(&polars::prelude::DataType::String)?;
        let str_chunked = str_series.str()?;
        Ok(str_chunked
            .into_iter()
            .filter_map(|opt_s| opt_s.map(String::from))
            .collect())
    }
    fn test_temporal_parsing_simple(&self, values: &[Option<&str>]) -> Result<f64, ProfilerError> {
        let non_null_values: Vec<_> = values.iter().filter_map(|&v| v).collect();
        if non_null_values.is_empty() {
            return Ok(0.0);
        }
        let total_count = non_null_values.len();
        let mut best_confidence = 0.0;
        for format in &self.config.temporal_formats {
            let successful_parses = non_null_values
                .par_iter()
                .filter(|&&v| self.parse_datetime_simple(v, format).is_some())
                .count();
            let confidence = successful_parses as f64 / total_count as f64;
            best_confidence = f64::max(best_confidence, confidence);
        }
        Ok(best_confidence)
    }
    fn calculate_temporal_stats_simple(
        &self,
        values: &[Option<&str>],
    ) -> Result<TemporalStats, ProfilerError> {
        let non_null_values: Vec<_> = values.iter().filter_map(|&v| v).collect();
        let mut datetime_values = Vec::new();
        let mut has_time = false;
        for value in &non_null_values {
            for format in &self.config.temporal_formats {
                if let Some(dt) = self.parse_datetime_simple(value, format) {
                    datetime_values.push(dt);
                    if format.contains("%H") || format.contains("%M") || format.contains("%S") {
                        has_time = true;
                    }
                    break;
                }
            }
        }
        if datetime_values.is_empty() {
            return Ok(TemporalStats {
                min_date: None,
                max_date: None,
                date_range_days: None,
                inferred_frequency: None,
                has_time_component: false,
                unique_count: 0,
            });
        }
        datetime_values.sort();
        let min_date = datetime_values.first().map(|dt| dt.to_rfc3339());
        let max_date = datetime_values.last().map(|dt| dt.to_rfc3339());
        let date_range_days =
            if let (Some(first), Some(last)) = (datetime_values.first(), datetime_values.last()) {
                Some(last.signed_duration_since(*first).num_days())
            } else {
                None
            };
        let unique_count = datetime_values.iter().collect::<HashSet<_>>().len();
        let inferred_frequency = self.infer_temporal_frequency_simple(&datetime_values);
        Ok(TemporalStats {
            min_date,
            max_date,
            date_range_days,
            inferred_frequency,
            has_time_component: has_time,
            unique_count,
        })
    }
    fn parse_datetime_simple(&self, value: &str, format: &str) -> Option<DateTime<chrono::Utc>> {
        if let Ok(dt) = NaiveDateTime::parse_from_str(value, format) {
            return Some(dt.and_utc());
        }
        if let Ok(date) = NaiveDate::parse_from_str(value, format) {
            return Some(date.and_hms_opt(0, 0, 0)?.and_utc());
        }
        None
    }
    fn infer_temporal_frequency_simple(
        &self,
        datetime_values: &[DateTime<chrono::Utc>],
    ) -> Option<String> {
        if datetime_values.len() < 2 {
            return None;
        }
        let mut deltas = Vec::new();
        for i in 1..datetime_values.len() {
            deltas.push(
                datetime_values[i]
                    .signed_duration_since(datetime_values[i - 1])
                    .num_milliseconds(),
            );
        }
        let mut delta_counts = HashMap::new();
        for delta in deltas {
            *delta_counts.entry(delta).or_insert(0) += 1;
        }
        let most_common_delta = delta_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(delta, _)| delta)?;
        let frequency = match most_common_delta {
            d if d >= 86_400_000 => "daily",
            d if d >= 3_600_000 => "hourly",
            d if d >= 60_000 => "minutely",
            _ => "irregular",
        };
        Some(frequency.to_string())
    }
    fn detect_quality_issues(
        &self,
        data_type: &DataType,
        null_percentage: f64,
        cardinality: Option<usize>,
        numeric_stats: &Option<NumericStats>,
        total_count: usize,
    ) -> Vec<String> {
        let mut issues = Vec::new();
        if null_percentage > 0.3 {
            issues.push(format!(
                "High null percentage: {:.1}%",
                null_percentage * 100.0
            ));
        }
        match data_type {
            DataType::Categorical => {
                if let Some(card) = cardinality {
                    if card > self.config.max_categorical_cardinality {
                        issues.push(format!("High cardinality: {card} unique values"));
                    }
                    if card == 1 && total_count > 1 {
                        issues.push("Single unique value (constant column)".to_string());
                    }
                    if card < 3 && total_count > 50 {
                        issues.push(format!(
                            "Very low cardinality for large dataset: {card} unique values"
                        ));
                    }
                }
            }
            DataType::Numeric => {
                if let Some(stats) = numeric_stats {
                    if let Some(std_dev) = stats.std {
                        if std_dev < 1e-9 && total_count > 1 {
                            issues.push("Zero standard deviation (constant values)".to_string());
                        }
                    }
                    if let (Some(mean), Some(std_dev)) = (stats.mean, stats.std) {
                        if mean > 0.0 && std_dev > 0.0 {
                            let cv = std_dev / mean;
                            if cv < 0.01 && total_count > 5 {
                                issues.push("Very low coefficient of variation (values too similar for meaningful visualisation)".to_string());
                            }
                        }
                    }
                    if let (Some(min), Some(max)) = (stats.min, stats.max) {
                        let range = max - min;
                        if range < 1e-9 && total_count > 1 {
                            issues.push("Zero range (all values identical)".to_string());
                        }
                        if let Some(card) = cardinality {
                            if card < 10 && total_count > 100 {
                                issues.push(format!("Low cardinality ({card}) for large numeric dataset - may be categorical"));
                            }
                            if card > 5 && range > 0.0 {
                                let avg_gap = range / (card as f64 - 1.0);
                                if avg_gap >= 1.0 && (avg_gap - avg_gap.round()).abs() < 0.1 {
                                    issues.push(format!(
                                        "Data appears quantised with ~{avg_gap:.0} unit intervals"
                                    ));
                                }
                            }
                        }
                    }
                    if stats.outlier_count > total_count / 10 {
                        issues.push(format!("High outlier count: {}", stats.outlier_count));
                    }
                    if let (Some(skew), Some(kurtosis)) = (stats.skewness, stats.kurtosis) {
                        if skew.abs() < 1.0 && kurtosis < -0.5 {
                            issues.push("Potential bimodal distribution detected".to_string());
                        }
                        if skew.abs() > 3.0 {
                            issues.push(format!("Highly skewed distribution: {skew:.2}"));
                        }
                    }
                    if let Some(card) = cardinality {
                        let uniqueness_ratio = card as f64 / total_count as f64;
                        if uniqueness_ratio < 0.05 && total_count > 20 {
                            issues.push("Very low uniqueness ratio - unsuitable for size-based visualisations".to_string());
                        }
                    }
                }
            }
            DataType::Temporal => {
                if let Some(card) = cardinality {
                    if card == 1 && total_count > 1 {
                        issues.push("Single time point (constant temporal column)".to_string());
                    }
                    if card < total_count / 10 && total_count > 20 {
                        issues
                            .push("Sparse temporal data with many repeated timestamps".to_string());
                    }
                }
            }
        }
        issues
    }
    fn calculate_quality_score(
        &self,
        null_percentage: f64,
        type_confidence: f64,
        cardinality: Option<usize>,
        issues: &[String],
        numeric_stats: &Option<NumericStats>,
    ) -> f64 {
        let weights = &self.config.quality_weights;
        let mut score = 1.0;
        score -= null_percentage * weights.null_penalty;
        score += (type_confidence - (1.0 - type_confidence)) * weights.type_confidence_bonus;
        if let Some(card) = cardinality {
            if card > self.config.max_categorical_cardinality {
                score -= (card as f64 / (card as f64 + 1000.0)).min(weights.cardinality_penalty);
            }
        }
        if let Some(stats) = numeric_stats {
            if let Some(std) = stats.std {
                if std > 1e-9 {
                    score -= (stats.outlier_count as f64 / (stats.outlier_count as f64 + 100.0))
                        * weights.outlier_penalty;
                }
            }
        }
        score -= issues.len() as f64 * 0.05;
        if null_percentage > 0.5 && cardinality.is_some_and(|c| c > 100) {
            score -= weights.interaction_factor;
        }
        score.clamp(0.0, 1.0)
    }
    pub fn get_dataset_summary(&self, profiles: &[DimensionProfile]) -> DatasetSummary {
        let total_dimensions = profiles.len();
        let (numeric_count, categorical_count, temporal_count) =
            profiles
                .iter()
                .fold((0, 0, 0), |(num, cat, temp), p| match p.data_type {
                    DataType::Numeric => (num + 1, cat, temp),
                    DataType::Categorical => (num, cat + 1, temp),
                    DataType::Temporal => (num, cat, temp + 1),
                });
        let avg_quality_score = if total_dimensions > 0 {
            profiles.iter().map(|p| p.quality_score).sum::<f64>() / total_dimensions as f64
        } else {
            0.0
        };
        let total_issues = profiles.iter().map(|p| p.issues.len()).sum();
        let chart_readiness_score = self.calculate_chart_readiness_score(profiles);
        DatasetSummary {
            total_dimensions,
            numeric_count,
            categorical_count,
            temporal_count,
            avg_quality_score,
            total_issues,
            chart_readiness_score,
        }
    }
    fn calculate_chart_readiness_score(&self, profiles: &[DimensionProfile]) -> f64 {
        if profiles.is_empty() {
            return 0.0;
        }
        let mut score = 0.0;
        score +=
            (profiles.iter().map(|p| p.quality_score).sum::<f64>() / profiles.len() as f64) * 0.5;
        let has_numeric = profiles
            .iter()
            .any(|p| matches!(p.data_type, DataType::Numeric));
        let has_categorical = profiles
            .iter()
            .any(|p| matches!(p.data_type, DataType::Categorical));
        let has_temporal = profiles
            .iter()
            .any(|p| matches!(p.data_type, DataType::Temporal));
        score += ([has_numeric, has_categorical, has_temporal]
            .iter()
            .filter(|&&x| x)
            .count() as f64
            / 3.0)
            * 0.3;
        score += (match profiles.len() {
            0 => 0.0,
            1 => 0.3,
            2..=8 => 1.0,
            9..=15 => 0.8,
            _ => 0.6,
        }) * 0.2;
        score.min(1.0)
    }
    pub fn export_profiles_json(
        &self,
        profiles: &[DimensionProfile],
    ) -> Result<String, ProfilerError> {
        serde_json::to_string_pretty(profiles)
            .map_err(|e| ProfilerError::Parsing(format!("JSON serialisation failed: {e}")))
    }
    pub fn export_summary_json(&self, summary: &DatasetSummary) -> Result<String, ProfilerError> {
        serde_json::to_string_pretty(summary)
            .map_err(|e| ProfilerError::Parsing(format!("JSON serialisation failed: {e}")))
    }
}
impl Default for DataProfiler {
    fn default() -> Self {
        Self::new()
    }
}
impl DimensionProfile {
    pub fn is_axis_suitable(&self) -> bool {
        self.quality_score > 0.7 && self.null_percentage < 0.3
    }
    pub fn is_color_suitable(&self) -> bool {
        matches!(self.data_type, DataType::Categorical)
            && self.cardinality.is_some_and(|c| c <= 10)
            && self.quality_score > 0.6
    }
    pub fn is_size_suitable(&self) -> bool {
        matches!(self.data_type, DataType::Numeric)
            && self
                .numeric_stats
                .as_ref()
                .is_some_and(|stats| stats.min.unwrap_or(-1.0) >= 0.0)
            && self.quality_score > 0.7
    }
    pub fn recommended_chart_roles(&self) -> Vec<String> {
        let mut roles = Vec::new();
        match self.data_type {
            DataType::Numeric => {
                roles.push("y-axis".to_string());
                roles.push("x-axis".to_string());
                if self.is_size_suitable() {
                    roles.push("size".to_string());
                }
            }
            DataType::Categorical => {
                roles.push("x-axis".to_string());
                if self.is_color_suitable() {
                    roles.push("colour".to_string());
                }
                roles.push("facet".to_string());
            }
            DataType::Temporal => {
                roles.push("x-axis".to_string());
                roles.push("timeline".to_string());
            }
        }
        roles
    }
    pub fn quality_description(&self) -> String {
        if self.issues.is_empty() {
            match self.quality_score {
                score if score > 0.8 => "Excellent".to_string(),
                score if score > 0.6 => "Good".to_string(),
                score if score > 0.4 => "Fair".to_string(),
                _ => "Poor".to_string(),
            }
        } else {
            format!("Issues: {}", self.issues.join(", "))
        }
    }
}
impl DatasetSummary {
    pub fn is_chart_ready(&self) -> bool {
        self.chart_readiness_score > 0.6
            && self.total_dimensions >= 2
            && self.avg_quality_score > 0.5
    }
    pub fn data_type_distribution(&self) -> HashMap<String, usize> {
        let mut dist = HashMap::new();
        dist.insert("Numeric".to_string(), self.numeric_count);
        dist.insert("Categorical".to_string(), self.categorical_count);
        dist.insert("Temporal".to_string(), self.temporal_count);
        dist
    }
    pub fn get_chart_recommendations(&self) -> Vec<String> {
        let mut recs = Vec::new();
        if self.numeric_count >= 2 {
            recs.push("scatter".to_string());
        }
        if self.categorical_count >= 1 && self.numeric_count >= 1 {
            recs.push("bar".to_string());
        }
        if self.temporal_count >= 1 && self.numeric_count >= 1 {
            recs.push("line".to_string());
        }
        if self.numeric_count >= 1 {
            recs.push("histogram".to_string());
        }
        if self.categorical_count >= 1 && self.numeric_count >= 1 {
            recs.push("box".to_string());
        }
        if self.categorical_count >= 1 {
            recs.push("pie".to_string());
        }
        recs
    }
    pub fn report(&self) -> String {
        let mut report = String::new();
        report.push_str("Dataset Summary\n===============\n");
        report.push_str(&format!("Total Dimensions: {}\n", self.total_dimensions));
        report.push_str(&format!("  - Numeric: {}\n", self.numeric_count));
        report.push_str(&format!("  - Categorical: {}\n", self.categorical_count));
        report.push_str(&format!("  - Temporal: {}\n", self.temporal_count));
        report.push_str("\nQuality Metrics:\n");
        report.push_str(&format!(
            "  - Average Quality Score: {:.2}\n",
            self.avg_quality_score
        ));
        report.push_str(&format!(
            "  - Chart Readiness Score: {:.2}\n",
            self.chart_readiness_score
        ));
        report.push_str(&format!("  - Total Issues: {}\n", self.total_issues));
        report.push_str(&format!(
            "\nChart Ready: {}\n",
            if self.is_chart_ready() { "Yes" } else { "No" }
        ));
        let recs = self.get_chart_recommendations();
        if !recs.is_empty() {
            report.push_str(&format!("\nRecommended Charts: {}\n", recs.join(", ")));
        }
        report
    }
}
impl std::fmt::Display for DimensionProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({:?}, quality: {:.2})",
            self.name, self.data_type, self.quality_score
        )
    }
}
impl std::fmt::Display for DatasetSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Dataset: {} dimensions, quality: {:.2}, chart readiness: {:.2}",
            self.total_dimensions, self.avg_quality_score, self.chart_readiness_score
        )
    }
}
impl ProfilingConfig {
    pub fn for_large_datasets() -> Self {
        Self {
            max_sample_values: 50,
            enable_advanced_stats: false,
            ..Default::default()
        }
    }
    pub fn for_high_quality() -> Self {
        Self {
            max_sample_values: 200,
            enable_advanced_stats: true,
            type_confidence_threshold: 0.9,
            quality_weights: QualityWeights {
                null_penalty: 0.4,
                cardinality_penalty: 0.15,
                type_confidence_bonus: 0.3,
                outlier_penalty: 0.2,
                interaction_factor: 0.15,
            },
            ..Default::default()
        }
    }
    pub fn for_fast_profiling() -> Self {
        Self {
            max_sample_values: 20,
            enable_advanced_stats: false,
            type_confidence_threshold: 0.7,
            temporal_formats: vec!["%Y-%m-%d".to_string(), "%Y-%m-%d %H:%M:%S".to_string()],
            ..Default::default()
        }
    }
}
