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

use crate::database::knowledge_adapter::KnowledgeNodeAdapter;
use crate::nlu::orchestrator::data_models::{ExtractedData, UnifiedNLUData};
use bson;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use surrealdb::{engine::remote::ws::Client, sql::Bytes, RecordId, Surreal};
use tracing::{debug, info, warn};

#[derive(Deserialize, Debug)]
struct CreatedRecord {
    id: RecordId,
}

#[derive(Deserialize, Debug)]
struct UtteranceRecord {
    id: RecordId,
    raw_text: Option<String>,
    from_source: Option<RecordId>,
}

pub struct DynamicStorage {
    db_client: Arc<Surreal<Client>>,
}

impl DynamicStorage {
    pub fn new(db_client: Arc<Surreal<Client>>) -> Self {
        Self { db_client }
    }

    pub async fn store_llm_output(
        &self,
        user_id: &str,
        channel: &str,
        raw_text: &str,
        unified_data: &UnifiedNLUData,
    ) -> Result<Value, String> {
        let mut results = Vec::new();
        let mut created_node_map: HashMap<String, RecordId> = HashMap::new();

        let source_id = self
            .find_or_create_source(user_id, channel, &mut results)
            .await?;
        let utterance_id = self
            .create_utterance_record(&source_id, raw_text, &mut results)
            .await?;
        let nlu_data_id = self
            .create_nlu_data_record(unified_data, &mut results)
            .await?;

        self.link_utterance_to_nlu_data(&utterance_id, &nlu_data_id)
            .await?;

        let extracted_data = &unified_data.extracted_data;
        self.create_nodes(extracted_data, &mut created_node_map, &mut results)
            .await?;
        self.create_relationships(extracted_data, &created_node_map, &mut results)
            .await?;
        self.link_nodes_to_utterance(&created_node_map, &utterance_id)
            .await;

        Ok(json!({
            "utterance_id": utterance_id.to_string(),
            "nlu_data_id": nlu_data_id.to_string(),
            "operations_completed": results.len(),
            "results": results
        }))
    }

    async fn find_or_create_source(
        &self,
        user_id: &str,
        channel: &str,
        results: &mut Vec<Value>,
    ) -> Result<RecordId, String> {
        let query = "UPSERT source SET user_id = $user_id, channel = $channel, properties = {} WHERE user_id = $user_id";
        let mut response = self
            .db_client
            .query(query)
            .bind(("user_id", user_id.to_string()))
            .bind(("channel", channel.to_string()))
            .await
            .map_err(|e| format!("Failed to upsert source: {e}"))?;

        let created: Vec<CreatedRecord> = response
            .take(0)
            .map_err(|e| format!("Failed to deserialise upserted source: {e}"))?;

        let record = created
            .first()
            .ok_or("DB did not return a record for upserted source")?;

        results.push(json!({ "status": "upserted", "table": "source", "record_id": record.id }));
        Ok(record.id.clone())
    }

    async fn create_utterance_record(
        &self,
        source_id: &RecordId,
        raw_text: &str,
        results: &mut Vec<Value>,
    ) -> Result<RecordId, String> {
        let query = "CREATE utterance SET from_source = $source_id, raw_text = $raw_text";
        let mut response = self
            .db_client
            .query(query)
            .bind(("source_id", source_id.clone()))
            .bind(("raw_text", raw_text.to_string()))
            .await
            .map_err(|e| format!("Failed to create utterance record: {e}"))?;

        let created: Vec<CreatedRecord> = response
            .take(0)
            .map_err(|e| format!("Failed to deserialise created utterance: {e}"))?;

        let record = created
            .first()
            .ok_or("DB did not return a record for created utterance")?;

        results.push(json!({ "status": "created", "table": "utterance", "record_id": record.id }));
        Ok(record.id.clone())
    }

    async fn create_nlu_data_record(
        &self,
        unified_data: &UnifiedNLUData,
        results: &mut Vec<Value>,
    ) -> Result<RecordId, String> {
        let nlu_output_bson = bson::to_vec(unified_data)
            .map_err(|e| format!("Failed to serialise nlu_output to BSON: {e}"))?;
        let nlu_output_json = serde_json::to_string(unified_data)
            .map_err(|e| format!("Failed to serialise nlu_output to JSON string: {e}"))?;

        let query = "CREATE nlu_data SET data_bson = $bson, data_json = $json";
        let mut response = self
            .db_client
            .query(query)
            .bind(("bson", Bytes::from(nlu_output_bson)))
            .bind(("json", nlu_output_json))
            .await
            .map_err(|e| format!("Failed to create nlu_data record: {e}"))?;

        let created: Vec<CreatedRecord> = response
            .take(0)
            .map_err(|e| format!("Failed to deserialise created nlu_data: {e}"))?;

        let record = created
            .first()
            .ok_or("DB did not return a record for created nlu_data")?;

        results.push(json!({ "status": "created", "table": "nlu_data", "record_id": record.id }));
        Ok(record.id.clone())
    }

    async fn link_utterance_to_nlu_data(
        &self,
        utterance_id: &RecordId,
        nlu_data_id: &RecordId,
    ) -> Result<(), String> {
        let query = "RELATE $utterance->has_nlu_output->$nlu_data";
        self.db_client
            .query(query)
            .bind(("utterance", utterance_id.clone()))
            .bind(("nlu_data", nlu_data_id.clone()))
            .await
            .map_err(|e| format!("Failed to link utterance to nlu_data: {e}"))?;
        Ok(())
    }

    async fn create_nodes(
        &self,
        extracted_data: &ExtractedData,
        node_map: &mut HashMap<String, RecordId>,
        results: &mut Vec<Value>,
    ) -> Result<(), String> {
        for node in &extracted_data.nodes {
            let temp_id = node.temp_id();
            if temp_id.is_empty() {
                warn!("Skipping node with empty temp_id: {:?}", node);
                continue;
            }

            let (node_type, properties) = match KnowledgeNodeAdapter::to_database_format(node) {
                Ok(data) => data,
                Err(e) => {
                    warn!(
                        "Failed to convert node with temp_id '{}' to DB format: {}",
                        temp_id, e
                    );
                    continue;
                }
            };

            debug!(
                "Creating node with type '{}' for temp_id '{}'",
                node_type, temp_id
            );

            let query = "CREATE nodes SET type = $node_type, properties = $properties";
            let mut response = self
                .db_client
                .query(query)
                .bind(("node_type", node_type))
                .bind(("properties", properties))
                .await
                .map_err(|e| format!("DB query failed for temp_id '{temp_id}': {e}"))?;

            let created: Vec<CreatedRecord> = response.take(0).map_err(|e| {
                format!("Failed to deserialise DB response for temp_id '{temp_id}': {e}")
            })?;

            if let Some(record) = created.first() {
                node_map.insert(temp_id.to_string(), record.id.clone());
                results.push(json!({ "status": "created", "table": "nodes", "record_id": record.id, "temp_id": temp_id }));
            } else {
                warn!(
                    "DB did not return a record after create operation for temp_id '{}'",
                    temp_id
                );
            }
        }
        Ok(())
    }

    async fn create_relationships(
        &self,
        extracted_data: &ExtractedData,
        node_map: &HashMap<String, RecordId>,
        results: &mut Vec<Value>,
    ) -> Result<(), String> {
        for rel in &extracted_data.relationships {
            if let (Some(source_id), Some(target_id)) =
                (node_map.get(&rel.source), node_map.get(&rel.target))
            {
                let label = rel.relation_type.to_uppercase();
                let properties = json!({
                    "confidence": rel.confidence,
                    "metadata": rel.metadata,
                });

                debug!(
                    "Creating edge '{}' from {} to {}",
                    label, source_id, target_id
                );

                let query = "RELATE $source_id->edges->$target_id SET label = $label, properties = $properties";
                let mut response = self
                    .db_client
                    .query(query)
                    .bind(("source_id", source_id.clone()))
                    .bind(("target_id", target_id.clone()))
                    .bind(("label", label.clone()))
                    .bind(("properties", properties))
                    .await
                    .map_err(|e| format!("Failed to create relationship edge: {e}"))?;

                let created: Vec<CreatedRecord> = response
                    .take(0)
                    .map_err(|e| format!("Failed to deserialise created edge: {e}"))?;

                if let Some(record) = created.first() {
                    results.push(json!({
                        "status": "created",
                        "table": "edges",
                        "record_id": record.id,
                        "label": label,
                        "from": source_id.to_string(),
                        "to": target_id.to_string(),
                    }));
                } else {
                    warn!(
                        "Failed to create relationship edge '{}': No record returned.",
                        rel.relation_type
                    );
                }
            } else {
                warn!("Could not create relationship for '{}' because source ('{}') or target ('{}') node was not found in the map.", rel.relation_type, rel.source, rel.target);
            }
        }
        Ok(())
    }

    async fn link_nodes_to_utterance(
        &self,
        node_map: &HashMap<String, RecordId>,
        utterance_id: &RecordId,
    ) {
        let mut link_count = 0;
        for node_id in node_map.values() {
            let query = "RELATE $node_id->derived_from->$utterance_id";
            if let Err(e) = self
                .db_client
                .query(query)
                .bind(("node_id", node_id.clone()))
                .bind(("utterance_id", utterance_id.clone()))
                .await
            {
                warn!(
                    "Failed to create 'derived_from' link for node {}: {}",
                    node_id, e
                );
            } else {
                link_count += 1;
            }
        }
        info!(
            "Successfully created {} 'derived_from' links to utterance {}.",
            link_count, utterance_id
        );
    }

    pub async fn get_utterances_for_nodes(&self, node_ids: &[String]) -> Result<Value, String> {
        if node_ids.is_empty() {
            return Ok(json!([]));
        }

        let node_record_ids: Result<Vec<RecordId>, _> =
            node_ids.iter().map(|id| id.parse::<RecordId>()).collect();

        let node_record_ids =
            node_record_ids.map_err(|e| format!("Failed to parse node IDs as RecordIds: {e}"))?;

        let mut all_utterance_ids = Vec::new();
        for node_id in &node_record_ids {
            let single_query = "SELECT VALUE out FROM derived_from WHERE in = $node_id";
            let mut response = self
                .db_client
                .query(single_query)
                .bind(("node_id", node_id.clone()))
                .await
                .map_err(|e| format!("Failed to retrieve utterance IDs for node {node_id}: {e}"))?;

            let utterance_ids: Vec<RecordId> = response.take(0).map_err(|e| {
                format!("Failed to deserialise utterance IDs for node {node_id}: {e}")
            })?;
            all_utterance_ids.extend(utterance_ids);
        }

        all_utterance_ids.sort();
        all_utterance_ids.dedup();

        if all_utterance_ids.is_empty() {
            return Ok(json!([]));
        }

        let query = "SELECT * FROM utterance WHERE id IN $utterance_ids";
        let mut response = self
            .db_client
            .query(query)
            .bind(("utterance_ids", all_utterance_ids))
            .await
            .map_err(|e| format!("Failed to retrieve utterances: {e}"))?;

        let utterances: Vec<UtteranceRecord> = response
            .take(0)
            .map_err(|e| format!("Failed to deserialise utterances: {e}"))?;

        if utterances.is_empty() {
            let alt_query = "SELECT out.* as utterance FROM derived_from WHERE in IN $node_ids";
            let mut alt_response = self
                .db_client
                .query(alt_query)
                .bind(("node_ids", node_record_ids))
                .await
                .map_err(|e| format!("Failed to execute alternative query: {e}"))?;

            let alt_utterances: Vec<Value> = alt_response
                .take(0)
                .map_err(|e| format!("Failed to deserialise alternative results: {e}"))?;
            return Ok(json!(alt_utterances));
        }

        let utterances_json: Vec<Value> = utterances
            .into_iter()
            .map(|u| {
                json!({
                    "id": u.id.to_string(),
                    "raw_text": u.raw_text,
                    "from_source": u.from_source.map(|s| s.to_string())
                })
            })
            .collect();

        Ok(json!(utterances_json))
    }
}
