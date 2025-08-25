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



#[cfg(all(feature = "nlu_builders", feature = "api_v2"))]
#[tokio::test]
async fn batch_api_v2_payload() {
    use stele::builders::{IngestBatchBuilder, NodeBuilder, RelationshipBuilder};
    use stele::StructuredStore;

    std::env::set_var("STELE_CANON_NS", "api_v2_ns");
    std::env::set_var("STELE_CANON_DB", "api_v2_db");
    std::env::set_var("STELE_CANON_URL", std::env::var("SURREALDB_URL").unwrap_or_else(|_| "ws://127.0.0.1:8000".into()));
    std::env::set_var("STELE_CANON_USER", std::env::var("SURREALDB_USER").unwrap_or_else(|_| "root".into()));
    std::env::set_var("STELE_CANON_PASS", std::env::var("SURREALDB_PASS").unwrap_or_else(|_| "root".into()));

    let canon = StructuredStore::connect_canonical_from_env().await.expect("connect");
    let store = StructuredStore::new_with_clients(canon.clone(), canon.clone(), true);

    let batch = IngestBatchBuilder::new()
        .add_node(NodeBuilder::new().temp_id("x").name("Node X").entity_type("thing"))
        .add_node(NodeBuilder::new().temp_id("y").name("Node Y").entity_type("thing"))
        .add_relationship(RelationshipBuilder::new().source_temp("x").target_temp("y").predicate("links_to"));

    let res = batch.execute(&store).await.expect("execute batch");
    let payload = res.api_v2.expect("api_v2 payload missing");
    assert_eq!(payload.get("api_version").and_then(|v| v.as_str()), Some("v2"));
    let nodes = payload.get("nodes").and_then(|v| v.as_array()).expect("nodes array");
    assert_eq!(nodes.len(), 2);
    let rels = payload.get("relationships").and_then(|v| v.as_array()).expect("rels array");
    assert_eq!(rels.len(), 1);
}
