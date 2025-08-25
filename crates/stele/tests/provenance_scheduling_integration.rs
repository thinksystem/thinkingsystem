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
async fn provenance_scheduling_integration() {
    std::env::set_var("STELE_CANON_NS", "prov_sched_ns");
    std::env::set_var("STELE_CANON_DB", "prov_sched_db");
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

    let payload = serde_json::json!({
        "source": "sched_test",
        "reasoning_hash": "sched_hash_1",
        "participants": ["User"],
        "utterance_ids": ["utt_sched_1"],
        "extra": {"kind": "sched"}
    });
    let evt = store
        .upsert_canonical_event("Scheduling Test Event", None, None, None, None, Some(payload))
        .await
        .expect("create event");

    
    let mut attempt = 0;
    let mut found = false;
    while attempt < 3 { 
        let prov = store.get_event_provenance(&evt).await.expect("get prov");
        if prov.as_ref().map(|v| v != &serde_json::json!({})).unwrap_or(false) { found = true; break; }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        attempt += 1;
    }
    assert!(found, "provenance not materialised after bounded retries");
}
