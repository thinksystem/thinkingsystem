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

use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::num::NonZeroUsize;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::sync::broadcast;

#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Lock was poisoned: {0}")]
    LockPoisoned(String),
    #[error("Node with ID '{0}' not found in the network")]
    NodeNotFound(String),
    #[error("System time error: {0}")]
    SystemTime(#[from] std::time::SystemTimeError),
    #[error("Broadcast channel send error")]
    BroadcastSendError,
}

impl<T> From<PoisonError<T>> for NetworkError {
    fn from(e: PoisonError<T>) -> Self {
        NetworkError::LockPoisoned(e.to_string())
    }
}

const W_DISTANCE: f64 = 1.0;
const W_LATENCY: f64 = 1.5;
const W_HEALTH_PENALTY: f64 = 200.0;
const GRID_RESOLUTION: f64 = 5.0;
const CONNECTION_DISTANCE_THRESHOLD: f64 = 15.0;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PathfindingMode {
    Dijkstra,
    AStar,
    ContractionHierarchy,
}

#[derive(Clone, Debug)]
pub enum ChangeType {
    NodeAdded,
    NodeRemoved,
    ConnectionsChanged,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}

impl Location {
    pub fn distance(&self, other: &Location) -> f64 {
        let dx = self.latitude - other.latitude;
        let dy = self.longitude - other.longitude;
        (dx * dx + dy * dy).sqrt()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub location: Location,
    pub latency: u32,
    pub health_score: f64,
    #[serde(skip, default = "Instant::now")]
    pub last_seen: Instant,
}

#[derive(Clone, Debug)]
pub struct NodeState {
    pub id: String,
    pub position: (f64, f64),
    pub latency: f64,
    pub connections: HashMap<String, f64>,
    pub last_seen: Instant,
    pub health_score: f64,

    pub level: usize,

    pub ch_forward_connections: Vec<(String, f64)>,
    pub ch_backward_connections: Vec<(String, f64)>,
}

impl NodeState {
    pub fn calculate_health_score(&mut self) -> f64 {
        let age_secs = Instant::now().duration_since(self.last_seen).as_secs_f64();
        let connection_factor = self.connections.len() as f64 * 0.1;
        let latency_factor = 1.0 / (self.latency + 1.0);

        let score = (1.0 / (age_secs + 1.0)) * connection_factor * latency_factor;
        self.health_score = score.clamp(0.0, 1.0);
        self.health_score
    }
}

#[derive(Clone, Debug)]
pub struct NetworkGraph {
    pub node_states: HashMap<String, NodeState>,
    pub topology_version: u64,
    pub ch_precomputed: bool,
}

impl Default for NetworkGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkGraph {
    pub fn new() -> Self {
        NetworkGraph {
            node_states: HashMap::new(),
            topology_version: 0,
            ch_precomputed: false,
        }
    }

    pub fn update_node(
        &mut self,
        id: String,
        position: (f64, f64),
        latency: f64,
        last_seen: Instant,
    ) {
        let state = self
            .node_states
            .entry(id.clone())
            .or_insert_with(|| NodeState {
                id: id.clone(),
                position,
                latency,
                connections: HashMap::new(),
                last_seen,
                health_score: 0.0,
                level: 0,
                ch_forward_connections: Vec::new(),
                ch_backward_connections: Vec::new(),
            });

        state.position = position;
        state.latency = latency;
        state.last_seen = last_seen;
        state.calculate_health_score();

        self.topology_version += 1;
        self.ch_precomputed = false;
    }

    pub fn add_connection(&mut self, from: String, to: String) -> Result<(), NetworkError> {
        let cost = {
            let from_node = self
                .node_states
                .get(&from)
                .ok_or_else(|| NetworkError::NodeNotFound(from.clone()))?;
            let to_node = self
                .node_states
                .get(&to)
                .ok_or_else(|| NetworkError::NodeNotFound(to.clone()))?;
            self.calculate_robust_edge_cost(from_node, to_node)
        };

        if let Some(node) = self.node_states.get_mut(&from) {
            node.connections.insert(to, cost);
            self.topology_version += 1;
            self.ch_precomputed = false;
        }
        Ok(())
    }

    fn calculate_robust_edge_cost(&self, from_node: &NodeState, to_node: &NodeState) -> f64 {
        let dx = to_node.position.0 - from_node.position.0;
        let dy = to_node.position.1 - from_node.position.1;
        let distance_cost = (dx * dx + dy * dy).sqrt() * W_DISTANCE;
        let latency_cost = to_node.latency * W_LATENCY;
        let health_penalty = (1.0 - to_node.health_score) * W_HEALTH_PENALTY;
        distance_cost + latency_cost + health_penalty
    }

    fn find_optimal_path_generic<F>(
        &self,
        start: &str,
        end: &str,
        heuristic: F,
    ) -> Option<OptimalPath>
    where
        F: Fn(&NodeState) -> f64,
    {
        let mut distances: HashMap<String, f64> = HashMap::new();
        let mut priority_queue = BinaryHeap::new();
        let mut predecessors: HashMap<String, String> = HashMap::new();

        distances.insert(start.to_string(), 0.0);
        priority_queue.push(PathState {
            cost: 0.0,
            position: start.to_string(),
        });

        while let Some(PathState { cost: _, position }) = priority_queue.pop() {
            if position == end {
                let mut path = VecDeque::new();
                let mut current = end.to_string();
                while let Some(prev) = predecessors.get(&current) {
                    path.push_front(current);
                    current = prev.clone();
                }
                path.push_front(current);
                return Some(OptimalPath {
                    nodes: path.into(),
                    total_cost: *distances.get(end).unwrap_or(&f64::INFINITY),
                });
            }

            let g_cost = *distances.get(&position).unwrap_or(&f64::INFINITY);

            if let Some(current_node) = self.node_states.get(&position) {
                for (neighbour_id, edge_cost) in &current_node.connections {
                    let new_g_cost = g_cost + *edge_cost;
                    if new_g_cost < *distances.get(neighbour_id).unwrap_or(&f64::INFINITY) {
                        distances.insert(neighbour_id.clone(), new_g_cost);
                        predecessors.insert(neighbour_id.clone(), position.clone());

                        let h_cost =
                            if let Some(neighbour_node) = self.node_states.get(neighbour_id) {
                                heuristic(neighbour_node)
                            } else {
                                0.0
                            };

                        priority_queue.push(PathState {
                            cost: new_g_cost + h_cost,
                            position: neighbour_id.clone(),
                        });
                    }
                }
            }
        }
        None
    }

    pub fn find_path_astar(&self, start: &str, end: &str) -> Option<OptimalPath> {
        let end_node = self.node_states.get(end)?;
        let end_pos = end_node.position;
        let heuristic = |node: &NodeState| {
            let dx = node.position.0 - end_pos.0;
            let dy = node.position.1 - end_pos.1;
            (dx * dx + dy * dy).sqrt()
        };
        self.find_optimal_path_generic(start, end, heuristic)
    }

    pub fn find_path_dijkstra(&self, start: &str, end: &str) -> Option<OptimalPath> {
        self.find_optimal_path_generic(start, end, |_| 0.0)
    }

    pub fn find_path_bfs(&self, start: &str, end: &str) -> Option<Vec<String>> {
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        let mut parent: HashMap<String, String> = HashMap::new();

        queue.push_back(start.to_string());
        visited.insert(start.to_string());

        while let Some(current) = queue.pop_front() {
            if current == end {
                let mut path = VecDeque::new();
                let mut node = end.to_string();
                while let Some(p) = parent.get(&node) {
                    path.push_front(node);
                    node = p.clone();
                }
                path.push_front(node);
                return Some(path.into());
            }

            if let Some(current_node) = self.node_states.get(&current) {
                for neighbour_id in current_node.connections.keys() {
                    if !visited.contains(neighbour_id) {
                        visited.insert(neighbour_id.clone());
                        parent.insert(neighbour_id.clone(), current.clone());
                        queue.push_back(neighbour_id.clone());
                    }
                }
            }
        }
        None
    }

    pub fn precompute_contraction_hierarchies(&mut self) {
        self.ch_precomputed = false;
    }

    pub fn find_path_contraction_hierarchy(&self, _start: &str, _end: &str) -> Option<OptimalPath> {
        if !self.ch_precomputed {
            eprintln!(
                "Warning: Contraction Hierarchies not precomputed. Pathfinding is not possible."
            );
            return None;
        }

        None
    }

    pub fn calculate_path_cost(&self, path: &[String]) -> Option<f64> {
        path.windows(2).try_fold(0.0, |acc, window| {
            let current_node = self.node_states.get(&window[0])?;
            let next_node = self.node_states.get(&window[1])?;
            let edge_cost = self.calculate_robust_edge_cost(current_node, next_node);
            Some(acc + edge_cost)
        })
    }
}

#[derive(Clone, PartialEq)]
struct PathState {
    cost: f64,
    position: String,
}
impl Eq for PathState {}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimalPath {
    pub nodes: Vec<String>,
    pub total_cost: f64,
}

#[derive(Clone, Debug)]
pub struct TopologyUpdate {
    pub node_id: String,
    pub change_type: ChangeType,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub total_nodes: usize,
    pub total_connections: usize,
    pub topology_version: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub avg_health_score: f64,
}

pub struct NetworkManager {
    graph: Arc<RwLock<NetworkGraph>>,

    path_cache: Arc<RwLock<LruCache<(String, String), OptimalPath>>>,
    cache_stats: Arc<RwLock<(u64, u64)>>,
    topology_updates: broadcast::Sender<TopologyUpdate>,
}

impl NetworkManager {
    pub async fn new(cache_size: usize) -> Result<Self, NetworkError> {
        let cache_capacity =
            NonZeroUsize::new(cache_size).unwrap_or_else(|| NonZeroUsize::new(1).unwrap());
        let (tx, _) = broadcast::channel(100);
        Ok(Self {
            graph: Arc::new(RwLock::new(NetworkGraph::new())),
            path_cache: Arc::new(RwLock::new(LruCache::new(cache_capacity))),
            cache_stats: Arc::new(RwLock::new((0, 0))),
            topology_updates: tx,
        })
    }

    pub async fn update_node(&self, node_info: NodeInfo) -> Result<(), NetworkError> {
        let mut graph = self.graph.write()?;
        let position = (node_info.location.latitude, node_info.location.longitude);
        graph.update_node(
            node_info.id.clone(),
            position,
            node_info.latency as f64,
            node_info.last_seen,
        );

        self.path_cache.write()?.clear();

        self.topology_updates
            .send(TopologyUpdate {
                node_id: node_info.id,
                change_type: ChangeType::NodeAdded,
                timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            })
            .map_err(|_| NetworkError::BroadcastSendError)?;

        Ok(())
    }

    pub async fn add_connection(&self, from: String, to: String) -> Result<(), NetworkError> {
        self.graph.write()?.add_connection(from, to)?;

        self.path_cache.write()?.clear();
        Ok(())
    }

    pub async fn find_path(
        &self,
        start: String,
        end: String,
        mode: PathfindingMode,
    ) -> Result<Option<OptimalPath>, NetworkError> {
        let cache_key = (start.clone(), end.clone());

        {
            let mut cache = self.path_cache.write()?;
            if let Some(cached_path) = cache.get(&cache_key) {
                self.cache_stats.write()?.0 += 1;
                return Ok(Some(cached_path.clone()));
            }
        }

        self.cache_stats.write()?.1 += 1;
        let graph_clone = self.graph.read()?.clone();

        let result = match mode {
            PathfindingMode::Dijkstra => graph_clone.find_path_dijkstra(&start, &end),
            PathfindingMode::AStar => graph_clone.find_path_astar(&start, &end),
            PathfindingMode::ContractionHierarchy => {
                graph_clone.find_path_contraction_hierarchy(&start, &end)
            }
        };

        if let Some(ref path) = result {
            self.path_cache.write()?.put(cache_key, path.clone());
        }

        Ok(result)
    }

    pub async fn find_path_bfs(
        &self,
        start: String,
        end: String,
    ) -> Result<Option<Vec<String>>, NetworkError> {
        let graph = self.graph.read()?;
        Ok(graph.find_path_bfs(&start, &end))
    }

    pub async fn find_optimal_path(
        &self,
        start: String,
        end: String,
    ) -> Result<Option<OptimalPath>, NetworkError> {
        self.find_path(start, end, PathfindingMode::Dijkstra).await
    }

    pub async fn get_network_stats(&self) -> Result<NetworkStats, NetworkError> {
        let graph = self.graph.read()?;
        let (hits, misses) = *self.cache_stats.read()?;

        let total_nodes = graph.node_states.len();
        let total_connections: usize = graph
            .node_states
            .values()
            .map(|n| n.connections.len())
            .sum();

        let avg_health_score = if total_nodes > 0 {
            graph
                .node_states
                .values()
                .map(|n| n.health_score)
                .sum::<f64>()
                / total_nodes as f64
        } else {
            0.0
        };

        Ok(NetworkStats {
            total_nodes,
            total_connections,
            topology_version: graph.topology_version,
            cache_hits: hits,
            cache_misses: misses,
            avg_health_score,
        })
    }

    pub fn subscribe_to_updates(&self) -> broadcast::Receiver<TopologyUpdate> {
        self.topology_updates.subscribe()
    }

    pub async fn optimise_network(&self) -> Result<(), NetworkError> {
        let mut graph = self.graph.write()?;

        if graph.node_states.len() < 2 {
            return Ok(());
        }

        for node in graph.node_states.values_mut() {
            node.calculate_health_score();
        }

        let mut grid: HashMap<(i32, i32), Vec<String>> = HashMap::new();
        for (id, node) in &graph.node_states {
            let cell_x = (node.position.0 / GRID_RESOLUTION) as i32;
            let cell_y = (node.position.1 / GRID_RESOLUTION) as i32;
            grid.entry((cell_x, cell_y)).or_default().push(id.clone());
        }

        let mut connections_to_add: Vec<(String, String)> = Vec::new();

        for ((x, y), cell_nodes) in &grid {
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if let Some(adjacent_nodes) = grid.get(&(x + dx, y + dy)) {
                        for from_id in cell_nodes {
                            for to_id in adjacent_nodes {
                                if from_id == to_id {
                                    continue;
                                }

                                if from_id < to_id {
                                    let from_node = &graph.node_states[from_id];
                                    let to_node = &graph.node_states[to_id];

                                    let dist_sq = (from_node.position.0 - to_node.position.0)
                                        .powi(2)
                                        + (from_node.position.1 - to_node.position.1).powi(2);

                                    if dist_sq.sqrt() < CONNECTION_DISTANCE_THRESHOLD {
                                        if !from_node.connections.contains_key(to_id) {
                                            connections_to_add
                                                .push((from_id.clone(), to_id.clone()));
                                        }
                                        if !to_node.connections.contains_key(from_id) {
                                            connections_to_add
                                                .push((to_id.clone(), from_id.clone()));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut new_connections_added = false;
        for (from, to) in connections_to_add {
            if graph.add_connection(from, to).is_ok() {
                new_connections_added = true;
            }
        }

        if new_connections_added {
            graph.topology_version += 1;

            self.path_cache.write()?.clear();
        }

        Ok(())
    }
}
