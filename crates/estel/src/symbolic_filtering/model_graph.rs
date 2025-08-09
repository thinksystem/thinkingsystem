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
use super::enhanced_features::GraphAwareFeatureExtractor;
use super::enhanced_symbolic::*;
use super::feature_graph::FeatureExtractor;
use super::models::Model;
pub struct GraphAwareNeuroSymbolicModel {
    pub feature_weights: Vec<f64>,
    pub symbolic_engine: GraphAwareSymbolicEngine,
    pub symbolic_influence: f64,
    pub feature_extractor: GraphAwareFeatureExtractor,
}
impl GraphAwareNeuroSymbolicModel {
    pub fn new() -> Self {
        let feature_extractor = GraphAwareFeatureExtractor::new();
        let feature_count = feature_extractor.feature_count();
        Self {
            feature_weights: vec![0.0; feature_count],
            symbolic_engine: GraphAwareSymbolicEngine::new(),
            symbolic_influence: 0.4,
            feature_extractor,
        }
    }
    pub fn analyse_and_predict(
        &mut self,
        spec: &ChartSpec,
        goal: &AnalysisGoal,
    ) -> EnhancedPrediction {
        self.feature_extractor.update_field_graph(spec);
        let analysis = self.symbolic_engine.analyse_chart_spec(spec);
        let prediction = self.predict(spec, goal);
        let (_, feedback) = self.symbolic_engine.enhanced_evaluate(spec, goal);
        EnhancedPrediction {
            score: prediction,
            analysis,
            feedback,
            confidence: self.compute_confidence(spec, goal),
        }
    }
    fn compute_confidence(&self, spec: &ChartSpec, _goal: &AnalysisGoal) -> f64 {
        let features = self.feature_extractor.extract(spec);
        let feature_variance = features.iter().map(|&f| f * f).sum::<f64>() / features.len() as f64;
        (1.0 - feature_variance).clamp(0.1, 0.95)
    }
}
impl Default for GraphAwareNeuroSymbolicModel {
    fn default() -> Self {
        Self::new()
    }
}
impl Model<GraphAwareFeatureExtractor> for GraphAwareNeuroSymbolicModel {
    fn predict(&self, spec: &ChartSpec, goal: &AnalysisGoal) -> f64 {
        let features = self.feature_extractor.extract(spec);
        let feature_score: f64 = features
            .iter()
            .zip(&self.feature_weights)
            .map(|(f, w)| f * w)
            .sum();
        let (symbolic_adj, _) = self.symbolic_engine.enhanced_evaluate(spec, goal);
        let combined_score = feature_score + (symbolic_adj * self.symbolic_influence);
        1.0 / (1.0 + (-combined_score).exp())
    }
    fn weights(&self) -> &[f64] {
        &self.feature_weights
    }
    fn weights_mut(&mut self) -> &mut [f64] {
        &mut self.feature_weights
    }
    fn feature_extractor(&self) -> &GraphAwareFeatureExtractor {
        &self.feature_extractor
    }
    fn symbolic_engine(&self) -> &super::symbolic::SymbolicEngine {
        &self.symbolic_engine.base_engine
    }
    fn compute_gradients(&self, prediction: f64, target: f64, features: &[f64]) -> Vec<f64> {
        let error = target - prediction;
        let gradient_base = error * prediction * (1.0 - prediction);
        features.iter().map(|&f| gradient_base * f).collect()
    }
}
#[derive(Debug)]
pub struct EnhancedPrediction {
    pub score: f64,
    pub analysis: ChartSpecAnalysis,
    pub feedback: Vec<String>,
    pub confidence: f64,
}
