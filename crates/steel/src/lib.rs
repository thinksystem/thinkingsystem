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

pub mod data_exchange;
pub mod iam;
pub mod llm;
#[cfg(feature = "surrealdb")]
pub mod messaging;
pub mod policy;

pub use iam::{
    Claims, DidDocument, JwtManager, TokenError, VcManager, VerifiableCredential,
};
#[cfg(feature = "surrealdb")]
pub use iam::IdentityProvider;

pub use llm::security::SecurityProcessor;
pub use llm::{AnthropicClient, ApiClient, OpenAIClient};

#[cfg(feature = "surrealdb")]
pub use messaging::{
    AlertPriority, ClientStatus, ContentAnalyser, ContentAnalysis, ManagerConfig,
    MessageDestination, MessageManager, MessageMetadata, MessageProcessor, MessageRouter,
    MessageType, MessagingClient, MetadataValue, NetworkStats, OptimalPath,
    PathfindingNetworkManager, PlatformManager,
};
#[cfg(feature = "surrealdb")]
pub use messaging::MessagingApp;

pub use policy::{AuthorisationDecision, PolicyEngine, PolicyLoader};

pub use data_exchange::*;
