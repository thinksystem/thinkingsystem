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


use std::str::FromStr;
use std::sync::Arc;
use stele::database::dynamic_storage::DynamicStorage;
use stele::database::surreal_token::SurrealTokenParser;
use stele::database::types::DatabaseCommand;
use stele::database::{
    connection::DatabaseConnection, dynamic_access::DynamicDataAccessLayer,
    query_kg::QueryKgBuilder,
};
use stele::llm::dynamic_selector::DynamicModelSelector;
use stele::llm::unified_adapter::UnifiedLLMAdapter;
use stele::nlu::{orchestrator::NLUOrchestrator, query_processor::QueryProcessor};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, Duration};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .init();
    dotenvy::dotenv().ok();

    let (command_tx, command_rx) = mpsc::channel(32);
    let (client_tx, mut client_rx) = mpsc::channel(1);
    let mut db_conn = DatabaseConnection::new(command_rx);
    tokio::spawn(async move {
        if let Err(e) = db_conn.run().await {
            eprintln!("Database connection handler error: {e}");
        }
    });
    let (connect_response_tx, connect_response_rx) = oneshot::channel();
    command_tx
        .send(DatabaseCommand::Connect {
            client_sender: client_tx,
            response_sender: connect_response_tx,
        })
        .await?;
    connect_response_rx.await??;
    let db = client_rx
        .recv()
        .await
        .ok_or("Failed to receive database client")?;

    seed_minimal_data(db.clone()).await?;

    let docs_path = if std::path::Path::new("crates/stele/src/database/instructions").exists() {
        "crates/stele/src/database/instructions"
    } else {
        "config/instructions"
    };
    info!("Building KG from {}", docs_path);
    let builder = QueryKgBuilder::new(docs_path);
    let kg = match builder.build_and_save_analysis("parsing_analysis.json") {
        Ok(kg) => kg,
        Err(e) => {
            error!("KG build failed: {e}");

            builder.build()?
        }
    };
    println!(
        "\n=== KG Summary ===\n{}\n",
        kg.parsing_registry.get_summary()
    );

    let mut parser = SurrealTokenParser::with_kg_hints(&kg);
    let demo_idioms = [
        "nodes->edge->nodes",
        "nodes.edges[0]",
        "projects->assigned->users",
    ];
    for idiom_str in demo_idioms {
        let token = parser.parse_idiom(idiom_str);
        let select = SurrealTokenParser::convert_idiom_to_select_query(&token);
        println!("Idiom: {idiom_str} -> Select: {select}");
    }

    let models_path = if std::path::Path::new("crates/stele/src/nlu/config/llm_models.yml").exists()
    {
        "crates/stele/src/nlu/config/llm_models.yml"
    } else {
        "./crates/stele/src/nlu/config/llm_models.yml"
    };
    let selector = Arc::new(DynamicModelSelector::from_config_path(models_path)?);
    let llm = Arc::new(UnifiedLLMAdapter::new(selector).await?);

    let config_dir = if std::path::Path::new("crates/stele/src/nlu/config").exists() {
        "crates/stele/src/nlu/config"
    } else {
        "./crates/stele/src/nlu/config"
    };
    let query_processor_toml =
        if std::path::Path::new("crates/stele/src/nlu/config/query_processor.toml").exists() {
            "crates/stele/src/nlu/config/query_processor.toml"
        } else {
            "./crates/stele/src/nlu/config/query_processor.toml"
        };

    let orchestrator = Arc::new(tokio::sync::RwLock::new(
        NLUOrchestrator::with_unified_adapter(config_dir, llm.clone())
            .await
            .expect("Failed to init NLU orchestrator"),
    ));
    let storage = Arc::new(DynamicStorage::with_regulariser(db.clone(), true));
    let query_processor =
        QueryProcessor::new(orchestrator.clone(), storage.clone(), query_processor_toml)
            .await
            .expect("Failed to init QueryProcessor");

    println!("\nIngesting demo statement via NLU → storage → regulariser...");
    let statement = "Dr. Alice leads the Quantum Research project";
    let mut current_utterance: Option<String> = None;
    match query_processor
        .process_and_store_input(statement, "kg_demo_user", "kg_idioms_demo")
        .await
    {
        Ok(resp) => {
            println!(
                "Stored statement. Utterance ID: {}",
                resp.get("utterance_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<unknown>")
            );
            current_utterance = resp
                .get("utterance_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
        Err(e) => {
            eprintln!("Failed to ingest statement: {e}");
        }
    }

    let meeting = "let's meet with Bob next Tuesday in the Boulevard 3pm";
    match query_processor
        .process_and_store_input(meeting, "kg_demo_user", "kg_idioms_demo")
        .await
    {
        Ok(resp) => {
            println!(
                "Stored meeting. Utterance ID: {}",
                resp.get("utterance_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<unknown>")
            );
        }
        Err(e) => {
            eprintln!("Failed to ingest meeting: {e}");
        }
    }

    sleep(Duration::from_millis(600)).await;

    summarise_canonical(&db).await?;

    list_recent_provenance(&db, current_utterance.as_deref()).await?;

    debug_probe_canonical(&db).await?;

    inspect_events_and_attends(&db).await?;

    run_datetime_stress_tests(&db).await?;

    let access = Arc::new(DynamicDataAccessLayer::new(db.clone(), llm.clone()).await?);

    let examples = vec![
        "list all entities",
        "nodes->edge->nodes",
        "show top 5 entities order by name",
    ];
    println!("Running {} example queries...", examples.len());
    for q in examples {
        println!("\n— Query: {q}");
        match access.query_natural_language(q).await {
            Ok(nodes) => {
                println!("Results: {}", nodes.len());
                for n in nodes.iter().take(3) {
                    println!("  • {}", n.temp_id());
                }
                if nodes.len() > 3 {
                    println!("  … and {} more", nodes.len() - 3);
                }
            }
            Err(e) => {
                println!("Error: {e}");
            }
        }
    }

    Ok(())
}

async fn seed_minimal_data(
    db: Arc<surrealdb::Surreal<surrealdb::engine::remote::ws::Client>>,
) -> Result<(), Box<dyn std::error::Error>> {
    db.query("CREATE nodes:alice SET type = 'Entity', properties = { name: 'Alice', entity_type: 'person' }")
        .await?;
    db.query(
        "CREATE nodes:bob SET type = 'Entity', properties = { name: 'Bob', entity_type: 'person' }",
    )
    .await?;
    db.query("CREATE nodes:quantum SET type = 'Entity', properties = { name: 'Quantum Research', entity_type: 'project' }")
        .await?;

    db.query("RELATE nodes:alice->edges->nodes:quantum SET label = 'LEADS'")
        .await?;

    let mut res = db.query("SELECT count() AS c FROM nodes GROUP ALL").await?;
    let counts: Vec<serde_json::Value> = res.take(0).unwrap_or_default();
    if let Some(c) = counts
        .first()
        .and_then(|v| v.get("c"))
        .and_then(|v| v.as_u64())
    {
        println!("Seed complete. Nodes in DB: {c}");
    }
    Ok(())
}

async fn summarise_canonical(
    db: &Arc<surrealdb::Surreal<surrealdb::engine::remote::ws::Client>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut c = 0u64;
    let mut list: Vec<serde_json::Value> = Vec::new();
    for _ in 0..10 {
        let mut res = db
            .query("SELECT count() AS c FROM canonical_entity GROUP ALL; SELECT name, entity_type FROM canonical_entity LIMIT 5;")
            .await?;
        let counts: Vec<serde_json::Value> = res.take(0).unwrap_or_default();
        list = res.take(1).unwrap_or_default();
        c = counts
            .first()
            .and_then(|v| v.get("c"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if c > 0 {
            break;
        }
        sleep(Duration::from_millis(150)).await;
    }
    println!("Canonical entities: {c}");
    for item in list.iter() {
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        let ty = item
            .get("entity_type")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        println!("  • {name} ({ty})");
    }

    let mut t = db
        .query("SELECT ->edges->nodes as neighbours FROM nodes:alice")
        .await?;
    let neigh: Vec<serde_json::Value> = t.take(0).unwrap_or_default();
    if let Some(first) = neigh.first() {
        println!("Traversal from nodes:alice ->edges->nodes:");

        if let Some(arr) = first.get("neighbours").and_then(|v| v.as_array()) {
            for v in arr.iter().take(5) {
                if let Some(id) = v.get("id").and_then(|x| x.as_str()) {
                    println!("  • {id}");
                }
            }
        }
    }
    Ok(())
}

async fn debug_probe_canonical(
    db: &Arc<surrealdb::Surreal<surrealdb::engine::remote::ws::Client>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let key = format!("probe:{nanos}");

    let mut check = db
        .query("SELECT id FROM canonical_entity WHERE canonical_key = $k LIMIT 1;")
        .bind(("k", key.clone()))
        .await?;
    let existing: Vec<serde_json::Value> = check.take(0).unwrap_or_default();
    if existing.is_empty() {
        let _ = db
            .query(
                "CREATE canonical_entity SET entity_type = 'probe', name = 'Probe Record', canonical_key = $k, extra = {}",
            )
            .bind(("k", key.clone()))
            .await;
    }

    let mut v = db
        .query(
            "SELECT id, name, entity_type FROM canonical_entity WHERE canonical_key = $k LIMIT 1;",
        )
        .bind(("k", key))
        .await?;
    let rows: Vec<serde_json::Value> = v.take(0).unwrap_or_default();
    if let Some(r) = rows.first() {
        let id = r.get("id").and_then(|x| x.as_str()).unwrap_or("<none>");
        let name = r.get("name").and_then(|x| x.as_str()).unwrap_or("<none>");
        let t = r
            .get("entity_type")
            .and_then(|x| x.as_str())
            .unwrap_or("<none>");
        println!("Probe verified row: {id} {name} ({t})");
    } else {
        println!("Probe verification SELECT returned no rows");
    }
    Ok(())
}

async fn list_recent_provenance(
    db: &Arc<surrealdb::Surreal<surrealdb::engine::remote::ws::Client>>,
    utterance: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(utt) = utterance {
        let mut res = db
            .query("SELECT out.kind as kind, out.created_at as created_at, in as utterance FROM utterance_has_provenance WHERE in = $utt ORDER BY out.created_at DESC LIMIT 5;")
            .bind(("utt", surrealdb::RecordId::from_str(utt).map_err(|_| "Invalid utterance id")?))
            .await?;
        let rows: Vec<serde_json::Value> = res.take(0).unwrap_or_default();
        println!("Recent provenance events for {utt} (up to 5):");
        if rows.is_empty() {
            println!("  • <none>");
        } else {
            for r in rows {
                let kind = r
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<unknown>");
                let ts = r
                    .get("created_at")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "<no ts>".to_string());
                let utt_print = r.get("utterance").and_then(|v| v.as_str()).unwrap_or(utt);
                println!("  • {kind} @ {ts} (utterance: {utt_print})");
            }
        }
        return Ok(());
    }

    let mut res = db
        .query("SELECT kind, created_at FROM provenance_event ORDER BY created_at DESC LIMIT 5;")
        .await?;
    let rows: Vec<serde_json::Value> = res.take(0).unwrap_or_default();
    println!("Recent provenance events (up to 5):");
    if rows.is_empty() {
        println!("  • <none>");
    } else {
        for r in rows {
            let kind = r
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");
            let ts = r
                .get("created_at")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "<no ts>".to_string());
            println!("  • {kind} @ {ts}");
        }
    }
    Ok(())
}

async fn inspect_events_and_attends(
    db: &Arc<surrealdb::Surreal<surrealdb::engine::remote::ws::Client>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut res = db
        .query(
            "SELECT id, title, start_at, location, created_at FROM canonical_event ORDER BY created_at DESC LIMIT 3;",
        )
        .await?;
    let events: Vec<serde_json::Value> = res.take(0).unwrap_or_default();
    println!("Recent canonical events (up to 3):");
    if events.is_empty() {
        println!("  • <none>");
    } else {
        for e in events {
            let id = e.get("id").and_then(|v| v.as_str()).unwrap_or("<none>");
            let title = e
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("<untitled>");
            let start_at = e
                .get("start_at")
                .map(|v| v.to_string())
                .unwrap_or("<none>".into());
            let location = e.get("location").and_then(|v| v.as_str()).unwrap_or("");
            println!("  • {id} — {title} @ {start_at} {location}");
        }
    }

    let mut facts = db
        .query(
            "SELECT id, subject_ref, object_ref, predicate, created_at FROM canonical_relationship_fact WHERE predicate = 'ATTENDS' ORDER BY created_at DESC LIMIT 5;",
        )
        .await?;
    let rows: Vec<serde_json::Value> = facts.take(0).unwrap_or_default();
    println!("Recent ATTENDS facts (up to 5):");
    if rows.is_empty() {
        println!("  • <none>");
    } else {
        for r in rows {
            let id = r.get("id").and_then(|v| v.as_str()).unwrap_or("<none>");
            let s = r
                .get("subject_ref")
                .and_then(|v| v.as_str())
                .unwrap_or("<s>");
            let o = r
                .get("object_ref")
                .and_then(|v| v.as_str())
                .unwrap_or("<o>");
            let t = r.get("predicate").and_then(|v| v.as_str()).unwrap_or("");
            println!("  • {id}: {s} -[{t}]-> {o}");
        }
    }
    Ok(())
}

async fn run_datetime_stress_tests(
    db: &Arc<surrealdb::Surreal<surrealdb::engine::remote::ws::Client>>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\nRunning datetime stress tests...");
    let samples = vec![
        ("Valid RFC3339 Z", "2025-08-12T15:00:00Z"),
        ("Valid RFC3339 +02:00", "2025-08-12T15:00:00+02:00"),
        ("Valid Date only", "2025-08-12"),
        ("Invalid missing TZ", "2025-08-12T15:00:00"),
        ("Invalid garbage", "not-a-date"),
    ];
    for (label, ts) in samples {
        let mut ok_cast = true;
        if let Err(e) = db
            .query("CREATE temp_event SET t = <datetime>$ts RETURN AFTER")
            .bind(("ts", ts))
            .await
        {
            ok_cast = false;
            println!(
                "  • {label}: cast failed as expected? {} — {e}",
                !label.starts_with("Valid")
            );
        }

        let title = format!("Stress {label}");
        match db
            .query("CREATE canonical_event SET title = $t, start_at = <datetime>$ts RETURN AFTER")
            .bind(("t", title.clone()))
            .bind(("ts", ts))
            .await
        {
            Err(e) => {
                println!("  • {label}: canonical_event create failed — {e}");
            }
            Ok(_) if ok_cast => {
                println!("  • {label}: OK");
            }
            Ok(_) => {}
        }
    }

    let _ = db.query("REMOVE TABLE temp_event").await;
    Ok(())
}
