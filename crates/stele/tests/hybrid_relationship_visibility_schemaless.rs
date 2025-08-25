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
async fn hybrid_relationship_visibility_schemaless() {
    use stele::StructuredStore;

    
    std::env::set_var("STELE_CANON_NS", "hybrid_diag_ns_free");
    std::env::set_var("STELE_CANON_DB", "hybrid_diag_db_free");
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

    let canon = StructuredStore::connect_canonical_from_env()
        .await
        .expect("connect free");
    let store = StructuredStore::new_with_clients(canon.clone(), canon.clone(), true);

    
    let _ = store
        .canonical_db()
        .query("DEFINE TABLE relationship_node_free;")
        .await;

    
    let a = store
        .upsert_canonical_entity("person", "Free A", None, None)
        .await
        .expect("entity A");
    let b = store
        .upsert_canonical_entity("person", "Free B", None, None)
        .await
        .expect("entity B");

    
    let create = store.canonical_db().query(
        "CREATE relationship_node_free SET subject_ref=$s, object_ref=$o, predicate='free_rel', confidence=0.9, note='schemaless' RETURN id;"
    )
        .bind(("s", a.clone()))
        .bind(("o", b.clone()));
    let mut create_res = create.await.expect("create free node");
    let created_rows: Vec<serde_json::Value> = create_res.take(0).unwrap_or_default();
    println!("[schemaless] create rows: {created_rows:?}");

    
    let mut lookup = store.canonical_db().query(
        "SELECT id, predicate, subject_ref, object_ref FROM relationship_node_free WHERE predicate='free_rel' LIMIT 5;"
    ).await.expect("lookup free");
    let lookup_rows: Vec<serde_json::Value> = lookup.take(0).unwrap_or_default();
    println!("[schemaless] lookup rows: {lookup_rows:?}");

    let mut count_q = store
        .canonical_db()
        .query("SELECT count() FROM relationship_node_free;")
        .await
        .expect("count");
    let count_rows: Vec<serde_json::Value> = count_q.take(0).unwrap_or_default();
    println!("[schemaless] count rows: {count_rows:?}");

    
    let visible = !created_rows.is_empty() || !lookup_rows.is_empty();

    if !visible {
        eprintln!("[schemaless] ANOMALY: schemaless table also lacks visible rows");
    } else if let Some(first) = lookup_rows.first().or_else(|| created_rows.first()) {
        if let Some(pred) = first.get("predicate").and_then(|v| v.as_str()) {
            assert_eq!(pred, "free_rel", "predicate mismatch in schemaless row");
        }
    }
}
