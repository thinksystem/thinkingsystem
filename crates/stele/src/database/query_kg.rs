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
use crate::database::surreal_token::SurrealTokenParser;
use lazy_static::lazy_static;
use petgraph::dot::Dot;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
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
    
    pub idiom_total_examples: usize,
    pub idiom_successful_parses: usize,
    pub idiom_failed_parses: usize,
    pub idiom_results: Vec<IdiomParsingResult>,
    pub idiom_success_rate: f32,

    
    pub ast_total_examples: usize,
    pub ast_successful_parses: usize,
    pub ast_failed_parses: usize,
    pub ast_records: Vec<AstParsingRecord>,
    pub ast_success_rate: f32,

    
    pub deltas: Vec<ParsingDelta>,
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
            idiom_total_examples: 0,
            idiom_successful_parses: 0,
            idiom_failed_parses: 0,
            idiom_results: Vec::new(),
            idiom_success_rate: 0.0,
            ast_total_examples: 0,
            ast_successful_parses: 0,
            ast_failed_parses: 0,
            ast_records: Vec::new(),
            ast_success_rate: 0.0,
            deltas: Vec::new(),
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
        if self.idiom_total_examples > 0 {
            self.idiom_success_rate =
                (self.idiom_successful_parses as f32 / self.idiom_total_examples as f32) * 100.0;
        }
        if self.ast_total_examples > 0 {
            self.ast_success_rate =
                (self.ast_successful_parses as f32 / self.ast_total_examples as f32) * 100.0;
        }
    }

    pub fn get_summary(&self) -> String {
        format!(
            "Parser Coverage: {:.1}% ({}/{})\nIdiom Coverage: {:.1}% ({}/{})\nUnsupported operators: {}\nMost common AST error patterns: {:?}",
            self.success_rate,
            self.successful_parses,
            self.total_examples,
            self.idiom_success_rate,
            self.idiom_successful_parses,
            self.idiom_total_examples,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IdiomParsingResult {
    pub source_type: String,
    pub name: String,
    pub context: Option<String>,
    pub query: String,
    pub success: bool,
    pub error_message: Option<String>,

    pub roughly_equal: Option<bool>,
    pub ast_summary: Option<String>,
    pub idiom_summary: Option<String>,
    pub similarity_score: Option<f32>,

    pub confidence: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AstParsingRecord {
    pub source_type: String,     
    pub name: String,            
    pub context: Option<String>, 
    pub query: String,           
    pub success: bool,
    pub error_message: Option<String>,
    pub ast_summary: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ParsingDelta {
    pub source_type: String,
    pub name: String,
    pub context: Option<String>,
    pub roughly_equal: Option<bool>,
    pub similarity_score: Option<f32>,
    pub ast_summary: Option<String>,
    pub idiom_summary: Option<String>,
    pub notes: Option<String>,
}

impl ParsingRegistry {
    pub fn add_idiom_result(&mut self, result: IdiomParsingResult) {
        self.idiom_total_examples += 1;
        if result.success {
            self.idiom_successful_parses += 1;
        } else {
            self.idiom_failed_parses += 1;
        }
        self.idiom_results.push(result);
        self.update_success_rate();
    }

    pub fn add_ast_record(&mut self, record: AstParsingRecord) {
        self.ast_total_examples += 1;
        if record.success {
            self.ast_successful_parses += 1;
        } else {
            self.ast_failed_parses += 1;
        }
        self.ast_records.push(record);
        self.update_success_rate();
    }

    pub fn compute_deltas(&mut self) {
        
        use std::collections::BTreeMap;
        let mut idx: BTreeMap<(String, String, Option<String>, String), &AstParsingRecord> =
            BTreeMap::new();
        for r in &self.ast_records {
            idx.insert(
                (
                    r.source_type.clone(),
                    r.name.clone(),
                    r.context.clone(),
                    r.query.clone(),
                ),
                r,
            );
        }
        self.deltas.clear();
        for ir in &self.idiom_results {
            let key = (
                ir.source_type.clone(),
                ir.name.clone(),
                ir.context.clone(),
                ir.query.clone(),
            );
            let ast = idx.get(&key);
            let notes = if let Some(ast_rec) = ast {
                if ast_rec.success && ir.success {
                    None
                } else if ast_rec.success && !ir.success {
                    Some("Idiom failed but AST parsed".to_string())
                } else if !ast_rec.success && ir.success {
                    Some("Idiom succeeded but AST failed".to_string())
                } else {
                    Some("Both failed".to_string())
                }
            } else {
                Some("No matching AST record".to_string())
            };

            self.deltas.push(ParsingDelta {
                source_type: ir.source_type.clone(),
                name: ir.name.clone(),
                context: ir.context.clone(),
                roughly_equal: ir.roughly_equal,
                similarity_score: ir.similarity_score,
                ast_summary: ast.and_then(|a| a.ast_summary.clone()),
                idiom_summary: ir.idiom_summary.clone(),
                notes,
            });
        }
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
    pub fn list_operator_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for node_idx in self.graph.node_indices() {
            if let Some(KgNode::Clause(name)) = self.graph.node_weight(node_idx) {
                let mut is_operator = false;
                for edge in self.graph.edges(node_idx) {
                    if let KgEdge::IsA = edge.weight() {
                        if let Some(KgNode::Concept(concept)) =
                            self.graph.node_weight(edge.target())
                        {
                            if concept == "Operator" {
                                is_operator = true;
                                break;
                            }
                        }
                    }
                }
                if is_operator {
                    names.push(name.clone());
                }
            }
        }
        names
    }
    pub fn suggest_clauses_for_text(&self, text: &str, limit: usize) -> Vec<String> {
        let q = text.to_lowercase();
        let mut scored: Vec<(String, u32)> = Vec::new();
        for node_idx in self.graph.node_indices() {
            if let Some(KgNode::Clause(name)) = self.graph.node_weight(node_idx) {
                let n = name.to_lowercase();
                let mut score = 0u32;
                if q.contains(&n) || n.contains(&q) {
                    score += 5;
                }

                for part in n.split_whitespace() {
                    if !part.is_empty() && q.contains(part) {
                        score += 1;
                    }
                }
                if score > 0 {
                    scored.push((name.clone(), score));
                }
            }
        }
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored.into_iter().take(limit).map(|(n, _)| n).collect()
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

        let mut parser_opt = match SurrealTokenParser::new() {
            Ok(p) => Some(p),
            Err(e) => {
                warn!(
                    "Failed to initialise SurrealTokenParser: {}. Idiom coverage disabled.",
                    e
                );
                None
            }
        };
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
                        self.load_and_process_operators_file(
                            &file_path_str,
                            &mut kg,
                            parser_opt.as_mut(),
                        )
                    } else {
                        self.load_and_process_instruction_file(
                            &file_path_str,
                            &mut kg,
                            parser_opt.as_mut(),
                        )
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

        
        kg.parsing_registry.compute_deltas();

        info!("{}", kg.parsing_registry.get_summary());

        if let Err(e) = kg.parsing_registry.save_to_file("parsing_analysis.json") {
            warn!("Failed to save parsing analysis: {}", e);
        } else {
            info!("Parsing analysis saved to parsing_analysis.json");
        }

        Ok(kg)
    }

    pub fn build_and_save_analysis(
        &self,
        out_path: &str,
    ) -> Result<QueryKnowledgeGraph, Box<dyn std::error::Error>> {
        let kg = self.build()?;
        if let Err(e) = kg.parsing_registry.save_to_file(out_path) {
            warn!("Failed to save parsing analysis to {}: {}", out_path, e);
        }
        Ok(kg)
    }
    fn load_and_process_instruction_file(
        &self,
        file_path: &str,
        kg: &mut QueryKnowledgeGraph,
        mut token_parser: Option<&mut SurrealTokenParser>,
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
                            
                            kg.parsing_registry.add_ast_record(AstParsingRecord {
                                source_type: "instruction".to_string(),
                                name: file.name.clone(),
                                context: Some(variant.name.clone()),
                                query: example.query.clone(),
                                success: true,
                                error_message: None,
                                ast_summary: Some(self.summarise_features(&example.query)),
                            });
                        } else {
                            warn!(query = %example.query, "Query parsed into an empty statement list, skipping.");
                            kg.parsing_registry.add_ast_record(AstParsingRecord {
                                source_type: "instruction".to_string(),
                                name: file.name.clone(),
                                context: Some(variant.name.clone()),
                                query: example.query.clone(),
                                success: false,
                                error_message: Some("Empty statement list".to_string()),
                                ast_summary: None,
                            });
                        }
                    }
                    Err(e) => {
                        warn!(query = %example.query, error = %e, "Failed to parse example query into AST, skipping.");
                        kg.parsing_registry.add_ast_record(AstParsingRecord {
                            source_type: "instruction".to_string(),
                            name: file.name.clone(),
                            context: Some(variant.name.clone()),
                            query: example.query.clone(),
                            success: false,
                            error_message: Some(e.to_string()),
                            ast_summary: None,
                        });
                    }
                }

                if let Some(parser) = token_parser.as_deref_mut() {
                    let idiom_res = match parser.parse_with_validation(&example.query) {
                        Ok(idiom) => {
                            let select = SurrealTokenParser::convert_idiom_to_select_query(&idiom);
                            let idiom_q = select.to_string();
                            let ast_q = &example.query;
                            let approx = self.roughly_equal(ast_q, &idiom_q);
                            let ast_sum = self.summarise_features(ast_q);
                            let idiom_sum = self.summarise_features(&idiom_q);
                            IdiomParsingResult {
                                source_type: "instruction".to_string(),
                                name: file.name.clone(),
                                context: Some(variant.name.clone()),
                                query: example.query.clone(),
                                success: true,
                                error_message: None,
                                roughly_equal: Some(approx),
                                ast_summary: Some(ast_sum),
                                idiom_summary: Some(idiom_sum),
                                similarity_score: Some(self.similarity_score(ast_q, &idiom_q)),
                                confidence: Some(self.similarity_score(ast_q, &idiom_q) / 100.0),
                            }
                        }
                        Err(err) => IdiomParsingResult {
                            source_type: "instruction".to_string(),
                            name: file.name.clone(),
                            context: Some(variant.name.clone()),
                            query: example.query.clone(),
                            success: false,
                            error_message: Some(err.to_string()),
                            roughly_equal: None,
                            ast_summary: Some(self.summarise_features(&example.query)),
                            idiom_summary: None,
                            similarity_score: None,
                            confidence: Some(0.0),
                        },
                    };
                    kg.parsing_registry.add_idiom_result(idiom_res);
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
        mut token_parser: Option<&mut SurrealTokenParser>,
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

                                
                                kg.parsing_registry.add_ast_record(AstParsingRecord {
                                    source_type: "operator".to_string(),
                                    name: operator_def.name.clone(),
                                    context: Some(example.title.clone()),
                                    query: example.query.clone(),
                                    success: true,
                                    error_message: None,
                                    ast_summary: Some(self.summarise_features(&example.query)),
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

                                kg.parsing_registry.add_ast_record(AstParsingRecord {
                                    source_type: "operator".to_string(),
                                    name: operator_def.name.clone(),
                                    context: Some(example.title.clone()),
                                    query: example.query.clone(),
                                    success: false,
                                    error_message: Some("Empty statement list".to_string()),
                                    ast_summary: None,
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

                            kg.parsing_registry.add_ast_record(AstParsingRecord {
                                source_type: "operator".to_string(),
                                name: operator_def.name.clone(),
                                context: Some(example.title.clone()),
                                query: example.query.clone(),
                                success: false,
                                error_message: Some(e.to_string()),
                                ast_summary: None,
                            });
                        }
                    }

                    if let Some(parser) = token_parser.as_deref_mut() {
                        let idiom_res = match parser.parse_with_validation(&example.query) {
                            Ok(idiom) => {
                                let select =
                                    SurrealTokenParser::convert_idiom_to_select_query(&idiom);
                                let idiom_q = select.to_string();
                                let ast_q = &example.query;
                                let approx = self.roughly_equal(ast_q, &idiom_q);
                                let ast_sum = self.summarise_features(ast_q);
                                let idiom_sum = self.summarise_features(&idiom_q);
                                IdiomParsingResult {
                                    source_type: "operator".to_string(),
                                    name: operator_def.name.clone(),
                                    context: Some(example.title.clone()),
                                    query: example.query.clone(),
                                    success: true,
                                    error_message: None,
                                    roughly_equal: Some(approx),
                                    ast_summary: Some(ast_sum),
                                    idiom_summary: Some(idiom_sum),
                                    similarity_score: Some(self.similarity_score(ast_q, &idiom_q)),
                                    confidence: Some(
                                        self.similarity_score(ast_q, &idiom_q) / 100.0,
                                    ),
                                }
                            }
                            Err(err) => IdiomParsingResult {
                                source_type: "operator".to_string(),
                                name: operator_def.name.clone(),
                                context: Some(example.title.clone()),
                                query: example.query.clone(),
                                success: false,
                                error_message: Some(err.to_string()),
                                roughly_equal: None,
                                ast_summary: Some(self.summarise_features(&example.query)),
                                idiom_summary: None,
                                similarity_score: None,
                                confidence: Some(0.0),
                            },
                        };
                        kg.parsing_registry.add_idiom_result(idiom_res);
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

        parameterized = DOC_PLACEHOLDER_RE
            .replace_all(&parameterized, " $placeholder ")
            .to_string();
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
    fn extract_features(&self, query: &str) -> (bool, usize, bool, bool, bool, Vec<String>) {
        let q = query.to_lowercase();
        let has_graph = q.contains("->") || q.contains("<-");
        let where_count = q.match_indices(" where ").count();
        let has_limit = q.contains(" limit ");
        let has_order = q.contains(" order by ");
        let has_count = q.contains("count(");

        let mut from_tables: Vec<String> = Vec::new();
        if let Some(pos) = q.find(" from ") {
            let tail = &q[pos + 6..];
            let end = tail
                .find(|c: char| [' ', '\n', '\r', ';'].contains(&c))
                .unwrap_or(tail.len());
            let segment = &tail[..end];
            for t in segment.split(',') {
                let name = t.trim().to_string();
                if !name.is_empty() {
                    from_tables.push(name);
                }
            }
        }
        (
            has_graph,
            where_count,
            has_limit,
            has_order,
            has_count,
            from_tables,
        )
    }
    fn summarise_features(&self, q: &str) -> String {
        let (g, w, l, o, c, mut t) = self.extract_features(q);
        t.sort();
        format!(
            "graph={} where={} limit={} order={} count={} from=[{}]",
            g,
            w,
            l,
            o,
            c,
            t.join(",")
        )
    }
    fn roughly_equal(&self, a: &str, b: &str) -> bool {
        let (ag, aw, al, ao, ac, at) = self.extract_features(a);
        let (bg, bw, bl, bo, bc, bt) = self.extract_features(b);
        let tables_overlap = !at.is_empty() && !bt.is_empty() && at.iter().any(|x| bt.contains(x));
        ag == bg
            && al == bl
            && ao == bo
            && ac == bc
            && (aw == bw || (aw as isize - bw as isize).abs() <= 1)
            && (tables_overlap || (at.is_empty() && bt.is_empty()))
    }
    fn similarity_score(&self, a: &str, b: &str) -> f32 {
        let (ag, aw, al, ao, ac, at) = self.extract_features(a);
        let (bg, bw, bl, bo, bc, bt) = self.extract_features(b);
        let mut score: f32 = 0.0;
        let mut total: f32 = 0.0;

        total += 20.0;
        if ag == bg {
            score += 20.0;
        }

        total += 10.0;
        if al == bl {
            score += 10.0;
        }

        total += 10.0;
        if ao == bo {
            score += 10.0;
        }

        total += 10.0;
        if ac == bc {
            score += 10.0;
        }

        total += 20.0;
        let diff = (aw as i32 - bw as i32).abs();
        let where_component = (20.0 - (diff as f32 * 5.0)).max(0.0);
        score += where_component;

        total += 30.0;
        let tables_overlap = !at.is_empty() && !bt.is_empty() && at.iter().any(|x| bt.contains(x));
        if tables_overlap || (at.is_empty() && bt.is_empty()) {
            score += 30.0;
        }
        if total <= 0.0 {
            0.0
        } else {
            (score / total * 100.0).min(100.0)
        }
    }
}
