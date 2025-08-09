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

use crate::scribes::discourse::{DiscourseState, Inscription, Testament};
use crate::scribes::ScribeId;
use async_trait::async_trait;
use serde_json::{json, Value};
use thiserror::Error;
#[derive(Error, Debug)]
pub enum CanonError {
    #[error("The discourse has already been sealed.")]
    DiscourseSealed,
    #[error(
        "Expected action '{expected}' from scribe '{scribe}', but received action '{received}'."
    )]
    UnexpectedAction {
        expected: String,
        received: String,
        scribe: ScribeId,
    },
    #[error("An inscription failed: {0}")]
    InscriptionFailed(String),
}
#[async_trait]
pub trait Canon: Send + Sync {
    async fn advance(&mut self, inscription: Inscription) -> Result<(), CanonError>;
    fn state(&mut self) -> DiscourseState;
    fn participants(&self) -> Vec<ScribeId>;
}
pub struct CanonOfIngestionAndVerification {
    data_scribe_id: ScribeId,
    knowledge_scribe_id: ScribeId,
    identity_scribe_id: ScribeId,
    step: WorkflowStep,
    is_sealed: bool,
    chronicle: Vec<Inscription>,
    raw_text: String,
    data_source: String,
    processed_data: String,
    linked_data: String,
    trust_score: f32,
}
pub enum WorkflowStep {
    PendingDataProcessing,
    PendingKnowledgeLinking,
    PendingSourceVerification,
    Done,
}
impl CanonOfIngestionAndVerification {
    pub fn new(
        raw_text: String,
        data_source: String,
        data_scribe_id: ScribeId,
        knowledge_scribe_id: ScribeId,
        identity_scribe_id: ScribeId,
    ) -> Self {
        Self {
            data_scribe_id,
            knowledge_scribe_id,
            identity_scribe_id,
            step: WorkflowStep::PendingDataProcessing,
            is_sealed: false,
            chronicle: Vec::new(),
            raw_text,
            data_source,
            processed_data: String::new(),
            linked_data: String::new(),
            trust_score: 0.0,
        }
    }
    fn seal_discourse(&mut self, was_successful: bool) -> Testament {
        self.is_sealed = true;
        self.step = WorkflowStep::Done;
        Testament {
            canon_invoked: "CanonOfIngestionAndVerification".into(),
            participants: self.participants(),
            was_successful,
            final_product: json!({
                "source_text": self.raw_text,
                "verified_knowledge": self.linked_data,
                "source_trust_score": self.trust_score,
            }),
            chronicle: self.chronicle.clone(),
        }
    }
}
#[async_trait]
impl Canon for CanonOfIngestionAndVerification {
    async fn advance(&mut self, inscription: Inscription) -> Result<(), CanonError> {
        if self.is_sealed {
            return Err(CanonError::DiscourseSealed);
        }
        self.chronicle.push(inscription.clone());
        if let Err(e) = &inscription.result {
            self.seal_discourse(false);
            return Err(CanonError::InscriptionFailed(e.clone()));
        }
        let result_value = inscription.result.unwrap();
        match self.step {
            WorkflowStep::PendingDataProcessing => {
                if inscription.scribe_id != self.data_scribe_id
                    || inscription.action != "process_data"
                {
                    return Err(CanonError::UnexpectedAction {
                        expected: "process_data".to_string(),
                        received: inscription.action,
                        scribe: inscription.scribe_id,
                    });
                }
                self.processed_data = result_value.as_str().unwrap_or_default().to_string();
                self.step = WorkflowStep::PendingKnowledgeLinking;
            }
            WorkflowStep::PendingKnowledgeLinking => {
                if inscription.scribe_id != self.knowledge_scribe_id
                    || inscription.action != "link_to_graph"
                {
                    return Err(CanonError::UnexpectedAction {
                        expected: "link_to_graph".to_string(),
                        received: inscription.action,
                        scribe: inscription.scribe_id,
                    });
                }
                self.linked_data = result_value.as_str().unwrap_or_default().to_string();
                self.step = WorkflowStep::PendingSourceVerification;
            }
            WorkflowStep::PendingSourceVerification => {
                if inscription.scribe_id != self.identity_scribe_id
                    || inscription.action != "verify_source"
                {
                    return Err(CanonError::UnexpectedAction {
                        expected: "verify_source".to_string(),
                        received: inscription.action,
                        scribe: inscription.scribe_id,
                    });
                }
                if let Some(obj) = result_value.as_object() {
                    self.trust_score = obj
                        .get("trust_score")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0) as f32;
                }
                self.seal_discourse(true);
            }
            WorkflowStep::Done => return Err(CanonError::DiscourseSealed),
        }
        Ok(())
    }
    fn state(&mut self) -> DiscourseState {
        if self.is_sealed {
            return DiscourseState::Concluded(self.seal_discourse(self.trust_score > 0.5));
        }
        match self.step {
            WorkflowStep::PendingDataProcessing => DiscourseState::AwaitingAction {
                scribe_id: self.data_scribe_id.clone(),
                action_name: "process_data".to_string(),
                context: json!({"text": self.raw_text, "urgency": 0.5}),
            },
            WorkflowStep::PendingKnowledgeLinking => DiscourseState::AwaitingAction {
                scribe_id: self.knowledge_scribe_id.clone(),
                action_name: "link_to_graph".to_string(),
                context: json!({"entities": [self.processed_data]}),
            },
            WorkflowStep::PendingSourceVerification => DiscourseState::AwaitingAction {
                scribe_id: self.identity_scribe_id.clone(),
                action_name: "verify_source".to_string(),
                context: json!({"source_id": self.data_source}),
            },
            WorkflowStep::Done => {
                DiscourseState::Concluded(self.seal_discourse(self.trust_score > 0.5))
            }
        }
    }
    fn participants(&self) -> Vec<ScribeId> {
        vec![
            self.data_scribe_id.clone(),
            self.knowledge_scribe_id.clone(),
            self.identity_scribe_id.clone(),
        ]
    }
}
