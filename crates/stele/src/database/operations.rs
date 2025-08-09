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

use crate::database::types::DatabaseError;
use crate::nlu::orchestrator::data_models::{
    ExtractedData, InputSegment, KnowledgeNode, Relationship, UnifiedNLUData,
};
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use surrealdb::engine::remote::ws::Client;
use surrealdb::RecordId;
use surrealdb::Surreal;
use uuid::Uuid;
pub struct DatabaseOperations {
    connection: Arc<Surreal<Client>>,
}
impl DatabaseOperations {
    pub fn new(connection: Arc<Surreal<Client>>) -> Self {
        Self { connection }
    }
    pub async fn store_unified_nlu_data(
        &self,
        data: &UnifiedNLUData,
        original_input: &str,
    ) -> Result<String, DatabaseError> {
        let utterance_uuid = Uuid::new_v4().to_string();
        let utterance_record_id = format!("utterance:{utterance_uuid}");
        let utterance_query = format!(
            "CREATE {utterance_record_id} CONTENT {{
                original_text: $original_text,
                processing_strategy: $strategy,
                execution_time_ms: $execution_time,
                cost_estimate: $cost_estimate,
                models_used: $models_used,
                confidence_scores: $confidence_scores,
                created_at: time::now(),
                segment_count: $segment_count
            }}"
        );
        self.connection
            .query(&utterance_query)
            .bind(("original_text", original_input.to_string()))
            .bind(("strategy", data.processing_metadata.strategy_used.clone()))
            .bind(("execution_time", data.processing_metadata.execution_time_ms))
            .bind((
                "cost_estimate",
                data.processing_metadata.total_cost_estimate,
            ))
            .bind(("models_used", data.processing_metadata.models_used.clone()))
            .bind((
                "confidence_scores",
                data.processing_metadata.confidence_scores.clone(),
            ))
            .bind(("segment_count", data.segments.len()))
            .await
            .map_err(|e| DatabaseError::Query(format!("Failed to store utterance: {e}")))?;
        self.store_extracted_data(&data.extracted_data, &utterance_record_id)
            .await?;
        self.store_segments(&data.segments, &utterance_record_id)
            .await?;
        println!("✅ Stored unified NLU data with utterance ID: {utterance_record_id}");
        Ok(utterance_record_id)
    }
    async fn store_extracted_data(
        &self,
        data: &ExtractedData,
        utterance_id: &str,
    ) -> Result<(), DatabaseError> {
        let mut node_map: HashMap<String, RecordId> = HashMap::new();
        for node in &data.nodes {
            let temp_id = node.temp_id();
            if temp_id.is_empty() {
                continue;
            }
            let record_id = self.store_node(node, utterance_id).await?;
            node_map.insert(temp_id.to_string(), record_id);
        }
        for relationship in &data.relationships {
            if let (Some(source_id), Some(target_id)) = (
                node_map.get(&relationship.source),
                node_map.get(&relationship.target),
            ) {
                self.store_relationship(relationship, source_id, target_id, utterance_id)
                    .await?;
            }
        }
        Ok(())
    }
    async fn store_node(
        &self,
        node: &KnowledgeNode,
        utterance_id: &str,
    ) -> Result<RecordId, DatabaseError> {
        let node_uuid = Uuid::new_v4().to_string();
        let (node_type_str, properties) = match node {
            KnowledgeNode::Entity(e) => ("Entity", serde_json::json!(e)),
            KnowledgeNode::Temporal(t) => ("Temporal", serde_json::json!(t)),
            KnowledgeNode::Numerical(n) => ("Numerical", serde_json::json!(n)),
            KnowledgeNode::Action(a) => ("Action", serde_json::json!(a)),
        };
        let table_name = "nodes";
        let record_id = RecordId::from((table_name, &node_uuid));
        let query = format!(
            "CREATE {record_id} CONTENT {{
                type: $type,
                properties: $properties,
                created_at: time::now()
            }}"
        );
        self.connection
            .query(&query)
            .bind(("type", node_type_str))
            .bind(("properties", properties))
            .await
            .map_err(|e| DatabaseError::Query(format!("Failed to store node: {e}")))?;
        self.create_extraction_relationship(&record_id.to_string(), utterance_id, "derived_from")
            .await?;
        Ok(record_id)
    }
    async fn store_relationship(
        &self,
        relationship: &Relationship,
        source_id: &RecordId,
        target_id: &RecordId,
        utterance_id: &str,
    ) -> Result<String, DatabaseError> {
        let query = format!(
            "RELATE {source_id}->edges->{target_id} SET label = $label, properties = $properties"
        );
        let properties = serde_json::json!({
            "confidence": relationship.confidence,
            "metadata": relationship.metadata,
            "created_at": Utc::now().to_rfc3339()
        });
        let mut result = self
            .connection
            .query(&query)
            .bind(("label", relationship.relation_type.clone()))
            .bind(("properties", properties))
            .await
            .map_err(|e| DatabaseError::Query(format!("Failed to store relationship: {e}")))?;
        let created_edge: Vec<Value> = result.take(0).unwrap_or_default();
        if let Some(edge) = created_edge.first() {
            if let Some(edge_id_val) = edge.get("id") {
                if let Some(edge_id) = edge_id_val.as_str() {
                    self.create_extraction_relationship(edge_id, utterance_id, "derived_from")
                        .await?;
                    return Ok(edge_id.to_string());
                }
            }
        }
        Err(DatabaseError::Query(
            "Failed to retrieve ID of created edge relationship.".to_string(),
        ))
    }
    async fn store_segments(
        &self,
        segments: &[InputSegment],
        utterance_id: &str,
    ) -> Result<(), DatabaseError> {
        if segments.is_empty() {
            return Ok(());
        }
        let mut transaction_query = "BEGIN TRANSACTION;".to_string();
        for segment in segments.iter() {
            let segment_id = format!("segment:{}", Uuid::new_v4());
            let segment_json = serde_json::to_string(segment).unwrap_or_else(|_| "{}".to_string());
            let create_query = format!("CREATE {segment_id} CONTENT {segment_json};");
            transaction_query.push_str(&create_query);
            let relate_query = format!("RELATE {utterance_id}->HAS_SEGMENT->{segment_id};");
            transaction_query.push_str(&relate_query);
        }
        transaction_query.push_str("COMMIT TRANSACTION;");
        self.connection
            .query(&transaction_query)
            .await
            .map_err(|e| {
                DatabaseError::Query(format!("Failed to commit segments transaction: {e}"))
            })?;
        Ok(())
    }
    async fn create_extraction_relationship(
        &self,
        from_id: &str,
        to_id: &str,
        relationship_type: &str,
    ) -> Result<(), DatabaseError> {
        let query = format!("RELATE {from_id}->{relationship_type}->{to_id}");
        self.connection.query(&query).await.map_err(|e| {
            DatabaseError::Query(format!("Failed to create extraction relationship: {e}"))
        })?;
        Ok(())
    }
    pub async fn execute_transaction(
        &self,
        query: &str,
        params: Option<&Value>,
    ) -> Result<Value, DatabaseError> {
        let mut db_query = self.connection.query(query);
        if let Some(params) = params {
            if let Some(params_obj) = params.as_object() {
                for (key, value) in params_obj {
                    db_query = db_query.bind((key.clone(), value.clone()));
                }
            }
        }
        let mut result = db_query
            .await
            .map_err(|e| DatabaseError::Query(format!("Transaction failed: {e}")))?;
        let data: Vec<Value> = result.take(0).unwrap_or_else(|_| vec![]);
        Ok(serde_json::json!({
            "success": true,
            "data": data,
            "timestamp": Utc::now().to_rfc3339()
        }))
    }
    pub async fn test_connection(&self) -> Result<(), DatabaseError> {
        self.connection
            .query("SELECT * FROM utterance LIMIT 1")
            .await
            .map_err(|e| DatabaseError::Query(format!("Test query failed: {e}")))?;
        Ok(())
    }
    pub async fn execute_query(&self, query: &str) -> Result<Value, DatabaseError> {
        let mut result = self
            .connection
            .query(query)
            .await
            .map_err(|e| DatabaseError::Query(format!("Query failed: {e}")))?;
        let data: Vec<Value> = result.take(0).unwrap_or_else(|_| vec![]);
        Ok(serde_json::json!({
            "success": true,
            "data": data,
            "timestamp": Utc::now().to_rfc3339()
        }))
    }
    pub async fn health_check(&self) -> Result<Value, DatabaseError> {
        let health_queries = vec![
            ("connection_test", "SELECT 1 as test"),
            (
                "entity_nodes_count",
                "SELECT count() FROM entity_nodes GROUP ALL",
            ),
            (
                "action_nodes_count",
                "SELECT count() FROM action_nodes GROUP ALL",
            ),
            (
                "temporal_nodes_count",
                "SELECT count() FROM temporal_nodes GROUP ALL",
            ),
            (
                "numerical_nodes_count",
                "SELECT count() FROM numerical_nodes GROUP ALL",
            ),
            (
                "relationships_count",
                "SELECT count() FROM relationships GROUP ALL",
            ),
            (
                "utterances_count",
                "SELECT count() FROM utterance GROUP ALL",
            ),
        ];
        let mut health_data = serde_json::Map::new();
        let mut all_healthy = true;
        for (name, query) in health_queries.iter() {
            match self.connection.query(*query).await {
                Ok(mut result) => {
                    let data: Vec<Value> = result.take(0).unwrap_or_default();
                    health_data.insert(
                        name.to_string(),
                        serde_json::json!({
                            "status": "healthy",
                            "data": data
                        }),
                    );
                }
                Err(e) => {
                    all_healthy = false;
                    health_data.insert(
                        name.to_string(),
                        serde_json::json!({
                            "status": "error",
                            "error": e.to_string()
                        }),
                    );
                }
            }
        }
        health_data.insert(
            "overall_status".to_string(),
            serde_json::json!(if all_healthy { "healthy" } else { "degraded" }),
        );
        health_data.insert(
            "timestamp".to_string(),
            serde_json::json!(Utc::now().to_rfc3339()),
        );
        Ok(Value::Object(health_data))
    }
    pub async fn get_comprehensive_stats(&self) -> Result<Value, DatabaseError> {
        let mut stats = serde_json::Map::new();
        let node_queries = vec![
            (
                "total_utterances",
                "SELECT count() FROM utterance GROUP ALL",
            ),
            ("total_segments", "SELECT count() FROM segment GROUP ALL"),
            (
                "total_entities",
                "SELECT count() FROM nodes WHERE type = 'Entity' GROUP ALL",
            ),
            (
                "total_actions",
                "SELECT count() FROM nodes WHERE type = 'Action' GROUP ALL",
            ),
            (
                "total_temporal_markers",
                "SELECT count() FROM nodes WHERE type = 'Temporal' GROUP ALL",
            ),
            (
                "total_numerical_values",
                "SELECT count() FROM nodes WHERE type = 'Numerical' GROUP ALL",
            ),
            ("total_relationships", "SELECT count() FROM edges GROUP ALL"),
        ];
        for (stat_name, query) in node_queries {
            match self.connection.query(query).await {
                Ok(mut result) => {
                    let data: Vec<Value> = result.take(0).unwrap_or_default();
                    if let Some(first_result) = data.first() {
                        stats.insert(stat_name.to_string(), first_result.clone());
                    } else {
                        stats.insert(stat_name.to_string(), serde_json::json!(0));
                    }
                }
                Err(e) => {
                    eprintln!("Failed to get {stat_name} stats: {e}");
                    stats.insert(
                        stat_name.to_string(),
                        serde_json::json!({
                            "error": e.to_string(),
                            "count": 0
                        }),
                    );
                }
            }
        }
        match self
            .connection
            .query(
                "SELECT *, time::format(created_at, '%Y-%m-%d %H:%M:%S') as formatted_time
             FROM utterance
             ORDER BY created_at DESC
             LIMIT 10",
            )
            .await
        {
            Ok(mut result) => {
                let recent_data: Vec<Value> = result.take(0).unwrap_or_default();
                stats.insert(
                    "recent_utterances".to_string(),
                    serde_json::json!(recent_data),
                );
            }
            Err(e) => {
                stats.insert(
                    "recent_utterances".to_string(),
                    serde_json::json!({"error": e.to_string()}),
                );
            }
        }
        stats.insert(
            "generated_at".to_string(),
            serde_json::json!(Utc::now().to_rfc3339()),
        );
        Ok(Value::Object(stats))
    }
    pub async fn get_utterance_breakdown(
        &self,
        utterance_id: &str,
    ) -> Result<Value, DatabaseError> {
        let mut breakdown = serde_json::Map::new();
        let utterance_query = format!("SELECT * FROM {utterance_id}");
        let mut result = self
            .connection
            .query(&utterance_query)
            .await
            .map_err(|e| DatabaseError::Query(format!("Failed to get utterance: {e}")))?;
        let utterance_data: Vec<Value> = result.take(0).unwrap_or_default();
        breakdown.insert("utterance".to_string(), serde_json::json!(utterance_data));
        let nodes_query = format!("SELECT * FROM nodes WHERE (<-derived_from<-({utterance_id}))");
        match self.connection.query(&nodes_query).await {
            Ok(mut result) => {
                let data: Vec<Value> = result.take(0).unwrap_or_default();
                breakdown.insert("nodes".to_string(), serde_json::json!(data));
            }
            Err(e) => {
                eprintln!("Failed to get nodes for utterance {utterance_id}: {e}");
                breakdown.insert("nodes".to_string(), serde_json::json!([]));
            }
        }
        let edges_query = format!("SELECT * FROM edges WHERE (<-derived_from<-({utterance_id}))");
        match self.connection.query(&edges_query).await {
            Ok(mut result) => {
                let data: Vec<Value> = result.take(0).unwrap_or_default();
                breakdown.insert("edges".to_string(), serde_json::json!(data));
            }
            Err(e) => {
                eprintln!("Failed to get edges for utterance {utterance_id}: {e}");
                breakdown.insert("edges".to_string(), serde_json::json!([]));
            }
        }
        breakdown.insert(
            "breakdown_generated_at".to_string(),
            serde_json::json!(Utc::now().to_rfc3339()),
        );
        Ok(Value::Object(breakdown))
    }
    pub async fn delete_utterance_cascade(
        &self,
        utterance_id: &str,
    ) -> Result<Value, DatabaseError> {
        let extraction_query = format!(
            "SELECT id FROM (SELECT id FROM nodes WHERE (<-derived_from<-({utterance_id}))) UNION ALL (SELECT id FROM edges WHERE (<-derived_from<-({utterance_id})))");
        let mut result = self
            .connection
            .query(&extraction_query)
            .await
            .map_err(|e| DatabaseError::Query(format!("Failed to query extractions: {e}")))?;
        let extractions: Vec<Value> = result.take(0).unwrap_or_default();
        let extraction_count = extractions.len();
        for extraction in extractions {
            if let Some(id) = extraction.get("id").and_then(|i| i.as_str()) {
                let delete_query = format!("DELETE {id}");
                let _ = self.connection.query(&delete_query).await;
            }
        }
        let delete_utterance_query = format!("DELETE {utterance_id}");
        self.connection
            .query(&delete_utterance_query)
            .await
            .map_err(|e| DatabaseError::Query(format!("Failed to delete utterance: {e}")))?;
        Ok(serde_json::json!({
            "success": true,
            "utterance_id": utterance_id,
            "extractions_deleted": extraction_count,
            "timestamp": Utc::now().to_rfc3339()
        }))
    }
    pub async fn find_similar_utterances(
        &self,
        text: &str,
        limit: usize,
    ) -> Result<Value, DatabaseError> {
        let query = "SELECT *,
                        string::similarity(original_text, $search_text) as similarity_score
                     FROM utterance
                     WHERE string::similarity(original_text, $search_text) > 0.3
                     ORDER BY similarity_score DESC
                     LIMIT $limit";
        let mut result = self
            .connection
            .query(query)
            .bind(("search_text", text.to_string()))
            .bind(("limit", limit))
            .await
            .map_err(|e| DatabaseError::Query(format!("Similarity search failed: {e}")))?;
        let similar_utterances: Vec<Value> = result.take(0).unwrap_or_default();
        Ok(serde_json::json!({
            "search_text": text,
            "similar_utterances": similar_utterances,
            "count": similar_utterances.len(),
            "timestamp": Utc::now().to_rfc3339()
        }))
    }
    pub async fn get_extraction_statistics(&self) -> Result<Value, DatabaseError> {
        let mut stats = serde_json::Map::new();
        match self.connection.query(
            "SELECT properties.entity_type, count() as count, avg(properties.confidence) as avg_confidence
             FROM entity_nodes
             GROUP BY properties.entity_type"
        ).await {
            Ok(mut result) => {
                let entity_stats: Vec<Value> = result.take(0).unwrap_or_default();
                stats.insert("entity_stats".to_string(), serde_json::json!(entity_stats));
            },
            Err(_) => {
                stats.insert("entity_stats".to_string(), serde_json::json!([]));
            }
        }
        match self.connection.query(
            "SELECT properties.verb, count() as count, avg(properties.confidence) as avg_confidence
             FROM action_nodes
             GROUP BY properties.verb"
        ).await {
            Ok(mut result) => {
                let action_stats: Vec<Value> = result.take(0).unwrap_or_default();
                stats.insert("action_stats".to_string(), serde_json::json!(action_stats));
            },
            Err(_) => {
                stats.insert("action_stats".to_string(), serde_json::json!([]));
            }
        }
        match self
            .connection
            .query(
                "SELECT relation_type, count() as count, avg(confidence) as avg_confidence
             FROM relationships
             GROUP BY relation_type",
            )
            .await
        {
            Ok(mut result) => {
                let relationship_stats: Vec<Value> = result.take(0).unwrap_or_default();
                stats.insert(
                    "relationship_stats".to_string(),
                    serde_json::json!(relationship_stats),
                );
            }
            Err(_) => {
                stats.insert("relationship_stats".to_string(), serde_json::json!([]));
            }
        }
        stats.insert(
            "generated_at".to_string(),
            serde_json::json!(Utc::now().to_rfc3339()),
        );
        Ok(Value::Object(stats))
    }
    pub async fn update_utterance_status(
        &self,
        utterance_id: &str,
        status: &str,
    ) -> Result<(), DatabaseError> {
        let query = format!(
            "UPDATE {utterance_id} SET
                processing_status = $status,
                last_updated = time::now()"
        );
        self.connection
            .query(&query)
            .bind(("status", status.to_string()))
            .await
            .map_err(|e| DatabaseError::Query(format!("Failed to update utterance status: {e}")))?;
        Ok(())
    }
    pub async fn get_performance_metrics(&self) -> Result<Value, DatabaseError> {
        let mut metrics = serde_json::Map::new();
        match self
            .connection
            .query(
                "SELECT processing_strategy,
                    avg(execution_time_ms) as avg_execution_time,
                    min(execution_time_ms) as min_execution_time,
                    max(execution_time_ms) as max_execution_time,
                    count() as count
             FROM utterance
             WHERE execution_time_ms IS NOT NONE
             GROUP BY processing_strategy",
            )
            .await
        {
            Ok(mut result) => {
                let performance_data: Vec<Value> = result.take(0).unwrap_or_default();
                metrics.insert(
                    "strategy_performance".to_string(),
                    serde_json::json!(performance_data),
                );
            }
            Err(_) => {
                metrics.insert("strategy_performance".to_string(), serde_json::json!([]));
            }
        }
        match self
            .connection
            .query(
                "SELECT processing_strategy,
                    avg(cost_estimate) as avg_cost,
                    sum(cost_estimate) as total_cost,
                    count() as count
             FROM utterance
             WHERE cost_estimate IS NOT NONE
             GROUP BY processing_strategy",
            )
            .await
        {
            Ok(mut result) => {
                let cost_data: Vec<Value> = result.take(0).unwrap_or_default();
                metrics.insert("cost_analysis".to_string(), serde_json::json!(cost_data));
            }
            Err(_) => {
                metrics.insert("cost_analysis".to_string(), serde_json::json!([]));
            }
        }
        metrics.insert(
            "generated_at".to_string(),
            serde_json::json!(Utc::now().to_rfc3339()),
        );
        Ok(Value::Object(metrics))
    }
    pub async fn cleanup_old_data(&self, retention_days: u32) -> Result<Value, DatabaseError> {
        let cutoff_query =
            format!("DELETE FROM utterance WHERE created_at < (time::now() - {retention_days}d)");
        let mut result = self
            .connection
            .query(&cutoff_query)
            .await
            .map_err(|e| DatabaseError::Query(format!("Cleanup failed: {e}")))?;
        let deleted_count: Vec<Value> = result.take(0).unwrap_or_default();
        Ok(serde_json::json!({
            "cleanup_completed": true,
            "retention_days": retention_days,
            "deleted_utterances": deleted_count.len(),
            "timestamp": Utc::now().to_rfc3339()
        }))
    }
    pub async fn handle_query(
        &self,
        data: Value,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(query_type) = data.get("type").and_then(|v| v.as_str()) {
            if query_type == "transaction" {
                if let Some(transaction_query) = data.get("query").and_then(|v| v.as_str()) {
                    println!("Executing SurrealDB query: {transaction_query}");
                    let result = self.connection.query(transaction_query).await?;
                    match result.check() {
                        Ok(_) => {
                            println!("✅ SurrealDB query executed successfully");
                            return Ok(serde_json::json!({
                                "status": "success",
                                "operation": "create",
                                "timestamp": chrono::Utc::now().to_rfc3339()
                            }));
                        }
                        Err(e) => {
                            println!("❌ SurrealDB query execution failed: {e}");
                            return Err(Box::new(e));
                        }
                    }
                } else {
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "Missing 'query' field for transaction",
                    )));
                }
            }
        }
        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid request: 'type' must be 'transaction'",
        )))
    }
    pub async fn handle_retrieve(
        &self,
        query: String,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let mut result = self.connection.query("SELECT *,
            ->has_token->(token_nodes WHERE confidence > 0.7).* as tokens,
            ->has_embedding->embedding_nodes.vector as embedding,
            ->has_semantic->semantic_nodes.extracted_data as semantic_data,
            vector::similarity(->has_embedding->embedding_nodes.vector, vector::encode($query)) as score
            FROM content_nodes
            WHERE vector::similarity(->has_embedding->embedding_nodes.vector, vector::encode($query)) > 0.7
            ORDER BY score DESC
            LIMIT 5")
            .bind(("query", query))
            .await?;
        let response_data: Vec<Value> = result.take(0)?;
        Ok(serde_json::Value::Array(response_data))
    }
    pub async fn create_sample_data(&self) -> Result<(), DatabaseError> {
        let sample_utterances = vec![
            (
                "Hello, my name is John and I work at Acme Corp",
                "statement",
            ),
            ("What is the weather like today?", "question"),
            ("Create a new project called 'AI Assistant'", "command"),
            (
                "John works with Sarah on the marketing team",
                "relationship",
            ),
        ];
        for (text, utterance_type) in sample_utterances {
            let utterance_id = format!("utterance:{}", Uuid::new_v4());
            let query = format!(
                "CREATE {utterance_id} CONTENT {{
                    original_text: $text,
                    utterance_type: $utterance_type,
                    processing_strategy: 'sample_data',
                    execution_time_ms: math::rand() * 1000,
                    cost_estimate: math::rand() * 0.01,
                    created_at: time::now()
                }}"
            );
            self.connection
                .query(&query)
                .bind(("text", text.to_string()))
                .bind(("utterance_type", utterance_type.to_string()))
                .await
                .map_err(|e| DatabaseError::Query(format!("Failed to create sample data: {e}")))?;
        }
        println!("✅ Sample data created successfully");
        Ok(())
    }
    pub async fn reset_all_data(&self) -> Result<(), DatabaseError> {
        let tables = vec![
            "utterance",
            "segment",
            "nodes",
            "edges",
            "derived_from",
            "source",
        ];
        for table in tables {
            let query = format!("DELETE FROM {table}");
            match self.connection.query(&query).await {
                Ok(_) => println!("✅ Cleared table: {table}"),
                Err(e) => eprintln!("⚠️ Failed to clear table {table}: {e}"),
            }
        }
        Ok(())
    }
}
