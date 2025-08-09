// SPDX-License-Identifier: AGPL-3.0-only
// Graph-aware symbolic engine that wraps the baseline SymbolicEngine and adds
// field-relationship analysis hooks. This is a lightweight adaptor so the
// symbolic feature compiles end-to-end.

use super::data_structures::{AnalysisGoal, ChartSpec, ChartType};
use super::field_analysis::{FieldRelationshipGraph, RelationshipType};
use super::symbolic::SymbolicEngine;

#[derive(Debug, Default)]
pub struct ChartSpecAnalysis {
    pub has_temporal_relation: bool,
    pub strong_correlation_pairs: usize,
    pub categorical_depth_hint: f64,
}

#[derive(Debug, Default)]
pub struct GraphAwareSymbolicEngine {
    pub base_engine: SymbolicEngine,
}

impl GraphAwareSymbolicEngine {
    pub fn new() -> Self {
        Self {
            base_engine: SymbolicEngine::default(),
        }
    }

    pub fn analyse_chart_spec(&self, spec: &ChartSpec) -> ChartSpecAnalysis {
        
        let mut graph = FieldRelationshipGraph::new();
        for profile in spec.column_profiles.values() {
            graph.add_field(profile.clone());
        }
        graph.analyse_relationships(None);
        let has_temporal = graph
            .edges
            .iter()
            .any(|e| matches!(e.relationship_type, RelationshipType::Temporal));
        let strong_corr = graph
            .edges
            .iter()
            .filter(|e| {
                matches!(e.relationship_type, RelationshipType::Correlation) && e.strength > 0.7
            })
            .count();
        let categorical_depth = graph
            .nodes
            .values()
            .filter(|n| {
                matches!(
                    n.profile.data_type,
                    super::data_structures::DataType::Categorical
                )
            })
            .map(|n| n.profile.cardinality.unwrap_or(0) as f64)
            .sum::<f64>()
            / (graph.nodes.len().max(1) as f64 * 10.0);
        ChartSpecAnalysis {
            has_temporal_relation: has_temporal,
            strong_correlation_pairs: strong_corr,
            categorical_depth_hint: categorical_depth.min(1.0),
        }
    }

    pub fn enhanced_evaluate(&self, spec: &ChartSpec, goal: &AnalysisGoal) -> (f64, Vec<String>) {
        let (base_adj, mut feedback) = self.base_engine.evaluate(spec, goal);
        let analysis = self.analyse_chart_spec(spec);
        let mut adj = base_adj;
        
        if analysis.has_temporal_relation
            && matches!(spec.chart_type, ChartType::Line)
            && matches!(goal, AnalysisGoal::ShowTrend)
        {
            adj += 0.1;
            feedback.push("Temporal relation supports line chart for trends".to_string());
        }
        if analysis.strong_correlation_pairs > 0
            && matches!(spec.chart_type, ChartType::Scatter)
            && matches!(goal, AnalysisGoal::FindRelationship)
        {
            adj += 0.1;
            feedback.push("Detected correlated fields; scatter is appropriate".to_string());
        }
        if analysis.categorical_depth_hint > 0.8 && matches!(spec.chart_type, ChartType::Pie) {
            adj -= 0.1;
            feedback.push("High categorical depth suggests avoiding pie".to_string());
        }
        (adj, feedback)
    }
}
