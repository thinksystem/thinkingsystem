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

use std::io::{self, Write};
use std::sync::Arc;
use stele::database::dynamic_access::DynamicDataAccessLayer;
use stele::database::dynamic_storage::DynamicStorage;
use stele::llm::dynamic_selector::DynamicModelSelector;
use stele::llm::unified_adapter::UnifiedLLMAdapter;
use stele::{
    database::{connection::DatabaseConnection, types::DatabaseCommand},
    nlu::{orchestrator::NLUOrchestrator, query_processor::QueryProcessor},
};
use tokio::sync::RwLock;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .init();

    info!("Starting Thinking System Core Interactive Demo");

    dotenvy::dotenv().ok();
    info!("Environment variables loaded");

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

    let database_client = client_rx
        .recv()
        .await
        .ok_or("Failed to receive database client from channel")?;
    info!("Database client received successfully.");

    let config_path = if std::path::Path::new("crates/stele/src/nlu/config").exists() {
        "crates/stele/src/nlu/config"
    } else if std::path::Path::new("../../../crates/stele/src/nlu/config").exists() {
        "../../../crates/stele/src/nlu/config"
    } else {
        return Err(
            "NLU config directory not found. Please run from workspace root or demo directory."
                .into(),
        );
    };

    let query_processor_config_path = if std::path::Path::new(
        "crates/stele/src/nlu/config/query_processor.toml",
    )
    .exists()
    {
        "crates/stele/src/nlu/config/query_processor.toml"
    } else if std::path::Path::new("../../../crates/stele/src/nlu/config/query_processor.toml")
        .exists()
    {
        "../../../crates/stele/src/nlu/config/query_processor.toml"
    } else {
        return Err("Query processor config file not found. Please run from workspace root or demo directory.".into());
    };

    let llm_models_config_path = if std::path::Path::new(
        "crates/stele/src/nlu/config/llm_models.yml",
    )
    .exists()
    {
        "crates/stele/src/nlu/config/llm_models.yml"
    } else if std::path::Path::new("../../../crates/stele/src/nlu/config/llm_models.yml").exists() {
        "../../../crates/stele/src/nlu/config/llm_models.yml"
    } else {
        return Err(
            "LLM models config file not found. Please run from workspace root or demo directory."
                .into(),
        );
    };

    let model_selector = Arc::new(
        DynamicModelSelector::from_config_path(llm_models_config_path)
            .map_err(|e| format!("Failed to create model selector: {e}"))?,
    );

    let llm_adapter = Arc::new(UnifiedLLMAdapter::new(model_selector).await?);
    info!("Unified LLM Adapter initialised with dynamic model selection (local-first, fallback enabled)");

    let orchestrator = Arc::new(RwLock::new(
        NLUOrchestrator::with_unified_adapter(config_path, llm_adapter.clone())
            .await
            .expect("Failed to create NLU orchestrator with unified adapter"),
    ));
    info!("NLU Orchestrator initialised with shared unified adapter.");

    let storage = Arc::new(DynamicStorage::new(database_client.clone()));
    info!("Dynamic Storage initialised.");

    let access_layer =
        Arc::new(DynamicDataAccessLayer::new(database_client.clone(), llm_adapter.clone()).await?);
    info!("Dynamic Data Access Layer initialised with shared LLM adapter.");

    let query_processor = QueryProcessor::new(
        orchestrator.clone(),
        storage.clone(),
        query_processor_config_path,
    )
    .await
    .expect("Failed to create QueryProcessor");
    info!("Query Processor initialised.");

    println!("\nThinking System Core Interactive Demo");
    println!("═══════════════════════════════════════════════════════════════");
    println!("This demo supports two modes:");
    println!();
    println!("STATEMENT MODE: Enter statements to analyse and store");
    println!("   Example: \"Dr. Smith leads the Quantum Research project\"");
    println!();
    println!("SEARCH MODE: Enter queries to search the knowledge base");
    println!("   Examples: \"find all theories\"");
    println!("            \"search for projects\"");
    println!("            \"show me entities\"");
    println!("            \"what theories exist\"");
    println!();
    println!("Tips:");
    println!("   - Search queries typically start with: find, search, show, what, where, list");
    println!("   - Statements are analysed for entities, relationships, and stored");
    println!("   - Type 'exit' to quit");
    println!("═══════════════════════════════════════════════════════════════");

    loop {
        print!("\nEnter your input: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        if input.eq_ignore_ascii_case("exit") {
            println!("Goodbye!");
            break;
        }

        println!("{}", "─".repeat(80));

        if is_search_query(input) {
            println!("SEARCH MODE: Processing natural language query...");
            handle_search_query(input, &access_layer).await;
        } else {
            println!("STATEMENT MODE: Analysing and storing statement...");
            handle_statement(input, &query_processor).await;
        }

        println!("{}", "─".repeat(80));
    }

    Ok(())
}

fn is_search_query(input: &str) -> bool {
    let input_lower = input.to_lowercase();
    let search_keywords = [
        "find", "search", "show", "list", "get", "retrieve", "what", "where", "which", "who",
        "how many", "display", "fetch", "query", "look for",
    ];

    search_keywords.iter().any(|keyword| {
        input_lower.starts_with(keyword) || input_lower.starts_with(&format!("{keyword} "))
    })
}

async fn handle_search_query(query: &str, access_layer: &Arc<DynamicDataAccessLayer>) {
    info!("Processing search query: '{}'", query);

    match access_layer.query_natural_language(query).await {
        Ok(nodes) => {
            if nodes.is_empty() {
                println!("No results found for: '{query}'");
                println!("Try different keywords or check if data exists in the database");
            } else {
                println!("Found {} result(s) for: '{}'", nodes.len(), query);
                println!();

                for (i, node) in nodes.iter().enumerate() {
                    println!("Result {} of {}:", i + 1, nodes.len());
                    match node {
                        stele::nlu::orchestrator::data_models::KnowledgeNode::Entity(entity) => {
                            println!("   Type: {} Entity", entity.entity_type);
                            println!("   Name: {}", entity.name);
                            println!("   Confidence: {:.2}", entity.confidence);
                            if let Some(metadata) = &entity.metadata {
                                println!("   Metadata: {metadata}");
                            }
                        }
                        stele::nlu::orchestrator::data_models::KnowledgeNode::Temporal(
                            temporal,
                        ) => {
                            println!("   Type: Temporal");
                            println!("   Text: {}", temporal.date_text);
                            if let Some(resolved) = &temporal.resolved_date {
                                println!("   Resolved: {resolved}");
                            }
                            println!("   Confidence: {:.2}", temporal.confidence);
                        }
                        stele::nlu::orchestrator::data_models::KnowledgeNode::Numerical(
                            numerical,
                        ) => {
                            println!("   Type: Numerical");
                            println!("   Value: {} {}", numerical.value, numerical.unit);
                            println!("   Confidence: {:.2}", numerical.confidence);
                        }
                        stele::nlu::orchestrator::data_models::KnowledgeNode::Action(action) => {
                            println!("   Type: Action");
                            println!("   Verb: {}", action.verb);
                            println!("   Confidence: {:.2}", action.confidence);
                        }
                    }
                    println!("   ID: {}", node.temp_id());
                    println!();
                }
            }
        }
        Err(e) => {
            error!("Search query failed: {}", e);
            println!("Search failed: {e}");
            println!("This might be due to:");
            println!("   - Unsupported query format");
            println!("   - Database connection issues");
            println!("   - No matching data in the database");
        }
    }
}

async fn handle_statement(statement: &str, query_processor: &QueryProcessor) {
    info!("Processing statement: '{}'", statement);

    match query_processor
        .process_and_store_input(statement, "demo_user", "interactive_cli")
        .await
    {
        Ok(response) => {
            println!("Statement processed and stored successfully!");

            if let Some(results) = response.get("results").and_then(|r| r.as_array()) {
                println!("Storage Operations: {} completed", results.len());

                let mut nodes_created = 0;
                let mut relationships_created = 0;

                for result in results {
                    if let Some(table) = result.get("table").and_then(|t| t.as_str()) {
                        match table {
                            "nodes" => nodes_created += 1,
                            "edges" => relationships_created += 1,
                            _ => {}
                        }
                    }
                }

                if nodes_created > 0 {
                    println!("Created {nodes_created} knowledge node(s)");
                }
                if relationships_created > 0 {
                    println!("Created {relationships_created} relationship(s)");
                }
            }

            if let Some(utterance_id) = response.get("utterance_id").and_then(|id| id.as_str()) {
                println!("Utterance ID: {utterance_id}");
            }

            println!("You can now search for this data using queries like:");
            println!("   'find all entities', 'search for theories', etc.");
        }
        Err(e) => {
            error!("Statement processing failed: {}", e);
            println!("Failed to process statement: {e}");
            println!("This might be due to:");
            println!("   - Complex sentence structure");
            println!("   - LLM processing errors");
            println!("   - Database storage issues");
        }
    }
}
