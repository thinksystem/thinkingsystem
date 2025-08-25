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
use surrealdb::sql::Thing;
use crate::StructuredStore;

#[derive(Debug, Default)]
pub struct KnowledgeQueryBuilder {
    id: Option<Thing>,
    limit: Option<usize>,
}

#[derive(Debug)]
pub struct KnowledgeQueryResult {
    pub raw: Value,
}

impl KnowledgeQueryBuilder {
    pub fn new() -> Self { Self::default() }
    pub fn id(mut self, id: Thing) -> Self { self.id = Some(id); self }
    pub fn limit(mut self, l: usize) -> Self { self.limit = Some(l); self }
    pub async fn execute(self, store: &StructuredStore) -> anyhow::Result<KnowledgeQueryResult> {
        
        let raw = if let Some(id) = self.id {
            let mut q = store.canonical_db().query("SELECT * FROM $id").bind(("id", id)).await?;
            let rows: Vec<Value> = q.take(0).unwrap_or_default();
            serde_json::json!({"rows": rows})
        } else {
            let mut q = store.canonical_db().query("SELECT * FROM canonical_event LIMIT $l").bind(("l", self.limit.unwrap_or(10) as i64)).await?;
            let rows: Vec<Value> = q.take(0).unwrap_or_default();
            serde_json::json!({"rows": rows})
        };
        Ok(KnowledgeQueryResult { raw })
    }
}
