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

use super::memory_components::Experience;
use crate::nlu::orchestrator::data_models::Action;
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use std::collections::{BTreeMap, HashMap};
extern crate libm;
const ACTION_EMBEDDING_DIM: usize = 32;
const TRANSFORMER_HEADS: usize = 4;
const WORLD_MODEL_HIDDEN_DIM: usize = 128;
#[derive(Debug, Clone, Copy)]
pub struct RegularisationConfig {
    pub dropout_rate: f32,
    pub l2_penalty: f32,
    pub l1_penalty: f32,
    pub gradient_clip_norm: f32,
}
impl Default for RegularisationConfig {
    fn default() -> Self {
        Self {
            dropout_rate: 0.1,
            l2_penalty: 1e-4,
            l1_penalty: 1e-5,
            gradient_clip_norm: 1.0,
        }
    }
}
fn apply_dropout(input: &mut [f32], rate: f32) {
    if rate <= 0.0 {
        return;
    }
    let mut rng = rand::thread_rng();
    let scale = 1.0 / (1.0 - rate);
    for val in input.iter_mut() {
        if rng.gen::<f32>() < rate {
            *val = 0.0;
        } else {
            *val *= scale;
        }
    }
}
#[derive(Debug, Clone)]
struct LayerNorm {
    gamma: Vec<f32>,
    beta: Vec<f32>,
    epsilon: f32,
}
impl LayerNorm {
    fn new(dim: usize) -> Self {
        Self {
            gamma: vec![1.0; dim],
            beta: vec![0.0; dim],
            epsilon: 1e-5,
        }
    }
    fn forward(&self, input: &mut [f32]) {
        let mean = input.iter().sum::<f32>() / input.len() as f32;
        let variance = input.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / input.len() as f32;
        let std_dev = (variance + self.epsilon).sqrt();
        for (i, val) in input.iter_mut().enumerate() {
            *val = self.gamma[i] * (*val - mean) / std_dev + self.beta[i];
        }
    }
}
#[derive(Debug, Clone)]
struct AttentionHead {
    weights_q: Vec<f32>,
    weights_k: Vec<f32>,
    weights_v: Vec<f32>,
    weights_o: Vec<f32>,
    dim: usize,
    head_dim: usize,
}
impl AttentionHead {
    fn new(dim: usize, head_dim: usize, _reg_config: &RegularisationConfig) -> Self {
        let mut rng = rand::thread_rng();
        let scale = (1.0 / dim as f32).sqrt();
        Self {
            weights_q: (0..dim * head_dim)
                .map(|_| rng.gen_range(-scale..scale))
                .collect(),
            weights_k: (0..dim * head_dim)
                .map(|_| rng.gen_range(-scale..scale))
                .collect(),
            weights_v: (0..dim * head_dim)
                .map(|_| rng.gen_range(-scale..scale))
                .collect(),
            weights_o: (0..head_dim * dim)
                .map(|_| rng.gen_range(-scale..scale))
                .collect(),
            dim,
            head_dim,
        }
    }
    fn forward(&self, sequence: &[Vec<f32>]) -> Vec<Vec<f32>> {
        let seq_len = sequence.len();
        if seq_len == 0 {
            return vec![];
        }
        let q_s: Vec<Vec<f32>> = sequence
            .iter()
            .map(|emb| linear(emb, &self.weights_q, self.dim, self.head_dim))
            .collect();
        let k_s: Vec<Vec<f32>> = sequence
            .iter()
            .map(|emb| linear(emb, &self.weights_k, self.dim, self.head_dim))
            .collect();
        let v_s: Vec<Vec<f32>> = sequence
            .iter()
            .map(|emb| linear(emb, &self.weights_v, self.dim, self.head_dim))
            .collect();
        let scale_factor = (self.head_dim as f32).sqrt();
        let mut attention_scores = vec![vec![0.0; seq_len]; seq_len];
        for i in 0..seq_len {
            for (j, k_vec) in k_s.iter().enumerate() {
                let dot_product = q_s[i].iter().zip(k_vec).map(|(q, k)| q * k).sum::<f32>();
                attention_scores[i][j] = dot_product / scale_factor;
            }
            softmax(&mut attention_scores[i]);
        }
        let mut output_sequence = vec![vec![0.0; self.head_dim]; seq_len];
        for i in 0..seq_len {
            for (j, v_vec) in v_s.iter().enumerate() {
                for (d, &v_val) in v_vec.iter().enumerate() {
                    output_sequence[i][d] += attention_scores[i][j] * v_val;
                }
            }
        }

        let final_output: Vec<Vec<f32>> = output_sequence
            .iter()
            .map(|head_output| linear(head_output, &self.weights_o, self.head_dim, self.dim))
            .collect();

        final_output
    }
}
#[derive(Debug, Clone)]
pub struct MiniTransformer {
    attention_heads: Vec<AttentionHead>,
    output_projection: Vec<f32>,
    layer_norm1: LayerNorm,
    layer_norm2: LayerNorm,
    dropout_rate: f32,
    dim: usize,
}
impl MiniTransformer {
    fn new(dim: usize, num_heads: usize, reg_config: &RegularisationConfig) -> Self {
        assert_eq!(
            dim % num_heads,
            0,
            "Embedding dimension must be divisible by number of heads"
        );
        let head_dim = dim / num_heads;
        let mut rng = rand::thread_rng();
        let scale = (1.0 / dim as f32).sqrt();
        Self {
            attention_heads: (0..num_heads)
                .map(|_| AttentionHead::new(dim, head_dim, reg_config))
                .collect(),
            output_projection: (0..dim * dim)
                .map(|_| rng.gen_range(-scale..scale))
                .collect(),
            layer_norm1: LayerNorm::new(dim),
            layer_norm2: LayerNorm::new(dim),
            dropout_rate: reg_config.dropout_rate,
            dim,
        }
    }
    fn forward(&self, sequence: &[Vec<f32>]) -> Vec<Vec<f32>> {
        let seq_len = sequence.len();
        if seq_len == 0 {
            return vec![];
        }
        let head_outputs: Vec<Vec<Vec<f32>>> = self
            .attention_heads
            .iter()
            .map(|head| head.forward(sequence))
            .collect();
        let mut concatenated = vec![vec![0.0; self.dim]; seq_len];
        for i in 0..seq_len {
            concatenated[i] = head_outputs
                .iter()
                .flat_map(|head_out| &head_out[i])
                .cloned()
                .collect();
        }
        let mut projected_output: Vec<Vec<f32>> = concatenated
            .iter()
            .map(|c| linear(c, &self.output_projection, self.dim, self.dim))
            .collect();
        let mut final_output = sequence.to_vec();
        for i in 0..seq_len {
            apply_dropout(&mut projected_output[i], self.dropout_rate);
            for d in 0..self.dim {
                final_output[i][d] += projected_output[i][d];
            }
            self.layer_norm1.forward(&mut final_output[i]);
        }

        let mut ffn_output = final_output.clone();
        for i in 0..seq_len {
            let mut ffn_values = vec![0.0; self.dim];
            for (d, ffn_value) in ffn_values.iter_mut().enumerate() {
                *ffn_value = final_output[i]
                    .iter()
                    .enumerate()
                    .map(|(idx, &val)| val * self.output_projection[d * self.dim + idx])
                    .sum::<f32>()
                    .max(0.0);
            }

            for (d, ffn_value) in ffn_values.iter().enumerate() {
                ffn_output[i][d] += ffn_value;
            }
            self.layer_norm2.forward(&mut ffn_output[i]);
        }

        ffn_output
    }
}
#[derive(Debug)]
pub struct EmbeddingService {
    transformer: MiniTransformer,
}
impl EmbeddingService {
    pub fn new(reg_config: &RegularisationConfig) -> Self {
        Self {
            transformer: MiniTransformer::new(ACTION_EMBEDDING_DIM, TRANSFORMER_HEADS, reg_config),
        }
    }
    pub fn get_embedding_dim() -> usize {
        ACTION_EMBEDDING_DIM
    }
    pub fn embed_actions(&self, actions: &[Action]) -> Vec<Vec<f32>> {
        if actions.is_empty() {
            return vec![];
        }
        let pre_embeddings: Vec<Vec<f32>> = actions
            .iter()
            .map(|action| {
                let mut pre_embedding = vec![0.0; ACTION_EMBEDDING_DIM];
                let words: Vec<&str> = action.verb.split_whitespace().collect();
                let word_count = words.len().max(1) as f32;
                for (word_idx, word) in words.iter().enumerate() {
                    let word_hash = word
                        .chars()
                        .fold(0u64, |acc, c| acc.wrapping_mul(31).wrapping_add(c as u64));
                    for i in 0..ACTION_EMBEDDING_DIM {
                        let feature_idx = (word_idx + i) % ACTION_EMBEDDING_DIM;
                        let hash_bit = ((word_hash >> (i % 64)) & 1) as f32;
                        pre_embedding[feature_idx] += hash_bit / word_count;
                    }
                }
                pre_embedding[0] = action.confidence;
                let norm = pre_embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for val in &mut pre_embedding {
                        *val /= norm;
                    }
                }
                pre_embedding
            })
            .collect();
        self.transformer.forward(&pre_embeddings)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionPair {
    cause: String,
    effect: String,
}
#[derive(Debug)]
pub struct CausalDiscoveryModule {
    significant_links: HashMap<ActionPair, f64>,
    significance_level: f64,
    temporal_window_secs: i64,
}
impl CausalDiscoveryModule {
    pub fn new(significance_level: f64, temporal_window_secs: i64) -> Self {
        Self {
            significant_links: HashMap::new(),
            significance_level,
            temporal_window_secs,
        }
    }
    fn build_temporal_index<'a>(
        experiences: &[&'a Experience],
    ) -> BTreeMap<DateTime<Utc>, Vec<&'a Action>> {
        let mut index: BTreeMap<DateTime<Utc>, Vec<&'a Action>> = BTreeMap::new();
        for exp in experiences {
            if let Some(action) = exp.action_sequence.last() {
                index.entry(exp.timestamp).or_default().push(action);
            }
        }
        index
    }
    pub fn discover_patterns(&mut self, experiences: &[&Experience]) {
        if experiences.len() < 2 {
            return;
        }
        let index = Self::build_temporal_index(experiences);
        let mut all_actions = HashMap::new();
        let mut co_occurrences = HashMap::new();
        let mut total_windows = 0;
        for (timestamp, actions) in index.iter() {
            let window_start = *timestamp - Duration::seconds(self.temporal_window_secs);
            for effect_action in actions {
                *all_actions
                    .entry(effect_action.verb.clone())
                    .or_insert(0u64) += 1;
                for (_, cause_actions) in index.range(window_start..*timestamp) {
                    total_windows += 1;
                    for cause_action in cause_actions {
                        if cause_action.verb != effect_action.verb {
                            let pair = ActionPair {
                                cause: cause_action.verb.clone(),
                                effect: effect_action.verb.clone(),
                            };
                            *co_occurrences.entry(pair).or_insert(0u64) += 1;
                        }
                    }
                }
            }
        }
        for (pair, n11) in co_occurrences.iter() {
            let n1_dot = *all_actions.get(&pair.cause).unwrap_or(&0);
            let n_dot1 = *all_actions.get(&pair.effect).unwrap_or(&0);
            if n1_dot == 0 || n_dot1 == 0 {
                continue;
            }
            let (p_value, _) = self::statistical_test(*n11, n1_dot, n_dot1, total_windows);
            if p_value < self.significance_level {
                println!(
                    "[Causal] Found significant link: {} -> {} (p-value: {:.4})",
                    pair.cause, pair.effect, p_value
                );
                self.significant_links.insert(pair.clone(), p_value);
            }
        }
    }
}
pub fn statistical_test(n11: u64, n1_dot: u64, n_dot1: u64, total: u64) -> (f64, f64) {
    if total == 0 {
        return (1.0, 0.0);
    }
    let n11 = n11 as f64;
    let n12 = (n1_dot.saturating_sub(n11 as u64)) as f64;
    let n21 = (n_dot1.saturating_sub(n11 as u64)) as f64;
    let n22 = (total
        .saturating_sub(n1_dot)
        .saturating_sub(n_dot1)
        .saturating_add(n11 as u64)) as f64;
    let total_f = total as f64;
    let n1_dot_f = n1_dot as f64;
    let n2_dot_f = n21 + n22;
    let n_dot1_f = n_dot1 as f64;
    let n_dot2_f = n12 + n22;
    let mut g_stat = 0.0;
    if n11 > 0.0 {
        g_stat += n11 * (n11 * total_f / (n1_dot_f * n_dot1_f)).ln();
    }
    if n12 > 0.0 {
        g_stat += n12 * (n12 * total_f / (n1_dot_f * n_dot2_f)).ln();
    }
    if n21 > 0.0 {
        g_stat += n21 * (n21 * total_f / (n2_dot_f * n_dot1_f)).ln();
    }
    if n22 > 0.0 {
        g_stat += n22 * (n22 * total_f / (n2_dot_f * n_dot2_f)).ln();
    }
    g_stat *= 2.0;
    let p_value = 1.0 - libm::erf(libm::sqrt(g_stat / 2.0));
    (p_value, g_stat)
}
#[derive(Debug, Clone)]
pub struct WorldModel {
    weights_hidden: Vec<f32>,
    bias_hidden: Vec<f32>,
    weights_state_out: Vec<f32>,
    bias_state_out: Vec<f32>,
    weights_reward_out: Vec<f32>,
    bias_reward_out: f32,
    state_dim: usize,
    action_dim: usize,
    hidden_dim: usize,
    learning_rate: f32,
    reg_config: RegularisationConfig,
    prediction_error_history: Vec<f32>,
}
impl WorldModel {
    pub fn new(state_dim: usize, action_dim: usize, reg_config: RegularisationConfig) -> Self {
        let mut rng = rand::thread_rng();
        let input_dim = state_dim + action_dim;
        let hidden_dim = WORLD_MODEL_HIDDEN_DIM;
        let he_std_dev_hidden = (2.0 / input_dim as f32).sqrt();
        let he_std_dev_out = (2.0 / hidden_dim as f32).sqrt();
        Self {
            weights_hidden: (0..input_dim * hidden_dim)
                .map(|_| rng.gen_range(-he_std_dev_hidden..he_std_dev_hidden))
                .collect(),
            bias_hidden: vec![0.0; hidden_dim],
            weights_state_out: (0..hidden_dim * state_dim)
                .map(|_| rng.gen_range(-he_std_dev_out..he_std_dev_out))
                .collect(),
            bias_state_out: vec![0.0; state_dim],
            weights_reward_out: (0..hidden_dim)
                .map(|_| rng.gen_range(-he_std_dev_out..he_std_dev_out))
                .collect(),
            bias_reward_out: 0.0,
            state_dim,
            action_dim,
            hidden_dim,
            learning_rate: 0.01,
            reg_config,
            prediction_error_history: Vec::with_capacity(100),
        }
    }
    fn forward_pass(&self, input: &[f32]) -> (f32, Vec<f32>, Vec<f32>) {
        let mut hidden_activations = self.bias_hidden.clone();
        for (i, hidden_activation) in hidden_activations.iter_mut().enumerate() {
            for j in 0..input.len() {
                *hidden_activation += input[j] * self.weights_hidden[i * input.len() + j];
            }
            *hidden_activation = hidden_activation.max(0.0);
        }
        let predicted_reward = hidden_activations
            .iter()
            .zip(&self.weights_reward_out)
            .map(|(a, w)| a * w)
            .sum::<f32>()
            + self.bias_reward_out;
        let mut predicted_next_state = self.bias_state_out.clone();
        for (i, predicted_state) in predicted_next_state.iter_mut().enumerate() {
            for (j, &hidden_activation) in hidden_activations.iter().enumerate() {
                *predicted_state +=
                    hidden_activation * self.weights_state_out[i * self.hidden_dim + j];
            }
        }
        (predicted_reward, predicted_next_state, hidden_activations)
    }
    pub fn predict(&self, current_state: &[f32], action: &[f32]) -> (f32, Vec<f32>) {
        if current_state.is_empty() || action.is_empty() {
            return (0.0, vec![0.0; self.state_dim]);
        }

        if action.len() != self.action_dim {
            eprintln!(
                "Warning: Expected action dimension {}, got {}",
                self.action_dim,
                action.len()
            );
            return (0.0, vec![0.0; self.state_dim]);
        }

        let input = [current_state, action].concat();
        let (reward, state, _) = self.forward_pass(&input);
        (reward, state)
    }
    pub fn train(&mut self, state: &[f32], action: &[f32], next_state: &[f32], reward: f32) {
        if action.len() != self.action_dim {
            eprintln!(
                "Warning: Expected action dimension {}, got {}",
                self.action_dim,
                action.len()
            );
            return;
        }

        let input = [state, action].concat();
        let input_dim = input.len();
        let (pred_reward, pred_next_state, hidden_activations) = self.forward_pass(&input);
        let reward_error = pred_reward - reward;
        let state_error_vec: Vec<f32> = pred_next_state
            .iter()
            .zip(next_state.iter())
            .map(|(p, a)| p - a)
            .collect();
        let total_error = reward_error.abs() + state_error_vec.iter().map(|e| e.abs()).sum::<f32>();
        if self.prediction_error_history.len() >= 100 {
            self.prediction_error_history.remove(0);
        }
        self.prediction_error_history.push(total_error);
        let mut grad_weights_reward_out: Vec<f32> = hidden_activations
            .iter()
            .map(|h| reward_error * h)
            .collect();
        let mut grad_bias_reward_out = reward_error;
        let mut grad_weights_state_out = vec![0.0; self.state_dim * self.hidden_dim];
        let mut grad_bias_state_out = vec![0.0; self.state_dim];
        for i in 0..self.state_dim {
            grad_bias_state_out[i] = state_error_vec[i];
            for j in 0..self.hidden_dim {
                grad_weights_state_out[i * self.hidden_dim + j] =
                    state_error_vec[i] * hidden_activations[j];
            }
        }
        let mut hidden_error_vec = vec![0.0; self.hidden_dim];
        for j in 0..self.hidden_dim {
            let error_from_reward = reward_error * self.weights_reward_out[j];
            let error_from_state: f32 = (0..self.state_dim)
                .map(|i| state_error_vec[i] * self.weights_state_out[i * self.hidden_dim + j])
                .sum();
            let relu_grad = if hidden_activations[j] > 0.0 {
                1.0
            } else {
                0.0
            };
            hidden_error_vec[j] = (error_from_reward + error_from_state) * relu_grad;
        }
        let mut grad_weights_hidden = vec![0.0; input_dim * self.hidden_dim];
        let mut grad_bias_hidden = vec![0.0; self.hidden_dim];
        for i in 0..self.hidden_dim {
            grad_bias_hidden[i] = hidden_error_vec[i];
            for j in 0..input_dim {
                grad_weights_hidden[i * input_dim + j] = hidden_error_vec[i] * input[j];
            }
        }
        clip_gradients_global_norm(
            &mut [
                &mut grad_weights_hidden,
                &mut grad_bias_hidden,
                &mut grad_weights_state_out,
                &mut grad_bias_state_out,
                &mut grad_weights_reward_out,
                std::slice::from_mut(&mut grad_bias_reward_out),
            ],
            self.reg_config.gradient_clip_norm,
        );
        let lr = self.learning_rate;
        for (i, weight) in self.weights_reward_out.iter_mut().enumerate() {
            *weight -= lr
                * (grad_weights_reward_out[i]
                    + self.reg_config.l2_penalty * *weight
                    + self.reg_config.l1_penalty * weight.signum());
        }
        self.bias_reward_out -= lr * grad_bias_reward_out;
        for (i, weight) in self.weights_state_out.iter_mut().enumerate() {
            *weight -= lr
                * (grad_weights_state_out[i]
                    + self.reg_config.l2_penalty * *weight
                    + self.reg_config.l1_penalty * weight.signum());
        }
        for (bias, &grad) in self.bias_state_out.iter_mut().zip(&grad_bias_state_out) {
            *bias -= lr * grad;
        }
        for (i, weight) in self.weights_hidden.iter_mut().enumerate() {
            *weight -= lr
                * (grad_weights_hidden[i]
                    + self.reg_config.l2_penalty * *weight
                    + self.reg_config.l1_penalty * weight.signum());
        }
        for (bias, &grad) in self.bias_hidden.iter_mut().zip(&grad_bias_hidden) {
            *bias -= lr * grad;
        }
    }
    pub fn get_average_prediction_error(&self) -> f32 {
        if self.prediction_error_history.is_empty() {
            0.5
        } else {
            self.prediction_error_history.iter().sum::<f32>()
                / self.prediction_error_history.len() as f32
        }
    }
}
#[derive(Debug, Clone)]
pub struct LSTM {
    weights_ih: Vec<f32>,
    weights_hh: Vec<f32>,
    bias_ih: Vec<f32>,
    bias_hh: Vec<f32>,
    input_size: usize,
    hidden_size: usize,
    reg_config: RegularisationConfig,
}
impl LSTM {
    pub fn new(input_size: usize, hidden_size: usize, reg_config: RegularisationConfig) -> Self {
        let mut rng = rand::thread_rng();
        let std_dev_ih = (2.0 / input_size as f32).sqrt();
        let std_dev_hh = (2.0 / hidden_size as f32).sqrt();
        let weights_ih = (0..input_size * hidden_size * 4)
            .map(|_| rng.gen_range(-std_dev_ih..std_dev_ih))
            .collect();
        let weights_hh = (0..hidden_size * hidden_size * 4)
            .map(|_| rng.gen_range(-std_dev_hh..std_dev_hh))
            .collect();
        Self {
            weights_ih,
            weights_hh,
            bias_ih: vec![0.0; hidden_size * 4],
            bias_hh: vec![0.0; hidden_size * 4],
            input_size,
            hidden_size,
            reg_config,
        }
    }
    pub fn forward(
        &self,
        input: &[f32],
        (prev_h, prev_c): (&[f32], &[f32]),
    ) -> (Vec<f32>, Vec<f32>) {
        if input.is_empty() {
            return (prev_h.to_vec(), prev_c.to_vec());
        }
        let gates_ih = self.linear(input, &self.weights_ih, &self.bias_ih, self.input_size);
        let gates_hh = self.linear(prev_h, &self.weights_hh, &self.bias_hh, self.hidden_size);
        let mut next_h = vec![0.0; self.hidden_size];
        let mut next_c = vec![0.0; self.hidden_size];
        for i in 0..self.hidden_size {
            let i_gate = sigmoid(gates_ih[i] + gates_hh[i]);
            let f_gate = sigmoid(gates_ih[i + self.hidden_size] + gates_hh[i + self.hidden_size]);
            let g_gate =
                (gates_ih[i + 2 * self.hidden_size] + gates_hh[i + 2 * self.hidden_size]).tanh();
            let o_gate =
                sigmoid(gates_ih[i + 3 * self.hidden_size] + gates_hh[i + 3 * self.hidden_size]);
            next_c[i] = f_gate * prev_c[i] + i_gate * g_gate;
            next_h[i] = o_gate * next_c[i].tanh();
        }
        apply_dropout(&mut next_h, self.reg_config.dropout_rate);
        (next_h, next_c)
    }
    fn linear(&self, input: &[f32], weights: &[f32], bias: &[f32], in_features: usize) -> Vec<f32> {
        let mut output = bias.to_vec();
        let out_features = self.hidden_size * 4;
        for i in 0..out_features {
            for j in 0..in_features {
                output[i] += input[j] * weights[i * in_features + j];
            }
        }
        output
    }
}
#[derive(Debug, Clone)]
pub struct AttentionMechanism {
    w_encoder: Vec<f32>,
    w_query: Vec<f32>,
    v: Vec<f32>,
    hidden_dim: usize,
}
impl AttentionMechanism {
    pub fn new(hidden_dim: usize) -> Self {
        let mut rng = rand::thread_rng();
        let std_dev = (1.0 / hidden_dim as f32).sqrt();
        Self {
            w_encoder: (0..hidden_dim * hidden_dim)
                .map(|_| rng.gen_range(-std_dev..std_dev))
                .collect(),
            w_query: (0..hidden_dim * hidden_dim)
                .map(|_| rng.gen_range(-std_dev..std_dev))
                .collect(),
            v: (0..hidden_dim)
                .map(|_| rng.gen_range(-std_dev..std_dev))
                .collect(),
            hidden_dim,
        }
    }
    pub fn compute_context_vector(
        &self,
        sequence_embeddings: &[Vec<f32>],
        query: &[f32],
    ) -> Vec<f32> {
        if sequence_embeddings.is_empty() {
            return vec![0.0; self.hidden_dim];
        }
        let projected_query = self.linear_transform(query, &self.w_query);
        let mut scores: Vec<f32> = sequence_embeddings
            .iter()
            .map(|encoder_output| {
                let projected_encoder_output =
                    self.linear_transform(encoder_output, &self.w_encoder);
                let combined = projected_encoder_output
                    .iter()
                    .zip(projected_query.iter())
                    .map(|(a, b)| (a + b).tanh());
                combined.zip(self.v.iter()).map(|(c, v_i)| c * v_i).sum()
            })
            .collect();
        softmax(&mut scores);
        let mut context_vector = vec![0.0; self.hidden_dim];
        for (i, embedding) in sequence_embeddings.iter().enumerate() {
            for j in 0..self.hidden_dim {
                context_vector[j] += scores[i] * embedding[j];
            }
        }
        context_vector
    }
    fn linear_transform(&self, input: &[f32], weights: &[f32]) -> Vec<f32> {
        let mut output = vec![0.0; self.hidden_dim];
        for i in 0..self.hidden_dim {
            for j in 0..self.hidden_dim {
                output[i] += input[j] * weights[i * self.hidden_dim + j];
            }
        }
        output
    }
    pub fn compute_scores(&self, sequence_embeddings: &[Vec<f32>]) -> Vec<f32> {
        if sequence_embeddings.is_empty() {
            return Vec::new();
        }
        let zero_query = vec![0.0; self.hidden_dim];
        let projected_query = self.linear_transform(&zero_query, &self.w_query);
        let mut scores: Vec<f32> = sequence_embeddings
            .iter()
            .map(|encoder_output| {
                let projected_encoder_output =
                    self.linear_transform(encoder_output, &self.w_encoder);
                let combined = projected_encoder_output
                    .iter()
                    .zip(projected_query.iter())
                    .map(|(a, b)| (a + b).tanh());
                combined.zip(self.v.iter()).map(|(c, v_i)| c * v_i).sum()
            })
            .collect();
        softmax(&mut scores);
        scores
    }
}
fn clip_gradients_global_norm(grads: &mut [&mut [f32]], max_norm: f32) {
    if max_norm <= 0.0 {
        return;
    }
    let total_norm_sq: f32 = grads.iter().flat_map(|g| g.iter().map(|v| v * v)).sum();
    let total_norm = total_norm_sq.sqrt();
    if total_norm > max_norm {
        let scale = max_norm / total_norm;
        for grad_group in grads {
            for val in grad_group.iter_mut() {
                *val *= scale;
            }
        }
    }
}
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}
fn softmax(scores: &mut [f32]) {
    if scores.is_empty() {
        return;
    }
    let max_score = scores.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
    let exps: Vec<f32> = scores.iter().map(|s| (s - max_score).exp()).collect();
    let sum_exps: f32 = exps.iter().sum();
    if sum_exps > 0.0 {
        for (score, exp) in scores.iter_mut().zip(exps.iter()) {
            *score = exp / sum_exps;
        }
    }
}
fn linear(input: &[f32], weights: &[f32], in_dim: usize, out_dim: usize) -> Vec<f32> {
    let mut output = vec![0.0; out_dim];
    for i in 0..out_dim {
        for j in 0..in_dim {
            output[i] += input[j] * weights[i * in_dim + j];
        }
    }
    output
}
