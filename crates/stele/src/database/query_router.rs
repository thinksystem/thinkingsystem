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



use crate::database::types::DatabaseError;
use serde_json::Value;
use surrealdb::engine::remote::ws::Client;
use surrealdb::Surreal;

pub struct QueryRouter<'a> {
    pub db: &'a Surreal<Client>,
}

impl<'a> QueryRouter<'a> {
    pub fn new(db: &'a Surreal<Client>) -> Self { Self { db } }

    pub async fn get_entities_by_name(&self, name: &str) -> Result<Vec<Value>, DatabaseError> {
        let name_owned = name.to_string();
        let mut res = self
            .db
            .query("SELECT * FROM canonical_entity WHERE name = $n")
            .bind(("n", name_owned))
            .await
            .map_err(|e| DatabaseError::Query(format!("get_entities_by_name failed: {e}")))?;
        let rows: Vec<Value> = res.take(0).unwrap_or_default();
        Ok(rows)
    }

    pub async fn get_tasks_by_status(&self, status: &str, assignee: Option<&str>) -> Result<Vec<Value>, DatabaseError> {
        let status_owned = status.to_string();
        let assignee_owned = assignee.map(|s| s.to_string());
        let query = if assignee_owned.is_some() {
            "SELECT * FROM canonical_task WHERE status = $s AND string::lower(title) CONTAINS string::lower($a)"
        } else {
            "SELECT * FROM canonical_task WHERE status = $s"
        };
        let mut q = self.db.query(query).bind(("s", status_owned));
        if let Some(a) = assignee_owned { q = q.bind(("a", a)); }
        let mut res = q
            .await
            .map_err(|e| DatabaseError::Query(format!("get_tasks_by_status failed: {e}")))?;
        let rows: Vec<Value> = res.take(0).unwrap_or_default();
        Ok(rows)
    }

    pub async fn get_events_in_range(&self, start: &str, end: &str) -> Result<Vec<Value>, DatabaseError> {
        let start_owned = start.to_string();
        let end_owned = end.to_string();
        let mut res = self
            .db
            .query("SELECT * FROM canonical_event WHERE start_time >= $st AND start_time <= $et")
            .bind(("st", start_owned))
            .bind(("et", end_owned))
            .await
            .map_err(|e| DatabaseError::Query(format!("get_events_in_range failed: {e}")))?;
        let rows: Vec<Value> = res.take(0).unwrap_or_default();
        Ok(rows)
    }
}
