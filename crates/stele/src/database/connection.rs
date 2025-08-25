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

use crate::database::operations::DatabaseOperations;
use crate::database::types::{DatabaseCommand, DatabaseError};
use dotenvy::dotenv;
use std::env;
use std::fs;
use std::sync::Arc;
use surrealdb::engine::remote::ws::{Client, Ws};
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;
use tokio::sync::mpsc::Receiver;
pub struct DatabaseConnection {
    pub client: Option<Arc<Surreal<Client>>>,
    pub operations: Option<DatabaseOperations>,
    command_receiver: Receiver<DatabaseCommand>,
}
impl DatabaseConnection {
    pub fn new(command_receiver: Receiver<DatabaseCommand>) -> Self {
        Self {
            client: None,
            operations: None,
            command_receiver,
        }
    }
    pub async fn run(&mut self) -> Result<(), DatabaseError> {
        println!("Starting database connection handler...");
        while let Some(command) = self.command_receiver.recv().await {
            match command {
                DatabaseCommand::Connect {
                    client_sender,
                    response_sender,
                } => {
                    if self.client.is_some() {
                        let _ = response_sender.send(Err(DatabaseError::ConnectionFailed(
                            "Already connected".to_string(),
                        )));
                        continue;
                    }
                    match self.connect().await {
                        Ok(_) => {
                            if let Some(client) = &self.client {
                                if client_sender.send(client.clone()).await.is_err() {
                                    eprintln!(
                                        "Failed to send database client back to main thread."
                                    );
                                    let _ =
                                        response_sender.send(Err(DatabaseError::ConnectionFailed(
                                            "Client channel closed".to_string(),
                                        )));
                                } else {
                                    let _ = response_sender.send(Ok(()));
                                }
                            } else {
                                let _ = response_sender.send(Err(DatabaseError::ConnectionFailed(
                                    "Client is None after successful connection".to_string(),
                                )));
                            }
                        }
                        Err(e) => {
                            let _ = response_sender.send(Err(e));
                        }
                    }
                }
                DatabaseCommand::Disconnect { response_sender } => {
                    println!("Disconnecting from database...");
                    self.client = None;
                    self.operations = None;
                    let _ = response_sender.send(Ok(()));
                    println!("Disconnected from database");
                }
                DatabaseCommand::TestQuery { response_sender } => {
                    if let Some(operations) = &self.operations {
                        match operations.test_connection().await {
                            Ok(_) => {
                                let _ = response_sender.send(Ok(()));
                                println!("Test query successful");
                            }
                            Err(e) => {
                                let _ = response_sender.send(Err(e));
                            }
                        }
                    } else {
                        let _ = response_sender.send(Err(DatabaseError::ConnectionFailed(
                            "Not connected to database".to_string(),
                        )));
                    }
                }
                DatabaseCommand::Transaction {
                    query,
                    params,
                    response_sender,
                } => {
                    if let Some(operations) = &self.operations {
                        match operations
                            .execute_transaction(&query, params.as_ref())
                            .await
                        {
                            Ok(result) => {
                                let _ = response_sender.send(Ok(result));
                                println!("Transaction executed successfully");
                            }
                            Err(e) => {
                                let _ = response_sender.send(Err(e));
                            }
                        }
                    } else {
                        let _ = response_sender.send(Err(DatabaseError::ConnectionFailed(
                            "Not connected to database".to_string(),
                        )));
                    }
                }
                DatabaseCommand::Retrieve {
                    query,
                    response_sender,
                } => {
                    if let Some(operations) = &self.operations {
                        match operations.execute_query(&query).await {
                            Ok(result) => {
                                let _ = response_sender.send(Ok(result));
                                println!("Query executed successfully");
                            }
                            Err(e) => {
                                let _ = response_sender.send(Err(e));
                            }
                        }
                    } else {
                        let _ = response_sender.send(Err(DatabaseError::ConnectionFailed(
                            "Not connected to database".to_string(),
                        )));
                    }
                }
            }
        }
        Ok(())
    }
    async fn connect(&mut self) -> Result<(), DatabaseError> {
        dotenv().ok();
        let endpoint = if let Ok(url) = env::var("SURREALDB_URL") {
            url.strip_prefix("ws://").unwrap_or(&url).to_string()
        } else {
            let host = env::var("SURREALDB_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
            let port = env::var("SURREALDB_PORT").unwrap_or_else(|_| "8000".to_string());
            format!("{host}:{port}")
        };
        let required = |key: &str| -> Result<String, DatabaseError> {
            env::var(key).map_err(|_| {
                DatabaseError::ConnectionFailed(format!(
                    "Missing required environment variable {key}"
                ))
            })
        };
        let user = required("SURREALDB_USER")?;
        let pass = required("SURREALDB_PASS")?;
        let ns = required("SURREALDB_NS")?;
        let db_name = required("SURREALDB_DB")?;
        println!("Attempting to connect to database at ws://{endpoint}");
        let db = Surreal::new::<Ws>(&endpoint).await.map_err(|e| {
            DatabaseError::ConnectionFailed(format!("Failed to create SurrealDB connection: {e}"))
        })?;
        db.signin(Root {
            username: &user,
            password: &pass,
        })
        .await
        .map_err(|e| DatabaseError::ConnectionFailed(format!("Failed to authenticate: {e}")))?;
        db.use_ns(&ns).use_db(&db_name).await.map_err(|e| {
            DatabaseError::ConnectionFailed(format!("Failed to select namespace/database: {e}"))
        })?;
        self.initialise_schema(&db).await?;
        let client = Arc::new(db);
        self.operations = Some(DatabaseOperations::new(client.clone()));
        self.client = Some(client);
        Ok(())
    }
    async fn initialise_schema(&self, client: &Surreal<Client>) -> Result<(), DatabaseError> {
        println!("Initialising dynamic graph schema (dynamic namespace)...");

        
        
        let allow_contamination = std::env::var("STELE_ALLOW_CONTAMINATION")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if !allow_contamination {
            match client.query("INFO FOR DB;").await {
                Ok(mut info) => {
                    let db_info: Option<serde_json::Value> = info.take(0).ok().flatten();
                    if let Some(serde_json::Value::Object(map)) = db_info {
                        if let Some(serde_json::Value::Object(tables)) = map.get("tables") {
                            
                            
                            let has_canonical = [
                                "canonical_entity",
                                "canonical_event",
                                "canonical_task",
                                "canonical_relationship_fact",
                            ]
                            .iter()
                            .any(|t| tables.contains_key(*t));
                            if has_canonical {
                                return Err(DatabaseError::ConnectionFailed(
                                    "Canonical tables detected in dynamic namespace. Refuse to start. Set STELE_ALLOW_CONTAMINATION=1 to override (not recommended)."
                                        .into(),
                                ));
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Failed to introspect DB for contamination check: {e}");
                }
            }
        }

        let embedded_schema: &str = include_str!("./config/dynamic_schema.sql");
        let schema_str = if !embedded_schema.trim().is_empty() {
            embedded_schema
        } else {
            let path = "crates/stele/src/database/config/dynamic_schema.sql";
            match fs::read_to_string(path) {
                Ok(s) => {
                    println!("Loaded schema from {path}");
                    Box::leak(s.into_boxed_str())
                }
                Err(_) => "",
            }
        };
        if schema_str.is_empty() {
            return Err(DatabaseError::ConnectionFailed(
                "Database schema not found or empty".to_string(),
            ));
        }
        match client.query(schema_str).await {
            Ok(_) => {
                println!("Database schema applied successfully.");
            }
            Err(e) => {
                return Err(DatabaseError::ConnectionFailed(format!(
                    "Failed to apply schema: {e}"
                )));
            }
        }
        println!("Dynamic schema initialisation completed.");
        Ok(())
    }
    pub async fn health_check(&self) -> Result<serde_json::Value, DatabaseError> {
        if let Some(operations) = &self.operations {
            operations.health_check().await
        } else {
            Err(DatabaseError::ConnectionFailed(
                "Not connected to database".to_string(),
            ))
        }
    }
    pub async fn get_statistics(&self) -> Result<serde_json::Value, DatabaseError> {
        if let Some(operations) = &self.operations {
            operations.get_comprehensive_stats().await
        } else {
            Err(DatabaseError::ConnectionFailed(
                "Not connected to database".to_string(),
            ))
        }
    }
}
