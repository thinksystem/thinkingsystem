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

use super::*;
pub fn evaluate_model<F: FeatureExtractor>(
    model: &NeuroSymbolicModel<F>,
    examples: &[TrainingExample]
) -> ModelEvaluation {
    let mut predictions = Vec::new();
    let mut targets = Vec::new();
    let mut errors = Vec::new();
    for example in examples {
        let prediction = model.predict(&example.spec, &example.analysis_goal);
        let error = (example.expert_score - prediction).abs();
        predictions.push(prediction);
        targets.push(example.expert_score);
        errors.push(error);
    }
    let mse = errors.iter().map(|&e| e * e).sum::<f64>() / errors.len() as f64;
    let mae = errors.iter().sum::<f64>() / errors.len() as f64;
    let target_mean = targets.iter().sum::<f64>() / targets.len() as f64;
    let ss_tot: f64 = targets.iter().map(|&t| (t - target_mean).powi(2)).sum();
    let ss_res: f64 = errors.iter().map(|&e| e * e).sum();
    let r_squared = 1.0 - (ss_res / ss_tot);
    ModelEvaluation {
        mse,
        mae,
        r_squared,
        predictions,
        targets,
    }
}
#[derive(Debug)]
pub struct ModelEvaluation {
    pub mse: f64,
    pub mae: f64,
    pub r_squared: f64,
    pub predictions: Vec<f64>,
    pub targets: Vec<f64>,
}
