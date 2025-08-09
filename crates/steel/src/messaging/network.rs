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

use crate::messaging::types::{Message, MessageDestination};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
pub enum NetworkError {
    ConnectionFailed(String),
    RoutingFailed(String),
    PeerNotFound(String),
    InvalidMessage(String),
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            NetworkError::ConnectionFailed(e) => write!(f, "Connection failed: {e}"),
            NetworkError::RoutingFailed(e) => write!(f, "Routing failed: {e}"),
            NetworkError::PeerNotFound(e) => write!(f, "Peer not found: {e}"),
            NetworkError::InvalidMessage(e) => write!(f, "Invalid message: {e}"),
        }
    }
}

impl std::error::Error for NetworkError {}

#[derive(Debug, Clone)]
pub enum RoutingStrategy {
    Direct,
    Broadcast,
    RoundRobin,
    HashBased,
}

#[derive(Debug, Clone)]
pub enum RouteType {
    Direct,
    Broadcast,
    Multicast,
}

#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub route_type: RouteType,
    pub target_peers: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum DeliveryMethod {
    Local,
    P2P,
    Relay,
    Platform(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relay {
    pub address: String,
    pub permanence_hours: u32,
    pub public_key: String,
    pub last_seen: DateTime<Utc>,
    pub reliability_score: f32,
    pub max_hops: u32,
}

impl Relay {
    pub fn new(address: String, public_key: String) -> Self {
        Self {
            address,
            permanence_hours: 24,
            public_key,
            last_seen: Utc::now(),
            reliability_score: 1.0,
            max_hops: 10,
        }
    }

    pub fn matches_requirements(&self, required_hours: u32) -> bool {
        self.permanence_hours >= required_hours
    }

    pub fn is_available(&self) -> bool {
        let now = Utc::now();
        let threshold = chrono::Duration::hours(1);
        now.signed_duration_since(self.last_seen) < threshold
    }

    pub fn update_reliability(&mut self, success: bool) {
        if success {
            self.reliability_score = (self.reliability_score * 0.9 + 0.1).min(1.0);
        } else {
            self.reliability_score = (self.reliability_score * 0.9).max(0.1);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub address: String,
    pub public_key: String,
    pub last_seen: DateTime<Utc>,
    pub capabilities: Vec<String>,
    pub trust_score: f32,
}

impl PeerInfo {
    pub fn new(peer_id: String, address: String, public_key: String) -> Self {
        Self {
            peer_id,
            address,
            public_key,
            last_seen: Utc::now(),
            capabilities: vec!["messaging".to_string()],
            trust_score: 0.5,
        }
    }

    pub fn is_online(&self) -> bool {
        let now = Utc::now();
        let threshold = chrono::Duration::minutes(5);
        now.signed_duration_since(self.last_seen) < threshold
    }
}

pub struct MessageRouter {
    routing_table: Arc<RwLock<HashMap<String, String>>>,
    relays: Arc<RwLock<Vec<Relay>>>,
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
    node_count: usize,
}

impl MessageRouter {
    pub fn new(node_count: usize) -> Self {
        Self {
            routing_table: Arc::new(RwLock::new(HashMap::new())),
            relays: Arc::new(RwLock::new(Vec::new())),
            peers: Arc::new(RwLock::new(HashMap::new())),
            node_count,
        }
    }

    pub async fn route_message(&self, message: &Message) -> Result<RoutingDecision, NetworkError> {
        let strategy = self.determine_routing_strategy(message).await;
        let destinations = self.resolve_destinations(message).await?;

        match strategy {
            RoutingStrategy::Direct => Ok(RoutingDecision {
                route_type: RouteType::Direct,
                target_peers: destinations,
            }),
            RoutingStrategy::Broadcast => Ok(RoutingDecision {
                route_type: RouteType::Broadcast,
                target_peers: (0..self.node_count).map(|i| format!("node-{i}")).collect(),
            }),
            RoutingStrategy::RoundRobin => {
                let node_id = self.next_round_robin_node().await;
                Ok(RoutingDecision {
                    route_type: RouteType::Multicast,
                    target_peers: vec![format!("node-{node_id}")],
                })
            }
            RoutingStrategy::HashBased => {
                let node_id = self.hash_based_node(&message.sender);
                Ok(RoutingDecision {
                    route_type: RouteType::Direct,
                    target_peers: vec![format!("node-{node_id}")],
                })
            }
        }
    }

    pub async fn broadcast_message(&self, message: &Message) -> Result<(), NetworkError> {
        let peers = self.peers.read().await;
        for (peer_id, _peer_info) in peers.iter() {
            println!("Broadcasting message {} to peer {}", message.mid, peer_id);
        }
        Ok(())
    }

    pub async fn add_relay(&self, relay: Relay) {
        let mut relays = self.relays.write().await;
        relays.push(relay);
    }

    pub async fn add_peer(&self, peer: PeerInfo) {
        let mut peers = self.peers.write().await;
        peers.insert(peer.peer_id.clone(), peer);
    }

    pub async fn get_best_relay(&self, requirements: u32) -> Option<Relay> {
        let relays = self.relays.read().await;
        relays
            .iter()
            .filter(|r| r.matches_requirements(requirements) && r.is_available())
            .max_by(|a, b| {
                a.reliability_score
                    .partial_cmp(&b.reliability_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
    }

    pub async fn determine_delivery_method(&self, message: &Message) -> DeliveryMethod {
        match &message.destination {
            MessageDestination::Single(recipient) => {
                let peers = self.peers.read().await;
                if peers.contains_key(recipient) {
                    DeliveryMethod::P2P
                } else {
                    DeliveryMethod::Relay
                }
            }
            MessageDestination::Multiple(recipients) => {
                if recipients.len() > 10 {
                    DeliveryMethod::Relay
                } else {
                    DeliveryMethod::P2P
                }
            }
        }
    }

    async fn determine_routing_strategy(&self, message: &Message) -> RoutingStrategy {
        match &message.destination {
            MessageDestination::Single(_) => RoutingStrategy::Direct,
            MessageDestination::Multiple(recipients) if recipients.len() > self.node_count => {
                RoutingStrategy::Broadcast
            }
            MessageDestination::Multiple(_) => RoutingStrategy::HashBased,
        }
    }

    async fn resolve_destinations(&self, message: &Message) -> Result<Vec<String>, NetworkError> {
        match &message.destination {
            MessageDestination::Single(recipient) => {
                let node = self.get_or_assign_node(recipient).await?;
                Ok(vec![node])
            }
            MessageDestination::Multiple(recipients) => {
                let mut destinations = Vec::new();
                for recipient in recipients {
                    let node = self.get_or_assign_node(recipient).await?;
                    destinations.push(node);
                }
                Ok(destinations)
            }
        }
    }

    async fn get_or_assign_node(&self, recipient: &str) -> Result<String, NetworkError> {
        let mut table = self.routing_table.write().await;
        if let Some(node) = table.get(recipient) {
            Ok(node.clone())
        } else {
            let node_id = self.hash_based_node(recipient);
            let node = format!("node-{node_id}");
            table.insert(recipient.to_string(), node.clone());
            Ok(node)
        }
    }

    async fn next_round_robin_node(&self) -> usize {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        (now.as_secs() as usize) % self.node_count
    }

    fn hash_based_node(&self, key: &str) -> usize {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.node_count
    }

    pub async fn update_peer_status(
        &self,
        peer_id: &str,
        online: bool,
    ) -> Result<(), NetworkError> {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(peer_id) {
            if online {
                peer.last_seen = Utc::now();
            }
            Ok(())
        } else {
            Err(NetworkError::PeerNotFound(peer_id.to_string()))
        }
    }

    pub async fn get_online_peers(&self) -> Vec<PeerInfo> {
        let peers = self.peers.read().await;
        peers.values().filter(|p| p.is_online()).cloned().collect()
    }

    pub async fn cleanup_stale_peers(&self) -> usize {
        let mut peers = self.peers.write().await;
        let initial_count = peers.len();

        peers.retain(|_, peer| peer.is_online());

        initial_count - peers.len()
    }
}

pub struct NetworkManager {
    router: MessageRouter,
    local_peer_id: String,
}

impl NetworkManager {
    pub fn new(node_count: usize, local_peer_id: String) -> Self {
        Self {
            router: MessageRouter::new(node_count),
            local_peer_id,
        }
    }

    pub async fn send_message(&self, message: Message) -> Result<(), NetworkError> {
        let delivery_method = self.router.determine_delivery_method(&message).await;
        let routing_decision = self.router.route_message(&message).await?;

        println!(
            "Local peer {} sending message {}",
            self.local_peer_id, message.mid
        );

        match delivery_method {
            DeliveryMethod::Local => {
                if routing_decision.target_peers.contains(&self.local_peer_id) {
                    println!(
                        "Message {} delivered to local peer {}",
                        message.mid, self.local_peer_id
                    );
                } else {
                    println!("Delivering message locally");
                }
                Ok(())
            }
            DeliveryMethod::P2P => {
                for destination in &routing_decision.target_peers {
                    println!(
                        "Sending message to peer: {} with content: {}",
                        destination, message.content
                    );
                }
                Ok(())
            }
            DeliveryMethod::Relay => {
                if let Some(relay) = self.router.get_best_relay(24).await {
                    println!("Sending message via relay: {}", relay.address);
                    Ok(())
                } else {
                    Err(NetworkError::RoutingFailed(
                        "No suitable relay found".to_string(),
                    ))
                }
            }
            DeliveryMethod::Platform(platform) => {
                println!("Sending message via platform: {platform}");
                Ok(())
            }
        }
    }

    pub async fn send_to_peers(
        &self,
        message: Message,
        target_peers: &[String],
    ) -> Result<(), NetworkError> {
        for peer_id in target_peers {
            if peer_id != &self.local_peer_id {
                println!(
                    "Local peer {} sending message {} to specific peer: {}",
                    self.local_peer_id, message.mid, peer_id
                );
            } else {
                println!("Skipping send to self (local peer {})", self.local_peer_id);
            }
        }
        Ok(())
    }

    pub fn get_local_peer_id(&self) -> &str {
        &self.local_peer_id
    }

    pub async fn register_local_peer(&self) {
        let local_peer = PeerInfo {
            peer_id: self.local_peer_id.clone(),
            address: format!("local://{}", self.local_peer_id),
            public_key: format!("pubkey_{}", self.local_peer_id),
            last_seen: chrono::Utc::now(),
            capabilities: vec!["messaging".to_string(), "routing".to_string()],
            trust_score: 1.0,
        };
        self.router.add_peer(local_peer).await;
        println!("Registered local peer: {}", self.local_peer_id);
    }

    pub async fn add_relay(&self, relay: Relay) {
        self.router.add_relay(relay).await;
    }

    pub async fn add_peer(&self, peer: PeerInfo) {
        self.router.add_peer(peer).await;
    }

    pub fn get_router(&self) -> &MessageRouter {
        &self.router
    }
}
