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

use crate::database::dynamic_storage::DynamicStorage;
use crate::nlu::orchestrator::{NLUOrchestrator, OrchestratorError, UnifiedNLUData};
use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{error, info, instrument};
#[derive(Error, Debug)]
pub enum QueryProcessorError {
    #[error("NLU Orchestrator failed: {0}")]
    Nlu(#[from] OrchestratorError),
    #[error("Dynamic storage failed: {0}")]
    Storage(String),
    #[error("Failed to serialise NLU data: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Configuration error: {0}")]
    Config(String),
}
pub type Result<T> = std::result::Result<T, QueryProcessorError>;
#[derive(Clone)]
pub struct QueryProcessor {
    orchestrator: Arc<RwLock<NLUOrchestrator>>,
    storage: Arc<DynamicStorage>,
}
impl QueryProcessor {
    pub async fn new(
        orchestrator: Arc<RwLock<NLUOrchestrator>>,
        storage: Arc<DynamicStorage>,
        _config_path: &str,
    ) -> Result<Self> {
        info!("Initialising new lean QueryProcessor");
        Ok(Self {
            orchestrator,
            storage,
        })
    }
    #[instrument(skip(self, input, user_id, channel), fields(input_length = input.len(), user_id = %user_id, channel = %channel))]
    pub async fn process_and_store_input(
        &self,
        input: &str,
        user_id: &str,
        channel: &str,
    ) -> Result<Value> {
        info!("Step 1: Processing input with NLU orchestrator");
        let unified_nlu_data: UnifiedNLUData = {
            let orchestrator = self.orchestrator.read().await;
            orchestrator.process_input(input).await?
        };
        info!(
            nodes = unified_nlu_data.extracted_data.nodes.len(),
            relationships = unified_nlu_data.extracted_data.relationships.len(),
            "Step 2: NLU processing complete, preparing for storage"
        );
        info!(user_id = %user_id, channel = %channel, "Step 3: Storing complete NLU output dynamically");
        let storage_result = self
            .storage
            .store_llm_output(user_id, channel, input, &unified_nlu_data)
            .await
            .map_err(QueryProcessorError::Storage)?;
        info!("Step 4: Dynamic storage successful");
        Ok(storage_result)
    }
    #[instrument(skip(self, instruction))]
    pub async fn process_instruction(&self, instruction: &str) -> Result<UnifiedNLUData> {
        info!("Processing instruction via public API for flow engine");
        let result = {
            let orchestrator = self.orchestrator.read().await;
            orchestrator.process_input(instruction).await?
        };
        Ok(result)
    }

    
    #[instrument(skip(self, unified_nlu_data), fields(user_id = %user_id, channel = %channel))]
    pub async fn store_nlu_data(
        &self,
        unified_nlu_data: &UnifiedNLUData,
        user_id: &str,
        channel: &str,
        raw_text: &str,
    ) -> Result<Value> {
        info!(
            nodes = unified_nlu_data.extracted_data.nodes.len(),
            relationships = unified_nlu_data.extracted_data.relationships.len(),
            "Storing pre-processed NLU data directly"
        );

        let storage_result = self
            .storage
            .store_llm_output(user_id, channel, raw_text, unified_nlu_data)
            .await
            .map_err(QueryProcessorError::Storage)?;

        info!("Direct NLU data storage successful");
        Ok(storage_result)
    }
}
