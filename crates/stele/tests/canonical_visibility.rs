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
async fn canonical_entity_minimal_visibility() {
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

    std::env::set_var("SURREALDB_NS", "dyn_ns_min_vis");
    std::env::set_var("SURREALDB_DB", "dyn_db_min_vis");
    std::env::set_var("STELE_CANON_NS", "canon_ns_min_vis");
    std::env::set_var("STELE_CANON_DB", "canon_db_min_vis");

    use stele::StructuredStore;
    let canon = StructuredStore::connect_canonical_from_env()
        .await
        .expect("connect canonical");

    if let Ok(mut ver) = canon.query("RETURN version();").await {
        let v: Vec<serde_json::Value> = ver.take(0).unwrap_or_default();
        eprintln!(
            "SERVER_VERSION={}",
            serde_json::to_string(&v).unwrap_or_default()
        );
    }
    if let Ok(mut info_db) = canon.query("INFO FOR DB;").await {
        let v: Vec<serde_json::Value> = info_db.take(0).unwrap_or_default();
        eprintln!("INFO_FOR_DB_ROWS={} (truncated)", v.len());
    }

    let mut create_query_resp = canon
        .query("CREATE canonical_entity SET entity_type = 'TestType', name = 'VisOne', canonical_key = 'TestType:VisOne', extra = {} RETURN AFTER;")
        .await
        .expect("create row (query)");

    eprintln!("CREATE_QUERY_RAW_DEBUG={create_query_resp:?}");
    let created_rows_query: Vec<serde_json::Value> = create_query_resp.take(0).unwrap_or_default();
    eprintln!(
        "CREATED_ROWS_QUERY={}",
        serde_json::to_string(&created_rows_query).unwrap_or_default()
    );

    use surrealdb::sql::Thing;
    #[derive(serde::Deserialize, Debug)]
    #[allow(dead_code)]
    struct CanonEnt {
        id: Thing,
        entity_type: String,
        name: String,
        canonical_key: String,
    }
    let created_via_api: Result<Option<CanonEnt>, _> = canon
        .create("canonical_entity")
        .content(serde_json::json!({
            "entity_type":"TestType",
            "name":"VisOne2",
            "canonical_key":"TestType:VisOne2",
            "extra":{}
        }))
        .await;
    match created_via_api {
        Ok(Some(r)) => eprintln!("CREATED_VIA_API={r:?}"),
        Ok(None) => eprintln!("CREATED_VIA_API=None"),
        Err(e) => eprintln!("CREATED_VIA_API_ERR={e}"),
    }

    if let Ok(mut probe_key) = canon
        .query("SELECT id, name, canonical_key FROM canonical_entity WHERE canonical_key = 'TestType:VisOne';")
        .await
    {
        let rows: Vec<serde_json::Value> = probe_key.take(0).unwrap_or_default();
        eprintln!("PROBE_BY_KEY={}", serde_json::to_string(&rows).unwrap_or_default());
    eprintln!("PROBE_BY_KEY_RAW_DEBUG={probe_key:?}");
    }
    if let Ok(mut probe_key2) = canon
        .query("SELECT id, name, canonical_key FROM canonical_entity WHERE canonical_key = 'TestType:VisOne2';")
        .await
    {
        let rows: Vec<serde_json::Value> = probe_key2.take(0).unwrap_or_default();
        eprintln!("PROBE_BY_KEY2={}", serde_json::to_string(&rows).unwrap_or_default());
    eprintln!("PROBE_BY_KEY2_RAW_DEBUG={probe_key2:?}");
    }

    if let Ok(mut sample) = canon
        .query("SELECT id, name, canonical_key FROM canonical_entity LIMIT 10;")
        .await
    {
        let rows: Vec<serde_json::Value> = sample.take(0).unwrap_or_default();
        eprintln!(
            "TABLE_SAMPLE={}",
            serde_json::to_string(&rows).unwrap_or_default()
        );
        eprintln!("TABLE_SAMPLE_RAW_DEBUG={sample:?}");
    }

    if let Ok(mut cnt) = canon
        .query("SELECT count() AS c FROM canonical_entity;")
        .await
    {
        let rows: Vec<serde_json::Value> = cnt.take(0).unwrap_or_default();
        eprintln!(
            "COUNT_ROWS={}",
            serde_json::to_string(&rows).unwrap_or_default()
        );
        eprintln!("COUNT_ROWS_RAW_DEBUG={cnt:?}");
    }

    let _ = canon.query("DEFINE TABLE scratch_test SCHEMALESS; CREATE scratch_test SET note='hello' RETURN AFTER;").await;
    if let Ok(mut scratch_all) = canon
        .query("SELECT * FROM scratch_test; SELECT count() AS c FROM scratch_test;")
        .await
    {
        let rows_a: Vec<serde_json::Value> = scratch_all.take(0).unwrap_or_default();
        let rows_b: Vec<serde_json::Value> = scratch_all.take(0).unwrap_or_default();
        eprintln!(
            "SCRATCH_ROWS={}",
            serde_json::to_string(&rows_a).unwrap_or_default()
        );
        eprintln!(
            "SCRATCH_COUNT={}",
            serde_json::to_string(&rows_b).unwrap_or_default()
        );
    }
}

#[tokio::test]
async fn canonical_entity_local_mem_visibility() {
    use surrealdb::engine::local::Mem;
    use surrealdb::Surreal;
    let db = Surreal::new::<Mem>(()).await.expect("mem init");

    db.use_ns("mem_ns_vis")
        .use_db("mem_db_vis")
        .await
        .expect("set ns/db");

    let schema: &str = include_str!("../src/database/config/canonical_schema.sql");
    let apply = db.query(schema).await.expect("apply schema");
    eprintln!("MEM_SCHEMA_APPLY_DEBUG={apply:?}");
    
    let _ = db
        .query(
            "DEFINE TABLE canonical_entity PERMISSIONS FOR select, create, update, delete WHERE true;",
        )
        .await;

    let mut q = db
        .query(
            "CREATE canonical_entity SET entity_type='MemType', name='MemOne', canonical_key='MemType:MemOne', extra={} RETURN AFTER; \
             SELECT id, name, canonical_key FROM canonical_entity WHERE canonical_key='MemType:MemOne'; \
             SELECT id, name, canonical_key FROM canonical_entity; \
             SELECT count() AS c FROM canonical_entity;",
        )
        .await
        .expect("create+probe mem rows");
    eprintln!("MEM_CREATE_RAW_DEBUG={q:?}");
    
    fn flatten_serialized(v: serde_json::Value) -> Vec<serde_json::Value> {
        match v {
            serde_json::Value::Array(items) => {
                let mut out = Vec::new();
                for item in items {
                    match item {
                        serde_json::Value::Array(inner) => out.extend(inner),
                        other => out.push(other),
                    }
                }
                out
            }
            other => vec![other],
        }
    }
    
    let created_rows_val: surrealdb::Value = q.take(0).unwrap_or_default();
    let created_rows =
        flatten_serialized(serde_json::to_value(created_rows_val).unwrap_or_default());
    eprintln!(
        "MEM_CREATED_ROWS={}",
        serde_json::to_string(&created_rows).unwrap_or_default()
    );
    
    let vis_val: surrealdb::Value = q.take(1).unwrap_or_default();
    let vis = flatten_serialized(serde_json::to_value(vis_val).unwrap_or_default());
    eprintln!(
        "MEM_PROBE_BY_KEY={}",
        serde_json::to_string(&vis).unwrap_or_default()
    );
    let vis_len = vis.len();
    
    let all_rows_val: surrealdb::Value = q.take(2).unwrap_or_default();
    let all_rows = flatten_serialized(serde_json::to_value(all_rows_val).unwrap_or_default());
    eprintln!(
        "MEM_TABLE_DUMP={}",
        serde_json::to_string(&all_rows).unwrap_or_default()
    );
    
    let cnt_val: surrealdb::Value = q.take(3).unwrap_or_default();
    let cnt = flatten_serialized(serde_json::to_value(cnt_val).unwrap_or_default());
    eprintln!(
        "MEM_COUNT={}",
        serde_json::to_string(&cnt).unwrap_or_default()
    );

    if let Ok(mut ctrl) = db
        .query("DEFINE TABLE mem_ctrl SCHEMALESS; CREATE mem_ctrl SET note='x' RETURN AFTER; SELECT * FROM mem_ctrl;")
        .await
    {
        
        let c1: Vec<serde_json::Value> = ctrl.take(0).unwrap_or_default();
        
        let c2: Vec<serde_json::Value> = match ctrl.take::<Vec<Vec<serde_json::Value>>>(1) {
            Ok(nested) => nested.into_iter().flatten().collect(),
            Err(_) => ctrl.take::<Vec<serde_json::Value>>(1).unwrap_or_default(),
        };
        
        let c3: Vec<serde_json::Value> = ctrl.take(2).unwrap_or_default();
        eprintln!(
            "MEM_CTRL_DEFINE_ROWS={}",
            serde_json::to_string(&c1).unwrap_or_default()
        );
        eprintln!(
            "MEM_CTRL_CREATED_ROWS={}",
            serde_json::to_string(&c2).unwrap_or_default()
        );
        eprintln!(
            "MEM_CTRL_SELECT_ROWS={}",
            serde_json::to_string(&c3).unwrap_or_default()
        );
    }
    assert!(
        vis_len == 1,
        "expected to be able to SELECT created row in mem engine"
    );
}
