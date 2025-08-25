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



use crate::database::query_metrics::record_query;

use crate::database::tokens::DateTimeToken;
use crate::database::types::DatabaseError;
use crate::provenance;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use std::{env, fs};
use surrealdb::sql::Thing;
use surrealdb::{engine::remote::ws::Client, RecordId, Surreal};
use tracing::{debug, info};
use tracing::{instrument, warn};

#[derive(serde::Deserialize)]
struct CreatedWithId {
    id: Thing,
}

#[derive(Clone)]
pub struct StructuredStore {
    canonical_db: Arc<Surreal<Client>>,

    dynamic_db: Arc<Surreal<Client>>,

    same_database: bool,

    canon_ns: Option<String>,
    canon_db_name: Option<String>,
    dynamic_ns: Option<String>,
    dynamic_db_name: Option<String>,
    trace_queries: bool,
}

impl StructuredStore {
    fn build_surreal_object_literal(value: &serde_json::Value) -> String {
        if let serde_json::Value::Object(map) = value {
            let mut parts: Vec<String> = Vec::new();
            for (k, v) in map {
                let lit = match v {
                    serde_json::Value::String(s) => format!("'{}'", s.replace("'", "''")),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Null => "null".into(),
                    _ => format!("{v}"),
                };
                parts.push(format!("{k}: {lit}"));
            }
            format!("{{{}}}", parts.join(", "))
        } else {
            "{}".into()
        }
    }

    pub async fn set_object_field_force(
        &self,
        id: &Thing,
        field: &str,
        value: &serde_json::Value,
    ) -> Result<(), DatabaseError> {
        let q1 = format!("UPDATE $id SET {field} = $v");
        if let Err(e) = self
            .canonical_db
            .query(q1.as_str())
            .bind(("id", id.clone()))
            .bind(("v", value.clone()))
            .await
        {
            warn!(id=%id, field=%field, error=%e, "set_object_field_force: initial update failed");
        };

        if self
            .object_field_is_nonempty(id, field)
            .await
            .unwrap_or(false)
        {
            return Ok(());
        }

        let literal = Self::build_surreal_object_literal(value);
        let q2 = format!("UPDATE $id SET {field} = {literal}");
        if let Err(e) = self
            .canonical_db
            .query(q2.as_str())
            .bind(("id", id.clone()))
            .await
        {
            warn!(id=%id, field=%field, error=%e, "set_object_field_force: literal update failed");
        }

        if self
            .object_field_is_nonempty(id, field)
            .await
            .unwrap_or(false)
        {
            return Ok(());
        }
        warn!(id=%id, field=%field, "set_object_field_force: field still empty after fallback");
        Ok(())
    }

    async fn object_field_is_nonempty(
        &self,
        id: &Thing,
        field: &str,
    ) -> Result<bool, DatabaseError> {
        let sel = format!("SELECT {field} FROM $id");
        let mut res = self
            .canonical_db
            .query(sel.as_str())
            .bind(("id", id.clone()))
            .await
            .map_err(|e| DatabaseError::Query(format!("verify field select failed: {e}")))?;
        if let Ok::<Vec<serde_json::Value>, _>(rows) = res.take(0) {
            if let Some(row) = rows.first() {
                if let Some(v) = row.get(field) {
                    return Ok(v != &serde_json::json!({}) && !v.is_null());
                }
            }
        }
        Ok(false)
    }

    pub fn new(db: Arc<Surreal<Client>>) -> Self {
        Self {
            canonical_db: db.clone(),
            dynamic_db: db,
            same_database: true,
            canon_ns: std::env::var("STELE_CANON_NS")
                .ok()
                .or_else(|| std::env::var("SURREALDB_NS").ok()),
            canon_db_name: std::env::var("STELE_CANON_DB")
                .ok()
                .or_else(|| std::env::var("SURREALDB_DB").ok()),
            dynamic_ns: std::env::var("SURREALDB_NS").ok(),
            dynamic_db_name: std::env::var("SURREALDB_DB").ok(),
            trace_queries: std::env::var("STELE_QUERY_TRACE")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        }
    }

    pub fn new_with_clients(
        canonical_db: Arc<Surreal<Client>>,
        dynamic_db: Arc<Surreal<Client>>,
        same_database: bool,
    ) -> Self {
        Self {
            canonical_db,
            dynamic_db,
            same_database,
            canon_ns: std::env::var("STELE_CANON_NS").ok(),
            canon_db_name: std::env::var("STELE_CANON_DB").ok(),
            dynamic_ns: std::env::var("SURREALDB_NS").ok(),
            dynamic_db_name: std::env::var("SURREALDB_DB").ok(),
            trace_queries: std::env::var("STELE_QUERY_TRACE")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        }
    }

    fn dyn_db(&self) -> &Surreal<Client> {
        self.dynamic_db.as_ref()
    }

    pub fn canonical_db(&self) -> &Surreal<Client> {
        self.canonical_db.as_ref()
    }

    fn trace(&self, label: &str, sql: &str) {
        if self.trace_queries {
            tracing::info!(target="stele::db::query", label, sql, canon_ns=?self.canon_ns, canon_db=?self.canon_db_name, dyn_ns=?self.dynamic_ns, dyn_db=?self.dynamic_db_name);
        }
    }

    pub async fn provenance_commits_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<serde_json::Value>, DatabaseError> {
        let dag = crate::provenance::dag::ProvDag::new(self);
        dag.query_commits_for_session(session_id).await
    }

    pub async fn provenance_execution_for_event(
        &self,
        canonical_event_id: &Thing,
    ) -> Result<Vec<serde_json::Value>, DatabaseError> {
        let dag = crate::provenance::dag::ProvDag::new(self);
        dag.query_exec_for_canonical_event(canonical_event_id).await
    }

    async fn upsert_canonical_ref(
        &self,
        kind: &str,
        canonical: &Thing,
        key: Option<&str>,
        name: Option<&str>,
        extra: Option<Value>,
    ) -> Result<Thing, DatabaseError> {
        let id_str = canonical.to_string();
        let kind_owned = kind.to_string();
        let key_owned = key.map(|s| s.to_string());
        let name_owned = name.map(|s| s.to_string());
        let extra_owned = extra.unwrap_or(serde_json::json!({}));

        let mut q = self
            .dyn_db()
            .query("SELECT id FROM canonical_ref WHERE canonical_id = $id LIMIT 1;")
            .bind(("id", id_str.clone()))
            .await
            .map_err(|e| DatabaseError::Query(format!("canonical_ref lookup failed: {e}")))?;
        self.trace(
            "SELECT canonical_ref by canonical_id",
            "SELECT id FROM canonical_ref WHERE canonical_id = $id LIMIT 1;",
        );
        if let Ok::<Vec<Thing>, _>(rows) = q.take(0) {
            if let Some(t) = rows.first() {
                return Ok(t.clone());
            }
        }

        let mut upsert = self
            .dyn_db()
            .query(
                "UPSERT canonical_ref SET kind = $k, canonical_id = $id, key = $key, name = $name, extra = $extra, created_at = time::now() WHERE canonical_id = $id RETURN AFTER",
            )
            .bind(("k", kind_owned))
            .bind(("id", id_str.clone()))
            .bind(("key", key_owned))
            .bind(("name", name_owned))
            .bind(("extra", extra_owned))
            .await
            .map_err(|e| DatabaseError::Query(format!("canonical_ref upsert failed: {e}")))?;
        self.trace("UPSERT canonical_ref","UPSERT canonical_ref SET kind = $k, canonical_id = $id, key = $key, name = $name, extra = $extra, created_at = time::now() WHERE canonical_id = $id RETURN AFTER");
        if let Ok::<Vec<CreatedWithId>, _>(rows) = upsert.take(0) {
            if let Some(first) = rows.first() {
                return Ok(first.id.clone());
            }
        }

        let mut verify = self
            .dyn_db()
            .query("SELECT id FROM canonical_ref WHERE canonical_id = $id LIMIT 1;")
            .bind(("id", id_str))
            .await
            .map_err(|e| {
                DatabaseError::Query(format!("canonical_ref post-upsert lookup failed: {e}"))
            })?;
        self.trace(
            "VERIFY canonical_ref",
            "SELECT id FROM canonical_ref WHERE canonical_id = $id LIMIT 1;",
        );
        if let Ok::<Vec<Thing>, _>(rows) = verify.take(0) {
            if let Some(id) = rows.first() {
                return Ok(id.clone());
            }
        }

        Err(DatabaseError::Query(
            "canonical_ref upsert returned no id".into(),
        ))
    }

    pub async fn connect_canonical_from_env() -> Result<Arc<Surreal<Client>>, DatabaseError> {
        use surrealdb::engine::remote::ws::Ws;
        use surrealdb::opt::auth::Root;

        let url = env::var("STELE_CANON_URL").map_err(|_| {
            DatabaseError::ConnectionFailed(
                "Missing STELE_CANON_URL (canonical DB must be configured separately)".into(),
            )
        })?;
        let endpoint = url.strip_prefix("ws://").unwrap_or(&url).to_string();
        let user = env::var("STELE_CANON_USER")
            .map_err(|_| DatabaseError::ConnectionFailed("Missing STELE_CANON_USER".into()))?;
        let pass = env::var("STELE_CANON_PASS")
            .map_err(|_| DatabaseError::ConnectionFailed("Missing STELE_CANON_PASS".into()))?;
        let ns = env::var("STELE_CANON_NS").map_err(|_| {
            DatabaseError::ConnectionFailed(
                "Missing STELE_CANON_NS (must be different to SURREALDB_NS)".into(),
            )
        })?;
        let db_name = env::var("STELE_CANON_DB")
            .map_err(|_| DatabaseError::ConnectionFailed("Missing STELE_CANON_DB".into()))?;

        if let Ok(dyn_ns) = env::var("SURREALDB_NS") {
            if dyn_ns == ns {
                return Err(DatabaseError::ConnectionFailed(
                    "Canonical and dynamic namespaces must differ. Set STELE_CANON_NS to a different namespace.".into(),
                ));
            }
        }

        let client = Surreal::new::<Ws>(&endpoint).await.map_err(|e| {
            DatabaseError::ConnectionFailed(format!("Canonical connect failed: {e}"))
        })?;
        client
            .signin(Root {
                username: &user,
                password: &pass,
            })
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(format!("Canonical auth failed: {e}")))?;
        client.use_ns(&ns).use_db(&db_name).await.map_err(|e| {
            DatabaseError::ConnectionFailed(format!("Canonical ns/db select failed: {e}"))
        })?;

        let embedded: &str = include_str!("./config/canonical_schema.sql");
        let schema = if !embedded.trim().is_empty() {
            embedded
        } else {
            let path = "crates/stele/src/database/config/canonical_schema.sql";
            match fs::read_to_string(path) {
                Ok(s) => Box::leak(s.into_boxed_str()),
                Err(_) => "",
            }
        };
        if schema.is_empty() {
            return Err(DatabaseError::ConnectionFailed(
                "Canonical schema not found or empty".into(),
            ));
        }
        client.query(schema).await.map_err(|e| {
            DatabaseError::ConnectionFailed(format!("Failed to apply canonical schema: {e}"))
        })?;

        Ok(Arc::new(client))
    }

    #[instrument(level = "info", skip(self, extra))]
    pub async fn upsert_canonical_entity(
        &self,
        entity_type: &str,
        name: &str,
        canonical_key: Option<&str>,
        extra: Option<Value>,
    ) -> Result<Thing, DatabaseError> {
        info!(entity_type = %entity_type, name = %name, has_canonical_key = canonical_key.is_some(), "StructuredStore: upsert_canonical_entity called");

        let et = entity_type.to_string();
        let nm = name.to_string();

        let key_final = canonical_key
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{}:{}", et.to_lowercase(), nm.to_lowercase()));

        info!(canonical_key = %key_final, entity_type = %et, name = %nm, "StructuredStore: upsert_canonical_entity start");
        let _key_for_verify = key_final.clone();
        if !key_final.is_empty() {
            let start = Instant::now();
            let mut res = self
                .canonical_db
                .query("SELECT id FROM canonical_entity WHERE canonical_key = $k LIMIT 1;")
                .bind(("k", key_final.clone()))
                .await
                .map_err(|e| DatabaseError::Query(format!("Lookup failed: {e}")))?;
            record_query(
                "SELECT canonical_entity by key",
                start.elapsed().as_millis(),
            );

            if let Ok::<Vec<Thing>, _>(ids) = res.take(0) {
                if !key_final.is_empty() {
                    info!(key = %key_final, existing_count = ids.len(), "StructuredStore: select canonical_entity by key");
                    if let Some(thing) = ids.first() {
                        if let Some(extra_val) = extra.clone() {
                            debug!(id = %thing, "StructuredStore: updating canonical_entity with extra");
                            let start = Instant::now();
                            self
                                .canonical_db
                                .query("UPDATE $id SET entity_type = $t, name = $n, extra = $e, updated_at = time::now()")
                                .bind(("id", thing.clone()))
                                .bind(("t", et.clone()))
                                .bind(("n", nm.clone()))
                                .bind(("e", extra_val))
                                .await
                                .map_err(|e| DatabaseError::Query(format!("Update failed: {e}")))?;
                            record_query(
                                "UPDATE canonical_entity (with extra)",
                                start.elapsed().as_millis(),
                            );
                        } else {
                            debug!(id = %thing, "StructuredStore: updating canonical_entity");
                            let start = Instant::now();
                            self
                                .canonical_db
                                .query("UPDATE $id SET entity_type = $t, name = $n, updated_at = time::now()")
                                .bind(("id", thing.clone()))
                                .bind(("t", et.clone()))
                                .bind(("n", nm.clone()))
                                .await
                                .map_err(|e| DatabaseError::Query(format!("Update failed: {e}")))?;
                            record_query("UPDATE canonical_entity", start.elapsed().as_millis());
                        }
                        info!(id = %thing, "StructuredStore: upsert_canonical_entity returning existing id");
                        return Ok(thing.clone());
                    }
                }
            }
        }

        let e_val = extra.unwrap_or(serde_json::json!({}));

        let start_upd = Instant::now();
        let update_result = self.canonical_db
            .query("UPDATE canonical_entity SET entity_type = $t, name = $n, extra = $e, updated_at = time::now() WHERE canonical_key = $k RETURN AFTER;")
            .bind(("t", et.clone()))
            .bind(("n", nm.clone()))
            .bind(("e", e_val.clone()))
            .bind(("k", key_final.clone()))
            .await;
        record_query(
            "UPDATE canonical_entity (upsert phase)",
            start_upd.elapsed().as_millis(),
        );
        if let Ok(mut upd) = update_result {
            if let Ok::<Vec<CreatedWithId>, _>(rows) = upd.take(0) {
                if let Some(first) = rows.first() {
                    if let Ok(mut vis) = self
                        .canonical_db
                        .query("SELECT id FROM canonical_entity WHERE id = $id;")
                        .bind(("id", first.id.clone()))
                        .await
                    {
                        if let Ok::<Vec<Thing>, _>(probe) = vis.take(0) {
                            info!(probe_len = probe.len(), id = %first.id, key = %key_final, "StructuredStore: upsert(update) immediate visibility");
                        }
                    }
                    return Ok(first.id.clone());
                } else {
                    info!(key = %key_final, "StructuredStore: upsert(update) no rows returned");
                }
            }
        } else {
            info!(key = %key_final, "StructuredStore: upsert(update) query error or empty");
        }

        if let Ok(mut cnt) = self
            .canonical_db
            .query("SELECT count() AS c FROM canonical_entity;")
            .await
        {
            if let Ok::<Vec<serde_json::Value>, _>(crows) = cnt.take(0) {
                info!(?crows, key = %key_final, "StructuredStore: pre-create table count");
            }
        }

        let start_create = Instant::now();
        let create_result = self.canonical_db
            .query("CREATE canonical_entity CONTENT { entity_type: $t, name: $n, canonical_key: $k, extra: $e } RETURN AFTER;")
            .bind(("t", et.clone()))
            .bind(("n", nm.clone()))
            .bind(("k", key_final.clone()))
            .bind(("e", e_val))
            .await;
        record_query(
            "CREATE canonical_entity (upsert)",
            start_create.elapsed().as_millis(),
        );
        match create_result {
            Ok(mut res) => {
                if let Ok::<Vec<CreatedWithId>, _>(rows) = res.take(0) {
                    if let Some(first) = rows.first() {
                        if let Ok(mut vis) = self
                            .canonical_db
                            .query("SELECT id FROM canonical_entity WHERE id = $id;")
                            .bind(("id", first.id.clone()))
                            .await
                        {
                            if let Ok::<Vec<Thing>, _>(probe) = vis.take(0) {
                                info!(probe_len = probe.len(), id = %first.id, key = %key_final, "StructuredStore: create path visibility by id");
                            }
                        }
                        if let Ok(mut vis2) = self
                            .canonical_db
                            .query("SELECT id FROM canonical_entity WHERE canonical_key = $k;")
                            .bind(("k", key_final.clone()))
                            .await
                        {
                            if let Ok::<Vec<Thing>, _>(probe2) = vis2.take(0) {
                                info!(probe2_len = probe2.len(), key = %key_final, "StructuredStore: create path visibility by key");
                            }
                        }
                        info!(id = %first.id, key = %key_final, "StructuredStore: upsert (create path)");
                        return Ok(first.id.clone());
                    } else {
                        info!(key = %key_final, "StructuredStore: create path returned empty rows vector");
                    }
                } else {
                    info!(key = %key_final, "StructuredStore: create path rows decode failed");
                }
            }
            Err(e) => {
                warn!(error = %e, key = %key_final, "StructuredStore: create path failed (possible race), selecting existing");
            }
        }

        if let Ok(mut sample) = self
            .canonical_db
            .query("SELECT id, name, canonical_key FROM canonical_entity LIMIT 5;")
            .await
        {
            if let Ok::<Vec<serde_json::Value>, _>(srows) = sample.take(0) {
                info!(sample_len = srows.len(), ?srows, key = %key_final, "StructuredStore: post-create sample");
            }
        }

        let mut verify = self
            .canonical_db
            .query("SELECT id FROM canonical_entity WHERE canonical_key = $k LIMIT 1;")
            .bind(("k", key_final.clone()))
            .await
            .map_err(|e| DatabaseError::Query(format!("Select fallback failed: {e}")))?;
        if let Ok::<Vec<Thing>, _>(ids) = verify.take(0) {
            if let Some(first) = ids.first() {
                return Ok(first.clone());
            }
        }

        #[cfg(test)]
        {
            use surrealdb::Value as RawVal;
            if let Ok(Some(rv)) = verify.take::<Option<RawVal>>(0) {
                let js = rv.to_string();
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&js) {
                    if let Some(arr) = v.as_array() {
                        for obj in arr {
                            if let Some(idv) = obj.get("id").and_then(|i| i.as_str()) {
                                if let Ok(t) = idv.parse::<Thing>() {
                                    return Ok(t);
                                }
                            }
                        }
                    }
                }
                info!(raw_verify_json = %js, key = %key_final, "StructuredStore: fallback surreal Value decode produced no id");
            }
        }

        if let Ok(mut cnt2) = self
            .canonical_db
            .query("SELECT count() AS c FROM canonical_entity;")
            .await
        {
            if let Ok::<Vec<serde_json::Value>, _>(crows2) = cnt2.take(0) {
                info!(?crows2, key = %key_final, "StructuredStore: final failure table count");
            }
        }
        Err(DatabaseError::Query(
            "Upsert canonical_entity: no id returned after update/create/select attempts".into(),
        ))
    }

    pub async fn upsert_canonical_task(
        &self,
        title: &str,
        assignee: Option<&str>,
        due_date: Option<&str>,
        status: Option<&str>,
        canonical_key: Option<&str>,
        extra: Option<Value>,
    ) -> Result<Thing, DatabaseError> {
        let title_owned = title.to_string();
        let _assignee_owned = assignee.map(|s| s.to_string());
        let due_owned =
            due_date.and_then(|s| DateTimeToken::new(s).ok().map(|tok| tok.to_rfc3339()));
        let status_owned = status.map(|s| s.to_string());
        let _key_owned = canonical_key.map(|s| s.to_string());

        if let Some(d) = due_owned.as_ref() {
            let mut res = self
                .canonical_db
                .query("SELECT id FROM canonical_task WHERE title = $t AND due_at = <datetime>$d LIMIT 1;")
                .bind(("t", title_owned.clone()))
                .bind(("d", d.clone()))
                .await
                .map_err(|e| DatabaseError::Query(format!("Lookup failed: {e}")))?;
            if let Ok::<Vec<Thing>, _>(ids) = res.take(0) {
                if let Some(thing) = ids.first() {
                    self
                        .canonical_db
                        .query("UPDATE $id SET title = $t, due_at = <datetime>$d, status = $s, provenance = $e, updated_at = time::now()")
                        .bind(("id", thing.clone()))
                        .bind(("t", title_owned.clone()))
                        .bind(("d", d.clone()))
                        .bind(("s", status_owned.clone()))
                        .bind(("e", extra.clone().unwrap_or(serde_json::json!({}))))
                        .await
                        .map_err(|e| DatabaseError::Query(format!("Update failed: {e}")))?;
                    debug!(id = %thing, "StructuredStore: updated canonical_task");
                    return Ok(thing.clone());
                }
            }
        }

        let e_val = extra.unwrap_or(serde_json::json!({}));
        let start = Instant::now();
        let mut res = if let Some(d) = due_owned.as_ref() {
            self
                .canonical_db
                .query("CREATE canonical_task SET title = $t, due_at = <datetime>$d, status = $s, provenance = $e, created_at = time::now() RETURN AFTER")
                .bind(("t", title_owned.clone()))
                .bind(("d", d.clone()))
                .bind(("s", status_owned.clone()))
                .bind(("e", e_val.clone()))
                .await
                .map_err(|e| DatabaseError::Query(format!("Create failed: {e}")))?
        } else {
            self
                .canonical_db
                .query("CREATE canonical_task SET title = $t, status = $s, provenance = $e, created_at = time::now() RETURN AFTER")
                .bind(("t", title_owned.clone()))
                .bind(("s", status_owned.clone()))
                .bind(("e", e_val.clone()))
                .await
                .map_err(|e| DatabaseError::Query(format!("Create failed: {e}")))?
        };
        record_query("CREATE canonical_task", start.elapsed().as_millis());
        let created: Vec<CreatedWithId> = res.take(0).unwrap_or_default();
        if let Some(first) = created.first() {
            info!(id = %first.id, "StructuredStore: created canonical_task");
            return Ok(first.id.clone());
        }

        if let Some(d) = due_owned.as_ref() {
            let mut verify = self
                .canonical_db
                .query("SELECT id, created_at FROM canonical_task WHERE title = $t AND due_at = <datetime>$d ORDER BY created_at DESC LIMIT 1;")
                .bind(("t", title_owned.clone()))
                .bind(("d", d.clone()))
                .await
                .map_err(|e| DatabaseError::Query(format!("Post-create lookup failed: {e}")))?;
            if let Ok::<Vec<Thing>, _>(rows2) = verify.take(0) {
                if let Some(id) = rows2.first() {
                    return Ok(id.clone());
                }
            }
        } else {
            let mut verify = self
                .canonical_db
                .query("SELECT id, created_at FROM canonical_task WHERE title = $t ORDER BY created_at DESC LIMIT 1;")
                .bind(("t", title_owned.clone()))
                .await
                .map_err(|e| DatabaseError::Query(format!("Post-create lookup failed: {e}")))?;
            if let Ok::<Vec<Thing>, _>(rows2) = verify.take(0) {
                if let Some(id) = rows2.first() {
                    return Ok(id.clone());
                }
            }
        }
        Err(DatabaseError::Query(
            "No ID returned from create (task)".into(),
        ))
    }

    pub async fn upsert_canonical_event(
        &self,
        title: &str,
        start_time: Option<&str>,
        end_time: Option<&str>,
        location: Option<&str>,
        canonical_key: Option<&str>,
        extra: Option<Value>,
    ) -> Result<Thing, DatabaseError> {
        let title_owned = title.to_string();
        let start_owned = start_time.and_then(|s| match DateTimeToken::new(s) {
            Ok(tok) => Some(tok.to_rfc3339()),
            Err(_) => None,
        });
        let end_owned = end_time.and_then(|s| match DateTimeToken::new(s) {
            Ok(tok) => Some(tok.to_rfc3339()),
            Err(_) => None,
        });
        let loc_owned = location.map(|s| s.to_string());
        let _key_owned = canonical_key.map(|s| s.to_string());

        let extra_clone_for_update = extra.clone();

        if let Some(st) = start_owned.as_ref() {
            let start = Instant::now();
            let mut res = self
                .canonical_db
                .query("SELECT id FROM canonical_event WHERE title = $t AND start_at = <datetime>$st LIMIT 1;")
                .bind(("t", title_owned.clone()))
                .bind(("st", st.clone()))
                .await
                .map_err(|e| DatabaseError::Query(format!("Lookup failed: {e}")))?;
            self.trace("SELECT canonical_event (title,start_at)","SELECT id FROM canonical_event WHERE title = $t AND start_at = <datetime>$st LIMIT 1;");
            record_query(
                "SELECT canonical_event by (title,start_at)",
                start.elapsed().as_millis(),
            );
            if let Ok::<Vec<Thing>, _>(ids) = res.take(0) {
                if let Some(thing) = ids.first() {
                    let mut q = self
                        .canonical_db
                        .query("UPDATE $id SET title = $t, description = $desc, start_at = <datetime>$st, updated_at = time::now()")
                        .bind(("id", thing.clone()))
                        .bind(("t", title_owned.clone()))
                        .bind(("desc", title_owned.clone()))
                        .bind(("st", st.clone()));
                    self.trace("UPDATE canonical_event core","UPDATE $id SET title = $t, description = $desc, start_at = <datetime>$st, updated_at = time::now()");
                    if let Some(et) = end_owned.as_ref() {
                        q = q.bind(("et", et.clone()));

                        self.canonical_db
                            .query("UPDATE $id SET end_at = <datetime>$et")
                            .bind(("id", thing.clone()))
                            .bind(("et", et.clone()))
                            .await
                            .map_err(|e| {
                                DatabaseError::Query(format!("Update end_at failed: {e}"))
                            })?;
                        self.trace(
                            "UPDATE canonical_event end_at",
                            "UPDATE $id SET end_at = <datetime>$et",
                        );
                    }

                    if let Some(prov) = extra_clone_for_update {
                        self.canonical_db
                            .query("UPDATE $id SET provenance = $prov")
                            .bind(("id", thing.clone()))
                            .bind(("prov", prov.clone()))
                            .await
                            .map_err(|e| {
                                DatabaseError::Query(format!("Update provenance failed: {e}"))
                            })?;
                        self.trace(
                            "UPDATE canonical_event provenance",
                            "UPDATE $id SET provenance = $prov",
                        );

                        let _ = self
                            .set_object_field_force(thing, "provenance", &prov)
                            .await;

                        if let Ok(mut chk) = self
                            .canonical_db
                            .query("SELECT provenance FROM $id")
                            .bind(("id", thing.clone()))
                            .await
                        {
                            if let Ok::<Vec<serde_json::Value>, _>(rows) = chk.take(0) {
                                let empty_now = rows
                                    .first()
                                    .and_then(|r| r.get("provenance"))
                                    .map(|p| p == &serde_json::json!({}))
                                    .unwrap_or(true);
                                if empty_now {
                                    if let Some(obj) = prov.as_object() {
                                        if let Some(src) = obj.get("source") {
                                            let _ = self
                                                .canonical_db
                                                .query("UPDATE $id SET provenance.source = $v")
                                                .bind(("id", thing.clone()))
                                                .bind(("v", src.clone()))
                                                .await;
                                        }
                                        if let Some(rh) = obj.get("reasoning_hash") {
                                            let _ = self
                                                .canonical_db
                                                .query(
                                                    "UPDATE $id SET provenance.reasoning_hash = $v",
                                                )
                                                .bind(("id", thing.clone()))
                                                .bind(("v", rh.clone()))
                                                .await;
                                        }
                                        if let Some(parts) = obj.get("participants") {
                                            let _ = self
                                                .canonical_db
                                                .query(
                                                    "UPDATE $id SET provenance.participants = $v",
                                                )
                                                .bind(("id", thing.clone()))
                                                .bind(("v", parts.clone()))
                                                .await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    q.await
                        .map_err(|e| DatabaseError::Query(format!("Update failed: {e}")))?;
                    debug!(id = %thing, "StructuredStore: updated canonical_event");
                    return Ok(thing.clone());
                }
            }
        }

        let e_val = extra.unwrap_or(serde_json::json!({}));
        let start = Instant::now();
        let mut res = {
            match (start_owned.as_ref(), end_owned.as_ref()) {
        (Some(st), Some(et)) => {
                    self.canonical_db
            .query("CREATE canonical_event SET title = $t, description = $desc, start_at = <datetime>$st, end_at = <datetime>$et, location = $loc, provenance = $e, created_at = time::now() RETURN AFTER")
                        .bind(("t", title_owned.clone()))
                        .bind(("desc", title_owned.clone()))
                        .bind(("st", st.clone()))
                        .bind(("et", et.clone()))
                        .bind(("loc", loc_owned.clone()))
                        .bind(("e", e_val.clone()))
                        .await
                        .map_err(|e| DatabaseError::Query(format!("Create failed: {e}")))?
                }
                (Some(st), None) => {
                    self.canonical_db
            .query("CREATE canonical_event SET title = $t, description = $desc, start_at = <datetime>$st, location = $loc, provenance = $e, created_at = time::now() RETURN AFTER")
                        .bind(("t", title_owned.clone()))
                        .bind(("desc", title_owned.clone()))
                        .bind(("st", st.clone()))
                        .bind(("loc", loc_owned.clone()))
                        .bind(("e", e_val.clone()))
                        .await
                        .map_err(|e| DatabaseError::Query(format!("Create failed: {e}")))?
                }
                (None, Some(et)) => {
                    self.canonical_db
            .query("CREATE canonical_event SET title = $t, description = $desc, end_at = <datetime>$et, location = $loc, provenance = $e, created_at = time::now() RETURN AFTER")
                        .bind(("t", title_owned.clone()))
                        .bind(("desc", title_owned.clone()))
                        .bind(("et", et.clone()))
                        .bind(("loc", loc_owned.clone()))
                        .bind(("e", e_val.clone()))
                        .await
                        .map_err(|e| DatabaseError::Query(format!("Create failed: {e}")))?
                }
                (None, None) => {
                    self.canonical_db
            .query("CREATE canonical_event SET title = $t, description = $desc, location = $loc, provenance = $e, created_at = time::now() RETURN AFTER")
                        .bind(("t", title_owned.clone()))
                        .bind(("desc", title_owned.clone()))
                        .bind(("loc", loc_owned.clone()))
                        .bind(("e", e_val.clone()))
                        .await
                        .map_err(|e| DatabaseError::Query(format!("Create failed: {e}")))?
                }
            }
        };
        self.trace(
            "CREATE canonical_event",
            "CREATE canonical_event SET ... provenance = $e ... RETURN AFTER",
        );
        record_query("CREATE canonical_event", start.elapsed().as_millis());
        let created: Vec<CreatedWithId> = res.take(0).unwrap_or_default();
        if let Some(first) = created.first() {
            {
                use crate::provenance::{context, dag::ProvDag};
                let prov_ctx = context::global();
                let meta = prov_ctx.snapshot().await;
                if meta.session_id.is_some() {
                    let dag = ProvDag::new(self);
                    let _ = dag.ensure_schema().await;
                    if let Ok(exec_id) = dag
                        .record_execution(
                            meta.session_id.as_deref().unwrap(),
                            meta.flow_id.as_deref(),
                            meta.theatre_id.as_deref(),
                            meta.block_ids.last().map(|s| s.as_str()),
                        )
                        .await
                    {
                        let _ = dag.record_commit(&exec_id, &first.id).await;
                    }
                }
            }

            info!(id = %first.id, "StructuredStore: created canonical_event");
            if !e_val.is_null() && e_val != serde_json::json!({}) {
                #[cfg(feature = "provenance_debug")]
                {
                    if let Ok(mut dbg_res) = self
                        .canonical_db
                        .query("SELECT provenance FROM $id")
                        .bind(("id", first.id.clone()))
                        .await
                    {
                        if let Ok::<Vec<serde_json::Value>, _>(dbg_rows) = dbg_res.take(0) {
                            if let Some(row) = dbg_rows.first() {
                                if let Some(rawp) = row.get("provenance") {
                                    tracing::debug!(id = %first.id, raw_create_provenance = %rawp, intended = %e_val, "Debug: provenance immediately after CREATE");
                                }
                            }
                        }
                    }
                }
                if let Ok(mut chk) = self
                    .canonical_db
                    .query("SELECT provenance FROM $id")
                    .bind(("id", first.id.clone()))
                    .await
                {
                    if let Ok::<Vec<serde_json::Value>, _>(rows) = chk.take(0) {
                        let empty_now = rows
                            .first()
                            .and_then(|r| r.get("provenance"))
                            .map(|p| p == &serde_json::json!({}))
                            .unwrap_or(true);
                        if empty_now {
                            warn!(id=%first.id, "Provenance empty after create; forcing rewrite");
                            let _ = self
                                .set_object_field_force(&first.id, "provenance", &e_val)
                                .await;

                            if let Ok(mut chk2) = self
                                .canonical_db
                                .query("SELECT provenance FROM $id")
                                .bind(("id", first.id.clone()))
                                .await
                            {
                                if let Ok::<Vec<serde_json::Value>, _>(rows2) = chk2.take(0) {
                                    let still_empty = rows2
                                        .first()
                                        .and_then(|r| r.get("provenance"))
                                        .map(|p| p == &serde_json::json!({}))
                                        .unwrap_or(true);
                                    if still_empty {
                                        if let Some(obj) = e_val.as_object() {
                                            if let Some(src) = obj.get("source") {
                                                let _ = self
                                                    .canonical_db
                                                    .query("UPDATE $id SET provenance.source = $v")
                                                    .bind(("id", first.id.clone()))
                                                    .bind(("v", src.clone()))
                                                    .await;
                                            }
                                            if let Some(rh) = obj.get("reasoning_hash") {
                                                let _ = self.canonical_db
                                                    .query("UPDATE $id SET provenance.reasoning_hash = $v")
                                                    .bind(("id", first.id.clone()))
                                                    .bind(("v", rh.clone()))
                                                    .await;
                                            }
                                            if let Some(parts) = obj.get("participants") {
                                                let _ = self.canonical_db
                                                    .query("UPDATE $id SET provenance.participants = $v")
                                                    .bind(("id", first.id.clone()))
                                                    .bind(("v", parts.clone()))
                                                    .await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(spec) = provenance_value_to_spec(&e_val) {
                    let writer = crate::provenance::ProvenanceWriter::new(self);
                    if let Err(e) = writer.apply_to_event(&first.id, &spec).await {
                        tracing::warn!(error = %e, id = %first.id, "ProvenanceWriter failed (create path)");
                    }

                    let dual_payload = serde_json::json!({
                        "event_id": first.id.to_string(),
                        "payload": e_val.clone()
                    });
                    if let Err(e) = self
                        .create_provenance_event("canonical_event_provenance", dual_payload)
                        .await
                    {
                        tracing::warn!(event_id = %first.id, error = %e, "Dual provenance_event create failed");
                    } else {
                        tracing::debug!(event_id = %first.id, "Dual provenance_event created");
                    }
                } else {
                    tracing::warn!(id = %first.id, "ProvenanceWriter: supplied provenance lacked required shape");
                }
            }
            return Ok(first.id.clone());
        }

        if let Some(st) = start_owned.as_ref() {
            let mut verify = self
                .canonical_db
                .query("SELECT id, created_at FROM canonical_event WHERE title = $t AND start_at = <datetime>$st ORDER BY created_at DESC LIMIT 1;")
                .bind(("t", title_owned.clone()))
                .bind(("st", st.clone()))
                .await
                .map_err(|e| DatabaseError::Query(format!("Post-create lookup failed: {e}")))?;
            if let Ok::<Vec<Thing>, _>(rows2) = verify.take(0) {
                if let Some(id) = rows2.first() {
                    return Ok(id.clone());
                }
            }
        } else {
            let mut verify = self
                .canonical_db
                .query("SELECT id, created_at FROM canonical_event WHERE title = $t ORDER BY created_at DESC LIMIT 1;")
                .bind(("t", title_owned.clone()))
                .await
                .map_err(|e| DatabaseError::Query(format!("Post-create lookup (title-only) failed: {e}")))?;
            if let Ok::<Vec<Thing>, _>(rows2) = verify.take(0) {
                if let Some(id) = rows2.first() {
                    return Ok(id.clone());
                }
            }
        }
        Err(DatabaseError::Query(
            "No ID returned from create (event)".into(),
        ))
    }

    pub async fn create_relationship_fact(
        &self,
        subject: &Thing,
        predicate: &str,
        object: &Thing,
        confidence: Option<f32>,
        provenance: Option<&str>,
    ) -> Result<Thing, DatabaseError> {
        let pred = predicate.to_string();
        let prov = provenance.map(|s| s.to_string());
        let start = Instant::now();
        let mut res = self
            .canonical_db
            .query("CREATE canonical_relationship_fact SET subject_ref = $s, predicate = $p, object_ref = $o, confidence = $c, provenance = $v, version_no = 1, valid_from = time::now(), tx_from = time::now(), created_at = time::now() RETURN AFTER")
            .bind(("s", subject.clone()))
            .bind(("p", pred.clone()))
            .bind(("o", object.clone()))
            .bind(("c", confidence))
            .bind(("v", prov.clone()))
            .await
            .map_err(|e| DatabaseError::Query(format!("Create failed: {e}")))?;
        record_query(
            "CREATE canonical_relationship_fact",
            start.elapsed().as_millis(),
        );
        let created: Vec<CreatedWithId> = res.take(0).unwrap_or_default();
        if let Some(first) = created.first() {
            info!(id = %first.id, "StructuredStore: created canonical_relationship_fact");

            if let Some(conf) = confidence {
                let _ = conf;
            }
            if let Some(flag) = crate::policy::anomaly::record_fact(
                &pred,
                None,
                &crate::policy::anomaly::AnomalyConfig::default(),
            ) {
                tracing::warn!(target: "stele::anomaly", predicate = %flag.predicate, count = flag.count, baseline = flag.baseline_avg, ratio = flag.ratio, "Fact burst anomaly detected");
            }
            return Ok(first.id.clone());
        }

        let mut verify = self
            .canonical_db
            .query("SELECT id, created_at FROM canonical_relationship_fact WHERE subject_ref = $s AND predicate = $p AND object_ref = $o ORDER BY created_at DESC LIMIT 1;")
            .bind(("s", subject.clone()))
            .bind(("p", pred))
            .bind(("o", object.clone()))
            .await
            .map_err(|e| DatabaseError::Query(format!("Post-create lookup failed: {e}")))?;
        if let Ok::<Vec<Thing>, _>(rows2) = verify.take(0) {
            if let Some(id) = rows2.first() {
                return Ok(id.clone());
            }
        }
        Err(DatabaseError::Query(
            "No ID returned from create (relationship_fact)".into(),
        ))
    }

    pub async fn get_event_provenance(
        &self,
        event: &Thing,
    ) -> Result<Option<Value>, DatabaseError> {
        let mut q = self
            .canonical_db
            .query("SELECT provenance FROM $id LIMIT 1;")
            .bind(("id", event.clone()))
            .await
            .map_err(|e| DatabaseError::Query(format!("provenance select failed: {e}")))?;
        let rows: Vec<Value> = q.take(0).unwrap_or_default();
        let direct = rows.first().and_then(|r| r.get("provenance")).cloned();
        let is_empty = direct
            .as_ref()
            .map(|v| v == &Value::Object(Default::default()))
            .unwrap_or(true);
        if !is_empty {
            return Ok(direct);
        }
        {
            let mut dq = self.dyn_db()
                .query("SELECT details, created_at FROM provenance_event WHERE kind = 'canonical_event_provenance' AND details.event_id = $eid ORDER BY created_at DESC LIMIT 1;")
                .bind(("eid", event.to_string()))
                .await
                .map_err(|e| DatabaseError::Query(format!("dual provenance lookup failed: {e}")))?;
            let drows: Vec<Value> = dq.take(0).unwrap_or_default();
            if let Some(first) = drows.first() {
                if let Some(details) = first.get("details") {
                    if let Some(payload) = details.get("payload") {
                        return Ok(Some(payload.clone()));
                    }
                    return Ok(Some(details.clone()));
                }
            }
        }
        Ok(None)
    }

    pub async fn list_relationship_facts(
        &self,
        predicate: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Value, DatabaseError> {
        let lim = limit.unwrap_or(50) as i64;
        let pred_owned = predicate.map(|p| p.to_string());
        let base_query = if pred_owned.is_some() {
            "SELECT predicate FROM canonical_relationship_fact WHERE predicate = $p LIMIT $l"
        } else {
            "SELECT predicate FROM canonical_relationship_fact LIMIT $l"
        };
        let mut qb = self.canonical_db.query(base_query).bind(("l", lim));
        if let Some(p) = pred_owned.as_ref() {
            qb = qb.bind(("p", p.clone()));
        }
        let mut q = qb.await.map_err(|e| {
            DatabaseError::Query(format!(
                "list_relationship_facts base projection failed: {e}"
            ))
        })?;
        let rows: Vec<Value> = q.take(0).unwrap_or_default();

        let mut results: Vec<Value> = Vec::new();
        if rows.iter().all(|r| r.get("id").is_none()) {
            let id_query = if pred_owned.is_some() {
                "SELECT id FROM canonical_relationship_fact WHERE predicate = $p LIMIT $l"
            } else {
                "SELECT id FROM canonical_relationship_fact LIMIT $l"
            };
            let mut idqb = self.canonical_db.query(id_query).bind(("l", lim));
            if let Some(p) = pred_owned.as_ref() {
                idqb = idqb.bind(("p", p.clone()));
            }
            if let Ok(mut iq) = idqb.await {
                if let Ok::<Vec<Value>, _>(id_rows) = iq.take(0) {
                    for (idx, id_row) in id_rows.into_iter().enumerate() {
                        let obj = serde_json::json!({"id": id_row.get("id").cloned().unwrap_or(Value::Null), "predicate": rows.get(idx).and_then(|r| r.get("predicate")).cloned().unwrap_or(Value::Null)});
                        results.push(obj);
                    }
                }
            }
        } else {
            results = rows;
        }

        for item in results.iter_mut() {
            if let Some(id_val) = item.get("id") {
                if let Some(id_str) = id_val.as_str() {
                    if let Ok(thing) = id_str.parse::<Thing>() {
                        if let Ok(mut eq) = self
                            .canonical_db
                            .query("SELECT subject_ref, object_ref, confidence FROM $id")
                            .bind(("id", thing))
                            .await
                        {
                            if let Ok::<Vec<Value>, _>(erows) = eq.take(0) {
                                if let Some(er) = erows.first() {
                                    if let Some(sr) = er.get("subject_ref") {
                                        item.as_object_mut()
                                            .unwrap()
                                            .insert("subject_ref".into(), sr.clone());
                                    }
                                    if let Some(or) = er.get("object_ref") {
                                        item.as_object_mut()
                                            .unwrap()
                                            .insert("object_ref".into(), or.clone());
                                    }
                                    if let Some(cf) = er.get("confidence") {
                                        item.as_object_mut()
                                            .unwrap()
                                            .insert("confidence".into(), cf.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(Value::Array(results))
    }

    #[allow(dead_code)]
    pub async fn supersede_relationship_fact(
        &self,
        existing: &Thing,
        new_confidence: Option<f32>,
        provenance: Option<&str>,
        branch_id: Option<&str>,
    ) -> Result<Thing, DatabaseError> {
        let mut q = self
            .canonical_db
            .query("SELECT id, version_no FROM $id LIMIT 1;")
            .bind(("id", existing.clone()))
            .await
            .map_err(|e| DatabaseError::Query(format!("version lookup failed: {e}")))?;
        let rows: Vec<serde_json::Value> = q.take(0).unwrap_or_default();
        let current_version = rows
            .first()
            .and_then(|v| v.get("version_no").and_then(|n| n.as_i64()))
            .unwrap_or(1) as i64;

        self.canonical_db
            .query("UPDATE $id SET tx_to = time::now()")
            .bind(("id", existing.clone()))
            .await
            .map_err(|e| DatabaseError::Query(format!("close prev version failed: {e}")))?;

        let mut fetch = self
            .canonical_db
            .query("SELECT subject_ref, predicate, object_ref FROM $id LIMIT 1;")
            .bind(("id", existing.clone()))
            .await
            .map_err(|e| DatabaseError::Query(format!("fetch prev content failed: {e}")))?;
        #[derive(serde::Deserialize)]
        struct PrevCore {
            subject_ref: Thing,
            predicate: String,
            object_ref: Thing,
        }
        let prev: Vec<PrevCore> = fetch.take(0).unwrap_or_default();
        let core = prev
            .first()
            .ok_or_else(|| DatabaseError::Query("prev core missing".into()))?;
        let mut create = self.canonical_db
            .query("CREATE canonical_relationship_fact SET subject_ref=$s, predicate=$p, object_ref=$o, confidence=$c, provenance=$v, version_no=$vn, supersedes=$sup, valid_from=time::now(), tx_from=time::now(), branch_id=$branch, branch_status=IF $branch != NONE THEN 'hypothesis' ELSE NONE END RETURN AFTER")
            .bind(("s", core.subject_ref.clone()))
            .bind(("p", core.predicate.clone()))
            .bind(("o", core.object_ref.clone()))
            .bind(("c", new_confidence))
            .bind(("v", provenance.map(|s| s.to_string())))
            .bind(("vn", current_version + 1))
            .bind(("sup", existing.clone()))
            .bind(("branch", branch_id.map(|b| b.to_string())))
            .await
            .map_err(|e| DatabaseError::Query(format!("create new version failed: {e}")))?;
        let created: Vec<CreatedWithId> = create.take(0).unwrap_or_default();
        created
            .first()
            .map(|c| c.id.clone())
            .ok_or_else(|| DatabaseError::Query("new version create returned no id".into()))
    }

    #[allow(dead_code)]
    pub async fn rollback_relationship_fact_version(
        &self,
        latest: &Thing,
    ) -> Result<(), DatabaseError> {
        self.canonical_db
            .query("UPDATE $id SET tx_to = time::now(), branch_status = IF branch_status = 'hypothesis' THEN 'rejected' ELSE branch_status END")
            .bind(("id", latest.clone()))
            .await
            .map_err(|e| DatabaseError::Query(format!("rollback close failed: {e}")))?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn promote_branch(
        &self,
        branch: &str,
        clear_branch: bool,
    ) -> Result<u64, DatabaseError> {
        let update_q = if clear_branch {
            "UPDATE canonical_relationship_fact SET branch_status = 'active', branch_id = NONE WHERE branch_id = $b AND (branch_status = 'hypothesis' OR branch_status = NONE);"
        } else {
            "UPDATE canonical_relationship_fact SET branch_status = 'active' WHERE branch_id = $b AND branch_status = 'hypothesis';"
        };
        let mut res = self
            .canonical_db
            .query(update_q)
            .bind(("b", branch.to_string()))
            .await
            .map_err(|e| DatabaseError::Query(format!("branch promote failed: {e}")))?;
        let count: Option<u64> = res.take(0).ok().and_then(|v: Vec<serde_json::Value>| {
            v.first()
                .and_then(|val| val.get("count").and_then(|c| c.as_u64()))
        });
        Ok(count.unwrap_or(0))
    }

    #[allow(dead_code)]
    pub async fn get_relationship_facts_as_of(
        &self,
        subject: Option<&Thing>,
        predicate: Option<&str>,
        object: Option<&Thing>,
        valid_time_at: Option<&str>,
        tx_time_at: Option<&str>,
        branch_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, DatabaseError> {
        let mut conditions: Vec<String> = Vec::new();
        if subject.is_some() {
            conditions.push("subject_ref = $subject".into());
        }
        if let Some(p) = predicate {
            if !p.is_empty() {
                conditions.push("predicate = $predicate".into());
            }
        }
        if object.is_some() {
            conditions.push("object_ref = $object".into());
        }
        if let Some(ts) = valid_time_at {
            if !ts.is_empty() {
                conditions.push("valid_from <= <datetime>$valid_time_at AND (valid_to = NONE OR valid_to > <datetime>$valid_time_at)".into());
            }
        }
        if let Some(ts) = tx_time_at {
            if !ts.is_empty() {
                conditions.push("tx_from <= <datetime>$tx_time_at AND (tx_to = NONE OR tx_to > <datetime>$tx_time_at)".into());
            }
        }
        if let Some(b) = branch_id {
            if !b.is_empty() {
                conditions.push("branch_id = $branch_id".into());
            }
        }

        if branch_id.is_none() {
            conditions.push("(branch_id = NONE OR branch_status = 'active')".into());
        }
        let where_clause = if conditions.is_empty() {
            "".to_string()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let query = format!("SELECT id, subject_ref, predicate, object_ref, valid_from, valid_to, tx_from, tx_to, version_no, supersedes, branch_id, branch_status, confidence, provenance FROM canonical_relationship_fact {where_clause} ORDER BY valid_from ASC, version_no ASC;");
        let mut q = self
            .canonical_db
            .query(query)
            .bind(("subject", subject.cloned()))
            .bind(("object", object.cloned()))
            .bind(("predicate", predicate.map(|p| p.to_string())))
            .bind(("valid_time_at", valid_time_at.map(|s| s.to_string())))
            .bind(("tx_time_at", tx_time_at.map(|s| s.to_string())))
            .bind(("branch_id", branch_id.map(|s| s.to_string())))
            .await
            .map_err(|e| DatabaseError::Query(format!("bitemporal fact query failed: {e}")))?;
        let rows: Vec<serde_json::Value> = q.take(0).unwrap_or_default();
        Ok(rows)
    }

    pub async fn get_current_relationship_facts(
        &self,
        subject: Option<&Thing>,
        predicate: Option<&str>,
        object: Option<&Thing>,
        branch_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, DatabaseError> {
        self.get_relationship_facts_as_of(subject, predicate, object, None, None, branch_id)
            .await
    }

    pub async fn relate_node_to_canonical_entity(
        &self,
        node: &RecordId,
        canonical: &Thing,
    ) -> Result<(), DatabaseError> {
        if !self.same_database {
            let cref = self
                .upsert_canonical_ref("entity", canonical, None, None, None)
                .await?;
            self.dyn_db()
                .query("RELATE $node->canonical_of->$cref")
                .bind(("node", node.clone()))
                .bind(("cref", cref))
                .await
                .map_err(|e| {
                    DatabaseError::Query(format!("Relate node→canonical_ref failed: {e}"))
                })?;
            return Ok(());
        }
        self.dyn_db()
            .query("RELATE $node->canonical_of_entity->$can")
            .bind(("node", node.clone()))
            .bind(("can", canonical.clone()))
            .await
            .map_err(|e| {
                DatabaseError::Query(format!("Relate node→canonical_entity failed: {e}"))
            })?;
        Ok(())
    }

    pub async fn relate_node_to_canonical_task(
        &self,
        node: &RecordId,
        canonical: &Thing,
    ) -> Result<(), DatabaseError> {
        if !self.same_database {
            let cref = self
                .upsert_canonical_ref("task", canonical, None, None, None)
                .await?;
            self.dyn_db()
                .query("RELATE $node->canonical_of->$cref")
                .bind(("node", node.clone()))
                .bind(("cref", cref))
                .await
                .map_err(|e| {
                    DatabaseError::Query(format!("Relate node→canonical_ref failed: {e}"))
                })?;
            return Ok(());
        }
        self.dyn_db()
            .query("RELATE $node->canonical_of_task->$can")
            .bind(("node", node.clone()))
            .bind(("can", canonical.clone()))
            .await
            .map_err(|e| DatabaseError::Query(format!("Relate node→canonical_task failed: {e}")))?;
        Ok(())
    }

    pub async fn relate_node_to_canonical_event(
        &self,
        node: &RecordId,
        canonical: &Thing,
    ) -> Result<(), DatabaseError> {
        if !self.same_database {
            let cref = self
                .upsert_canonical_ref("event", canonical, None, None, None)
                .await?;
            self.dyn_db()
                .query("RELATE $node->canonical_of->$cref")
                .bind(("node", node.clone()))
                .bind(("cref", cref))
                .await
                .map_err(|e| {
                    DatabaseError::Query(format!("Relate node→canonical_ref failed: {e}"))
                })?;
            return Ok(());
        }
        self.dyn_db()
            .query("RELATE $node->canonical_of_event->$can")
            .bind(("node", node.clone()))
            .bind(("can", canonical.clone()))
            .await
            .map_err(|e| {
                DatabaseError::Query(format!("Relate node→canonical_event failed: {e}"))
            })?;
        Ok(())
    }

    pub async fn relate_utterance_to_canonical_entity(
        &self,
        utterance: &RecordId,
        canonical: &Thing,
    ) -> Result<(), DatabaseError> {
        if !self.same_database {
            let cref = self
                .upsert_canonical_ref("entity", canonical, None, None, None)
                .await?;
            self.dyn_db()
                .query("RELATE $utt->utterance_mentions->$cref")
                .bind(("utt", utterance.clone()))
                .bind(("cref", cref))
                .await
                .map_err(|e| {
                    DatabaseError::Query(format!("Relate utterance→canonical_ref failed: {e}"))
                })?;
            return Ok(());
        }
        self.dyn_db()
            .query("RELATE $utt->utterance_mentions_entity->$can")
            .bind(("utt", utterance.clone()))
            .bind(("can", canonical.clone()))
            .await
            .map_err(|e| {
                DatabaseError::Query(format!("Relate utterance→canonical_entity failed: {e}"))
            })?;
        Ok(())
    }

    pub async fn relate_utterance_to_canonical_task(
        &self,
        utterance: &RecordId,
        canonical: &Thing,
    ) -> Result<(), DatabaseError> {
        if !self.same_database {
            let cref = self
                .upsert_canonical_ref("task", canonical, None, None, None)
                .await?;
            self.dyn_db()
                .query("RELATE $utt->utterance_mentions->$cref")
                .bind(("utt", utterance.clone()))
                .bind(("cref", cref))
                .await
                .map_err(|e| {
                    DatabaseError::Query(format!(
                        "Relate utterance→canonical_ref (task) failed: {e}"
                    ))
                })?;
            return Ok(());
        }
        self.dyn_db()
            .query("RELATE $utt->utterance_mentions_task->$can")
            .bind(("utt", utterance.clone()))
            .bind(("can", canonical.clone()))
            .await
            .map_err(|e| {
                DatabaseError::Query(format!("Relate utterance→canonical_task failed: {e}"))
            })?;
        Ok(())
    }

    pub async fn relate_utterance_to_canonical_event(
        &self,
        utterance: &RecordId,
        canonical: &Thing,
    ) -> Result<(), DatabaseError> {
        if !self.same_database {
            let cref = self
                .upsert_canonical_ref("event", canonical, None, None, None)
                .await?;
            self.dyn_db()
                .query("RELATE $utt->utterance_mentions->$cref")
                .bind(("utt", utterance.clone()))
                .bind(("cref", cref))
                .await
                .map_err(|e| {
                    DatabaseError::Query(format!("Relate utterance→canonical_ref failed: {e}"))
                })?;
            return Ok(());
        }
        self.dyn_db()
            .query("RELATE $utt->utterance_mentions_event->$can")
            .bind(("utt", utterance.clone()))
            .bind(("can", canonical.clone()))
            .await
            .map_err(|e| {
                DatabaseError::Query(format!("Relate utterance→canonical_event failed: {e}"))
            })?;
        Ok(())
    }

    pub async fn create_provenance_event(
        &self,
        kind: &str,
        details: Value,
    ) -> Result<Thing, DatabaseError> {
        let k = kind.to_string();
        let start = Instant::now();
        let mut res = self
            .dyn_db()
            .query("CREATE provenance_event SET kind = $k, details = $d, created_at = time::now() RETURN AFTER")
            .bind(("k", k))
            .bind(("d", details))
            .await
            .map_err(|e| DatabaseError::Query(format!("Provenance create failed: {e}")))?;
        record_query("CREATE provenance_event", start.elapsed().as_millis());
        let created: Vec<CreatedWithId> = res.take(0).unwrap_or_default();
        if let Some(first) = created.first() {
            return Ok(first.id.clone());
        }
        Err(DatabaseError::Query(
            "No ID returned from create (provenance_event)".into(),
        ))
    }

    pub async fn relate_utterance_to_provenance(
        &self,
        utterance: &RecordId,
        prov: &Thing,
    ) -> Result<(), DatabaseError> {
        self.dyn_db()
            .query("RELATE $utt->utterance_has_provenance->$prov")
            .bind(("utt", utterance.clone()))
            .bind(("prov", prov.clone()))
            .await
            .map_err(|e| {
                DatabaseError::Query(format!("Relate utterance→provenance_event failed: {e}"))
            })?;
        Ok(())
    }
}

#[cfg(any(test, feature = "nlu_builders", feature = "provenance_debug"))]
impl StructuredStore {
    pub async fn test_fetch_canonical_entities_flat(
        &self,
    ) -> Result<Vec<serde_json::Value>, DatabaseError> {
        let mut q = self
            .canonical_db
            .query("SELECT * FROM canonical_entity LIMIT 50;")
            .await
            .map_err(|e| DatabaseError::Query(format!("test fetch failed: {e}")))?;

        if let Ok::<Vec<serde_json::Value>, _>(rows) = q.take(0) {
            if !rows.is_empty() {
                return Ok(rows);
            }
        }
        use surrealdb::Value as RawVal;
        if let Ok(Some(rv)) = q.take::<Option<RawVal>>(0) {
            let js = rv.to_string();
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&js) {
                let mut out = Vec::new();
                fn rec(v: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
                    match v {
                        serde_json::Value::Array(a) => {
                            for x in a {
                                rec(x, out);
                            }
                        }
                        serde_json::Value::Object(_) => out.push(v.clone()),
                        _ => {}
                    }
                }
                rec(&val, &mut out);
                return Ok(out);
            }
        }
        Ok(Vec::new())
    }

    pub async fn test_fetch_relationship_facts_flat(
        &self,
        predicate: &str,
    ) -> Result<Vec<serde_json::Value>, DatabaseError> {
        let p_owned = predicate.to_string();
        let mut q = self
            .canonical_db
            .query("SELECT * FROM canonical_relationship_fact WHERE predicate = $p LIMIT 25;")
            .bind(("p", p_owned))
            .await
            .map_err(|e| DatabaseError::Query(format!("rel facts test fetch failed: {e}")))?;
        if let Ok::<Vec<serde_json::Value>, _>(rows) = q.take(0) {
            if !rows.is_empty() {
                return Ok(rows);
            }
        }
        use surrealdb::Value as RawVal;
        if let Ok(Some(rv)) = q.take::<Option<RawVal>>(0) {
            let js = rv.to_string();
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&js) {
                let mut out = Vec::new();
                fn rec(v: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
                    match v {
                        serde_json::Value::Array(a) => {
                            for x in a {
                                rec(x, out);
                            }
                        }
                        serde_json::Value::Object(_) => out.push(v.clone()),
                        _ => {}
                    }
                }
                rec(&val, &mut out);
                if !out.is_empty() {
                    return Ok(out);
                }
            }
        }
        Ok(Vec::new())
    }
}

fn provenance_value_to_spec(v: &Value) -> Option<provenance::ProvenanceSpec> {
    let source = v
        .get("source")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown")
        .to_string();
    let reasoning_hash = v
        .get("reasoning_hash")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());
    let participant_names = v
        .get("participants")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let utterance_ids = v
        .get("utterance_ids")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let extra = v
        .get("extra")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    Some(provenance::ProvenanceSpec {
        source,
        reasoning_hash,
        participant_names,
        utterance_ids,
        extra,
    })
}
