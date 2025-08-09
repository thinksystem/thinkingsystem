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

use crate::scribes::core::q_learning_core::QLearningCore;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
const KNOWLEDGE_SCRIBE_STATES: usize = 128;
const KNOWLEDGE_SCRIBE_ACTIONS: usize = 4;
const PCA_HISTORY_SIZE: usize = 200;
const EMBEDDING_DIM: usize = 32;
const SIMILARITY_THRESHOLD_FOR_LINK: f32 = 0.85;
trait EmbeddingModel: Send + Sync + std::fmt::Debug {
    fn embed(&self, text: &str) -> Vec<f32>;
}
#[derive(Debug)]
struct SimpleEmbeddingModel;
impl EmbeddingModel for SimpleEmbeddingModel {
    fn embed(&self, text: &str) -> Vec<f32> {
        let mut vec = vec![0.0; EMBEDDING_DIM];
        for (i, byte) in text.bytes().enumerate() {
            let idx = i % EMBEDDING_DIM;
            vec[idx] += (byte as f32) / 255.0;
        }
        let norm = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut vec {
                *val /= norm;
            }
        }
        vec
    }
}
#[derive(Debug, Clone)]
struct KnowledgeEntity {
    data: Value,
    vector: Vec<f32>,
}
#[derive(Debug, Clone, Default)]
struct KnowledgeGraph {
    entities: HashMap<String, KnowledgeEntity>,
    relations: HashMap<String, HashMap<String, f32>>,
}
#[derive(Debug, Clone)]
pub struct KnowledgeScribe {
    pub id: String,
    cognitive_core: QLearningCore,
    graph: KnowledgeGraph,
    embedding_model: Arc<dyn EmbeddingModel>,
    state_history: VecDeque<Vec<f32>>,
    principal_components: Vec<Vec<f32>>,
    last_state_action: Option<(usize, usize)>,
}
impl KnowledgeScribe {
    pub fn new(id: String) -> Self {
        Self {
            id,
            cognitive_core: QLearningCore::new(
                KNOWLEDGE_SCRIBE_STATES,
                KNOWLEDGE_SCRIBE_ACTIONS,
                0.95,
                0.1,
                0.1,
                16,
            ),
            graph: KnowledgeGraph::default(),
            embedding_model: Arc::new(SimpleEmbeddingModel),
            state_history: VecDeque::with_capacity(PCA_HISTORY_SIZE),
            principal_components: Vec::new(),
            last_state_action: None,
        }
    }
    pub fn get_entity_data(&self, entity_name: &str) -> Option<&Value> {
        self.graph
            .entities
            .get(entity_name)
            .map(|entity| &entity.data)
    }

    pub async fn link_data_to_graph(&mut self, context: &Value) -> Result<Value, String> {
        let state = self.calculate_state(context);

        let empty_vec = vec![];
        let entities = context["entities"].as_array().unwrap_or(&empty_vec);
        let num_entities = entities.len();

        let valid_actions: Vec<usize> = match num_entities {
            0 => vec![],
            1 => vec![0, 1, 2],
            _ => vec![0, 1, 2, 3],
        };

        if valid_actions.is_empty() {
            return Err("Not enough entities to perform any knowledge graph operation".to_string());
        }

        let action = self.cognitive_core.choose_action(state, &valid_actions);
        self.last_state_action = Some((state, action));
        match action {
            0 => self.upsert_and_link_entity(context),
            1 => self.force_create_new_entity(context),
            2 => self.merge_entities(context),
            3 => self.strengthen_relation(context),
            _ => unreachable!(),
        }
    }
    pub fn record_reward(&mut self, reward: f32) {
        if let Some((last_state, last_action)) = self.last_state_action {
            let next_state = self.calculate_state(&json!({}));
            self.cognitive_core
                .add_experience(last_state, last_action, reward, next_state);
            self.cognitive_core.update_q_values();
        }
        self.last_state_action = None;
    }
    pub fn modulate_core(&mut self, aggressiveness: f32) {
        let base_exploration = 0.1;
        let modulated_exploration = base_exploration + (aggressiveness - 0.5) * 0.15;
        self.cognitive_core
            .set_modulated_exploration_rate(modulated_exploration);
    }
    fn calculate_state(&mut self, data: &Value) -> usize {
        let raw_vector = self.get_raw_state_vector(data);
        if self.state_history.len() == PCA_HISTORY_SIZE {
            self.state_history.pop_front();
        }
        self.state_history.push_back(raw_vector.clone());
        let update_frequency = if self.graph.entities.len() < 1000 {
            50
        } else {
            200
        };
        if self.state_history.len() >= PCA_HISTORY_SIZE
            && self.state_history.len() % update_frequency == 0
        {
            self.update_principal_components();
        }
        if self.principal_components.is_empty() {
            return self.bin_raw_vector(&raw_vector);
        }
        let projection = self
            .principal_components
            .iter()
            .map(|pc| dot_product(pc, &raw_vector))
            .collect::<Vec<f32>>();
        let mut state = 0;
        for (i, &val) in projection.iter().enumerate() {
            state += if val > 0.0 { 1 } else { 0 } * (2_usize.pow(i as u32));
        }
        state % KNOWLEDGE_SCRIBE_STATES
    }
    fn get_raw_state_vector(&self, data: &Value) -> Vec<f32> {
        let (similarity, most_similar_id) = self.calculate_similarity(data);
        let pagerank = self.calculate_pagerank();
        let communities = self.find_communities_leiden();
        vec![
            self.graph.entities.len() as f32,
            self.graph
                .relations
                .values()
                .map(|r| r.len())
                .sum::<usize>() as f32,
            pagerank
                .get(most_similar_id.as_deref().unwrap_or(""))
                .cloned()
                .unwrap_or(0.0),
            communities.values().max().cloned().unwrap_or(0) as f32 + 1.0,
            1.0 - similarity,
            similarity,
            data["entities"].as_array().map_or(0, |e| e.len()) as f32,
        ]
    }
    fn upsert_and_link_entity(&mut self, data: &Value) -> Result<Value, String> {
        let (similarity, best_match_id) = self.calculate_similarity(data);
        let source_entity_id = data["entities"][0]
            .as_str()
            .ok_or("Missing source entity")?
            .to_string();
        if similarity > SIMILARITY_THRESHOLD_FOR_LINK && best_match_id.is_some() {
            let target_id = best_match_id.unwrap();
            self.strengthen_specific_relation(&source_entity_id, &target_id);
            Ok(
                json!({ "status": "linked", "source": source_entity_id, "target": target_id, "similarity": similarity }),
            )
        } else {
            self.create_new_entity_internal(&source_entity_id, data)
        }
    }
    fn force_create_new_entity(&mut self, data: &Value) -> Result<Value, String> {
        let source_entity_id = data["entities"][0]
            .as_str()
            .ok_or("Missing source entity")?
            .to_string();
        self.create_new_entity_internal(&source_entity_id, data)
    }
    fn merge_entities(&mut self, data: &Value) -> Result<Value, String> {
        let (_, target_id_opt) = self.calculate_similarity(data);
        let target_id = target_id_opt.ok_or("No entity to merge into.")?;
        let source_id = data["entities"][0].as_str().unwrap_or("").to_string();
        if let Some(source_entity) = self.graph.entities.get(&source_id).cloned() {
            if let Some(target_entity) = self.graph.entities.get_mut(&target_id) {
                target_entity.vector = source_entity
                    .vector
                    .iter()
                    .zip(&target_entity.vector)
                    .map(|(a, b)| (a + b) / 2.0)
                    .collect();
                return Ok(
                    json!({"status": "merged", "source": source_id, "into_entity": target_id}),
                );
            }
        }
        Err("Merge failed: source or target not found".to_string())
    }
    fn strengthen_relation(&mut self, data: &Value) -> Result<Value, String> {
        let entities = data["entities"].as_array().ok_or("No entities in data")?;
        if entities.len() < 2 {
            return Err("Not enough entities to strengthen a relation".to_string());
        }
        let id1 = entities[0].as_str().ok_or("Invalid entity")?.to_string();
        let id2 = entities[1].as_str().ok_or("Invalid entity")?.to_string();
        self.strengthen_specific_relation(&id1, &id2);
        Ok(json!({"status": "relation_strengthened", "between": [id1, id2]}))
    }
    fn calculate_similarity(&self, data: &Value) -> (f32, Option<String>) {
        let entity_name = data["entities"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if self.graph.entities.is_empty() || entity_name.is_empty() {
            return (0.0, None);
        }
        let incoming_vec = self.embedding_model.embed(entity_name);
        let best_match = self
            .graph
            .entities
            .iter()
            .map(|(id, entity)| (id.clone(), cosine_similarity(&incoming_vec, &entity.vector)))
            .max_by(|(_, sim_a), (_, sim_b)| {
                sim_a
                    .partial_cmp(sim_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        if let Some((id, max_sim)) = best_match {
            (max_sim, Some(id))
        } else {
            (0.0, None)
        }
    }
    fn calculate_pagerank(&self) -> HashMap<String, f32> {
        const DAMPING_FACTOR: f32 = 0.85;
        const ITERATIONS: usize = 20;
        let mut scores = HashMap::new();
        let n = self.graph.entities.len();
        if n == 0 {
            return scores;
        }
        let initial_score = 1.0 / n as f32;
        let mut out_links: HashMap<String, usize> = HashMap::new();
        for id in self.graph.entities.keys() {
            scores.insert(id.clone(), initial_score);
            out_links.insert(
                id.clone(),
                self.graph.relations.get(id).map_or(0, |r| r.len()),
            );
        }
        for _ in 0..ITERATIONS {
            let mut new_scores = scores.clone();
            for id in self.graph.entities.keys() {
                let mut rank_sum = 0.0;
                for (source_id, source_rels) in &self.graph.relations {
                    if source_rels.contains_key(id) {
                        if let Some(source_score) = scores.get(source_id) {
                            rank_sum +=
                                source_score / *out_links.get(source_id).unwrap_or(&1) as f32;
                        }
                    }
                }
                new_scores.insert(
                    id.clone(),
                    ((1.0 - DAMPING_FACTOR) / n as f32) + DAMPING_FACTOR * rank_sum,
                );
            }
            scores = new_scores;
        }
        scores
    }
    fn find_communities_leiden(&self) -> HashMap<String, usize> {
        let nodes: Vec<String> = self.graph.entities.keys().cloned().collect();
        if nodes.is_empty() {
            return HashMap::new();
        }
        let modularity_manager = Modularity::new(&self.graph);
        let mut partition: HashMap<String, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let mut improvement = true;
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 100;
        while improvement && iterations < MAX_ITERATIONS {
            iterations += 1;
            improvement = false;
            for node in &nodes {
                let best_community = modularity_manager.find_best_move(node, &partition);
                if let Some(new_community) = best_community {
                    if partition[node] != new_community {
                        partition.insert(node.clone(), new_community);
                        improvement = true;
                    }
                }
            }
            let mut refined_partition = partition.clone();
            let mut next_community_id = partition.values().max().cloned().unwrap_or(0) + 1;
            for (node, community_id) in &partition {
                let (internal_weight, _) =
                    modularity_manager.get_node_connectivity(node, *community_id, &partition);
                if internal_weight < 0.1 {
                    refined_partition.insert(node.clone(), next_community_id);
                    next_community_id += 1;
                    improvement = true;
                }
            }
            partition = refined_partition;
        }
        partition
    }
    fn create_new_entity_internal(&mut self, name: &str, data: &Value) -> Result<Value, String> {
        if self.graph.entities.contains_key(name) {
            return Err("Entity already exists".to_string());
        }
        let new_entity = KnowledgeEntity {
            data: data.clone(),
            vector: self.embedding_model.embed(name),
        };
        self.graph.entities.insert(name.to_string(), new_entity);
        Ok(json!({ "status": "created", "entity": name }))
    }
    fn strengthen_specific_relation(&mut self, id1: &str, id2: &str) {
        let entry1 = self.graph.relations.entry(id1.to_string()).or_default();
        *entry1.entry(id2.to_string()).or_insert(0.5) =
            (*entry1.get(id2).unwrap_or(&0.5) + 0.1).min(1.0);
        let entry2 = self.graph.relations.entry(id2.to_string()).or_default();
        *entry2.entry(id1.to_string()).or_insert(0.5) =
            (*entry2.get(id1).unwrap_or(&0.5) + 0.1).min(1.0);
    }
    fn bin_raw_vector(&self, vector: &[f32]) -> usize {
        let density_bin = (vector[1].clamp(0.0, 10.0) / 10.0 * 4.0).floor() as usize;
        let novelty_bin = (vector[4].clamp(0.0, 1.0) * 4.0).floor() as usize;
        (density_bin * 4 + novelty_bin) % KNOWLEDGE_SCRIBE_STATES
    }
    fn update_principal_components(&mut self) {
        if self.state_history.len() < 2 {
            return;
        }
        let num_features = self.state_history[0].len();
        let num_samples = self.state_history.len();
        let mut mean = vec![0.0; num_features];
        for row in &self.state_history {
            for (i, &val) in row.iter().enumerate() {
                mean[i] += val;
            }
        }
        for val in &mut mean {
            *val /= num_samples as f32;
        }
        let mut cov_matrix = vec![vec![0.0; num_features]; num_features];
        for row in &self.state_history {
            let centered_row: Vec<f32> = row.iter().zip(&mean).map(|(r, m)| r - m).collect();
            for i in 0..num_features {
                for j in 0..num_features {
                    cov_matrix[i][j] += centered_row[i] * centered_row[j];
                }
            }
        }
        for row in &mut cov_matrix {
            for val in row {
                *val /= (num_samples - 1) as f32;
            }
        }
        let mut b = vec![1.0; num_features];
        for _ in 0..10 {
            let mut new_b = vec![0.0; num_features];
            for i in 0..num_features {
                for (j, &b_val) in b.iter().enumerate() {
                    new_b[i] += cov_matrix[i][j] * b_val;
                }
            }
            let norm = new_b.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                b = new_b.iter().map(|x| x / norm).collect();
            }
        }
        self.principal_components = vec![b];
    }
}
struct Modularity<'a> {
    graph: &'a KnowledgeGraph,
    total_edge_weight: f32,
    node_degrees: HashMap<String, f32>,
}
impl<'a> Modularity<'a> {
    fn new(graph: &'a KnowledgeGraph) -> Self {
        let mut total_edge_weight = 0.0;
        let mut node_degrees = HashMap::new();
        for (source, targets) in &graph.relations {
            let degree: f32 = targets.values().sum();
            total_edge_weight += degree;
            node_degrees.insert(source.clone(), degree);
        }
        Self {
            graph,
            total_edge_weight: total_edge_weight / 2.0,
            node_degrees,
        }
    }
    fn find_best_move(&self, node_id: &str, partition: &HashMap<String, usize>) -> Option<usize> {
        let mut best_community = partition[node_id];
        let mut max_gain = 0.0;
        let neighbours = self.graph.relations.get(node_id)?;
        for neighbour_id in neighbours.keys() {
            if let Some(&neighbour_community) = partition.get(neighbour_id) {
                let gain = self.calculate_modularity_gain(node_id, neighbour_community, partition);
                if gain > max_gain {
                    max_gain = gain;
                    best_community = neighbour_community;
                }
            }
        }
        Some(best_community)
    }
    fn calculate_modularity_gain(
        &self,
        node_id: &str,
        target_community: usize,
        partition: &HashMap<String, usize>,
    ) -> f32 {
        let (k_i_in, _) = self.get_node_connectivity(node_id, target_community, partition);
        let sigma_tot = self.get_community_weight(target_community, partition);
        let k_i = self.node_degrees.get(node_id).cloned().unwrap_or(0.0);
        if self.total_edge_weight == 0.0 {
            return 0.0;
        }
        k_i_in / self.total_edge_weight - (sigma_tot * k_i) / (2.0 * self.total_edge_weight.powi(2))
    }
    fn get_community_weight(&self, community_id: usize, partition: &HashMap<String, usize>) -> f32 {
        partition
            .iter()
            .filter(|(_, &comm)| comm == community_id)
            .map(|(node, _)| self.node_degrees.get(node).cloned().unwrap_or(0.0))
            .sum()
    }
    fn get_node_connectivity(
        &self,
        node_id: &str,
        target_community: usize,
        partition: &HashMap<String, usize>,
    ) -> (f32, HashMap<usize, f32>) {
        let mut internal_weight = 0.0;
        let mut external_weights = HashMap::new();
        if let Some(neighbours) = self.graph.relations.get(node_id) {
            for (neighbour_id, weight) in neighbours {
                if let Some(&neighbour_community) = partition.get(neighbour_id) {
                    if neighbour_community == target_community {
                        internal_weight += weight;
                    } else {
                        *external_weights.entry(neighbour_community).or_insert(0.0) += weight;
                    }
                }
            }
        }
        (internal_weight, external_weights)
    }
}
fn dot_product(vec_a: &[f32], vec_b: &[f32]) -> f32 {
    vec_a.iter().zip(vec_b).map(|(a, b)| a * b).sum()
}
fn cosine_similarity(vec_a: &[f32], vec_b: &[f32]) -> f32 {
    let dot = dot_product(vec_a, vec_b);
    let mag_a = dot_product(vec_a, vec_a).sqrt();
    let mag_b = dot_product(vec_b, vec_b).sqrt();
    if mag_a == 0.0 || mag_b == 0.0 {
        0.0
    } else {
        dot / (mag_a * mag_b)
    }
}
