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

use super::data_structures::*;
use std::collections::{HashMap, HashSet};
#[derive(Debug, Clone)]
pub struct FieldRelationshipGraph {
    pub nodes: HashMap<String, FieldNode>,
    pub edges: Vec<FieldEdge>,
    adjacency: HashMap<String, Vec<String>>,
}
#[derive(Debug, Clone)]
pub struct FieldNode {
    pub name: String,
    pub profile: ColumnProfile,
    pub embedding: Vec<f64>,
    pub centrality_score: f64,
    pub semantic_tags: HashSet<String>,
}
#[derive(Debug, Clone)]
pub struct FieldEdge {
    pub source: String,
    pub target: String,
    pub relationship_type: RelationshipType,
    pub strength: f64,
    pub statistical_evidence: StatisticalEvidence,
}
#[derive(Debug, Clone, PartialEq)]
pub enum RelationshipType {
    Correlation,
    Causation,
    Hierarchy,
    Similarity,
    Complement,
    Substitution,
    Temporal,
    Categorical,
}
#[derive(Debug, Clone)]
pub struct StatisticalEvidence {
    pub mutual_information: f64,
    pub correlation_coefficient: f64,
    pub distance_correlation: f64,
    pub entropy_ratio: f64,
    pub co_occurrence_frequency: f64,
}
impl FieldRelationshipGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            adjacency: HashMap::new(),
        }
    }
    pub fn add_field(&mut self, profile: ColumnProfile) {
        let embedding = self.compute_field_embedding(&profile);
        let semantic_tags = self.extract_semantic_tags(&profile);
        let node = FieldNode {
            name: profile.name.clone(),
            profile,
            embedding,
            centrality_score: 0.0,
            semantic_tags,
        };
        let key = node.name.clone();
        self.nodes.insert(key.clone(), node);
        self.adjacency.insert(key, Vec::new());
    }
    pub fn analyse_relationships(&mut self, data_sample: Option<&[HashMap<String, String>]>) {
        self.compute_pairwise_relationships(data_sample);
        self.compute_centrality_scores();
        self.detect_field_communities();
    }
    fn compute_field_embedding(&self, profile: &ColumnProfile) -> Vec<f64> {
        let mut embedding = vec![0.0; 16];
        embedding[0] = profile.cardinality.unwrap_or(0) as f64 / 1000.0;
        embedding[1] = if profile.has_nulls { 1.0 } else { 0.0 };
        match profile.data_type {
            DataType::Numeric => { embedding[2] = 1.0; embedding[8] = profile.cardinality.unwrap_or(0) as f64 / 10000.0; }
            DataType::Categorical => { embedding[3] = 1.0; embedding[9] = (profile.cardinality.unwrap_or(0) as f64).ln() / 10.0; }
            DataType::Temporal => { embedding[4] = 1.0; embedding[10] = 1.0; }
            DataType::Boolean => { embedding[5] = 1.0; embedding[11] = 0.1; }
        }
        let name_lower = profile.name.to_lowercase();
        embedding[6] = if name_lower.contains("date") || name_lower.contains("time") { 1.0 } else { 0.0 };
        embedding[7] = if name_lower.contains("id") || name_lower.ends_with("_id") { 1.0 } else { 0.0 };
        embedding[12] = if name_lower.contains("sales") || name_lower.contains("revenue") || name_lower.contains("amount") { 1.0 } else { 0.0 };
        embedding[13] = if name_lower.contains("count") || name_lower.contains("quantity") { 1.0 } else { 0.0 };
        embedding[14] = if name_lower.contains("rate") || name_lower.contains("percent") || name_lower.contains("ratio") { 1.0 } else { 0.0 };
        embedding[15] = if name_lower.contains("name") || name_lower.contains("category") || name_lower.contains("type") { 1.0 } else { 0.0 };
        embedding
    }
    fn extract_semantic_tags(&self, profile: &ColumnProfile) -> HashSet<String> {
        let mut tags = HashSet::new();
        let name_lower = profile.name.to_lowercase();
        if name_lower.contains("date") || name_lower.contains("time") || profile.data_type == DataType::Temporal {
            tags.insert("temporal".to_string());
        }
        if name_lower.contains("id") || name_lower.ends_with("_id") {
            tags.insert("identifier".to_string());
        }
        if name_lower.contains("sales") || name_lower.contains("revenue") || name_lower.contains("amount") {
            tags.insert("monetary".to_string());
        }
        if name_lower.contains("count") || name_lower.contains("quantity") {
            tags.insert("quantity".to_string());
        }
        if profile.data_type == DataType::Numeric {
            if profile.cardinality.unwrap_or(0) < 20 {
                tags.insert("discrete_numeric".to_string());
            } else {
                tags.insert("continuous_numeric".to_string());
            }
        }
        if profile.data_type == DataType::Categorical {
            if profile.cardinality.unwrap_or(0) <= 7 {
                tags.insert("low_cardinality_categorical".to_string());
            } else {
                tags.insert("high_cardinality_categorical".to_string());
            }
        }
        tags
    }
    fn compute_pairwise_relationships(&mut self, data_sample: Option<&[HashMap<String, String>]>) {
        let field_names: Vec<String> = self.nodes.keys().cloned().collect();
        for i in 0..field_names.len() {
            for j in (i + 1)..field_names.len() {
                let field_a = &field_names[i];
                let field_b = &field_names[j];
                if let (Some(node_a), Some(node_b)) = (self.nodes.get(field_a), self.nodes.get(field_b)) {
                    let relationship = self.analyse_field_pair(node_a, node_b, data_sample);
                    if relationship.strength > 0.1 {
                        self.edges.push(relationship.clone());
                        self.adjacency.get_mut(field_a).unwrap().push(field_b.clone());
                        self.adjacency.get_mut(field_b).unwrap().push(field_a.clone());
                    }
                }
            }
        }
    }
    fn analyse_field_pair(
        &self,
        node_a: &FieldNode,
        node_b: &FieldNode,
        _data_sample: Option<&[HashMap<String, String>]>
    ) -> FieldEdge {
        let embedding_similarity = cosine_similarity(&node_a.embedding, &node_b.embedding);
        let tag_overlap = node_a.semantic_tags.intersection(&node_b.semantic_tags).count() as f64 /
                         (node_a.semantic_tags.len().max(node_b.semantic_tags.len()) as f64).max(1.0);
        let type_compatibility = self.compute_type_compatibility(&node_a.profile, &node_b.profile);
        let (relationship_type, base_strength) = self.infer_relationship_type(node_a, node_b);
        let final_strength = (embedding_similarity * 0.4 + tag_overlap * 0.3 + type_compatibility * 0.3) * base_strength;
        let statistical_evidence = StatisticalEvidence {
            mutual_information: embedding_similarity * 0.8,
            correlation_coefficient: if matches!(relationship_type, RelationshipType::Correlation) { embedding_similarity } else { 0.1 },
            distance_correlation: embedding_similarity * 0.7,
            entropy_ratio: tag_overlap,
            co_occurrence_frequency: final_strength,
        };
        FieldEdge {
            source: node_a.name.clone(),
            target: node_b.name.clone(),
            relationship_type,
            strength: final_strength,
            statistical_evidence,
        }
    }
    fn compute_type_compatibility(&self, profile_a: &ColumnProfile, profile_b: &ColumnProfile) -> f64 {
        match (&profile_a.data_type, &profile_b.data_type) {
            (DataType::Numeric, DataType::Numeric) => 1.0,
            (DataType::Categorical, DataType::Categorical) => 0.8,
            (DataType::Temporal, DataType::Temporal) => 1.0,
            (DataType::Numeric, DataType::Temporal) | (DataType::Temporal, DataType::Numeric) => 0.7,
            (DataType::Categorical, DataType::Numeric) | (DataType::Numeric, DataType::Categorical) => 0.5,
            (DataType::Boolean, _) | (_, DataType::Boolean) => 0.3,
            _ => 0.2,
        }
    }
    fn infer_relationship_type(&self, node_a: &FieldNode, node_b: &FieldNode) -> (RelationshipType, f64) {
        if node_a.semantic_tags.contains("temporal") || node_b.semantic_tags.contains("temporal") {
            return (RelationshipType::Temporal, 0.9);
        }
        if node_a.semantic_tags.contains("identifier") || node_b.semantic_tags.contains("identifier") {
            return (RelationshipType::Hierarchy, 0.8);
        }
        if (node_a.semantic_tags.contains("monetary") && node_b.semantic_tags.contains("quantity")) ||
           (node_b.semantic_tags.contains("monetary") && node_a.semantic_tags.contains("quantity")) {
            return (RelationshipType::Complement, 0.9);
        }
        let shared_semantic_tags = node_a.semantic_tags.intersection(&node_b.semantic_tags).count();
        if shared_semantic_tags > 0 {
            return (RelationshipType::Similarity, 0.7);
        }
        if matches!(node_a.profile.data_type, DataType::Numeric) &&
           matches!(node_b.profile.data_type, DataType::Numeric) {
            return (RelationshipType::Correlation, 0.6);
        }
        (RelationshipType::Similarity, 0.3)
    }
    fn compute_centrality_scores(&mut self) {
        let nodes_len = self.nodes.len();
        for (field_name, neighbours) in &self.adjacency {
            if let Some(node) = self.nodes.get_mut(field_name) {
                node.centrality_score = neighbours.len() as f64 / (nodes_len.saturating_sub(1)) as f64;
            }
        }
    }
    fn detect_field_communities(&mut self) {
    }
    pub fn get_visualisation_recommendations(&self, goal: &AnalysisGoal) -> Vec<FieldRecommendation> {
        let mut recommendations = Vec::new();
        match goal {
            AnalysisGoal::FindRelationship => {
                for edge in &self.edges {
                    if matches!(edge.relationship_type, RelationshipType::Correlation | RelationshipType::Complement) &&
                       edge.strength > 0.7 {
                        recommendations.push(FieldRecommendation {
                            field_pair: (edge.source.clone(), edge.target.clone()),
                            recommended_chart_types: vec![ChartType::Scatter],
                            confidence: edge.strength,
                            reasoning: format!("Strong {} relationship detected (strength: {:.2})",
                                             format!("{:?}", edge.relationship_type).to_lowercase(),
                                             edge.strength),
                        });
                    }
                }
            }
            AnalysisGoal::ShowTrend => {
                for edge in &self.edges {
                    if matches!(edge.relationship_type, RelationshipType::Temporal) && edge.strength > 0.6 {
                        let (temporal_field, numeric_field) = if self.nodes.get(&edge.source).unwrap().semantic_tags.contains("temporal") {
                            (&edge.source, &edge.target)
                        } else {
                            (&edge.target, &edge.source)
                        };
                        if let Some(numeric_node) = self.nodes.get(numeric_field) {
                            if matches!(numeric_node.profile.data_type, DataType::Numeric) {
                                recommendations.push(FieldRecommendation {
                                    field_pair: (temporal_field.clone(), numeric_field.clone()),
                                    recommended_chart_types: vec![ChartType::Line],
                                    confidence: edge.strength,
                                    reasoning: format!("Temporal field '{temporal_field}' paired with numeric field '{numeric_field}' ideal for trend analysis"),
                                });
                            }
                        }
                    }
                }
            }
            _ => {
            }
        }
        recommendations.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        recommendations.truncate(5);
        recommendations
    }
    pub fn get_field_insights(&self, field_name: &str) -> Option<FieldInsights> {
        self.nodes.get(field_name).map(|node| {
            let connected_fields: Vec<String> = self.adjacency.get(field_name)
                .unwrap_or(&vec![])
                .clone();
            let strongest_relationships: Vec<_> = self.edges.iter()
                .filter(|edge| edge.source == field_name || edge.target == field_name)
                .cloned()
                .collect();
            FieldInsights {
                field_name: field_name.to_string(),
                centrality_score: node.centrality_score,
                semantic_tags: node.semantic_tags.clone(),
                connected_fields,
                strongest_relationships,
                visualisation_potential: self.assess_visualisation_potential(node),
            }
        })
    }
    fn assess_visualisation_potential(&self, node: &FieldNode) -> VisualisationPotential {
        let mut potential = VisualisationPotential {
            as_x_axis: 0.0,
            as_y_axis: 0.0,
            as_colour_encoding: 0.0,
            as_size_encoding: 0.0,
            recommended_roles: Vec::new(),
        };
        match node.profile.data_type {
            DataType::Categorical => {
                let cardinality = node.profile.cardinality.unwrap_or(0);
                potential.as_x_axis = if cardinality <= 20 { 0.9 } else { 0.3 };
                potential.as_colour_encoding = if cardinality <= 7 { 0.9 } else { 0.2 };
                potential.recommended_roles.push("categorical_axis".to_string());
                if cardinality <= 7 {
                    potential.recommended_roles.push("colour_encoding".to_string());
                }
            }
            DataType::Numeric => {
                potential.as_y_axis = 0.9;
                potential.as_size_encoding = 0.8;
                if node.semantic_tags.contains("discrete_numeric") {
                    potential.as_x_axis = 0.6;
                }
                potential.recommended_roles.push("measure".to_string());
                potential.recommended_roles.push("size_encoding".to_string());
            }
            DataType::Temporal => {
                potential.as_x_axis = 0.95;
                potential.recommended_roles.push("time_axis".to_string());
            }
            DataType::Boolean => {
                potential.as_colour_encoding = 0.7;
                potential.recommended_roles.push("binary_encoding".to_string());
            }
        }
        potential
    }
}
impl Default for FieldRelationshipGraph {
    fn default() -> Self { Self::new() }
}
#[derive(Debug, Clone)]
pub struct FieldRecommendation {
    pub field_pair: (String, String),
    pub recommended_chart_types: Vec<ChartType>,
    pub confidence: f64,
    pub reasoning: String,
}
#[derive(Debug, Clone)]
pub struct FieldInsights {
    pub field_name: String,
    pub centrality_score: f64,
    pub semantic_tags: HashSet<String>,
    pub connected_fields: Vec<String>,
    pub strongest_relationships: Vec<FieldEdge>,
    pub visualisation_potential: VisualisationPotential,
}
#[derive(Debug, Clone)]
pub struct VisualisationPotential {
    pub as_x_axis: f64,
    pub as_y_axis: f64,
    pub as_colour_encoding: f64,
    pub as_size_encoding: f64,
    pub recommended_roles: Vec<String>,
}
fn cosine_similarity(vec_a: &[f64], vec_b: &[f64]) -> f64 {
    if vec_a.len() != vec_b.len() {
        return 0.0;
    }
    let dot_product: f64 = vec_a.iter().zip(vec_b).map(|(a, b)| a * b).sum();
    let magnitude_a: f64 = vec_a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let magnitude_b: f64 = vec_b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        0.0
    } else {
        dot_product / (magnitude_a * magnitude_b)
    }
}
