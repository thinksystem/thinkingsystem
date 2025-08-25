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


use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::time::sleep;

pub mod context;
pub mod dag;

static PROV_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static PROV_SUCCESS: AtomicU64 = AtomicU64::new(0);
static PROV_EMPTY: AtomicU64 = AtomicU64::new(0);
static PROV_LATE_FILL: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvenanceSpec {
    pub source: String,
    pub reasoning_hash: Option<String>,
    pub participant_names: Vec<String>,
    pub utterance_ids: Vec<String>,
    pub extra: Value,
}

impl ProvenanceSpec {
    pub fn to_value(&self) -> Value {
        serde_json::json!({
            "source": self.source,
            "reasoning_hash": self.reasoning_hash,
            "participants": self.participant_names,
            "utterance_ids": self.utterance_ids,
            "extra": self.extra,
        })
    }
    pub fn is_meaningful(&self) -> bool {
        self.reasoning_hash.is_some()
            || !self.participant_names.is_empty()
            || !self.utterance_ids.is_empty()
    }
}

pub struct ProvenanceWriter<'a> {
    pub store: &'a crate::database::structured_store::StructuredStore,
}

impl<'a> ProvenanceWriter<'a> {
    pub fn new(store: &'a crate::database::structured_store::StructuredStore) -> Self {
        Self { store }
    }

    pub async fn apply_to_event(
        &self,
        event_id: &surrealdb::sql::Thing,
        spec: &ProvenanceSpec,
    ) -> Result<(), String> {
        PROV_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
        if !spec.is_meaningful() {
            tracing::warn!(id = %event_id, "ProvenanceWriter: empty or minimal spec supplied; proceeding");
        }
        let mut participant_ids: Vec<String> = Vec::new();
        for name in &spec.participant_names {
            if let Ok(id) = self
                .store
                .upsert_canonical_entity("person", name, Some(&format!("person:{name}")), None)
                .await
            {
                participant_ids.push(id.to_string());
            }
        }
        let mut val = spec.to_value();
        if !participant_ids.is_empty() {
            if let serde_json::Value::Object(ref mut map) = val {
                map.insert(
                    "participant_ids".into(),
                    serde_json::Value::Array(
                        participant_ids
                            .into_iter()
                            .map(serde_json::Value::String)
                            .collect(),
                    ),
                );
            }
        }
        if let Err(e) = self
            .store
            .canonical_db()
            .query("UPDATE $id SET provenance = <object>$p")
            .bind(("id", event_id.clone()))
            .bind(("p", val.clone()))
            .await
        {
            tracing::warn!(id=%event_id, error=%e, "ProvenanceWriter: initial provenance update failed");
        }
        match self
            .store
            .set_object_field_force(event_id, "provenance", &val)
            .await
        {
            Ok(_) => {
                PROV_SUCCESS.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(e) => {
                tracing::warn!(id=%event_id, error=%e, "ProvenanceWriter: set_object_field_force error");
                sleep(Duration::from_millis(20)).await;
                let original_val = spec.to_value();
                if self
                    .store
                    .set_object_field_force(event_id, "provenance", &original_val)
                    .await
                    .is_ok()
                {
                    PROV_SUCCESS.fetch_add(1, Ordering::Relaxed);
                    PROV_LATE_FILL.fetch_add(1, Ordering::Relaxed);
                    return Ok(());
                }
                PROV_EMPTY.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(id = %event_id, "ProvenanceWriter: provenance empty after two attempts");
                Ok(())
            }
        }
    }
}

pub fn provenance_metrics() -> (u64, u64, u64, u64) {
    (
        PROV_ATTEMPTS.load(Ordering::Relaxed),
        PROV_SUCCESS.load(Ordering::Relaxed),
        PROV_EMPTY.load(Ordering::Relaxed),
        PROV_LATE_FILL.load(Ordering::Relaxed),
    )
}
