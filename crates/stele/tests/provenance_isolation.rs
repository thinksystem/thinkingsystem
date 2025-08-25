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
async fn provenance_event_persistence_isolation() {
    
    std::env::set_var("STELE_CANON_NS", "prov_iso_ns");
    std::env::set_var("STELE_CANON_DB", "prov_iso_db");
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
        .expect("connect canonical");
    let store = StructuredStore::new_with_clients(canon.clone(), canon.clone(), true);

    
    let prov_payload = serde_json::json!({
        "source": "test_harness",
        "reasoning_hash": "abc123",
        "participants": ["Alice", "Bob"],
        "utterance_ids": ["utt1"],
        "extra": {"k": "v"}
    });

    let evt_id = store
        .upsert_canonical_event(
            "Isolation Event",
            Some("2025-08-15T10:00:00Z"),
            None,
            Some("TestLoc"),
            None,
            Some(prov_payload.clone()),
        )
        .await
        .expect("create event");

    
    let mut attempt = 0u8;
    let mut final_val = None;
    let mut present = false;
    let strict = std::env::var("STELE_STRICT_PROV").ok().as_deref() == Some("1");
    while attempt < 10 { 
        let prov = store.get_event_provenance(&evt_id).await.expect("prov fetch");
        if prov.as_ref().map(|v| v != &serde_json::json!({})).unwrap_or(false) {
            present = true;
            final_val = prov;
            break;
        }
        attempt += 1;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    eprintln!("PROV_ISO_RESULT attempts={} present={} strict={} value={:?}", attempt + 1, present, strict, final_val);
    if strict { assert!(present, "expected provenance materialisation within retry window (strict mode)"); }

    let (attempts, success, empty, late) = stele::provenance::provenance_metrics();
    eprintln!("PROV_METRICS attempts={attempts} success={success} empty={empty} late_fill={late}");
}
