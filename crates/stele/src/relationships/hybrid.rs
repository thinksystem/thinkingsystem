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


use serde_json::Value;
use surrealdb::sql::Thing;

#[cfg(feature = "nlu_builders")]
use crate::builders::PendingRelationship;
use crate::database::types::DatabaseError;
use crate::StructuredStore;

#[cfg(feature = "nlu_builders")]
pub struct HybridRelationshipResult {
    pub fact_id: Option<Thing>,
    pub node_id: Option<Thing>,
    pub degraded: bool,
}

#[cfg(feature = "nlu_builders")]
pub async fn persist_hybrid_relationship(
    store: &StructuredStore,
    rel: &PendingRelationship,
    subject: &Thing,
    object: &Thing,
) -> Result<HybridRelationshipResult, DatabaseError> {
    crate::relationships::metrics::record_attempt();
    let prov_owned = rel.provenance.as_ref().map(|v| v.to_string());
    let fact_id = store
        .create_relationship_fact(
            subject,
            rel.predicate.as_deref().unwrap_or("related_to"),
            object,
            rel.confidence,
            prov_owned.as_deref(),
        )
        .await?;
    
    async fn attempt_create(
        store: &StructuredStore,
        rel: &PendingRelationship,
        subject: &Thing,
        object: &Thing,
    ) -> Result<Option<Thing>, DatabaseError> {
        let predicate = rel.predicate.clone().unwrap_or_else(|| "related_to".into());
        let provenance = rel
            .provenance
            .clone()
            .unwrap_or(Value::Object(Default::default()));
        let confidence = rel.confidence;
        let q = store.canonical_db().query(
            
            "CREATE relationship_node SET subject_ref = $s, object_ref = $o, predicate = $p, confidence = $c, provenance = $prov RETURN id;"
        )
            .bind(("s", subject.clone()))
            .bind(("o", object.clone()))
            .bind(("p", predicate.clone()))
            .bind(("c", confidence))
            .bind(("prov", provenance.clone()));
        match q.await {
            Ok(mut res) => {
                let rows: Vec<serde_json::Value> = res.take(0).unwrap_or_default();
                println!("[hybrid-create] rows_len={} rows={rows:?}", rows.len());
                if let Some(first) = rows.first() {
                    if let Some(id_str) = first.get("id").and_then(|v| v.as_str()) {
                        if let Ok(thing) = id_str.parse::<Thing>() {
                            return Ok(Some(thing));
                        }
                    }
                    if let Some(id_obj) = first.get("id").and_then(|v| v.as_object()) {
                        if let (Some(tb), Some(id_raw)) = (id_obj.get("tb"), id_obj.get("id")) {
                            if let (Some(tb_s), Some(id_s)) = (tb.as_str(), id_raw.as_str()) {
                                let composed = format!("{tb_s}:{id_s}");
                                if let Ok(thing) = composed.parse::<Thing>() {
                                    return Ok(Some(thing));
                                }
                            }
                        }
                    }
                    println!("[hybrid-create] could not parse id from first row: {first:?}");
                } else {
                    println!("[hybrid-create] no first row returned");
                }
                
                tokio::time::sleep(std::time::Duration::from_millis(40)).await;
                let sel = store.canonical_db().query("SELECT id FROM relationship_node WHERE subject_ref = $s AND predicate = $p AND object_ref = $o ORDER BY created_at DESC LIMIT 1;")
                    .bind(("s", subject.clone()))
                    .bind(("p", predicate.clone()))
                    .bind(("o", object.clone()));
                if let Ok(mut sel_res) = sel.await {
                    if let Ok::<Vec<serde_json::Value>, _>(sel_rows) = sel_res.take(0) {
                        println!("[hybrid-create] fallback lookup rows={sel_rows:?}");
                        if let Some(first) = sel_rows.first() {
                            if let Some(id_val) = first.get("id").and_then(|v| v.as_str()) {
                                if let Ok(thing) = id_val.parse::<Thing>() {
                                    return Ok(Some(thing));
                                }
                            }
                        }
                    }
                }
                if let Ok(mut broad) = store.canonical_db().query("SELECT id, predicate FROM relationship_node ORDER BY created_at DESC LIMIT 3;").await {
                if let Ok::<Vec<serde_json::Value>, _>(brows) = broad.take(0) { println!("[hybrid-create] diagnostic rows={brows:?}"); }
                }
                Ok(None)
            }
            Err(e) => Err(DatabaseError::Query(format!(
                "hybrid node create failed: {e}"
            ))),
        }
    }
    let node_id = match attempt_create(store, rel, subject, object).await {
        Ok(opt) => opt,
        Err(e) => {
            
            let schema_stmt = [
                "DEFINE TABLE relationship_node SCHEMAFULL PERMISSIONS FULL;",
                "DEFINE FIELD subject_ref ON TABLE relationship_node TYPE record<canonical_entity> | record<canonical_event>;",
                "DEFINE FIELD object_ref ON TABLE relationship_node TYPE record<canonical_entity> | record<canonical_event>;",
                "DEFINE FIELD predicate ON TABLE relationship_node TYPE string;",
                "DEFINE FIELD confidence ON TABLE relationship_node TYPE option<number>;",
                "DEFINE FIELD provenance ON TABLE relationship_node TYPE option<object>;",
                "DEFINE FIELD created_at ON TABLE relationship_node TYPE datetime VALUE time::now();",
                "DEFINE INDEX relationship_node_predicate_idx ON TABLE relationship_node COLUMNS predicate;",
                "DEFINE INDEX relationship_node_subject_idx ON TABLE relationship_node COLUMNS subject_ref;",
                "DEFINE INDEX relationship_node_object_idx ON TABLE relationship_node COLUMNS object_ref;",
            ].join("\n");
            if let Err(se) = store.canonical_db().query(schema_stmt.as_str()).await {
                println!("[hybrid-create] lazy schema define error: {se}");
            }
            match attempt_create(store, rel, subject, object).await {
                Ok(opt2) => opt2,
                Err(_e2) => {
                    println!("[hybrid-create] node create failed after schema attempt: {e}");
                    None
                }
            }
        }
    };
    if let Some(nid) = node_id {
        crate::relationships::metrics::record_success();
        Ok(HybridRelationshipResult {
            fact_id: Some(fact_id),
            node_id: Some(nid),
            degraded: false,
        })
    } else {
        crate::relationships::metrics::record_fallback();
        Ok(HybridRelationshipResult {
            fact_id: Some(fact_id),
            node_id: None,
            degraded: true,
        })
    }
}
