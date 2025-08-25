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

use serde::Deserialize;
use std::collections::{HashMap, HashSet};

use crate::database::sanitize::sanitize_table_name;
use surrealdb::{engine::remote::ws::Client, Surreal};
use thiserror::Error;
use tokio::fs;
use tracing::{debug, info};
#[derive(Error, Debug)]
pub enum SchemaAnalyserError {
    #[error("Database query failed: {0}")]
    DbQuery(#[from] surrealdb::Error),
    #[error("I/O error reading configuration file: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML parsing error for configuration file: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("Schema introspection failed: {0}")]
    Introspection(String),
}
#[derive(Debug, Clone, Deserialize)]
pub struct PatternsConfig {
    pub patterns: Vec<GraphPattern>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct DomainsConfig {
    pub domains: Vec<SemanticDomain>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct GraphSchema {
    pub tables: HashMap<String, TableSchema>,
    pub relationships: HashMap<String, RelationshipType>,
    pub graph_patterns: Vec<GraphPattern>,
    pub semantic_domains: Vec<SemanticDomain>,
    #[serde(default)]
    pub node_search_patterns: HashMap<String, Vec<String>>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct TableSchema {
    pub table_type: TableType,
    pub is_schemaless: bool,
    pub fields: HashMap<String, FieldInfo>,
    pub field_value_counts: HashMap<String, HashMap<String, u64>>,
    pub indexes: Vec<IndexInfo>,
    pub discovered_properties: HashMap<String, HashSet<String>>,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub enum TableType {
    Normal,
    Relation,
    Unknown,
}
#[derive(Debug, Clone, Deserialize)]
pub struct FieldInfo {
    pub field_type: String,
    pub is_optional: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct IndexInfo {
    pub name: String,
    pub fields: String,
    pub unique: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct RelationshipType {
    pub semantic_category: String,
    pub directionality: Directionality,
    pub avg_confidence: f64,
    pub count: u64,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub enum Directionality {
    Unidirectional,
    Bidirectional,
    Hierarchical,
}
#[derive(Debug, Clone, Deserialize)]
pub struct GraphPattern {}
#[derive(Debug, Clone, Deserialize)]
pub struct SemanticDomain {}
pub struct GraphSchemaAnalyser;
impl GraphSchemaAnalyser {
    pub async fn analyse(
        db: &Surreal<Client>,
        patterns_path: &str,
        domains_path: &str,
    ) -> Result<GraphSchema, SchemaAnalyserError> {
        let mut schema = Self::load_configurations(patterns_path, domains_path).await?;
        Self::introspect_database_natively(db, &mut schema).await?;
        Self::discover_schemaless_patterns(db, &mut schema).await?;
        Self::analyse_relationship_patterns(db, &mut schema).await?;
        Self::define_search_patterns(&mut schema);
        info!(
            "Analysed graph schema: {} tables, {} relationship types, {} patterns, {} domains",
            schema.tables.len(),
            schema.relationships.len(),
            schema.graph_patterns.len(),
            schema.semantic_domains.len()
        );
        Ok(schema)
    }
    async fn load_configurations(
        patterns_path: &str,
        domains_path: &str,
    ) -> Result<GraphSchema, SchemaAnalyserError> {
        let patterns_content = fs::read_to_string(patterns_path).await?;
        let domains_content = fs::read_to_string(domains_path).await?;
        let patterns_config: PatternsConfig = serde_yaml::from_str(&patterns_content)?;
        let domains_config: DomainsConfig = serde_yaml::from_str(&domains_content)?;
        Ok(GraphSchema {
            tables: HashMap::new(),
            relationships: HashMap::new(),
            graph_patterns: patterns_config.patterns,
            semantic_domains: domains_config.domains,
            node_search_patterns: HashMap::new(),
        })
    }
    async fn introspect_database_natively(
        db: &Surreal<Client>,
        schema: &mut GraphSchema,
    ) -> Result<(), SchemaAnalyserError> {
        let mut response = db.query("INFO FOR DB;").await?;
        let db_info: Option<serde_json::Value> = response.take(0)?;
        if let Some(serde_json::Value::Object(info)) = db_info {
            if let Some(serde_json::Value::Object(tables)) = info.get("tables") {
                for (table_name, _) in tables.iter() {
                    Self::introspect_table(db, table_name, schema).await?;
                }
            }
        } else {
            return Err(SchemaAnalyserError::Introspection(
                "Could not retrieve database info".to_string(),
            ));
        }
        Ok(())
    }
    async fn introspect_table(
        db: &Surreal<Client>,
        table_name: &str,
        schema: &mut GraphSchema,
    ) -> Result<(), SchemaAnalyserError> {
        let t = sanitize_table_name(table_name);
        let mut response = db.query(format!("INFO FOR TABLE {t};")).await?;
        let table_info: Option<serde_json::Value> = response.take(0)?;
        if let Some(serde_json::Value::Object(info)) = table_info {
            let mut fields = HashMap::new();
            if let Some(serde_json::Value::Object(field_defs)) = info.get("fields") {
                for (name, def) in field_defs.iter() {
                    if let serde_json::Value::String(def_str) = def {
                        fields.insert(
                            name.clone(),
                            FieldInfo {
                                field_type: def_str
                                    .split_whitespace()
                                    .last()
                                    .unwrap_or("any")
                                    .to_string(),
                                is_optional: !def_str.contains("ASSERT"),
                            },
                        );
                    }
                }
            }
            let mut indexes = Vec::new();
            if let Some(serde_json::Value::Object(index_defs)) = info.get("indexes") {
                for (name, def) in index_defs.iter() {
                    if let serde_json::Value::Object(def_obj) = def {
                        indexes.push(IndexInfo {
                            name: name.clone(),
                            fields: def_obj
                                .get("fields")
                                .and_then(|v| {
                                    if let serde_json::Value::String(s) = v {
                                        Some(s.to_string())
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or_default(),
                            unique: def_obj
                                .get("unique")
                                .and_then(|v| {
                                    if let serde_json::Value::Bool(b) = v {
                                        Some(*b)
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or(false),
                        });
                    }
                }
            }
            let is_schemaless = fields.is_empty();
            let table_type = if t == "edges" || t.starts_with("has_") || t.ends_with("_from") {
                TableType::Relation
            } else {
                TableType::Normal
            };
            schema.tables.insert(
                t.to_string(),
                TableSchema {
                    table_type,
                    is_schemaless,
                    fields,
                    indexes,
                    field_value_counts: HashMap::new(),
                    discovered_properties: HashMap::new(),
                },
            );
        }
        Ok(())
    }
    async fn discover_schemaless_patterns(
        db: &Surreal<Client>,
        schema: &mut GraphSchema,
    ) -> Result<(), SchemaAnalyserError> {
        for (table_name, table_schema) in schema.tables.iter_mut() {
            if table_schema.is_schemaless {
                let t = sanitize_table_name(table_name);
                let query = format!("SELECT properties FROM {t} LIMIT 100");
                let mut response = db.query(&query).await?;
                let results: Vec<serde_json::Value> = response.take(0)?;
                let mut property_keys = HashMap::new();
                for result in results {
                    if let serde_json::Value::Object(obj) = result {
                        if let Some(serde_json::Value::Object(props)) = obj.get("properties") {
                            for key in props.keys() {
                                property_keys
                                    .entry("properties".to_string())
                                    .or_insert_with(HashSet::new)
                                    .insert(key.clone());
                            }
                        }
                    }
                }
                table_schema.discovered_properties = property_keys;
                debug!(
                    "Discovered properties for schemaless table '{}': {:?}",
                    table_name, table_schema.discovered_properties
                );
                if table_name == "nodes" {
                    let mut type_counts_response = db
                        .query("SELECT type, count() FROM nodes GROUP BY type")
                        .await?;
                    let type_counts_results: Vec<serde_json::Value> =
                        type_counts_response.take(0)?;
                    let mut counts = HashMap::new();
                    for res in type_counts_results {
                        if let serde_json::Value::Object(obj) = res {
                            if let (
                                Some(serde_json::Value::String(type_val)),
                                Some(serde_json::Value::Number(count)),
                            ) = (obj.get("type"), obj.get("count"))
                            {
                                counts.insert(type_val.to_string(), count.as_u64().unwrap_or(0));
                            }
                        }
                    }
                    table_schema
                        .field_value_counts
                        .insert("type".to_string(), counts);
                }
            }
        }
        Ok(())
    }
    async fn analyse_relationship_patterns(
        db: &Surreal<Client>,
        schema: &mut GraphSchema,
    ) -> Result<(), SchemaAnalyserError> {
        let query = "SELECT label, count() AS count, math::mean(properties.confidence) AS avg_confidence FROM edges GROUP BY label";
        let mut response = db.query(query).await?;
        let results: Vec<serde_json::Value> = response.take(0)?;
        for result in results {
            if let serde_json::Value::Object(obj) = result {
                if let (
                    Some(serde_json::Value::String(label)),
                    Some(serde_json::Value::Number(count)),
                    Some(avg_confidence),
                ) = (
                    obj.get("label"),
                    obj.get("count"),
                    obj.get("avg_confidence"),
                ) {
                    let confidence = match avg_confidence {
                        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.8),
                        _ => 0.8,
                    };
                    schema.relationships.insert(
                        label.to_string(),
                        RelationshipType {
                            semantic_category: Self::categorise_relationship(label.as_str()),
                            directionality: Self::determine_directionality(label.as_str()),
                            avg_confidence: confidence,
                            count: count.as_u64().unwrap_or(0),
                        },
                    );
                }
            }
        }
        Ok(())
    }
    fn define_search_patterns(schema: &mut GraphSchema) {
        let mut patterns = HashMap::new();
        patterns.insert(
            "any entity type or category".to_string(),
            vec![
                "type".to_string(),
                "properties.entity_type".to_string(),
                "properties.metadata.type".to_string(),
            ],
        );
        patterns.insert(
            "entity name or label".to_string(),
            vec!["properties.name".to_string()],
        );
        schema.node_search_patterns = patterns;
        debug!(
            "Defined node search patterns: {:?}",
            schema.node_search_patterns
        );
    }
    fn categorise_relationship(label: &str) -> String {
        match label {
            _ if label.starts_with("HAS_") => "possession".to_string(),
            _ if label.starts_with("SECURES") | label.starts_with("PROTECTS") => {
                "security".to_string()
            }
            _ if label.starts_with("INTEGRATES") | label.starts_with("USES") => {
                "technology".to_string()
            }
            _ if label.starts_with("OCCURRED") | label.starts_with("DURING") => {
                "temporal".to_string()
            }
            _ if label.ends_with("_FROM") => "lineage".to_string(),
            _ => "general".to_string(),
        }
    }
    fn determine_directionality(label: &str) -> Directionality {
        match label {
            "HAS_SUBJECT" | "HAS_OBJECT" | "PROTECTS" | "SECURES" | "DERIVED_FROM" => {
                Directionality::Unidirectional
            }
            "INTEGRATES_WITH" | "CONCURRENT_WITH" => Directionality::Bidirectional,
            "HAS_FAMILY_MEMBER" => Directionality::Hierarchical,
            _ => Directionality::Unidirectional,
        }
    }
}
