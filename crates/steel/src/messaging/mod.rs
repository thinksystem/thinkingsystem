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

pub mod client;
#[cfg(feature = "surrealdb")]
pub mod database;
pub mod insight;
pub mod management;
pub mod network;
pub mod pathfinding;
pub mod platforms;
pub mod resilience;
pub mod types;

#[cfg(test)]
pub mod tests;

pub use management::{ManagerConfig, MessageManager, MessageProcessor};

pub use client::{AlertPriority, ClientStatus, MessagingClient};
#[cfg(feature = "surrealdb")]
pub use database::MessagingApp;
pub use insight::{ContentAnalyser, ContentAnalysis, MessageSecurity};
pub use network::{MessageRouter, NetworkManager, Relay};
pub use pathfinding::{NetworkStats, OptimalPath, PathfindingNetworkManager};
pub use platforms::{PlatformBridge, PlatformManager, PlatformType};
pub use resilience::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerManager, CircuitBreakerState,
};
pub use types::{
    EdgeLabel, GraphEdge, GraphNode, Message, MessageDestination, MessageMetadata, MessageType,
    MetadataValue, NodeType,
};
