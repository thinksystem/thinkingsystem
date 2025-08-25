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

pub mod blocks;
#[cfg(feature = "nlu_builders")]
pub mod builders;
pub mod database;
pub mod flows;
pub mod codegen;
pub mod graphs;
pub mod kg_service;
pub mod llm;
pub mod memory;
pub mod nlu;
pub mod policy;
pub mod provenance; 
#[cfg(feature = "hybrid_relationships")]
pub mod relationships;
pub mod scribes;
pub mod util; 
pub use blocks::types::*;
pub use blocks::{
    BaseBlock, BlockBehaviour, BlockError, BlockInput, BlockRegistry, BlockResult, BlockType,
    FlowExecutor, FlowFactory,
};
pub use database::structured_store::StructuredStore; 
pub use database::{
    DatabaseConnection, DatabaseError as SteleDbError, DatabaseMetrics,
    QueryResult as SteleQueryResult, SurrealToken, SurrealTokenParser,
};
pub use estel::*;
pub use flows::{
    ChannelState, FlowBuilder, FlowDefinition, FlowStateManager, SecurityConfig, UnifiedFlowEngine,
};
pub use kg_service::{KgFact, KgIngestSummary, KgQueryFilter, KgQueryResult, KgService};
pub use nlu::{DatabaseInterface, NLUOrchestrator, QueryProcessor};
pub use policy::*;
pub use scribes::*;
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LLMConfig {
    pub model_name: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub api_key: Option<String>,
}
impl Default for LLMConfig {
    fn default() -> Self {
        Self {
            model_name: "gpt-4".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
            api_key: None,
        }
    }
}
