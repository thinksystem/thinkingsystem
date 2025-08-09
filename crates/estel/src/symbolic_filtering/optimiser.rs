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

pub trait Optimiser {
    fn step(&mut self, weights: &mut [f64], gradients: &[f64]);
    fn reset(&mut self);
    fn learning_rate(&self) -> f64;
    fn set_learning_rate(&mut self, lr: f64);
}
#[derive(Debug, Clone)]
pub enum OptimiserConfig {
    Adam { learning_rate: f64, beta1: f64, beta2: f64, epsilon: f64 },
    SGD { learning_rate: f64, momentum: f64 },
    RMSprop { learning_rate: f64, decay: f64, epsilon: f64 },
    AdaGrad { learning_rate: f64, epsilon: f64 },
}
impl Default for OptimiserConfig {
    fn default() -> Self {
        OptimiserConfig::Adam {
            learning_rate: 0.001,
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
        }
    }
}
pub fn create_optimiser(config: OptimiserConfig, size: usize) -> Box<dyn Optimiser> {
    match config {
        OptimiserConfig::Adam { learning_rate, beta1, beta2, epsilon } => {
            Box::new(Adam::new(size, learning_rate, beta1, beta2, epsilon))
        }
        OptimiserConfig::SGD { learning_rate, momentum } => {
            Box::new(SGD::new(size, learning_rate, momentum))
        }
        OptimiserConfig::RMSprop { learning_rate, decay, epsilon } => {
            Box::new(RMSprop::new(size, learning_rate, decay, epsilon))
        }
        OptimiserConfig::AdaGrad { learning_rate, epsilon } => {
            Box::new(AdaGrad::new(size, learning_rate, epsilon))
        }
    }
}
pub struct Adam {
    learning_rate: f64,
    beta1: f64,
    beta2: f64,
    epsilon: f64,
    m: Vec<f64>,
    v: Vec<f64>,
    t: u32,
}
impl Adam {
    pub fn new(size: usize, learning_rate: f64, beta1: f64, beta2: f64, epsilon: f64) -> Self {
        Self {
            learning_rate,
            beta1,
            beta2,
            epsilon,
            m: vec![0.0; size],
            v: vec![0.0; size],
            t: 0,
        }
    }
}
impl Optimiser for Adam {
    fn step(&mut self, weights: &mut [f64], gradients: &[f64]) {
        self.t += 1;
        for i in 0..weights.len() {
            self.m[i] = self.beta1 * self.m[i] + (1.0 - self.beta1) * gradients[i];
            self.v[i] = self.beta2 * self.v[i] + (1.0 - self.beta2) * gradients[i].powi(2);
            let m_hat = self.m[i] / (1.0 - self.beta1.powi(self.t as i32));
            let v_hat = self.v[i] / (1.0 - self.beta2.powi(self.t as i32));
            weights[i] -= self.learning_rate * m_hat / (v_hat.sqrt() + self.epsilon);
        }
    }
    fn reset(&mut self) {
        self.m.fill(0.0);
        self.v.fill(0.0);
        self.t = 0;
    }
    fn learning_rate(&self) -> f64 { self.learning_rate }
    fn set_learning_rate(&mut self, lr: f64) { self.learning_rate = lr; }
}
pub struct SGD {
    learning_rate: f64,
    momentum: f64,
    velocity: Vec<f64>,
}
impl SGD {
    pub fn new(size: usize, learning_rate: f64, momentum: f64) -> Self {
        Self {
            learning_rate,
            momentum,
            velocity: vec![0.0; size],
        }
    }
}
impl Optimiser for SGD {
    fn step(&mut self, weights: &mut [f64], gradients: &[f64]) {
        for i in 0..weights.len() {
            self.velocity[i] = self.momentum * self.velocity[i] - self.learning_rate * gradients[i];
            weights[i] += self.velocity[i];
        }
    }
    fn reset(&mut self) {
        self.velocity.fill(0.0);
    }
    fn learning_rate(&self) -> f64 { self.learning_rate }
    fn set_learning_rate(&mut self, lr: f64) { self.learning_rate = lr; }
}
pub struct RMSprop {
    learning_rate: f64,
    decay: f64,
    epsilon: f64,
    cache: Vec<f64>,
}
impl RMSprop {
    pub fn new(size: usize, learning_rate: f64, decay: f64, epsilon: f64) -> Self {
        Self {
            learning_rate,
            decay,
            epsilon,
            cache: vec![0.0; size],
        }
    }
}
impl Optimiser for RMSprop {
    fn step(&mut self, weights: &mut [f64], gradients: &[f64]) {
        for i in 0..weights.len() {
            self.cache[i] = self.decay * self.cache[i] + (1.0 - self.decay) * gradients[i].powi(2);
            weights[i] -= self.learning_rate * gradients[i] / (self.cache[i].sqrt() + self.epsilon);
        }
    }
    fn reset(&mut self) {
        self.cache.fill(0.0);
    }
    fn learning_rate(&self) -> f64 { self.learning_rate }
    fn set_learning_rate(&mut self, lr: f64) { self.learning_rate = lr; }
}
pub struct AdaGrad {
    learning_rate: f64,
    epsilon: f64,
    sum_squared_gradients: Vec<f64>,
}
impl AdaGrad {
    pub fn new(size: usize, learning_rate: f64, epsilon: f64) -> Self {
        Self {
            learning_rate,
            epsilon,
            sum_squared_gradients: vec![0.0; size],
        }
    }
}
impl Optimiser for AdaGrad {
    fn step(&mut self, weights: &mut [f64], gradients: &[f64]) {
        for i in 0..weights.len() {
            self.sum_squared_gradients[i] += gradients[i].powi(2);
            weights[i] -= self.learning_rate * gradients[i] /
                (self.sum_squared_gradients[i].sqrt() + self.epsilon);
        }
    }
    fn reset(&mut self) {
        self.sum_squared_gradients.fill(0.0);
    }
    fn learning_rate(&self) -> f64 { self.learning_rate }
    fn set_learning_rate(&mut self, lr: f64) { self.learning_rate = lr; }
}
