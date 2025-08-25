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



use stele::{provenance::provenance_metrics, StructuredStore};

#[tokio::test]
async fn provenance_sampling() {
    std::env::set_var("STELE_CANON_NS", "prov_sample_ns");
    std::env::set_var("STELE_CANON_DB", "prov_sample_db");
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

    let (base_attempts, base_success, base_empty, base_late) = provenance_metrics();
    let iterations = 15usize;
    for i in 0..iterations {
        let payload = serde_json::json!({
            "source": "sampling_test",
            "reasoning_hash": format!("hash_{i}"),
            "participants": ["Alice"],
            "utterance_ids": [format!("utt_{i}")],
            "extra": {"seq": i}
        });
        let evt = store
            .upsert_canonical_event(
                &format!("Sampling Event {i}"),
                None,
                None,
                None,
                None,
                Some(payload.clone()),
            )
            .await
            .expect("create event");
        
        let prov = store.get_event_provenance(&evt).await.expect("get prov");
        let ok = prov.as_ref().map(|v| v != &serde_json::json!({})).unwrap_or(false);
        if !ok { 
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            let prov2 = store.get_event_provenance(&evt).await.expect("prov2");
            let ok2 = prov2.as_ref().map(|v| v != &serde_json::json!({})).unwrap_or(false);
            eprintln!("PROV_SAMPLE i={i} first_empty retry_present={ok2} value={prov2:?}");
        } else {
            eprintln!("PROV_SAMPLE i={i} present value={prov:?}");
        }
    }
    let (attempts, success, empty, late) = provenance_metrics();
    eprintln!(
        "PROV_SAMPLE_METRICS attempts_delta={} success_delta={} empty_delta={} late_fill_delta={} total_attempts={} total_success={} total_empty={} total_late_fill={}",
        attempts - base_attempts,
        success - base_success,
        empty - base_empty,
        late - base_late,
        attempts,
        success,
        empty,
        late
    );
    
    assert_eq!(attempts - base_attempts, iterations as u64, "attempt mismatch");
    assert_eq!(success - base_success, iterations as u64, "expected all successes (immediate or late)");
    assert_eq!(empty - base_empty, 0, "expected no definitive empties after retry");
}
