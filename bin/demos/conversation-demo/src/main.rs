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



mod args;
mod nlu_runtime;
mod telegram_client;
#[cfg(feature = "ui")]
mod ui;
pub use args::Args;

use anyhow::Result;
use clap::{Parser, Subcommand};
use sleet::{execute_flow, BlockDefinition, BlockType, FlowDefinition};
use stele::database::structured_store::StructuredStore;
use stele::llm::core::LLMAdapter as _;
use stele::llm::unified_adapter::UnifiedLLMAdapter;
use stele::scribes::specialists::knowledge_scribe::enrich_utterance;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Subcommand, Debug, Clone)]
enum Commands {
    
    Ingest {
        #[arg(long)]
        user: String,
        #[arg(long)]
        channel: String,
        #[arg(long)]
        text: String,
    },
    
    Smoke {
        
        #[arg(long, default_value = "generator")]
        user: String,
        
        #[arg(long, default_value = "bulk_llm")]
        channel: String,
        
        #[arg(long, default_value_t = 5)]
        count: usize,
        
        #[arg(long)]
        seed: Option<u64>,
        
        #[arg(long, default_value = "smoke-offload")]
        label: String,
    },
    
    Offload {
        #[arg(long, default_value = "phase8-offload")]
        label: String,
    },
    
    Bitemporal {
        
        #[arg(long)]
        as_of: Option<String>,
        #[arg(long)]
        predicate: Option<String>,
        #[arg(long)]
        branch: Option<String>,
    },
    
    DbSummary,
    
    Generate {
        
        #[arg(long, default_value = "generator")]
        user: String,
        
        #[arg(long, default_value = "bulk_llm")]
        channel: String,
        
        #[arg(long, default_value_t = 5)]
        count: usize,
        
        #[arg(long)]
        seed: Option<u64>,
    },
    
    #[cfg(feature = "ui")]
    UiPanels,
    
}

#[derive(Parser, Debug, Clone)]
#[command(name = "conversation-demo")]
#[command(
    about = "CLI for NLU ingest, sleet offload, and bitemporal queries. UI available with --features ui."
)]
struct Cli {
    #[arg(long, default_value_t = false)]
    debug: bool,
    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    
    let args = Cli::parse();
    
    let filter = if args.debug {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new("debug,reqwest=info,hyper=info,h2=info,hyper_util=info,rustls=info")
        })
    } else {
        EnvFilter::new("info,reqwest=warn,hyper=warn,h2=warn,hyper_util=warn,rustls=warn,tungstenite=warn,tokio_tungstenite=warn")
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
    info!("Starting Conversation Demo (CLI)");

    
    let dyn_ns = std::env::var("SURREALDB_NS").unwrap_or_default();
    let dyn_db = std::env::var("SURREALDB_DB").unwrap_or_default();
    let canon_ok = [
        "STELE_CANON_URL",
        "STELE_CANON_USER",
        "STELE_CANON_PASS",
        "STELE_CANON_NS",
        "STELE_CANON_DB",
    ]
    .into_iter()
    .all(|k| std::env::var(k).is_ok());
    if !canon_ok {
        eprintln!(
            "ERROR: Canonical DB required for this demo. Please set STELE_CANON_URL/USER/PASS/NS/DB to a separate namespace/db from SURREALDB_NS/DB."
        );
        std::process::exit(1);
    }
    if let (Ok(c_ns), Ok(c_db)) = (
        std::env::var("STELE_CANON_NS"),
        std::env::var("STELE_CANON_DB"),
    ) {
        if c_ns == dyn_ns && c_db == dyn_db {
            eprintln!(
                "ERROR: Canonical (STELE_CANON_NS/DB) must differ from dynamic (SURREALDB_NS/DB). Current: {c_ns}/{c_db}."
            );
            std::process::exit(1);
        }
    }
    eprintln!("INFO: Dynamic namespace/db = {dyn_ns}/{dyn_db}");

    match args.command {
        Commands::Ingest {
            user,
            channel,
            text,
        } => {
            let rt = nlu_runtime::NluRuntime::init().await?;
            let out = rt
                .query_processor
                .process_and_store_input(&text, &user, &channel)
                .await?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Commands::Offload { label } => {
            
            let mut flow = FlowDefinition::new("phase8_demo", "start");
            flow.add_block(BlockDefinition::new(
                "start",
                BlockType::Compute {
                    
                    expression: "1".into(),
                    output_key: "result".into(),
                    next_block: "end".into(),
                },
            ));
            flow.add_block(BlockDefinition::new("end", BlockType::Terminate));
            let status = execute_flow(flow, 10_000, None)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("offload_status={status:?}");
            
            let rt = nlu_runtime::NluRuntime::init().await?;
            let mut q = rt
                .db
                .clone()
                .query("CREATE provenance_event SET kind = 'sleet_offload', details = $d, created_at = time::now() RETURN AFTER;")
                .bind(("d", serde_json::json!({"label": label})) )
                .await?;
            let created: Vec<serde_json::Value> = q.take(0).unwrap_or_default();
            if let Some(first) = created.first() {
                println!(
                    "provenance_event={}",
                    first["id"].as_str().unwrap_or("<unknown>")
                );
            }
        }
        Commands::Bitemporal {
            as_of,
            predicate,
            branch,
        } => {
            
            let rt = nlu_runtime::NluRuntime::init().await?;
            let Some(canon) = rt.canonical_db.clone() else {
                warn!("Canonical DB not configured (STELE_CANON_*). Bitemporal query skipped.");
                return Ok(());
            };
            let store = StructuredStore::new_with_clients(canon, rt.db.clone(), false);
            let rows = if let Some(ts) = as_of.as_deref() {
                store
                    .get_relationship_facts_as_of(
                        None,
                        predicate.as_deref(),
                        None,
                        Some(ts),
                        Some(ts),
                        branch.as_deref(),
                    )
                    .await?
            } else {
                store
                    .get_current_relationship_facts(
                        None,
                        predicate.as_deref(),
                        None,
                        branch.as_deref(),
                    )
                    .await?
            };
            println!("facts={} (showing up to 20)", rows.len());
            for r in rows.iter().take(20) {
                println!("{}", serde_json::to_string_pretty(r)?);
            }
        }
        Commands::DbSummary => {
            let rt = nlu_runtime::NluRuntime::init().await?;
            
            if let Ok(ms) = std::env::var("STELE_SUMMARY_DELAY_MS") {
                if let Ok(ms) = ms.parse::<u64>() {
                    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                }
            }
            
            let mut q = rt
                .db
                .clone()
                .query("SELECT count() as c FROM nodes; SELECT count() as c FROM edges;")
                .await?;
            let nodes_c: Vec<serde_json::Value> = q.take(0).unwrap_or_default();
            let edges_c: Vec<serde_json::Value> = q.take(1).unwrap_or_default();
            let n = nodes_c.first().and_then(|v| v["c"].as_u64()).unwrap_or(0);
            let e = edges_c.first().and_then(|v| v["c"].as_u64()).unwrap_or(0);
            println!("dynamic_nodes={n} dynamic_edges={e}");
            if let Some(canon) = rt.canonical_db.clone() {
                let mut qq = canon.query("SELECT count() as c FROM canonical_entity; SELECT count() as c FROM canonical_relationship_fact;").await?;
                let ce: Vec<serde_json::Value> = qq.take(0).unwrap_or_default();
                let cr: Vec<serde_json::Value> = qq.take(1).unwrap_or_default();
                let ce_c = ce.first().and_then(|v| v["c"].as_u64()).unwrap_or(0);
                let cr_c = cr.first().and_then(|v| v["c"].as_u64()).unwrap_or(0);
                println!("canonical_entities={ce_c} canonical_relationship_facts={cr_c}");
            }
        }
        Commands::Generate {
            user,
            channel,
            count,
            seed,
        } => {
            
            let rt = nlu_runtime::NluRuntime::init().await?;
            let llm = UnifiedLLMAdapter::with_defaults().await?;
            let _ = seed; 
            let mut successes = 0usize;
            for i in 0..count {
                let prompt = format!("Generate one short, fact-like sentence containing two named people, a dated event with a time, and optionally a task or location. Make it realistic and concise. Item #{i}.");
                let req = llm_contracts::LLMRequest {
                    id: uuid::Uuid::new_v4(),
                    prompt,
                    system_prompt: None,
                    model_requirements: llm_contracts::ModelRequirements {
                        capabilities: vec!["reasoning".into()],
                        preferred_speed_tier: None,
                        max_cost_tier: None,
                        min_max_tokens: None,
                    },
                    generation_config: llm_contracts::GenerationConfig {
                        max_tokens: Some(64),
                        temperature: Some(0.7),
                        top_p: None,
                        stop_sequences: None,
                        stream: Some(false),
                    },
                    context: None,
                };
                match llm.generate_response(req).await {
                    Ok(resp) => {
                        match rt
                            .query_processor
                            .process_and_store_input(&resp.content, &user, &channel)
                            .await
                        {
                            Ok(_) => {
                                successes += 1;
                                if i % 10 == 9 {
                                    println!("ingested={successes}/{count}");
                                }
                            }
                            Err(e) => eprintln!("ingest_error[{i}]: {e}"),
                        }
                    }
                    Err(e) => eprintln!("llm_error[{i}]: {e}"),
                }
            }
            println!("ingested_total={successes}/{count}");
            
            if let Ok(ms) = std::env::var("STELE_SUMMARY_DELAY_MS") {
                if let Ok(ms) = ms.parse::<u64>() {
                    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                }
            }
            let mut q = rt
                .db
                .clone()
                .query("SELECT count() as c FROM nodes; SELECT count() as c FROM edges;")
                .await?;
            let nodes_c: Vec<serde_json::Value> = q.take(0).unwrap_or_default();
            let edges_c: Vec<serde_json::Value> = q.take(1).unwrap_or_default();
            let n = nodes_c.first().and_then(|v| v["c"].as_u64()).unwrap_or(0);
            let e = edges_c.first().and_then(|v| v["c"].as_u64()).unwrap_or(0);
            println!("post_ingest dynamic_nodes={n} dynamic_edges={e}");
        }
        Commands::Smoke {
            user,
            channel,
            count,
            seed,
            label,
        } => {
            
            let rt = nlu_runtime::NluRuntime::init().await?;
            let llm = UnifiedLLMAdapter::with_defaults().await?;
            let _ = seed; 
            let mut successes = 0usize;
            let mut utterance_ids: Vec<String> = Vec::new();
            for i in 0..count {
                let prompt = format!("Generate one short, fact-like sentence containing two named people, a dated event with a time, and optionally a task or location. Make it realistic and concise. Item #{i}.");
                let req = llm_contracts::LLMRequest {
                    id: uuid::Uuid::new_v4(),
                    prompt,
                    system_prompt: None,
                    model_requirements: llm_contracts::ModelRequirements {
                        capabilities: vec!["reasoning".into()],
                        preferred_speed_tier: None,
                        max_cost_tier: None,
                        min_max_tokens: None,
                    },
                    generation_config: llm_contracts::GenerationConfig {
                        max_tokens: Some(64),
                        temperature: Some(0.7),
                        top_p: None,
                        stop_sequences: None,
                        stream: Some(false),
                    },
                    context: None,
                };
                match llm.generate_response(req).await {
                    Ok(resp) => match rt
                        .query_processor
                        .process_and_store_input(&resp.content, &user, &channel)
                        .await
                    {
                        Ok(v) => {
                            successes += 1;
                            if let Some(s) = v.get("utterance_id").and_then(|x| x.as_str()) {
                                utterance_ids.push(s.to_string());
                            }
                        }
                        Err(e) => eprintln!("ingest_error[{i}]: {e}"),
                    },
                    Err(e) => eprintln!("llm_error[{i}]: {e}"),
                }
            }
            println!("ingested_total={successes}/{count}");

            
            let mut total_embeds = 0usize;
            let mut total_props = 0usize;
            for uid in &utterance_ids {
                match enrich_utterance(rt.db.clone(), uid.clone()).await {
                    Ok((e, p)) => {
                        total_embeds += e;
                        total_props += p;
                    }
                    Err(e) => warn!(error = %e, utterance = %uid, "enrich_utterance failed"),
                }
            }
            println!(
                "knowledge_embeddings_added={total_embeds} proposed_edges_added={total_props}"
            );

            
            let mut flow = FlowDefinition::new("phase8_demo", "start");
            flow.add_block(BlockDefinition::new(
                "start",
                BlockType::Compute {
                    expression: "1".into(),
                    output_key: "result".into(),
                    next_block: "end".into(),
                },
            ));
            flow.add_block(BlockDefinition::new("end", BlockType::Terminate));
            let status = execute_flow(flow, 10_000, None)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("offload_status={status:?}");

            
            let store = if let Some(canon) = rt.canonical_db.clone() {
                StructuredStore::new_with_clients(canon, rt.db.clone(), false)
            } else {
                
                StructuredStore::new(rt.db.clone())
            };
            let prov = store
                .create_provenance_event("sleet_offload", serde_json::json!({"label": label}))
                .await?;
            info!(prov = %prov, message = "provenance_event created");
            for uid in &utterance_ids {
                if let Some((tb, idpart)) = uid.split_once(':') {
                    let utt = surrealdb::RecordId::from((tb, idpart));
                    if let Err(e) = store.relate_utterance_to_provenance(&utt, &prov).await {
                        warn!(error = %e, utterance = %uid, "failed to relate utterance to provenance_event");
                    }
                } else {
                    warn!(utterance = %uid, "invalid utterance id format; expected 'table:id'");
                }
            }

            
            if let Ok(ms) = std::env::var("STELE_SUMMARY_DELAY_MS") {
                if let Ok(ms) = ms.parse::<u64>() {
                    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                }
            }

            
            let mut q = rt
                .db
                .clone()
                .query("SELECT count() as c FROM nodes; SELECT count() as c FROM edges; SELECT count() as c FROM knowledge_embedding; SELECT count() as c FROM proposed_edge; SELECT count() as c FROM utterance_has_provenance;")
                .await?;
            let nodes_c: Vec<serde_json::Value> = q.take(0).unwrap_or_default();
            let edges_c: Vec<serde_json::Value> = q.take(1).unwrap_or_default();
            let ke_c: Vec<serde_json::Value> = q.take(2).unwrap_or_default();
            let pe_c: Vec<serde_json::Value> = q.take(3).unwrap_or_default();
            let prov_c: Vec<serde_json::Value> = q.take(4).unwrap_or_default();
            let n = nodes_c.first().and_then(|v| v["c"].as_u64()).unwrap_or(0);
            let e = edges_c.first().and_then(|v| v["c"].as_u64()).unwrap_or(0);
            let ke = ke_c.first().and_then(|v| v["c"].as_u64()).unwrap_or(0);
            let pe = pe_c.first().and_then(|v| v["c"].as_u64()).unwrap_or(0);
            let up = prov_c.first().and_then(|v| v["c"].as_u64()).unwrap_or(0);
            println!(
                "dynamic_nodes={n} dynamic_edges={e} knowledge_embeddings={ke} proposed_edges={pe} utterance_has_provenance={up}"
            );
            if let Some(canon) = rt.canonical_db.clone() {
                let mut qq = canon
                    .query("SELECT count() as c FROM canonical_entity; SELECT count() as c FROM canonical_relationship_fact; SELECT count() as c FROM canonical_task;")
                    .await?;
                let ce: Vec<serde_json::Value> = qq.take(0).unwrap_or_default();
                let cr: Vec<serde_json::Value> = qq.take(1).unwrap_or_default();
                let ct: Vec<serde_json::Value> = qq.take(2).unwrap_or_default();
                let ce_c = ce.first().and_then(|v| v["c"].as_u64()).unwrap_or(0);
                let cr_c = cr.first().and_then(|v| v["c"].as_u64()).unwrap_or(0);
                let ct_c = ct.first().and_then(|v| v["c"].as_u64()).unwrap_or(0);
                println!("canonical_entities={ce_c} canonical_relationship_facts={cr_c} canonical_tasks={ct_c}");
            }
        }
        #[cfg(feature = "ui")]
        Commands::UiPanels => {
            
            let ui_args = Args::from_env();
            let options = eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_inner_size([1024.0, 720.0])
                    .with_title("Conversation Demo – Telegram UI"),
                ..Default::default()
            };
            eframe::run_native(
                "Conversation Demo – Telegram UI",
                options,
                Box::new(|_cc| Ok(Box::new(ui::App::new(ui_args)))),
            )
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        }
    }
    Ok(())
}
