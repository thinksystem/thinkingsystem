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

use crate::database::prompt_builder::ContextualPromptEngine;
use crate::database::query_kg::QueryKnowledgeGraph;
use crate::database::schema_analyser::GraphSchema;
use crate::nlu::orchestrator::data_models::{AdvancedQueryIntent, QueryComplexity, TraversalInfo};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};
#[derive(Debug, thiserror::Error)]
pub enum IntentError {
    #[error("Failed to load or parse tool definitions: {0}")]
    ConfigError(String),
    #[error("Failed to build the prompt for the LLM.")]
    PromptError,
    #[error("LLM processing failed: {0}")]
    LLMError(String),
    #[error("Failed to parse LLM response into a valid plan: {0}")]
    ResponseParseError(#[from] serde_json::Error),
    #[error("Could not find a valid JSON object in the LLM response.")]
    JsonNotFoundInResponse,
    #[error("Knowledge graph analysis failed: {0}")]
    KnowledgeGraphError(String),
}
#[derive(Debug, Serialize, Deserialize)]
struct ToolSetConfig {
    tools: Vec<ToolDefinition>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolDefinition {
    name: String,
    description: String,
    parameters: Value,
}
#[derive(Debug, Deserialize, Clone)]
pub struct ToolCall {
    pub name: String,
    #[serde(rename = "parameters", default)]
    pub arguments: HashMap<String, Value>,
}
#[derive(Debug, Deserialize)]
struct LlmResponse {
    tool_calls: Vec<ToolCall>,
}
#[derive(Debug, Serialize)]
struct EnhancedSchemaContext {
    known_node_types: Vec<String>,
    known_relationship_labels: Vec<String>,
    indexed_node_fields: Vec<String>,
    node_search_patterns: HashMap<String, Vec<String>>,
    query_hints: QueryHints,
}
#[derive(Debug, Serialize, Default)]
struct QueryHints {
    recommended_tools: Vec<String>,
    parameter_suggestions: HashMap<String, Value>,
    reasoning: Vec<String>,
}
pub struct IntelligentIntentAnalyser {
    tool_definitions: Vec<ToolDefinition>,
    system_prompt_template: String,
    graph_schema: Arc<RwLock<GraphSchema>>,
    query_kg: Arc<QueryKnowledgeGraph>,
    prompt_engine: ContextualPromptEngine,
}
lazy_static! {
    static ref MARKDOWN_JSON_REGEX: Regex =
        Regex::new(r"(?s)```(?:json)?\s*(\{.*\})\s*```").unwrap();
    static ref BRACE_JSON_REGEX: Regex = Regex::new(r"(?s)(\{.*\})").unwrap();
    static ref HOPS_REGEX: Regex = Regex::new(r"(\d+)\s*(?:hops|steps)").unwrap();
}
impl IntelligentIntentAnalyser {
    pub fn new(
        graph_schema: Arc<RwLock<GraphSchema>>,
        query_kg: Arc<QueryKnowledgeGraph>,
    ) -> Result<Self, IntentError> {
        let tool_definitions_path = if std::path::Path::new(
            "crates/stele/src/database/config/tool_definitions.yaml",
        )
        .exists()
        {
            "crates/stele/src/database/config/tool_definitions.yaml"
        } else if std::path::Path::new(
            "../../../crates/stele/src/database/config/tool_definitions.yaml",
        )
        .exists()
        {
            "../../../crates/stele/src/database/config/tool_definitions.yaml"
        } else {
            return Err(IntentError::ConfigError("Tool definitions file not found. Please run from workspace root or demo directory.".into()));
        };

        let config_content = fs::read_to_string(tool_definitions_path)
            .map_err(|e| IntentError::ConfigError(e.to_string()))?;
        let config: ToolSetConfig = serde_yaml::from_str(&config_content)
            .map_err(|e| IntentError::ConfigError(e.to_string()))?;
        let prompt_engine = ContextualPromptEngine::new(4000, query_kg.clone())
            .map_err(|e| IntentError::KnowledgeGraphError(e.to_string()))?;
        info!(
            "IntelligentIntentAnalyser initialised with {} tools and knowledge graph support.",
            config.tools.len()
        );
        Ok(Self {
            tool_definitions: config.tools,
            system_prompt_template: r#"SYSTEM PROMPT:
You are an expert system that translates natural language queries into a series of structured function calls to build a database query.
Your ONLY output must be a single, valid JSON object with a key named "tool_calls". Do not include any other text, explanations, or markdown formatting.

**CRITICAL INSTRUCTIONS:**
1.  **Multi-Field Search for Types**: When a user asks for a category (e.g., "concepts", "people", "systems"), you MUST search across all relevant fields as specified in `node_search_patterns`.
2.  **Use Known Labels**: When filtering by relationship, you MUST use a label from the `known_relationship_labels` list.
3.  **Prioritise Indexed Fields**: The fields in `indexed_node_fields` are fast. Use them whenever possible.
4.  **LEVERAGE QUERY HINTS**: Pay special attention to the `query_hints` section below. These are intelligent recommendations based on semantic analysis of the user's query.

{context}

**QUERY ANALYSIS GUIDANCE:**
If the query_hints section contains recommended_tools, strongly consider using those tools.
If parameter_suggestions are provided, use those as default values for the suggested parameters.
The reasoning section explains why these recommendations were made.

USER QUERY:
{user_query}"#.to_string(),
            graph_schema,
            query_kg,
            prompt_engine,
        })
    }
    pub async fn analyse_intent(
        &self,
        natural_query: &str,
    ) -> Result<AdvancedQueryIntent, IntentError> {
        let query_lower = natural_query.to_lowercase();
        let mut intent = AdvancedQueryIntent {
            original_query: natural_query.to_string(),
            ..Default::default()
        };
        let complex_keywords = [
            "connected to",
            "related to",
            "hops",
            "path",
            "travers",
            "relationship between",
        ];
        if complex_keywords.iter().any(|&kw| query_lower.contains(kw))
            || HOPS_REGEX.is_match(&query_lower)
        {
            intent.complexity = QueryComplexity::ComplexGraph;
        } else {
            intent.complexity = QueryComplexity::SimpleLookup;
        }
        let schema = self.graph_schema.read().await;
        if let Some(nodes_table) = schema.tables.get("nodes") {
            if let Some(type_counts) = nodes_table.field_value_counts.get("type") {
                for node_type in type_counts.keys() {
                    if query_lower.contains(&node_type.to_lowercase()) {
                        intent.entities.push(node_type.clone());
                    }
                }
            }
        }
        for rel_label in schema.relationships.keys() {
            if query_lower.contains(&rel_label.to_lowercase())
                || query_lower.contains(&rel_label.to_lowercase().replace('_', " "))
            {
                intent.relationships.push(rel_label.clone());
            }
        }
        drop(schema);
        if let Some(captures) = HOPS_REGEX.captures(&query_lower) {
            if let Some(hops_str) = captures.get(1) {
                if let Ok(hops) = hops_str.as_str().parse::<u8>() {
                    intent.traversals.push(TraversalInfo {
                        hops,
                        via_relationships: intent.relationships.clone(),
                        direction: "any".to_string(),
                    });
                }
            }
        }
        if query_lower.contains("reliable") || query_lower.contains("confident") {
            intent
                .filters
                .insert("min_confidence".to_string(), json!(0.8));
        }
        if query_lower.contains("recent")
            || query_lower.contains("latest")
            || query_lower.contains("newest")
        {
            intent.filters.insert(
                "orderBy".to_string(),
                json!({"field": "created_at", "direction": "DESC"}),
            );
        }
        debug!("Lightweight intent analysis complete: {:?}", intent);
        Ok(intent)
    }
    pub async fn build_prompt_for_query(&self, natural_query: &str) -> Result<String, IntentError> {
        let query_hints = self.generate_query_hints(natural_query).await?;
        debug!(
            "Generated {} query hints from knowledge graph",
            query_hints.recommended_tools.len()
        );
        let schema_guard = self.graph_schema.read().await;
        let enhanced_context = self
            .build_enhanced_context(&schema_guard, query_hints)
            .await?;
        drop(schema_guard);
        let system_prompt = self.build_enhanced_system_prompt(&enhanced_context)?;
        let full_prompt = system_prompt.replace("{user_query}", natural_query);
        debug!("Constructed enhanced LLM prompt for intent analysis.");
        Ok(full_prompt)
    }
    async fn generate_query_hints(&self, query: &str) -> Result<QueryHints, IntentError> {
        let mut hints = QueryHints::default();

        debug!(
            "Analysing query structure using knowledge graph with {} nodes: {}",
            self.query_kg.graph.node_count(),
            query
        );

        let query_lower = query.to_lowercase();
        if query_lower.contains("select") || query_lower.contains("find") {
            hints.recommended_tools.push("query_tool".to_string());
            hints
                .reasoning
                .push("Query operations detected".to_string());
        }
        if query_lower.contains("create") || query_lower.contains("insert") {
            hints.recommended_tools.push("create_tool".to_string());
            hints
                .reasoning
                .push("Create operations detected".to_string());
        }

        let scored_instructions = self
            .prompt_engine
            .find_and_score_instructions_via_graph(query);
        for (score, instruction) in scored_instructions.iter().take(3) {
            let tool_name = self.map_instruction_to_tool(&instruction.name);
            if let Some(tool) = tool_name {
                hints.recommended_tools.push(tool.clone());
                hints.reasoning.push(format!(
                    "Tool '{}' recommended based on instruction '{}' (score: {})",
                    tool, instruction.name, score
                ));
            }
        }
        if query.to_lowercase().contains("reliable") || query.to_lowercase().contains("confident") {
            hints.parameter_suggestions.insert(
                "filter_by_confidence".to_string(),
                serde_json::json!({"min_score": 0.8}),
            );
            hints
                .reasoning
                .push("High confidence filtering suggested due to 'reliable' keyword".to_string());
        }
        if query.to_lowercase().contains("recent") || query.to_lowercase().contains("latest") {
            hints.parameter_suggestions.insert(
                "set_result_order".to_string(),
                serde_json::json!({"field": "created_at", "direction": "DESC"}),
            );
            hints
                .reasoning
                .push("Time-based ordering suggested due to temporal keywords".to_string());
        }
        let relationship_keywords = [
            "related to",
            "connected to",
            "derived from",
            "caused by",
            "part of",
        ];
        for keyword in &relationship_keywords {
            if query.to_lowercase().contains(keyword) {
                hints
                    .recommended_tools
                    .push("filter_by_relationship".to_string());
                hints.reasoning.push(format!(
                    "Relationship filtering suggested due to '{keyword}'"
                ));
                break;
            }
        }
        debug!(
            "Generated query hints: {} tools, {} parameters, {} reasoning points",
            hints.recommended_tools.len(),
            hints.parameter_suggestions.len(),
            hints.reasoning.len()
        );
        Ok(hints)
    }
    fn map_instruction_to_tool(&self, instruction_name: &str) -> Option<String> {
        match instruction_name.to_lowercase().as_str() {
            name if name.contains("select") => Some("find_entities".to_string()),
            name if name.contains("where") => Some("filter_by_confidence".to_string()),
            name if name.contains("order") || name.contains("sort") => {
                Some("set_result_order".to_string())
            }
            name if name.contains("limit") => Some("set_result_limit".to_string()),
            name if name.contains("relationship") || name.contains("join") => {
                Some("filter_by_relationship".to_string())
            }
            name if name.contains("temporal") || name.contains("time") => {
                Some("filter_by_temporal".to_string())
            }
            _ => None,
        }
    }
    async fn build_enhanced_context(
        &self,
        schema: &GraphSchema,
        query_hints: QueryHints,
    ) -> Result<EnhancedSchemaContext, IntentError> {
        let known_node_types: Vec<String> =
            schema.tables.get("nodes").map_or_else(Vec::new, |ts| {
                ts.field_value_counts
                    .get("type")
                    .map_or_else(Vec::new, |counts| counts.keys().cloned().collect())
            });
        let enhanced_context = EnhancedSchemaContext {
            known_node_types,
            known_relationship_labels: schema.relationships.keys().cloned().collect(),
            indexed_node_fields: vec!["type".to_string(), "properties.*".to_string()],
            node_search_patterns: schema.node_search_patterns.clone(),
            query_hints,
        };
        Ok(enhanced_context)
    }
    fn build_enhanced_system_prompt(
        &self,
        enhanced_context: &EnhancedSchemaContext,
    ) -> Result<String, IntentError> {
        let context_json = serde_json::to_string_pretty(enhanced_context)
            .map_err(|e| IntentError::ConfigError(e.to_string()))?;
        let tools_json = serde_json::to_string_pretty(&self.tool_definitions)
            .map_err(|e| IntentError::ConfigError(e.to_string()))?;

        let template = self.system_prompt_template
            .replace("{context}", &format!("<enhanced_schema_context>\n{context_json}\n</enhanced_schema_context>\n<tool_definitions>\n{tools_json}\n</tool_definitions>"));

        Ok(template)
    }
    pub fn parse_response_to_plan(
        &self,
        llm_response_str: &str,
    ) -> Result<Vec<ToolCall>, IntentError> {
        debug!(response = %llm_response_str, "Received raw response from LLM for parsing.");
        let parsed_response = Self::parse_and_validate_llm_response(llm_response_str)?;
        info!(
            "Successfully parsed LLM response into a plan with {} tool calls.",
            parsed_response.tool_calls.len()
        );
        Ok(parsed_response.tool_calls)
    }
    fn parse_and_validate_llm_response(text: &str) -> Result<LlmResponse, IntentError> {
        let json_str =
            Self::extract_json_from_llm_output(text).ok_or(IntentError::JsonNotFoundInResponse)?;
        serde_json::from_str(&json_str).map_err(IntentError::from)
    }
    fn extract_json_from_llm_output(text: &str) -> Option<String> {
        if let Some(captures) = MARKDOWN_JSON_REGEX.captures(text) {
            if let Some(json_match) = captures.get(1) {
                return Some(json_match.as_str().to_string());
            }
        }
        if let Some(captures) = BRACE_JSON_REGEX.captures(text) {
            if let Some(json_match) = captures.get(1) {
                return Some(json_match.as_str().to_string());
            }
        }
        None
    }
}
