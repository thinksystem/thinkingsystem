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

use crate::database::{
    connection::DatabaseConnection,
    data_processor::{DataProcessorError, QueryWithParams},
    types::{DatabaseCommand, DatabaseError, DatabaseStats},
};
use serde_json::Value;
use std::sync::Arc;
use surrealdb::{engine::remote::ws::Client, Surreal};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
#[derive(Debug)]
pub enum DatabaseTaskError {
    TaskPanicked,
    TaskFinished(DatabaseError),
    TaskAborted,
}
impl std::fmt::Display for DatabaseTaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseTaskError::TaskPanicked => write!(f, "Database task panicked"),
            DatabaseTaskError::TaskFinished(e) => {
                write!(f, "Database task finished with error: {e}")
            }
            DatabaseTaskError::TaskAborted => write!(f, "Database task was aborted"),
        }
    }
}
impl std::error::Error for DatabaseTaskError {}
pub struct DatabaseInterface {
    pub command_tx: mpsc::Sender<DatabaseCommand>,
    pub db_stats: DatabaseStats,
    client: Arc<Surreal<Client>>,
    db_task_handle: JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
}
impl DatabaseInterface {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (command_tx, command_rx) = mpsc::channel(100);
        let (client_tx, mut client_rx) = mpsc::channel(1);
        let mut db_conn = DatabaseConnection::new(command_rx);
        let db_task_handle = tokio::spawn(async move {
            db_conn
                .run()
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })
        });
        let (connect_response_tx, connect_response_rx) = oneshot::channel();
        command_tx
            .send(DatabaseCommand::Connect {
                client_sender: client_tx,
                response_sender: connect_response_tx,
            })
            .await
            .map_err(|e| format!("Failed to send connect command: {e}"))?;
        connect_response_rx
            .await
            .map_err(|e| format!("Failed to receive connect response: {e}"))?
            .map_err(|e| format!("Database connection failed: {e}"))?;
        let client = client_rx
            .recv()
            .await
            .ok_or("Failed to receive database client from connection task")?;
        Ok(Self {
            command_tx,
            db_stats: DatabaseStats::default(),
            client,
            db_task_handle,
        })
    }
    pub fn get_client(&self) -> Arc<Surreal<Client>> {
        self.client.clone()
    }
    pub async fn execute_query(
        &self,
        query_with_params: QueryWithParams,
    ) -> Result<Value, DataProcessorError> {
        let (response_tx, response_rx) = oneshot::channel();
        let command = DatabaseCommand::Transaction {
            query: query_with_params.query,
            params: Some(Value::Object(
                query_with_params.params.into_iter().collect(),
            )),
            response_sender: response_tx,
        };
        self.command_tx
            .send(command)
            .await
            .map_err(|e| DataProcessorError::Processing(format!("Failed to send command: {e}")))?;
        let result = response_rx.await.map_err(|e| {
            DataProcessorError::Processing(format!("Failed to receive response: {e}"))
        })?;
        result.map_err(|e| DataProcessorError::Processing(format!("Database error: {e}")))
    }
    pub async fn shutdown(&self) -> Result<(), String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(DatabaseCommand::Disconnect {
                response_sender: response_tx,
            })
            .await
            .map_err(|e| format!("Failed to send disconnect command: {e}"))?;
        response_rx
            .await
            .map_err(|e| format!("Failed to receive disconnect response: {e}"))?
            .map_err(|e| format!("Database disconnect failed: {e}"))?;
        self.db_task_handle.abort();
        Ok(())
    }
    pub async fn test_connection(&self) -> Result<(), DatabaseError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(DatabaseCommand::TestQuery {
                response_sender: response_tx,
            })
            .await
            .map_err(|_| DatabaseError::Disconnected)?;
        response_rx.await.map_err(|_| DatabaseError::Disconnected)?
    }
    pub async fn retrieve_data(&self, query: String) -> Result<Value, DatabaseError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(DatabaseCommand::Retrieve {
                query,
                response_sender: response_tx,
            })
            .await
            .map_err(|_| DatabaseError::Disconnected)?;
        response_rx.await.map_err(|_| DatabaseError::Disconnected)?
    }
    pub async fn execute_transaction(
        &self,
        query: String,
        params: Option<Value>,
    ) -> Result<Value, DatabaseError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(DatabaseCommand::Transaction {
                query,
                params,
                response_sender: response_tx,
            })
            .await
            .map_err(|_| DatabaseError::Disconnected)?;
        response_rx.await.map_err(|_| DatabaseError::Disconnected)?
    }
    pub async fn check_database_health(&self) -> Result<(), DatabaseError> {
        Ok(())
    }
    pub fn is_database_task_alive(&self) -> bool {
        !self.db_task_handle.is_finished()
    }
}
