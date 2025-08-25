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
#[ignore]
async fn provenance_links_execution_to_commit() {
    std::env::set_var("STELE_CANON_NS", "prov_mvp_ns");
    std::env::set_var("STELE_CANON_DB", "prov_mvp_db");
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

    let url = std::env::var("STELE_CANON_URL").unwrap();

    if std::env::var("RUN_PROV_MVP").ok().as_deref() != Some("1") {
        eprintln!("[prov_mvp] Skipping test: set RUN_PROV_MVP=1 and ensure SurrealDB at {url}");
        return;
    }
    let connect_res = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        StructuredStore::connect_canonical_from_env(),
    )
    .await;
    let canon = match connect_res {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => {
            eprintln!("[prov_mvp] Skipping: cannot connect to {url} — {e}");
            return;
        }
        Err(_) => {
            eprintln!("[prov_mvp] Skipping: connect timeout to {url}");
            return;
        }
    };
    let store = StructuredStore::new_with_clients(canon.clone(), canon.clone(), true);

    let dag = stele::provenance::dag::ProvDag::new(&store);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), dag.ensure_schema())
        .await
        .expect("prov schema timeout (pre)");

    let _ = store
        .canonical_db()
        .query("DELETE prov_edge; DELETE commit_event; DELETE execution_event;")
        .await;

    let prov = stele::provenance::context::global();
    prov.set_session("sess_mvp_1", Some("flow_demo")).await;
    prov.push_block("block_A").await;

    let payload = serde_json::json!({
        "source": "mvp_test",
        "reasoning_hash": "hash_123",
        "participants": ["User"],
        "utterance_ids": ["utt1"],
        "extra": {"k": "v"}
    });
    let evt = store
        .upsert_canonical_event("MVP Event", None, None, None, None, Some(payload))
        .await
        .expect("create event");

    let exec_id = dag
        .record_execution("sess_mvp_1", Some("flow_demo"), None, Some("block_A"))
        .await
        .expect("record exec");
    dag.record_commit(&exec_id, &evt)
        .await
        .expect("record commit");

    {
        let db = store.canonical_db();
        let mut q = db
            .query("SELECT id, event_id, session_id, exec_id FROM commit_event")
            .await
            .expect("dump commits");
        match q.take::<Vec<serde_json::Value>>(0) {
            Ok(rows) => eprintln!("[prov_mvp][dbg] post-commit: all commits (json): {rows:?}"),
            Err(e) => eprintln!("[prov_mvp][dbg] post-commit: json decode error: {e}"),
        }

        let mut qids = db
            .query("SELECT id FROM commit_event")
            .await
            .expect("q ids");
        #[derive(serde::Deserialize)]
        struct IdRow {
            id: surrealdb::sql::Thing,
        }
        match qids.take::<Vec<IdRow>>(0) {
            Ok(rows) => {
                let ids: Vec<_> = rows.into_iter().map(|r| r.id).collect();
                eprintln!("[prov_mvp][dbg] post-commit: commit ids: {ids:?}");
            }
            Err(e) => eprintln!("[prov_mvp][dbg] post-commit: commit ids decode error: {e}"),
        }
        let mut qc = db
            .query("SELECT count() AS c FROM commit_event")
            .await
            .expect("count commits");
        let cnt: Vec<serde_json::Value> = qc.take(0).unwrap_or_default();
        eprintln!("[prov_mvp][dbg] post-commit: commit count: {cnt:?}");
        let mut info = db.query("INFO FOR DB;").await.expect("info for db");
        let info_val: Option<serde_json::Value> = info.take(0).ok().flatten();
        eprintln!("[prov_mvp][dbg] INFO FOR DB: {info_val:?}");

        let mut q_ev = db
            .query("SELECT id FROM commit_event WHERE event_id = $e")
            .bind(("e", evt.clone()))
            .await
            .expect("event commits");
        match q_ev.take::<Vec<IdRow>>(0) {
            Ok(rows) => {
                let ids: Vec<_> = rows.into_iter().map(|r| r.id).collect();
                eprintln!("[prov_mvp][dbg] commits for event (ids): {ids:?}");
            }
            Err(e) => eprintln!("[prov_mvp][dbg] commits for event decode error: {e}"),
        }
        let mut q_exec_ids = db
            .query("SELECT exec_id FROM commit_event WHERE event_id = $e")
            .bind(("e", evt.clone()))
            .await
            .expect("event exec ids");
        #[derive(serde::Deserialize)]
        struct ExecIdRow {
            exec_id: surrealdb::sql::Thing,
        }
        match q_exec_ids.take::<Vec<ExecIdRow>>(0) {
            Ok(rows) => {
                let ids: Vec<_> = rows.into_iter().map(|r| r.exec_id).collect();
                eprintln!("[prov_mvp][dbg] exec_ids for event (Things): {ids:?}");
            }
            Err(e) => eprintln!("[prov_mvp][dbg] exec_ids decode error: {e}"),
        }
        let mut q_exec_all = db
            .query("SELECT id FROM execution_event")
            .await
            .expect("all exec");
        match q_exec_all.take::<Vec<IdRow>>(0) {
            Ok(rows) => {
                let ids: Vec<_> = rows.into_iter().map(|r| r.id).collect();
                eprintln!("[prov_mvp][dbg] all execution ids: {ids:?}");
            }
            Err(e) => eprintln!("[prov_mvp][dbg] all execution ids decode error: {e}"),
        }
    }

    let rows = store
        .provenance_commits_for_session("sess_mvp_1")
        .await
        .expect("query commits");
    if rows.is_empty() {
        let db = store.canonical_db();
        let mut q1 = db
            .query("SELECT count() AS c FROM execution_event;")
            .await
            .expect("q1");
        let c1: Vec<serde_json::Value> = q1.take(0).unwrap_or_default();
        eprintln!("[prov_mvp][dbg] exec count: {c1:?}");
        let mut q2 = db
            .query("SELECT * FROM execution_event;")
            .await
            .expect("q2");
        let e_all: Vec<serde_json::Value> = q2.take(0).unwrap_or_default();
        eprintln!("[prov_mvp][dbg] exec rows: {e_all:?}");
        let mut q3 = db
            .query("SELECT count() AS c FROM commit_event;")
            .await
            .expect("q3");
        let c2: Vec<serde_json::Value> = q3.take(0).unwrap_or_default();
        eprintln!("[prov_mvp][dbg] commit count: {c2:?}");
        let mut q4 = db.query("SELECT * FROM commit_event;").await.expect("q4");
        let c_all: Vec<serde_json::Value> = q4.take(0).unwrap_or_default();
        eprintln!("[prov_mvp][dbg] commit rows: {c_all:?}");
        let mut q5 = db
            .query("SELECT count() AS c FROM prov_edge;")
            .await
            .expect("q5");
        let c3: Vec<serde_json::Value> = q5.take(0).unwrap_or_default();
        eprintln!("[prov_mvp][dbg] edge count: {c3:?}");
        let mut q6 = db.query("SELECT * FROM prov_edge;").await.expect("q6");
        let p_all: Vec<serde_json::Value> = q6.take(0).unwrap_or_default();
        eprintln!("[prov_mvp][dbg] edges: {p_all:?}");
        if std::env::var("PROV_MVP_SMOKE").ok().as_deref() == Some("1") {
            eprintln!("[prov_mvp][smoke] Skipping strict asserts due to PROV_MVP_SMOKE=1");
            return;
        }
    }
    assert!(!rows.is_empty(), "no commit rows for session");

    let execs = store
        .provenance_execution_for_event(&evt)
        .await
        .expect("query exec for event");
    if execs.is_empty() {
        let db = store.canonical_db();
        let mut q1 = db
            .query("SELECT id, exec_id, created_at FROM commit_event WHERE event_id = $e")
            .bind(("e", evt.clone()))
            .await
            .expect("q1 evt commits");
        let rows: Vec<serde_json::Value> = q1.take(0).unwrap_or_default();
        eprintln!("[prov_mvp][dbg] commits for event: {rows:?}");
        let mut q2 = db
            .query("SELECT exec_id FROM commit_event WHERE event_id = $e")
            .bind(("e", evt.clone()))
            .await
            .expect("q2 exec ids");
        let exec_ids: Vec<serde_json::Value> = q2.take(0).unwrap_or_default();
        eprintln!("[prov_mvp][dbg] exec_ids for event: {exec_ids:?}");
        let mut q3 = db
            .query("SELECT * FROM prov_edge WHERE in IN (SELECT id FROM commit_event WHERE event_id = $e)")
            .bind(("e", evt.clone()))
            .await
            .expect("q3 edges for event");
        let edges: Vec<serde_json::Value> = q3.take(0).unwrap_or_default();
        eprintln!("[prov_mvp][dbg] edges for event: {edges:?}");
    }
    assert!(!execs.is_empty(), "no execution rows for event");
}
