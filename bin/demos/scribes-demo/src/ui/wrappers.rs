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

use crate::ui::core::UIBridge;
use crate::ui::logging_trait::UILogged;
use crate::{demo_processor::DemoDataProcessor, identity::EnhancedIdentityVerifier};
use serde_json::{json, Value};
use std::sync::Arc;
use stele::scribes::core::q_learning_core::QLearningCore;
use stele::scribes::specialists::KnowledgeScribe;

pub struct UIKnowledgeScribe {
    inner: KnowledgeScribe,
    ui_bridge: Arc<UIBridge>,
    component_name: String,
}

impl UIKnowledgeScribe {
    pub fn new(inner: KnowledgeScribe, ui_bridge: Arc<UIBridge>) -> Self {
        Self {
            inner,
            ui_bridge,
            component_name: "Knowledge Scribe".to_string(),
        }
    }

    pub async fn link_data_to_graph(&mut self, context: &Value) -> Result<Value, String> {
        let operation = "link_data_to_graph";
        let is_llm = false;

        let empty_vec = vec![];
        let entities = context["entities"].as_array().unwrap_or(&empty_vec);
        if entities.is_empty() {
            let fallback_result = json!({
                "status": "skipped",
                "reason": "empty_entities",
                "message": "No entities provided for knowledge graph operation"
            });

            self.ui_bridge.log_scribe_operation(
                &self.component_name,
                operation,
                context.clone(),
                Some(fallback_result.clone()),
                Some(0),
                is_llm,
            );

            return Ok(fallback_result);
        }

        let start_time = chrono::Utc::now();
        let result = self.inner.link_data_to_graph(context).await;

        let processing_time = chrono::Utc::now()
            .signed_duration_since(start_time)
            .num_milliseconds() as u128;

        match &result {
            Ok(output) => {
                self.ui_bridge.log_scribe_operation(
                    &self.component_name,
                    operation,
                    context.clone(),
                    Some(output.clone()),
                    Some(processing_time),
                    is_llm,
                );
            }
            Err(error) => {
                let handled_result = if error.contains("Not enough entities")
                    || error.contains("source or target not found")
                    || error.contains("No entity to merge into")
                {
                    let graceful_response = json!({
                        "status": "handled_gracefully",
                        "original_error": error,
                        "reason": "insufficient_data_for_operation",
                        "message": "Operation requires more entities or existing graph state"
                    });

                    self.ui_bridge.log_scribe_operation(
                        &self.component_name,
                        operation,
                        context.clone(),
                        Some(graceful_response.clone()),
                        Some(processing_time),
                        is_llm,
                    );

                    Ok(graceful_response)
                } else {
                    self.ui_bridge.log_scribe_operation(
                        &self.component_name,
                        operation,
                        context.clone(),
                        None,
                        Some(processing_time),
                        is_llm,
                    );
                    eprintln!(
                        "❌ {} operation '{}' failed: {}",
                        self.component_name, operation, error
                    );
                    Err(error.clone())
                };

                return handled_result;
            }
        }

        result
    }

    pub async fn test_knowledge_specialist_ui(
        &mut self,
        test_data: &Value,
    ) -> Result<String, String> {
        let operation = "test_knowledge_specialist_ui";
        let is_llm = false;
        let start_time = chrono::Utc::now();

        let content = test_data
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("No content provided");

        let entities = test_data
            .get("entities")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>())
            .unwrap_or_else(|| vec!["entity1", "entity2"]);

        let knowledge_context = json!({
            "entities": entities,
            "content": content,
            "operation": "user_prompt_analysis"
        });

        let knowledge_result = self.link_data_to_graph(&knowledge_context).await;

        let processing_time = chrono::Utc::now()
            .signed_duration_since(start_time)
            .num_milliseconds() as u128;

        match knowledge_result {
            Ok(result) => {
                let success_message = format!(
                    "Knowledge Scribe processed user prompt: '{content}' with entities {entities:?}. Result: {result}"
                );

                self.ui_bridge.log_scribe_operation(
                    &self.component_name,
                    operation,
                    test_data.clone(),
                    Some(result),
                    Some(processing_time),
                    is_llm,
                );

                Ok(success_message)
            }
            Err(error) => {
                self.ui_bridge.log_scribe_operation(
                    &self.component_name,
                    operation,
                    test_data.clone(),
                    None,
                    Some(processing_time),
                    is_llm,
                );

                Err(format!(
                    "Knowledge processing failed for '{content}': {error}"
                ))
            }
        }
    }
}

impl UILogged for UIKnowledgeScribe {
    fn get_ui_bridge(&self) -> &Arc<UIBridge> {
        &self.ui_bridge
    }

    fn get_component_name(&self) -> &str {
        &self.component_name
    }
}

pub struct UIDataProcessor {
    inner: Arc<DemoDataProcessor>,
    ui_bridge: Arc<UIBridge>,
    component_name: String,
}

impl UIDataProcessor {
    pub fn new(inner: Arc<DemoDataProcessor>, ui_bridge: Arc<UIBridge>) -> Self {
        Self {
            inner,
            ui_bridge,
            component_name: "Data Scribe".to_string(),
        }
    }

    pub async fn process_data(&self, context: &Value) -> Result<Value, String> {
        let is_llm_call = context
            .get("text")
            .and_then(|t| t.as_str())
            .map(|s| s.len() > 100)
            .unwrap_or(false);

        self.log_async_operation(
            "process_data",
            context,
            is_llm_call,
            self.inner.process_data(context),
        )
        .await
    }

    pub async fn store_extracted_data(&self, data: &Value) -> Result<Value, String> {
        self.log_async_operation(
            "store_extracted_data",
            data,
            false,
            self.inner.store_extracted_data(data),
        )
        .await
    }

    pub async fn extract_entities(&self, text: &str) -> Result<Vec<String>, String> {
        let input = serde_json::json!({"text": text});

        let result = self
            .log_async_operation("extract_entities", &input, true, async {
                let entities_value = self.inner.extract_entities(text).await?;
                let entities: Vec<String> = if let Some(entities_array) = entities_value.as_array()
                {
                    entities_array
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                } else {
                    vec![]
                };
                Ok(entities)
            })
            .await;

        result
    }

    pub async fn process_user_data(&self, user_context: Value) -> Result<String, String> {
        let user_text = user_context
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("No text provided");

        let data_result = self.process_data(&user_context).await;

        match data_result {
            Ok(result) => Ok(format!(
                "Data Scribe analysed user input: '{user_text}'. Analysis: {result}"
            )),
            Err(error) => Err(format!("Data processing failed for '{user_text}': {error}")),
        }
    }
}

impl UILogged for UIDataProcessor {
    fn get_ui_bridge(&self) -> &Arc<UIBridge> {
        &self.ui_bridge
    }

    fn get_component_name(&self) -> &str {
        &self.component_name
    }
}

pub struct UIIdentityVerifier {
    inner: Arc<EnhancedIdentityVerifier>,
    ui_bridge: Arc<UIBridge>,
    component_name: String,
}

impl UIIdentityVerifier {
    pub fn new(inner: Arc<EnhancedIdentityVerifier>, ui_bridge: Arc<UIBridge>) -> Self {
        Self {
            inner,
            ui_bridge,
            component_name: "Identity Scribe".to_string(),
        }
    }

    pub async fn verify_source(&self, context: &Value) -> Result<Value, String> {
        self.log_async_operation(
            "verify_source",
            context,
            false,
            self.inner.verify_source(context),
        )
        .await
    }

    pub async fn link_identities(&self, context: &Value) -> Result<Value, String> {
        self.log_async_operation(
            "link_identities",
            context,
            false,
            self.inner.link_identities(context),
        )
        .await
    }

    pub async fn verify_user_identity(&self, identity_context: &Value) -> Result<String, String> {
        let user_content = identity_context
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("No content provided");

        let verify_result = self.verify_source(identity_context).await;

        match verify_result {
            Ok(result) => Ok(format!(
                "Identity Scribe verified user prompt: '{user_content}'. Verification: {result}"
            )),
            Err(error) => Err(format!(
                "Identity verification failed for '{user_content}': {error}"
            )),
        }
    }
}

impl UILogged for UIIdentityVerifier {
    fn get_ui_bridge(&self) -> &Arc<UIBridge> {
        &self.ui_bridge
    }

    fn get_component_name(&self) -> &str {
        &self.component_name
    }
}

pub struct UIQLearning {
    inner: QLearningCore,
    ui_bridge: Arc<UIBridge>,
    component_name: String,
}

impl UIQLearning {
    pub fn new(inner: QLearningCore, ui_bridge: Arc<UIBridge>) -> Self {
        Self {
            inner,
            ui_bridge,
            component_name: "Q-Learning".to_string(),
        }
    }

    pub fn choose_action(&mut self, state: usize, valid_actions: &[usize]) -> usize {
        let input = serde_json::json!({
            "state": state,
            "valid_actions": valid_actions
        });

        self.log_sync_operation("choose_action", &input, false, || {
            let action = self.inner.choose_action(state, valid_actions);
            Ok::<usize, String>(action)
        })
        .unwrap_or(0)
    }

    pub fn add_experience(&mut self, state: usize, action: usize, reward: f64, next_state: usize) {
        let input = serde_json::json!({
            "state": state,
            "action": action,
            "reward": reward,
            "next_state": next_state
        });

        let operation = "add_experience";
        let start_time = chrono::Utc::now();

        self.inner
            .add_experience(state, action, reward as f32, next_state);

        let processing_time = chrono::Utc::now()
            .signed_duration_since(start_time)
            .num_milliseconds() as u128;

        self.ui_bridge.log_scribe_operation(
            &self.component_name,
            operation,
            input,
            Some(serde_json::json!({"status": "success"})),
            Some(processing_time),
            false,
        );
    }

    pub fn update_q_values(&mut self) {
        let input = serde_json::json!({"operation": "update_q_values"});
        let operation = "update_q_values";
        let start_time = chrono::Utc::now();

        self.inner.update_q_values();

        let processing_time = chrono::Utc::now()
            .signed_duration_since(start_time)
            .num_milliseconds() as u128;

        self.ui_bridge.log_scribe_operation(
            &self.component_name,
            operation,
            input,
            Some(serde_json::json!({"status": "success"})),
            Some(processing_time),
            false,
        );
    }
}

impl UILogged for UIQLearning {
    fn get_ui_bridge(&self) -> &Arc<UIBridge> {
        &self.ui_bridge
    }

    fn get_component_name(&self) -> &str {
        &self.component_name
    }
}
