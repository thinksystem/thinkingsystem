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
use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, info, warn};

static PREDICATE_ALIASES: Lazy<HashMap<String, String>> = Lazy::new(|| {
    if let Ok(json_str) = std::env::var("STELE_PREDICATE_ALIASES") {
        serde_json::from_str::<HashMap<String, String>>(&json_str).unwrap_or_default()
    } else {
        HashMap::new()
    }
});

fn normalize_relation_label(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => out.push(ch.to_ascii_uppercase()),
            ' ' | '\t' | '-' | '\n' | '\r' | '/' => out.push('_'),
            '_' => out.push('_'),
            _ => {  }
        }
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    if out.starts_with('_') {
        out = out.trim_start_matches('_').to_string();
    }
    if out.ends_with('_') {
        out = out.trim_end_matches('_').to_string();
    }
    if let Some(mapped) = PREDICATE_ALIASES.get(&out) {
        return mapped.clone();
    }
    if out.is_empty() {
        "RELATES_TO".to_string()
    } else {
        out
    }
}
pub fn transform_llm_output(llm_json: &Value) -> Result<(ExtractedData, Option<String>), String> {
    debug!("Processing LLM output in adapter");
    
    if llm_json.get("canonical_entity").is_some()
        || llm_json.get("canonical_event").is_some()
        || llm_json.get("canonical_task").is_some()
        || llm_json.get("canonical_relationship_fact").is_some()
    {
        info!("Processing canonicalize format");
        return process_canonical_format(llm_json);
    }
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
fn sanitize_id_component(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}
fn process_canonical_format(llm_json: &Value) -> Result<(ExtractedData, Option<String>), String> {
    let mut extracted = ExtractedData::default();
    let mut temp_index = 0usize;
    use std::collections::HashMap;
    let mut name_to_id: HashMap<String, String> = HashMap::new();
    
    if let Some(arr) = llm_json.get("canonical_entity").and_then(|v| v.as_array()) {
        for ent in arr {
            let name = ent.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                continue;
            }
            let etype = ent
                .get("entity_type")
                .and_then(|v| v.as_str())
                .unwrap_or("other")
                .to_string();
            let temp_id = format!("can_entity_{}_{}", temp_index, sanitize_id_component(name));
            temp_index += 1;
            let confidence = ent
                .get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.8) as f32;
            let mut metadata = ent
                .get("metadata")
                .cloned()
                .unwrap_or(serde_json::json!({}));
            if let Some(obj) = metadata.as_object_mut() {
                if let Some(key) = ent.get("canonical_key").and_then(|v| v.as_str()) {
                    obj.insert("canonical_key".into(), Value::String(key.to_string()));
                }
                obj.insert("source".into(), Value::String("canonicalize".into()));
            }
            extracted.nodes.push(KnowledgeNode::Entity(Entity {
                temp_id: temp_id.clone(),
                name: name.to_string(),
                entity_type: etype,
                confidence,
                metadata: Some(metadata),
            }));
            name_to_id.insert(name.to_lowercase(), temp_id);
        }
    }
    
    if let Some(arr) = llm_json.get("canonical_event").and_then(|v| v.as_array()) {
        for ev in arr {
            let title = ev.get("title").and_then(|v| v.as_str()).unwrap_or("event");
            let start_at = ev.get("start_at").and_then(|v| v.as_str());
            let end_at = ev.get("end_at").and_then(|v| v.as_str());
            let tz = ev.get("timezone").and_then(|v| v.as_str());
            let loc = ev.get("location").and_then(|v| v.as_str());
            let status = ev.get("status").and_then(|v| v.as_str());
            let temp_id = format!("can_event_{}_{}", temp_index, sanitize_id_component(title));
            temp_index += 1;
            let confidence = ev.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.8) as f32;
            let mut metadata = ev.get("metadata").cloned().unwrap_or(serde_json::json!({}));
            if let Some(obj) = metadata.as_object_mut() {
                if let Some(key) = ev.get("canonical_key").and_then(|v| v.as_str()) {
                    obj.insert("canonical_key".into(), Value::String(key.to_string()));
                }
                if let Some(tz) = tz {
                    obj.insert("timezone".into(), Value::String(tz.to_string()));
                }
                if let Some(e) = end_at {
                    obj.insert("end_at".into(), Value::String(e.to_string()));
                }
                if let Some(s) = status {
                    obj.insert("status".into(), Value::String(s.to_string()));
                }
                if let Some(l) = loc {
                    obj.insert("location_hint".into(), Value::String(l.to_string()));
                }
                obj.insert("source".into(), Value::String("canonicalize".into()));
                obj.insert("title".into(), Value::String(title.to_string()));
            }
            extracted
                .nodes
                .push(KnowledgeNode::Temporal(TemporalMarker {
                    temp_id: temp_id.clone(),
                    date_text: start_at.unwrap_or(title).to_string(),
                    resolved_date: start_at.map(|s| s.to_string()),
                    confidence,
                    metadata: Some(metadata),
                }));
            
            
            name_to_id.insert(title.to_lowercase(), temp_id);
        }
    }
    
    if let Some(arr) = llm_json
        .get("canonical_relationship_fact")
        .and_then(|v| v.as_array())
    {
        for rf in arr {
            let predicate = rf
                .get("relation_type")
                .and_then(|v| v.as_str())
                .unwrap_or("RELATES_TO")
                .to_string();
            let subj_name = rf
                .get("subject_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            let obj_name = rf
                .get("object_title_or_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            if subj_name.is_empty() || obj_name.is_empty() {
                continue;
            }
            if let (Some(sid), Some(oid)) = (name_to_id.get(&subj_name), name_to_id.get(&obj_name))
            {
                let conf = rf.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.8) as f32;
                extracted.relationships.push(Relationship {
                    source: sid.clone(),
                    target: oid.clone(),
                    relation_type: normalize_relation_label(&predicate),
                    confidence: conf,
                    metadata: rf.get("metadata").cloned(),
                });
            }
        }
    }
    Ok((extracted, None))
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
    
    use std::collections::HashSet;
    let mut known_ids: HashSet<String> = extracted_data
        .nodes
        .iter()
        .map(|n| n.temp_id().to_string())
        .collect();
    
    let mut relationships: Vec<Relationship> = Vec::new();
    if let Some(relationships_array) = extracted_data_obj
        .get("relationships")
        .and_then(|v| v.as_array())
    {
        for rel_value in relationships_array {
            match serde_json::from_value::<Relationship>(rel_value.clone()) {
                Ok(mut relationship) => {
                    relationship.relation_type =
                        normalize_relation_label(&relationship.relation_type);
                    relationships.push(relationship)
                }
                Err(e) => warn!(
                    "Failed to convert bundled relationship: {:?}, error: {}",
                    rel_value, e
                ),
            }
        }
    }
    
    
    let enable_adapter_temporal_synth = std::env::var("STELE_ENABLE_ADAPTER_TEMPORAL_SYNTH")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if enable_adapter_temporal_synth {
        let mut synthesized_temporals: Vec<KnowledgeNode> = Vec::new();
        for rel in &relationships {
            for endpoint in [rel.source.as_str(), rel.target.as_str()] {
                if known_ids.contains(endpoint) {
                    continue;
                }
                let lower = endpoint.to_lowercase();
                let looks_temporal = lower.starts_with("temporal_")
                    || lower.starts_with("time_")
                    || lower.starts_with("date_")
                    || rel.relation_type.eq_ignore_ascii_case("TIMED_AT");
                if looks_temporal {
                    let date_text = endpoint
                        .trim_start_matches("temporal_")
                        .trim_start_matches("time_")
                        .trim_start_matches("date_")
                        .replace('_', " ");
                    let temporal = TemporalMarker {
                        temp_id: endpoint.to_string(),
                        date_text: if date_text.is_empty() {
                            endpoint.to_string()
                        } else {
                            date_text
                        },
                        resolved_date: None,
                        confidence: 0.75,
                        metadata: None,
                    };
                    synthesized_temporals.push(KnowledgeNode::Temporal(temporal));
                    known_ids.insert(endpoint.to_string());
                }
            }
        }
        if !synthesized_temporals.is_empty() {
            info!(
                count = synthesized_temporals.len(),
                "Adapter: synthesized missing Temporal nodes from relationships (feature-gated)"
            );
            extracted_data.nodes.extend(synthesized_temporals);
        }
    } else {
        debug!("Adapter temporal synthesis disabled (STELE_ENABLE_ADAPTER_TEMPORAL_SYNTH != true)");
    }
    
    extracted_data.relationships = relationships;
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
        
        "Location" => {
            let mut entity_data = data.clone();
            
            if entity_data.get("entity_type").is_none() {
                if let Some(obj) = entity_data.as_object_mut() {
                    obj.insert("entity_type".to_string(), Value::String("location".into()));
                }
            }
            let entity: Entity = serde_json::from_value(entity_data)
                .map_err(|e| format!("Failed to parse Location as Entity: {e}"))?;
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
