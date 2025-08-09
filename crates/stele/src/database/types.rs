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
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use surrealdb::engine::remote::ws::Client;
use surrealdb::Surreal;
use tokio::sync::{mpsc, oneshot};
#[derive(Debug)]
pub enum DatabaseError {
    ConnectionFailed(String),
    Query(String),
    QueryFailed(String),
    TransactionFailed(String),
    SerialisationError(String),
    ValidationError(String),
    Timeout,
    Disconnected,
}
impl std::fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseError::ConnectionFailed(msg) => write!(f, "Connection failed: {msg}"),
            DatabaseError::Query(msg) => write!(f, "Query error: {msg}"),
            DatabaseError::QueryFailed(msg) => write!(f, "Query failed: {msg}"),
            DatabaseError::TransactionFailed(msg) => write!(f, "Transaction failed: {msg}"),
            DatabaseError::SerialisationError(msg) => write!(f, "Serialisation error: {msg}"),
            DatabaseError::ValidationError(msg) => write!(f, "Validation error: {msg}"),
            DatabaseError::Timeout => write!(f, "Operation timed out"),
            DatabaseError::Disconnected => write!(f, "Database disconnected"),
        }
    }
}
impl std::error::Error for DatabaseError {}
#[derive(Debug)]
pub enum DatabaseCommand {
    Connect {
        client_sender: mpsc::Sender<Arc<Surreal<Client>>>,
        response_sender: oneshot::Sender<Result<(), DatabaseError>>,
    },
    Disconnect {
        response_sender: oneshot::Sender<Result<(), DatabaseError>>,
    },
    TestQuery {
        response_sender: oneshot::Sender<Result<(), DatabaseError>>,
    },
    Transaction {
        query: String,
        params: Option<Value>,
        response_sender: oneshot::Sender<Result<Value, DatabaseError>>,
    },
    Retrieve {
        query: String,
        response_sender: oneshot::Sender<Result<Value, DatabaseError>>,
    },
}
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ConnectionStatus {
    Connected,
    Connecting,
    Disconnected,
    TransactionInProgress,
    TransactionCommitted,
    TransactionRolledBack,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Response {
    token: String,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QueryResult<T> {
    pub data: Vec<T>,
    pub metadata: HashMap<String, String>,
    pub execution_time: f64,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DatabaseMetrics {
    pub query_count: u64,
    pub average_response_time: f64,
    pub success_rate: f32,
    pub transaction_count: u64,
    pub transaction_failure_rate: f32,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransactionMetrics {
    pub transaction_id: String,
    pub start_time: String,
    pub end_time: Option<String>,
    pub operations_count: u32,
    pub status: TransactionStatus,
    pub error_message: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum TransactionStatus {
    Started,
    InProgress,
    Committed,
    RolledBack,
    Failed,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VectorEmbedding {
    pub vector: Vec<f32>,
    pub timestamp: String,
    pub source_id: String,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SemanticRelation {
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,
    pub confidence: f32,
    pub metadata: HashMap<String, Value>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TemporalContext {
    pub timestamp: String,
    pub context_type: String,
    pub duration: Option<String>,
    pub recurring: bool,
    pub metadata: HashMap<String, Value>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QueryIntent {
    pub intent: String,
    pub domain: String,
    pub expected_answer_type: String,
    pub confidence: f32,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TokenNode {
    pub category: String,
    pub value: String,
    pub confidence: f32,
    pub metadata: HashMap<String, Value>,
    pub subcategories: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContentNode {
    pub raw_text: String,
    pub node_type: String,
    pub sentiment: f32,
    pub topics: Vec<String>,
    pub timestamp: String,
}
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct DatabaseStats {
    pub total_nodes: u64,
    pub total_relationships: u64,
    pub storage_used: u64,
    pub index_size: u64,
    pub last_backup: String,
    pub active_transactions: u32,
    pub total_transactions: u64,
}
