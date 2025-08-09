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

use crate::database::query_builder::{
    Condition, Operator, OrderClause, OrderDirection, RelateQuery, SelectQuery,
};
use crate::database::query_validator::ValidatedQuery;
use crate::nlu::orchestrator::data_models::{KnowledgeNode, UnifiedNLUData};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
#[derive(Debug, Clone)]
struct PlainTokenData {
    category: String,
    value: String,
    confidence: f64,
}
#[derive(Debug, Clone)]
struct PlainProcessedData {
    raw_text: String,
    sentiment: f64,
    topics: Vec<String>,
    tokens: Vec<PlainTokenData>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseQueries {
    pub queries: HashMap<String, String>,
    pub transaction: TransactionQueries,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionQueries {
    pub begin: String,
    pub commit: String,
    pub rollback: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceMessages {
    pub messages: Messages,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Messages {
    pub startup: StartupMessages,
    pub success: SuccessMessages,
    pub error: ErrorMessages,
    pub warnings: WarningMessages,
    pub commands: CommandMessages,
    pub usage: UsageMessages,
    pub status: StatusMessages,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupMessages {
    pub title: String,
    pub commands: String,
    pub separator: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessMessages {
    pub text_stored: String,
    pub database_healthy: String,
    pub shutdown_complete: String,
    pub retrieved: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMessages {
    pub store_failed: String,
    pub retrieve_failed: String,
    pub health_check_failed: String,
    pub shutdown_failed: String,
    pub query_failed: String,
    pub negotiation_failed: String,
    pub transaction_failed: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarningMessages {
    pub task_stopped: String,
    pub periodic_health_failed: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMessages {
    pub shutdown_graceful: String,
    pub shutdown_initiated: String,
    pub shutdown_attempting: String,
    pub unknown_command: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageMessages {
    pub store: String,
    pub retrieve: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusMessages {
    pub task_running: String,
    pub task_stopped: String,
    pub task_status: String,
    pub stats: String,
}
#[derive(Debug, thiserror::Error)]
pub enum DataProcessorError {
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Database operation failed: {0}")]
    Database(String),
    #[error("Serialisation error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Channel error: {0}")]
    Channel(String),
    #[error("Processing error: {0}")]
    Processing(String),
    #[error("Query validation error: {0}")]
    QueryValidation(String),
}
#[derive(Debug, Clone)]
pub struct QueryWithParams {
    pub query: String,
    pub params: HashMap<String, Value>,
}
pub struct DatabaseProcessor {
    queries: DatabaseQueries,
    messages: InterfaceMessages,
}
impl DatabaseProcessor {
    pub fn new() -> Result<Self, DataProcessorError> {
        let queries = Self::load_queries()?;
        let messages = Self::load_messages()?;
        Ok(Self { queries, messages })
    }
    fn convert_to_plain_data(
        &self,
        nlu_data: &UnifiedNLUData,
    ) -> Result<PlainProcessedData, DataProcessorError> {
        let raw_text = nlu_data.get_raw_text();
        let topics = nlu_data.processing_metadata.topics.clone();
        let plain_tokens = nlu_data
            .extracted_data
            .entities()
            .filter(|entity| {
                !entity.entity_type.contains("\"")
                    && !entity.name.contains("[")
                    && !entity.name.contains("{")
                    && entity.name.len() > 1
            })
            .map(|entity| PlainTokenData {
                category: entity.entity_type.clone(),
                value: entity.name.clone(),
                confidence: entity.confidence as f64,
            })
            .collect();
        Ok(PlainProcessedData {
            raw_text,
            sentiment: nlu_data.processing_metadata.sentiment_score as f64,
            topics,
            tokens: plain_tokens,
        })
    }
    fn load_queries() -> Result<DatabaseQueries, DataProcessorError> {
        let config_path = Path::new("config/database_queries.yml");
        let content = std::fs::read_to_string(config_path).map_err(|e| {
            DataProcessorError::Config(format!("Failed to read queries config: {e}"))
        })?;
        serde_yaml::from_str(&content)
            .map_err(|e| DataProcessorError::Config(format!("Failed to parse queries config: {e}")))
    }
    fn load_messages() -> Result<InterfaceMessages, DataProcessorError> {
        let config_path = Path::new("config/interface_messages.yml");
        let content = std::fs::read_to_string(config_path).map_err(|e| {
            DataProcessorError::Config(format!("Failed to read messages config: {e}"))
        })?;
        serde_yaml::from_str(&content).map_err(|e| {
            DataProcessorError::Config(format!("Failed to parse messages config: {e}"))
        })
    }
    pub fn get_messages(&self) -> &InterfaceMessages {
        &self.messages
    }
    pub fn create_parameterized_query(
        &self,
        query_name: &str,
        params: HashMap<String, Value>,
    ) -> Result<QueryWithParams, DataProcessorError> {
        let template = self.queries.queries.get(query_name).ok_or_else(|| {
            DataProcessorError::Config(format!("Query template '{query_name}' not found"))
        })?;
        Ok(QueryWithParams {
            query: template.clone(),
            params,
        })
    }
    pub fn build_store_transaction(
        &self,
        nlu_data: &UnifiedNLUData,
    ) -> Result<QueryWithParams, DataProcessorError> {
        let plain_data = self.convert_to_plain_data(nlu_data)?;
        let mut transaction_parts = Vec::new();
        let mut all_params = HashMap::new();
        transaction_parts.push(self.queries.transaction.begin.clone());
        let content_params = HashMap::from([
            (
                "raw_text".to_string(),
                Value::String(plain_data.raw_text.clone()),
            ),
            (
                "sentiment".to_string(),
                Value::Number(
                    serde_json::Number::from_f64(plain_data.sentiment)
                        .unwrap_or(serde_json::Number::from(0)),
                ),
            ),
            (
                "topics".to_string(),
                Value::Array(plain_data.topics.into_iter().map(Value::String).collect()),
            ),
        ]);
        let content_query =
            self.create_parameterized_query("create_content_node", content_params.clone())?;
        transaction_parts.push(content_query.query);
        for (key, value) in content_query.params {
            all_params.insert(format!("content_{key}"), value);
        }
        for (idx, token_data) in plain_data.tokens.iter().enumerate() {
            let token_params = HashMap::from([
                (
                    "category".to_string(),
                    Value::String(token_data.category.clone()),
                ),
                ("value".to_string(), Value::String(token_data.value.clone())),
                (
                    "confidence".to_string(),
                    Value::Number(
                        serde_json::Number::from_f64(token_data.confidence)
                            .unwrap_or(serde_json::Number::from(0)),
                    ),
                ),
            ]);
            let token_query =
                self.create_parameterized_query("create_token_node", token_params.clone())?;
            transaction_parts.push(token_query.query);
            for (key, value) in token_query.params {
                all_params.insert(format!("token_{idx}_{key}"), value);
            }
            let relate_params = HashMap::from([
                (
                    "confidence".to_string(),
                    Value::Number(
                        serde_json::Number::from_f64(token_data.confidence)
                            .unwrap_or(serde_json::Number::from(0)),
                    ),
                ),
                (
                    "category".to_string(),
                    Value::String(token_data.category.clone()),
                ),
            ]);
            let relate_query =
                self.create_parameterized_query("relate_content_token", relate_params.clone())?;
            transaction_parts.push(relate_query.query);
            for (key, value) in relate_query.params {
                all_params.insert(format!("relate_{idx}_{key}"), value);
            }
        }
        let embedding_params = HashMap::from([(
            "text".to_string(),
            Value::String(plain_data.raw_text.clone()),
        )]);
        let embedding_query =
            self.create_parameterized_query("create_embedding_node", embedding_params.clone())?;
        transaction_parts.push(embedding_query.query);
        for (key, value) in embedding_query.params {
            all_params.insert(format!("embedding_{key}"), value);
        }
        let relate_embedding_query =
            self.create_parameterized_query("relate_content_embedding", HashMap::new())?;
        transaction_parts.push(relate_embedding_query.query);
        transaction_parts.push(self.queries.transaction.commit.clone());
        Ok(QueryWithParams {
            query: transaction_parts.join(";\n"),
            params: all_params,
        })
    }
    fn clean_nlu_data(&self, nlu_data: &UnifiedNLUData) -> UnifiedNLUData {
        let mut cleaned_data = nlu_data.clone();
        cleaned_data
            .segments
            .retain(|segment| !segment.text.trim().is_empty());
        cleaned_data.extracted_data.nodes.retain(|node| match node {
            KnowledgeNode::Entity(entity) => {
                !entity.name.trim().is_empty()
                    && !entity.entity_type.contains("\"")
                    && entity.name.len() > 1
            }
            _ => true,
        });
        cleaned_data.processing_metadata.sentiment_score = cleaned_data
            .processing_metadata
            .sentiment_score
            .clamp(-1.0, 1.0);
        cleaned_data.processing_metadata.topics.retain(|topic| {
            topic.len() > 2
                && !topic.contains("Here are")
                && !topic.contains("*")
                && !topic.contains("-")
        });
        cleaned_data
    }
    pub fn build_query_from_validated(
        &self,
        validated_query: ValidatedQuery,
        context: Option<&UnifiedNLUData>,
    ) -> Result<QueryWithParams, DataProcessorError> {
        match validated_query {
            ValidatedQuery::Select(select_query) => self.build_select_query(*select_query, context),
            ValidatedQuery::Relate(relate_query) => self.build_relate_query(*relate_query, context),
        }
    }
    pub fn build_select_query(
        &self,
        query: SelectQuery,
        context: Option<&UnifiedNLUData>,
    ) -> Result<QueryWithParams, DataProcessorError> {
        let mut params = HashMap::new();
        let query_string = query.to_string();
        if let Some(ctx) = context {
            if !ctx.processing_metadata.topics.is_empty() {
                params.insert(
                    "context_topics".to_string(),
                    json!(ctx.processing_metadata.topics),
                );
            }
            if ctx.processing_metadata.sentiment_score.abs() > 0.1 {
                params.insert(
                    "context_sentiment".to_string(),
                    json!(ctx.processing_metadata.sentiment_score),
                );
            }
            if ctx.extracted_data.entities().count() > 0 {
                let entity_names: Vec<String> = ctx
                    .extracted_data
                    .entities()
                    .map(|e| e.name.clone())
                    .collect();
                params.insert("context_entities".to_string(), json!(entity_names));
            }
        }
        Ok(QueryWithParams {
            query: query_string,
            params,
        })
    }
    pub fn build_relate_query(
        &self,
        query: RelateQuery,
        context: Option<&UnifiedNLUData>,
    ) -> Result<QueryWithParams, DataProcessorError> {
        let mut params = HashMap::new();
        if let Some(ctx) = context {
            params.insert(
                "context_sentiment".to_string(),
                json!(ctx.processing_metadata.sentiment_score),
            );
            params.insert(
                "context_topics".to_string(),
                json!(ctx.processing_metadata.topics),
            );
        }
        let query_string = query.to_string();
        if let Some((_key, value)) = query.set_fields.iter().next() {
            params.insert("relation_data".to_string(), value.clone());
        }
        Ok(QueryWithParams {
            query: query_string,
            params,
        })
    }
    pub fn process_and_store(
        &self,
        nlu_data: &UnifiedNLUData,
    ) -> Result<Value, DataProcessorError> {
        let cleaned_data = self.clean_nlu_data(nlu_data);
        let transaction = self.build_store_transaction(&cleaned_data)?;
        Ok(json!({
            "query": transaction.query,
            "params": transaction.params,
            "data_summary": {
                "segments_count": cleaned_data.segments.len(),
                "entities_count": cleaned_data.extracted_data.entities().count(),
                "topics_count": cleaned_data.processing_metadata.topics.len(),
                "sentiment_score": cleaned_data.processing_metadata.sentiment_score,
                "execution_time_ms": cleaned_data.processing_metadata.execution_time_ms
            }
        }))
    }
    pub fn create_retrieval_query(
        &self,
        context: &UnifiedNLUData,
    ) -> Result<QueryWithParams, DataProcessorError> {
        let mut select_query = SelectQuery::new()
            .from(vec!["content".to_string()])
            .fields(vec!["*".to_string()]);
        if !context.processing_metadata.topics.is_empty() {
            let condition = Condition::simple(
                "topics",
                Operator::ContainsAny,
                json!(context.processing_metadata.topics),
            );
            select_query = select_query.where_condition(condition);
        }
        if context.processing_metadata.sentiment_score.abs() > 0.1 {
            let sentiment_condition = if context.processing_metadata.sentiment_score > 0.0 {
                Condition::simple("sentiment", Operator::GreaterThan, json!(0.0))
            } else {
                Condition::simple("sentiment", Operator::LessThan, json!(0.0))
            };
            select_query = select_query.where_condition(sentiment_condition);
        }
        self.build_select_query(select_query, Some(context))
    }
    pub fn get_processing_stats(&self, nlu_data: &UnifiedNLUData) -> HashMap<String, Value> {
        let mut stats = HashMap::new();
        stats.insert("total_segments".to_string(), json!(nlu_data.segments.len()));
        stats.insert(
            "total_entities".to_string(),
            json!(nlu_data.extracted_data.entities().count()),
        );
        stats.insert(
            "total_actions".to_string(),
            json!(nlu_data.extracted_data.actions().count()),
        );
        stats.insert(
            "total_relationships".to_string(),
            json!(nlu_data.extracted_data.relationships.len()),
        );
        stats.insert(
            "sentiment_score".to_string(),
            json!(nlu_data.processing_metadata.sentiment_score),
        );
        stats.insert(
            "topic_count".to_string(),
            json!(nlu_data.processing_metadata.topics.len()),
        );
        stats.insert(
            "processing_time_ms".to_string(),
            json!(nlu_data.processing_metadata.execution_time_ms),
        );
        stats.insert(
            "models_used".to_string(),
            json!(nlu_data.processing_metadata.models_used),
        );
        stats.insert(
            "strategy_used".to_string(),
            json!(nlu_data.processing_metadata.strategy_used),
        );
        stats
    }
    pub fn validate_nlu_data(&self, nlu_data: &UnifiedNLUData) -> Result<(), DataProcessorError> {
        nlu_data.validate().map_err(|e| {
            DataProcessorError::Processing(format!("NLU data validation failed: {e}"))
        })?;
        if nlu_data.segments.is_empty() {
            return Err(DataProcessorError::Processing(
                "No segments to process".to_string(),
            ));
        }
        if nlu_data.processing_metadata.models_used.is_empty() {
            return Err(DataProcessorError::Processing(
                "No models were used in processing".to_string(),
            ));
        }
        Ok(())
    }
    pub fn create_search_query(
        &self,
        keywords: Vec<String>,
        filters: HashMap<String, Value>,
    ) -> Result<QueryWithParams, DataProcessorError> {
        let mut select_query = SelectQuery::new()
            .from(vec!["content".to_string()])
            .fields(vec!["*".to_string()])
            .limit(50);
        if !keywords.is_empty() {
            let keyword_condition =
                Condition::simple("text", Operator::Contains, json!(keywords.join(" ")));
            select_query = select_query.where_condition(keyword_condition);
        }
        for (field, value) in filters {
            let condition = Condition::simple(&field, Operator::Equals, value);
            select_query = select_query.where_condition(condition);
        }
        let order_clause = OrderClause::new("relevance".to_string(), OrderDirection::Desc);
        select_query = select_query.order_by(vec![order_clause]);
        self.build_select_query(select_query, None)
    }
    pub fn create_analytics_query(
        &self,
        metric: &str,
    ) -> Result<QueryWithParams, DataProcessorError> {
        let query_string = match metric {
            "sentiment_distribution" => {
                "SELECT sentiment, count() AS count FROM content GROUP BY sentiment ORDER BY sentiment"
            },
            "topic_frequency" => {
                "SELECT topics, count() AS frequency FROM content GROUP BY topics ORDER BY frequency DESC LIMIT 20"
            },
            "processing_stats" => {
                "SELECT AVG(processing_time_ms) AS avg_processing_time, COUNT() AS total_processed FROM content"
            },
            _ => return Err(DataProcessorError::Processing(format!("Unknown analytics metric: {metric}")))
        };
        Ok(QueryWithParams {
            query: query_string.to_string(),
            params: HashMap::new(),
        })
    }
    pub fn get_health_metrics(&self) -> HashMap<String, Value> {
        let mut metrics = HashMap::new();
        metrics.insert("processor_status".to_string(), json!("healthy"));
        metrics.insert(
            "queries_loaded".to_string(),
            json!(self.queries.queries.len()),
        );
        metrics.insert("messages_loaded".to_string(), json!("complete"));
        metrics.insert(
            "last_check".to_string(),
            json!(chrono::Utc::now().to_rfc3339()),
        );
        metrics
    }
    pub fn batch_process(
        &self,
        nlu_data_batch: Vec<&UnifiedNLUData>,
    ) -> Result<Vec<QueryWithParams>, DataProcessorError> {
        let mut batch_queries = Vec::new();
        for nlu_data in nlu_data_batch {
            self.validate_nlu_data(nlu_data)?;
            let cleaned_data = self.clean_nlu_data(nlu_data);
            let transaction = self.build_store_transaction(&cleaned_data)?;
            batch_queries.push(transaction);
        }
        Ok(batch_queries)
    }
    pub fn create_backup_query(&self) -> Result<QueryWithParams, DataProcessorError> {
        let query_string = "SELECT * FROM content, tokens, embeddings";
        Ok(QueryWithParams {
            query: query_string.to_string(),
            params: HashMap::new(),
        })
    }
    pub fn create_cleanup_query(
        &self,
        days_old: u32,
    ) -> Result<QueryWithParams, DataProcessorError> {
        let cutoff_date = chrono::Utc::now() - chrono::Duration::days(days_old as i64);
        let mut params = HashMap::new();
        params.insert("cutoff_date".to_string(), json!(cutoff_date.to_rfc3339()));
        let query_string = "DELETE FROM content WHERE created_at < $cutoff_date";
        Ok(QueryWithParams {
            query: query_string.to_string(),
            params,
        })
    }
}
impl Default for DatabaseProcessor {
    fn default() -> Self {
        Self::new().expect("Failed to create default DatabaseProcessor")
    }
}
pub trait DatabaseProcessable {
    fn to_database_format(&self) -> Result<Value, DataProcessorError>;
    fn get_search_keywords(&self) -> Vec<String>;
    fn get_storage_priority(&self) -> u8;
}
impl DatabaseProcessable for UnifiedNLUData {
    fn to_database_format(&self) -> Result<Value, DataProcessorError> {
        let processor = DatabaseProcessor::new()?;
        processor.process_and_store(self)
    }
    fn get_search_keywords(&self) -> Vec<String> {
        let mut keywords = Vec::new();
        keywords.extend(self.processing_metadata.topics.clone());
        for entity in self.extracted_data.entities() {
            keywords.push(entity.name.clone());
        }
        for action in self.extracted_data.actions() {
            keywords.push(action.verb.clone());
        }
        keywords.sort();
        keywords.dedup();
        keywords.retain(|k| !k.trim().is_empty());
        keywords
    }
    fn get_storage_priority(&self) -> u8 {
        let entity_score = (self.extracted_data.entities().count() as f32 * 0.3) as u8;
        let relationship_score = (self.extracted_data.relationships.len() as f32 * 0.5) as u8;
        let topic_score = (self.processing_metadata.topics.len() as f32 * 0.2) as u8;
        (entity_score + relationship_score + topic_score).min(100)
    }
}
