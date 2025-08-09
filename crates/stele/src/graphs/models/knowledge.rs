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
use std::collections::HashMap;
use surrealdb::sql::Thing;

use crate::graphs::core::{Edge, GraphClient, Node, Record};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptNode {
    pub id: Thing,
    pub name: String,
    pub category: String,
    pub description: String,
    pub confidence: f64,
    pub properties: HashMap<String, serde_json::Value>,
    pub embedding: Option<Vec<f64>>,
}

impl Record for ConceptNode {
    const TABLE_NAME: &'static str = "concepts";
}

impl Node for ConceptNode {
    fn get_id(&self) -> &Thing {
        &self.id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticEdge {
    pub id: Thing,
    pub relationship_type: RelationshipType,
    pub strength: f64,
    pub confidence: f64,
    pub context: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Record for SemanticEdge {
    const TABLE_NAME: &'static str = "semantic_relations";
}

impl Edge for SemanticEdge {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RelationshipType {
    IsA,
    PartOf,
    Contains,

    Uses,
    Enables,
    Requires,

    Similar,
    Opposite,
    Related,

    Precedes,
    Follows,

    Custom(String),
}

impl std::fmt::Display for RelationshipType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelationshipType::IsA => write!(f, "is-a"),
            RelationshipType::PartOf => write!(f, "part-of"),
            RelationshipType::Contains => write!(f, "contains"),
            RelationshipType::Uses => write!(f, "uses"),
            RelationshipType::Enables => write!(f, "enables"),
            RelationshipType::Requires => write!(f, "requires"),
            RelationshipType::Similar => write!(f, "similar"),
            RelationshipType::Opposite => write!(f, "opposite"),
            RelationshipType::Related => write!(f, "related"),
            RelationshipType::Precedes => write!(f, "precedes"),
            RelationshipType::Follows => write!(f, "follows"),
            RelationshipType::Custom(name) => write!(f, "{name}"),
        }
    }
}

impl GraphClient<ConceptNode, SemanticEdge> {
    pub async fn init_knowledge_schema(&self) -> Result<(), surrealdb::Error> {
        self.define(
            "DEFINE TABLE concepts SCHEMAFULL;
             DEFINE FIELD name ON concepts TYPE string;
             DEFINE FIELD category ON concepts TYPE string;
             DEFINE FIELD description ON concepts TYPE string;
             DEFINE FIELD confidence ON concepts TYPE number ASSERT $value >= 0 AND $value <= 1;
             DEFINE FIELD properties ON concepts TYPE object;
             DEFINE FIELD embedding ON concepts TYPE array;
             DEFINE INDEX concepts_name_idx ON concepts COLUMNS name UNIQUE;
             DEFINE INDEX concepts_category_idx ON concepts COLUMNS category;",
        )
        .await?;

        self.define(
            "DEFINE TABLE semantic_relations SCHEMAFULL PERMISSIONS NONE TYPE RELATION;
             DEFINE FIELD relationship_type ON semantic_relations TYPE string;
             DEFINE FIELD strength ON semantic_relations TYPE number ASSERT $value >= 0 AND $value <= 1;
             DEFINE FIELD confidence ON semantic_relations TYPE number ASSERT $value >= 0 AND $value <= 1;
             DEFINE FIELD context ON semantic_relations TYPE option<string>;
             DEFINE FIELD metadata ON semantic_relations TYPE object;"
        ).await?;

        Ok(())
    }

    pub async fn find_concepts_by_category(
        &self,
        category: &str,
    ) -> Result<Vec<ConceptNode>, surrealdb::Error> {
        let sql = "SELECT * FROM concepts WHERE category = $category ORDER BY confidence DESC";
        self.query(sql).bind("category", category).execute().await
    }

    pub async fn find_similar_concepts(
        &self,
        target_embedding: &[f64],
        threshold: f64,
        limit: usize,
    ) -> Result<Vec<ConceptNode>, surrealdb::Error> {
        let sql = "SELECT * FROM concepts WHERE embedding IS NOT NULL LIMIT $limit";
        let concepts: Vec<ConceptNode> = self.query(sql).bind("limit", limit).execute().await?;

        let similar_concepts: Vec<ConceptNode> = concepts
            .into_iter()
            .filter(|concept| {
                if let Some(ref embedding) = concept.embedding {
                    cosine_similarity(target_embedding, embedding) >= threshold
                } else {
                    false
                }
            })
            .collect();

        Ok(similar_concepts)
    }

    pub async fn get_related_concepts(
        &self,
        concept_id: &Thing,
        relationship_type: &RelationshipType,
    ) -> Result<Vec<ConceptNode>, surrealdb::Error> {
        let rel_type_str = relationship_type.to_string();
        let sql = "SELECT VALUE out FROM semantic_relations WHERE in = $concept_id AND relationship_type = $rel_type";

        let result = self
            .query(sql)
            .bind("concept_id", concept_id.clone())
            .bind("rel_type", rel_type_str)
            .execute::<Thing>()
            .await?;

        let mut concepts = Vec::new();
        for thing_id in result {
            if let Some(concept) = self.get_node(&thing_id).await? {
                concepts.push(concept);
            }
        }

        Ok(concepts)
    }

    pub async fn create_semantic_relationship(
        &self,
        from_concept: &Thing,
        to_concept: &Thing,
        relationship_type: RelationshipType,
        strength: f64,
        confidence: f64,
        context: Option<String>,
    ) -> Result<Option<SemanticEdge>, surrealdb::Error> {
        let edge = SemanticEdge {
            id: crate::graphs::models::default_thing(),
            relationship_type,
            strength,
            confidence,
            context,
            metadata: HashMap::new(),
        };

        self.add_edge(from_concept, to_concept, edge).await
    }

    pub async fn find_semantic_path(
        &self,
        from: &Thing,
        to: &Thing,
    ) -> Result<Vec<surrealdb::sql::Value>, surrealdb::Error> {
        self.route_path(from, to).await
    }

    pub async fn get_concept_hierarchy(
        &self,
        concept_id: &Thing,
    ) -> Result<Vec<ConceptNode>, surrealdb::Error> {
        let isa_concepts = self
            .get_related_concepts(concept_id, &RelationshipType::IsA)
            .await?;
        let partof_concepts = self
            .get_related_concepts(concept_id, &RelationshipType::PartOf)
            .await?;

        let mut hierarchy = isa_concepts;
        hierarchy.extend(partof_concepts);
        Ok(hierarchy)
    }

    pub async fn update_concept_confidence(
        &self,
        concept_id: &Thing,
        new_confidence: f64,
    ) -> Result<(), surrealdb::Error> {
        let sql = format!("UPDATE {concept_id} SET confidence = $confidence");

        let _ = self
            .query(&sql)
            .bind("confidence", new_confidence)
            .execute::<ConceptNode>()
            .await?;

        Ok(())
    }
}

fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
}
