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
use surrealdb::sql::Thing;

use crate::database::structured_store::StructuredStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionEventRec {
    pub id: Thing,
    pub session_id: String,
    pub flow_id: Option<String>,
    pub theatre_id: Option<String>,
    pub block_id: Option<String>,
    pub created_at: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitEventRec {
    pub id: Thing,
    pub event_id: Thing,
    pub created_at: serde_json::Value,
}

pub struct ProvDag<'a> {
    store: &'a StructuredStore,
}

impl<'a> ProvDag<'a> {
    pub fn new(store: &'a StructuredStore) -> Self {
        Self { store }
    }

    fn trace_enabled() -> bool {
        std::env::var("STELE_PROV_TRACE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    }

    fn tlog(label: &str, msg: &str) {
        if Self::trace_enabled() {
            eprintln!("[prov][{label}] {msg}");
        }
    }

    pub async fn ensure_schema(&self) -> Result<(), crate::database::types::DatabaseError> {
        let db = self.store.canonical_db();
        // Load provenance schema from config SQL file to align with canonical schemas
        let schema: &str = include_str!("../database/config/provenance_schema.sql");
        let mut applied = 0usize;
        for part in schema.split(';') {
            let stmt = part.trim();
            if stmt.is_empty() {
                continue;
            }
            let sql = format!("{stmt};");
            Self::tlog(
                "schema",
                &format!("apply: {}", stmt.lines().next().unwrap_or("")),
            );
            let _ = db.query(&sql).await; // ignore errors if already defined
            applied += 1;
        }
        Self::tlog("schema", &format!("applied {applied} statements"));
        Ok(())
    }

    pub async fn record_execution(
        &self,
        session_id: &str,
        flow_id: Option<&str>,
        theatre_id: Option<&str>,
        block_id: Option<&str>,
    ) -> Result<Thing, crate::database::types::DatabaseError> {
        let mut res = self
            .store
            .canonical_db()
            .query(
                "CREATE execution_event SET session_id=$s, flow_id=$f, theatre_id=$t, block_id=$b, created_at=time::now() RETURN AFTER",
            )
            .bind(("s", session_id.to_string()))
            .bind(("f", flow_id.map(|v| v.to_string())))
            .bind(("t", theatre_id.map(|v| v.to_string())))
            .bind(("b", block_id.map(|v| v.to_string())))
            .await
            .map_err(|e| crate::database::types::DatabaseError::Query(format!(
                "exec event create failed: {e}"
            )))?;
        let created: Vec<ExecutionEventRec> = res.take(0).unwrap_or_default();
        if created.is_empty() {
            // dump table for debugging
            let mut dump = self
                .store
                .canonical_db()
                .query("SELECT * FROM execution_event")
                .await;
            if let Ok(q) = dump.as_mut() {
                let vals: Vec<serde_json::Value> = q.take(0).unwrap_or_default();
                Self::tlog(
                    "record_exec",
                    &format!(
                        "post-create dump: {}",
                        serde_json::to_string(&vals).unwrap_or_default()
                    ),
                );
            } else if let Err(e) = dump {
                Self::tlog("record_exec", &format!("dump err: {e}"));
            }
        }
        created
            .first()
            .map(|r| r.id.clone())
            .ok_or_else(|| crate::database::types::DatabaseError::Query("no exec id".into()))
    }

    pub async fn record_commit(
        &self,
        exec_id: &Thing,
        canonical_event_id: &Thing,
    ) -> Result<Thing, crate::database::types::DatabaseError> {
        // Ensure schema exists in case caller forgot
        let _ = self.ensure_schema().await;

        // Fetch session_id from the execution_event to denormalize into commit_event
        #[derive(serde::Deserialize)]
        struct ExecMeta {
            session_id: String,
        }
        let mut sidq = self
            .store
            .canonical_db()
            .query("SELECT session_id FROM execution_event WHERE id = $id LIMIT 1")
            .bind(("id", exec_id.clone()))
            .await
            .map_err(|e| {
                crate::database::types::DatabaseError::Query(format!(
                    "fetch exec session_id failed: {e}"
                ))
            })?;
        let sid_rows: Vec<ExecMeta> = sidq.take(0).unwrap_or_default();
        let session_id = sid_rows
            .first()
            .map(|r| r.session_id.clone())
            .unwrap_or_default();
        Self::tlog(
            "record_commit",
            &format!("exec_id={exec_id}, session_id={session_id} -> event_id={canonical_event_id}"),
        );

        // Try 1: CREATE ... SET ... RETURN id (simplest projection)
        if let Ok(mut res1) = self
            .store
            .canonical_db()
            .query(
                "CREATE commit_event SET event_id=$e, session_id=$s, exec_id=$x, created_at=time::now() RETURN id",
            )
            .bind(("e", canonical_event_id.clone()))
            .bind(("s", session_id.clone()))
            .bind(("x", exec_id.clone()))
            .await
        {
            let ids: Vec<Thing> = res1.take(0).unwrap_or_default();
            let len = ids.len();
            Self::tlog("record_commit", &format!("RETURN id rows: {len}"));
            if let Some(first) = ids.first() {
                let _ = self
                    .store
                    .canonical_db()
                    .query("RELATE $out->prov_edge->$in SET relation='execution_to_commit'")
                    .bind(("out", exec_id.clone()))
                    .bind(("in", first.clone()))
                    .await;
                Self::tlog("record_commit", &format!("linked edge for commit {first}"));
                if let Ok(mut qc) = self
                    .store
                    .canonical_db()
                    .query("SELECT count() AS c FROM commit_event")
                    .await
                {
                    let vals: Vec<serde_json::Value> = qc.take(0).unwrap_or_default();
                    let s = serde_json::to_string(&vals).unwrap_or_default();
                    Self::tlog("record_commit", &format!("post-link commit count: {s}"));
                }
                return Ok(first.clone());
            }
        }

        // Try 2: CREATE ... RETURN AFTER (id in object)
        #[derive(serde::Deserialize)]
        struct CreatedId {
            id: Thing,
        }
        if let Ok(mut res2) = self
            .store
            .canonical_db()
            .query(
                "CREATE commit_event SET event_id=$e, session_id=$s, exec_id=$x, created_at=time::now() RETURN AFTER",
            )
            .bind(("e", canonical_event_id.clone()))
            .bind(("s", session_id.clone()))
            .bind(("x", exec_id.clone()))
            .await
        {
            let rows: Vec<CreatedId> = res2.take(0).unwrap_or_default();
            Self::tlog("record_commit", &format!("RETURN AFTER rows: {}", rows.len()));
            if let Some(first) = rows.first() {
                let _ = self
                    .store
                    .canonical_db()
                    .query("RELATE $out->prov_edge->$in SET relation='execution_to_commit'")
                    .bind(("out", exec_id.clone()))
                    .bind(("in", first.id.clone()))
                    .await;
                Self::tlog(
                    "record_commit",
                    &format!("linked edge for commit {}", first.id),
                );
                if let Ok(mut qc) = self
                    .store
                    .canonical_db()
                    .query("SELECT count() AS c FROM commit_event")
                    .await
                {
                    let vals: Vec<serde_json::Value> = qc.take(0).unwrap_or_default();
                    Self::tlog(
                        "record_commit",
                        &format!(
                            "post-link commit count: {}",
                            serde_json::to_string(&vals).unwrap_or_default()
                        ),
                    );
                }
                return Ok(first.id.clone());
            }
        }

        // Try 3: CREATE CONTENT ... RETURN AFTER
        if let Ok(mut res3) = self
            .store
            .canonical_db()
            .query(
                "CREATE commit_event CONTENT { event_id: $e, session_id: $s, exec_id: $x, created_at: time::now() } RETURN AFTER",
            )
            .bind(("e", canonical_event_id.clone()))
            .bind(("s", session_id.clone()))
            .bind(("x", exec_id.clone()))
            .await
        {
            let rows: Vec<CreatedId> = res3.take(0).unwrap_or_default();
            let len = rows.len();
            Self::tlog("record_commit", &format!("CONTENT RETURN AFTER rows: {len}"));
            if let Some(first) = rows.first() {
                let _ = self
                    .store
                    .canonical_db()
                    .query("RELATE $out->prov_edge->$in SET relation='execution_to_commit'")
                    .bind(("out", exec_id.clone()))
                    .bind(("in", first.id.clone()))
                    .await;
                let id = &first.id;
                Self::tlog("record_commit", &format!("linked edge for commit {id}"));
                if let Ok(mut qc) = self
                    .store
                    .canonical_db()
                    .query("SELECT count() AS c FROM commit_event")
                    .await
                {
                    let vals: Vec<serde_json::Value> = qc.take(0).unwrap_or_default();
                    let s = serde_json::to_string(&vals).unwrap_or_default();
                    Self::tlog("record_commit", &format!("post-link commit count: {s}"));
                }
                return Ok(first.id.clone());
            }
        }

        // Fallback: Select the most recent commit for this event
        let mut verify = self
            .store
            .canonical_db()
            .query(
                "SELECT id, created_at FROM commit_event WHERE event_id = $e ORDER BY created_at DESC LIMIT 1;",
            )
            .bind(("e", canonical_event_id.clone()))
            .await
            .map_err(|e| {
                crate::database::types::DatabaseError::Query(format!(
                    "commit verify select failed: {e}"
                ))
            })?;
        let ids: Vec<Thing> = verify.take(0).unwrap_or_default();
        let len = ids.len();
        Self::tlog("record_commit", &format!("verify select ids: {len}"));
        if let Some(cid) = ids.first() {
            let _ = self
                .store
                .canonical_db()
                .query("RELATE $out->prov_edge->$in SET relation='execution_to_commit'")
                .bind(("out", exec_id.clone()))
                .bind(("in", cid.clone()))
                .await;
            Self::tlog("record_commit", &format!("linked edge for commit {cid}"));
            if let Ok(mut qc) = self
                .store
                .canonical_db()
                .query("SELECT count() AS c FROM commit_event")
                .await
            {
                let vals: Vec<serde_json::Value> = qc.take(0).unwrap_or_default();
                let s = serde_json::to_string(&vals).unwrap_or_default();
                Self::tlog("record_commit", &format!("post-link commit count: {s}"));
            }
            return Ok(cid.clone());
        }
        Err(crate::database::types::DatabaseError::Query(
            "no commit id".into(),
        ))
    }

    pub async fn query_commits_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<serde_json::Value>, crate::database::types::DatabaseError> {
        // Direct denormalized lookup for reliability across Surreal versions
        let sql = r#"SELECT id, event_id, created_at FROM commit_event WHERE session_id = $s"#;
        let mut q = self
            .store
            .canonical_db()
            .query(sql)
            .bind(("s", session_id.to_string()))
            .await
            .map_err(|e| {
                crate::database::types::DatabaseError::Query(format!(
                    "query commits for session failed: {e}"
                ))
            })?;
        let typed: Vec<CommitEventRec> = q.take(0).unwrap_or_default();
        let mut out = Vec::with_capacity(typed.len());
        for r in typed {
            out.push(serde_json::json!({
                "id": r.id.to_string(),
                "event_id": r.event_id.to_string(),
                "created_at": r.created_at,
            }));
        }
        Ok(out)
    }

    pub async fn query_exec_for_canonical_event(
        &self,
        event_id: &Thing,
    ) -> Result<Vec<serde_json::Value>, crate::database::types::DatabaseError> {
        // Step 1: execution ids via commit_event.exec_id for this event
        #[derive(Deserialize)]
        struct ExecIdRow {
            exec_id: Thing,
        }
        let mut q1 = self
            .store
            .canonical_db()
            .query("SELECT exec_id FROM commit_event WHERE event_id = $e")
            .bind(("e", event_id.clone()))
            .await
            .map_err(|e| {
                crate::database::types::DatabaseError::Query(format!(
                    "query exec ids for event failed: {e}"
                ))
            })?;
        let exec_rows: Vec<ExecIdRow> = q1.take(0).unwrap_or_default();
        let exec_ids: Vec<Thing> = exec_rows.into_iter().map(|r| r.exec_id).collect();
        if exec_ids.is_empty() {
            return Ok(vec![]);
        }
        // Step 2: exec rows
        let mut q2 = self
            .store
            .canonical_db()
            .query("SELECT id, session_id, flow_id, theatre_id, block_id, created_at FROM execution_event WHERE id IN $eids")
            .bind(("eids", exec_ids))
            .await
            .map_err(|e| {
                crate::database::types::DatabaseError::Query(format!(
                    "query execution rows failed: {e}"
                ))
            })?;
        let typed: Vec<ExecutionEventRec> = q2.take(0).unwrap_or_default();
        let mut out = Vec::with_capacity(typed.len());
        for r in typed {
            out.push(serde_json::json!({
                "id": r.id.to_string(),
                "session_id": r.session_id,
                "flow_id": r.flow_id,
                "theatre_id": r.theatre_id,
                "block_id": r.block_id,
                "created_at": r.created_at,
            }));
        }
        Ok(out)
    }
}
