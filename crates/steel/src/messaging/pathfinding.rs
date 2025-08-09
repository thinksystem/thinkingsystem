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

use crate::messaging::network::PeerInfo;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

const W_LATENCY: f64 = 1.5;
const W_HEALTH_PENALTY: f64 = 200.0;

#[derive(Debug, Clone)]
pub struct NodeState {
    pub id: String,
    pub position: (f64, f64),
    pub latency: f64,
    pub connections: Vec<String>,
    pub last_updated: u64,
    pub health_score: f64,
}

impl NodeState {
    pub fn new(id: String, position: (f64, f64), latency: f64) -> Self {
        Self {
            id,
            position,
            latency,
            connections: Vec::new(),
            last_updated: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            health_score: 1.0,
        }
    }

    pub fn calculate_health_score(&mut self) {
        let current_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let age_seconds = current_time.saturating_sub(self.last_updated);
        let age_penalty = (age_seconds as f64 / 300.0).min(1.0);

        let latency_score = (1.0 / (1.0 + self.latency / 100.0)).max(0.1);
        let age_score = 1.0 - age_penalty;

        self.health_score = (latency_score * age_score).max(0.1);
    }
}

#[derive(Debug)]
pub struct NetworkState {
    pub node_states: HashMap<String, NodeState>,
    pub path_cache: HashMap<(String, String), Vec<String>>,
    pub topology_version: u64,
    pub cache_capacity: usize,
}

impl NetworkState {
    pub fn new(cache_capacity: usize) -> Self {
        Self {
            node_states: HashMap::new(),
            path_cache: HashMap::new(),
            topology_version: 0,
            cache_capacity,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OptimalPath {
    pub nodes: Vec<String>,
    pub total_cost: f64,
}

#[derive(Debug, Clone)]
struct PathState {
    cost: f64,
    position: String,
}

impl Eq for PathState {}

impl PartialEq for PathState {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost
    }
}

impl Ord for PathState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for PathState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub struct PathfindingNetworkManager {
    state: Arc<RwLock<NetworkState>>,
}

impl PathfindingNetworkManager {
    pub fn new(cache_capacity: usize) -> Self {
        Self {
            state: Arc::new(RwLock::new(NetworkState::new(cache_capacity))),
        }
    }

    pub async fn update_node(
        &self,
        id: String,
        position: (f64, f64),
        latency: f64,
        connections: Vec<String>,
    ) {
        let mut state = self.state.write().await;

        let node_state = state
            .node_states
            .entry(id.clone())
            .or_insert_with(|| NodeState::new(id.clone(), position, latency));

        node_state.position = position;
        node_state.latency = latency;
        node_state.connections = connections;
        node_state.last_updated = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        node_state.calculate_health_score();

        state.topology_version += 1;
        state.path_cache.clear();
    }

    pub async fn find_optimal_path(&self, start: &str, end: &str) -> Option<OptimalPath> {
        let cache_key = (start.to_string(), end.to_string());

        {
            let state = self.state.read().await;
            if let Some(cached_path) = state.path_cache.get(&cache_key) {
                let cost = self
                    .calculate_path_cost(cached_path)
                    .await
                    .unwrap_or(f64::INFINITY);
                return Some(OptimalPath {
                    nodes: cached_path.clone(),
                    total_cost: cost,
                });
            }
        }

        let state = self.state.read().await;
        let mut distances: HashMap<String, f64> = HashMap::new();
        let mut priority_queue = BinaryHeap::new();
        let mut predecessors: HashMap<String, String> = HashMap::new();

        distances.insert(start.to_string(), 0.0);
        priority_queue.push(PathState {
            cost: 0.0,
            position: start.to_string(),
        });

        while let Some(PathState { cost, position }) = priority_queue.pop() {
            if position == end {
                let mut path = vec![end.to_string()];
                let mut current = end.to_string();

                while let Some(prev) = predecessors.get(&current) {
                    path.push(prev.clone());
                    current = prev.clone();
                }
                path.reverse();

                let result = OptimalPath {
                    nodes: path.clone(),
                    total_cost: cost,
                };

                drop(state);
                let mut state_write = self.state.write().await;

                if state_write.path_cache.len() >= state_write.cache_capacity {
                    state_write.path_cache.clear();
                }

                state_write.path_cache.insert(cache_key, path);
                return Some(result);
            }

            if cost > *distances.get(&position).unwrap_or(&f64::INFINITY) {
                continue;
            }

            if let Some(current_node) = state.node_states.get(&position) {
                for neighbour_id in &current_node.connections {
                    if let Some(neighbour_node) = state.node_states.get(neighbour_id) {
                        let edge_cost = self
                            .calculate_robust_edge_cost(current_node, neighbour_node)
                            .await;
                        let new_cost = cost + edge_cost;

                        if new_cost < *distances.get(neighbour_id).unwrap_or(&f64::INFINITY) {
                            priority_queue.push(PathState {
                                cost: new_cost,
                                position: neighbour_id.clone(),
                            });
                            distances.insert(neighbour_id.clone(), new_cost);
                            predecessors.insert(neighbour_id.clone(), position.clone());
                        }
                    }
                }
            }
        }

        None
    }

    async fn calculate_robust_edge_cost(&self, from_node: &NodeState, to_node: &NodeState) -> f64 {
        let dx = to_node.position.0 - from_node.position.0;
        let dy = to_node.position.1 - from_node.position.1;
        let distance_cost = (dx * dx + dy * dy).sqrt();

        let latency_cost = to_node.latency * W_LATENCY;

        let health_penalty = (1.0 - to_node.health_score) * W_HEALTH_PENALTY;

        distance_cost + latency_cost + health_penalty
    }

    async fn calculate_path_cost(&self, path: &[String]) -> Option<f64> {
        let state = self.state.read().await;
        let mut total_cost = 0.0;

        for window in path.windows(2) {
            let current = state.node_states.get(&window[0])?;
            let next = state.node_states.get(&window[1])?;
            total_cost += self.calculate_robust_edge_cost(current, next).await;
        }

        Some(total_cost)
    }

    pub async fn get_network_stats(&self) -> NetworkStats {
        let state = self.state.read().await;

        let total_nodes = state.node_states.len();
        let avg_health = if total_nodes > 0 {
            state
                .node_states
                .values()
                .map(|node| node.health_score)
                .sum::<f64>()
                / total_nodes as f64
        } else {
            0.0
        };

        let total_connections: usize = state
            .node_states
            .values()
            .map(|node| node.connections.len())
            .sum();

        NetworkStats {
            total_nodes,
            total_connections,
            average_health_score: avg_health,
            cache_size: state.path_cache.len(),
            topology_version: state.topology_version,
        }
    }

    pub async fn update_from_peer(&self, peer: &PeerInfo) {
        let position = self.hash_to_position(&peer.peer_id);

        let latency = (1.0 - peer.trust_score as f64) * 1000.0;

        self.update_node(
            peer.peer_id.clone(),
            position,
            latency,
            peer.capabilities.clone(),
        )
        .await;
    }

    fn hash_to_position(&self, peer_id: &str) -> (f64, f64) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        peer_id.hash(&mut hasher);
        let hash = hasher.finish();

        let x = ((hash & 0xFFFFFFFFu64) as f64 / 0xFFFFFFFFu64 as f64) * 1000.0;
        let y = (((hash >> 32) & 0xFFFFFFFFu64) as f64 / 0xFFFFFFFFu64 as f64) * 1000.0;

        (x, y)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub total_nodes: usize,
    pub total_connections: usize,
    pub average_health_score: f64,
    pub cache_size: usize,
    pub topology_version: u64,
}
