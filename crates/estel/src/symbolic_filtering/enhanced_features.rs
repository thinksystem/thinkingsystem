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
use super::feature_graph::{FeatureExtractor, SimpleFeatureExtractor};
use super::field_analysis::*;
pub struct GraphAwareFeatureExtractor {
    base_extractor: SimpleFeatureExtractor,
    field_graph: FieldRelationshipGraph,
}
impl GraphAwareFeatureExtractor {
    pub fn new() -> Self {
        Self {
            base_extractor: SimpleFeatureExtractor,
            field_graph: FieldRelationshipGraph::new(),
        }
    }
    pub fn update_field_graph(&mut self, spec: &ChartSpec) {
        self.field_graph = FieldRelationshipGraph::new();
        for profile in spec.column_profiles.values() {
            self.field_graph.add_field(profile.clone());
        }
        self.field_graph.analyse_relationships(None);
    }
    fn extract_graph_features(&self, spec: &ChartSpec) -> Vec<f64> {
        let mut graph_features = vec![0.0; 12];
        let all_fields: Vec<_> = [vec![spec.x_axis_field.clone()], spec.y_axis_fields.clone()].concat();
        let avg_centrality: f64 = all_fields.iter()
            .filter_map(|field| self.field_graph.get_field_insights(field))
            .map(|insights| insights.centrality_score)
            .sum::<f64>() / all_fields.len().max(1) as f64;
        graph_features[0] = avg_centrality;
        let strong_relationships = self.field_graph.edges.iter()
            .filter(|edge| {
                all_fields.contains(&edge.source) && all_fields.contains(&edge.target) && edge.strength > 0.7
            })
            .count();
        graph_features[1] = (strong_relationships as f64) / (all_fields.len().max(1) as f64);
        graph_features[2] = if self.field_graph.edges.iter().any(|edge| {
            matches!(edge.relationship_type, RelationshipType::Temporal) &&
            (all_fields.contains(&edge.source) || all_fields.contains(&edge.target))
        }) { 1.0 } else { 0.0 };
        graph_features[3] = if self.field_graph.edges.iter().any(|edge| {
            matches!(edge.relationship_type, RelationshipType::Correlation) && edge.strength > 0.5 &&
            (all_fields.contains(&edge.source) || all_fields.contains(&edge.target))
        }) { 1.0 } else { 0.0 };
        let unique_tags: std::collections::HashSet<String> = all_fields.iter()
            .filter_map(|field| self.field_graph.nodes.get(field))
            .flat_map(|node| &node.semantic_tags)
            .cloned()
            .collect();
        graph_features[4] = unique_tags.len() as f64 / 10.0;
        if let Some(x_insights) = self.field_graph.get_field_insights(&spec.x_axis_field) {
            graph_features[5] = x_insights.visualisation_potential.as_x_axis;
        }
        let y_axis_suitability: f64 = spec.y_axis_fields.iter()
            .filter_map(|field| self.field_graph.get_field_insights(field))
            .map(|insights| insights.visualisation_potential.as_y_axis)
            .sum::<f64>() / spec.y_axis_fields.len().max(1) as f64;
        graph_features[6] = y_axis_suitability;
        if let Some(ref colour_field) = spec.colour_field {
            if let Some(color_insights) = self.field_graph.get_field_insights(colour_field) {
                graph_features[7] = color_insights.visualisation_potential.as_colour_encoding;
            }
        }
        let relationship_compatibility = self.assess_chart_relationship_compatibility(spec);
        graph_features[8] = relationship_compatibility.correlation_compatibility;
        graph_features[9] = relationship_compatibility.temporal_compatibility;
        graph_features[10] = relationship_compatibility.categorical_compatibility;
        graph_features[11] = relationship_compatibility.hierarchical_compatibility;
        graph_features
    }
    fn assess_chart_relationship_compatibility(&self, spec: &ChartSpec) -> RelationshipCompatibility {
        let mut compatibility = RelationshipCompatibility {
            correlation_compatibility: 0.0,
            temporal_compatibility: 0.0,
            categorical_compatibility: 0.0,
            hierarchical_compatibility: 0.0,
        };
        let all_fields: Vec<_> = [vec![spec.x_axis_field.clone()], spec.y_axis_fields.clone()].concat();
        for edge in &self.field_graph.edges {
            if all_fields.contains(&edge.source) && all_fields.contains(&edge.target) {
                let compatibility_score = edge.strength * match spec.chart_type {
                    ChartType::Scatter => 1.0,
                    ChartType::Line => 0.8,
                    ChartType::Bar => 0.6,
                    _ => 0.4,
                };
                match edge.relationship_type {
                    RelationshipType::Correlation => {
                        compatibility.correlation_compatibility = compatibility.correlation_compatibility.max(compatibility_score);
                    }
                    RelationshipType::Temporal => {
                        compatibility.temporal_compatibility = compatibility.temporal_compatibility.max(compatibility_score);
                    }
                    RelationshipType::Similarity | RelationshipType::Complement => {
                        compatibility.categorical_compatibility = compatibility.categorical_compatibility.max(compatibility_score);
                    }
                    RelationshipType::Hierarchy => {
                        compatibility.hierarchical_compatibility = compatibility.hierarchical_compatibility.max(compatibility_score);
                    }
                    _ => {}
                }
            }
        }
        compatibility
    }
}
impl FeatureExtractor for GraphAwareFeatureExtractor {
    fn feature_count(&self) -> usize {
        self.base_extractor.feature_count() + 12
    }
    fn extract(&self, spec: &ChartSpec) -> Vec<f64> {
        let mut features = self.base_extractor.extract(spec);
        let graph_features = self.extract_graph_features(spec);
        features.extend(graph_features);
        features
    }
}
impl Default for GraphAwareFeatureExtractor {
    fn default() -> Self { Self::new() }
}
#[derive(Debug, Clone)]
struct RelationshipCompatibility {
    correlation_compatibility: f64,
    temporal_compatibility: f64,
    categorical_compatibility: f64,
    hierarchical_compatibility: f64,
}
