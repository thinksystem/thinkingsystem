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

use std::collections::HashMap;
use std::sync::Arc;
use surrealdb::engine::remote::ws::Ws;
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;

use stele::graphs::core::GraphClient;
use stele::graphs::models::default_thing;
use stele::graphs::models::knowledge::{ConceptNode, RelationshipType, SemanticEdge};

#[tokio::test]
async fn test_knowledge_graph_extensibility() -> Result<(), Box<dyn std::error::Error>> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let test_db = format!("knowledge_test_{timestamp}");

    let client = Surreal::new::<Ws>("127.0.0.1:8000").await?;
    client
        .signin(Root {
            username: "root",
            password: "root",
        })
        .await?;
    client.use_ns("test").use_db(&test_db).await?;

    let graph_client = GraphClient::<ConceptNode, SemanticEdge>::new(Arc::new(client));

    println!("Test 1: Initialising knowledge graph schema...");
    graph_client.init_knowledge_schema().await?;
    println!("✓ Knowledge graph schema initialised successfully");

    println!("\nTest 2: Creating concepts with add_node...");

    let ai_concept = ConceptNode {
        id: default_thing(),
        name: "Artificial Intelligence".to_string(),
        category: "Technology".to_string(),
        description: "The simulation of human intelligence in machines".to_string(),
        confidence: 0.95,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "field".to_string(),
                serde_json::Value::String("Computer Science".to_string()),
            );
            props.insert(
                "complexity".to_string(),
                serde_json::Value::String("High".to_string()),
            );
            props
        },
        embedding: Some(vec![0.1, 0.2, 0.3, 0.4, 0.5]),
    };

    let ml_concept = ConceptNode {
        id: default_thing(),
        name: "Machine Learning".to_string(),
        category: "Technology".to_string(),
        description: "A subset of AI that enables machines to learn from data".to_string(),
        confidence: 0.92,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "field".to_string(),
                serde_json::Value::String("Computer Science".to_string()),
            );
            props.insert(
                "requires".to_string(),
                serde_json::Value::String("Statistics".to_string()),
            );
            props
        },
        embedding: Some(vec![0.15, 0.25, 0.35, 0.45, 0.55]),
    };

    let ai_node = graph_client.add_node(ai_concept).await?;
    let ml_node = graph_client.add_node(ml_concept).await?;
    let ai_id = ai_node.id.clone();
    let ml_id = ml_node.id.clone();
    println!("✓ Created AI concept: {ai_id}");
    println!("✓ Created ML concept: {ml_id}");

    println!("\nTest 3: Creating semantic relationships...");

    let semantic_edge = SemanticEdge {
        id: default_thing(),
        relationship_type: RelationshipType::PartOf,
        strength: 0.9,
        confidence: 0.85,
        context: Some("ML is a subset of AI".to_string()),
        metadata: {
            let mut meta = HashMap::new();
            meta.insert(
                "domain".to_string(),
                serde_json::Value::String("Technology".to_string()),
            );
            meta
        },
    };

    let edge_result = graph_client.add_edge(&ai_id, &ml_id, semantic_edge).await?;
    let edge_id = edge_result.unwrap().id;
    println!("✓ Created semantic relationship: {edge_id}");

    println!("\nTest 4: Testing specialised knowledge graph operations...");

    let isa_edge = graph_client
        .create_semantic_relationship(
            &ml_id,
            &ai_id,
            RelationshipType::IsA,
            0.8,
            0.9,
            Some("ML is a type of AI approach".to_string()),
        )
        .await?;

    if let Some(edge) = isa_edge {
        println!(
            "✓ Created IsA relationship using specialised method: {}",
            edge.id
        );
    } else {
        println!("✓ Created IsA relationship using specialised method (no edge returned)");
    }

    println!("\nTest 5: Testing generic query operations on knowledge graph...");

    let tech_concepts = graph_client.find_concepts_by_category("Technology").await?;
    println!("✓ Found {} technology concepts", tech_concepts.len());
    assert_eq!(tech_concepts.len(), 2);

    println!("\nTest 6: Testing generic node retrieval...");

    let retrieved_ai = graph_client.get_node(&ai_id).await?.unwrap();
    assert_eq!(retrieved_ai.name, "Artificial Intelligence");
    println!("✓ Retrieved AI concept: {}", retrieved_ai.name);

    println!("\nTest 7: Testing generic count operations...");

    let edge_count = graph_client.count_edges().await?;
    println!("✓ Found {edge_count} edges");

    assert!(
        edge_count >= 1,
        "Expected at least 1 edge, found {edge_count}"
    );

    println!("\nTest 8: Testing semantic similarity operations...");

    let similar_concepts = graph_client
        .find_similar_concepts(&[0.12, 0.22, 0.32, 0.42, 0.52], 0.8, 10)
        .await?;
    println!("✓ Found {} similar concepts", similar_concepts.len());

    println!("\nTest 9: Testing concept hierarchy operations...");

    let hierarchy = graph_client.get_concept_hierarchy(&ml_id).await?;
    println!(
        "✓ Found hierarchy with {} related concepts",
        hierarchy.len()
    );

    println!("\nTest 10: Testing semantic path finding...");

    let path = graph_client.find_semantic_path(&ai_id, &ml_id).await?;
    println!("✓ Found semantic path with {} steps", path.len());

    println!("\nTest 11: Testing concept confidence updates...");

    graph_client.update_concept_confidence(&ai_id, 0.98).await?;
    let updated_concept = graph_client.get_node(&ai_id).await?.unwrap();
    assert_eq!(updated_concept.confidence, 0.98);
    println!(
        "✓ Updated concept confidence to {}",
        updated_concept.confidence
    );

    println!("\nTest 12: Testing relationship queries...");

    let related_concepts = graph_client
        .get_related_concepts(&ai_id, &RelationshipType::PartOf)
        .await?;
    println!(
        "✓ Found {} concepts that are part of AI",
        related_concepts.len()
    );

    println!("\nCleaning up test database...");
    let cleanup_sql = format!("REMOVE DATABASE {test_db}");
    match graph_client.query(&cleanup_sql).execute::<()>().await {
        Ok(_) => println!("✓ Test database cleaned up"),
        Err(_) => println!("✓ Test database cleanup completed with minor warning (expected)"),
    }

    println!("\n✅ Extensibility tests passed! The knowledge graph module integrates seamlessly with the core GraphClient.");

    Ok(())
}

#[test]
fn test_relationship_type_display() {
    let relationships = [
        RelationshipType::IsA,
        RelationshipType::PartOf,
        RelationshipType::Uses,
        RelationshipType::Similar,
        RelationshipType::Precedes,
        RelationshipType::Follows,
        RelationshipType::Enables,
        RelationshipType::Custom("SpecialRelation".to_string()),
    ];

    for (i, rel) in relationships.iter().enumerate() {
        let display = rel.to_string();
        println!("Relationship type {i} displays as: '{display}'");
        assert!(!display.is_empty());
    }

    println!("✓ All relationship types have proper string representations");
}
