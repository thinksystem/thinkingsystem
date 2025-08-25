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
use crate::database::regulariser::Regulariser;
use crate::database::structured_store::StructuredStore;
use crate::nlu::orchestrator::data_models::{ExtractedData, UnifiedNLUData};
use bson;
use once_cell::sync::Lazy;
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
    enable_regulariser: bool,
}

impl DynamicStorage {
    pub fn new(db_client: Arc<Surreal<Client>>) -> Self {
        Self {
            db_client,
            enable_regulariser: false,
        }
    }

    pub fn with_regulariser(db_client: Arc<Surreal<Client>>, enable_regulariser: bool) -> Self {
        Self {
            db_client,
            enable_regulariser,
        }
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
        
        if let Err(e) = self
            .create_relationships(
                extracted_data,
                &mut created_node_map,
                &mut results,
                &utterance_id,
            )
            .await
        {
            warn!(
                error = %e,
                "Some relationships failed to create; proceeding to link node lineage"
            );
        }
        
        self.link_nodes_to_utterance(&created_node_map, &utterance_id)
            .await;

        if self.enable_regulariser {
            let canonical_client = match StructuredStore::connect_canonical_from_env().await {
                Ok(c) => c,
                Err(e) => {
                    warn!(error = %e, "Canonical DB not configured/available; skipping regulariser");
                    return Ok(json!({
                        "utterance_id": utterance_id.to_string(),
                        "nlu_data_id": nlu_data_id.to_string(),
                        "operations_completed": results.len(),
                        "results": results
                    }));
                }
            };

            let same_database = {
                use std::env;
                let d_url = env::var("SURREALDB_URL").unwrap_or_default();
                let c_url = env::var("STELE_CANON_URL")
                    .unwrap_or_else(|_| env::var("SURREALDB_URL").unwrap_or_default());
                let d_ns = env::var("SURREALDB_NS").unwrap_or_default();
                let c_ns = env::var("STELE_CANON_NS")
                    .unwrap_or_else(|_| env::var("SURREALDB_NS").unwrap_or_default());
                let d_db = env::var("SURREALDB_DB").unwrap_or_default();
                let c_db = env::var("STELE_CANON_DB")
                    .unwrap_or_else(|_| env::var("SURREALDB_DB").unwrap_or_default());
                d_url == c_url && d_ns == c_ns && d_db == c_db
            };
            let store = StructuredStore::new_with_clients(
                canonical_client,
                self.db_client.clone(),
                same_database,
            );
            let regulariser = Regulariser::new(store);
            let extracted = unified_data.extracted_data.clone();
            let lineage_node_map = created_node_map.clone();
            let lineage_utterance = utterance_id.clone();
            tokio::spawn(async move {
                let _inflight = crate::policy::backpressure::inflight_guard();
                let start = std::time::Instant::now();
                let result = regulariser
                    .regularise_extracted_data_with_lineage(
                        &extracted,
                        Some(&lineage_node_map),
                        Some(&lineage_utterance),
                    )
                    .await;
                let latency_ms = start.elapsed().as_millis() as f64;

                let failed = result.is_err();
                crate::policy::backpressure::record_run_metrics(latency_ms, failed, 1);
                crate::policy::backpressure::log_snapshot_if_enabled();
                match result {
                    Ok(outcome) => tracing::info!(
                        nodes = extracted.nodes.len(),
                        rels = extracted.relationships.len(),
                        entity_ids = outcome.entity_ids.len(),
                        task_ids = outcome.task_ids.len(),
                        event_ids = outcome.event_ids.len(),
                        fact_ids = outcome.relationship_fact_ids.len(),
                        "Background regulariser completed successfully"
                    ),
                    Err(e) => tracing::error!(
                        error = %e,
                        nodes = extracted.nodes.len(),
                        rels = extracted.relationships.len(),
                        "Background regulariser failed"
                    ),
                }
            });
        }

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
        node_map: &mut HashMap<String, RecordId>,
        results: &mut Vec<Value>,
        utterance_id: &RecordId,
    ) -> Result<(), String> {
        
        let allow_placeholders = std::env::var("STELE_ENABLE_EDGE_PLACEHOLDERS")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        for rel in &extracted_data.relationships {
            
            let label = Self::normalize_relation_label(&rel.relation_type);

            
            let mut source_id_opt = node_map.get(&rel.source).cloned();
            let mut target_id_opt = node_map.get(&rel.target).cloned();

            if source_id_opt.is_none() && allow_placeholders {
                if let Ok(pid) = self.create_placeholder_node(&rel.source).await {
                    node_map.insert(rel.source.clone(), pid.clone());
                    source_id_opt = Some(pid);
                }
            }
            if target_id_opt.is_none() && allow_placeholders {
                if let Ok(pid) = self.create_placeholder_node(&rel.target).await {
                    node_map.insert(rel.target.clone(), pid.clone());
                    target_id_opt = Some(pid);
                }
            }

            let (Some(source_id), Some(target_id)) = (source_id_opt, target_id_opt) else {
                warn!(
                    "Could not create relationship for '{}' because source ('{}') or target ('{}') node was not found in the map.",
                    rel.relation_type, rel.source, rel.target
                );
                continue;
            };

            
            if source_id == target_id
                && !std::env::var("STELE_ALLOW_SELF_RELATIONSHIPS")
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    .unwrap_or(false)
            {
                warn!(
                    "Skipping self-relationship '{}' on node {}",
                    label, source_id
                );
                continue;
            }

            let properties = json!({
                "confidence": rel.confidence,
                "metadata": rel.metadata,
            });

            
            let mut existing_q = match self
                .db_client
                .query(
                    "SELECT id, properties FROM edges WHERE in = $src AND out = $dst AND label = $label LIMIT 1",
                )
                .bind(("src", source_id.clone()))
                .bind(("dst", target_id.clone()))
                .bind(("label", label.clone()))
                .await
            {
                Ok(q) => q,
                Err(e) => {
                    warn!(
                        error = %e,
                        from = %source_id,
                        to = %target_id,
                        label = %label,
                        "Failed to check existing relationship; skipping this rel"
                    );
                    continue;
                }
            };

            
            let existing: Vec<serde_json::Value> = match existing_q.take(0) {
                Ok(v) => v,
                Err(e) => {
                    warn!(
                        error = %e,
                        from = %source_id,
                        to = %target_id,
                        label = %label,
                        "Failed to read existing edge lookup; skipping this rel"
                    );
                    continue;
                }
            };

            if let Some(row) = existing.first() {
                
                let existing_id: RecordId = match row.get("id") {
                    Some(id_val) => {
                        if let Some(s) = id_val.as_str() {
                            match s.parse::<RecordId>() {
                                Ok(id) => id,
                                Err(e) => {
                                    warn!(
                                        error = %e,
                                        value = %s,
                                        "Failed to parse existing edge id; skipping update"
                                    );
                                    continue;
                                }
                            }
                        } else {
                            match serde_json::from_value::<RecordId>(id_val.clone()) {
                                Ok(id) => id,
                                Err(e) => {
                                    warn!(
                                        error = %e,
                                        "Failed to decode existing edge id; skipping update"
                                    );
                                    continue;
                                }
                            }
                        }
                    }
                    None => {
                        warn!("Existing edge id missing; skipping update");
                        continue;
                    }
                };

                
                let mut upd = match self
                    .db_client
                    .query(
                        "UPDATE $id SET properties = merge(properties, $properties), updated_at = time::now() RETURN AFTER",
                    )
                    .bind(("id", existing_id.clone()))
                    .bind(("properties", properties.clone()))
                    .await
                {
                    Ok(q) => q,
                    Err(e) => {
                        warn!(
                            error = %e,
                            id = %existing_id,
                            "Failed to update existing relationship; skipping"
                        );
                        continue;
                    }
                };

                let updated: Vec<CreatedRecord> = match upd.take(0) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!(
                            error = %e,
                            id = %existing_id,
                            "Failed to read updated edge; continuing"
                        );
                        continue;
                    }
                };

                if let Some(record) = updated.first() {
                    
                    if let Err(e) = self
                        .db_client
                        .query("RELATE $id->edge_derived_from->$utt")
                        .bind(("id", record.id.clone()))
                        .bind(("utt", utterance_id.clone()))
                        .await
                    {
                        warn!(
                            error = %e,
                            id = %record.id,
                            "Failed to relate edge_derived_from for updated edge"
                        );
                    }
                    results.push(json!({
                        "status": "updated",
                        "table": "edges",
                        "record_id": record.id,
                        "label": label,
                        "from": source_id.to_string(),
                        "to": target_id.to_string(),
                    }));
                }
                continue;
            }

            debug!(
                "Creating edge '{}' from {} to {}",
                label, source_id, target_id
            );

            
            let mut response = match self
                .db_client
                .query("RELATE $source_id->edges->$target_id SET label = $label, properties = $properties, created_at = time::now() RETURN AFTER")
                .bind(("source_id", source_id.clone()))
                .bind(("target_id", target_id.clone()))
                .bind(("label", label.clone()))
                .bind(("properties", properties))
                .await
            {
                Ok(q) => q,
                Err(e) => {
                    warn!(
                        error = %e,
                        from = %source_id,
                        to = %target_id,
                        label = %label,
                        "Failed to create relationship edge; skipping"
                    );
                    continue;
                }
            };

            let created: Vec<CreatedRecord> = match response.take(0) {
                Ok(v) => v,
                Err(e) => {
                    warn!(
                        error = %e,
                        from = %source_id,
                        to = %target_id,
                        label = %label,
                        "Failed to read created edge; continuing"
                    );
                    continue;
                }
            };

            if let Some(record) = created.first() {
                
                if let Err(e) = self
                    .db_client
                    .query("RELATE $id->edge_derived_from->$utt")
                    .bind(("id", record.id.clone()))
                    .bind(("utt", utterance_id.clone()))
                    .await
                {
                    warn!(
                        error = %e,
                        id = %record.id,
                        "Failed to relate edge_derived_from for created edge"
                    );
                }
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
        }
        Ok(())
    }

    
    async fn create_placeholder_node(&self, temp_ref: &str) -> Result<RecordId, String> {
        let mut resp = self
            .db_client
            .query("CREATE nodes SET type = 'placeholder', properties = { temp_ref: $r, source: 'edge_backfill' }")
            .bind(("r", temp_ref.to_string()))
            .await
            .map_err(|e| format!("Failed to create placeholder node: {e}"))?;

        let created: Vec<CreatedRecord> = resp
            .take(0)
            .map_err(|e| format!("Failed to deserialise placeholder node: {e}"))?;
        let record = created
            .first()
            .ok_or("DB did not return a record for placeholder node")?;
        Ok(record.id.clone())
    }

    fn normalize_relation_label(raw: &str) -> String {
        static PREDICATE_ALIASES: Lazy<HashMap<String, String>> = Lazy::new(|| {
            if let Ok(json_str) = std::env::var("STELE_PREDICATE_ALIASES") {
                serde_json::from_str::<HashMap<String, String>>(&json_str).unwrap_or_default()
            } else {
                HashMap::new()
            }
        });

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
