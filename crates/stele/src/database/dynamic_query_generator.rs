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

use serde::Deserialize;
use serde_json::Value;
use std::fs;
use tracing::{debug, error, info, instrument, warn};
#[derive(Debug, Deserialize)]
struct QueryRules {
    schema_definition: SchemaDefinition,
    statements: Vec<StatementRule>,
    operators: Vec<OperatorRule>,
    best_practices: Vec<BestPractice>,
}
#[derive(Debug, Deserialize)]
struct SchemaDefinition {
    file: String,
}
#[derive(Debug, Deserialize, Clone)]
struct StatementRule {
    name: String,
    triggers: Vec<String>,
    rule_file: String,
}
#[derive(Debug, Deserialize, Clone)]
struct OperatorRule {
    name: String,
    triggers: Vec<String>,
    rule_file: String,
}
#[derive(Debug, Deserialize)]
struct BestPractice {
    id: String,
    rule: String,
}
#[derive(Debug, Deserialize)]
struct PromptTemplateConfig {
    prompt_definition: PromptDefinition,
    injected_data: std::collections::HashMap<String, String>,
}
#[derive(Debug, Deserialize)]
struct PromptDefinition {
    sections: Vec<PromptSection>,
}
#[derive(Debug, Deserialize)]
struct PromptSection {
    name: String,
    template: String,
}
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum WhereCondition {
    Simple {
        field: String,
        operator: String,
        value: Value,
    },
    And {
        #[serde(rename = "AND")]
        conditions: Vec<WhereCondition>,
    },
    Or {
        #[serde(rename = "OR")]
        conditions: Vec<WhereCondition>,
    },
}
#[derive(Debug, Deserialize)]
pub struct QueryIntent {
    pub select_fields: Vec<String>,
    pub from_targets: Vec<String>,
    #[serde(default)]
    pub where_conditions: Vec<WhereCondition>,
    #[serde(default)]
    pub order_by: Vec<String>,
    pub limit: Option<u64>,
    #[serde(default)]
    pub let_statements: std::collections::HashMap<String, String>,
}
#[derive(Debug, thiserror::Error)]
pub enum IntentGenerationError {
    #[error("Failed to load or parse rule/template files: {0}")]
    ConfigLoading(String),
    #[error("Could not detect a clear user intent from the query: '{0}'")]
    IntentNotDetected(String),
    #[error("LLM failed to generate a valid JSON response: {0}")]
    LLMGeneration(String),
    #[error("Failed to parse LLM JSON response into a QueryIntent: {0}")]
    LLMResponseParse(#[from] serde_json::Error),
}
pub struct IntelligentIntentGenerator {
    rules: QueryRules,
    schema_content: String,
    prompt_config: PromptTemplateConfig,
}
impl IntelligentIntentGenerator {
    pub fn new() -> Result<Self, IntentGenerationError> {
        let rules_content = fs::read_to_string("src/database/config/query_rules.yaml")
            .map_err(|e| IntentGenerationError::ConfigLoading(e.to_string()))?;
        let rules: QueryRules = serde_yaml::from_str(&rules_content)
            .map_err(|e| IntentGenerationError::ConfigLoading(e.to_string()))?;
        let schema_content = fs::read_to_string(&rules.schema_definition.file)
            .map_err(|e| IntentGenerationError::ConfigLoading(e.to_string()))?;
        let prompt_content = fs::read_to_string("src/database/config/query_builder_prompt.yaml")
            .map_err(|e| IntentGenerationError::ConfigLoading(e.to_string()))?;
        let prompt_config: PromptTemplateConfig = serde_yaml::from_str(&prompt_content)
            .map_err(|e| IntentGenerationError::ConfigLoading(e.to_string()))?;
        info!("Intelligent Intent Generator initialised with updated prompt structure.");
        Ok(Self {
            rules,
            schema_content,
            prompt_config,
        })
    }
    #[instrument(skip(self, text), fields(natural_language_query = %text))]
    pub async fn generate_intent(&self, text: &str) -> Result<QueryIntent, IntentGenerationError> {
        let (statement_rule, operator_rules) = self.detect_intent(text)?;
        info!(intent = %statement_rule.name, "Detected user intent.");
        let prompt = self.assemble_prompt_from_template(text, &statement_rule, &operator_rules)?;
        debug!(
            prompt_length = prompt.len(),
            "Assembled LLM prompt from updated template."
        );
        let intent = self.call_llm_for_intent_generation(&prompt).await?;
        info!("LLM generated a structured query intent with nested conditions.");
        Ok(intent)
    }
    fn detect_intent(
        &self,
        text: &str,
    ) -> Result<(StatementRule, Vec<OperatorRule>), IntentGenerationError> {
        let lower_text = text.to_lowercase();
        let statement_rule = self
            .rules
            .statements
            .iter()
            .find(|rule| {
                rule.triggers
                    .iter()
                    .any(|trigger| lower_text.contains(trigger))
            })
            .cloned()
            .ok_or_else(|| IntentGenerationError::IntentNotDetected(text.to_string()))?;
        let operator_rules = self
            .rules
            .operators
            .iter()
            .filter(|rule| {
                rule.triggers
                    .iter()
                    .any(|trigger| lower_text.contains(trigger))
            })
            .cloned()
            .collect();
        Ok((statement_rule, operator_rules))
    }
    fn assemble_prompt_from_template(
        &self,
        natural_query: &str,
        statement: &StatementRule,
        operators: &[OperatorRule],
    ) -> Result<String, IntentGenerationError> {
        let mut relevant_rules = String::new();
        let statement_rule_content = fs::read_to_string(&statement.rule_file).map_err(|e| {
            IntentGenerationError::ConfigLoading(format!(
                "Failed to load {}: {}",
                statement.rule_file, e
            ))
        })?;
        relevant_rules.push_str(&statement_rule_content);
        relevant_rules.push_str("\n\n");
        for op_rule in operators {
            debug!(operator = %op_rule.name, "Including operator rule in prompt");
            let op_rule_content = fs::read_to_string(&op_rule.rule_file).map_err(|e| {
                IntentGenerationError::ConfigLoading(format!(
                    "Failed to load {} ({}): {}",
                    op_rule.rule_file, op_rule.name, e
                ))
            })?;
            relevant_rules.push_str(&op_rule_content);
            relevant_rules.push_str("\n\n");
        }
        let best_practices = self
            .rules
            .best_practices
            .iter()
            .map(|bp| format!("- (ID: {}) {}", bp.id, bp.rule))
            .collect::<Vec<String>>()
            .join("\n");
        let mut full_prompt = String::new();
        for section in &self.prompt_config.prompt_definition.sections {
            debug!(section = %section.name, "Processing prompt section");
            let mut populated_template = section.template.clone();
            populated_template = populated_template.replace("{natural_query}", natural_query);
            populated_template =
                populated_template.replace("{schema_content}", &self.schema_content);
            populated_template =
                populated_template.replace("{relevant_rules}", relevant_rules.trim());
            populated_template = populated_template.replace("{best_practices}", &best_practices);
            for (key, value) in &self.prompt_config.injected_data {
                let placeholder = format!("{{{key}}}");
                populated_template = populated_template.replace(&placeholder, value);
            }
            full_prompt.push_str(&populated_template);
            full_prompt.push('\n');
        }
        Ok(full_prompt)
    }
    async fn call_llm_for_intent_generation(
        &self,
        prompt: &str,
    ) -> Result<QueryIntent, IntentGenerationError> {
        info!("[MOCK] Simulating LLM call to generate structured intent with OR conditions.");
        let json_response = if prompt.contains("find people") || prompt.contains("person") {
            r#"{
                "select_fields": ["*"],
                "from_targets": ["nodes"],
                "where_conditions": [
                    {
                        "OR": [
                            { "field": "type", "operator": "=", "value": "person" },
                            { "field": "properties.entity_type", "operator": "=", "value": "person" },
                            { "field": "properties.metadata.type", "operator": "=", "value": "person" }
                        ]
                    }
                ],
                "order_by": [],
                "limit": null
            }"#
        } else if prompt.contains("find entities") || prompt.contains("entity") {
            r#"{
                "select_fields": ["*"],
                "from_targets": ["nodes"],
                "where_conditions": [
                    {
                        "OR": [
                            { "field": "type", "operator": "=", "value": "entity" },
                            { "field": "properties.entity_type", "operator": "=", "value": "entity" },
                            { "field": "properties.metadata.type", "operator": "=", "value": "entity" }
                        ]
                    }
                ],
                "order_by": [],
                "limit": null
            }"#
        } else if prompt.contains("find places") || prompt.contains("place") {
            r#"{
                "select_fields": ["*"],
                "from_targets": ["nodes"],
                "where_conditions": [
                    {
                        "OR": [
                            { "field": "type", "operator": "=", "value": "place" },
                            { "field": "properties.entity_type", "operator": "=", "value": "place" },
                            { "field": "properties.metadata.type", "operator": "=", "value": "place" }
                        ]
                    }
                ],
                "order_by": [],
                "limit": null
            }"#
        } else if prompt.contains("find all concepts related to the 'Mind-Web' initiative") {
            r#"{
                "let_statements": {
                    "utterance_id": "(SELECT VALUE id FROM utterance WHERE raw_text @@ 'Mind-Web' LIMIT 1)"
                },
                "select_fields": ["*"],
                "from_targets": ["(SELECT VALUE in FROM derived_from WHERE out = $utterance_id)"],
                "where_conditions": [],
                "order_by": [],
                "limit": null
            }"#
        } else {
            warn!("[MOCK] No specific mock response for this prompt. Returning a default broad query.");
            r#"{
                "select_fields": ["*"],
                "from_targets": ["nodes"],
                "where_conditions": [],
                "order_by": [],
                "limit": 10
            }"#
        };
        let query_intent: QueryIntent = serde_json::from_str(json_response).map_err(|e| {
            error!(
                "Failed to parse mock LLM response: {}. Response was: {}",
                e, json_response
            );
            IntentGenerationError::LLMResponseParse(e)
        })?;
        Ok(query_intent)
    }
}
