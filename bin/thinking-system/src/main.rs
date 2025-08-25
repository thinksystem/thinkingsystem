// SPDX-License-Identifier: AGPL-3.0-only
// Minimal bootstrap; all runtime logic & handlers reside in library modules.
use anyhow::Result;
use sleet::agents::{registry::AgentRegistry, RegistryConfig};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use steel::data_exchange::DataExchangeProcessor;

use stele::llm::unified_adapter::UnifiedLLMAdapter;
use thinking_system::{
    config::{load_data_exchange_config, load_system_config},
    http::routes::{
        build_router, iroh_secret_from_env, load_policy_engine, random_secret_key,
        spawn_accept_loop, start_quic_endpoint,
    },
    state::maybe_init_database,
    AppState,
};
use tokio::{sync::RwLock, time::sleep};
use tracing::{info, warn};
mod messaging; 
use clap::{Parser, Subcommand};
use thinking_system::scribes;

#[derive(Parser, Debug, Clone)]
#[command(name = "thinking-system", about = "Unified Thinking System runtime")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Command>,
}

#[derive(Subcommand, Debug, Clone)]
enum Command {
    
    Serve,
    
    Ui {
        #[arg(long)]
        layout: Option<std::path::PathBuf>,
    },
    
    UiScribes,
    
    UiTelegram {
        
        #[arg(long)]
        token: Option<String>,
        
        #[arg(long, value_name = "CHAT_ID")]
        chat_id: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();
    let cli = Cli::parse();
    match cli.cmd.unwrap_or(Command::Serve) {
        Command::Serve => run_server().await,
        Command::Ui { layout } => launch_ui(layout).await,
        Command::UiScribes => launch_scribes_ui().await,
        Command::UiTelegram { token, chat_id } => launch_telegram_ui(token, chat_id).await,
    }
}

async fn launch_ui(_layout: Option<std::path::PathBuf>) -> Result<()> {
    
    use thinking_system_ui::backend_api::{BackendFuture, BackendRunner};
    let runner: BackendRunner = std::sync::Arc::new(|ui_bridge, scenario| {
        Box::pin(async move { scribes::run_with_ui(ui_bridge, scenario).await }) as BackendFuture
    });
    
    let generator = Some(scribes::build_demo_scenario_generator());
    thinking_system_ui::run_ui_with_backend_and_generator_async(runner, generator)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

async fn launch_scribes_ui() -> Result<()> {
    use tokio::process::Command;
    warn!("Launching Scribes Demo UI via subprocess (cargo run -p scribes-demo -- --gui)");
    let status = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("scribes-demo")
        .arg("--")
        .arg("--gui")
        .status()
        .await?;
    if !status.success() {
        anyhow::bail!("scribes-demo UI exited with status: {status}");
    }
    Ok(())
}

async fn launch_telegram_ui(token: Option<String>, chat_id: Option<String>) -> Result<()> {
    
    let token = token
        .or_else(|| std::env::var("TELEGRAM_BOT_TOKEN").ok())
        .or_else(|| std::env::var("TS_TELEGRAM_BOT_TOKEN").ok());
    let chat_id = chat_id.or_else(|| std::env::var("TELEGRAM_CHAT_ID").ok());

    println!("Starting Simple Telegram Demo...");
    if token.is_some() {
        println!("Bot token loaded from config");
    } else {
        println!("No bot token found. You'll need to enter it in the UI.");
    }
    if chat_id.is_some() {
        println!("Chat ID loaded from config");
    } else {
        println!("No chat ID found. You'll need to enter it in the UI.");
    }

    let backend = thinking_system::telegram::new_ui_backend();
    thinking_system_ui::run_telegram_ui_with_backend_and_prefill_async(backend, token, chat_id)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

async fn run_server() -> Result<()> {
    info!("thinking-system starting");

    let secret = iroh_secret_from_env().unwrap_or_else(random_secret_key);
    let endpoint = start_quic_endpoint(secret, "steel/data-exchange/0").await?;
    info!(node_id=%endpoint.node_id(), "iroh endpoint ready");

    let llm = Arc::new(UnifiedLLMAdapter::with_defaults().await?);
    let (jwt, vc) = thinking_system::core::auth::init_auth_managers();
    let policy_opt = load_policy_engine().await;

    
    let system_cfg = load_system_config().await;

    let processor = match load_data_exchange_config().await? {
        Some(cfg) => match DataExchangeProcessor::new(&cfg).await {
            Ok(p) => {
                info!(count=%cfg.providers.len(), "data exchange initialised");
                Some(Arc::new(RwLock::new(p)))
            }
            Err(e) => {
                warn!("data exchange init error: {e}");
                None
            }
        },
        None => None,
    };
    

    
    let env_flag = thinking_system::core::util::env_flag;

    let persist = env_flag("TS_AGENT_PERSIST");
    let store_path = std::env::var("TS_AGENT_STORE_PATH").ok();
    let agent_registry = AgentRegistry::new(RegistryConfig {
        persistent: persist,
        max_in_memory: 128,
        storage_path: store_path,
    })
    .expect("agent registry init");
    
    let primary_db = maybe_init_database().await;
    let knowledge_enabled = env_flag("TS_ENABLE_KNOWLEDGE");

    
    let scribe_runtime = if knowledge_enabled {
        
        let data_scribe_res = stele::scribes::specialists::data_scribe::DataScribe::new(
            "data-runtime".into(),
            "config/nlu_core",
        )
        .await;
        match data_scribe_res {
            Ok(data_scribe) => {
                match stele::scribes::runtime::ScribeRuntimeManager::new(
                    stele::scribes::specialists::identity_scribe::IdentityScribe::new(
                        "identity-runtime".into(),
                    ),
                    data_scribe,
                    stele::scribes::specialists::knowledge_scribe::KnowledgeScribe::new(
                        "knowledge-runtime".into(),
                    ),
                )
                .await
                {
                    Ok(rt) => Some(Arc::new(rt)),
                    Err(e) => {
                        warn!(error=%e, "scribe runtime init failed");
                        None
                    }
                }
            }
            Err(e) => {
                warn!(error=%e, "data scribe init failed");
                None
            }
        }
    } else {
        None
    };

    let state = AppState {
        endpoint: endpoint.clone(),
        processor,
        llm,
        jwt,
        vc,
        policy: Arc::new(RwLock::new(policy_opt)),
        agents: Arc::new(RwLock::new(agent_registry)),
        db_client: primary_db.clone(),
        nlu: thinking_system::nlu::init_optional_nlu().await,
        knowledge_scribe: if knowledge_enabled {
            Some(Arc::new(tokio::sync::RwLock::new(
                stele::scribes::specialists::knowledge_scribe::KnowledgeScribe::new(
                    "supervisor-ks".into(),
                ),
            )))
        } else {
            None
        },
        kg_service: Arc::new(thinking_system::kg::KgService::new(
            primary_db.clone(),
            if knowledge_enabled {
                Some(Arc::new(tokio::sync::RwLock::new(
                    stele::scribes::specialists::knowledge_scribe::KnowledgeScribe::new(
                        "kg-service".into(),
                    ),
                )))
            } else {
                None
            },
        )),
        audit_events: Arc::new(RwLock::new(thinking_system::AuditRing::new(512))),
        ingestion_tx: None, 
        last_analysis_id: Arc::new(RwLock::new(None)),
        canonical_tx: None,
        config: Some(system_cfg.clone()),
        recent_ingest: Arc::new(RwLock::new(std::collections::HashMap::new())),
        messaging_shutdown: None,
        scribe_runtime,
    };
    let _audit = thinking_system::AuditEmitter::new(state.audit_events.clone());
    
    let state = state;

    
    let canon_cfg = system_cfg
        .pipeline
        .as_ref()
        .and_then(|p| p.canonicalisation.clone());
    let mut state = state; 
    let _pipeline = thinking_system::ingestion::IngestionPipeline::init(&mut state, canon_cfg);

    let body_limit: usize = std::env::var("TS_BODY_LIMIT_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(64 * 1024);
    
    if let Some(tx) = messaging::spawn_messaging(state.clone()).await {
        state.messaging_shutdown = Some(tx);
    }
    
    let state_arc = Arc::new(state.clone());
    spawn_accept_loop(endpoint.clone(), state_arc.clone()).await;
    let app = build_router(state.clone(), body_limit);
    let addr: SocketAddr = std::env::var("TS_HTTP_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8080".into())
        .parse()
        .expect("valid TS_HTTP_ADDR");
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            warn!(error=%e, %addr, "bind failed, using ephemeral");
            tokio::net::TcpListener::bind("127.0.0.1:0").await?
        }
    };
    let local = listener.local_addr()?;
    info!(%local, "control plane listening");

    tokio::select! { _ = axum::serve(listener, app) => {} _ = tokio::signal::ctrl_c() => {} }
    
    if let Some(tx) = state.messaging_shutdown.take() {
        let _ = tx.send(()).await;
    }
    sleep(Duration::from_millis(50)).await;
    info!("thinking-system shutting down");
    Ok(())
}


