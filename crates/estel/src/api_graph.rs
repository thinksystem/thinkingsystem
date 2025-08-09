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

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "PascalCase")]
pub enum DataType {
    Numeric,
    Categorical,
    Temporal,
}
impl DataType {
    pub fn is_numeric(&self) -> bool {
        matches!(self, DataType::Numeric)
    }
    pub fn is_categorical(&self) -> bool {
        matches!(self, DataType::Categorical)
    }
    pub fn is_temporal(&self) -> bool {
        matches!(self, DataType::Temporal)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgSpec {
    pub data_type: DataTypeSpec,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DataTypeSpec {
    Single(DataType),
    Multiple(Vec<DataType>),
}
impl DataTypeSpec {
    pub fn accepts(&self, data_type: &DataType) -> bool {
        match self {
            DataTypeSpec::Single(dt) => dt == data_type,
            DataTypeSpec::Multiple(types) => types.contains(data_type),
        }
    }
    pub fn accepted_types(&self) -> Vec<&DataType> {
        match self {
            DataTypeSpec::Single(dt) => vec![dt],
            DataTypeSpec::Multiple(types) => types.iter().collect(),
        }
    }
    pub fn primary_type(&self) -> &DataType {
        match self {
            DataTypeSpec::Single(dt) => dt,
            DataTypeSpec::Multiple(types) => types.first().unwrap_or(&DataType::Numeric),
        }
    }
    pub fn is_flexible(&self) -> bool {
        matches!(self, DataTypeSpec::Multiple(_))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartNode {
    pub name: String,
    pub library: String,
    pub description: String,
    pub tags: Vec<String>,
    pub args: HashMap<String, ArgSpec>,
}
impl ChartNode {
    pub fn required_args(&self) -> Vec<(&String, &ArgSpec)> {
        self.args.iter().filter(|(_, spec)| spec.required).collect()
    }
    pub fn optional_args(&self) -> Vec<(&String, &ArgSpec)> {
        self.args
            .iter()
            .filter(|(_, spec)| !spec.required)
            .collect()
    }
    pub fn can_render_with(&self, available_data_types: &HashMap<String, DataType>) -> bool {
        for (_, arg_spec) in self.required_args() {
            let compatible = available_data_types
                .values()
                .any(|dt| arg_spec.data_type.accepts(dt));
            if !compatible {
                return false;
            }
        }
        true
    }
    pub fn has_any_tag(&self, tags: &[&str]) -> bool {
        tags.iter().any(|tag| self.tags.contains(&tag.to_string()))
    }
    pub fn has_all_tags(&self, tags: &[&str]) -> bool {
        tags.iter().all(|tag| self.tags.contains(&tag.to_string()))
    }
    pub fn calculate_semantic_score(
        &self,
        has_temporal: bool,
        numeric_count: usize,
        categorical_count: usize,
    ) -> f64 {
        let mut score: f64 = 0.0;
        if has_temporal {
            if self.has_any_tag(&["timeseries", "temporal", "trend", "time"]) {
                score += 0.5;
            }
            if self.has_any_tag(&["animation"]) && numeric_count > 1 {
                score += 0.3;
            }
        }
        if numeric_count >= 1 {
            if self.has_any_tag(&["distribution", "histogram", "density", "statistical"]) {
                score += 0.4;
            }
            if self.has_any_tag(&["frequency"]) {
                score += 0.3;
            }
        }
        if numeric_count >= 2 {
            if self.has_any_tag(&["relationship", "correlation", "scatter"]) {
                score += 0.5;
            }
            if self.has_any_tag(&["bubble"]) && numeric_count >= 3 {
                score += 0.2;
            }
            if self.has_any_tag(&["regression"]) {
                score += 0.3;
            }
        }
        if categorical_count > 0 {
            if self.has_any_tag(&["comparison", "categorical", "bar", "ranking"]) {
                score += 0.3;
            }
            if self.has_any_tag(&["proportion", "parts-of-whole", "pie"]) {
                score += 0.25;
            }
        }
        if categorical_count > 1 {
            if self.has_any_tag(&["hierarchy", "nested", "treemap", "sunburst"]) {
                score += 0.4;
            }
            if self.has_any_tag(&["flow", "alluvial"]) {
                score += 0.35;
            }
        }
        if self.has_any_tag(&["map", "geospatial", "location", "thematic"]) {
            score += 0.2;
        }
        if numeric_count + categorical_count > 3
            && self.has_any_tag(&["multivariate", "pairwise", "parallel", "matrix"])
        {
            score += 0.3;
        }
        if numeric_count >= 1
            && self.has_any_tag(&["statistical", "summary", "box", "violin", "outlier"])
        {
            score += 0.4;
        }
        if self.has_any_tag(&["business", "funnel", "conversion", "kpi", "dashboard"]) {
            score += 0.25;
        }
        if self.has_any_tag(&["3d", "polar", "ternary", "specialised"]) {
            score += 0.2;
        }
        score.min(1.0)
    }
    pub fn supports_animation(&self) -> bool {
        self.args.contains_key("animation_frame")
            || self.args.contains_key("animation_group")
            || self.has_any_tag(&["animation"])
    }
    pub fn supports_marginals(&self) -> bool {
        self.args.contains_key("marginal_x") || self.args.contains_key("marginal_y")
    }
    pub fn uses_path_hierarchy(&self) -> bool {
        self.args.contains_key("path")
    }
    pub fn uses_traditional_hierarchy(&self) -> bool {
        self.args.contains_key("ids") && self.args.contains_key("parents")
    }
    pub fn supports_faceting(&self) -> bool {
        self.args.contains_key("facet_row") || self.args.contains_key("facet_col")
    }
    pub fn complexity_score(&self) -> f64 {
        let base_complexity = self.args.len() as f64 / 15.0;
        let optional_ratio = self.optional_args().len() as f64 / self.args.len() as f64;
        let has_advanced_features =
            self.has_any_tag(&["3d", "polar", "ternary", "specialised", "advanced"]);
        let has_animation = self.supports_animation();
        let has_faceting = self.supports_faceting();
        let mut complexity = base_complexity;
        complexity += optional_ratio * 0.3;
        if has_advanced_features {
            complexity += 0.4;
        }
        if has_animation {
            complexity += 0.2;
        }
        if has_faceting {
            complexity += 0.15;
        }
        complexity.min(1.0)
    }
    pub fn get_arg_info(&self, arg_name: &str) -> Option<ArgumentInfo> {
        self.args.get(arg_name).map(|spec| ArgumentInfo {
            name: arg_name.to_string(),
            data_type: spec.data_type.clone(),
            required: spec.required,
            description: spec.description.clone(),
        })
    }
    pub fn get_all_arg_info(&self) -> Vec<ArgumentInfo> {
        self.args
            .iter()
            .map(|(name, spec)| ArgumentInfo {
                name: name.clone(),
                data_type: spec.data_type.clone(),
                required: spec.required,
                description: spec.description.clone(),
            })
            .collect()
    }
    pub fn is_suitable_for_data(
        &self,
        numeric_count: usize,
        categorical_count: usize,
        temporal_count: usize,
        total_dimensions: usize,
    ) -> bool {
        let required_args = self.required_args();
        let min_required = required_args.len();
        if total_dimensions < min_required {
            return false;
        }
        let needs_numeric = required_args
            .iter()
            .any(|(_, spec)| spec.data_type.accepts(&DataType::Numeric));
        let needs_categorical = required_args
            .iter()
            .any(|(_, spec)| spec.data_type.accepts(&DataType::Categorical));
        let needs_temporal = required_args
            .iter()
            .any(|(_, spec)| spec.data_type.accepts(&DataType::Temporal));
        if needs_numeric && numeric_count == 0 {
            return false;
        }
        if needs_categorical && categorical_count == 0 {
            return false;
        }
        if needs_temporal && temporal_count == 0 {
            return false;
        }
        true
    }
}
#[derive(Debug, Clone)]
pub struct ArgumentInfo {
    pub name: String,
    pub data_type: DataTypeSpec,
    pub required: bool,
    pub description: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
struct ApiConfig {
    charts: Vec<ChartNode>,
}
#[derive(Debug)]
pub struct ApiGraph {
    charts: Vec<ChartNode>,
    chart_by_name: HashMap<String, ChartNode>,
    charts_by_library: HashMap<String, Vec<usize>>,
    charts_by_tag: HashMap<String, Vec<usize>>,
}
impl ApiGraph {
    pub fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref()).with_context(|| {
            format!(
                "Failed to read API config file: {}",
                path.as_ref().display()
            )
        })?;
        Self::from_yaml_string(&content)
    }
    pub fn from_yaml_string(yaml_content: &str) -> Result<Self> {
        let config: ApiConfig =
            serde_yaml::from_str(yaml_content).context("Failed to parse API config YAML")?;
        let mut chart_by_name = HashMap::new();
        let mut charts_by_library: HashMap<String, Vec<usize>> = HashMap::new();
        let mut charts_by_tag: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, chart) in config.charts.iter().enumerate() {
            if chart_by_name
                .insert(chart.name.clone(), chart.clone())
                .is_some()
            {
                anyhow::bail!("Duplicate chart name found: {}", chart.name);
            }
            charts_by_library
                .entry(chart.library.clone())
                .or_default()
                .push(idx);
            for tag in &chart.tags {
                charts_by_tag.entry(tag.clone()).or_default().push(idx);
            }
        }
        Ok(ApiGraph {
            charts: config.charts,
            chart_by_name,
            charts_by_library,
            charts_by_tag,
        })
    }
    pub fn get_all_charts(&self) -> &[ChartNode] {
        &self.charts
    }
    pub fn get_chart(&self, name: &str) -> Option<&ChartNode> {
        self.chart_by_name.get(name)
    }
    pub fn get_charts_by_library(&self, library: &str) -> Vec<&ChartNode> {
        if let Some(indices) = self.charts_by_library.get(library) {
            indices.iter().map(|&idx| &self.charts[idx]).collect()
        } else {
            Vec::new()
        }
    }
    pub fn get_libraries(&self) -> Vec<String> {
        self.charts_by_library.keys().cloned().collect()
    }
    pub fn get_charts_by_tags(&self, tags: &[&str]) -> Vec<&ChartNode> {
        if tags.is_empty() {
            return Vec::new();
        }
        let mut result_indices: Option<Vec<usize>> = None;
        for tag in tags {
            if let Some(indices) = self.charts_by_tag.get(*tag) {
                match result_indices {
                    None => result_indices = Some(indices.clone()),
                    Some(ref mut existing) => {
                        existing.extend(indices);
                        existing.sort_unstable();
                        existing.dedup();
                    }
                }
            }
        }
        result_indices
            .unwrap_or_default()
            .iter()
            .map(|&idx| &self.charts[idx])
            .collect()
    }
    pub fn get_compatible_charts(
        &self,
        available_data_types: &HashMap<String, DataType>,
    ) -> Vec<&ChartNode> {
        self.charts
            .iter()
            .filter(|chart| chart.can_render_with(available_data_types))
            .collect()
    }
    pub fn get_compatible_charts_detailed(
        &self,
        available_data_types: &HashMap<String, DataType>,
    ) -> Vec<ChartCompatibility> {
        self.charts
            .iter()
            .map(|chart| {
                let mut missing_required = Vec::new();
                let mut available_mappings = Vec::new();
                let mut optional_mappings = Vec::new();
                for (arg_name, arg_spec) in &chart.args {
                    let accepted_types = arg_spec.data_type.accepted_types();
                    let compatible_types: Vec<_> = accepted_types
                        .iter()
                        .filter(|dt| {
                            available_data_types
                                .values()
                                .any(|available| available == **dt)
                        })
                        .collect();
                    if compatible_types.is_empty() && arg_spec.required {
                        missing_required.push(MissingArgument {
                            name: arg_name.clone(),
                            required_types: accepted_types
                                .iter()
                                .map(|dt| format!("{dt:?}"))
                                .collect(),
                            description: arg_spec.description.clone(),
                        });
                    } else if !compatible_types.is_empty() {
                        let mapping = AvailableMapping {
                            name: arg_name.clone(),
                            compatible_types: compatible_types
                                .iter()
                                .map(|dt| format!("{dt:?}"))
                                .collect(),
                            description: arg_spec.description.clone(),
                            required: arg_spec.required,
                        };
                        if arg_spec.required {
                            available_mappings.push(mapping);
                        } else {
                            optional_mappings.push(mapping);
                        }
                    }
                }
                ChartCompatibility {
                    chart_name: chart.name.clone(),
                    chart_description: chart.description.clone(),
                    tags: chart.tags.clone(),
                    can_render: missing_required.is_empty(),
                    missing_required,
                    available_mappings,
                    optional_mappings,
                    complexity_score: chart.complexity_score(),
                    supports_animation: chart.supports_animation(),
                    supports_faceting: chart.supports_faceting(),
                    supports_marginals: chart.supports_marginals(),
                }
            })
            .collect()
    }
    pub fn get_charts_supporting_data_type(
        &self,
        arg_name: &str,
        data_type: &DataType,
    ) -> Vec<&ChartNode> {
        self.charts
            .iter()
            .filter(|chart| {
                chart
                    .args
                    .get(arg_name)
                    .map(|spec| spec.data_type.accepts(data_type))
                    .unwrap_or(false)
            })
            .collect()
    }
    pub fn get_suitable_charts(
        &self,
        numeric_count: usize,
        categorical_count: usize,
        temporal_count: usize,
    ) -> Vec<&ChartNode> {
        let total_dimensions = numeric_count + categorical_count + temporal_count;
        self.charts
            .iter()
            .filter(|chart| {
                chart.is_suitable_for_data(
                    numeric_count,
                    categorical_count,
                    temporal_count,
                    total_dimensions,
                )
            })
            .collect()
    }
    pub fn get_animation_charts(&self) -> Vec<&ChartNode> {
        self.charts
            .iter()
            .filter(|chart| chart.supports_animation())
            .collect()
    }
    pub fn get_faceting_charts(&self) -> Vec<&ChartNode> {
        self.charts
            .iter()
            .filter(|chart| chart.supports_faceting())
            .collect()
    }
    pub fn get_marginal_charts(&self) -> Vec<&ChartNode> {
        self.charts
            .iter()
            .filter(|chart| chart.supports_marginals())
            .collect()
    }
    pub fn get_hierarchical_charts(&self) -> Vec<&ChartNode> {
        self.charts
            .iter()
            .filter(|chart| chart.uses_path_hierarchy() || chart.uses_traditional_hierarchy())
            .collect()
    }
    pub fn get_charts_by_complexity(
        &self,
        min_complexity: f64,
        max_complexity: f64,
    ) -> Vec<&ChartNode> {
        self.charts
            .iter()
            .filter(|chart| {
                let complexity = chart.complexity_score();
                complexity >= min_complexity && complexity <= max_complexity
            })
            .collect()
    }
    pub fn get_beginner_friendly_charts(&self) -> Vec<&ChartNode> {
        self.get_charts_by_complexity(0.0, 0.4)
    }
    pub fn get_advanced_charts(&self) -> Vec<&ChartNode> {
        self.get_charts_by_complexity(0.6, 1.0)
    }
    pub fn search_charts(&self, query: &str) -> Vec<&ChartNode> {
        let query_lower = query.to_lowercase();
        self.charts
            .iter()
            .filter(|chart| {
                chart.name.to_lowercase().contains(&query_lower)
                    || chart.description.to_lowercase().contains(&query_lower)
                    || chart
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
    pub fn stats(&self) -> ApiGraphStats {
        let total_charts = self.charts.len();
        let libraries = self.get_libraries();
        let mut args_per_chart = Vec::new();
        let mut all_arg_names = std::collections::HashSet::new();
        let mut all_tags = std::collections::HashSet::new();
        let mut complexity_scores = Vec::new();
        let mut animation_count = 0;
        let mut faceting_count = 0;
        let mut marginal_count = 0;
        let mut hierarchical_count = 0;
        for chart in &self.charts {
            args_per_chart.push(chart.args.len());
            all_arg_names.extend(chart.args.keys().cloned());
            all_tags.extend(chart.tags.iter().cloned());
            complexity_scores.push(chart.complexity_score());
            if chart.supports_animation() {
                animation_count += 1;
            }
            if chart.supports_faceting() {
                faceting_count += 1;
            }
            if chart.supports_marginals() {
                marginal_count += 1;
            }
            if chart.uses_path_hierarchy() || chart.uses_traditional_hierarchy() {
                hierarchical_count += 1;
            }
        }
        ApiGraphStats {
            total_charts,
            total_libraries: libraries.len(),
            libraries,
            unique_arg_names: all_arg_names.len(),
            unique_tags: all_tags.len(),
            avg_args_per_chart: if total_charts > 0 {
                args_per_chart.iter().sum::<usize>() as f64 / total_charts as f64
            } else {
                0.0
            },
            avg_complexity: if total_charts > 0 {
                complexity_scores.iter().sum::<f64>() / total_charts as f64
            } else {
                0.0
            },
            animation_support_count: animation_count,
            faceting_support_count: faceting_count,
            marginal_support_count: marginal_count,
            hierarchical_support_count: hierarchical_count,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        let mut names = std::collections::HashSet::new();
        for chart in &self.charts {
            if !names.insert(&chart.name) {
                return Err(format!("Duplicate chart name: {}", chart.name));
            }
        }
        for chart in &self.charts {
            if chart.library.is_empty() {
                return Err(format!("Chart '{}' has empty library", chart.name));
            }
        }
        for chart in &self.charts {
            if chart.args.is_empty() {
                return Err(format!("Chart '{}' has no arguments", chart.name));
            }
            for (arg_name, arg_spec) in &chart.args {
                if arg_name.is_empty() {
                    return Err(format!("Chart '{}' has empty argument name", chart.name));
                }
                if arg_spec.data_type.accepted_types().is_empty() {
                    return Err(format!(
                        "Chart '{}' argument '{}' has no accepted types",
                        chart.name, arg_name
                    ));
                }
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub struct ChartCompatibility {
    pub chart_name: String,
    pub chart_description: String,
    pub tags: Vec<String>,
    pub can_render: bool,
    pub missing_required: Vec<MissingArgument>,
    pub available_mappings: Vec<AvailableMapping>,
    pub optional_mappings: Vec<AvailableMapping>,
    pub complexity_score: f64,
    pub supports_animation: bool,
    pub supports_faceting: bool,
    pub supports_marginals: bool,
}
#[derive(Debug, Clone)]
pub struct MissingArgument {
    pub name: String,
    pub required_types: Vec<String>,
    pub description: Option<String>,
}
#[derive(Debug, Clone)]
pub struct AvailableMapping {
    pub name: String,
    pub compatible_types: Vec<String>,
    pub description: Option<String>,
    pub required: bool,
}
#[derive(Debug)]
pub struct ApiGraphStats {
    pub total_charts: usize,
    pub total_libraries: usize,
    pub libraries: Vec<String>,
    pub unique_arg_names: usize,
    pub unique_tags: usize,
    pub avg_args_per_chart: f64,
    pub avg_complexity: f64,
    pub animation_support_count: usize,
    pub faceting_support_count: usize,
    pub marginal_support_count: usize,
    pub hierarchical_support_count: usize,
}
impl ApiGraphStats {
    pub fn summary(&self) -> String {
        format!(
            "API Graph Summary:\n\
            - Total Charts: {}\n\
            - Libraries: {} ({})\n\
            - Unique Arguments: {}\n\
            - Unique Tags: {}\n\
            - Average Arguments per Chart: {:.1}\n\
            - Average Complexity: {:.2}\n\
            - Animation Support: {} charts\n\
            - Faceting Support: {} charts\n\
            - Marginal Plots: {} charts\n\
            - Hierarchical Charts: {} charts",
            self.total_charts,
            self.total_libraries,
            self.libraries.join(", "),
            self.unique_arg_names,
            self.unique_tags,
            self.avg_args_per_chart,
            self.avg_complexity,
            self.animation_support_count,
            self.faceting_support_count,
            self.marginal_support_count,
            self.hierarchical_support_count
        )
    }
}
