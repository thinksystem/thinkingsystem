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



use stele::StructuredStore;

#[tokio::test]
async fn relationship_field_visibility_minimal() {
    std::env::set_var("STELE_CANON_NS", "rel_repro_ns");
    std::env::set_var("STELE_CANON_DB", "rel_repro_db");
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

    let canon = StructuredStore::connect_canonical_from_env().await.expect("connect");
    let store = StructuredStore::new_with_clients(canon.clone(), canon.clone(), true);

    
    let a = store.upsert_canonical_event("RelRepro A", None, None, None, None, None).await.expect("create a");
    let b = store.upsert_canonical_event("RelRepro B", None, None, None, None, None).await.expect("create b");

    let rel_id = store.create_relationship_fact(&a, "related_to", &b, Some(0.5), None).await.expect("create rel");

    
    let mut full = store.canonical_db().query("SELECT * FROM $id;").bind(("id", rel_id.clone())).await.expect("full q");
    let full_rows: Vec<serde_json::Value> = full.take(0).unwrap_or_default();

    
    let mut fields = store.canonical_db().query("SELECT subject_ref, predicate, object_ref, confidence FROM $id;").bind(("id", rel_id.clone())).await.expect("fields q");
    let field_rows: Vec<serde_json::Value> = fields.take(0).unwrap_or_default();

    
    let mut pred_only = store.canonical_db().query("SELECT predicate FROM $id;").bind(("id", rel_id.clone())).await.expect("pred q");
    let pred_rows: Vec<serde_json::Value> = pred_only.take(0).unwrap_or_default();

    eprintln!("REL_MIN_REPRO id={rel_id} full_rows={full_rows:?} field_rows={field_rows:?} pred_rows={pred_rows:?}");

    
    assert!(!pred_rows.is_empty(), "predicate-only projection unexpectedly empty");
}
