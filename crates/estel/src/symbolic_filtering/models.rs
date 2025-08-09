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
use super::feature_graph::FeatureExtractor;
use super::symbolic::SymbolicEngine;
pub trait Model<F: FeatureExtractor> {
    fn predict(&self, spec: &ChartSpec, goal: &AnalysisGoal) -> f64;
    fn weights(&self) -> &[f64];
    fn weights_mut(&mut self) -> &mut [f64];
    fn feature_extractor(&self) -> &F;
    fn compute_gradients(&self, prediction: f64, target: f64, features: &[f64]) -> Vec<f64>;
    fn symbolic_engine(&self) -> &SymbolicEngine;
}
pub struct NeuroSymbolicModel<F: FeatureExtractor> {
    pub feature_weights: Vec<f64>,
    pub symbolic_engine: SymbolicEngine,
    pub symbolic_influence: f64,
    pub feature_extractor: F,
}
impl<F: FeatureExtractor> NeuroSymbolicModel<F> {
    pub fn new(feature_extractor: F) -> Self {
        let feature_count = feature_extractor.feature_count();
        Self {
            feature_weights: vec![0.0; feature_count],
            symbolic_engine: SymbolicEngine::default(),
            symbolic_influence: 0.4,
            feature_extractor,
        }
    }
    pub fn with_symbolic_influence(mut self, influence: f64) -> Self {
        self.symbolic_influence = influence.clamp(0.0, 1.0);
        self
    }
    pub fn set_symbolic_influence(&mut self, influence: f64) {
        self.symbolic_influence = influence.clamp(0.0, 1.0);
    }
    pub fn add_symbolic_rule(&mut self, rule: super::symbolic::SymbolicRule) {
        self.symbolic_engine.add_rule(rule);
    }
}
impl<F: FeatureExtractor> Model<F> for NeuroSymbolicModel<F> {
    fn predict(&self, spec: &ChartSpec, goal: &AnalysisGoal) -> f64 {
        let features = self.feature_extractor.extract(spec);
        let feature_score: f64 = features.iter().zip(&self.feature_weights).map(|(f, w)| f * w).sum();
        let (symbolic_adj, _) = self.symbolic_engine.evaluate(spec, goal);
        let combined_score = feature_score + (symbolic_adj * self.symbolic_influence);
        1.0 / (1.0 + (-combined_score).exp())
    }
    fn weights(&self) -> &[f64] { &self.feature_weights }
    fn weights_mut(&mut self) -> &mut [f64] { &mut self.feature_weights }
    fn feature_extractor(&self) -> &F { &self.feature_extractor }
    fn symbolic_engine(&self) -> &SymbolicEngine { &self.symbolic_engine }
    fn compute_gradients(&self, prediction: f64, target: f64, features: &[f64]) -> Vec<f64> {
        let error = target - prediction;
        let gradient_base = error * prediction * (1.0 - prediction);
        features.iter().map(|&f| gradient_base * f).collect()
    }
}
