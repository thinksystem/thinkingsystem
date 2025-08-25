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



#[cfg(feature = "nlu_builders")]
use std::time::Instant;
#[cfg(feature = "nlu_builders")]
use stele::{
    builders::{IngestBatchBuilder, NodeBuilder},
    StructuredStore,
};

#[cfg(feature = "nlu_builders")]
#[tokio::test]
async fn builder_performance_smoke() {
    std::env::set_var("STELE_CANON_NS", "perf_smoke_ns");
    std::env::set_var("STELE_CANON_DB", "perf_smoke_db");
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

    let n = 10;
    
    let t_direct_start = Instant::now();
    for i in 0..n {
        let _ = store
            .upsert_canonical_entity("person", &format!("PerfDirect {i}"), None, None)
            .await
            .expect("direct upsert");
    }
    let direct_ms = t_direct_start.elapsed().as_millis();

    
    let mut batch = IngestBatchBuilder::new();
    for i in 0..n {
        batch = batch.add_node(
            NodeBuilder::new()
                .temp_id(format!("t{i}"))
                .name(format!("PerfBuilder {i}"))
                .entity_type("person"),
        );
    }
    let t_builder_start = Instant::now();
    let res = batch.execute(&store).await.expect("batch execute");
    assert_eq!(res.node_map.len(), n as usize, "expected node count");
    let builder_ms = t_builder_start.elapsed().as_millis();

    let ratio = builder_ms as f64 / (direct_ms.max(1)) as f64;
    eprintln!("PERF_SMOKE direct_ms={direct_ms} builder_ms={builder_ms} ratio={ratio:.2}");
    assert!(
        ratio <= 1.10,
        "builder overhead exceeded 10% (ratio={ratio:.2})"
    );
}


#[cfg(not(feature = "nlu_builders"))]
#[tokio::test]
async fn builder_performance_smoke() {
    eprintln!("builder_performance_smoke skipped (nlu_builders feature disabled)");
}
