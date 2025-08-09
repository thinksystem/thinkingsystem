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
use std::collections::HashMap;
use std::sync::Arc;
use surrealdb::{engine::remote::ws::Client, Surreal};
use uuid::Uuid;
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DatabaseCommand {
    pub graph_type: GraphType,
    pub operation: Operation,
    pub entities: DatabaseEntities,
    pub metadata: HashMap<String, String>,
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum GraphType {
    Agent,
    Event,
    Personalisation,
    Knowledge,
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Operation {
    Create,
    Update,
    Delete,
    Query,
    Relate,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DatabaseEntities {
    pub node_data: Option<serde_json::Value>,
    pub edge_data: Option<serde_json::Value>,
    pub filters: HashMap<String, String>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QueryResult {
    pub data: serde_json::Value,
    pub metadata: HashMap<String, String>,
    pub execution_time_ms: u64,
}
pub struct DatabaseInterface {
    pub db: Arc<Surreal<Client>>,
}
impl DatabaseInterface {
    pub fn new(db: Arc<Surreal<Client>>) -> Self {
        Self { db }
    }
    pub async fn execute_batch_commands(
        &self,
        commands: &[DatabaseCommand],
    ) -> Result<Vec<QueryResult>, Box<dyn std::error::Error>> {
        if commands.is_empty() {
            return Ok(Vec::new());
        }
        if commands.len() == 1 {
            let result = self.execute_command(&commands[0]).await?;
            return Ok(vec![result]);
        }
        let mut transaction_queries = Vec::new();
        let mut results = Vec::new();
        for command in commands.iter() {
            match &command.operation {
                Operation::Create => {
                    if let Some(node_data) = &command.entities.node_data {
                        let table = match command.graph_type {
                            GraphType::Agent => "agent_nodes",
                            GraphType::Event => "event_nodes",
                            GraphType::Personalisation => "location_nodes",
                            GraphType::Knowledge => "relationship_nodes",
                        };
                        let uuid_str = uuid::Uuid::new_v4().to_string().replace("-", "");
                        let id = format!("{}_{}", table.trim_end_matches("_nodes"), uuid_str);
                        transaction_queries.push(format!(
                            "CREATE {}:{} CONTENT {} SET created_at = time::now()",
                            table,
                            id,
                            serde_json::to_string(node_data)?
                        ));
                    }
                }
                _ => {
                    let result = self.execute_command(command).await?;
                    results.push(result);
                }
            }
        }
        if !transaction_queries.is_empty() {
            let transaction = format!(
                "BEGIN TRANSACTION; {}; COMMIT TRANSACTION;",
                transaction_queries.join("; ")
            );
            let mut result = self.db.query(&transaction).await?;
            let _: Vec<serde_json::Value> = result.take(0)?;
            results.push(QueryResult {
                data: serde_json::json!({"status": "batch_executed", "count": transaction_queries.len()}),
                metadata: HashMap::from([("operation".to_string(), "batch_transaction".to_string())]),
                execution_time_ms: 0,
            });
        }
        Ok(results)
    }
    fn clean_record_id(id: &str) -> String {
        id.replace("-", "").to_lowercase()
    }
    pub async fn execute_command(
        &self,
        command: &DatabaseCommand,
    ) -> Result<QueryResult, Box<dyn std::error::Error>> {
        println!("DatabaseInterface::execute_command called with: {command:?}");
        let start_time = std::time::Instant::now();
        match &command.operation {
            Operation::Create => {
                let table = match command.graph_type {
                    GraphType::Agent => "agent_nodes",
                    GraphType::Event => "event_nodes",
                    GraphType::Personalisation => "location_nodes",
                    GraphType::Knowledge => "relationship_nodes",
                };
                if let Some(node_data) = &command.entities.node_data {
                    match self
                        .create_node(
                            table,
                            DatabaseEntities {
                                node_data: Some(node_data.clone()),
                                edge_data: command.entities.edge_data.clone(),
                                filters: command.entities.filters.clone(),
                            },
                        )
                        .await
                    {
                        Ok(result) => Ok(QueryResult {
                            data: result,
                            metadata: HashMap::from([(
                                "operation".to_string(),
                                "create".to_string(),
                            )]),
                            execution_time_ms: start_time.elapsed().as_millis() as u64,
                        }),
                        Err(e) => {
                            println!("create_node failed: {e}");
                            Ok(QueryResult {
                                data: serde_json::json!({
                                    "error": e.to_string(),
                                    "status": "failed"
                                }),
                                metadata: HashMap::from([(
                                    "operation".to_string(),
                                    "create".to_string(),
                                )]),
                                execution_time_ms: start_time.elapsed().as_millis() as u64,
                            })
                        }
                    }
                } else {
                    Ok(QueryResult {
                        data: serde_json::json!({
                            "error": "No node data provided for create operation",
                            "status": "failed"
                        }),
                        metadata: HashMap::from([("operation".to_string(), "create".to_string())]),
                        execution_time_ms: start_time.elapsed().as_millis() as u64,
                    })
                }
            }
            Operation::Query => {
                let table_name = match command.graph_type {
                    GraphType::Event => "event_nodes",
                    GraphType::Personalisation => "location_nodes",
                    GraphType::Agent => "entity_nodes",
                    GraphType::Knowledge => "content_nodes",
                };
                let mut query = format!("SELECT * FROM {table_name}");
                if !command.entities.filters.is_empty() {
                    let conditions: Vec<String> = command
                        .entities
                        .filters
                        .iter()
                        .map(|(key, value)| format!("{key} = '{value}'"))
                        .collect();
                    query.push_str(&format!(" WHERE {}", conditions.join(" AND ")));
                }
                println!("Executing query: {query}");
                let mut result = self.db.query(&query).await.map_err(|e| {
                    println!("SurrealDB query error: {e}");
                    e
                })?;
                let results: Vec<serde_json::Value> = result.take(0)?;
                Ok(QueryResult {
                    data: serde_json::json!({
                        "results": results,
                        "count": results.len()
                    }),
                    metadata: HashMap::from([("operation".to_string(), "query".to_string())]),
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                })
            }
            Operation::Update => {
                if let Some(node_data) = &command.entities.node_data {
                    let id = command
                        .entities
                        .filters
                        .get("id")
                        .ok_or("Missing id for update operation")?;
                    let clean_id = Self::clean_record_id(id);
                    let query = format!("UPDATE event_nodes:{clean_id} MERGE $data RETURN *");
                    println!("Executing update query: {query}");
                    let mut result = self
                        .db
                        .query(&query)
                        .bind(("data", node_data.clone()))
                        .await
                        .map_err(|e| {
                            println!("SurrealDB update error: {e}");
                            e
                        })?;
                    let updated: Option<serde_json::Value> = result.take(0)?;
                    Ok(QueryResult {
                        data: serde_json::json!({
                            "status": "updated",
                            "id": id,
                            "data": updated
                        }),
                        metadata: HashMap::from([("operation".to_string(), "update".to_string())]),
                        execution_time_ms: start_time.elapsed().as_millis() as u64,
                    })
                } else {
                    Ok(QueryResult {
                        data: serde_json::json!({
                            "error": "No node data provided for update operation",
                            "status": "failed"
                        }),
                        metadata: HashMap::from([("operation".to_string(), "update".to_string())]),
                        execution_time_ms: start_time.elapsed().as_millis() as u64,
                    })
                }
            }
            Operation::Delete => Ok(QueryResult {
                data: serde_json::json!({
                    "message": "Delete operation not yet implemented",
                    "status": "not_implemented"
                }),
                metadata: HashMap::from([("operation".to_string(), "delete".to_string())]),
                execution_time_ms: start_time.elapsed().as_millis() as u64,
            }),
            Operation::Relate => {
                if let Some(edge_data) = &command.entities.edge_data {
                    match self
                        .create_relation(&DatabaseEntities {
                            node_data: command.entities.node_data.clone(),
                            edge_data: Some(edge_data.clone()),
                            filters: command.entities.filters.clone(),
                        })
                        .await
                    {
                        Ok(result) => Ok(QueryResult {
                            data: result,
                            metadata: HashMap::from([(
                                "operation".to_string(),
                                "relate".to_string(),
                            )]),
                            execution_time_ms: start_time.elapsed().as_millis() as u64,
                        }),
                        Err(e) => Ok(QueryResult {
                            data: serde_json::json!({
                                "error": e.to_string(),
                                "status": "failed"
                            }),
                            metadata: HashMap::from([(
                                "operation".to_string(),
                                "relate".to_string(),
                            )]),
                            execution_time_ms: start_time.elapsed().as_millis() as u64,
                        }),
                    }
                } else {
                    Ok(QueryResult {
                        data: serde_json::json!({
                            "error": "No edge data provided for relate operation",
                            "status": "failed"
                        }),
                        metadata: HashMap::from([("operation".to_string(), "relate".to_string())]),
                        execution_time_ms: start_time.elapsed().as_millis() as u64,
                    })
                }
            }
        }
    }
    async fn create_node(
        &self,
        _table: &str,
        entities: DatabaseEntities,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let _start_time = std::time::Instant::now();
        if let Some(mut node_data) = entities.node_data {
            let record_type = match node_data.get("type").and_then(|t| t.as_str()) {
                Some("segment_metadata") => "content_nodes",
                Some("utterance") => "content_nodes",
                Some("temporal_marker") => "temporal_nodes",
                Some("entity") => "entity_nodes",
                Some("action") => "action_nodes",
                Some("location") => "location_nodes",
                Some("numerical") => "numerical_nodes",
                Some("relationship") => "relationship_nodes",
                _ => "content_nodes",
            };
            let id = format!("{}", uuid::Uuid::new_v4().simple());
            if let Some(obj) = node_data.as_object_mut() {
                obj.remove("id");
                if let Some(_created_at_str) = obj.get("created_at").and_then(|v| v.as_str()) {
                    obj.remove("created_at");
                }
                if record_type == "content_nodes" {
                    let node_type = match obj.get("type").and_then(|t| t.as_str()) {
                        Some("utterance") => "utterance",
                        Some("segment_metadata") => "segment",
                        _ => "content",
                    };
                    obj.insert("node_type".to_string(), serde_json::json!(node_type));
                    if !obj.contains_key("content") {
                        let content = obj
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("No content provided");
                        obj.insert("content".to_string(), serde_json::json!(content));
                    }
                }
                match record_type {
                    "entity_nodes" => {
                        if !obj.contains_key("metadata") || obj.get("metadata").unwrap().is_null() {
                            obj.insert("metadata".to_string(), serde_json::json!({}));
                        }
                        if !obj.contains_key("name") {
                            obj.insert("name".to_string(), serde_json::json!("unnamed"));
                        }
                        if !obj.contains_key("entity_type") {
                            obj.insert("entity_type".to_string(), serde_json::json!("unknown"));
                        }
                    }
                    "location_nodes" => {
                        if !obj.contains_key("coordinates") {
                            obj.insert("coordinates".to_string(), serde_json::json!({}));
                        }
                        if !obj.contains_key("location_type") {
                            obj.insert("location_type".to_string(), serde_json::json!("unknown"));
                        }
                        if !obj.contains_key("name") {
                            obj.insert("name".to_string(), serde_json::json!("unnamed"));
                        }
                    }
                    "temporal_nodes" => {
                        if !obj.contains_key("temporal_data") {
                            obj.insert("temporal_data".to_string(), serde_json::json!({}));
                        }
                    }
                    "action_nodes" => {
                        if !obj.contains_key("verb") {
                            obj.insert("verb".to_string(), serde_json::json!("unknown"));
                        }
                    }
                    "numerical_nodes" => {
                        if !obj.contains_key("value") {
                            obj.insert("value".to_string(), serde_json::json!(0.0));
                        }
                    }
                    "relationship_nodes" => {
                        if !obj.contains_key("source") {
                            obj.insert("source".to_string(), serde_json::json!("unknown"));
                        }
                        if !obj.contains_key("target") {
                            obj.insert("target".to_string(), serde_json::json!("unknown"));
                        }
                        if !obj.contains_key("relation_type") {
                            obj.insert("relation_type".to_string(), serde_json::json!("unknown"));
                        }
                    }
                    _ => {}
                }
            }
            let node_data_clone = node_data.clone();
            let clean_id = Self::clean_record_id(&id);
            if let Some(node_obj) = node_data_clone.as_object() {
                let mut params: Vec<(String, serde_json::Value)> = Vec::new();
                let mut set_clauses = Vec::new();
                for (key, value) in node_obj {
                    set_clauses.push(format!("{key} = ${key}"));
                    params.push((key.clone(), value.clone()));
                }
                set_clauses.push("created_at = time::now()".to_string());
                let query = format!(
                    "CREATE {}:{} SET {} RETURN *",
                    record_type,
                    clean_id,
                    set_clauses.join(", ")
                );
                println!("Executing query: {query}");
                let mut query_builder = self.db.query(&query);
                for (key, value) in params {
                    query_builder = query_builder.bind((key, value));
                }
                let mut result = query_builder.await.map_err(|e| {
                    println!("SurrealDB query error: {e}");
                    e
                })?;
                let created: Option<serde_json::Value> = result.take(0)?;
                Ok(serde_json::json!({
                    "status": "created",
                    "id": id,
                    "data": created
                }))
            } else {
                Err("Node data must be an object".into())
            }
        } else {
            Err("No node data provided".into())
        }
    }
    async fn query_nodes(
        &self,
        graph_type: &GraphType,
        filters: &HashMap<String, String>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let table = match graph_type {
            GraphType::Agent => "agent_nodes",
            GraphType::Event => "event_nodes",
            GraphType::Personalisation => "location_nodes",
            GraphType::Knowledge => "relationship_nodes",
        };
        let mut query = format!("SELECT * FROM {table}");
        if !filters.is_empty() {
            let conditions: Vec<String> = filters
                .iter()
                .map(|(key, value)| format!("{key} = '{value}'"))
                .collect();
            query.push_str(&format!(" WHERE {}", conditions.join(" AND ")));
        }
        query.push_str(" ORDER BY created_at DESC");
        let mut result = self.db.query(&query).await?;
        let nodes: Vec<serde_json::Value> = result.take(0)?;
        Ok(serde_json::json!({
            "nodes": nodes,
            "count": nodes.len()
        }))
    }
    async fn update_node(
        &self,
        entities: DatabaseEntities,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let node_data = entities.node_data.ok_or("Missing node data")?;
        let id = entities
            .filters
            .get("id")
            .ok_or("Missing id for update")?
            .clone();
        let query = format!("UPDATE {id} MERGE $data SET updated_at = time::now() RETURN *");
        let mut result = self.db.query(&query).bind(("data", node_data)).await?;
        let updated: Option<serde_json::Value> = result.take(0)?;
        Ok(serde_json::json!({
            "status": "updated",
            "id": id,
            "data": updated
        }))
    }
    async fn delete_node(
        &self,
        filters: &HashMap<String, String>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let id = filters.get("id").ok_or("Missing id for deletion")?;
        let query = format!("DELETE {id} RETURN BEFORE");
        let mut result = self.db.query(&query).await?;
        let deleted: Option<serde_json::Value> = result.take(0)?;
        Ok(serde_json::json!({
            "status": "deleted",
            "id": id,
            "data": deleted
        }))
    }
    async fn create_relation(
        &self,
        entities: &DatabaseEntities,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let edge_data = entities.edge_data.as_ref().ok_or("Missing edge data")?;
        let from = edge_data
            .get("from")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'from'")?;
        let to = edge_data
            .get("to")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'to'")?;
        let relation_type = edge_data
            .get("relation_type")
            .and_then(|v| v.as_str())
            .unwrap_or("RELATES_TO");
        let clean_from = from.replace("-", "");
        let clean_to = to.replace("-", "");

        let query = format!(
            "RELATE content_nodes:{clean_from}->{relation_type}->content_nodes:{clean_to} RETURN *"
        );
        println!("Executing relation query: {query}");
        let mut result = self.db.query(&query).await.map_err(|e| {
            println!("SurrealDB relation error: {e}");
            e
        })?;
        let relation: Option<serde_json::Value> = result.take(0)?;
        Ok(serde_json::json!({
            "status": "related",
            "from": from,
            "to": to,
            "type": relation_type,
            "data": relation
        }))
    }

    pub async fn manage_graph_node(
        &self,
        operation: &str,
        graph_type: &GraphType,
        filters: &HashMap<String, String>,
        entities: Option<&Vec<crate::nlu::orchestrator::data_models::Entity>>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        match operation {
            "query" => self.query_nodes(graph_type, filters).await,
            "update" => {
                if let Some(entities) = entities {
                    let db_entities = DatabaseEntities {
                        node_data: Some(serde_json::to_value(entities)?),
                        edge_data: None,
                        filters: filters.clone(),
                    };
                    self.update_node(db_entities).await
                } else {
                    Err("Update operation requires entity data".into())
                }
            }
            "delete" => self.delete_node(filters).await,
            _ => Err(format!("Unsupported operation: {operation}").into()),
        }
    }

    pub async fn store_nlu_data(
        &self,
        extracted_data: &crate::nlu::orchestrator::data_models::ExtractedData,
    ) -> Result<QueryResult, Box<dyn std::error::Error>> {
        let start_time = std::time::Instant::now();
        let mut queries = Vec::new();
        let mut created_ids = Vec::new();
        for node in &extracted_data.nodes {
            match node {
                crate::nlu::orchestrator::data_models::KnowledgeNode::Entity(entity) => {
                    let id = format!("entity_{}", Uuid::new_v4().simple());
                    queries.push(format!(
                        "CREATE entity_nodes:{} SET name = '{}', entity_type = '{}', metadata = {}, created_at = time::now()",
                        id, entity.name, entity.entity_type,
                        serde_json::to_string(&entity.metadata)?.replace("\"", "'")
                    ));
                    created_ids.push(id);
                }
                crate::nlu::orchestrator::data_models::KnowledgeNode::Temporal(temporal) => {
                    let id = format!("temporal_{}", Uuid::new_v4().simple());
                    queries.push(format!(
                        "CREATE temporal_nodes:{} SET date_text = '{}', resolved_date = '{}', confidence = {}, metadata = {}, created_at = time::now()",
                        id, temporal.date_text,
                        temporal.resolved_date.as_deref().unwrap_or(""),
                        temporal.confidence,
                        serde_json::to_string(&temporal.metadata)?.replace("\"", "'")
                    ));
                    created_ids.push(id);
                }
                _ => {}
            }
        }
        if queries.is_empty() {
            return Ok(QueryResult {
                data: serde_json::json!({
                    "stored_count": 0,
                    "created_ids": []
                }),
                metadata: HashMap::from([("operation".to_string(), "store_nlu_data".to_string())]),
                execution_time_ms: start_time.elapsed().as_millis() as u64,
            });
        }
        let transaction_query = format!(
            "BEGIN TRANSACTION; {}; COMMIT TRANSACTION;",
            queries.join("; ")
        );
        let mut result = self.db.query(&transaction_query).await?;
        let _: Vec<serde_json::Value> = result.take(0)?;
        let execution_time = start_time.elapsed().as_millis() as u64;
        Ok(QueryResult {
            data: serde_json::json!({
                "stored_count": created_ids.len(),
                "created_ids": created_ids
            }),
            metadata: HashMap::from([("operation".to_string(), "store_nlu_data".to_string())]),
            execution_time_ms: execution_time,
        })
    }
    pub async fn get_database_info(&self) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let mut info = serde_json::Map::new();
        match self.get_stats().await {
            Ok(stats) => {
                info.insert("table_stats".to_string(), serde_json::json!(stats));
            }
            Err(e) => {
                info.insert("stats_error".to_string(), serde_json::json!(e.to_string()));
            }
        }
        match self.health_check().await {
            Ok(healthy) => {
                info.insert("database_healthy".to_string(), serde_json::json!(healthy));
            }
            Err(e) => {
                info.insert(
                    "health_check_error".to_string(),
                    serde_json::json!(e.to_string()),
                );
            }
        }
        info.insert(
            "connection_status".to_string(),
            serde_json::json!("connected"),
        );
        info.insert(
            "timestamp".to_string(),
            serde_json::json!(chrono::Utc::now().to_rfc3339()),
        );
        Ok(serde_json::Value::Object(info))
    }
    pub async fn get_stats(
        &self,
    ) -> Result<HashMap<String, serde_json::Value>, Box<dyn std::error::Error>> {
        let mut stats = HashMap::new();
        let tables = [
            "agent_nodes",
            "event_nodes",
            "location_nodes",
            "relationship_nodes",
            "entity_nodes",
            "temporal_nodes",
        ];
        for table in &tables {
            let query = format!("SELECT count() FROM {table} GROUP ALL");
            match self.db.query(&query).await {
                Ok(mut result) => {
                    let count: Option<i64> = result.take(0).unwrap_or(Some(0));
                    stats.insert(table.to_string(), serde_json::json!(count.unwrap_or(0)));
                }
                Err(_) => {
                    stats.insert(table.to_string(), serde_json::json!(0));
                }
            }
        }
        Ok(stats)
    }
    pub async fn raw_query(
        &self,
        query: &str,
        params: HashMap<String, serde_json::Value>,
    ) -> Result<QueryResult, Box<dyn std::error::Error>> {
        let start_time = std::time::Instant::now();
        let mut db_query = self.db.query(query);
        for (key, value) in params {
            db_query = db_query.bind((key, value));
        }
        let mut result = db_query.await?;
        let data: Vec<serde_json::Value> = result.take(0)?;
        let execution_time = start_time.elapsed().as_millis() as u64;
        Ok(QueryResult {
            data: serde_json::json!({ "results": data, "count": data.len() }),
            metadata: HashMap::from([("operation".to_string(), "raw_query".to_string())]),
            execution_time_ms: execution_time,
        })
    }
    pub async fn health_check(&self) -> Result<bool, Box<dyn std::error::Error>> {
        let mut result = self.db.query("SELECT 1").await?;
        let _: Option<serde_json::Value> = result.take(0)?;
        Ok(true)
    }
}
#[derive(Debug)]
pub struct DatabaseError {
    pub message: String,
}
impl std::fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Database Error: {}", self.message)
    }
}
impl std::error::Error for DatabaseError {}
impl From<surrealdb::Error> for DatabaseError {
    fn from(err: surrealdb::Error) -> Self {
        DatabaseError {
            message: err.to_string(),
        }
    }
}
impl From<serde_json::Error> for DatabaseError {
    fn from(err: serde_json::Error) -> Self {
        DatabaseError {
            message: err.to_string(),
        }
    }
}
impl From<String> for DatabaseError {
    fn from(message: String) -> Self {
        DatabaseError { message }
    }
}
impl From<&str> for DatabaseError {
    fn from(message: &str) -> Self {
        DatabaseError {
            message: message.to_string(),
        }
    }
}
impl std::fmt::Display for GraphType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphType::Agent => write!(f, "Agent"),
            GraphType::Event => write!(f, "Event"),
            GraphType::Personalisation => write!(f, "Personalisation"),
            GraphType::Knowledge => write!(f, "Knowledge"),
        }
    }
}
