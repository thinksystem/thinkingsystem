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

use crate::scribes::base_scribe::BaseScribe;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};
pub const TD_ERROR_HISTORY_SIZE: usize = 1000;
pub const LEARNING_RATE_HISTORY_SIZE: usize = 1000;
pub const MIN_EXPLORATION_RATE: f32 = 0.01;
pub const MIN_LEARNING_RATE: f32 = 0.001;
pub const MAX_LEARNING_RATE: f32 = 0.1;
pub const ELIGIBILITY_TRACE_THRESHOLD: f32 = 0.001;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub state: usize,
    pub action: usize,
    pub reward: f32,
    pub next_state: usize,
    pub priority: f32,
    pub timestamp: u64,
}
impl Experience {
    pub fn new(state: usize, action: usize, reward: f32, next_state: usize) -> Self {
        let priority = reward.abs().max(0.01);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            state,
            action,
            reward,
            next_state,
            priority,
            timestamp,
        }
    }
}
#[derive(Debug, Clone)]
pub struct SumTree {
    tree: Vec<f32>,
    capacity: usize,
    next_idx: usize,
    size: usize,
}
impl SumTree {
    pub fn new(capacity: usize) -> Self {
        let tree_size = 2 * capacity - 1;
        Self {
            tree: vec![0.0; tree_size],
            capacity,
            next_idx: 0,
            size: 0,
        }
    }
    pub fn add(&mut self, priority: f32) -> usize {
        let idx = self.next_idx;
        self.update(idx, priority);
        self.next_idx = (self.next_idx + 1) % self.capacity;
        if self.size < self.capacity {
            self.size += 1;
        }
        idx
    }
    pub fn update(&mut self, idx: usize, priority: f32) {
        if idx >= self.capacity {
            return;
        }
        let tree_idx = idx + self.capacity - 1;
        let change = priority - self.tree[tree_idx];
        self.tree[tree_idx] = priority;
        let mut parent_idx = tree_idx;
        while parent_idx > 0 {
            parent_idx = (parent_idx - 1) / 2;
            self.tree[parent_idx] += change;
        }
    }
    pub fn sample(&self, value: f32) -> usize {
        let mut idx = 0;
        let mut remaining_value = value;
        while idx < self.capacity - 1 {
            let left_child = 2 * idx + 1;
            let right_child = 2 * idx + 2;
            if left_child < self.tree.len() && remaining_value <= self.tree[left_child] {
                idx = left_child;
            } else {
                if left_child < self.tree.len() {
                    remaining_value -= self.tree[left_child];
                }
                idx = right_child;
            }
        }
        idx - (self.capacity - 1)
    }
    pub fn total(&self) -> f32 {
        if self.tree.is_empty() {
            0.0
        } else {
            self.tree[0]
        }
    }
}
#[derive(Debug, Clone)]
pub struct PrioritisedReplayBuffer {
    experiences: Vec<Option<Experience>>,
    sum_tree: SumTree,
    capacity: usize,
    next_idx: usize,
    size: usize,
}
impl PrioritisedReplayBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            experiences: vec![None; capacity],
            sum_tree: SumTree::new(capacity),
            capacity,
            next_idx: 0,
            size: 0,
        }
    }
    pub fn add(&mut self, experience: Experience) {
        let idx = self.next_idx;
        let priority = experience.priority;
        self.experiences[idx] = Some(experience);
        self.sum_tree.add(priority);
        self.next_idx = (self.next_idx + 1) % self.capacity;
        if self.size < self.capacity {
            self.size += 1;
        }
    }
    pub fn sample(&self, batch_size: usize) -> Vec<(usize, Experience)> {
        if self.size == 0 {
            return Vec::new();
        }
        let mut batch = Vec::with_capacity(batch_size);
        let mut rng = rand::thread_rng();
        let total_priority = self.sum_tree.total();
        if total_priority <= 0.0 {
            return Vec::new();
        }
        let segment_size = total_priority / batch_size as f32;
        for i in 0..batch_size {
            let value = rng.gen_range((segment_size * i as f32)..(segment_size * (i + 1) as f32));
            let idx = self.sum_tree.sample(value);
            if let Some(ref experience) = self.experiences[idx] {
                batch.push((idx, experience.clone()));
            }
        }
        batch
    }
    pub fn update_priority(&mut self, idx: usize, priority: f32) {
        if idx < self.capacity {
            if let Some(ref mut experience) = self.experiences[idx] {
                experience.priority = priority;
            }
            self.sum_tree.update(idx, priority);
        }
    }
    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MetaLearningStats {
    goal_success_rate: HashMap<String, f32>,
    avg_reward: f32,
    td_error_history: VecDeque<f32>,
    learning_rate_history: VecDeque<f32>,
    
    total_attempted_items: u64,
    total_applied_items: u64,
    partial_apply_events: u64,
    backoff_events: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Goal {
    name: String,
    weight: f32,
    prerequisites: Vec<String>,
    is_achieved: bool,
    difficulty_score: f32,
}
#[derive(Debug, Clone)]
pub struct QLearningCore {
    pub agent: BaseScribe,
    gamma: f32,
    learning_rate: f32,
    exploration_rate: f32,
    batch_size: usize,
    replay_buffer: PrioritisedReplayBuffer,
    eligibility_traces: HashMap<(usize, usize), f32>,
    n_step: usize,
    n_step_buffer: VecDeque<Experience>,
    goals: HashMap<String, Goal>,
    meta_stats: MetaLearningStats,
}
impl QLearningCore {
    pub fn new(
        num_states: usize,
        num_actions: usize,
        gamma: f32,
        learning_rate: f32,
        exploration_rate: f32,
        batch_size: usize,
    ) -> Self {
        const MAX_BUFFER_SIZE: usize = 10000;
        Self {
            agent: BaseScribe::new(num_states, num_actions),
            gamma,
            learning_rate,
            exploration_rate,
            batch_size,
            replay_buffer: PrioritisedReplayBuffer::new(MAX_BUFFER_SIZE),
            eligibility_traces: HashMap::new(),
            n_step: 3,
            n_step_buffer: VecDeque::new(),
            goals: HashMap::new(),
            meta_stats: MetaLearningStats::default(),
        }
    }
    pub fn set_modulated_exploration_rate(&mut self, new_rate: f32) {
        self.exploration_rate = new_rate.clamp(MIN_EXPLORATION_RATE, 1.0);
    }
    pub fn choose_action(&self, state: usize, valid_actions: &[usize]) -> usize {
        if valid_actions.is_empty() {
            return 0;
        }
        let mut rng = rand::thread_rng();
        if rng.gen::<f32>() < self.exploration_rate {
            valid_actions[rng.gen_range(0..valid_actions.len())]
        } else {
            self.get_best_valid_action(state, valid_actions)
        }
    }
    fn get_best_valid_action(&self, state: usize, valid_actions: &[usize]) -> usize {
        valid_actions
            .iter()
            .max_by(|&&a, &&b| {
                let q_a = self
                    .agent
                    .q_table()
                    .get(state)
                    .and_then(|row| row.get(a))
                    .unwrap_or(&f32::NEG_INFINITY);
                let q_b = self
                    .agent
                    .q_table()
                    .get(state)
                    .and_then(|row| row.get(b))
                    .unwrap_or(&f32::NEG_INFINITY);
                q_a.partial_cmp(q_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied()
            .unwrap_or(valid_actions[0])
    }
    pub fn add_experience(&mut self, state: usize, action: usize, reward: f32, next_state: usize) {
        let experience = Experience::new(state, action, reward, next_state);
        self.n_step_buffer.push_back(experience.clone());
        self.replay_buffer.add(experience);
    }
    pub fn update_q_values(&mut self) {
        if self.replay_buffer.len() < self.batch_size {
            return;
        }
        let batch = self.replay_buffer.sample(self.batch_size);
        if batch.is_empty() {
            return;
        }
        let mut total_td_error = 0.0;
        let mut total_reward = 0.0;
        for (buffer_idx, experience) in &batch {
            let td_error = self.process_experience(experience);
            total_td_error += td_error;
            total_reward += experience.reward;
            let new_priority = td_error.abs().max(0.01);
            self.replay_buffer
                .update_priority(*buffer_idx, new_priority);
            self.update_eligibility_traces(experience.state, experience.action, td_error);
        }
        self.process_n_step_buffer();
        let batch_len = batch.len() as f32;
        self.update_meta_statistics(total_td_error / batch_len, total_reward / batch_len);
    }
    fn process_experience(&mut self, experience: &Experience) -> f32 {
        let old_q_value = self.agent.q_table()[experience.state][experience.action];
        let next_state_max_q = self.agent.q_table()[experience.next_state]
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max);
        let weighted_reward = self.calculate_weighted_reward(experience.reward);
        let target = weighted_reward + self.gamma * next_state_max_q;
        let td_error = target - old_q_value;
        self.agent.q_table_mut()[experience.state][experience.action] +=
            self.learning_rate * td_error;
        td_error
    }
    fn process_n_step_buffer(&mut self) {
        while self.n_step_buffer.len() >= self.n_step {
            let mut n_step_return = 0.0;
            let mut gamma_power = 1.0;
            for i in 0..self.n_step {
                if let Some(exp) = self.n_step_buffer.get(i) {
                    n_step_return += gamma_power * self.calculate_weighted_reward(exp.reward);
                    gamma_power *= self.gamma;
                }
            }
            if let Some(first_exp) = self.n_step_buffer.front().cloned() {
                let bootstrap_state = self
                    .n_step_buffer
                    .get(self.n_step - 1)
                    .map_or(first_exp.next_state, |e| e.next_state);
                let bootstrap_value = self.agent.q_table()[bootstrap_state]
                    .iter()
                    .cloned()
                    .fold(f32::NEG_INFINITY, f32::max);
                let target = n_step_return + gamma_power * bootstrap_value;
                let old_q_value = self.agent.q_table()[first_exp.state][first_exp.action];
                let td_error = target - old_q_value;
                self.agent.q_table_mut()[first_exp.state][first_exp.action] +=
                    self.learning_rate * td_error;
            }
            self.n_step_buffer.pop_front();
        }
    }
    fn update_meta_statistics(&mut self, avg_td_error: f32, avg_batch_reward: f32) {
        let n = self.meta_stats.learning_rate_history.len() as f32;
        self.meta_stats.avg_reward =
            (self.meta_stats.avg_reward * n + avg_batch_reward) / (n + 1.0);
        self.meta_stats.td_error_history.push_back(avg_td_error);
        if self.meta_stats.td_error_history.len() > TD_ERROR_HISTORY_SIZE {
            self.meta_stats.td_error_history.pop_front();
        }
        let td_variance = self.calculate_td_error_variance();
        let new_learning_rate = self.learning_rate * (1.0 / (1.0 + td_variance));
        self.learning_rate = new_learning_rate.clamp(MIN_LEARNING_RATE, MAX_LEARNING_RATE);
        self.meta_stats
            .learning_rate_history
            .push_back(self.learning_rate);
        if self.meta_stats.learning_rate_history.len() > LEARNING_RATE_HISTORY_SIZE {
            self.meta_stats.learning_rate_history.pop_front();
        }
    }
    fn calculate_td_error_variance(&self) -> f32 {
        if self.meta_stats.td_error_history.is_empty() {
            return 0.0;
        }
        let mean = self.meta_stats.td_error_history.iter().sum::<f32>()
            / self.meta_stats.td_error_history.len() as f32;
        self.meta_stats
            .td_error_history
            .iter()
            .map(|&x| (x - mean).powi(2))
            .sum::<f32>()
            / self.meta_stats.td_error_history.len() as f32
    }
    fn update_eligibility_traces(&mut self, state: usize, action: usize, td_error: f32) {
        *self
            .eligibility_traces
            .entry((state, action))
            .or_insert(0.0) += 1.0;
        let keys_to_update: Vec<(usize, usize)> = self.eligibility_traces.keys().cloned().collect();
        let mut keys_to_remove = Vec::new();
        for (s, a) in keys_to_update {
            let trace_value = self.eligibility_traces[&(s, a)];
            self.agent.q_table_mut()[s][a] += self.learning_rate * td_error * trace_value;
            let new_trace_value = trace_value * self.gamma;
            if new_trace_value < ELIGIBILITY_TRACE_THRESHOLD {
                keys_to_remove.push((s, a));
            } else {
                self.eligibility_traces.insert((s, a), new_trace_value);
            }
        }
        for key in keys_to_remove {
            self.eligibility_traces.remove(&key);
        }
    }
    fn calculate_weighted_reward(&self, base_reward: f32) -> f32 {
        let mut weighted_reward = base_reward;
        for goal in self.goals.values() {
            if self.are_prerequisites_met(&goal.name) {
                weighted_reward *= goal.weight;
            }
        }
        weighted_reward
    }
    fn are_prerequisites_met(&self, goal_name: &str) -> bool {
        if let Some(goal) = self.goals.get(goal_name) {
            goal.prerequisites
                .iter()
                .all(|prereq| self.goals.get(prereq).is_some_and(|g| g.is_achieved))
        } else {
            false
        }
    }
    pub fn add_hierarchical_goal(&mut self, name: String, weight: f32, prerequisites: Vec<String>) {
        self.goals.insert(
            name.clone(),
            Goal {
                name,
                weight,
                prerequisites,
                is_achieved: false,
                difficulty_score: 1.0,
            },
        );
    }
    pub fn get_learning_metrics(&self) -> HashMap<String, f32> {
        let mut metrics = HashMap::new();
        metrics.insert("exploration_rate".to_string(), self.exploration_rate);
        metrics.insert("learning_rate".to_string(), self.learning_rate);
        metrics.insert("avg_reward".to_string(), self.meta_stats.avg_reward);
        if let Some(latest_td_error) = self.meta_stats.td_error_history.back() {
            metrics.insert("latest_td_error".to_string(), *latest_td_error);
        }
        metrics.insert(
            "td_error_variance".to_string(),
            self.calculate_td_error_variance(),
        );
        metrics.insert(
            "replay_buffer_size".to_string(),
            self.replay_buffer.len() as f32,
        );
        metrics.insert(
            "active_eligibility_traces".to_string(),
            self.eligibility_traces.len() as f32,
        );
        
        let attempted = self.meta_stats.total_attempted_items as f32;
        let applied = self.meta_stats.total_applied_items as f32;
        let success_ratio = if attempted > 0.0 {
            applied / attempted
        } else {
            0.0
        };
        metrics.insert("apply_attempted".to_string(), attempted);
        metrics.insert("apply_applied".to_string(), applied);
        metrics.insert("apply_success_ratio".to_string(), success_ratio);
        metrics.insert(
            "apply_partial_events".to_string(),
            self.meta_stats.partial_apply_events as f32,
        );
        metrics.insert(
            "apply_backoff_events".to_string(),
            self.meta_stats.backoff_events as f32,
        );
        metrics
    }

    
    
    
    pub fn record_apply_outcome(
        &mut self,
        attempted: usize,
        applied: usize,
        backoffs: usize,
    ) -> f32 {
        let attempted_u64 = attempted as u64;
        let applied_u64 = applied as u64;
        let partial = applied > 0 && applied < attempted;
        let success_ratio = if attempted > 0 {
            applied as f32 / attempted as f32
        } else {
            0.0
        };

        
        self.meta_stats.total_attempted_items = self
            .meta_stats
            .total_attempted_items
            .saturating_add(attempted_u64);
        self.meta_stats.total_applied_items = self
            .meta_stats
            .total_applied_items
            .saturating_add(applied_u64);
        if partial {
            self.meta_stats.partial_apply_events =
                self.meta_stats.partial_apply_events.saturating_add(1);
        }
        if backoffs > 0 {
            self.meta_stats.backoff_events = self
                .meta_stats
                .backoff_events
                .saturating_add(backoffs as u64);
        }

        
        let mut shaped = success_ratio;
        if attempted > 0 && applied == 0 {
            shaped -= 1.0; 
        }
        shaped -= 0.1 * (backoffs as f32);

        
        let target_exploration = (1.0 - success_ratio).clamp(MIN_EXPLORATION_RATE, 1.0);
        
        let alpha = 0.2;
        let new_rate = self.exploration_rate + alpha * (target_exploration - self.exploration_rate);
        self.set_modulated_exploration_rate(new_rate);

        shaped
    }
}
