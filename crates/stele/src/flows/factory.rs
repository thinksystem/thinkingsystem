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

use crate::blocks::rules::BlockError;
use crate::flows::engine::UnifiedFlowEngine;
use crate::flows::state::UnifiedState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, RwLockWriteGuard};
use uuid::Uuid;
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Input {
    pub text: String,
    pub metadata: HashMap<String, JsonValue>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseMetadata {
    request_id: String,
    processing_time: u64,
    server_timestamp: DateTime<Utc>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct PaginationLinks {
    next: Option<String>,
    prev: Option<String>,
    first: String,
    last: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct EnhancedResponse<T> {
    data: T,
    metadata: ResponseMetadata,
    links: Option<PaginationLinks>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct BulkFlowRequest {
    flows: Vec<FlowOperation>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct FlowOperation {
    flow_id: String,
    input: Input,
}
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<JsonValue>,
    pub request_id: String,
    #[serde(skip)]
    status: StatusCode,
}
impl ApiError {
    fn from_engine_error(e: BlockError, request_id: String) -> Self {
        let (status, code, message) = match e {
            BlockError::BlockNotFound(id) => (
                StatusCode::NOT_FOUND,
                "BLOCK_NOT_FOUND",
                format!("The required block '{id}' could not be found in the flow."),
            ),
            BlockError::MissingProperty(prop) => (
                StatusCode::BAD_REQUEST,
                "MISSING_PROPERTY",
                format!("A required property is missing: {prop}"),
            ),
            BlockError::InvalidPropertyType(prop) => (
                StatusCode::BAD_REQUEST,
                "INVALID_PROPERTY_TYPE",
                format!("A property has an invalid type: {prop}"),
            ),
            BlockError::ProcessingError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "FLOW_PROCESSING_ERROR",
                msg,
            ),
            BlockError::ApiRequestError(msg) => {
                (StatusCode::BAD_GATEWAY, "EXTERNAL_API_ERROR", msg)
            }
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_FLOW_ERROR",
                e.to_string(),
            ),
        };
        Self {
            code: code.to_string(),
            message,
            details: None,
            request_id,
            status,
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status;
        let body = Json(self);
        (status, body).into_response()
    }
}
impl<T: Serialize> IntoResponse for EnhancedResponse<T> {
    fn into_response(self) -> Response {
        let status = StatusCode::OK;
        let body = Json(self);
        (status, body).into_response()
    }
}
#[derive(Debug)]
struct FlowLock {
    owner: String,
    timestamp: DateTime<Utc>,
    timeout: Duration,
}
struct FlowLockGuard<'a> {
    flow_id: String,
    locks: RwLockWriteGuard<'a, HashMap<String, FlowLock>>,
}
impl<'a> Drop for FlowLockGuard<'a> {
    fn drop(&mut self) {
        self.locks.remove(&self.flow_id);
    }
}
pub struct FlowExecutor {
    engine: Arc<UnifiedFlowEngine>,
    active_flows: RwLock<HashMap<String, FlowLock>>,
}
impl FlowExecutor {
    pub fn new(engine: UnifiedFlowEngine) -> Self {
        Self {
            engine: Arc::new(engine),
            active_flows: RwLock::new(HashMap::new()),
        }
    }
    async fn acquire_flow_lock<'a>(
        &'a self,
        flow_id: &str,
        owner: String,
        request_id: String,
    ) -> Result<FlowLockGuard<'a>, ApiError> {
        let mut locks = self.active_flows.write().await;
        if let Some(lock) = locks.get(flow_id) {
            if lock.timestamp + lock.timeout > Utc::now() {
                return Err(ApiError {
                    code: "FLOW_LOCKED".to_string(),
                    message: "Flow is locked by another process".to_string(),
                    details: Some(json!({ "owner": lock.owner })),
                    request_id,
                    status: StatusCode::CONFLICT,
                });
            }
        }
        locks.insert(
            flow_id.to_string(),
            FlowLock {
                owner,
                timestamp: Utc::now(),
                timeout: Duration::from_secs(30),
            },
        );
        Ok(FlowLockGuard {
            flow_id: flow_id.to_string(),
            locks,
        })
    }
    async fn execute_flow(
        &self,
        flow_id: String,
        mut state: UnifiedState,
    ) -> Result<EnhancedResponse<JsonValue>, ApiError> {
        let start_time = Instant::now();
        let request_id = Uuid::new_v4().to_string();
        let _lock_guard = self
            .acquire_flow_lock(&flow_id, request_id.clone(), request_id.clone())
            .await?;
        let result = self.engine.process_flow(&flow_id, &mut state).await;
        match result {
            Ok(_) => Ok(EnhancedResponse {
                data: serde_json::to_value(state.get_flow_state()).unwrap_or_default(),
                metadata: ResponseMetadata {
                    request_id,
                    processing_time: start_time.elapsed().as_millis() as u64,
                    server_timestamp: Utc::now(),
                },
                links: None,
            }),
            Err(e) => Err(ApiError::from_engine_error(e, request_id)),
        }
    }
    pub async fn execute_flows_batch(
        &self,
        batch: Vec<FlowOperation>,
    ) -> Vec<Result<EnhancedResponse<JsonValue>, ApiError>> {
        futures::stream::iter(batch)
            .map(|op| {
                let state = create_state_from_input(&op.input, op.flow_id.clone());
                self.execute_flow(op.flow_id, state)
            })
            .buffer_unordered(10)
            .collect()
            .await
    }
}
fn create_state_from_input(input: &Input, flow_id: String) -> UnifiedState {
    UnifiedState::new(
        input
            .metadata
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        input
            .metadata
            .get("operator_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        input
            .metadata
            .get("channel_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    )
    .with_flow(flow_id)
}
async fn handle_single_flow(
    State(executor): State<Arc<FlowExecutor>>,
    Path(flow_id): Path<String>,
    Json(input): Json<Input>,
) -> impl IntoResponse {
    let state = create_state_from_input(&input, flow_id.clone());
    executor.execute_flow(flow_id, state).await
}
async fn handle_batch_flow(
    State(executor): State<Arc<FlowExecutor>>,
    Json(batch): Json<BulkFlowRequest>,
) -> impl IntoResponse {
    let results = executor.execute_flows_batch(batch.flows).await;
    Json(results)
}
async fn handle_flow_status(
    State(executor): State<Arc<FlowExecutor>>,
    Path(flow_id): Path<String>,
) -> impl IntoResponse {
    let locks = executor.active_flows.read().await;
    let is_locked = locks.contains_key(&flow_id);
    EnhancedResponse {
        data: json!({ "locked": is_locked }),
        metadata: ResponseMetadata {
            request_id: Uuid::new_v4().to_string(),
            processing_time: 0,
            server_timestamp: Utc::now(),
        },
        links: None,
    }
}
pub struct FlowFactory {
    executor: Arc<FlowExecutor>,
}
impl FlowFactory {
    pub fn new(engine: UnifiedFlowEngine) -> Self {
        Self {
            executor: Arc::new(FlowExecutor::new(engine)),
        }
    }
    pub fn create_routes(self) -> Router {
        Router::new()
            .route("/v1/flows/:flow_id", post(handle_single_flow))
            .route("/v1/flows/batch", post(handle_batch_flow))
            .route("/v1/flows/:flow_id/status", get(handle_flow_status))
            .with_state(self.executor)
    }
}
