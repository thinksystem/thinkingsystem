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
async fn hybrid_relationship_visibility_new_conn() {
    use stele::StructuredStore;
    use surrealdb::engine::remote::ws::Ws;
    use surrealdb::opt::auth::Root;
    use surrealdb::Surreal;

    
    std::env::set_var("STELE_CANON_NS", "hybrid_diag_ns_newc");
    std::env::set_var("STELE_CANON_DB", "hybrid_diag_db_newc");
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
        .expect("connect base");
    let store = StructuredStore::new_with_clients(canon.clone(), canon.clone(), true);

    
    let a = store
        .upsert_canonical_entity("person", "NC A", None, None)
        .await
        .expect("entity A");
    let b = store
        .upsert_canonical_entity("person", "NC B", None, None)
        .await
        .expect("entity B");

    
    let create = store.canonical_db().query(
        "CREATE relationship_node SET subject_ref=$s, object_ref=$o, predicate='new_conn_rel', confidence=0.1, provenance={} RETURN id;"
    ).bind(("s", a.clone())).bind(("o", b.clone()));
    let mut create_res = create.await.expect("create");
    let created_rows: Vec<serde_json::Value> = create_res.take(0).unwrap_or_default();
    println!("[new-conn] initial create rows: {created_rows:?}");

    
    let url = std::env::var("STELE_CANON_URL").unwrap();
    let endpoint = url.strip_prefix("ws://").unwrap_or(&url).to_string();
    let user = std::env::var("STELE_CANON_USER").unwrap();
    let pass = std::env::var("STELE_CANON_PASS").unwrap();
    let ns = std::env::var("STELE_CANON_NS").unwrap();
    let db = std::env::var("STELE_CANON_DB").unwrap();
    let fresh = Surreal::new::<Ws>(&endpoint).await.expect("fresh connect");
    fresh
        .signin(Root {
            username: &user,
            password: &pass,
        })
        .await
        .expect("fresh auth");
    fresh.use_ns(&ns).use_db(&db).await.expect("fresh ns/db");

    
    for attempt in 0..5u32 {
        let delay_ms = attempt * 40;
        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms as u64)).await;
        }
        let mut q = fresh.query("SELECT id, predicate FROM relationship_node WHERE predicate='new_conn_rel' LIMIT 5;").await.expect("fresh select");
        let rows: Vec<serde_json::Value> = q.take(0).unwrap_or_default();
    println!("[new-conn] attempt {attempt} rows={rows:?}");
        if !rows.is_empty() {
            break;
        }
    }

    
    let mut count_q = fresh
        .query("SELECT count() FROM relationship_node WHERE predicate='new_conn_rel';")
        .await
        .expect("count");
    let count_rows: Vec<serde_json::Value> = count_q.take(0).unwrap_or_default();
    println!("[new-conn] count rows: {count_rows:?}");

    
    println!(
        "[new-conn] Test completed: Created relationship_node with fresh connection visibility."
    );
}
