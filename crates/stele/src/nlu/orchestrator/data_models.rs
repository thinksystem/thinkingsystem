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

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum SegmentType {
    Statement { intent: String },
    Question { expected_answer_type: String },
    Command { operation: String },
    Relationship { from: String, to: String },
}
impl Default for SegmentType {
    fn default() -> Self {
        SegmentType::Statement {
            intent: "unknown".to_string(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InputSegment {
    pub text: String,
    pub segment_type: SegmentType,
    #[serde(default)]
    pub tokens: Vec<String>,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub dependencies: Vec<usize>,
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}
impl InputSegment {
    pub fn new(text: String, segment_type: SegmentType) -> Self {
        Self {
            text,
            segment_type,
            ..Default::default()
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Entity {
    #[serde(default)]
    pub temp_id: String,
    #[serde(alias = "value")]
    pub name: String,
    #[serde(alias = "type")]
    pub entity_type: String,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub metadata: Option<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TemporalMarker {
    #[serde(default)]
    pub temp_id: String,
    #[serde(alias = "text")]
    pub date_text: String,
    pub resolved_date: Option<String>,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub metadata: Option<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NumericalValue {
    #[serde(default)]
    pub temp_id: String,
    pub value: f64,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub metadata: Option<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Action {
    #[serde(default)]
    pub temp_id: String,
    pub verb: String,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub metadata: Option<Value>,
}
impl Eq for Action {}
impl std::hash::Hash for Action {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.temp_id.hash(state);
        self.verb.hash(state);
        if let Some(ref meta) = self.metadata {
            meta.to_string().hash(state);
        } else {
            "None".hash(state);
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "node_type", content = "data")]
pub enum KnowledgeNode {
    Entity(Entity),
    Temporal(TemporalMarker),
    Numerical(NumericalValue),
    Action(Action),
}
impl KnowledgeNode {
    pub fn temp_id(&self) -> &str {
        match self {
            KnowledgeNode::Entity(e) => &e.temp_id,
            KnowledgeNode::Temporal(t) => &t.temp_id,
            KnowledgeNode::Numerical(n) => &n.temp_id,
            KnowledgeNode::Action(a) => &a.temp_id,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Relationship {
    pub source: String,
    pub target: String,
    #[serde(alias = "predicate")]
    pub relation_type: String,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub metadata: Option<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtractedData {
    #[serde(default)]
    pub nodes: Vec<KnowledgeNode>,
    #[serde(default)]
    pub relationships: Vec<Relationship>,
}
impl ExtractedData {
    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.nodes.iter().filter_map(|node| {
            if let KnowledgeNode::Entity(e) = node {
                Some(e)
            } else {
                None
            }
        })
    }
    pub fn actions(&self) -> impl Iterator<Item = &Action> {
        self.nodes.iter().filter_map(|node| {
            if let KnowledgeNode::Action(a) = node {
                Some(a)
            } else {
                None
            }
        })
    }
    pub fn temporal_markers(&self) -> impl Iterator<Item = &TemporalMarker> {
        self.nodes.iter().filter_map(|node| {
            if let KnowledgeNode::Temporal(t) = node {
                Some(t)
            } else {
                None
            }
        })
    }
    pub fn numerical_values(&self) -> impl Iterator<Item = &NumericalValue> {
        self.nodes.iter().filter_map(|node| {
            if let KnowledgeNode::Numerical(n) = node {
                Some(n)
            } else {
                None
            }
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessingMetadata {
    #[serde(default)]
    pub strategy_used: String,
    #[serde(default)]
    pub models_used: Vec<String>,
    #[serde(default)]
    pub execution_time_ms: u64,
    #[serde(default)]
    pub total_cost_estimate: f64,
    #[serde(default)]
    pub confidence_scores: HashMap<String, f64>,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub sentiment_score: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedNLUData {
    pub segments: Vec<InputSegment>,
    pub extracted_data: ExtractedData,
    pub processing_metadata: ProcessingMetadata,
}
impl UnifiedNLUData {
    pub fn get_raw_text(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<&str>>()
            .join(" ")
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.segments.is_empty() {
            return Err("No segments found in NLU data".to_string());
        }
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub struct TaskOutput {
    pub task_name: String,
    pub data: serde_json::Value,
    pub model_used: String,
    pub execution_time: std::time::Duration,
    pub success: bool,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum QueryComplexity {
    SimpleLookup,
    ComplexGraph,
    Federated,
    #[default]
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TraversalInfo {
    pub hops: u8,
    pub via_relationships: Vec<String>,
    pub direction: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdvancedQueryIntent {
    pub complexity: QueryComplexity,
    pub entities: Vec<String>,
    pub relationships: Vec<String>,
    pub traversals: Vec<TraversalInfo>,
    pub filters: HashMap<String, Value>,
    pub original_query: String,
}
