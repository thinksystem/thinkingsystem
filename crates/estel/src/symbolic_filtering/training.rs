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

use super::data_structures::TrainingExample;
use super::feature_graph::FeatureExtractor;
use super::models::Model;
use super::optimiser::{Optimiser, OptimiserConfig, create_optimiser};
pub struct TrainingConfig {
    pub epochs: u32,
    pub batch_size: usize,
    pub validation_split: f64,
    pub early_stopping_patience: Option<u32>,
    pub learning_rate_schedule: Option<LearningRateSchedule>,
}
impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            epochs: 100,
            batch_size: 32,
            validation_split: 0.2,
            early_stopping_patience: Some(10),
            learning_rate_schedule: None,
        }
    }
}
#[derive(Debug, Clone)]
pub enum LearningRateSchedule {
    StepDecay { step_size: u32, gamma: f64 },
    ExponentialDecay { gamma: f64 },
}
pub struct Trainer<'a, M: Model<F>, F: FeatureExtractor> {
    model: &'a mut M,
    optimiser: Box<dyn Optimiser>,
    config: TrainingConfig,
    _phantom: std::marker::PhantomData<F>,
}
impl<'a, M: Model<F>, F: FeatureExtractor> Trainer<'a, M, F> {
    pub fn new(model: &'a mut M, optimiser_config: OptimiserConfig, config: TrainingConfig) -> Self {
        let optimiser = create_optimiser(optimiser_config, model.weights().len());
        Self {
            model,
            optimiser,
            config,
            _phantom: std::marker::PhantomData
        }
    }
    pub fn train(&mut self, dataset: &[TrainingExample]) -> TrainingResult {
        let (train_data, val_data) = self.split_data(dataset);
        let mut best_val_loss = f64::INFINITY;
        let mut patience_counter = 0;
        let mut training_losses = Vec::new();
        let mut validation_losses = Vec::new();
        for epoch in 0..self.config.epochs {
            if let Some(schedule) = self.config.learning_rate_schedule.clone() {
                self.apply_lr_schedule(&schedule, epoch);
            }
            let train_loss = self.train_epoch(&train_data);
            let val_loss = self.validate(&val_data);
            training_losses.push(train_loss);
            validation_losses.push(val_loss);
            if (epoch + 1) % 20 == 0 {
                println!("Epoch {}: Train Loss: {:.4}, Val Loss: {:.4}",
                         epoch + 1, train_loss, val_loss);
            }
            if let Some(patience) = self.config.early_stopping_patience {
                if val_loss < best_val_loss {
                    best_val_loss = val_loss;
                    patience_counter = 0;
                } else {
                    patience_counter += 1;
                    if patience_counter >= patience {
                        println!("Early stopping at epoch {}", epoch + 1);
                        break;
                    }
                }
            }
        }
        let final_train_loss = *training_losses.last().unwrap_or(&0.0);
        let final_val_loss = *validation_losses.last().unwrap_or(&0.0);
        TrainingResult {
            training_losses,
            validation_losses,
            final_train_loss,
            final_val_loss,
        }
    }
    fn train_epoch(&mut self, data: &[TrainingExample]) -> f64 {
        let mut total_loss = 0.0;
        let mut batches_processed = 0;
        for batch in data.chunks(self.config.batch_size) {
            let batch_loss = self.train_batch(batch);
            total_loss += batch_loss;
            batches_processed += 1;
        }
        total_loss / batches_processed as f64
    }
    fn train_batch(&mut self, batch: &[TrainingExample]) -> f64 {
        let mut batch_gradients = vec![0.0; self.model.weights().len()];
        let mut batch_loss = 0.0;
        for example in batch {
            let features = self.model.feature_extractor().extract(&example.spec);
            let prediction = self.model.predict(&example.spec, &example.analysis_goal);
            let loss = (example.expert_score - prediction).powi(2);
            batch_loss += loss;
            let gradients = self.model.compute_gradients(prediction, example.expert_score, &features);
            for (i, gradient) in gradients.iter().enumerate() {
                batch_gradients[i] += gradient;
            }
        }
        for gradient in &mut batch_gradients {
            *gradient /= batch.len() as f64;
        }
        self.optimiser.step(self.model.weights_mut(), &batch_gradients);
        batch_loss / batch.len() as f64
    }
    fn validate(&self, data: &[TrainingExample]) -> f64 {
        let mut total_loss = 0.0;
        for example in data {
            let prediction = self.model.predict(&example.spec, &example.analysis_goal);
            total_loss += (example.expert_score - prediction).powi(2);
        }
        total_loss / data.len() as f64
    }
    fn split_data(&self, dataset: &[TrainingExample]) -> (Vec<TrainingExample>, Vec<TrainingExample>) {
        let split_idx = ((1.0 - self.config.validation_split) * dataset.len() as f64) as usize;
        let (train, val) = dataset.split_at(split_idx);
        (train.to_vec(), val.to_vec())
    }
    fn apply_lr_schedule(&mut self, schedule: &LearningRateSchedule, epoch: u32) {
        let current_lr = self.optimiser.learning_rate();
        let new_lr = match schedule {
            LearningRateSchedule::StepDecay { step_size, gamma } => {
                if epoch % step_size == 0 && epoch > 0 {
                    current_lr * gamma
                } else {
                    current_lr
                }
            }
            LearningRateSchedule::ExponentialDecay { gamma } => {
                current_lr * gamma
            }
        };
        if new_lr != current_lr {
            self.optimiser.set_learning_rate(new_lr);
        }
    }
}
#[derive(Debug)]
pub struct TrainingResult {
    pub training_losses: Vec<f64>,
    pub validation_losses: Vec<f64>,
    pub final_train_loss: f64,
    pub final_val_loss: f64,
}
