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

use crate::database::prompt_builder::InstructionFile;
use lazy_static::lazy_static;
use petgraph::dot::Dot;
use petgraph::graph::{DiGraph, NodeIndex};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::{self, Display};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use surrealdb::sql::Statement;
use tracing::{debug, error, info, warn};
lazy_static! {
    static ref STRING_LITERAL_RE: Regex = Regex::new(r#"'[^']*'|`[^`]*|"[^"]*""#).unwrap();
    static ref NUMERIC_LITERAL_RE: Regex = Regex::new(r"\b\d+(\.\d+)?\b").unwrap();
    static ref RECORD_ID_RE: Regex =
        Regex::new(r"\b[a-zA-Z_][a-zA-Z0-9_]*:[a-zA-Z0-9_:]*\b").unwrap();
    static ref FUNCTION_CALL_RE: Regex = Regex::new(r"\b[a-zA-Z_][a-zA-Z0-9_:]*\([^)]*\)").unwrap();
    static ref DOC_PLACEHOLDER_RE: Regex = Regex::new(r"<[^>]+>|\{[^}]+\}").unwrap();
}
#[derive(Debug, Deserialize)]
struct OperatorExample {
    title: String,
    query: String,
}
#[derive(Debug, Deserialize)]
struct OperatorSpec {
    #[serde(default)]
    aliases: Vec<String>,
    summary: String,
    signature: String,
    #[serde(default)]
    examples: Vec<OperatorExample>,
}
#[derive(Debug, Deserialize)]
struct OperatorDefinition {
    name: String,
    spec: OperatorSpec,
}
#[derive(Debug, Deserialize)]
struct OperatorGroup {
    definitions: Vec<OperatorDefinition>,
}
#[derive(Debug, Deserialize)]
struct OperatorsFile {
    operator_groups: std::collections::HashMap<String, OperatorGroup>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OperatorParsingResult {
    pub operator_name: String,
    pub example_title: String,
    pub query: String,
    pub success: bool,
    pub error_message: Option<String>,
    pub error_type: Option<String>, 
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ParsingRegistry {
    pub total_examples: usize,
    pub successful_parses: usize,
    pub failed_parses: usize,
    pub results: Vec<OperatorParsingResult>,
    pub unsupported_operators: HashMap<String, Vec<String>>, 
    pub success_rate: f32,
}

impl Default for ParsingRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ParsingRegistry {
    pub fn new() -> Self {
        Self {
            total_examples: 0,
            successful_parses: 0,
            failed_parses: 0,
            results: Vec::new(),
            unsupported_operators: HashMap::new(),
            success_rate: 0.0,
        }
    }

    pub fn add_result(&mut self, result: OperatorParsingResult) {
        self.total_examples += 1;

        if result.success {
            self.successful_parses += 1;
        } else {
            self.failed_parses += 1;

            
            let error_type = result
                .error_type
                .clone()
                .unwrap_or_else(|| "Unknown".to_string());
            self.unsupported_operators
                .entry(result.operator_name.clone())
                .or_default()
                .push(error_type);
        }

        self.results.push(result);
        self.update_success_rate();
    }

    fn update_success_rate(&mut self) {
        if self.total_examples > 0 {
            self.success_rate =
                (self.successful_parses as f32 / self.total_examples as f32) * 100.0;
        }
    }

    pub fn get_summary(&self) -> String {
        format!(
            "Parser Coverage: {:.1}% ({}/{} examples parsed successfully)\n\
            Unsupported operators: {}\n\
            Most common error patterns: {:?}",
            self.success_rate,
            self.successful_parses,
            self.total_examples,
            self.unsupported_operators.len(),
            self.get_common_error_patterns()
        )
    }

    fn get_common_error_patterns(&self) -> Vec<(String, usize)> {
        let mut error_counts: HashMap<String, usize> = HashMap::new();

        for result in &self.results {
            if let Some(error_type) = &result.error_type {
                *error_counts.entry(error_type.clone()).or_insert(0) += 1;
            }
        }

        let mut sorted: Vec<_> = error_counts.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.into_iter().take(5).collect()
    }

    pub fn save_to_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PatternNode {
    pub parameterized_str: String,
    pub ast: Statement,
}
#[derive(Clone)]
pub enum KgNode {
    Concept(String),
    Clause(String),
    Pattern(Box<PatternNode>),
}
impl fmt::Debug for KgNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
    }
}
impl PartialEq for KgNode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Concept(l0), Self::Concept(r0)) => l0 == r0,
            (Self::Clause(l0), Self::Clause(r0)) => l0 == r0,
            (Self::Pattern(l0), Self::Pattern(r0)) => l0.parameterized_str == r0.parameterized_str,
            _ => false,
        }
    }
}
impl Eq for KgNode {}
impl Hash for KgNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            KgNode::Concept(s) => s.hash(state),
            KgNode::Clause(s) => s.hash(state),
            KgNode::Pattern(p) => p.parameterized_str.hash(state),
        }
    }
}
impl Display for KgNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KgNode::Concept(s) => write!(f, "Concept(\"{s}\")"),
            KgNode::Clause(s) => write!(f, "Clause(\"{s}\")"),
            KgNode::Pattern(p) => write!(
                f,
                "Pattern(\"{}\")",
                p.parameterized_str.replace('"', "\\\"")
            ),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KgEdge {
    IsA,
    CanUse,
    Implements,
}
pub struct QueryKnowledgeGraph {
    pub graph: DiGraph<KgNode, KgEdge>,
    node_map: HashMap<KgNode, NodeIndex>,
    pub parsing_registry: ParsingRegistry,
}
impl QueryKnowledgeGraph {
    fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_map: HashMap::new(),
            parsing_registry: ParsingRegistry::new(),
        }
    }
    pub fn to_dot(&self) -> String {
        format!("{:?}", Dot::new(&self.graph))
    }

    fn extract_error_type(error_msg: &str) -> String {
        
        if error_msg.contains("Unexpected token") {
            if let Some(token_start) = error_msg.find("Unexpected token `") {
                let search_start = token_start + 18;
                if let Some(token_end) = error_msg[search_start..].find('`') {
                    return format!(
                        "Unexpected token: {}",
                        &error_msg[search_start..search_start + token_end]
                    );
                }
            }
            "Unexpected token".to_string()
        } else if error_msg.contains("Invalid token") {
            if let Some(token_start) = error_msg.find("Invalid token `") {
                let search_start = token_start + 15;
                if let Some(token_end) = error_msg[search_start..].find('`') {
                    return format!(
                        "Invalid token: {}",
                        &error_msg[search_start..search_start + token_end]
                    );
                }
            }
            "Invalid token".to_string()
        } else if error_msg.contains("expected") {
            "Missing expected syntax".to_string()
        } else if error_msg.contains("Parse error") {
            "Parse error".to_string()
        } else {
            "Other parsing error".to_string()
        }
    }
}
pub struct QueryKgBuilder {
    docs_path: String,
}
impl QueryKgBuilder {
    pub fn new(docs_path: &str) -> Self {
        Self {
            docs_path: docs_path.to_string(),
        }
    }
    pub fn build(&self) -> Result<QueryKnowledgeGraph, Box<dyn std::error::Error>> {
        info!(
            "Starting Knowledge Graph generation from path: {}",
            self.docs_path
        );
        let mut kg = QueryKnowledgeGraph::new();
        let base_path = Path::new(&self.docs_path);
        let instruction_dirs = ["statement", "clause", "operator"];
        for dir in &instruction_dirs {
            let dir_path = base_path.join(dir);
            if !dir_path.is_dir() {
                debug!("Skipping non-existent directory: {:?}", dir_path);
                continue;
            }
            for entry in fs::read_dir(dir_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file()
                    && path.extension().and_then(|s| s.to_str()) == Some("yaml")
                    && !path.file_name().unwrap().to_str().unwrap().starts_with('_')
                {
                    let file_path_str = path.to_string_lossy().to_string();
                    debug!("Processing documentation file: {}", file_path_str);
                    let result = if file_path_str.ends_with("operators.yml")
                        || file_path_str.ends_with("operators.yaml")
                    {
                        self.load_and_process_operators_file(&file_path_str, &mut kg)
                    } else {
                        self.load_and_process_instruction_file(&file_path_str, &mut kg)
                    };
                    if let Err(e) = result {
                        error!(file = %file_path_str, error = %e, "Failed to process documentation file for KG");
                    }
                }
            }
        }
        info!(
            "Knowledge Graph generation complete. Nodes: {}, Edges: {}",
            kg.graph.node_count(),
            kg.graph.edge_count()
        );

        
        info!("{}", kg.parsing_registry.get_summary());

        
        if let Err(e) = kg.parsing_registry.save_to_file("parsing_analysis.json") {
            warn!("Failed to save parsing analysis: {}", e);
        } else {
            info!("Parsing analysis saved to parsing_analysis.json");
        }

        Ok(kg)
    }
    fn load_and_process_instruction_file(
        &self,
        file_path: &str,
        kg: &mut QueryKnowledgeGraph,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let content_str = fs::read_to_string(file_path)?;
        let file: InstructionFile = serde_yaml::from_str(&content_str)
            .map_err(|e| format!("Failed to parse YAML file {file_path}: {e}"))?;
        let concept_node = self.get_or_create_node(kg, KgNode::Concept(file.instruction_type));
        let primary_clause_node = self.get_or_create_node(kg, KgNode::Clause(file.name.clone()));
        kg.graph
            .add_edge(primary_clause_node, concept_node, KgEdge::IsA);
        for variant in &file.spec.variants {
            for example in &variant.examples {
                if example.query.trim().is_empty() {
                    continue;
                }

                debug!(
                    "Processing example with query pattern: '{}'",
                    example.query.chars().take(50).collect::<String>()
                );

                match surrealdb::sql::parse(&example.query) {
                    Ok(statements) => {
                        if let Some(ast) = statements.into_iter().next() {
                            let pattern_str = self.parameterize_query(&example.query);
                            let pattern_node_data = PatternNode {
                                parameterized_str: pattern_str,
                                ast,
                            };
                            let pattern_kg_node = self.get_or_create_node(
                                kg,
                                KgNode::Pattern(Box::new(pattern_node_data)),
                            );
                            kg.graph.add_edge(
                                pattern_kg_node,
                                primary_clause_node,
                                KgEdge::Implements,
                            );
                        } else {
                            warn!(query = %example.query, "Query parsed into an empty statement list, skipping.");
                        }
                    }
                    Err(e) => {
                        warn!(query = %example.query, error = %e, "Failed to parse example query into AST, skipping.");
                    }
                }
            }
        }
        for feature in &file.spec.features {
            let feature_clause_node =
                self.get_or_create_node(kg, KgNode::Clause(feature.name.clone()));
            kg.graph
                .add_edge(primary_clause_node, feature_clause_node, KgEdge::CanUse);
        }
        Ok(())
    }
    fn load_and_process_operators_file(
        &self,
        file_path: &str,
        kg: &mut QueryKnowledgeGraph,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let content_str = fs::read_to_string(file_path)?;
        let operators_file: OperatorsFile = serde_yaml::from_str(&content_str)
            .map_err(|e| format!("Failed to parse operators YAML file {file_path}: {e}"))?;
        let operator_concept_node =
            self.get_or_create_node(kg, KgNode::Concept("Operator".to_string()));
        for (_group_name, group) in operators_file.operator_groups {
            for operator_def in group.definitions {
                let primary_operator_node =
                    self.get_or_create_node(kg, KgNode::Clause(operator_def.name.clone()));
                kg.graph
                    .add_edge(primary_operator_node, operator_concept_node, KgEdge::IsA);

                debug!(
                    "Processing operator '{}' with signature: '{}'",
                    operator_def.name, operator_def.spec.signature
                );

                for alias in operator_def.spec.aliases {
                    let alias_node = self.get_or_create_node(kg, KgNode::Clause(alias));
                    kg.graph
                        .add_edge(alias_node, primary_operator_node, KgEdge::CanUse);
                }
                for example in operator_def.spec.examples {
                    if example.query.trim().is_empty() {
                        continue;
                    }

                    debug!(
                        "Processing operator example: '{}' with summary: '{}'",
                        example.title, operator_def.spec.summary
                    );

                    match surrealdb::sql::parse(&example.query) {
                        Ok(statements) => {
                            if let Some(ast) = statements.into_iter().next() {
                                let pattern_str = self.parameterize_query(&example.query);
                                let pattern_node_data = PatternNode {
                                    parameterized_str: pattern_str,
                                    ast,
                                };
                                let pattern_kg_node = self.get_or_create_node(
                                    kg,
                                    KgNode::Pattern(Box::new(pattern_node_data)),
                                );
                                kg.graph.add_edge(
                                    pattern_kg_node,
                                    primary_operator_node,
                                    KgEdge::Implements,
                                );

                                
                                kg.parsing_registry.add_result(OperatorParsingResult {
                                    operator_name: operator_def.name.clone(),
                                    example_title: example.title.clone(),
                                    query: example.query.clone(),
                                    success: true,
                                    error_message: None,
                                    error_type: None,
                                });
                            } else {
                                warn!(query = %example.query, "Operator query parsed into an empty statement list, skipping.");

                                
                                kg.parsing_registry.add_result(OperatorParsingResult {
                                    operator_name: operator_def.name.clone(),
                                    example_title: example.title.clone(),
                                    query: example.query.clone(),
                                    success: false,
                                    error_message: Some("Empty statement list".to_string()),
                                    error_type: Some("Empty result".to_string()),
                                });
                            }
                        }
                        Err(e) => {
                            let error_message = e.to_string();
                            let error_type =
                                QueryKnowledgeGraph::extract_error_type(&error_message);

                            debug!(
                                operator = %operator_def.name,
                                example = %example.title,
                                query = %example.query,
                                error = %e,
                                error_type = %error_type,
                                "Operator example parsing failed - adding to unsupported registry."
                            );

                            
                            kg.parsing_registry.add_result(OperatorParsingResult {
                                operator_name: operator_def.name.clone(),
                                example_title: example.title.clone(),
                                query: example.query.clone(),
                                success: false,
                                error_message: Some(error_message),
                                error_type: Some(error_type),
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn get_or_create_node(&self, kg: &mut QueryKnowledgeGraph, node: KgNode) -> NodeIndex {
        if let Some(&index) = kg.node_map.get(&node) {
            return index;
        }
        let index = kg.graph.add_node(node.clone());
        kg.node_map.insert(node, index);
        index
    }
    fn parameterize_query(&self, query: &str) -> String {
        let mut parameterized = query.to_string();
        parameterized = STRING_LITERAL_RE
            .replace_all(&parameterized, " $string ")
            .to_string();
        parameterized = NUMERIC_LITERAL_RE
            .replace_all(&parameterized, " $number ")
            .to_string();
        parameterized = FUNCTION_CALL_RE
            .replace_all(&parameterized, " $function_call ")
            .to_string();
        parameterized = RECORD_ID_RE
            .replace_all(&parameterized, " $record_id ")
            .to_string();
        let cleaned = parameterized
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        cleaned
            .replace(" -> ", "->")
            .replace(" <- ", "<-")
            .replace("= >", "=>")
            .replace("< =", "<=")
            .replace("> =", ">=")
            .replace("! =", "!=")
            .replace("+ =", "+=")
            .replace("- =", "-=")
            .replace("... ;", "...")
    }
}
