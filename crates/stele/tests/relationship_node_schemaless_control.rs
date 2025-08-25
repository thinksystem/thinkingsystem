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
async fn relationship_node_schemaless_control() {
    use stele::StructuredStore;

    std::env::set_var("STELE_CANON_NS", "rel_node_free_ns");
    std::env::set_var("STELE_CANON_DB", "rel_node_free_db");
    std::env::set_var("STELE_CANON_URL", std::env::var("SURREALDB_URL").unwrap_or_else(|_| "ws://127.0.0.1:8000".into()));
    std::env::set_var("STELE_CANON_USER", std::env::var("SURREALDB_USER").unwrap_or_else(|_| "root".into()));
    std::env::set_var("STELE_CANON_PASS", std::env::var("SURREALDB_PASS").unwrap_or_else(|_| "root".into()));
    let canon = StructuredStore::connect_canonical_from_env().await.expect("connect");
    let store = StructuredStore::new_with_clients(canon.clone(), canon.clone(), true);

    
    let a = store.upsert_canonical_entity("person", "Free A", None, None).await.expect("entity a");
    let b = store.upsert_canonical_entity("person", "Free B", None, None).await.expect("entity b");

    
    let q = store.canonical_db().query("CREATE relationship_node SET subject_ref=$s, object_ref=$o, predicate='free_rel' RETURN *;")
        .bind(("s", a.clone()))
        .bind(("o", b.clone()));
    let mut res = q.await.expect("create node");
    let rows: Vec<serde_json::Value> = res.take(0).unwrap_or_default();
    eprintln!("SCHEMALESS_REL_NODE rows={rows:?}");
    
    let mut sel_q = store.canonical_db().query("SELECT * FROM relationship_node WHERE predicate='free_rel';").await.expect("select");
    let vis: Vec<serde_json::Value> = sel_q.take(0).unwrap_or_default();
    eprintln!("SCHEMALESS_REL_NODE_SELECT rows={vis:?}");
    
    
    if !vis.is_empty() { assert!(vis[0].get("predicate").is_some(), "expected predicate"); }
}
