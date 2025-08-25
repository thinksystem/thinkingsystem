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
use std::sync::Arc;
use stele::database::structured_store::StructuredStore;
use stele::database::types::DatabaseCommand;
use stele::database::{
    connection::DatabaseConnection, dynamic_access::DynamicDataAccessLayer,
    dynamic_storage::DynamicStorage,
};
use stele::llm::dynamic_selector::DynamicModelSelector;
use stele::llm::unified_adapter::UnifiedLLMAdapter;
use stele::nlu::{orchestrator::NLUOrchestrator, query_processor::QueryProcessor};
use tokio::sync::{mpsc, oneshot, RwLock};

pub struct NluRuntime {
    #[allow(dead_code)]
    pub llm: Arc<UnifiedLLMAdapter>,
    #[allow(dead_code)]
    pub orchestrator: Arc<RwLock<NLUOrchestrator>>,
    #[allow(dead_code)]
    pub storage: Arc<DynamicStorage>,
    #[allow(dead_code)]
    pub access: Arc<DynamicDataAccessLayer>,
    pub query_processor: QueryProcessor,
    pub db: Arc<surrealdb::Surreal<surrealdb::engine::remote::ws::Client>>,
    pub canonical_db: Option<Arc<surrealdb::Surreal<surrealdb::engine::remote::ws::Client>>>,
}

impl NluRuntime {
    pub async fn init() -> Result<Self> {
        
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
            .await
            .expect("connect send");
        let connect_result = connect_response_rx.await.expect("connect rx");
        if let Err(e) = connect_result {
            
            let allow = std::env::var("STELE_ALLOW_CONTAMINATION")
                .ok()
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            if allow {
                eprintln!("WARN: {e}");
            } else {
                panic!("db connect result: {e}");
            }
        }
        let db_client = client_rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("Failed to receive DB client"))?;

        
        let canonical_db = {
            let canon_ready = std::env::var("STELE_CANON_URL").is_ok()
                && std::env::var("STELE_CANON_USER").is_ok()
                && std::env::var("STELE_CANON_PASS").is_ok()
                && std::env::var("STELE_CANON_NS").is_ok()
                && std::env::var("STELE_CANON_DB").is_ok();
            if canon_ready {
                match StructuredStore::connect_canonical_from_env().await {
                    Ok(c) => Some(c),
                    Err(e) => {
                        return Err(anyhow::anyhow!(
                            "Canonical DB required but connection failed: {e}"
                        ));
                    }
                }
            } else {
                return Err(anyhow::anyhow!(
                    "Canonical DB required. Set STELE_CANON_URL/USER/PASS/NS/DB and ensure it differs from SURREALDB_NS/DB."
                ));
            }
        };

        
        let dyn_ns = std::env::var("SURREALDB_NS").unwrap_or_default();
        let dyn_db = std::env::var("SURREALDB_DB").unwrap_or_default();
        let canon_ns = std::env::var("STELE_CANON_NS").unwrap_or_default();
        let canon_db = std::env::var("STELE_CANON_DB").unwrap_or_default();
        if let Some(_canon_client) = &canonical_db {
            if !canon_ns.is_empty()
                && dyn_ns == canon_ns
                && dyn_db == canon_db
                && !dyn_ns.is_empty()
            {
                return Err(anyhow::anyhow!("Namespace separation violation: dynamic ({dyn_ns}/{dyn_db}) matches canonical ({canon_ns}/{canon_db})"));
            }
            
            let mut leak_tables: Vec<&'static str> = Vec::new();
            // We'll attempt INFO FOR DB; and parse tables list
            if let Ok(mut info_res) = db_client.clone().query("INFO FOR DB;").await {
                if let Ok::<Vec<serde_json::Value>, _>(vals) = info_res.take(0) {
                    
                    if let Some(first) = vals.first() {
                        if let Some(tables) = first.get("tables") {
                            let names: Vec<String> = match tables {
                                serde_json::Value::Array(arr) => arr
                                    .iter()
                                    .filter_map(|t| {
                                        t.get("name")
                                            .and_then(|n| n.as_str())
                                            .map(|s| s.to_string())
                                    })
                                    .collect(),
                                _ => Vec::new(),
                            };
                            for t in [
                                "canonical_entity",
                                "canonical_event",
                                "canonical_task",
                                "canonical_relationship_fact",
                            ]
                            .iter()
                            {
                                if names.iter().any(|n| n == t) {
                                    leak_tables.push(t);
                                }
                            }
                        }
                    }
                }
            }
            if !leak_tables.is_empty() {
                eprintln!("WARNING: Canonical tables present in dynamic namespace ({dyn_ns}/{dyn_db}): {leak_tables:?}. They should only exist in canonical namespace {canon_ns}/{canon_db}. Consider cleaning old data or using a fresh namespace.");
            }
            
            eprintln!("INFO: Dynamic namespace/db = {dyn_ns}/{dyn_db}; Canonical namespace/db = {canon_ns}/{canon_db}");
        }

        
        let llm_models_config_path =
            if std::path::Path::new("crates/stele/src/nlu/config/llm_models.yml").exists() {
                "crates/stele/src/nlu/config/llm_models.yml"
            } else {
                "./crates/stele/src/nlu/config/llm_models.yml"
            };
        let selector = Arc::new(DynamicModelSelector::from_config_path(
            llm_models_config_path,
        )?);
        let llm = Arc::new(UnifiedLLMAdapter::new(selector).await?);

        
        let config_dir = if std::path::Path::new("crates/stele/src/nlu/config").exists() {
            "crates/stele/src/nlu/config"
        } else {
            "./crates/stele/src/nlu/config"
        };
        let qp_path =
            if std::path::Path::new("crates/stele/src/nlu/config/query_processor.toml").exists() {
                "crates/stele/src/nlu/config/query_processor.toml"
            } else {
                "./crates/stele/src/nlu/config/query_processor.toml"
            };

        let orchestrator = Arc::new(RwLock::new(
            NLUOrchestrator::with_unified_adapter(config_dir, llm.clone()).await?,
        ));
        
        let storage = Arc::new(DynamicStorage::with_regulariser(
            db_client.clone(),
            canonical_db.is_some(),
        ));
        let access = Arc::new(DynamicDataAccessLayer::new(db_client.clone(), llm.clone()).await?);
        let query_processor =
            QueryProcessor::new(orchestrator.clone(), storage.clone(), qp_path).await?;

        Ok(Self {
            llm,
            orchestrator,
            storage,
            access,
            query_processor,
            db: db_client.clone(),
            canonical_db,
        })
    }
}
