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
use stele::nlu::llm_processor::LLMAdapter;
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
            "NLU config directory not found. Please run from the workspace root or demo directory."
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
        return Err("Query processor config file not found. Please run from the workspace root or demo directory.".into());
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
            "LLM models config file not found. Please run from the workspace root or demo directory."
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
    println!("This demo uses AI to automatically detect whether your input is:");
    println!();
    println!("QUESTION MODE: Searches for and retrieves existing knowledge");
    println!("   Examples: \"What theories exist?\", \"Find all entities\"");
    println!("             \"Show me projects\", \"Who leads the quantum research?\"");
    println!();
    println!("STATEMENT MODE: Analyses and stores new information");
    println!("   Examples: \"Dr. Smith leads the Quantum Research project\"");
    println!("             \"The theory explains quantum mechanics\"");
    println!("             \"John works at Microsoft\"");
    println!();
    println!("Enhanced Features:");
    println!("   - AI-powered input classification using an LLM");
    println!("   - Natural-language responses to questions with full context");
    println!("   - Graph data and original utterances included in results");
    println!("   - Intelligent fallback to keyword detection if needed");
    println!();
    println!("Tips:");
    println!("   - Type naturally. The AI will determine the appropriate mode.");
    println!("   - Questions get comprehensive answers with structured data.");
    println!("   - Statements are analysed for entities and relationships, and then stored.");
    println!("   - Type 'exit' to quit.");
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

        match determine_input_type(input, &llm_adapter).await {
            Ok(InputType::Question) => {
                println!("QUESTION MODE: Processing question and retrieving relevant data...");
                handle_question(input, &access_layer, &storage, &llm_adapter).await;
            }
            Ok(InputType::Statement) => {
                println!("STATEMENT MODE: Analysing and storing statement...");
                handle_statement(input, &query_processor).await;
            }
            Err(e) => {
                error!("Failed to determine input type: {}", e);
                println!("Error processing input: {e}");
                println!("Falling back to simple keyword detection...");

                match simple_keyword_detection(input) {
                    InputType::Question => {
                        println!(
                            "QUESTION MODE: Processing question and retrieving relevant data..."
                        );
                        handle_question(input, &access_layer, &storage, &llm_adapter).await;
                    }
                    InputType::Statement => {
                        println!("STATEMENT MODE: Analysing and storing statement...");
                        handle_statement(input, &query_processor).await;
                    }
                }
            }
        }

        println!("{}", "─".repeat(80));
    }

    Ok(())
}

async fn determine_input_type(
    input: &str,
    llm_adapter: &Arc<UnifiedLLMAdapter>,
) -> Result<InputType, Box<dyn std::error::Error>> {
    let system_prompt = r#"Analyse the user's input to determine if it is a QUESTION or a STATEMENT. A QUESTION asks for or requests information. A STATEMENT provides or asserts information. Your response must be a single word: either 'QUESTION' or 'STATEMENT'."#;

    match llm_adapter
        .generate_response(&format!("{system_prompt}\n\nUser input: {input}"))
        .await
    {
        Ok(response) => {
            let response_text = response.trim().to_uppercase();
            if response_text.contains("QUESTION") {
                Ok(InputType::Question)
            } else if response_text.contains("STATEMENT") {
                Ok(InputType::Statement)
            } else {
                Ok(simple_keyword_detection(input))
            }
        }
        Err(_) => Ok(simple_keyword_detection(input)),
    }
}

#[derive(Debug, Clone)]
enum InputType {
    Question,
    Statement,
}

fn simple_keyword_detection(input: &str) -> InputType {
    let input_lower = input.to_lowercase();
    let question_keywords = [
        "find", "search", "show", "list", "get", "retrieve", "what", "where", "which", "who",
        "how many", "display", "fetch", "query", "look for", "tell me", "explain",
    ];

    if question_keywords.iter().any(|keyword| {
        input_lower.starts_with(keyword) || input_lower.starts_with(&format!("{keyword} "))
    }) {
        InputType::Question
    } else {
        InputType::Statement
    }
}

async fn handle_question(
    query: &str,
    access_layer: &Arc<DynamicDataAccessLayer>,
    storage: &Arc<DynamicStorage>,
    llm_adapter: &Arc<UnifiedLLMAdapter>,
) {
    info!("Processing question: '{}'", query);

    match access_layer.query_natural_language(query).await {
        Ok(nodes) => {
            if nodes.is_empty() {
                println!("No results found for: '{query}'");
                println!("Try different keywords or check if data exists in the database");
            } else {
                let node_ids: Vec<String> = nodes
                    .iter()
                    .map(|node| node.temp_id().to_string())
                    .collect();

                let utterances = match storage.get_utterances_for_nodes(&node_ids).await {
                    Ok(utterances) => utterances,
                    Err(_) => serde_json::json!([]),
                };

                let relevant_nodes = filter_relevant_nodes(&nodes, query);

                let response_prompt = create_focused_prompt(query, &relevant_nodes, &utterances);

                match llm_adapter.generate_response(&response_prompt).await {
                    Ok(llm_response) => {
                        println!("{llm_response}");
                    }
                    Err(e) => {
                        error!("Failed to generate LLM response: {}", e);
                        println!(
                            "Found {} result(s) but could not generate a natural response",
                            nodes.len()
                        );

                        for (i, node) in relevant_nodes.iter().take(5).enumerate() {
                            println!("  {}. {}", i + 1, format_node_simple(node));
                        }
                    }
                }
            }
        }
        Err(e) => {
            error!("Question processing failed: {}", e);
            println!("Sorry, I could not process your question: {e}");
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

fn filter_relevant_nodes<'a>(
    nodes: &'a [stele::nlu::orchestrator::data_models::KnowledgeNode],
    query: &str,
) -> Vec<&'a stele::nlu::orchestrator::data_models::KnowledgeNode> {
    use stele::nlu::orchestrator::data_models::KnowledgeNode;

    let query_lower = query.to_lowercase();
    let mut relevant_nodes = Vec::new();
    let mut other_nodes = Vec::new();

    for node in nodes {
        let is_relevant = match node {
            KnowledgeNode::Entity(entity) => {
                query_lower.contains(&entity.entity_type.to_lowercase())
                    || query_lower.contains(&entity.name.to_lowercase())
                    || (query_lower.contains("sport")
                        && entity
                            .metadata
                            .as_ref()
                            .and_then(|m| m.get("type"))
                            .and_then(|t| t.as_str())
                            .map(|s| s.contains("sport"))
                            .unwrap_or(false))
                    || entity.confidence > 0.9
            }
            KnowledgeNode::Action(action) => action.confidence > 0.8,
            KnowledgeNode::Temporal(_) | KnowledgeNode::Numerical(_) => false,
        };

        if is_relevant {
            relevant_nodes.push(node);
        } else {
            other_nodes.push(node);
        }
    }

    if relevant_nodes.is_empty() {
        let mut sorted_nodes: Vec<_> = nodes.iter().collect();
        sorted_nodes.sort_by(|a, b| {
            let conf_a = match a {
                KnowledgeNode::Entity(e) => e.confidence,
                KnowledgeNode::Action(a) => a.confidence,
                KnowledgeNode::Temporal(t) => t.confidence,
                KnowledgeNode::Numerical(n) => n.confidence,
            };
            let conf_b = match b {
                KnowledgeNode::Entity(e) => e.confidence,
                KnowledgeNode::Action(a) => a.confidence,
                KnowledgeNode::Temporal(t) => t.confidence,
                KnowledgeNode::Numerical(n) => n.confidence,
            };
            conf_b
                .partial_cmp(&conf_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted_nodes.into_iter().take(10).collect()
    } else {
        relevant_nodes.extend(other_nodes.into_iter().take(5));
        relevant_nodes
    }
}

fn create_focused_prompt(
    query: &str,
    relevant_nodes: &[&stele::nlu::orchestrator::data_models::KnowledgeNode],
    utterances: &serde_json::Value,
) -> String {
    use stele::nlu::orchestrator::data_models::KnowledgeNode;

    let mut prompt = format!("You are a helpful assistant. The user asked: \"{query}\"\n\n");

    if let Some(utterances_array) = utterances.as_array() {
        if !utterances_array.is_empty() {
            prompt.push_str("Based on the following original statements:\n");
            for utterance in utterances_array.iter().take(5) {
                if let Some(text) = utterance.get("raw_text").and_then(|t| t.as_str()) {
                    prompt.push_str(&format!("- \"{text}\"\n"));
                }
            }
            prompt.push('\n');
        }
    }

    if !relevant_nodes.is_empty() {
        prompt.push_str("I found the following relevant entities:\n");
        for node in relevant_nodes.iter().take(10) {
            match node {
                KnowledgeNode::Entity(entity) => {
                    prompt.push_str(&format!(
                        "- {} ({}): {:.0}% confidence",
                        entity.name,
                        entity.entity_type,
                        entity.confidence * 100.0
                    ));
                    if let Some(metadata) = &entity.metadata {
                        if let Some(type_info) = metadata.get("type").and_then(|t| t.as_str()) {
                            prompt.push_str(&format!(" [{type_info}]"));
                        }
                    }
                    prompt.push('\n');
                }
                KnowledgeNode::Action(action) => {
                    prompt.push_str(&format!(
                        "- Action: {} ({:.0}% confidence)\n",
                        action.verb,
                        action.confidence * 100.0
                    ));
                }
                _ => {}
            }
        }
        prompt.push('\n');
    }

    prompt.push_str("Based on the provided data, give a helpful and concise answer to the user's question. Be conversational and focus on the most relevant information.");

    prompt
}

fn format_node_simple(node: &stele::nlu::orchestrator::data_models::KnowledgeNode) -> String {
    use stele::nlu::orchestrator::data_models::KnowledgeNode;

    match node {
        KnowledgeNode::Entity(entity) => {
            format!("{} ({})", entity.name, entity.entity_type)
        }
        KnowledgeNode::Action(action) => {
            format!("Action: {}", action.verb)
        }
        KnowledgeNode::Temporal(temporal) => {
            format!("Time: {}", temporal.date_text)
        }
        KnowledgeNode::Numerical(numerical) => {
            format!("Number: {} {}", numerical.value, numerical.unit)
        }
    }
}
