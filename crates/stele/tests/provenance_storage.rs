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



use stele::database::structured_store::StructuredStore;

#[tokio::test]
#[ignore]
async fn provenance_binding_vs_inline() {
    
    std::env::set_var("STELE_CANON_URL", std::env::var("SURREALDB_URL").unwrap_or_else(|_| "ws://127.0.0.1:8000".into()));
    std::env::set_var("STELE_CANON_USER", std::env::var("SURREALDB_USER").unwrap_or_else(|_| "root".into()));
    std::env::set_var("STELE_CANON_PASS", std::env::var("SURREALDB_PASS").unwrap_or_else(|_| "root".into()));
    std::env::set_var("STELE_CANON_NS", "canon_ns_test");
    std::env::set_var("STELE_CANON_DB", "canon_db_test");

    let canon = StructuredStore::connect_canonical_from_env().await.expect("canon connect");

    
    let _ = canon.query("DELETE canonical_event WHERE title IN ['ProvTestInline','ProvTestBind'];").await;

    
    let mut inline_res = canon
        .query("CREATE canonical_event SET title='ProvTestInline', description='ProvTestInline', provenance = {source:'inline', participants:['x','y']} RETURN AFTER;")
        .await
        .expect("inline create");
    let inline_rows: Vec<serde_json::Value> = inline_res.take(0).unwrap_or_default();
    println!("INLINE_CREATE={}", serde_json::to_string(&inline_rows).unwrap_or_default());

    
    let prov_bound = serde_json::json!({"source":"bound","participants":["x","y"],"reasoning_hash":"hash:test"});
    let mut bound_res = canon
        .query("CREATE canonical_event SET title = 'ProvTestBind', description='ProvTestBind', provenance = <object>$p RETURN AFTER;")
        .bind(("p", prov_bound.clone()))
        .await
        .expect("bound create");
    let bound_rows: Vec<serde_json::Value> = bound_res.take(0).unwrap_or_default();
    println!("BOUND_CREATE={}", serde_json::to_string(&bound_rows).unwrap_or_default());

    
    let mut fetch = canon.query("SELECT title, provenance FROM canonical_event WHERE title IN ['ProvTestInline','ProvTestBind'] ORDER BY title;").await.expect("fetch rows");
    let fetched: Vec<serde_json::Value> = fetch.take(0).unwrap_or_default();
    println!("FETCHED_ROWS={}", serde_json::to_string(&fetched).unwrap_or_default());

    
    let inline = fetched.iter().find(|r| r.get("title").and_then(|v| v.as_str()) == Some("ProvTestInline")).expect("inline row");
    let bound = fetched.iter().find(|r| r.get("title").and_then(|v| v.as_str()) == Some("ProvTestBind")).expect("bound row");
    
    if inline.get("provenance") == Some(&serde_json::json!({})) || bound.get("provenance") == Some(&serde_json::json!({})) {
        println!("WARNING: provenance empty anomaly reproduced (inline_empty={}, bound_empty={})", inline.get("provenance") == Some(&serde_json::json!({})), bound.get("provenance") == Some(&serde_json::json!({})));
    }
}
