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


use serde_json::Value;
use surrealdb::{engine::remote::ws::Client, sql::Thing, Surreal};

use super::types::DatabaseError;

pub fn json_diff(a: &Value, b: &Value) -> Value {
    match (a, b) {
        (Value::Object(ma), Value::Object(mb)) => {
            let mut out = serde_json::Map::new();
            
            let mut keys: std::collections::BTreeSet<&str> = ma.keys().map(|k| k.as_str()).collect();
            keys.extend(mb.keys().map(|k| k.as_str()));
            for k in keys {
                let va = ma.get(k);
                let vb = mb.get(k);
                if va == vb { continue; }
                match (va, vb) {
                    (Some(va), Some(vb)) => {
                        let sub = json_diff(va, vb);
                        if !sub.is_null() {
                            out.insert(k.to_string(), sub);
                        } else if va != vb {
                            out.insert(k.to_string(), serde_json::json!({"old": va, "new": vb}));
                        }
                    }
                    (Some(va), None) => { out.insert(k.to_string(), serde_json::json!({"old": va, "new": null})); }
                    (None, Some(vb)) => { out.insert(k.to_string(), serde_json::json!({"old": null, "new": vb})); }
                    _ => {}
                }
            }
            Value::Object(out)
        }
        _ => {
            if a != b { serde_json::json!({"old": a, "new": b}) } else { Value::Null }
        }
    }
}

pub async fn diff_relationship_fact_ids(
    db: &Surreal<Client>,
    a: &Thing,
    b: &Thing,
) -> Result<Value, DatabaseError> {
    let mut qa = db
        .query("SELECT * FROM $id LIMIT 1;")
        .bind(("id", a.clone()))
        .await
        .map_err(|e| DatabaseError::Query(format!("diff fetch a failed: {e}")))?;
    let mut qb = db
        .query("SELECT * FROM $id LIMIT 1;")
        .bind(("id", b.clone()))
        .await
        .map_err(|e| DatabaseError::Query(format!("diff fetch b failed: {e}")))?;
    let va: Vec<Value> = qa.take(0).unwrap_or_default();
    let vb: Vec<Value> = qb.take(0).unwrap_or_default();
    let oa = va.first().cloned().unwrap_or(Value::Null);
    let ob = vb.first().cloned().unwrap_or(Value::Null);
    Ok(json_diff(&oa, &ob))
}
