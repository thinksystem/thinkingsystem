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



#[cfg(feature = "hybrid_relationships")]
#[tokio::test]
async fn hybrid_relationship_visibility_diag() {
    use stele::StructuredStore;


    std::env::set_var("STELE_CANON_NS", "hybrid_diag_ns");
    std::env::set_var("STELE_CANON_DB", "hybrid_diag_db");
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
        .expect("connect diag");
    let store = StructuredStore::new_with_clients(canon.clone(), canon.clone(), true);


    let a = store
        .upsert_canonical_entity("person", "Diag A", None, None)
        .await
        .expect("entity A");
    let b = store
        .upsert_canonical_entity("person", "Diag B", None, None)
        .await
        .expect("entity B");


    let create = store.canonical_db().query(
        "CREATE relationship_node SET subject_ref = $s, object_ref = $o, predicate = 'diag_rel', confidence = 0.5 RETURN id;"
    )
        .bind(("s", a.clone()))
        .bind(("o", b.clone()));
    let mut create_res = create.await.expect("create rel node");
    let created_rows: Vec<serde_json::Value> = create_res.take(0).unwrap_or_default();
    println!("[diag] create rows: {created_rows:?}");


    let sel = store.canonical_db().query(
        "SELECT id FROM relationship_node WHERE subject_ref = $s AND predicate = 'diag_rel' AND object_ref = $o LIMIT 5;"
    )
        .bind(("s", a.clone()))
        .bind(("o", b.clone()));
    let mut sel_res = sel.await.expect("select triple");
    let sel_rows: Vec<serde_json::Value> = sel_res.take(0).unwrap_or_default();
    println!("[diag] triple select rows: {sel_rows:?}");


    let mut count_q = store
        .canonical_db()
        .query("SELECT count() FROM relationship_node;")
        .await
        .expect("count");
    let count_rows: Vec<serde_json::Value> = count_q.take(0).unwrap_or_default();
    println!("[diag] count rows: {count_rows:?}");

    let mut sample_q = store
        .canonical_db()
        .query("SELECT id, predicate, subject_ref, object_ref FROM relationship_node LIMIT 5;")
        .await
        .expect("sample");
    let sample_rows: Vec<serde_json::Value> = sample_q.take(0).unwrap_or_default();
    println!("[diag] sample rows: {sample_rows:?}");



    if created_rows.is_empty() && sel_rows.is_empty() && sample_rows.is_empty() {
        eprintln!("[diag] ANOMALY: relationship_node creation invisible (A3 candidate)");
    } else if let Some(first) = sample_rows.first() {
        if let Some(pred) = first.get("predicate").and_then(|v| v.as_str()) {
            assert_eq!(pred, "diag_rel", "unexpected predicate in sample");
        }
    }
}
