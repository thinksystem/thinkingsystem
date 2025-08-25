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



use std::time::{SystemTime, UNIX_EPOCH};
use surrealdb::engine::remote::ws::Ws;
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;


async fn create_select_count<C: surrealdb::Connection>(
    client: &Surreal<C>,
    table: &str,
    label: &str,
) -> (
    Vec<serde_json::Value>,
    Option<serde_json::Value>,
    Vec<serde_json::Value>,
) {
    let mut q_create = client
        .query(format!("CREATE {table} SET predicate='{label}' RETURN id;").as_str())
        .await
        .expect("create");
    let create_rows: Vec<serde_json::Value> = q_create.take(0).unwrap_or_default();
    println!("[direct] {table} create rows={create_rows:?}");

    let mut q_sel = client
        .query(format!("SELECT id, predicate FROM {table};").as_str())
        .await
        .expect("select all");
    let sel_rows: Vec<serde_json::Value> = q_sel.take(0).unwrap_or_default();
    println!("[direct] {table} select rows={sel_rows:?}");

    let mut q_cnt = client
        .query(format!("SELECT count() FROM {table};").as_str())
        .await
        .expect("count");
    let cnt_rows: Vec<serde_json::Value> = q_cnt.take(0).unwrap_or_default();
    println!("[direct] {table} count rows={cnt_rows:?}");
    let count_val = cnt_rows.first().and_then(|v| v.get("count")).cloned();
    (sel_rows, count_val, create_rows)
}

#[tokio::test]
async fn relationship_node_direct_repro() {
    let endpoint = "127.0.0.1:8000"; 
    let client = Surreal::new::<Ws>(endpoint).await.expect("connect");
    client
        .signin(Root {
            username: "root",
            password: "root",
        })
        .await
        .expect("auth");
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let ns = format!("direct_repro_ns_{ts}");
    let db = format!("direct_repro_db_{ts}");
    client.use_ns(&ns).use_db(&db).await.expect("nsdb");

    let _ = client.query("DEFINE TABLE relationship_node_plain;").await;
    let _ = client.query("DEFINE TABLE relationship_node_full SCHEMAFULL; DEFINE FIELD predicate ON TABLE relationship_node_full TYPE string;").await;

    let (rows_plain_initial, count_plain_initial, create_plain_rows) =
        create_select_count(&client, "relationship_node_plain", "plain_rel_1").await;

    let explicit_id = format!("relationship_node_plain:{}", ts % 1000000);
    let stmt_explicit = format!("CREATE {explicit_id} SET predicate='plain_rel_2' RETURN id;");
    let mut q4 = client
        .query(stmt_explicit.as_str())
        .await
        .expect("plain create explicit");
    let rows4: Vec<serde_json::Value> = q4.take(0).unwrap_or_default();
    println!("[direct] plain explicit create rows={rows4:?}");

    let stmt_fetch_explicit = format!("SELECT * FROM {explicit_id} LIMIT 1;");
    let mut q5 = client
        .query(stmt_fetch_explicit.as_str())
        .await
        .expect("plain fetch explicit");
    let rows5: Vec<serde_json::Value> = q5.take(0).unwrap_or_default();
    println!("[direct] plain explicit fetch rows={rows5:?}");

    let (rows_full_initial, count_full_initial, create_full_rows) =
        create_select_count(&client, "relationship_node_full", "full_rel_1").await;

    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    let mut q9 = client.query("SELECT id, predicate FROM relationship_node_plain; SELECT id, predicate FROM relationship_node_full;").await.expect("reselect both");
    let re_plain: Vec<serde_json::Value> = q9.take(0).unwrap_or_default();
    let re_full: Vec<serde_json::Value> = q9.take(1).unwrap_or_default();
    println!("[direct] reselect plain rows={re_plain:?}");
    println!("[direct] reselect full rows={re_full:?}");

    let anomaly_plain = count_plain_initial.is_some()
        && (count_plain_initial != Some(serde_json::json!(0)))
        && rows_plain_initial.is_empty();
    let anomaly_full = count_full_initial.is_some()
        && (count_full_initial != Some(serde_json::json!(0)))
        && rows_full_initial.is_empty();
    println!("[direct] anomaly_plain={anomaly_plain} anomaly_full={anomaly_full} count_plain={count_plain_initial:?} count_full={count_full_initial:?} create_plain_rows={create_plain_rows:?} create_full_rows={create_full_rows:?}");

    
    assert!(
        anomaly_plain && anomaly_full,
        "Expected visibility anomaly (plain={anomaly_plain} full={anomaly_full})"
    );
}
