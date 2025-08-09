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

use super::data_structures::*;
pub trait FeatureExtractor {
    fn extract(&self, spec: &ChartSpec) -> Vec<f64>;
    fn feature_count(&self) -> usize;
}
pub struct SimpleFeatureExtractor;
pub const FEATURE_COUNT: usize = 6;
impl FeatureExtractor for SimpleFeatureExtractor {
    fn feature_count(&self) -> usize { FEATURE_COUNT }
    fn extract(&self, spec: &ChartSpec) -> Vec<f64> {
        let mut features = vec![0.0; self.feature_count()];
        if spec.chart_type == ChartType::Bar { features[0] = 1.0; }
        if spec.chart_type == ChartType::Line { features[1] = 1.0; }
        if spec.chart_type == ChartType::Scatter { features[2] = 1.0; }
        if spec.y_axis_fields.len() > 1 { features[3] = 1.0; }
        if let Some(profile) = spec.column_profiles.get(&spec.x_axis_field) {
            features[4] = (profile.cardinality.unwrap_or(0) as f64).min(20.0) / 20.0;
        }
        if spec.y_axis_fields.iter().all(|f| spec.column_profiles.get(f).is_some_and(|p| p.data_type == DataType::Numeric)) {
            features[5] = 1.0;
        }
        features
    }
}
