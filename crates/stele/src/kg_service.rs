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


use crate::scribes::specialists::knowledge_scribe::KnowledgeScribe;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use surrealdb::{engine::remote::ws::Client, Surreal};
use tokio::sync::RwLock;

type Db = Surreal<Client>;

#[derive(Clone)]
pub struct KgService {
    db: Option<Arc<Db>>,
    knowledge: Option<Arc<RwLock<KnowledgeScribe>>>,
    subj_pred_re: Arc<Regex>,
    max_len: usize,
}

impl KgService {
    pub fn new(db: Option<Arc<Db>>, knowledge: Option<Arc<RwLock<KnowledgeScribe>>>) -> Self {
        Self {
            db,
            knowledge,
            subj_pred_re: Arc::new(Regex::new(r"^[A-Za-z0-9_:\-]{1,128}$").unwrap()),
            max_len: 256,
        }
    }
    pub fn has_db(&self) -> bool {
        self.db.is_some()
    }
    pub fn has_memory(&self) -> bool {
        self.knowledge.is_some()
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct KgFact {
    pub subject: String,
    pub predicate: String,
    pub object: serde_json::Value,
}

fn skip_zero(v: &usize) -> bool {
    *v == 0
}

#[derive(Debug, Default, Serialize)]
pub struct KgIngestSummary {
    pub accepted: usize,
    pub persisted: usize,
    pub skipped_invalid: usize,
    pub skipped_duplicate: usize,
    #[serde(skip_serializing_if = "skip_zero")]
    pub provenance_links: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub accepted_facts: Vec<KgFact>, 
}

impl KgService {
    fn validate(&self, f: &KgFact) -> Result<(), String> {
        if !self.subj_pred_re.is_match(&f.subject) {
            return Err("invalid subject".into());
        }
        if !self.subj_pred_re.is_match(&f.predicate) {
            return Err("invalid predicate".into());
        }
        if f.subject.len() > self.max_len || f.predicate.len() > self.max_len {
            return Err("subject/predicate too long".into());
        }
        Ok(())
    }
    pub async fn ingest_facts(
        &self,
        facts: Vec<KgFact>,
        provenance_sources: Option<&[String]>,
    ) -> KgIngestSummary {
        let mut summary = KgIngestSummary::default();
        let provenance_vec: Option<Vec<String>> = provenance_sources.map(|s| s.to_vec());
        let mut unique: HashSet<(String, String, String)> = HashSet::new();
        let mut staged: Vec<KgFact> = Vec::new();
        for f in facts.into_iter() {
            if let Err(e) = self.validate(&f) {
                summary.skipped_invalid += 1;
                summary
                    .errors
                    .push(format!("{}:{} -> {}", f.subject, f.predicate, e));
                continue;
            }
            let obj_key = if f.object.is_string() {
                f.object.as_str().unwrap().to_string()
            } else {
                f.object.to_string()
            };
            if !unique.insert((f.subject.clone(), f.predicate.clone(), obj_key)) {
                summary.skipped_duplicate += 1;
                continue;
            }
            if let Some(ks) = &self.knowledge {
                let mut entities = vec![serde_json::Value::String(f.subject.clone())];
                if let Some(os) = f.object.as_str() {
                    entities.push(serde_json::Value::String(os.to_string()));
                }
                let ctx = serde_json::json!({"entities": entities});
                if let Ok(mut guard) = ks.try_write() {
                    let _ = guard.link_data_to_graph(&ctx).await;
                } else {
                    let mut guard = ks.write().await;
                    let _ = guard.link_data_to_graph(&ctx).await;
                }
            }
            summary.accepted_facts.push(f.clone());
            staged.push(f);
        }
        summary.accepted = staged.len();
        if let Some(db) = &self.db {
            for f in staged.into_iter() {
                if let Ok(mut res) = db.clone().query("CREATE edge SET subject=$s, predicate=$p, object=$o, created_at=time::now();").bind(("s", f.subject)).bind(("p", f.predicate)).bind(("o", f.object)).await {
                    let created: Vec<serde_json::Value> = res.take(0).unwrap_or_default();
                    summary.persisted += 1;
                    if let Some(first) = created.first() {
                        if let Some(edge_id_str) = first.get("id").and_then(|v| v.as_str()) {
                            if let Some(srcs) = provenance_vec.as_ref() {
                                let edge_id = edge_id_str.to_string();
                                for u in srcs.iter() {
                                    let utter = u.clone();
                                    if db.clone().query("CREATE kg_fact_provenance SET edge=$e, utterance=$u, created_at=time::now();").bind(("e", edge_id.clone())).bind(("u", utter)).await.is_ok() { summary.provenance_links += 1; }
                                }
                            }
                        }
                    }
                }
            }
        }
        summary
    }

    pub async fn query(&self, filter: KgQueryFilter) -> KgQueryResult {
        let mut rows: Vec<serde_json::Value> = Vec::new();
        if let Some(db) = &self.db {
            let mut conditions: Vec<&str> = Vec::new();
            if filter.subject.is_some() {
                conditions.push("subject = $subject");
            }
            if filter.predicate.is_some() {
                conditions.push("predicate = $predicate");
            }
            if filter.object.is_some() {
                conditions.push("string(object) = $object");
            }
            let mut sql = String::from("SELECT subject, predicate, object, created_at FROM edge");
            if !conditions.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&conditions.join(" AND "));
            }
            if let Some(lim) = filter.limit {
                sql.push_str(&format!(" LIMIT {}", lim.min(500)));
            }
            if let Ok(mut q) = db
                .clone()
                .query(sql)
                .bind(("subject", filter.subject.clone().unwrap_or_default()))
                .bind(("predicate", filter.predicate.clone().unwrap_or_default()))
                .bind(("object", filter.object.clone().unwrap_or_default()))
                .await
            {
                rows = q.take(0).unwrap_or_default();
            }
        }
        let mut memory_summary: Option<serde_json::Value> = None;
        if let (Some(ks), Some(subj)) = (&self.knowledge, filter.subject.as_ref()) {
            let guard = ks.read().await;
            if let Some(data) = guard.get_entity_data(subj) {
                let related: Vec<String> = rows
                    .iter()
                    .filter_map(|r| {
                        if r["subject"].as_str() == Some(subj.as_str()) {
                            r["object"].as_str().map(|s| s.to_string())
                        } else {
                            None
                        }
                    })
                    .collect();
                memory_summary = Some(
                    serde_json::json!({"entity": subj, "data": data, "related_in_results": related}),
                );
            }
        }
        KgQueryResult {
            rows,
            memory: memory_summary,
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct KgQueryFilter {
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub predicate: Option<String>,
    #[serde(default)]
    pub object: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Serialize, Debug)]
pub struct KgQueryResult {
    pub rows: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<serde_json::Value>,
}
