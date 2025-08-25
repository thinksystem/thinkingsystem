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



use serde::Serialize;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use surrealdb::engine::any;
use surrealdb::Surreal;

#[derive(Debug, Serialize, Clone)]
struct ProbeResult {
    label: String,
    create_variant: String,
    create_return_len: usize,
    select_len: usize,
    count: Option<u64>,
    explicit_id: bool,
    had_index: bool,
    table: String,
}

async fn run_create<C: surrealdb::Connection>(
    client: &Surreal<C>,
    table: &str,
    create_stmt: &str,
    select_stmt: &str,
    label: &str,
    had_index: bool,
    explicit_id: bool,
) -> ProbeResult {
    let mut q_create = client.query(create_stmt).await.expect("create");
    let create_rows: Vec<serde_json::Value> = q_create.take(0).unwrap_or_default();
    let create_return_len = create_rows.len();

    let mut q_count = client
        .query(format!("SELECT count() FROM {table};").as_str())
        .await
        .expect("count");
    let count_rows: Vec<serde_json::Value> = q_count.take(0).unwrap_or_default();
    let count_val = count_rows
        .first()
        .and_then(|v| v.get("count"))
        .and_then(|v| v.as_u64());

    
    let mut q_sel = client.query(select_stmt).await.expect("select");
    let sel_rows: Vec<serde_json::Value> = q_sel.take(0).unwrap_or_default();

    
    tokio::time::sleep(Duration::from_millis(120)).await;
    let mut q_re = client.query(select_stmt).await.expect("reselect");
    let re_rows: Vec<serde_json::Value> = q_re.take(0).unwrap_or_default();
    let final_rows = if !re_rows.is_empty() {
        re_rows
    } else {
        sel_rows
    };

    println!(
        "[probe] label={label} table={table} variant={create_stmt:?} create_len={create_return_len} count={count_val:?} select_len={} explicit_id={explicit_id} had_index={had_index}",
        final_rows.len()
    );

    ProbeResult {
        label: label.to_string(),
        create_variant: create_stmt.to_string(),
        create_return_len,
        select_len: final_rows.len(),
        count: count_val,
        explicit_id,
        had_index,
        table: table.to_string(),
    }
}

#[tokio::test]
async fn relationship_node_visibility_probes() {
    let client: Surreal<_> = any::connect("mem://").await.expect("mem connect");
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let ns = format!("probe_ns_{ts}");
    let db = format!("probe_db_{ts}");
    client.use_ns(&ns).use_db(&db).await.expect("nsdb");

    
    
    let _ = client.query("DEFINE TABLE rn_plain;").await;
    
    let _ = client
        .query("DEFINE TABLE rn_plain_indexed; DEFINE INDEX idx_pred ON TABLE rn_plain_indexed FIELDS predicate UNIQUE;")
        .await;
    
    let _ = client.query("DEFINE TABLE rn_alt_field SCHEMAFULL; DEFINE FIELD foo ON TABLE rn_alt_field TYPE string;").await;
    
    let _ = client.query("DEFINE TABLE rn_content SCHEMAFULL; DEFINE FIELD predicate ON TABLE rn_content TYPE string; DEFINE FIELD extra ON TABLE rn_content TYPE int;").await;
    
    let _ = client.query("DEFINE TABLE rn_insert;").await;

    let mut results = Vec::new();

    
    results.push(
        run_create(
            &client,
            "rn_plain",
            "CREATE rn_plain SET predicate='a1' RETURN id;",
            "SELECT id,predicate FROM rn_plain;",
            "plain_return_id",
            false,
            false,
        )
        .await,
    );
    results.push(
        run_create(
            &client,
            "rn_plain",
            "CREATE rn_plain SET predicate='a2' RETURN AFTER;",
            "SELECT id,predicate FROM rn_plain;",
            "plain_return_after",
            false,
            false,
        )
        .await,
    );
    results.push(
        run_create(
            &client,
            "rn_plain",
            "CREATE rn_plain SET predicate='a3' RETURN *;",
            "SELECT id,predicate FROM rn_plain;",
            "plain_return_star",
            false,
            false,
        )
        .await,
    );
    results.push(
        run_create(
            &client,
            "rn_plain",
            "CREATE rn_plain SET predicate='a4' RETURN NONE;",
            "SELECT id,predicate FROM rn_plain;",
            "plain_return_none",
            false,
            false,
        )
        .await,
    );

    
    results.push(
        run_create(
            &client,
            "rn_plain_indexed",
            "CREATE rn_plain_indexed SET predicate='ix1' RETURN id;",
            "SELECT id,predicate FROM rn_plain_indexed;",
            "indexed_return_id",
            true,
            false,
        )
        .await,
    );

    
    results.push(
        run_create(
            &client,
            "rn_alt_field",
            "CREATE rn_alt_field SET foo='f1' RETURN *;",
            "SELECT id,foo FROM rn_alt_field;",
            "alt_field_return_star",
            false,
            false,
        )
        .await,
    );

    
    results.push(
        run_create(
            &client,
            "rn_content",
            "CREATE rn_content CONTENT { predicate: 'c1', extra: 1 } RETURN *;",
            "SELECT id,predicate,extra FROM rn_content;",
            "content_return_star",
            false,
            false,
        )
        .await,
    );

    
    let explicit_id = format!("rn_plain:{}", ts % 1000000);
    let create_explicit = format!("CREATE {explicit_id} SET predicate='exp1' RETURN id;");
    let select_explicit = format!("SELECT * FROM {explicit_id};");
    results.push(
        run_create(
            &client,
            "rn_plain",
            &create_explicit,
            &select_explicit,
            "explicit_id_return_id",
            false,
            true,
        )
        .await,
    );

    
    results.push(
        run_create(
            &client,
            "rn_insert",
            "INSERT INTO rn_insert { predicate: 'ins1' } RETURN *;",
            "SELECT id,predicate FROM rn_insert;",
            "insert_return_star",
            false,
            false,
        )
        .await,
    );

    
    let summary = serde_json::to_string_pretty(&results).unwrap();
    println!("[probe-summary] {summary}");

    
    let base = results
        .iter()
        .find(|r| r.label == "plain_return_id")
        .unwrap();
    let baseline_anomaly = base.count.unwrap_or(0) > 0 && base.select_len == 0;
    assert!(baseline_anomaly, "Baseline anomaly (plain_return_id) not present; upstream may have fixed issue—revisit tests.");
}
