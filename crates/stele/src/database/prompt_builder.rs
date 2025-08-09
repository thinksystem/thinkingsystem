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

use crate::database::query_kg::{KgEdge, KgNode, QueryKnowledgeGraph};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Example {
    #[serde(default)]
    pub setup: Vec<String>,
    pub query: String,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Variant {
    pub name: String,
    pub summary: String,
    #[serde(default)]
    pub syntax: Option<String>,
    #[serde(default)]
    pub examples: Vec<Example>,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Feature {
    pub name: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub syntax: Option<String>,
    #[serde(default)]
    pub examples: Vec<Example>,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Spec {
    pub syntax: String,
    pub summary: String,
    pub variants: Vec<Variant>,
    #[serde(default)]
    pub features: Vec<Feature>,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct InstructionFile {
    #[serde(rename = "type")]
    pub instruction_type: String,
    pub name: String,
    pub spec: Spec,
}
#[derive(Debug, Deserialize, Serialize)]
struct OperatorExample {
    title: String,
    query: String,
}
#[derive(Debug, Deserialize, Serialize)]
struct OperatorSpec {
    #[serde(default)]
    aliases: Vec<String>,
    summary: String,
    signature: String,
    #[serde(default)]
    examples: Vec<OperatorExample>,
}
#[derive(Debug, Deserialize, Serialize)]
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
#[derive(Debug, Clone)]
pub struct LoadedInstruction {
    pub name: String,
    pub instruction_type: String,
    pub file_path: String,
    pub spec: Spec,
    pub triggers: Vec<String>,
    pub priority: u8,
    pub token_estimate: usize,
}
pub struct ContextualPromptEngine {
    loaded_instructions: Vec<LoadedInstruction>,
    context_token_limit: usize,
    knowledge_graph: Arc<QueryKnowledgeGraph>,
}
impl ContextualPromptEngine {
    pub fn new(
        context_token_limit: usize,
        knowledge_graph: Arc<QueryKnowledgeGraph>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let loaded_instructions = Self::load_all_instructions()?;
        info!(
            "ContextualPromptEngine initialised with {} instructions and KG with {} nodes.",
            loaded_instructions.len(),
            knowledge_graph.graph.node_count()
        );
        Ok(Self {
            loaded_instructions,
            context_token_limit,
            knowledge_graph,
        })
    }
    pub fn build_prompt_for_sql_generation(&self, natural_query: &str) -> String {
        let scored_instructions = self.find_and_score_instructions_via_graph(natural_query);
        let prompt_context = self.pack_context(scored_instructions);
        Self::assemble_final_prompt(natural_query, &prompt_context)
    }
    pub fn find_and_score_instructions_via_graph<'a>(
        &'a self,
        query: &str,
    ) -> Vec<(u32, &'a LoadedInstruction)> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();
        let seed_nodes = self.find_seed_nodes(&query_words);
        if seed_nodes.is_empty() {
            warn!("No seed nodes found for query: {}", query);
            return Vec::new();
        }
        let scored_nodes = self.traverse_and_score_graph(seed_nodes);
        self.map_nodes_to_instructions(scored_nodes)
    }
    pub fn find_seed_nodes(&self, query_words: &[&str]) -> Vec<petgraph::graph::NodeIndex> {
        let mut seed_nodes = Vec::new();
        for node_idx in self.knowledge_graph.graph.node_indices() {
            if let Some(node) = self.knowledge_graph.graph.node_weight(node_idx) {
                let node_label = match node {
                    KgNode::Concept(name) => name,
                    KgNode::Clause(name) => name,
                    KgNode::Pattern(_) => continue,
                };
                let label_lower = node_label.to_lowercase();
                if query_words.contains(&label_lower.as_str()) {
                    seed_nodes.push(node_idx);
                    continue;
                }
                if query_words
                    .iter()
                    .any(|&word| label_lower.contains(word) || word.contains(&label_lower))
                {
                    seed_nodes.push(node_idx);
                }
            }
        }
        debug!("Found {} seed nodes for graph traversal", seed_nodes.len());
        seed_nodes
    }
    pub fn traverse_and_score_graph(
        &self,
        seed_nodes: Vec<petgraph::graph::NodeIndex>,
    ) -> Vec<(u32, petgraph::graph::NodeIndex)> {
        let mut scored_nodes: HashMap<petgraph::graph::NodeIndex, u32> = HashMap::new();
        let mut queue: VecDeque<(petgraph::graph::NodeIndex, u32, u32)> = VecDeque::new();
        let max_depth = 3;
        for seed in seed_nodes {
            scored_nodes.insert(seed, 1000);
            queue.push_back((seed, 1000, 0));
        }
        while let Some((current_node, current_score, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            for edge_ref in self
                .knowledge_graph
                .graph
                .edges_directed(current_node, Direction::Outgoing)
            {
                let target_node = edge_ref.target();
                let edge_weight = edge_ref.weight();
                let new_score = self.calculate_edge_score(current_score, edge_weight, depth);
                let should_update = scored_nodes
                    .get(&target_node)
                    .is_none_or(|&existing_score| new_score > existing_score);
                if should_update {
                    scored_nodes.insert(target_node, new_score);
                    queue.push_back((target_node, new_score, depth + 1));
                }
            }
            for edge_ref in self
                .knowledge_graph
                .graph
                .edges_directed(current_node, Direction::Incoming)
            {
                let source_node = edge_ref.source();
                let edge_weight = edge_ref.weight();
                let new_score = self.calculate_edge_score(current_score, edge_weight, depth);
                let should_update = scored_nodes
                    .get(&source_node)
                    .is_none_or(|&existing_score| new_score > existing_score);
                if should_update {
                    scored_nodes.insert(source_node, new_score);
                    queue.push_back((source_node, new_score, depth + 1));
                }
            }
        }
        let mut result: Vec<(u32, petgraph::graph::NodeIndex)> = scored_nodes
            .into_iter()
            .map(|(node, score)| (score, node))
            .collect();
        result.sort_by(|a, b| b.0.cmp(&a.0));
        result
    }
    pub fn calculate_edge_score(&self, current_score: u32, edge: &KgEdge, depth: u32) -> u32 {
        let edge_multiplier = match edge {
            KgEdge::Implements => 0.8,
            KgEdge::CanUse => 0.6,
            KgEdge::IsA => 0.7,
        };
        let depth_decay = 0.5_f64.powi(depth as i32);
        ((current_score as f64) * edge_multiplier * depth_decay) as u32
    }
    fn map_nodes_to_instructions(
        &self,
        scored_nodes: Vec<(u32, petgraph::graph::NodeIndex)>,
    ) -> Vec<(u32, &LoadedInstruction)> {
        let mut result = Vec::new();
        for (score, node_idx) in scored_nodes {
            if let Some(node) = self.knowledge_graph.graph.node_weight(node_idx) {
                if let Some(instruction) = self.find_instruction_for_node(node) {
                    result.push((score, instruction));
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        result.retain(|(_, instruction)| {
            let key = (&instruction.name, &instruction.instruction_type);
            seen.insert(key)
        });
        result
    }
    fn find_instruction_for_node<'a>(&'a self, node: &KgNode) -> Option<&'a LoadedInstruction> {
        let target_name = match node {
            KgNode::Concept(name) => name,
            KgNode::Clause(name) => name,
            KgNode::Pattern(_) => return None,
        };
        self.loaded_instructions.iter().find(|instruction| {
            instruction.name == *target_name
                || instruction
                    .triggers
                    .iter()
                    .any(|trigger| trigger == target_name)
        })
    }
    fn load_all_instructions() -> Result<Vec<LoadedInstruction>, Box<dyn std::error::Error>> {
        let mut instructions = Vec::new();
        let base_path = Path::new("crates/stele/src/database/instructions");
        let instruction_dirs = [("statement", 10), ("clause", 20), ("operator", 30)];
        for (dir, priority) in &instruction_dirs {
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
                    if let Err(e) =
                        Self::load_and_parse_file(&file_path_str, *priority, &mut instructions)
                    {
                        error!(file = %file_path_str, error = %e, "Failed to process documentation file");
                    }
                }
            }
        }
        Ok(instructions)
    }
    fn load_and_parse_file(
        file_path: &str,
        priority: u8,
        instructions: &mut Vec<LoadedInstruction>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let content_str = fs::read_to_string(file_path)?;
        if file_path.ends_with("operators.yaml") {
            return Self::load_operators_file(&content_str, file_path, priority, instructions);
        }
        let file: InstructionFile = serde_yaml::from_str(&content_str)
            .map_err(|e| format!("Failed to parse YAML file {file_path}: {e}"))?;
        let triggers = Self::generate_triggers_for_instruction(&file.name, &file.spec);
        let token_estimate = Self::estimate_tokens(&content_str);
        instructions.push(LoadedInstruction {
            name: file.name,
            instruction_type: file.instruction_type,
            file_path: file_path.to_string(),
            spec: file.spec,
            triggers,
            priority,
            token_estimate,
        });
        Ok(())
    }
    fn load_operators_file(
        content_str: &str,
        file_path: &str,
        priority: u8,
        instructions: &mut Vec<LoadedInstruction>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let operators_file: OperatorsFile = serde_yaml::from_str(content_str)
            .map_err(|e| format!("Failed to parse operators YAML file {file_path}: {e}"))?;
        for (_group_name, group) in operators_file.operator_groups {
            for operator_def in group.definitions {
                let examples: Vec<Example> = operator_def
                    .spec
                    .examples
                    .iter()
                    .map(|ex| Example {
                        setup: vec![],
                        query: ex.query.clone(),
                    })
                    .collect();
                let variant = Variant {
                    name: "Default".to_string(),
                    summary: operator_def.spec.summary.clone(),
                    syntax: Some(operator_def.spec.signature.clone()),
                    examples,
                };
                let spec = Spec {
                    syntax: format!("... {} ...", operator_def.name),
                    summary: operator_def.spec.summary.clone(),
                    variants: vec![variant],
                    features: vec![],
                };
                let mut triggers = vec![operator_def.name.clone()];
                triggers.extend(operator_def.spec.aliases.clone());
                let token_estimate = Self::estimate_tokens(&serde_yaml::to_string(&operator_def)?);
                instructions.push(LoadedInstruction {
                    name: operator_def.name.clone(),
                    instruction_type: "Operator".to_string(),
                    file_path: file_path.to_string(),
                    spec,
                    triggers,
                    priority,
                    token_estimate,
                });
            }
        }
        Ok(())
    }
    fn pack_context(&self, scored_instructions: Vec<(u32, &LoadedInstruction)>) -> String {
        let mut context_parts = Vec::new();
        let mut current_tokens = 0;
        for (score, instruction) in scored_instructions {
            if current_tokens + instruction.token_estimate > self.context_token_limit {
                warn!(
                    "Skipping instruction '{}' to stay within token limit. Score was {}.",
                    instruction.name, score
                );
                continue;
            }
            context_parts.push(Self::format_instruction_for_prompt(instruction));
            current_tokens += instruction.token_estimate;
        }
        debug!(
            "Packed {} instructions into context, using ~{} tokens.",
            context_parts.len(),
            current_tokens
        );
        context_parts.join("\n\n---\n\n")
    }
    fn assemble_final_prompt(natural_query: &str, context: &str) -> String {
        format!(
            r#"SYSTEM PROMPT:
You are an expert system that translates natural language into one or more valid SurrealDB queries.
Your ONLY output must be a single, valid JSON object. Do not include any other text, explanations, or markdown formatting.
The JSON object must have a key named "candidates", which is an array of objects. Each object in the array represents a potential query and must have two keys:
1. "query": A string containing the full, executable SurrealDB query.
2. "confidence": A number from 0.0 to 1.0 indicating your confidence in the query's correctness.
3. "explanation": A brief string explaining your reasoning for this specific query.
Generate 1 to 3 candidate queries. The first candidate should be your most confident answer.
Use the following documentation as your primary reference. Do not use features or syntax not present in this context.
<DOCUMENTATION_CONTEXT>
{context}
</DOCUMENTATION_CONTEXT>
USER QUERY:
{natural_query}
JSON_OUTPUT:
"#
        )
    }
    fn format_instruction_for_prompt(instruction: &LoadedInstruction) -> String {
        let mut parts = vec![
            format!("Instruction Type: {}", instruction.instruction_type),
            format!("Name: {}", instruction.name),
            format!("Summary: {}", instruction.spec.summary),
            format!(
                "General Syntax: {}",
                instruction.spec.syntax.trim().replace('\n', " ")
            ),
        ];
        for variant in &instruction.spec.variants {
            parts.push(format!("\nVariant: {}", variant.name));
            parts.push(format!("  Summary: {}", variant.summary));
            if let Some(syntax) = &variant.syntax {
                parts.push(format!("  Syntax: {}", syntax.trim().replace('\n', " ")));
            }
            if let Some(example) = variant.examples.first() {
                parts.push(format!(
                    "  Example: {}",
                    example.query.trim().replace('\n', " ")
                ));
            }
        }
        parts.join("\n")
    }
    fn generate_triggers_for_instruction(name: &str, spec: &Spec) -> Vec<String> {
        let mut triggers = vec![name.to_lowercase()];
        for variant in &spec.variants {
            triggers.push(variant.name.to_lowercase());
        }
        for feature in &spec.features {
            triggers.push(feature.name.to_lowercase());
        }
        triggers
    }
    fn estimate_tokens(text: &str) -> usize {
        (text.len() as f64 / 4.0).ceil() as usize
    }
}
