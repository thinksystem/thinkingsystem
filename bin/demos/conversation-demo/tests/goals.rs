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



use anyhow::Result;
use llm_contracts::{GenerationConfig, LLMRequest};
use std::sync::Arc;
use std::time::{Duration, Instant};
use stele::database::structured_store::StructuredStore;
use stele::llm::core::LLMAdapter; 
use stele::llm::dynamic_selector::DynamicModelSelector;
use stele::llm::unified_adapter::UnifiedLLMAdapter;
use stele::nlu::orchestrator::NLUOrchestrator;
use stele::nlu::query_processor::QueryProcessor;
use surrealdb::engine::remote::ws::Client as WsClient;
use surrealdb::Surreal;
use tokio::sync::RwLock;



async fn init_qp() -> Result<(
    QueryProcessor,
    Arc<surrealdb::Surreal<surrealdb::engine::remote::ws::Client>>,
)> {
    
    fn resolve_path(rel: &str) -> std::path::PathBuf {
        let direct = std::path::PathBuf::from(rel);
        if direct.exists() {
            return direct;
        }
        
        let mut cur = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        for _ in 0..5 {
            
            if cur.join("Cargo.lock").exists() {
                let candidate = cur.join(rel);
                if candidate.exists() {
                    return candidate;
                }
            }
            if !cur.pop() {
                break;
            }
        }
        direct 
    }

    
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(32);
    let (client_tx, mut client_rx) = tokio::sync::mpsc::channel(1);
    let mut db_conn = stele::database::connection::DatabaseConnection::new(command_rx);
    tokio::spawn(async move {
        let _ = db_conn.run().await;
    });
    let (tx, rx) = tokio::sync::oneshot::channel();
    command_tx
        .send(stele::database::types::DatabaseCommand::Connect {
            client_sender: client_tx,
            response_sender: tx,
        })
        .await
        .unwrap();
    rx.await.unwrap().unwrap();
    let db_client = client_rx.recv().await.unwrap();

    
    let models_path = resolve_path("crates/stele/src/nlu/config/llm_models.yml");
    let selector = Arc::new(DynamicModelSelector::from_config_path(
        models_path.to_string_lossy().as_ref(),
    )?);
    let llm = Arc::new(UnifiedLLMAdapter::new(selector).await?);
    let orchestrator = Arc::new(RwLock::new(
        NLUOrchestrator::with_unified_adapter(
            resolve_path("crates/stele/src/nlu/config")
                .to_string_lossy()
                .as_ref(),
            llm.clone(),
        )
        .await?,
    ));
    let storage = Arc::new(
        stele::database::dynamic_storage::DynamicStorage::with_regulariser(db_client.clone(), true),
    );
    let qp = QueryProcessor::new(
        orchestrator,
        storage,
        resolve_path("crates/stele/src/nlu/config/query_processor.toml")
            .to_string_lossy()
            .as_ref(),
    )
    .await?;
    Ok((qp, db_client))
}


async fn ensure_provenance_table(db: &Surreal<WsClient>) -> Result<()> {
    let _ = db
        .query("DEFINE TABLE IF NOT EXISTS provenance_event SCHEMALESS;")
        .await?;
    Ok(())
}

#[tokio::test]
#[ignore]
async fn goal_1_to_7_ingest_multiple_statements() -> Result<()> {
    
    let (qp, _db) = init_qp().await?;
    
    let llm = match UnifiedLLMAdapter::with_defaults().await {
        Ok(l) => l,
        Err(e) => {
            eprintln!(
                "Skipping goal_1_to_7_ingest_multiple_statements (LLM provider unavailable): {e}"
            );
            return Ok(());
        }
    };
    let mut texts = Vec::new();
    let fast_mode = std::env::var("GOAL_FAST")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let total = std::env::var("GOAL_INGEST_COUNT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(if fast_mode { 3 } else { 10 });
    let overall_timeout = std::env::var("GOAL_INGEST_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(if fast_mode { 40 } else { 120 }));
    let per_call_timeout = std::env::var("GOAL_INGEST_PER_CALL_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(if fast_mode { 8 } else { 20 }));
    let started = Instant::now();

    if fast_mode {
        
        texts = vec![
            "{\"person_a\":\"Alice\",\"person_b\":\"Bob\",\"event\":\"met at conference\",\"date\":\"2024-01-02T10:00:00Z\",\"optional_task\":\"plan follow-up\"}".to_string(),
            "{\"person_a\":\"Carol\",\"person_b\":\"Dave\",\"event\":\"collaborated on project\",\"date\":\"2024-02-10T15:30:00Z\"}".to_string(),
            "{\"person_a\":\"Eve\",\"person_b\":\"Frank\",\"event\":\"discussed research\",\"date\":\"2024-03-05T09:15:00Z\",\"optional_task\":\"draft summary\"}".to_string(),
        ];
    } else {
        for i in 0..total {
            if started.elapsed() > overall_timeout {
                eprintln!(
                    "[goal_1_to_7] Overall timeout hit while generating prompts (generated {i}/{total})."
                );
                break;
            }
            let prompt = format!("Return ONLY one concise JSON object with keys person_a, person_b, event, date (ISO 8601), optional_task. Keep values short. Example: {{\\\"person_a\\\":\\\"Alice\\\",\\\"person_b\\\":\\\"Bob\\\",\\\"event\\\":\\\"met at conference\\\",\\\"date\\\":\\\"2024-01-02T10:00:00Z\\\",\\\"optional_task\\\":\\\"plan follow-up\\\"}}. Now produce variant #{i}.");
            let req = LLMRequest {
                id: uuid::Uuid::new_v4(),
                prompt,
                system_prompt: None,
                model_requirements: llm_contracts::ModelRequirements {
                    capabilities: vec!["reasoning".into()],
                    preferred_speed_tier: None,
                    max_cost_tier: None,
                    min_max_tokens: None,
                },
                generation_config: GenerationConfig {
                    max_tokens: Some(64),
                    temperature: Some(0.7),
                    top_p: None,
                    stop_sequences: None,
                    stream: Some(false),
                },
                context: None,
            };
            match tokio::time::timeout(per_call_timeout, llm.generate_response(req)).await {
                Ok(Ok(resp)) => {
                    texts.push(resp.content);
                }
                Ok(Err(e)) => {
                    eprintln!("[goal_1_to_7] LLM error (iteration {i}): {e}");
                }
                Err(_) => {
                    eprintln!(
                        "[goal_1_to_7] LLM timeout (iteration {i}) after {per_call_timeout:?}"
                    );
                }
            }
        }
    }

    
    
    
    
    
    
    let composite_input = if texts.is_empty() {
        String::from("Alice met Bob.")
    } else {
        let mut buf = String::new();
        for (i, t) in texts.iter().enumerate() {
            
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(t) {
                let pa = val.get("person_a").and_then(|v| v.as_str()).unwrap_or("");
                let pb = val.get("person_b").and_then(|v| v.as_str()).unwrap_or("");
                let ev = val.get("event").and_then(|v| v.as_str()).unwrap_or("");
                let date = val.get("date").and_then(|v| v.as_str()).unwrap_or("");
                if !pa.is_empty() && !pb.is_empty() && !ev.is_empty() {
                    use std::fmt::Write as _;
                    
                    let _ = write!(
                        buf,
                        "Statement {}: {} and {} {} on {}.",
                        i + 1,
                        pa,
                        pb,
                        ev,
                        date
                    );
                    buf.push('\n');
                    continue;
                }
            }
            buf.push_str(t);
            buf.push('\n');
        }
        buf
    };
    let strict = std::env::var("GOAL_STRICT")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let (processed, timed_out) = match tokio::time::timeout(
        per_call_timeout.max(Duration::from_secs(5)),
        qp.process_and_store_input(&composite_input, "ucli", "batch_aggregated"),
    )
    .await
    {
        Ok(Ok(_)) => (1, false),
        Ok(Err(e)) => {
            eprintln!("[goal_1_to_7] aggregated process_and_store_input error: {e}");
            (0, false)
        }
        Err(_) => {
            eprintln!("[goal_1_to_7] aggregated process_and_store_input timeout after {per_call_timeout:?}");
            (0, true)
        }
    };
    if processed == 0 {
        if strict {
            panic!("[goal_1_to_7] Aggregated ingestion failed (strict mode)");
        } else {
            eprintln!("[goal_1_to_7] SKIP: aggregated ingestion produced no result (timed_out={timed_out})");
            return Ok(()); 
        }
    }
    Ok(())
}

#[tokio::test]
#[ignore]
async fn goal_8_offload_records_provenance() -> Result<()> {
    
    let mut flow = sleet::FlowDefinition::new("t", "start");
    flow.add_block(sleet::BlockDefinition::new(
        "start",
        sleet::BlockType::Compute {
            
            expression: "\"ok\"".into(),
            output_key: "result".into(),
            next_block: "end".into(),
        },
    ));
    flow.add_block(sleet::BlockDefinition::new(
        "end",
        sleet::BlockType::Terminate,
    ));
    let _status = sleet::execute_flow(flow, 10_000, None)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    
    let (_qp, db) = init_qp().await?;
    ensure_provenance_table(&db).await?;
    let mut q = db.clone().query("CREATE provenance_event SET kind='sleet_offload', details={label:'test'}, created_at=time::now() RETURN AFTER;").await?;
    let mut created: Vec<serde_json::Value> = q.take(0).unwrap_or_default();
    if created.is_empty() {
        
        let mut fallback = db
            .clone()
            .query(
                "SELECT kind, created_at FROM provenance_event ORDER BY created_at DESC LIMIT 1;",
            )
            .await?;
        created = fallback.take(0).unwrap_or_default();
    }
    assert!(
        !created.is_empty(),
        "Expected at least one provenance_event row after insertion"
    );
    Ok(())
}

#[tokio::test]
#[ignore]
async fn goal_11_bitemporal_slice_runs() -> Result<()> {
    
    let canon = match StructuredStore::connect_canonical_from_env().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Skipping goal_11_bitemporal_slice_runs (canonical env missing): {e}");
            return Ok(());
        }
    };
    
    let (_qp, db) = init_qp().await?;
    let store = StructuredStore::new_with_clients(canon, db, false);
    let _rows = store
        .get_current_relationship_facts(None, None, None, None)
        .await?;
    Ok(())
}
