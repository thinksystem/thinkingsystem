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

use crate::database::intent_analyser::{IntelligentIntentAnalyser, IntentError};
use crate::database::knowledge_adapter::{AdapterError, KnowledgeNodeAdapter};
use crate::database::query_builder::SelectQuery;
use crate::database::query_generator::AdvancedQueryGenerator;
use crate::database::query_kg::{QueryKgBuilder, QueryKnowledgeGraph};
use crate::database::query_validator::{QueryNegotiator, QueryRules};
use crate::database::schema_analyser::{GraphSchema, GraphSchemaAnalyser, SchemaAnalyserError};
use crate::database::surreal_token::SurrealTokenParser;
use crate::nlu::llm_processor::LLMAdapter;
use crate::nlu::orchestrator::data_models::{AdvancedQueryIntent, KnowledgeNode, QueryComplexity};
use serde_json::{json, Value};
use std::sync::Arc;
use surrealdb::{engine::remote::ws::Client, RecordId, Surreal};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

#[derive(serde::Deserialize, Debug)]
struct SurrealDbRecord {
    id: RecordId,
    #[serde(rename = "type")]
    node_type: String,
    properties: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum DdalError {
    #[error("Database query execution failed: {0}")]
    Database(#[from] surrealdb::Error),
    #[error("Schema analysis failed: {0}")]
    Schema(#[from] SchemaAnalyserError),
    #[error("Intent analysis configuration failed: {0}")]
    IntentConfig(String),
    #[error("LLM query planning failed: {0}")]
    Planning(String),
    #[error("Failed to build a valid query from the plan or intent: {0}")]
    QueryBuilding(String),
    #[error("Hydration of database records failed: {0}")]
    Hydration(#[from] AdapterError),
    #[error("Knowledge graph initialisation failed: {0}")]
    KnowledgeGraph(String),
    #[error("Query complexity type is not yet supported: {0:?}")]
    UnsupportedComplexity(QueryComplexity),
}

impl From<IntentError> for DdalError {
    fn from(e: IntentError) -> Self {
        match e {
            IntentError::ConfigError(s) => DdalError::IntentConfig(s),
            IntentError::LLMError(s) => DdalError::Planning(s),
            IntentError::ResponseParseError(e) => {
                DdalError::Planning(format!("Failed to parse LLM response: {e}"))
            }
            _ => DdalError::Planning(e.to_string()),
        }
    }
}

pub struct DynamicDataAccessLayer {
    db_client: Arc<Surreal<Client>>,
    llm_adapter: Arc<dyn LLMAdapter + Send + Sync>,
    intent_analyser: Arc<IntelligentIntentAnalyser>,
    query_generator: Arc<AdvancedQueryGenerator>,
    query_kg: Arc<QueryKnowledgeGraph>,
    graph_schema: Arc<RwLock<GraphSchema>>,
}

impl DynamicDataAccessLayer {
    pub async fn new(
        db_client: Arc<Surreal<Client>>,
        llm_adapter: Arc<dyn LLMAdapter + Send + Sync>,
    ) -> Result<Self, DdalError> {
        let docs_path = if std::path::Path::new("crates/stele/src/database/instructions").exists() {
            "crates/stele/src/database/instructions"
        } else if std::path::Path::new("../../../crates/stele/src/database/instructions").exists() {
            "../../../crates/stele/src/database/instructions"
        } else {
            return Err(DdalError::KnowledgeGraph("Database instructions not found. Please run from the workspace root or demo directory.".into()));
        };

        let patterns_path = if std::path::Path::new(
            "crates/stele/src/database/config/patterns.yaml",
        )
        .exists()
        {
            "crates/stele/src/database/config/patterns.yaml"
        } else if std::path::Path::new("../../../crates/stele/src/database/config/patterns.yaml")
            .exists()
        {
            "../../../crates/stele/src/database/config/patterns.yaml"
        } else {
            return Err(DdalError::KnowledgeGraph("Database patterns config not found. Please run from the workspace root or demo directory.".into()));
        };

        let domains_path = if std::path::Path::new("crates/stele/src/database/config/domains.yaml")
            .exists()
        {
            "crates/stele/src/database/config/domains.yaml"
        } else if std::path::Path::new("../../../crates/stele/src/database/config/domains.yaml")
            .exists()
        {
            "../../../crates/stele/src/database/config/domains.yaml"
        } else {
            return Err(DdalError::KnowledgeGraph("Database domains config not found. Please run from the workspace root or demo directory.".into()));
        };

        info!("Initialising Dynamic Data Access Layer pipeline...");
        info!("Building Query Knowledge Graph from documentation...");
        let query_kg = Arc::new(
            QueryKgBuilder::new(docs_path)
                .build()
                .map_err(|e| DdalError::KnowledgeGraph(e.to_string()))?,
        );
        info!(
            "Query Knowledge Graph built with {} nodes",
            query_kg.graph.node_count()
        );

        let initial_schema =
            GraphSchemaAnalyser::analyse(&db_client, patterns_path, domains_path).await?;
        let graph_schema = Arc::new(RwLock::new(initial_schema));
        info!("Database schema analysis complete.");

        let intent_analyser = Arc::new(IntelligentIntentAnalyser::new(
            graph_schema.clone(),
            query_kg.clone(),
        )?);
        let query_generator = Arc::new(AdvancedQueryGenerator::new(graph_schema.clone()));
        info!("Intent analyser and query generator initialised with knowledge graph support.");

        Ok(Self {
            db_client,
            llm_adapter,
            intent_analyser,
            query_generator,
            graph_schema,
            query_kg,
        })
    }

    pub async fn refresh_schema(&self) -> Result<(), DdalError> {
        info!("Refreshing database schema...");
        let new_schema = GraphSchemaAnalyser::analyse(
            &self.db_client,
            "crates/stele/src/database/config/patterns.yaml",
            "crates/stele/src/database/config/domains.yaml",
        )
        .await?;
        let mut schema_lock = self.graph_schema.write().await;
        *schema_lock = new_schema;
        info!("Schema refresh complete.");
        Ok(())
    }

    pub async fn query_natural_language(
        &self,
        text: &str,
    ) -> Result<Vec<KnowledgeNode>, DdalError> {
        info!("Performing lightweight intent analysis...");
        let intent = self.intent_analyser.analyse_intent(text).await?;
        debug!("Analysed intent: {:?}", intent);

        let select_query = match intent.complexity {
            QueryComplexity::SimpleLookup => {
                info!("Routing to direct query generation for simple lookup.");
                self.query_generator
                    .build_query_from_intent(&intent)
                    .await
                    .map_err(DdalError::QueryBuilding)?
            }
            QueryComplexity::ComplexGraph | QueryComplexity::Unknown => {
                info!("Routing to LLM-assisted planning for complex query.");
                self.plan_and_build_with_llm(&intent).await?
            }
            QueryComplexity::Federated => {
                warn!("Received a query with 'Federated' complexity, which is not yet supported.");
                return Err(DdalError::UnsupportedComplexity(intent.complexity));
            }
        };

        info!("Constructed SurrealQL query: {}", select_query.to_string());
        self.execute_and_hydrate(select_query).await
    }

    async fn plan_and_build_with_llm(
        &self,
        intent: &AdvancedQueryIntent,
    ) -> Result<SelectQuery, DdalError> {
        
        if let Some(select) = self.try_kg_guided_idiom(&intent.original_query).await {
            info!("Built query via KG-guided idiom path.");
            return Ok(select);
        }

        
        let prompt = self
            .intent_analyser
            .build_prompt_for_query(&intent.original_query)
            .await?;
        debug!("Constructed enhanced prompt for LLM intent analysis.");
        info!("Sending request to LLM for query planning...");

        let llm_response_text = self
            .llm_adapter
            .process_text(&prompt)
            .await
            .map_err(|e| DdalError::Planning(e.to_string()))?;
        info!("Received LLM response for query plan.");
        debug!("LLM Raw Response: {}", llm_response_text);

        let plan = self
            .intent_analyser
            .parse_response_to_plan(&llm_response_text)?;
        debug!("Parsed LLM response into plan with {} steps.", plan.len());

        let select_query = self
            .query_generator
            .build_query_from_plan(&plan, Some(intent))
            .await
            .map_err(DdalError::QueryBuilding)?;

        Ok(select_query)
    }

    async fn try_kg_guided_idiom(&self, text: &str) -> Option<SelectQuery> {
        
        let candidates = self.query_kg.suggest_clauses_for_text(text, 3);
        if candidates.is_empty() {
            return None;
        }
    
    let rules = QueryRules {
            allowed_tables: Vec::new(),
            max_conditions: 100,
            allowed_operators: Vec::new(),
            field_types: std::collections::HashMap::new(),
            relationships: Vec::new(),
        };
    let negotiator = QueryNegotiator::new(rules).with_kg_hints(&self.query_kg);
        let mut parser = SurrealTokenParser::with_negotiator(negotiator);
        for _cand in candidates {
            
            
            let maybe_idiom = text.to_string();
            if let Ok(idiom) = parser.parse_with_validation(&maybe_idiom) {
                let select = SurrealTokenParser::convert_idiom_to_select_query(&idiom);
                return Some(select);
            }
        }
        None
    }

    async fn execute_and_hydrate(
        &self,
        query: SelectQuery,
    ) -> Result<Vec<KnowledgeNode>, DdalError> {
        let sql = query.to_string();
        debug!("Executing SurrealQL query: {}", sql);
        let mut response = self.db_client.query(&sql).await?;

        let surreal_records: Vec<SurrealDbRecord> = response.take(0).map_err(|e| {
            error!("Failed to deserialise SurrealDB records: {}", e);
            DdalError::Database(e)
        })?;

        let mut hydrated_nodes = Vec::new();
        let total_records = surreal_records.len();

        for record in surreal_records {
            let record_value = json!({
                "id": record.id.to_string(),
                "type": record.node_type,
                "properties": record.properties
            });

            match KnowledgeNodeAdapter::from_database_record(record_value.clone()) {
                Ok(node) => hydrated_nodes.push(node),
                Err(e) => {
                    warn!(error = %e, record = %record_value, "Skipping record due to hydration failure.");
                }
            }
        }
        info!(
            "Successfully hydrated {} out of {} records",
            hydrated_nodes.len(),
            total_records
        );
        Ok(hydrated_nodes)
    }

    pub async fn get_node_by_id(&self, id: &RecordId) -> Result<Option<KnowledgeNode>, DdalError> {
        let result: Option<SurrealDbRecord> = self.db_client.select(id).await?;
        if let Some(record) = result {
            let record_value = json!({
                "id": record.id,
                "type": record.node_type,
                "properties": record.properties
            });
            match KnowledgeNodeAdapter::from_database_record(record_value) {
                Ok(node) => Ok(Some(node)),
                Err(e) => {
                    error!(error = %e, record_id = %id, "Failed to hydrate a specifically requested node.");
                    Err(DdalError::Hydration(e))
                }
            }
        } else {
            Ok(None)
        }
    }

    pub async fn get_existing_node_types(&self) -> Result<Vec<String>, DdalError> {
        let mut response = self
            .db_client
            .query("SELECT DISTINCT type FROM nodes")
            .await?;
        let types: Vec<Value> = response.take(0)?;
        let type_strings: Vec<String> = types
            .into_iter()
            .filter_map(|v| {
                v.get("type")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        debug!("Found existing node types: {:?}", type_strings);
        Ok(type_strings)
    }

    pub async fn get_database_stats(&self) -> Result<Value, DdalError> {
        let mut stats = serde_json::Map::new();
        let mut response = self
            .db_client
            .query("SELECT count() FROM nodes GROUP ALL")
            .await?;
        let total_count: Vec<Value> = response.take(0)?;
        stats.insert(
            "total_nodes".to_string(),
            json!(total_count
                .first()
                .and_then(|v| v.get("count"))
                .and_then(|c| c.as_u64())
                .unwrap_or(0)),
        );

        let mut type_response = self
            .db_client
            .query("SELECT type, count() FROM nodes GROUP BY type")
            .await?;
        let type_counts: Vec<Value> = type_response.take(0)?;
        let mut types_map = serde_json::Map::new();
        for type_count in type_counts {
            if let (Some(type_name), Some(count)) = (
                type_count.get("type").and_then(|t| t.as_str()),
                type_count.get("count").and_then(|c| c.as_u64()),
            ) {
                types_map.insert(type_name.to_string(), json!(count));
            }
        }
        stats.insert("types".to_string(), json!(types_map));

        let mut rel_response = self
            .db_client
            .query("SELECT count() FROM edges GROUP ALL")
            .await?;
        let rel_count: Vec<Value> = rel_response.take(0)?;
        stats.insert(
            "total_relationships".to_string(),
            json!(rel_count
                .first()
                .and_then(|v| v.get("count"))
                .and_then(|c| c.as_u64())
                .unwrap_or(0)),
        );

        Ok(json!(stats))
    }

    pub async fn enhanced_query(&self, query: &str) -> Result<Value, DdalError> {
        let query_lower = query.to_lowercase();
        if query_lower.contains("stats") || query_lower.contains("statistics") {
            let stats = self.get_database_stats().await?;
            return Ok(json!({
                "query_type": "statistics",
                "data": stats
            }));
        }
        if query_lower.contains("types")
            && (query_lower.contains("show") || query_lower.contains("list"))
        {
            let types = self.get_existing_node_types().await?;
            return Ok(json!({
                "query_type": "types_list",
                "data": types
            }));
        }
        let nodes = self.query_natural_language(query).await?;
        Ok(json!({
            "query_type": "natural_language",
            "count": nodes.len(),
            "data": nodes
        }))
    }

    pub async fn execute_raw_query(&self, sql: &str) -> Result<Vec<Value>, DdalError> {
        info!("Executing raw query: {}", sql);
        let mut response = self.db_client.query(sql).await?;
        let results: Vec<Value> = response.take(0)?;
        Ok(results)
    }

    pub async fn get_suggested_queries(&self) -> Result<Vec<String>, DdalError> {
        let mut suggestions = vec![
            "show everything".to_string(),
            "database statistics".to_string(),
            "list all types".to_string(),
        ];
        let schema_guard = self.graph_schema.read().await;
        if let Some(nodes_table) = schema_guard.tables.get("nodes") {
            if let Some(type_counts) = nodes_table.field_value_counts.get("type") {
                for type_name in type_counts.keys() {
                    suggestions.push(format!("find all {type_name} entities"));
                    suggestions.push(format!("show me 10 {type_name} nodes"));
                }
            }
        }
        let mut rel_types: Vec<_> = schema_guard.relationships.keys().collect();
        rel_types.sort();
        if let Some(rel_type) = rel_types.first() {
            suggestions.push(format!("find nodes with the '{rel_type}' relationship"));
        }
        let stats = self.get_database_stats().await?;
        if let Some(total) = stats.get("total_nodes").and_then(|v| v.as_u64()) {
            if total > 100 {
                suggestions.push("find the newest entities".to_string());
                suggestions.push("show the top 5 most reliable concepts".to_string());
            }
        }
        Ok(suggestions)
    }
}
