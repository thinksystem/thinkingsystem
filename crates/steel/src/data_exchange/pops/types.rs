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



use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;


pub type NodeID = String;
pub type Signature = String;
pub type TaskID = u64;


pub const W_DISTANCE: f64 = 1.0;
pub const W_LATENCY: f64 = 1.5;
pub const W_HEALTH_PENALTY: f64 = 200.0;
pub const W_SLASH_PENALTY: f64 = 500.0;


#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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


#[derive(Debug)]
pub enum PoPSMessage {

    AcceptOffering {
        offering: PublishedOffering,
        task_id: TaskID,
        responder: oneshot::Sender<Result<StakedTask, String>>,
    },

    Execute {
        task: StakedTask,
        payload: String,
        responder: oneshot::Sender<Result<(String, ProofOfPerformanceReceipt), String>>,
    },
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: NodeID,
    pub name: String,
    pub location: Location,
    pub latency: u32,
    pub health_score: f64,
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeState {
    pub id: NodeID,
    pub position: (f64, f64),
    pub latency: f64,
    pub connections: Vec<(NodeID, f64)>,
    pub last_updated: u64,
    pub health_score: f64,

    pub trust_score: TrustScore,
    pub slash_count: u32,

    pub level: usize,
    pub ch_forward_connections: Vec<(NodeID, f64)>,
    pub ch_backward_connections: Vec<(NodeID, f64)>,

    #[serde(skip, default = "Instant::now")]
    pub last_seen: Instant,
}


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TrustScore {

    pub value: f64,

    pub last_updated_ts: u64,
}

impl Default for TrustScore {
    fn default() -> Self {
        TrustScore {
            value: 0.1,
            last_updated_ts: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PenaltyCurve {

    pub steepness: f64,

    pub centre: f64,
}


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SlaGuarantees {
    pub max_latency_ms: u32,
    pub min_tps: f64,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedOffering {
    pub offering_id: String,
    pub node_id: NodeID,
    pub promised_guarantees: SlaGuarantees,
    pub penalty_curve: PenaltyCurve,
    pub reward: u64,
    pub collateral: u64,
    #[serde(skip, default = "SystemTime::now")]
    pub last_updated: SystemTime,
}


#[derive(Debug, Clone)]
pub struct StakedTask {
    pub task_id: TaskID,
    pub offering: PublishedOffering,
    pub consumer_id: NodeID,
    pub client_signature: Signature,
}


#[derive(Debug, Clone)]
pub struct ProofOfPerformanceReceipt {
    pub task_id: TaskID,
    pub node_id: NodeID,
    pub time_to_first_byte_ms: u32,
    pub final_tps: f64,

    pub output_hash: [u8; 32],
    pub node_signature: Signature,
}


#[derive(Debug, Clone, Copy)]
pub struct SupplierPricingFactors {
    pub base_reward: u64,

    pub confidence: f64,

    pub reputation_bonus_multiplier: f64,
}


#[derive(Debug, Clone, Copy)]
pub struct ConsumerFactors {

    pub cost_of_failure: f64,
}


#[derive(Debug, Clone)]
pub struct OptimalPath {
    pub nodes: Vec<NodeID>,
    pub total_cost: f64,
}


#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub total_nodes: usize,
    pub total_connections: usize,
    pub topology_version: u64,
    pub avg_health_score: f64,
}
