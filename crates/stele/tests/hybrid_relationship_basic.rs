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



#[cfg(all(feature = "nlu_builders", feature = "hybrid_relationships"))]
#[tokio::test]
async fn hybrid_relationship_basic() {
    use stele::builders::{
        IngestBatchBuilder, NodeBuilder, RelationshipBuilder, RelationshipStrategy,
    };
    use stele::StructuredStore;
    use surrealdb::sql::Thing;

    std::env::set_var("STELE_CANON_NS", "hybrid_rel_ns");
    std::env::set_var("STELE_CANON_DB", "hybrid_rel_db");
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
        .expect("connect");
    let store = StructuredStore::new_with_clients(canon.clone(), canon.clone(), true);

    let batch = IngestBatchBuilder::new()
        .add_node(
            NodeBuilder::new()
                .temp_id("a")
                .name("Hybrid A")
                .entity_type("person"),
        )
        .add_node(
            NodeBuilder::new()
                .temp_id("b")
                .name("Hybrid B")
                .entity_type("person"),
        )
        .add_relationship(
            RelationshipBuilder::new()
                .source_temp("a")
                .target_temp("b")
                .predicate("collaborates_with")
                .strategy(RelationshipStrategy::Hybrid),
        );

    let res = batch.execute(&store).await.expect("execute batch");
    assert_eq!(res.node_map.len(), 2, "expected two nodes");
    assert_eq!(
        res.relationships.len(),
        1,
        "expected one relationship result"
    );
    let rel = &res.relationships[0];
    eprintln!("HYBRID_REL_RESULT {rel}");
    assert!(rel.get("fact_id").is_some(), "fact id missing");
    
    if rel.get("node_id").is_none() {
        assert_eq!(
            rel.get("degraded").and_then(|v| v.as_bool()),
            Some(true),
            "expected degraded true when node_id absent"
        );
    
    let v = rel.get("virtual_node").and_then(|v| v.as_object()).expect("virtual_node missing when degraded");
    assert_eq!(v.get("predicate").and_then(|p| p.as_str()), rel.get("predicate").and_then(|p| p.as_str()), "virtual node predicate mismatch");
    assert_eq!(v.get("synthetic").and_then(|s| s.as_bool()), Some(true), "virtual node synthetic flag");
    }
    
    if let Some(fact_id) = rel.get("fact_id").and_then(|v| v.as_str()) {
        if let Ok(thing) = fact_id.parse::<Thing>() {
            let mut q = store
                .canonical_db()
                .query("SELECT predicate FROM $id")
                .bind(("id", thing))
                .await
                .expect("query fact");
            let rows: Vec<serde_json::Value> = q.take(0).unwrap_or_default();
            assert!(!rows.is_empty(), "predicate-only fetch should return row");
        }
    }
}
