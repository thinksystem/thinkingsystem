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
async fn provenance_field_diagnostics() {
    std::env::set_var("STELE_CANON_URL", std::env::var("SURREALDB_URL").unwrap_or_else(|_| "ws://127.0.0.1:8000".into()));
    std::env::set_var("STELE_CANON_USER", std::env::var("SURREALDB_USER").unwrap_or_else(|_| "root".into()));
    std::env::set_var("STELE_CANON_PASS", std::env::var("SURREALDB_PASS").unwrap_or_else(|_| "root".into()));
    std::env::set_var("STELE_CANON_NS", "canon_ns_diag");
    std::env::set_var("STELE_CANON_DB", "canon_db_diag");

    let canon = StructuredStore::connect_canonical_from_env().await.expect("connect");

    
    let _ = canon.query("DEFINE FIELD prov2 ON TABLE canonical_event TYPE option<object>;").await;

    
    let _ = canon.query("DELETE canonical_event WHERE title IN ['DiagProv','DiagProv2','DiagSimple'];").await;

    
    let mut a = canon.query("CREATE canonical_event SET title='DiagProv', description='DiagProv', provenance = {source:'diag', participants:['a']} RETURN AFTER;").await.expect("create a");
    let a_rows: Vec<serde_json::Value> = a.take(0).unwrap_or_default();
    println!("A_ROW={}", serde_json::to_string(&a_rows).unwrap());

    
    let mut b = canon.query("CREATE canonical_event SET title='DiagProv2', description='DiagProv2', prov2 = {source:'diag2', participants:['b']} RETURN AFTER;").await.expect("create b");
    let b_rows: Vec<serde_json::Value> = b.take(0).unwrap_or_default();
    println!("B_ROW={}", serde_json::to_string(&b_rows).unwrap());

    
    let mut c = canon.query("CREATE canonical_event SET title='DiagSimple', description='DiagSimple', provenance = {a:1} RETURN AFTER;").await.expect("create c");
    let c_rows: Vec<serde_json::Value> = c.take(0).unwrap_or_default();
    println!("C_ROW={}", serde_json::to_string(&c_rows).unwrap());

    
    let mut fetch = canon.query("SELECT title, provenance, prov2 FROM canonical_event WHERE title CONTAINS 'Diag' ORDER BY title;").await.expect("fetch");
    let fetched: Vec<serde_json::Value> = fetch.take(0).unwrap_or_default();
    println!("FETCH_DIAG={}", serde_json::to_string(&fetched).unwrap());
}
