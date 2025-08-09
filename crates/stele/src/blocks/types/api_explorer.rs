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

use crate::blocks::{BlockBehaviour, BlockError, BlockResult};
use anyhow::Result;
use serde_json::{json, Value};
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tracing::{info, warn};

pub trait DataExchangeInterface: Send + Sync {
    fn explore_api<'life0, 'async_trait>(
        &'life0 self,
        endpoint: &'life0 str,
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait;
}

#[derive(Clone)]
pub struct APIExplorerBlock {
    id: String,
    data_exchange: Arc<dyn DataExchangeInterface>,
}

impl APIExplorerBlock {
    pub fn new(id: String, data_exchange: Arc<dyn DataExchangeInterface>) -> Self {
        Self { id, data_exchange }
    }
}

impl BlockBehaviour for APIExplorerBlock {
    fn id(&self) -> &str {
        &self.id
    }

    fn clone_box(&self) -> Box<dyn BlockBehaviour> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn validate(&self) -> Result<(), BlockError> {
        if self.id.is_empty() {
            return Err(BlockError::ValidationError(
                "Block ID cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    fn process<'life0, 'async_trait>(
        &'life0 self,
        state: &'life0 mut HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<BlockResult, BlockError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        let data_exchange = self.data_exchange.clone();
        let id = self.id.clone();

        Box::pin(async move {
            info!("API Explorer Block: {}", id);

            let endpoint = state
                .get("api_endpoint")
                .and_then(|v| v.as_str())
                .unwrap_or("/get")
                .to_string();

            info!("Exploring API endpoint: {}", endpoint);

            match data_exchange.explore_api(&endpoint).await {
                Ok(api_response) => {
                    state.insert("api_response".to_string(), api_response);
                    state.insert(
                        "api_exploration_timestamp".to_string(),
                        json!(chrono::Utc::now().to_rfc3339()),
                    );
                    state.insert("api_exploration_success".to_string(), json!(true));

                    info!("API exploration completed successfully");
                    Ok(BlockResult::Move("content_generator_block".to_string()))
                }
                Err(e) => {
                    warn!("API exploration failed: {}", e);
                    state.insert("api_exploration_error".to_string(), json!(e.to_string()));
                    state.insert("api_exploration_success".to_string(), json!(false));

                    let mock_response = json!({
                        "type": "mock",
                        "message": "API exploration failed, using mock data",
                        "mock_data": {
                            "status": "ok",
                            "data": {
                                "timestamp": chrono::Utc::now().to_rfc3339(),
                                "mock": true
                            }
                        }
                    });

                    state.insert("api_response".to_string(), mock_response);
                    Ok(BlockResult::Move("content_generator_block".to_string()))
                }
            }
        })
    }
}
