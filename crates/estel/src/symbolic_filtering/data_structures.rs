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
use std::collections::HashMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartSpec {
    pub chart_type: ChartType,
    pub x_axis_field: String,
    pub y_axis_fields: Vec<String>,
    pub colour_field: Option<String>,
    pub column_profiles: HashMap<String, ColumnProfile>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChartType {
    Bar, Line, Scatter, Pie, Histogram, BoxPlot,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnProfile {
    pub name: String,
    pub data_type: DataType,
    pub cardinality: Option<u64>,
    pub has_nulls: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    Numeric, Categorical, Temporal, Boolean,
}
#[derive(Debug, Clone)]
pub struct TrainingExample {
    pub spec: ChartSpec,
    pub expert_score: f64,
    pub analysis_goal: AnalysisGoal,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AnalysisGoal {
    Compare, ShowTrend, ShowDistribution, FindRelationship, ShowComposition,
}
