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



#[tokio::test]
async fn builder_entity_relationship_parity() {
    
    std::env::set_var("SURREALDB_NS", "dyn_ns_parity");
    std::env::set_var("SURREALDB_DB", "dyn_db_parity");
    std::env::set_var(
        "STELE_CANON_URL",
        std::env::var("SURREALDB_URL").unwrap_or_else(|_| "ws://127.0.0.1:8000".into()),
    );
    std::env::set_var(
        "STELE_CANON_USER",
        std::env::var("SURREALDB_USER").unwrap_or_else(|_| "root".into()),
    );
    std::env::set_var(
        "STELE_CANON_PASS",
        std::env::var("SURREALDB_PASS").unwrap_or_else(|_| "root".into()),
    );
    std::env::set_var("STELE_CANON_NS", "canon_ns_parity");
    std::env::set_var("STELE_CANON_DB", "canon_db_parity");

    #[cfg(not(feature = "nlu_builders"))]
    { eprintln!("nlu_builders feature not enabled; skipping parity test"); return; }

    #[cfg(feature = "nlu_builders")]
    {

    use stele::StructuredStore;
    let canon = StructuredStore::connect_canonical_from_env()
        .await
        .expect("connect canonical");
    let store = StructuredStore::new_with_clients(canon.clone(), canon.clone(), true);

    
    let _ = canon
        .query("DELETE canonical_relationship_fact WHERE predicate = 'related_to';")
        .await;
    let _ = canon
        .query("DELETE canonical_entity WHERE name IN ['Alpha','Beta'];")
        .await;

    
    let a_legacy = store
        .upsert_canonical_entity("Thing", "Alpha", Some("Thing:Alpha"), None)
        .await
        .expect("legacy alpha");
    let b_legacy = store
        .upsert_canonical_entity("Thing", "Beta", Some("Thing:Beta"), None)
        .await
        .expect("legacy beta");
    eprintln!("LEGACY_IDS a={a_legacy:?} b={b_legacy:?}");

    
    use stele::builders::{IngestBatchBuilder, NodeBuilder, RelationshipBuilder};
    let batch = IngestBatchBuilder::new()
        .add_node(
            NodeBuilder::new()
                .temp_id("na")
                .entity_type("Thing")
                .name("Alpha"),
        )
        .add_node(
            NodeBuilder::new()
                .temp_id("nb")
                .entity_type("Thing")
                .name("Beta"),
        )
        .add_relationship(
            RelationshipBuilder::new()
                .source_temp("na")
                .target_temp("nb")
                .predicate("related_to"),
        );
    let ingest_res = batch.execute(&store).await.expect("builder ingest");
    eprintln!(
        "BUILDER_INGEST_RESULT nodes={} rels={}",
        ingest_res.node_map.len(),
        ingest_res.relationships.len()
    );
    
    for (i, rid) in ingest_res.relationships.iter().enumerate() {
        eprintln!("REL_ID[{i}]={rid:?}");
        
        if let Ok(mut by_id) = canon
            .query("SELECT predicate, string(subject_ref) AS subject_ref, string(object_ref) AS object_ref, created_at FROM $id LIMIT 1;")
            .bind(("id", rid.clone()))
            .await
        {
            if let Ok::<Vec<serde_json::Value>, _>(rows) = by_id.take(0) {
                eprintln!(
                    "REL_DIRECT_FETCH[{i}]={}",
                    serde_json::to_string(&rows).unwrap_or_default()
                );
            } else {
                eprintln!("REL_DIRECT_FETCH[{i}]=DECODE_EMPTY");
            }
        } else {
            eprintln!("REL_DIRECT_FETCH[{i}]=QUERY_ERR");
        }
    }
    
    if let Ok(mut rel_all) = canon
        .query("SELECT predicate, string(subject_ref) AS subject_ref, string(object_ref) AS object_ref FROM canonical_relationship_fact LIMIT 50;")
        .await
    {
        if let Ok::<Vec<serde_json::Value>, _>(rows) = rel_all.take(0) {
            eprintln!(
                "REL_RAW_ROWS={}",
                serde_json::to_string(&rows).unwrap_or_default()
            );
        } else {
            eprintln!("REL_RAW_ROWS=DECODE_EMPTY");
        }
    } else {
        eprintln!("REL_RAW_ROWS=QUERY_ERR");
    }
    if let Ok(mut rel_cnt) = canon
        .query("SELECT count() AS c FROM canonical_relationship_fact;")
        .await
    {
        if let Ok::<Vec<serde_json::Value>, _>(rows) = rel_cnt.take(0) {
            eprintln!(
                "REL_COUNT_ROWS={}",
                serde_json::to_string(&rows).unwrap_or_default()
            );
        }
    }

    
    #[allow(dead_code)]
    fn flatten_json(value: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
        match value {
            serde_json::Value::Array(a) => {
                if a.len() == 1 {
                    flatten_json(&a[0], out);
                } else {
                    for v in a {
                        flatten_json(v, out);
                    }
                }
            }
            serde_json::Value::Object(_) => out.push(value.clone()),
            _ => {}
        }
    }

    
    let mut entities: Vec<serde_json::Value> = Vec::new();
    for key in ["Thing:Alpha", "Thing:Beta"] {
        if let Ok(mut q) = canon
            .query("SELECT name, canonical_key FROM canonical_entity WHERE canonical_key = $k LIMIT 1;")
            .bind(("k", key))
            .await
        {
            if let Ok::<Vec<serde_json::Value>, _>(mut rows) = q.take(0) {
                if rows.is_empty() {
                    eprintln!("MISS_KEY={key}");
                } else {
                    eprintln!("HIT_KEY={key}");
                    entities.append(&mut rows);
                }
            } else {
                eprintln!("DECODE_FAIL_KEY={key}");
            }
        } else {
            eprintln!("QUERY_ERR_KEY={key}");
        }
    }
    if entities.is_empty() {
        
        if let Ok(mut ns_state) = canon.query("RETURN {ns: ns(), db: db()};").await {
            if let Ok::<Vec<serde_json::Value>, _>(v) = ns_state.take(0) {
                eprintln!("CANON_NS_DB_STATE={}", serde_json::to_string(&v).unwrap_or_default());
            }
        }
        if let Ok(mut info_tbl) = canon.query("INFO FOR TABLE canonical_entity;").await {
            let raw: Option<surrealdb::Value> = info_tbl.take(0).ok().flatten();
            if let Some(r) = raw {
                eprintln!("INFO_TABLE_ENTITY_RAW={r:?}");
            }
        }
        if let Ok(all_via_select) = canon
            .select::<Vec<serde_json::Value>>("canonical_entity")
            .await
        {
            eprintln!(
                "ALT_SELECT_API_ROWS={}",
                serde_json::to_string(&all_via_select).unwrap_or_default()
            );
        }
        
        if let Ok(fallback_rows) = store.test_fetch_canonical_entities_flat().await {
            eprintln!(
                "FALLBACK_HELPER_ROWS={}",
                serde_json::to_string(&fallback_rows).unwrap_or_default()
            );
        } else {
            eprintln!("FALLBACK_HELPER_ROWS=ERR");
        }
        
        if let Ok(mut wildcard) = canon
            .query("SELECT * FROM canonical_entity FETCH * LIMIT 20;")
            .await
        {
            if let Ok::<Vec<serde_json::Value>, _>(rows) = wildcard.take(0) {
                eprintln!(
                    "WILDCARD_FETCH_ROWS={}",
                    serde_json::to_string(&rows).unwrap_or_default()
                );
            }
        }
        
        if let Ok(mut qcnt) = canon
            .query("SELECT count() AS c FROM canonical_entity;")
            .await
        {
            if let Ok::<Vec<serde_json::Value>, _>(crow) = qcnt.take(0) {
                eprintln!(
                    "ALT_COUNT_ROWS={}",
                    serde_json::to_string(&crow).unwrap_or_default()
                );
            }
        }
        
        if let Ok(fresh) = StructuredStore::connect_canonical_from_env().await {
            if let Ok(mut fresh_sel) = fresh
                .query("SELECT id, name, canonical_key FROM canonical_entity LIMIT 20;")
                .await
            {
                if let Ok::<Vec<serde_json::Value>, _>(frows) = fresh_sel.take(0) {
                    eprintln!(
                        "FRESH_CONN_ROWS={}",
                        serde_json::to_string(&frows).unwrap_or_default()
                    );
                }
            } else {
                eprintln!("FRESH_CONN_SELECT_ERR");
            }
            if let Ok(mut fresh_cnt) = fresh
                .query("SELECT count() AS c FROM canonical_entity;")
                .await
            {
                if let Ok::<Vec<serde_json::Value>, _>(fc) = fresh_cnt.take(0) {
                    eprintln!(
                        "FRESH_CONN_COUNT={}",
                        serde_json::to_string(&fc).unwrap_or_default()
                    );
                }
            }
        } else {
            eprintln!("FRESH_CONN_CREATE_ERR");
        }
    }
    eprintln!(
        "ENTITIES_FINAL={}",
        serde_json::to_string(&entities).unwrap_or_default()
    );
    assert_eq!(entities.len(), 2, "expected exactly two canonical entities");

    
    let mut rel_attempts = 0u8;
    let rel_rows: Vec<serde_json::Value> = loop {
        rel_attempts += 1;
        if let Ok(mut q) = canon
            .query("SELECT predicate FROM canonical_relationship_fact WHERE predicate = 'related_to' LIMIT 10;")
            .await
        {
            let rows: Vec<serde_json::Value> = q.take(0).unwrap_or_default();
            if !rows.is_empty() { break rows; }
        }
        
        if let Ok(mut probe) = canon
            .query("SELECT predicate FROM canonical_relationship_fact LIMIT 5;")
            .await
        {
            if let Ok::<Vec<serde_json::Value>, _>(prows) = probe.take(0) {
                eprintln!(
                    "REL_PROBE_PRED_ROWS_ATTEMPT{rel_attempts}={}",
                    serde_json::to_string(&prows).unwrap_or_default()
                );
            }
        }
        
        match canon
                .query("SELECT subject_ref FROM canonical_relationship_fact WHERE predicate = 'related_to' LIMIT 1;")
                .await
            {
                Ok(mut sq) => {
                    let srows: Vec<serde_json::Value> = sq.take(0).unwrap_or_default();
                    if !srows.is_empty() {
                        eprintln!(
                            "REL_SUBJECT_ONLY_ROWS_ATTEMPT{rel_attempts}={}",
                            serde_json::to_string(&srows).unwrap_or_default()
                        );
                    } else {
                        eprintln!("REL_SUBJECT_ONLY_EMPTY_ATTEMPT{rel_attempts}");
                    }
                }
                Err(e) => eprintln!("REL_SUBJECT_ONLY_ERR_ATTEMPT{rel_attempts}={e}"),
            }
        match canon
                .query("SELECT object_ref FROM canonical_relationship_fact WHERE predicate = 'related_to' LIMIT 1;")
                .await
            {
                Ok(mut oq) => {
                    let orows: Vec<serde_json::Value> = oq.take(0).unwrap_or_default();
                    if !orows.is_empty() {
                        eprintln!(
                            "REL_OBJECT_ONLY_ROWS_ATTEMPT{rel_attempts}={}",
                            serde_json::to_string(&orows).unwrap_or_default()
                        );
                    } else {
                        eprintln!("REL_OBJECT_ONLY_EMPTY_ATTEMPT{rel_attempts}");
                    }
                }
                Err(e) => eprintln!("REL_OBJECT_ONLY_ERR_ATTEMPT{rel_attempts}={e}"),
            }
        if rel_attempts > 5 {
            break Vec::new();
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };
    eprintln!(
        "RELATIONSHIP_FACTS_PRED_ONLY={}",
        serde_json::to_string(&rel_rows).unwrap_or_default()
    );
    assert!(
        !rel_rows.is_empty(),
        "expected related_to fact (predicate-only fetch)"
    );
    
    if let Ok(flat) = store.test_fetch_relationship_facts_flat("related_to").await {
        if !flat.is_empty() {
            eprintln!(
                "REL_FACTS_FLAT_HELPER={}",
                serde_json::to_string(&flat).unwrap_or_default()
            );
        } else {
            eprintln!("REL_FACTS_FLAT_HELPER=EMPTY");
        }
    } else {
        eprintln!("REL_FACTS_FLAT_HELPER=ERR");
    }
    }
}
