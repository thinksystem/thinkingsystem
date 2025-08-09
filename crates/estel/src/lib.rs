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

pub mod api_graph;
pub mod chart_matcher;
pub mod data_profiler;
pub mod error;

#[cfg(feature = "data-handler")]
pub mod data_handler;


#[cfg(feature = "symbolic")]
pub mod symbolic_filtering;


#[cfg(feature = "learned-scorer")]
pub mod learned_scorer;

pub use api_graph::{ApiGraph, ArgSpec, ChartNode, DataType, DataTypeSpec};
pub use chart_matcher::{MatchingConfig, RenderSpec};
pub use data_profiler::{DataProfiler, DatasetSummary, DimensionProfile, ProfilingConfig};

pub use error::{ChartSuggestionError, ConfigError, DataError, ErrorReporter, Result};
#[cfg(feature = "learned-scorer")]
pub use learned_scorer::{
    DatasetStats as LearnedDatasetStats, FeatureVector as LearnedFeatureVector, LearnedScorer,
};
use polars::prelude::DataFrame;

pub struct ChartSuggestionSystem {
    api_graph: ApiGraph,
    profiler: DataProfiler,
    matching_config: MatchingConfig,
}
impl ChartSuggestionSystem {
    pub fn new() -> Result<Self> {
        let api_graph = ApiGraph::from_yaml_file("config/plotly_api.yml").map_err(|e| {
            ChartSuggestionError::Config(ConfigError::ValidationFailed {
                reason: format!("Failed to load API config: {e}"),
            })
        })?;
        let profiler = DataProfiler::new();
        let matching_config = MatchingConfig::default();
        Ok(Self {
            api_graph,
            profiler,
            matching_config,
        })
    }
    pub fn with_config(
        api_config_path: &str,
        profiling_config: ProfilingConfig,
        matching_config: MatchingConfig,
    ) -> Result<Self> {
        let api_graph = ApiGraph::from_yaml_file(api_config_path).map_err(|e| {
            ChartSuggestionError::Config(ConfigError::ValidationFailed {
                reason: format!("Failed to load API config: {e}"),
            })
        })?;
        let profiler = DataProfiler::with_config(profiling_config);
        Ok(Self {
            api_graph,
            profiler,
            matching_config,
        })
    }
    pub fn suggest_charts_from_csv(&self, csv_path: &str) -> Result<Vec<RenderSpec>> {
        let profiles = self.profiler.profile_csv(csv_path).map_err(|e| {
            ChartSuggestionError::Data(DataError::LowDataQuality {
                reason: format!("Failed to profile CSV file '{csv_path}': {e}"),
            })
        })?;
        Ok(chart_matcher::find_qualified_charts(
            &profiles,
            &self.api_graph,
            &self.matching_config,
        ))
    }
    pub fn suggest_charts_from_dataframe(&self, df: &DataFrame) -> Result<Vec<RenderSpec>> {
        let profiles = self.profiler.profile_dataframe(df).map_err(|e| {
            ChartSuggestionError::Config(ConfigError::ValidationFailed {
                reason: format!("Failed to profile dataframe: {e}"),
            })
        })?;
        Ok(chart_matcher::find_qualified_charts(
            &profiles,
            &self.api_graph,
            &self.matching_config,
        ))
    }
    pub fn profile_csv(&self, csv_path: &str) -> Result<Vec<DimensionProfile>> {
        self.profiler.profile_csv(csv_path).map_err(|e| {
            ChartSuggestionError::Data(DataError::LowDataQuality {
                reason: format!("Failed to profile CSV file '{csv_path}': {e}"),
            })
        })
    }
    pub fn get_summary(&self, profiles: &[DimensionProfile]) -> DatasetSummary {
        self.profiler.get_dataset_summary(profiles)
    }
    pub fn get_available_charts(&self) -> &[ChartNode] {
        self.api_graph.get_all_charts()
    }
    pub fn get_charts_by_library(&self, library: &str) -> Vec<&ChartNode> {
        self.api_graph.get_charts_by_library(library)
    }
    #[cfg(feature = "learned-scorer")]
    pub fn suggest_charts_reranked_from_csv(
        &self,
        csv_path: &str,
        scorer: &LearnedScorer,
        symbolic_scores: Option<&std::collections::HashMap<String, f64>>,
    ) -> Result<Vec<(RenderSpec, f64)>> {
        let profiles = self.profiler.profile_csv(csv_path).map_err(|e| {
            ChartSuggestionError::Data(DataError::LowDataQuality {
                reason: format!("Failed to profile CSV file '{csv_path}': {e}"),
            })
        })?;
        let specs =
            chart_matcher::find_qualified_charts(&profiles, &self.api_graph, &self.matching_config);
        Ok(scorer.rerank_specs(&profiles, specs, symbolic_scores))
    }

    #[cfg(feature = "learned-scorer")]
    pub fn suggest_charts_reranked_from_dataframe(
        &self,
        df: &polars::prelude::DataFrame,
        scorer: &LearnedScorer,
        symbolic_scores: Option<&std::collections::HashMap<String, f64>>,
    ) -> Result<Vec<(RenderSpec, f64)>> {
        let profiles = self.profiler.profile_dataframe(df).map_err(|e| {
            ChartSuggestionError::Config(ConfigError::ValidationFailed {
                reason: format!("Failed to profile dataframe: {e}"),
            })
        })?;
        let specs =
            chart_matcher::find_qualified_charts(&profiles, &self.api_graph, &self.matching_config);
        Ok(scorer.rerank_specs(&profiles, specs, symbolic_scores))
    }
}
impl Default for ChartSuggestionSystem {
    fn default() -> Self {
        Self::new().expect("Failed to create default chart suggestion system")
    }
}
