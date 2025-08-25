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



use crate::database::structured_store::StructuredStore;
use crate::nlu::orchestrator::data_models::{
    Action, Entity, ExtractedData, KnowledgeNode, TemporalMarker,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use surrealdb::sql::Thing;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlanScores {
    pub overall: f32,
    pub per_item: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvenanceInfo {
    pub source: String,
    pub utterance_hint: Option<String>,
    pub method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalEntityItem {
    pub entity_type: String,
    pub name: String,
    pub canonical_key: String,
    pub extra: Value,
    pub confidence: f32,
    pub provenance: ProvenanceInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalTaskItem {
    pub title: String,
    pub assignee: Option<String>,
    pub due_at: Option<String>,
    pub status: Option<String>,
    pub canonical_key: Option<String>,
    pub provenance: Value,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalEventItem {
    pub title: String,
    pub start_at: Option<String>,
    pub end_at: Option<String>,
    pub location: Option<String>,
    pub canonical_key: Option<String>,
    pub provenance: Value,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalFactItem {
    pub subject_key: String,
    pub predicate: String,
    pub object_key: String,
    pub confidence: f32,
    pub provenance: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CanonicalPlan {
    pub entities: Vec<CanonicalEntityItem>,
    pub tasks: Vec<CanonicalTaskItem>,
    pub events: Vec<CanonicalEventItem>,
    pub facts: Vec<CanonicalFactItem>,
    pub lineage_hints: HashMap<String, String>, 
    pub scores: PlanScores,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScribeContext {
    pub min_item_confidence: f32,
    pub min_plan_confidence: f32,
    pub allowed_predicates: Option<Vec<String>>, 
}

impl Default for ScribeContext {
    fn default() -> Self {
        let min_item = std::env::var("STELE_MIN_ITEM_CONFIDENCE")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.5);
        let min_plan = std::env::var("STELE_MIN_PLAN_CONFIDENCE")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.5);
        Self {
            min_item_confidence: min_item,
            min_plan_confidence: min_plan,
            allowed_predicates: None,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ScribeError {
    #[error("plan invalid: {0}")]
    Invalid(String),
    #[error("apply failed: {0}")]
    Apply(String),
}

pub trait CanonicalScribe {
    fn plan(&self, data: &ExtractedData) -> Result<CanonicalPlan, ScribeError>;
}


pub fn validate_plan(plan: &CanonicalPlan, ctx: &ScribeContext) -> Result<(), ScribeError> {
    if plan.scores.overall < ctx.min_plan_confidence {
        return Err(ScribeError::Invalid(format!(
            "plan score {} below threshold {}",
            plan.scores.overall, ctx.min_plan_confidence
        )));
    }
    for e in &plan.entities {
        if e.canonical_key.trim().is_empty() {
            return Err(ScribeError::Invalid(
                "entity with empty canonical_key".into(),
            ));
        }
        if e.confidence < ctx.min_item_confidence {
            return Err(ScribeError::Invalid(
                "entity below confidence threshold".into(),
            ));
        }
    }
    let known: std::collections::HashSet<_> = plan
        .entities
        .iter()
        .map(|e| e.canonical_key.as_str())
        .collect();
    for f in &plan.facts {
        if f.confidence < ctx.min_item_confidence {
            return Err(ScribeError::Invalid(
                "fact below confidence threshold".into(),
            ));
        }
        if !known.contains(f.subject_key.as_str()) || !known.contains(f.object_key.as_str()) {
            return Err(ScribeError::Invalid(
                "fact references unknown subject/object key".into(),
            ));
        }
    }
    Ok(())
}

pub struct HeuristicScribe;

impl HeuristicScribe {
    fn normalise_key(entity_type: &str, name: &str) -> String {
        format!(
            "{}:{}",
            entity_type.trim().to_lowercase(),
            name.trim().to_lowercase()
        )
    }
}

impl CanonicalScribe for HeuristicScribe {
    fn plan(&self, data: &ExtractedData) -> Result<CanonicalPlan, ScribeError> {
        let mut entities: Vec<CanonicalEntityItem> = Vec::new();
        let mut tasks: Vec<CanonicalTaskItem> = Vec::new();
        let mut events: Vec<CanonicalEventItem> = Vec::new();
        let mut facts: Vec<CanonicalFactItem> = Vec::new();
        let mut lineage_hints: HashMap<String, String> = HashMap::new();
        
        let mut entity_types: HashMap<String, String> = HashMap::new(); 
        let mut action_verbs: HashMap<String, String> = HashMap::new(); 

        
        for node in &data.nodes {
            match node {
                KnowledgeNode::Entity(Entity {
                    temp_id,
                    entity_type,
                    name,
                    metadata,
                    ..
                }) => {
                    let key = Self::normalise_key(entity_type, name);
                    lineage_hints.insert(temp_id.clone(), key.clone());
                    entity_types.insert(temp_id.clone(), entity_type.clone());
                    entities.push(CanonicalEntityItem {
                        entity_type: entity_type.clone(),
                        name: name.clone(),
                        canonical_key: key,
                        extra: metadata
                            .clone()
                            .unwrap_or(Value::Object(Default::default())),
                        confidence: 0.9,
                        provenance: ProvenanceInfo {
                            source: "heuristic".into(),
                            utterance_hint: None,
                            method: "regulariser-mirror".into(),
                        },
                    });
                }
                KnowledgeNode::Action(Action { verb, metadata, .. }) => {
                    
                    if let Some(tid) = metadata
                        .as_ref()
                        .and_then(|m| m.get("temp_id"))
                        .and_then(|v| v.as_str())
                    {
                        action_verbs.insert(tid.to_string(), verb.clone());
                    }
                    tasks.push(CanonicalTaskItem {
                        title: verb.clone(),
                        assignee: metadata
                            .as_ref()
                            .and_then(|m| m.get("assignee"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        due_at: metadata
                            .as_ref()
                            .and_then(|m| m.get("due_date"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        status: metadata
                            .as_ref()
                            .and_then(|m| m.get("status"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        canonical_key: None,
                        provenance: metadata
                            .clone()
                            .unwrap_or(Value::Object(Default::default())),
                        confidence: 0.6,
                    });
                }
                KnowledgeNode::Temporal(TemporalMarker {
                    date_text,
                    resolved_date,
                    metadata,
                    ..
                }) => {
                    let title = if date_text.is_empty() {
                        "event".to_string()
                    } else {
                        date_text.clone()
                    };
                    events.push(CanonicalEventItem {
                        title,
                        start_at: resolved_date.clone(),
                        end_at: None,
                        location: None,
                        canonical_key: None,
                        provenance: metadata
                            .clone()
                            .unwrap_or(Value::Object(Default::default())),
                        confidence: 0.6,
                    });
                }
                _ => {}
            }
        }

        
        
        for rel in &data.relationships {
            if let (Some(sk), Some(ok)) = (
                lineage_hints.get(&rel.source),
                lineage_hints.get(&rel.target),
            ) {
                facts.push(CanonicalFactItem {
                    subject_key: sk.clone(),
                    predicate: rel.relation_type.clone(),
                    object_key: ok.clone(),
                    confidence: rel.confidence.max(0.5),
                    provenance: rel
                        .metadata
                        .clone()
                        .unwrap_or(Value::Object(Default::default())),
                });
            }
        }

        
        
        let mut action_subject: HashMap<&str, &str> = HashMap::new();
        let mut action_object: HashMap<&str, &str> = HashMap::new();
        
        let action_ids: std::collections::HashSet<&str> = data
            .nodes
            .iter()
            .filter_map(|n| match n {
                KnowledgeNode::Action(a) => Some(a.temp_id.as_str()),
                _ => None,
            })
            .collect();
        for rel in &data.relationships {
            let src = rel.source.as_str();
            let tgt = rel.target.as_str();
            if action_ids.contains(src) {
                match rel.relation_type.as_str() {
                    "HAS_SUBJECT" | "PERFORMS" => {
                        if entity_types.contains_key(tgt) {
                            action_subject.insert(src, tgt);
                        }
                    }
                    "HAS_OBJECT" | "MANAGES" => {
                        if entity_types.contains_key(tgt) {
                            action_object.insert(src, tgt);
                        }
                    }
                    _ => {}
                }
            }
        }
        for (action, subj) in action_subject.iter() {
            if let Some(obj) = action_object.get(*action) {
                if let (Some(sk), Some(ok)) = (lineage_hints.get(*subj), lineage_hints.get(*obj)) {
                    
                    let mut predicate = "RELATED_TO".to_string();
                    
                    if data
                        .relationships
                        .iter()
                        .any(|r| r.source == *action && r.relation_type == "MANAGES")
                    {
                        predicate = "MANAGES".to_string();
                    } else if let Some(v) = action_verbs.get(*action) {
                        let v_l = v.to_lowercase();
                        if v_l.contains("lead") {
                            predicate = "LEADS".to_string();
                        } else if v_l.contains("manage") {
                            predicate = "MANAGES".to_string();
                        }
                    }
                    facts.push(CanonicalFactItem {
                        subject_key: sk.clone(),
                        predicate,
                        object_key: ok.clone(),
                        confidence: 0.7,
                        provenance: serde_json::json!({"inferred_from": action}),
                    });
                }
            }
        }

        Ok(CanonicalPlan {
            entities,
            tasks,
            events,
            facts,
            lineage_hints,
            scores: PlanScores {
                overall: 0.8,
                per_item: vec![],
            },
        })
    }
}

pub struct PlanApplyResult {
    pub entity_ids: HashMap<String, Thing>, 
    pub task_ids: Vec<Thing>,
    pub event_ids: Vec<Thing>,
    pub attempted_entities: usize,
    pub attempted_tasks: usize,
    pub attempted_events: usize,
    pub applied_entities: usize,
    pub applied_tasks: usize,
    pub applied_events: usize,
    pub backoff_events: usize,
}

pub async fn interpret_and_apply(
    store: &StructuredStore,
    plan: &CanonicalPlan,
    ctx: &ScribeContext,
) -> Result<PlanApplyResult, ScribeError> {
    
    let mut entity_ids: HashMap<String, Thing> = HashMap::new();
    let mut any_applied = false;
    let mut attempted_entities = 0usize;
    let mut backoff_events = 0usize;
    for e in &plan.entities {
        if e.confidence < ctx.min_item_confidence {
            continue;
        }
        attempted_entities += 1;
        match store
            .upsert_canonical_entity(
                &e.entity_type,
                &e.name,
                Some(&e.canonical_key),
                Some(e.extra.clone()),
            )
            .await
        {
            Ok(id) => {
                entity_ids.insert(e.canonical_key.clone(), id);
                any_applied = true;
                
                if std::env::var("STELE_DEMO_COUNT_BACKOFF").ok().is_some() {
                    backoff_events = backoff_events.saturating_add(0); 
                }
            }
            Err(err) => {
                tracing::warn!(target = "stele::canonical_scribe", error = %err, key = %e.canonical_key, "apply: entity upsert failed; continuing");
                continue;
            }
        }
    }

    
    let mut task_ids: Vec<Thing> = Vec::new();
    let mut attempted_tasks = 0usize;
    for t in &plan.tasks {
        if t.confidence < ctx.min_item_confidence {
            continue;
        }
        attempted_tasks += 1;
        match store
            .upsert_canonical_task(
                &t.title,
                t.assignee.as_deref(),
                t.due_at.as_deref(),
                t.status.as_deref(),
                t.canonical_key.as_deref(),
                Some(t.provenance.clone()),
            )
            .await
        {
            Ok(id) => {
                task_ids.push(id);
                any_applied = true;
            }
            Err(err) => {
                tracing::warn!(target = "stele::canonical_scribe", error = %err, title = %t.title, "apply: task upsert failed; continuing");
                continue;
            }
        }
    }

    
    let mut event_ids: Vec<Thing> = Vec::new();
    let mut attempted_events = 0usize;
    for ev in &plan.events {
        if ev.confidence < ctx.min_item_confidence {
            continue;
        }
        attempted_events += 1;
        match store
            .upsert_canonical_event(
                &ev.title,
                ev.start_at.as_deref(),
                ev.end_at.as_deref(),
                ev.location.as_deref(),
                ev.canonical_key.as_deref(),
                Some(ev.provenance.clone()),
            )
            .await
        {
            Ok(id) => {
                event_ids.push(id);
                any_applied = true;
            }
            Err(err) => {
                tracing::warn!(target = "stele::canonical_scribe", error = %err, title = %ev.title, "apply: event upsert failed; continuing");
                continue;
            }
        }
    }

    
    for f in &plan.facts {
        if f.confidence < ctx.min_item_confidence {
            continue;
        }
        let s = match entity_ids.get(&f.subject_key) {
            Some(id) => id,
            None => continue,
        };
        let o = match entity_ids.get(&f.object_key) {
            Some(id) => id,
            None => continue,
        };
        if let Err(e) = store
            .create_relationship_fact(s, &f.predicate, o, Some(f.confidence), None)
            .await
        {
            tracing::warn!(target = "stele::canonical_scribe", error = %e, "apply: create_relationship_fact failed; continuing");
        }
    }

    if !any_applied {
        return Err(ScribeError::Apply("no items applied".into()));
    }
    
    let applied_entities = entity_ids.len();
    let applied_tasks = task_ids.len();
    let applied_events = event_ids.len();
    Ok(PlanApplyResult {
        entity_ids,
        task_ids,
        event_ids,
        attempted_entities,
        attempted_tasks,
        attempted_events,
        applied_entities,
        applied_tasks,
        applied_events,
        backoff_events,
    })
}
