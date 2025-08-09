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

use crate::graphs::core::{Edge, GraphClient, Node, Record};
use crate::graphs::models::default_thing;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use surrealdb::sql::{Datetime, Thing};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PersonalisationNode {
    #[serde(default = "default_thing")]
    pub id: Thing,
    pub node_type: String,
    pub values: Vec<String>,
    pub embeddings: Vec<f32>,
    pub timestamp: Datetime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PersonalisationEdge {
    #[serde(default = "default_thing")]
    pub id: Thing,
    pub weight: f32,
    pub relation: String,
}

impl Record for PersonalisationNode {
    const TABLE_NAME: &'static str = "personalisation_nodes";
}
impl Node for PersonalisationNode {
    fn get_id(&self) -> &Thing {
        &self.id
    }
}
impl Record for PersonalisationEdge {
    const TABLE_NAME: &'static str = "personalisation_edges";
}
impl Edge for PersonalisationEdge {}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() {
        return 0.0;
    }

    let mut dot_product = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;

    for i in 0..a.len() {
        dot_product += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    (dot_product / (norm_a.sqrt() * norm_b.sqrt())) as f64
}

impl GraphClient<PersonalisationNode, PersonalisationEdge> {
    pub async fn init_schema(&self) -> Result<(), surrealdb::Error> {
        let schema = r#"
            DEFINE TABLE personalisation_nodes SCHEMAFULL;
            DEFINE FIELD node_type ON personalisation_nodes TYPE string;
            DEFINE FIELD values ON personalisation_nodes TYPE array<string>;
            DEFINE FIELD embeddings ON personalisation_nodes TYPE array<float>;
            DEFINE FIELD timestamp ON personalisation_nodes TYPE datetime;
            DEFINE INDEX embedding_idx ON personalisation_nodes FIELDS embeddings MTREE DIMENSION 384;
            DEFINE TABLE personalisation_edges SCHEMAFULL TYPE RELATION IN personalisation_nodes OUT personalisation_nodes;
            DEFINE FIELD weight ON personalisation_edges TYPE float;
            DEFINE FIELD relation ON personalisation_edges TYPE string;
        "#;
        self.define(schema).await
    }

    pub async fn find_similar_nodes(
        &self,
        embedding: Vec<f32>,
        limit: i32,
    ) -> Result<Vec<(PersonalisationNode, f64)>, surrealdb::Error> {
        let sql = "SELECT * FROM personalisation_nodes LIMIT $limit";

        let nodes: Vec<PersonalisationNode> =
            self.query(sql).bind("limit", limit).execute().await?;

        let mut results = Vec::new();
        for node in nodes {
            let similarity = cosine_similarity(&node.embeddings, &embedding);
            results.push((node, similarity));
        }

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(results)
    }
    pub async fn optimise(
        &self,
        user_feedback: HashMap<String, f32>,
    ) -> Result<(), surrealdb::Error> {
        for (node_id_str, score) in user_feedback {
            let node_id = Thing::from((PersonalisationNode::TABLE_NAME.to_string(), node_id_str));
            let sql = "UPDATE personalisation_edges SET weight = weight * $score WHERE in = $node_id OR out = $node_id";
            self.query(sql)
                .bind("score", score)
                .bind("node_id", &node_id)
                .execute::<serde_json::Value>()
                .await?;
        }
        Ok(())
    }
}
