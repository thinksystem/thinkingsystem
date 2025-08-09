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

use chrono::Utc;
use std::sync::Arc;
use surrealdb::engine::remote::ws::{Client, Ws};
use surrealdb::opt::auth::Root;
use surrealdb::sql::{Datetime, Thing};
use surrealdb::Surreal;

use stele::graphs::core::{GraphClient, Node};
use stele::graphs::models::{
    default_thing,
    event::{EventEdge, EventNode},
    personalisation::{PersonalisationEdge, PersonalisationNode},
    scribe::{ScribeEdge, ScribeNode},
};

async fn setup_test_db() -> Result<Arc<Surreal<Client>>, Box<dyn std::error::Error>> {
    let db = Surreal::new::<Ws>("127.0.0.1:8000").await?;

    db.signin(Root {
        username: "root",
        password: "root",
    })
    .await?;

    let db_name = format!(
        "graph_tests_{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    db.use_ns("test").use_db(&db_name).await?;

    Ok(Arc::new(db))
}

fn create_test_event(name: &str, start_offset_hours: i64, duration_hours: i64) -> EventNode {
    let now = Utc::now();
    let start_time = now + chrono::Duration::hours(start_offset_hours);
    let end_time = start_time + chrono::Duration::hours(duration_hours);

    EventNode {
        id: default_thing(),
        name: name.to_string(),
        start_time: Datetime::from(start_time),
        end_time: Datetime::from(end_time),
    }
}

fn create_test_personalisation_node(node_type: &str, values: Vec<String>) -> PersonalisationNode {
    PersonalisationNode {
        id: default_thing(),
        node_type: node_type.to_string(),
        values,
        embeddings: vec![0.1; 384],
        timestamp: Datetime::from(Utc::now()),
    }
}

fn create_test_scribe(name: &str, x: f32, y: f32) -> ScribeNode {
    let mut specialisation = std::collections::HashMap::new();
    specialisation.insert("rust".to_string(), 0.8);
    specialisation.insert("databases".to_string(), 0.6);

    ScribeNode {
        id: default_thing(),
        name: name.to_string(),
        coords: [x, y],
        specialisation,
    }
}

#[tokio::test]
async fn test_graph_client_basic_operations() -> Result<(), Box<dyn std::error::Error>> {
    let db = setup_test_db().await?;
    let client = GraphClient::<EventNode, EventEdge>::new(db.clone());

    client.init_schema().await?;

    let event1 = create_test_event("Meeting", 0, 2);
    let event2 = create_test_event("Lunch", 3, 1);

    let added_event1 = client.add_node(event1.clone()).await?;
    let added_event2 = client.add_node(event2.clone()).await?;

    assert_eq!(added_event1.name, "Meeting");
    assert_eq!(added_event2.name, "Lunch");

    let edge = EventEdge {
        id: default_thing(),
        relationship: "before".to_string(),
        weight: 1.0,
    };

    let added_edge = client
        .add_edge(added_event1.get_id(), added_event2.get_id(), edge)
        .await?;

    assert!(added_edge.is_some());
    assert_eq!(added_edge.unwrap().relationship, "before");

    let retrieved_event = client.get_node(added_event1.get_id()).await?;
    assert!(retrieved_event.is_some());
    assert_eq!(retrieved_event.unwrap().name, "Meeting");

    let neighbours = client.get_neighbours(added_event1.get_id()).await?;
    assert_eq!(neighbours.len(), 1);
    assert_eq!(neighbours[0].name, "Lunch");
    Ok(())
}

#[tokio::test]
async fn test_event_graph_specialised_operations() -> Result<(), Box<dyn std::error::Error>> {
    let db = setup_test_db().await?;
    let client = GraphClient::<EventNode, EventEdge>::new(db.clone());

    client.init_schema().await?;

    let now = Utc::now();
    let event1 = EventNode {
        id: default_thing(),
        name: "Conference".to_string(),
        start_time: Datetime::from(now),
        end_time: Datetime::from(now + chrono::Duration::hours(4)),
    };

    let event2 = EventNode {
        id: default_thing(),
        name: "Workshop".to_string(),
        start_time: Datetime::from(now + chrono::Duration::hours(2)),
        end_time: Datetime::from(now + chrono::Duration::hours(6)),
    };

    let event3 = EventNode {
        id: default_thing(),
        name: "Dinner".to_string(),
        start_time: Datetime::from(now + chrono::Duration::hours(8)),
        end_time: Datetime::from(now + chrono::Duration::hours(10)),
    };

    let added_event1 = client.add_node(event1).await?;
    let _added_event2 = client.add_node(event2).await?;
    let _added_event3 = client.add_node(event3).await?;

    let search_start = Datetime::from(now - chrono::Duration::hours(1));
    let search_end = Datetime::from(now + chrono::Duration::hours(5));

    let events_in_range = client
        .find_events_in_range(search_start, search_end)
        .await?;

    assert_eq!(events_in_range.len(), 2);

    let overlapping = client
        .find_overlapping_events(added_event1.get_id())
        .await?;

    assert_eq!(overlapping.len(), 1);
    assert_eq!(overlapping[0].name, "Workshop");

    Ok(())
}

#[tokio::test]
async fn test_personalisation_graph_operations() -> Result<(), Box<dyn std::error::Error>> {
    let db = setup_test_db().await?;
    let client = GraphClient::<PersonalisationNode, PersonalisationEdge>::new(db.clone());

    client.init_schema().await?;

    let node1 = create_test_personalisation_node(
        "preference",
        vec!["rust".to_string(), "databases".to_string()],
    );
    let node2 = create_test_personalisation_node(
        "skill",
        vec!["programming".to_string(), "system_design".to_string()],
    );

    let added_node1 = client.add_node(node1).await?;
    let added_node2 = client.add_node(node2).await?;

    let edge = PersonalisationEdge {
        id: default_thing(),
        weight: 0.8,
        relation: "enhances".to_string(),
    };

    let _added_edge = client
        .add_edge(added_node1.get_id(), added_node2.get_id(), edge)
        .await?;

    let query_embedding = vec![0.1, 0.2, 0.3, 0.4, 0.5];
    let similar_nodes = client.find_similar_nodes(query_embedding, 5).await?;

    assert!(similar_nodes.len() >= 2);

    let mut feedback = std::collections::HashMap::new();
    feedback.insert(added_node1.get_id().id.to_string(), 1.2);
    feedback.insert(added_node2.get_id().id.to_string(), 0.8);

    client.optimise(feedback).await?;

    Ok(())
}

#[tokio::test]
async fn test_scribe_graph_operations() -> Result<(), Box<dyn std::error::Error>> {
    let db = setup_test_db().await?;
    let client = GraphClient::<ScribeNode, ScribeEdge>::new(db.clone());

    client.init_schema().await?;

    let scribe1 = create_test_scribe("Alice", 0.0, 0.0);
    let scribe2 = create_test_scribe("Bob", 3.0, 4.0);
    let scribe3 = create_test_scribe("Charlie", 10.0, 10.0);

    let added_scribe1 = client.add_node(scribe1).await?;
    let added_scribe2 = client.add_node(scribe2).await?;
    let _added_scribe3 = client.add_node(scribe3).await?;

    let route_edge = ScribeEdge {
        id: default_thing(),
        weight: 5.0,
    };

    let _route = client
        .add_edge(added_scribe1.get_id(), added_scribe2.get_id(), route_edge)
        .await?;

    let updated_scribe = client
        .set_scribe_coords(added_scribe1.get_id(), [1.0, 1.0])
        .await?;

    assert!(updated_scribe.is_some());
    assert_eq!(updated_scribe.unwrap().coords, [1.0, 1.0]);

    let centre = [0.0, 0.0];
    let scribes_nearby = client.find_scribes_within_radius(centre, 6.0).await?;

    assert!(scribes_nearby.len() >= 2);

    let nearest = client.find_nearest_scribe([0.5, 0.5]).await?;
    assert!(nearest.is_some());

    let updated_specialisation = client
        .update_specialisation(added_scribe1.get_id(), "python".to_string(), 0.9)
        .await?;

    assert!(updated_specialisation.is_some());

    Ok(())
}

#[tokio::test]
async fn test_graph_routing_and_complex_queries() -> Result<(), Box<dyn std::error::Error>> {
    let db = setup_test_db().await?;
    let client = GraphClient::<ScribeNode, ScribeEdge>::new(db.clone());

    client.init_schema().await?;

    let scribe_a = create_test_scribe("ScribeA", 0.0, 0.0);
    let scribe_b = create_test_scribe("ScribeB", 1.0, 0.0);
    let scribe_c = create_test_scribe("ScribeC", 2.0, 0.0);

    let added_a = client.add_node(scribe_a).await?;
    let added_b = client.add_node(scribe_b).await?;
    let added_c = client.add_node(scribe_c).await?;

    let edge_ab = ScribeEdge {
        id: default_thing(),
        weight: 1.0,
    };
    let edge_bc = ScribeEdge {
        id: default_thing(),
        weight: 1.0,
    };

    client
        .add_edge(added_a.get_id(), added_b.get_id(), edge_ab)
        .await?;
    client
        .add_edge(added_b.get_id(), added_c.get_id(), edge_bc)
        .await?;

    let path = client
        .route_path(added_a.get_id(), added_c.get_id())
        .await?;
    assert!(!path.is_empty());

    let custom_results: Vec<serde_json::Value> = client
        .query("SELECT name, coords FROM scribes WHERE coords[0] <= $max_x")
        .bind("max_x", 1.5)
        .execute()
        .await?;

    assert!(custom_results.len() >= 2);

    Ok(())
}

#[tokio::test]
async fn test_concurrent_graph_operations() -> Result<(), Box<dyn std::error::Error>> {
    let db = setup_test_db().await?;
    let client = Arc::new(GraphClient::<EventNode, EventEdge>::new(db.clone()));

    client.init_schema().await?;

    let mut handles = vec![];

    for i in 0..10 {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move {
            let event = create_test_event(&format!("Event{i}"), i, 1);
            client_clone.add_node(event).await
        });
        handles.push(handle);
    }

    let mut results = vec![];
    for handle in handles {
        let result = handle.await??;
        results.push(result);
    }

    assert_eq!(results.len(), 10);

    let mut names: Vec<String> = results.iter().map(|e| e.name.clone()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 10);

    Ok(())
}

#[tokio::test]
async fn test_graph_error_handling() -> Result<(), Box<dyn std::error::Error>> {
    let db = setup_test_db().await?;
    let client = GraphClient::<EventNode, EventEdge>::new(db.clone());

    client.init_schema().await?;

    let fake_id = Thing::from(("events", "nonexistent"));
    let result = client.get_node(&fake_id).await?;
    assert!(result.is_none());

    let neighbours = client.get_neighbours(&fake_id).await?;
    assert!(neighbours.is_empty());

    let invalid_query_result: Result<Vec<serde_json::Value>, _> = client
        .query("SELECT * FROM invalid_table_that_does_not_exist")
        .execute()
        .await;

    assert!(invalid_query_result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_schema_validation() -> Result<(), Box<dyn std::error::Error>> {
    let db = setup_test_db().await?;

    let event_client = GraphClient::<EventNode, EventEdge>::new(db.clone());
    event_client.init_schema().await?;

    let personalisation_client =
        GraphClient::<PersonalisationNode, PersonalisationEdge>::new(db.clone());
    personalisation_client.init_schema().await?;

    let scribe_client = GraphClient::<ScribeNode, ScribeEdge>::new(db.clone());
    scribe_client.init_schema().await?;

    let info_result: Vec<serde_json::Value> = event_client.query("INFO FOR DB").execute().await?;

    assert!(!info_result.is_empty());

    Ok(())
}

async fn cleanup_test_db(db: &Surreal<Client>) -> Result<(), Box<dyn std::error::Error>> {
    db.query("REMOVE TABLE events").await?;
    db.query("REMOVE TABLE event_links").await?;
    db.query("REMOVE TABLE personalisation_nodes").await?;
    db.query("REMOVE TABLE personalisation_edges").await?;
    db.query("REMOVE TABLE scribes").await?;
    db.query("REMOVE TABLE routes").await?;

    Ok(())
}

#[tokio::test]
async fn test_full_integration_workflow() -> Result<(), Box<dyn std::error::Error>> {
    let db = setup_test_db().await?;

    let event_client = GraphClient::<EventNode, EventEdge>::new(db.clone());
    let scribe_client = GraphClient::<ScribeNode, ScribeEdge>::new(db.clone());

    event_client.init_schema().await?;
    scribe_client.init_schema().await?;

    let workshop_event = create_test_event("Rust Workshop", 0, 3);
    let _added_event = event_client.add_node(workshop_event).await?;

    let rust_expert = create_test_scribe("RustExpert", 1.0, 1.0);
    let beginner = create_test_scribe("Beginner", 2.0, 2.0);

    let added_expert = scribe_client.add_node(rust_expert).await?;
    let added_beginner = scribe_client.add_node(beginner).await?;

    let collaboration_route = ScribeEdge {
        id: default_thing(),
        weight: 0.9,
    };

    scribe_client
        .add_edge(
            added_expert.get_id(),
            added_beginner.get_id(),
            collaboration_route,
        )
        .await?;

    let events = event_client
        .find_events_in_range(
            Datetime::from(Utc::now() - chrono::Duration::hours(1)),
            Datetime::from(Utc::now() + chrono::Duration::hours(4)),
        )
        .await?;

    let scribes = scribe_client
        .find_scribes_within_radius([1.5, 1.5], 2.0)
        .await?;

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, "Rust Workshop");
    assert_eq!(scribes.len(), 2);

    cleanup_test_db(&db).await?;

    Ok(())
}
