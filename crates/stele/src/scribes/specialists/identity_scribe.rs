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

use crate::scribes::core::q_learning_core::QLearningCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use thiserror::Error;
#[derive(Error, Debug)]
pub enum IdentityScribeError {
    #[error("Identity with ID '{0}' not found.")]
    IdentityNotFound(String),
}
const IDENTITY_SCRIBE_STATES: usize = 16;
const IDENTITY_SCRIBE_ACTIONS: usize = 3;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub id: String,
    pub provider_name: String,
    pub trust_score: f32,
    pub roles: Vec<String>,
    pub metadata: HashMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub source_id: String,
    pub target_id: String,
    pub relationship_type: String,
    pub confidence: f32,
}
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct IdentityGraph {
    identities: HashMap<String, Identity>,
    relationships: HashMap<String, Vec<Relationship>>,
}
#[derive(Debug, Clone)]
pub struct IdentityScribe {
    pub id: String,
    cognitive_core: QLearningCore,
    identity_graph: IdentityGraph,
    last_state_action: Option<(usize, usize)>,
}
impl IdentityScribe {
    pub fn new(id: String) -> Self {
        let mut identity_graph = IdentityGraph::default();
        identity_graph.identities.insert(
            "urn:stele:log:1138".to_string(),
            Identity {
                id: "urn:stele:log:1138".to_string(),
                provider_name: "internal".to_string(),
                trust_score: 0.95,
                roles: vec!["system_log".to_string()],
                metadata: HashMap::new(),
            },
        );
        Self {
            id,
            cognitive_core: QLearningCore::new(
                IDENTITY_SCRIBE_STATES,
                IDENTITY_SCRIBE_ACTIONS,
                0.90,
                0.1,
                0.1,
                8,
            ),
            identity_graph,
            last_state_action: None,
        }
    }
    pub async fn verify_source(&self, context: &Value) -> Result<Value, String> {
        let source_id = context["source_id"].as_str().ok_or("Missing source_id")?;
        if let Some(identity) = self.identity_graph.identities.get(source_id) {
            Ok(
                json!({ "status": "Verified", "trust_score": identity.trust_score, "roles": identity.roles }),
            )
        } else {
            Ok(json!({ "status": "Unknown", "trust_score": 0.2, "roles": [] }))
        }
    }
    pub async fn link_identities(&mut self, context: &Value) -> Result<Value, String> {
        let source_id = context["source_id"]
            .as_str()
            .ok_or("Missing source_id")?
            .to_string();
        let target_id = context["target_id"]
            .as_str()
            .ok_or("Missing target_id")?
            .to_string();
        if !self.identity_graph.identities.contains_key(&source_id)
            || !self.identity_graph.identities.contains_key(&target_id)
        {
            return Err("Cannot link non-existent identities".to_string());
        }
        let relationship = Relationship {
            source_id: source_id.clone(),
            target_id,
            relationship_type: context["type"].as_str().unwrap_or("related").to_string(),
            confidence: 1.0,
        };
        self.identity_graph
            .relationships
            .entry(source_id)
            .or_default()
            .push(relationship);
        Ok(json!({"status": "linked"}))
    }
    pub async fn update_trust_score(&mut self, context: &Value) -> Result<Value, String> {
        let id = context["id"].as_str().ok_or("Missing id")?;
        let new_score = context["score"].as_f64().ok_or("Missing score")? as f32;
        if let Some(identity) = self.identity_graph.identities.get_mut(id) {
            identity.trust_score = new_score.clamp(0.0, 1.0);
            Ok(json!({"status": "updated", "new_score": identity.trust_score}))
        } else {
            Err(format!("Identity '{id}' not found for trust update."))
        }
    }
    pub fn record_reward(&mut self, reward: f32) {
        if let Some((last_state, last_action)) = self.last_state_action {
            let next_state = self.calculate_state();
            self.cognitive_core
                .add_experience(last_state, last_action, reward, next_state);
            self.cognitive_core.update_q_values();
        }
        self.last_state_action = None;
    }
    pub fn modulate_core(&mut self, aggressiveness: f32) {
        let base_exploration = 0.1;
        let modulated_exploration = base_exploration + (aggressiveness - 0.5) * 0.2;
        self.cognitive_core
            .set_modulated_exploration_rate(modulated_exploration);
    }
    fn calculate_state(&self) -> usize {
        let total_score: f32 = self
            .identity_graph
            .identities
            .values()
            .map(|i| i.trust_score)
            .sum();
        let count = self.identity_graph.identities.len() as f32;
        let avg_trust = if count > 0.0 {
            total_score / count
        } else {
            0.5
        };
        let trust_bin = (avg_trust * 4.0).floor() as usize % 4;
        let total_identities = self.identity_graph.identities.len();
        let linked_identities: HashSet<_> = self
            .identity_graph
            .relationships
            .keys()
            .flat_map(|k| {
                self.identity_graph
                    .relationships
                    .get(k)
                    .unwrap()
                    .iter()
                    .map(|r| &r.target_id)
                    .chain(std::iter::once(k))
            })
            .collect();
        let orphan_count = total_identities.saturating_sub(linked_identities.len());
        let orphan_ratio = if total_identities > 0 {
            orphan_count as f32 / total_identities as f32
        } else {
            0.0
        };
        let health_bin = ((1.0 - orphan_ratio) * 4.0).floor() as usize % 4;
        trust_bin * 4 + health_bin
    }
    pub fn get_identity(&self, identity_id: &str) -> Option<&Identity> {
        self.identity_graph.identities.get(identity_id)
    }
    pub fn get_statistics(&self) -> Value {
        json!({
            "total_identities": self.identity_graph.identities.len(),
            "total_relationships": self.identity_graph.relationships.values().map(|v| v.len()).sum::<usize>(),
            "current_rl_state": self.calculate_state(),
        })
    }
}
