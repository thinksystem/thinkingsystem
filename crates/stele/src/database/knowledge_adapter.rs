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

use crate::nlu::orchestrator::data_models::{
    Action, Entity, KnowledgeNode, NumericalValue, TemporalMarker,
};
use serde_json::{from_value, Value};
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("The provided database record was not a JSON object.")]
    NotAnObject,
    #[error("The database record is missing the required 'type' field.")]
    MissingTypeField,
    #[error("The database record is missing the required 'properties' field.")]
    MissingPropertiesField,
    #[error("Failed to deserialise the 'properties' field for node type '{node_type}': {source}")]
    DeserializationError {
        node_type: String,
        #[source]
        source: serde_json::Error,
    },
}
pub struct KnowledgeNodeAdapter;
impl KnowledgeNodeAdapter {
    pub fn from_database_record(record_value: Value) -> Result<KnowledgeNode, AdapterError> {
        let record_obj = record_value.as_object().ok_or(AdapterError::NotAnObject)?;
        let node_type = record_obj
            .get("type")
            .and_then(Value::as_str)
            .ok_or(AdapterError::MissingTypeField)?;
        let mut properties = record_obj
            .get("properties")
            .cloned()
            .ok_or(AdapterError::MissingPropertiesField)?;
        let temp_id = record_obj
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        match node_type {
            "temporal" => {
                if let Value::Object(ref mut props_obj) = properties {
                    props_obj.insert("temp_id".to_string(), Value::String(temp_id.clone()));
                }
                let temporal: TemporalMarker =
                    from_value(properties).map_err(|e| AdapterError::DeserializationError {
                        node_type: "temporal".to_string(),
                        source: e,
                    })?;
                Ok(KnowledgeNode::Temporal(temporal))
            }
            "numerical" => {
                if let Value::Object(ref mut props_obj) = properties {
                    props_obj.insert("temp_id".to_string(), Value::String(temp_id.clone()));
                }
                let numerical: NumericalValue =
                    from_value(properties).map_err(|e| AdapterError::DeserializationError {
                        node_type: "numerical".to_string(),
                        source: e,
                    })?;
                Ok(KnowledgeNode::Numerical(numerical))
            }
            "action" => {
                if let Value::Object(ref mut props_obj) = properties {
                    props_obj.insert("temp_id".to_string(), Value::String(temp_id.clone()));
                }
                let action: Action =
                    from_value(properties).map_err(|e| AdapterError::DeserializationError {
                        node_type: "action".to_string(),
                        source: e,
                    })?;
                Ok(KnowledgeNode::Action(action))
            }
            _ => {
                if let Value::Object(ref mut props_obj) = properties {
                    props_obj.insert(
                        "entity_type".to_string(),
                        Value::String(node_type.to_string()),
                    );
                    props_obj.insert("temp_id".to_string(), Value::String(temp_id.clone()));
                    if !props_obj.contains_key("name") {
                        let name = props_obj
                            .get("title")
                            .or_else(|| props_obj.get("label"))
                            .or_else(|| props_obj.get("value"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown");
                        props_obj.insert("name".to_string(), Value::String(name.to_string()));
                    }
                    if !props_obj.contains_key("confidence") {
                        props_obj.insert(
                            "confidence".to_string(),
                            Value::Number(serde_json::Number::from_f64(1.0).unwrap()),
                        );
                    }
                }
                let entity: Entity =
                    from_value(properties).map_err(|e| AdapterError::DeserializationError {
                        node_type: node_type.to_string(),
                        source: e,
                    })?;
                Ok(KnowledgeNode::Entity(entity))
            }
        }
    }
    pub fn to_database_format(node: &KnowledgeNode) -> Result<(String, Value), AdapterError> {
        let result = match node {
            KnowledgeNode::Entity(e) => (
                e.entity_type.clone(),
                serde_json::json!({
                    "name": e.name,
                    "confidence": e.confidence,
                    "metadata": e.metadata
                }),
            ),
            KnowledgeNode::Temporal(t) => (
                "temporal".to_string(),
                serde_json::json!({
                    "date_text": t.date_text,
                    "resolved_date": t.resolved_date,
                    "confidence": t.confidence,
                    "metadata": t.metadata
                }),
            ),
            KnowledgeNode::Numerical(n) => (
                "numerical".to_string(),
                serde_json::json!({
                    "value": n.value,
                    "unit": n.unit,
                    "confidence": n.confidence,
                    "metadata": n.metadata
                }),
            ),
            KnowledgeNode::Action(a) => (
                "action".to_string(),
                serde_json::json!({
                    "verb": a.verb,
                    "confidence": a.confidence,
                    "metadata": a.metadata
                }),
            ),
        };
        Ok(result)
    }
    pub async fn get_existing_node_types(
        db_client: &surrealdb::Surreal<surrealdb::engine::remote::ws::Client>,
    ) -> Result<Vec<String>, surrealdb::Error> {
        let mut response = db_client.query("SELECT DISTINCT type FROM nodes").await?;
        let types: Vec<serde_json::Value> = response.take(0)?;
        let type_strings: Vec<String> = types
            .into_iter()
            .filter_map(|v| {
                v.get("type")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        Ok(type_strings)
    }
    pub fn can_hydrate_type(node_type: &str) -> bool {
        matches!(
            node_type.to_lowercase().as_str(),
            "entity"
                | "concept"
                | "relationship"
                | "event"
                | "location"
                | "person"
                | "organisation"
        )
    }
    pub async fn get_suggested_queries(
        db_client: &surrealdb::Surreal<surrealdb::engine::remote::ws::Client>,
    ) -> Result<Vec<String>, surrealdb::Error> {
        let types = Self::get_existing_node_types(db_client).await?;
        let mut suggestions = vec![
            "find all entities".to_string(),
            "show me everything".to_string(),
        ];
        for type_name in types {
            suggestions.push(format!("find all {type_name}"));
            suggestions.push(format!("search for {type_name}"));
            suggestions.push(format!("show me {type_name} entities"));
        }
        Ok(suggestions)
    }
}
