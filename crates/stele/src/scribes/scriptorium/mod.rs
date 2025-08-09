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

pub mod canon;
pub mod learning_system;
use self::canon::Canon;
use self::learning_system::LearningSystem;
use crate::scribes::discourse::{DiscourseState, Inscription, Testament};
use crate::scribes::{Scribe, ScribeId, ScribeState};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
pub struct Scriptorium {
    scribes: RwLock<HashMap<ScribeId, ScribeState>>,
    learning_system: Arc<RwLock<LearningSystem>>,
}
impl Default for Scriptorium {
    fn default() -> Self {
        Self::new()
    }
}

impl Scriptorium {
    pub fn new() -> Self {
        Self {
            scribes: RwLock::new(HashMap::new()),
            learning_system: Arc::new(RwLock::new(LearningSystem::new())),
        }
    }
    pub async fn register_scribe(&self, specialist: Scribe) {
        let state = ScribeState::new(specialist);
        self.scribes
            .write()
            .await
            .insert(state.specialist.id(), state);
    }
    pub async fn get_scribe_state(&self, id: &ScribeId) -> Option<ScribeState> {
        self.scribes.read().await.get(id).cloned()
    }
    pub async fn initiate_discourse(
        &self,
        mut canon: Box<dyn Canon>,
        initial_data: Value,
    ) -> Testament {
        if let Some(raw_text) = initial_data.get("raw_text") {
            println!("Initiating discourse with raw_text: {raw_text}");
        }
        if let Some(data_source) = initial_data.get("data_source") {
            println!("Data source: {data_source}");
        }

        let _discourse_context = initial_data;

        let testament = loop {
            match canon.state() {
                DiscourseState::AwaitingAction {
                    scribe_id,
                    action_name,
                    context,
                } => {
                    let inscription = self
                        .delegate_action(&scribe_id, &action_name, &context)
                        .await;
                    canon.advance(inscription).await.unwrap_or_else(|e| {
                        eprintln!("Canon advance error (may be benign): {e}");
                    });
                }
                DiscourseState::Concluded(testament) => {
                    break testament;
                }
            }
        };
        self.process_testament(&testament).await;
        testament
    }
    async fn delegate_action(
        &self,
        scribe_id: &ScribeId,
        action_name: &str,
        canon_data: &Value,
    ) -> Inscription {
        let mut scribes = self.scribes.write().await;
        let scribe_state = scribes
            .get_mut(scribe_id)
            .expect("Required Scribe not found.");
        let result = match &mut scribe_state.specialist {
            Scribe::Data(s) if action_name == "process_data" => s
                .process_data(canon_data)
                .await
                .map(|result| json!({"processed_data": result})),
            Scribe::Knowledge(s) if action_name == "link_to_graph" => s
                .link_data_to_graph(canon_data)
                .await
                .map(|result| json!({"graph_data": result})),
            Scribe::Identity(s) if action_name == "verify_source" => {
                s.verify_source(canon_data).await
            }
            _ => Err(format!(
                "Scribe {scribe_id} cannot perform action {action_name}"
            )),
        };
        Inscription {
            scribe_id: scribe_id.clone(),
            action: action_name.to_string(),
            result,
        }
    }
    async fn process_testament(&self, testament: &Testament) {
        let mut learning_system = self.learning_system.write().await;
        let mut scribes = self.scribes.write().await;
        for id in &testament.participants {
            if let Some(scribe) = scribes.get_mut(id) {
                let new_strategy = learning_system.evolve_strategy(scribe.strategy, testament);
                scribe.strategy = new_strategy;
                scribe.interactions_completed += 1;
            }
        }
    }
}
