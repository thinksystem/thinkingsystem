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
    Action, Entity, ExtractedData, KnowledgeNode, NumericalValue, Relationship, TemporalMarker,
};
use serde_json::Value;
use tracing::{debug, info, warn};
pub fn transform_llm_output(llm_json: &Value) -> Result<(ExtractedData, Option<String>), String> {
    debug!("Processing LLM output in adapter");
    if let Some(extracted_data_obj) = llm_json.get("extracted_data") {
        info!("Processing bundled extraction format");
        return process_bundled_format(extracted_data_obj);
    }
    match serde_json::from_value::<ExtractedData>(llm_json.clone()) {
        Ok(data) => {
            debug!("Successfully parsed LLM output into ExtractedData struct (direct format).");
            Ok((data, None))
        }
        Err(e) => {
            warn!(
                "Adapter failed to deserialise JSON into ExtractedData. Error: {}, JSON: {}",
                e, llm_json
            );
            Err(format!("Adapter failed to parse LLM output: {e}"))
        }
    }
}
fn process_bundled_format(
    extracted_data_obj: &Value,
) -> Result<(ExtractedData, Option<String>), String> {
    let mut extracted_data = ExtractedData::default();
    if let Some(nodes_array) = extracted_data_obj.get("nodes").and_then(|v| v.as_array()) {
        for node_value in nodes_array {
            match convert_bundled_node_to_knowledge_node(node_value) {
                Ok(knowledge_node) => extracted_data.nodes.push(knowledge_node),
                Err(e) => warn!(
                    "Failed to convert bundled node: {:?}, error: {}",
                    node_value, e
                ),
            }
        }
    }
    if let Some(relationships_array) = extracted_data_obj
        .get("relationships")
        .and_then(|v| v.as_array())
    {
        for rel_value in relationships_array {
            match serde_json::from_value::<Relationship>(rel_value.clone()) {
                Ok(relationship) => extracted_data.relationships.push(relationship),
                Err(e) => warn!(
                    "Failed to convert bundled relationship: {:?}, error: {}",
                    rel_value, e
                ),
            }
        }
    }
    info!(
        "Bundled extraction processed: {} nodes, {} relationships",
        extracted_data.nodes.len(),
        extracted_data.relationships.len()
    );
    Ok((extracted_data, None))
}
fn convert_bundled_node_to_knowledge_node(node_value: &Value) -> Result<KnowledgeNode, String> {
    let node_type = node_value
        .get("node_type")
        .and_then(|v| v.as_str())
        .ok_or("Missing node_type field")?;
    let data = node_value.get("data").ok_or("Missing data field")?;
    match node_type {
        "Entity" => {
            let entity: Entity = serde_json::from_value(data.clone())
                .map_err(|e| format!("Failed to parse Entity: {e}"))?;
            Ok(KnowledgeNode::Entity(entity))
        }
        "Action" => {
            let action: Action = serde_json::from_value(data.clone())
                .map_err(|e| format!("Failed to parse Action: {e}"))?;
            Ok(KnowledgeNode::Action(action))
        }
        "Temporal" => {
            let mut temporal_data = data.clone();
            let value_to_copy = temporal_data.get("value").cloned();
            if let Some(value) = value_to_copy {
                if temporal_data.get("date_text").is_none() {
                    temporal_data
                        .as_object_mut()
                        .unwrap()
                        .insert("date_text".to_string(), value);
                }
            }
            if let Some(obj) = temporal_data.as_object_mut() {
                obj.remove("value");
                obj.remove("temporal_type");
            }
            let temporal: TemporalMarker = serde_json::from_value(temporal_data)
                .map_err(|e| format!("Failed to parse Temporal: {e}"))?;
            Ok(KnowledgeNode::Temporal(temporal))
        }
        "Numerical" => {
            let numerical: NumericalValue = serde_json::from_value(data.clone())
                .map_err(|e| format!("Failed to parse Numerical: {e}"))?;
            Ok(KnowledgeNode::Numerical(numerical))
        }
        _ => Err(format!("Unknown node type: {node_type}")),
    }
}
