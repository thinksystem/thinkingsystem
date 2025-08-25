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
use crate::database::types::DatabaseError;
use crate::nlu::orchestrator::data_models::{
    Action, Entity, ExtractedData, KnowledgeNode, NumericalValue, Relationship, TemporalMarker,
};
use crate::policy::backpressure::{current_signal, BackpressureLevel};
use crate::scribes::canonical::{validate_plan, CanonicalScribe, HeuristicScribe, ScribeContext};
use crate::scribes::core::q_learning_core::QLearningCore;
use std::collections::HashMap;
use surrealdb::sql::Thing;
use surrealdb::RecordId;
use tracing::{debug, info, warn};




#[derive(Debug, Default, Clone)]
pub struct RegulariserOutcome {
    pub entity_ids: Vec<Thing>,
    pub task_ids: Vec<Thing>,
    pub event_ids: Vec<Thing>,
    pub relationship_fact_ids: Vec<Thing>,
}

pub struct Regulariser {
    store: StructuredStore,
}

impl Regulariser {
    pub fn new(store: StructuredStore) -> Self {
        Self { store }
    }

    pub async fn regularise_nodes(&self, nodes: &[KnowledgeNode]) -> Result<(), DatabaseError> {
        debug!(
            count = nodes.len(),
            "Regulariser: starting node regularisation"
        );
        let mut mapped = 0usize;
        for node in nodes {
            if let KnowledgeNode::Entity(Entity {
                entity_type,
                name,
                metadata,
                ..
            }) = node
            {
                debug!(%entity_type, %name, "Regulariser: upserting canonical_entity");
                let canonical_key = metadata
                    .as_ref()
                    .and_then(|m| m.as_object())
                    .and_then(|obj| obj.get("canonical_key"))
                    .and_then(|v| v.as_str());
                match self
                    .store
                    .upsert_canonical_entity(entity_type, name, canonical_key, metadata.clone())
                    .await
                {
                    Ok(id) => {
                        info!(%id, %entity_type, %name, "Regulariser: upserted canonical_entity");
                        mapped += 1;
                    }
                    Err(e) => {
                        warn!(%entity_type, %name, error = %e, "Regulariser: upsert_canonical_entity failed; continuing");
                    }
                }
            }
        }
        if mapped == 0 {
            warn!("Regulariser: no entity nodes to regularise");
        }
        info!("Regulariser: node regularisation complete");
        Ok(())
    }

    pub async fn regularise_extracted_data(
        &self,
        data: &ExtractedData,
    ) -> Result<RegulariserOutcome, DatabaseError> {
        self.regularise_extracted_data_with_lineage(data, None, None).await
    }

    pub async fn regularise_extracted_data_with_lineage(
        &self,
        data: &ExtractedData,
        node_map: Option<&std::collections::HashMap<String, RecordId>>,
        utterance: Option<&RecordId>,
    ) -> Result<RegulariserOutcome, DatabaseError> {
        let mut outcome = RegulariserOutcome::default();
        let use_scribe = std::env::var("STELE_ENABLE_SCRIBE")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if use_scribe {
            tracing::info!(
                target = "stele::regulariser",
                "Scribe path enabled via STELE_ENABLE_SCRIBE"
            );
            let scribe = HeuristicScribe;
            let ctx = ScribeContext::default();
            match scribe.plan(data) {
                Ok(plan) => {
                    tracing::info!(
                        target = "stele::regulariser",
                        entities = plan.entities.len(),
                        tasks = plan.tasks.len(),
                        events = plan.events.len(),
                        facts = plan.facts.len(),
                        "Scribe produced plan"
                    );

                    let shadow = std::env::var("STELE_POLICY_SHADOW").ok().is_some();
                    if shadow {
                        match validate_plan(&plan, &ctx) {
                            Ok(_) => tracing::info!(
                                target = "stele::regulariser",
                                "Shadow policy: plan valid (advise-only)"
                            ),
                            Err(e) => {
                                tracing::warn!(target = "stele::regulariser", error = %e, "Shadow policy: plan invalid (advise-only)")
                            }
                        }
                    }
                    if let Err(e) = validate_plan(&plan, &ctx) {
                        tracing::warn!(target = "stele::regulariser", error = %e, "Plan validation failed; falling back to heuristic regulariser");

                        if let Ok(prov_id) = self
                            .store
                            .create_provenance_event(
                                "plan_validation_failed",
                                serde_json::json!({"error": e.to_string(), "counts": {"entities": plan.entities.len(), "tasks": plan.tasks.len(), "events": plan.events.len(), "facts": plan.facts.len()}}),
                            )
                            .await
                        {
                            if let Some(utt) = utterance { let _ = self.store.relate_utterance_to_provenance(utt, &prov_id).await; }
                        }
                    } else {
                        match current_signal() {
                            BackpressureLevel::Red => {
                                tracing::warn!(
                                    target = "stele::regulariser",
                                    "Backpressure=Red: skipping scribe apply and falling back"
                                );
                                if let Ok(prov_id) = self
                                    .store
                                    .create_provenance_event(
                                        "scribe_apply_skipped_backpressure",
                                        serde_json::json!({"level": "red"}),
                                    )
                                    .await
                                {
                                    if let Some(utt) = utterance {
                                        let _ = self
                                            .store
                                            .relate_utterance_to_provenance(utt, &prov_id)
                                            .await;
                                    }
                                }
                            }
                            BackpressureLevel::Amber => tracing::info!(
                                target = "stele::regulariser",
                                "Backpressure=Amber: proceed with caution"
                            ),
                            BackpressureLevel::Green => {}
                        }
                        if matches!(current_signal(), BackpressureLevel::Red) {
                        } else {
                            match crate::scribes::canonical::interpret_and_apply(
                                &self.store,
                                &plan,
                                &ctx,
                            )
                            .await
                            {
                                Ok(_applied) => {
                                    if let Ok(prov_id) = self
                                    .store
                                    .create_provenance_event(
                                        "scribe_apply_success",
                                        serde_json::json!({
                                            "attempted": {"entities": _applied.attempted_entities, "tasks": _applied.attempted_tasks, "events": _applied.attempted_events},
                                            "applied": {"entities": _applied.applied_entities, "tasks": _applied.applied_tasks, "events": _applied.applied_events},
                                            "backoffs": _applied.backoff_events
                                        }),
                                    )
                                    .await
                                {
                                    if let Some(utt) = utterance { let _ = self.store.relate_utterance_to_provenance(utt, &prov_id).await; }
                                }

                                    if let Some(map) = node_map {
                                        tracing::debug!(
                                            target = "stele::regulariser",
                                            hint_count = plan.lineage_hints.len(),
                                            "Applying lineage hints"
                                        );
                                        for (temp_id, key) in plan.lineage_hints.iter() {
                                            if let Some(raw) = map.get(temp_id) {
                                                if let Some(can) = _applied.entity_ids.get(key) {
                                                    let _ = self
                                                        .store
                                                        .relate_node_to_canonical_entity(raw, can)
                                                        .await;
                                                }
                                            }
                                        }
                                    }
                                    if let Some(utt) = utterance {
                                        for (_, can) in _applied.entity_ids.iter() {
                                            let _ = self
                                                .store
                                                .relate_utterance_to_canonical_entity(utt, can)
                                                .await;
                                        }
                                    }

                                    if std::env::var("STELE_DEMO_RL").ok().is_some() {
                                        let mut q = QLearningCore::new(4, 4, 0.9, 0.05, 0.2, 8);
                                        let attempted = _applied.attempted_entities
                                            + _applied.attempted_tasks
                                            + _applied.attempted_events;
                                        let applied = _applied.applied_entities
                                            + _applied.applied_tasks
                                            + _applied.applied_events;
                                        let shaped = q.record_apply_outcome(
                                            attempted,
                                            applied,
                                            _applied.backoff_events,
                                        );
                                        tracing::info!(
                                            target = "stele::regulariser",
                                            attempted,
                                            applied,
                                            backoffs = _applied.backoff_events,
                                            shaped_reward = shaped,
                                            "RL demo: recorded apply outcome"
                                        );
                                    }
                                    tracing::info!(
                                        target = "stele::regulariser",
                                        "Scribe plan applied successfully"
                                    );
                                    
                                    
                                    return Ok(outcome);
                                }
                                Err(e) => {
                                    tracing::warn!(target = "stele::regulariser", error = %e, "Plan application failed; falling back");

                                    if let Ok(prov_id) = self
                                        .store
                                        .create_provenance_event(
                                            "scribe_apply_failed",
                                            serde_json::json!({"error": e.to_string()}),
                                        )
                                        .await
                                    {
                                        if let Some(utt) = utterance {
                                            let _ = self
                                                .store
                                                .relate_utterance_to_provenance(utt, &prov_id)
                                                .await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(target = "stele::regulariser", error = %e, "Scribe planning failed; falling back");
                    if let Ok(prov_id) = self
                        .store
                        .create_provenance_event(
                            "scribe_plan_failed",
                            serde_json::json!({"error": e.to_string()}),
                        )
                        .await
                    {
                        if let Some(utt) = utterance {
                            let _ = self
                                .store
                                .relate_utterance_to_provenance(utt, &prov_id)
                                .await;
                        }
                    }
                }
            }
        }
        debug!(
            nodes = data.nodes.len(),
            rels = data.relationships.len(),
            "Regulariser: starting full regularisation"
        );
        let mut id_map: HashMap<&str, Thing> = HashMap::new();

    for node in &data.nodes {
            match node {
                KnowledgeNode::Entity(Entity {
                    temp_id,
                    entity_type,
                    name,
                    metadata,
                    ..
                }) => {
                    debug!(temp_id = %temp_id, %entity_type, %name, "Regulariser: mapping entity → canonical_entity");
                    let canonical_key = metadata
                        .as_ref()
                        .and_then(|m| m.as_object())
                        .and_then(|obj| obj.get("canonical_key"))
                        .and_then(|v| v.as_str());
                    match self
                        .store
                        .upsert_canonical_entity(entity_type, name, canonical_key, metadata.clone())
                        .await
                    {
                        Ok(id) => {
                            if !temp_id.is_empty() {
                                id_map.insert(temp_id.as_str(), id);
                            }
                            outcome.entity_ids.push(id_map.get(temp_id.as_str()).unwrap().clone());

                            if let Some(map) = node_map {
                                if let Some(raw) = map.get(temp_id) {
                                    if let Err(e) = self
                                        .store
                                        .relate_node_to_canonical_entity(
                                            raw,
                                            id_map.get(temp_id.as_str()).unwrap(),
                                        )
                                        .await
                                    {
                                        warn!(temp_id = %temp_id, error = %e, "Regulariser: relate node→canonical_entity failed; continuing");
                                    }
                                }
                            }
                            if let Some(utt) = utterance {
                                if let Err(e) = self
                                    .store
                                    .relate_utterance_to_canonical_entity(
                                        utt,
                                        id_map.get(temp_id.as_str()).unwrap(),
                                    )
                                    .await
                                {
                                    warn!(temp_id = %temp_id, error = %e, "Regulariser: relate utterance→canonical_entity failed; continuing");
                                }
                            }
                        }
                        Err(e) => {
                            warn!(temp_id = %temp_id, %entity_type, %name, error = %e, "Regulariser: upsert_canonical_entity failed; continuing");
                        }
                    }
                }
                KnowledgeNode::Action(Action {
                    temp_id,
                    verb,
                    metadata,
                    ..
                }) => {
                    debug!(temp_id = %temp_id, %verb, "Regulariser: mapping action → canonical_task");
                    let canonical_key = metadata
                        .as_ref()
                        .and_then(|m| m.as_object())
                        .and_then(|obj| obj.get("canonical_key"))
                        .and_then(|v| v.as_str());

                    let status = metadata
                        .as_ref()
                        .and_then(|m| m.as_object())
                        .and_then(|o| o.get("status"))
                        .and_then(|v| v.as_str());
                    let assignee = metadata
                        .as_ref()
                        .and_then(|m| m.as_object())
                        .and_then(|o| o.get("assignee"))
                        .and_then(|v| v.as_str());
                    let due_date = metadata
                        .as_ref()
                        .and_then(|m| m.as_object())
                        .and_then(|o| o.get("due_date"))
                        .and_then(|v| v.as_str());
                    match self
                        .store
                        .upsert_canonical_task(
                            verb,
                            assignee,
                            due_date,
                            status,
                            canonical_key,
                            metadata.clone(),
                        )
                        .await
                    {
                        Ok(id) => {
                            if !temp_id.is_empty() {
                                id_map.insert(temp_id.as_str(), id);
                            }
                            outcome.task_ids.push(id_map.get(temp_id.as_str()).unwrap().clone());
                            if let Some(map) = node_map {
                                if let Some(raw) = map.get(temp_id) {
                                    if let Err(e) = self
                                        .store
                                        .relate_node_to_canonical_task(
                                            raw,
                                            id_map.get(temp_id.as_str()).unwrap(),
                                        )
                                        .await
                                    {
                                        warn!(temp_id = %temp_id, error = %e, "Regulariser: relate node→canonical_task failed; continuing");
                                    }
                                }
                            }
                            if let Some(utt) = utterance {
                                if let Err(e) = self
                                    .store
                                    .relate_utterance_to_canonical_task(
                                        utt,
                                        id_map.get(temp_id.as_str()).unwrap(),
                                    )
                                    .await
                                {
                                    warn!(temp_id = %temp_id, error = %e, "Regulariser: relate utterance→canonical_task failed; continuing");
                                }
                            }
                        }
                        Err(e) => {
                            warn!(temp_id = %temp_id, verb = %verb, error = %e, "Regulariser: upsert_canonical_task failed; continuing");
                        }
                    }
                }
                KnowledgeNode::Temporal(TemporalMarker {
                    temp_id,
                    date_text,
                    resolved_date,
                    metadata,
                    ..
                }) => {
                    debug!(temp_id = %temp_id, %date_text, "Regulariser: mapping temporal → canonical_event");
                    let canonical_key = metadata
                        .as_ref()
                        .and_then(|m| m.as_object())
                        .and_then(|obj| obj.get("canonical_key"))
                        .and_then(|v| v.as_str());
                    
                    let meet_present = data.nodes.iter().any(|n| match n {
                        KnowledgeNode::Action(a) => a.verb.to_lowercase().contains("meet"),
                        _ => false,
                    });
                    let title = if meet_present {
                        "Meeting"
                    } else if date_text.is_empty() {
                        "event"
                    } else {
                        date_text.as_str()
                    };
                    let start_time = resolved_date.as_deref();
                    
                    let location_str: Option<String> = data
                        .nodes
                        .iter()
                        .filter_map(|n| match n {
                            KnowledgeNode::Entity(Entity {
                                entity_type, name, ..
                            }) => {
                                let et = entity_type.to_lowercase();
                                if et.contains("location")
                                    || et.contains("place")
                                    || et.contains("address")
                                    || et.contains("venue")
                                {
                                    Some(name.clone())
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        })
                        .next();
                    match self
                        .store
                        .upsert_canonical_event(
                            title,
                            start_time,
                            None,
                            location_str.as_deref(),
                            canonical_key,
                            metadata.clone(),
                        )
                        .await
                    {
                        Ok(id) => {
                            if !temp_id.is_empty() {
                                id_map.insert(temp_id.as_str(), id);
                            }
                            outcome.event_ids.push(id_map.get(temp_id.as_str()).unwrap().clone());
                            if let Some(map) = node_map {
                                if let Some(raw) = map.get(temp_id) {
                                    if let Err(e) = self
                                        .store
                                        .relate_node_to_canonical_event(
                                            raw,
                                            id_map.get(temp_id.as_str()).unwrap(),
                                        )
                                        .await
                                    {
                                        warn!(temp_id = %temp_id, error = %e, "Regulariser: relate node→canonical_event failed; continuing");
                                    }
                                }
                            }
                            if let Some(utt) = utterance {
                                if let Err(e) = self
                                    .store
                                    .relate_utterance_to_canonical_event(
                                        utt,
                                        id_map.get(temp_id.as_str()).unwrap(),
                                    )
                                    .await
                                {
                                    warn!(temp_id = %temp_id, error = %e, "Regulariser: relate utterance→canonical_event failed; continuing");
                                }
                            }
                        }
                        Err(e) => {
                            warn!(temp_id = %temp_id, title = %title, error = %e, "Regulariser: upsert_canonical_event failed; continuing");
                        }
                    }
                }
                KnowledgeNode::Numerical(NumericalValue { .. }) => {}
            }
        }

        for rel in &data.relationships {
            debug!(source = %rel.source, target = %rel.target, predicate = %rel.relation_type, "Regulariser: creating relationship fact (if refs mapped)");
            if let Some(fid) = self.create_relationship_if_possible(rel, &id_map).await? {
                outcome.relationship_fact_ids.push(fid);
            }
        }
        
        let meeting_detected = data.nodes.iter().any(
            |n| matches!(n, KnowledgeNode::Action(a) if a.verb.to_lowercase().contains("meet")),
        );
        if meeting_detected {
            
            let event_ids: Vec<Thing> = data
                .nodes
                .iter()
                .filter_map(|n| match n {
                    KnowledgeNode::Temporal(TemporalMarker { temp_id, .. }) => {
                        id_map.get(temp_id.as_str()).cloned()
                    }
                    _ => None,
                })
                .collect();
            if !event_ids.is_empty() {
                let person_ids: Vec<Thing> = data
                    .nodes
                    .iter()
                    .filter_map(|n| match n {
                        KnowledgeNode::Entity(Entity {
                            temp_id,
                            entity_type,
                            ..
                        }) => {
                            if entity_type.eq_ignore_ascii_case("person") {
                                id_map.get(temp_id.as_str()).cloned()
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
                    .collect();
                for pid in person_ids.iter() {
                    for eid in event_ids.iter() {
                        match self
                            .store
                            .create_relationship_fact(pid, "ATTENDS", eid, Some(0.9), None)
                            .await {
                                Ok(fid) => outcome.relationship_fact_ids.push(fid),
                                Err(e) => warn!(error = %e, "Regulariser: failed to create ATTENDS fact; continuing"),
                            }
                    }
                }
            }
        }
        info!("Regulariser: full regularisation complete");
        Ok(outcome)
    }

    async fn create_relationship_if_possible(
        &self,
        rel: &Relationship,
        id_map: &HashMap<&str, Thing>,
    ) -> Result<Option<Thing>, DatabaseError> {
        let s = id_map.get(rel.source.as_str());
        let o = id_map.get(rel.target.as_str());
        if let (Some(subject), Some(object)) = (s, o) {
            debug!(predicate = %rel.relation_type, "Regulariser: creating canonical_relationship_fact");
            let provenance = rel
                .metadata
                .as_ref()
                .and_then(|m| m.as_object())
                .and_then(|o| o.get("provenance"))
                .and_then(|v| v.as_str());
            match self
                .store
                .create_relationship_fact(
                    subject,
                    rel.relation_type.as_str(),
                    object,
                    Some(rel.confidence),
                    provenance,
                )
                .await {
                    Ok(fid) => return Ok(Some(fid)),
                    Err(e) => {
                        warn!(predicate = %rel.relation_type, error = %e, "Regulariser: create_relationship_fact failed; continuing");
                    }
                }
        } else {
            warn!(source = %rel.source, target = %rel.target, "Regulariser: could not create relationship fact (subject/object missing)");
        }
        Ok(None)
    }
}
