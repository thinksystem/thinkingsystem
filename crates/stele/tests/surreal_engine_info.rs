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
use surrealdb::engine::remote::ws::Ws;
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;

#[tokio::test]
async fn surreal_engine_info() {
    
    std::env::set_var("STELE_CANON_NS", "engine_info_ns");
    std::env::set_var("STELE_CANON_DB", "engine_info_db");
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

    let _canon = StructuredStore::connect_canonical_from_env()
        .await
        .expect("connect");

    
    let url = std::env::var("STELE_CANON_URL").unwrap();
    let endpoint = url.strip_prefix("ws://").unwrap_or(&url).to_string();
    let user = std::env::var("STELE_CANON_USER").unwrap();
    let pass = std::env::var("STELE_CANON_PASS").unwrap();
    let ns = std::env::var("STELE_CANON_NS").unwrap();
    let db = std::env::var("STELE_CANON_DB").unwrap();

    let client = Surreal::new::<Ws>(&endpoint).await.expect("fresh connect");
    client
        .signin(Root {
            username: &user,
            password: &pass,
        })
        .await
        .expect("auth");
    client.use_ns(&ns).use_db(&db).await.expect("ns/db");

    for stmt in ["INFO FOR DB;", "INFO FOR NS;", "INFO FOR KV;"] {
        
        match client.query(stmt).await {
            Ok(mut res) => {
                let out: Vec<serde_json::Value> = res.take(0).unwrap_or_default();
                println!("[engine-info] {stmt} => {out:?}");
            }
            Err(e) => println!("[engine-info] {stmt} error: {e}"),
        }
    }

    
    for stmt in ["RETURN version();", "SELECT version();", "INFO FOR KV;"] {
        
        match client.query(stmt).await {
            Ok(mut res) => {
                let out: Vec<serde_json::Value> = res.take(0).unwrap_or_default();
                println!("[engine-version] {stmt} => {out:?}");
            }
            Err(e) => println!("[engine-version] {stmt} error: {e}"),
        }
    }
}
