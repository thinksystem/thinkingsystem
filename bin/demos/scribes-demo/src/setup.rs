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

use crate::cli::LogLevel;
use crate::demo_processor::DemoDataProcessor;
use crate::llm_logging::LLMLogger;
use crate::local_llm_interface::LocalLLMInterface;
use crate::logging_adapter::LoggingLLMAdapter;
use chrono::Utc;
use std::sync::Arc;
use steel::IdentityProvider;
use stele::llm::unified_adapter::UnifiedLLMAdapter;
use stele::nlu::llm_processor::LLMAdapter;
use surrealdb::engine::any::connect;
use tokio::sync::Mutex;
use tracing::info;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

pub struct SystemConfig {
    pub session_id: String,
    pub llm_logger: Arc<LLMLogger>,
    pub llm_adapter: Arc<UnifiedLLMAdapter>,
    pub logging_adapter: Arc<LoggingLLMAdapter>,
    pub iam_provider: Arc<IdentityProvider>,
    pub local_llm_interface: Arc<Mutex<LocalLLMInterface>>,
    pub enhanced_data_processor: Arc<DemoDataProcessor>,
}

pub fn setup_logging(
    log_level: Option<LogLevel>,
    trace: bool,
) -> Result<tracing_appender::non_blocking::WorkerGuard, Box<dyn std::error::Error>> {
    let log_level_str = if let Some(level) = log_level {
        level.as_str()
    } else if trace {
        "trace"
    } else {
        "debug"
    };

    info!("Setting log level to: {}", log_level_str);

    let file_appender = RollingFileAppender::new(Rotation::DAILY, "logs", "scribes-demo.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter_stdout =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level_str));
    let env_filter_file =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level_str));

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .with_target(true)
                .with_thread_ids(true)
                .with_filter(env_filter_stdout),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_target(true)
                .with_thread_ids(true)
                .with_ansi(false)
                .with_filter(env_filter_file),
        )
        .init();

    Ok(guard)
}

pub async fn initialise_system() -> Result<SystemConfig, Box<dyn std::error::Error>> {
    info!("=== Starting Enhanced STELE Scribes System Demo ===");
    let session_id = format!("session_{}", Utc::now().format("%Y%m%d_%H%M%S"));
    info!(session_id = %session_id, "Demo session started");

    let llm_logger = Arc::new(LLMLogger::new(
        "logs/llm_interactions.jsonl",
        session_id.clone(),
        true,
    ));

    info!("--- Initialising Enhanced LLM Adapter with Dynamic Model Selection ---");

    let llm_adapter = Arc::new(
        UnifiedLLMAdapter::with_defaults()
            .await
            .map_err(|e| format!("Failed to initialise unified LLM adapter: {e}"))?,
    );
    info!("Unified LLM Adapter initialised with dynamic model selection from llm_models.yml configuration.");

    let logging_adapter = match LoggingLLMAdapter::ollama(Arc::clone(&llm_logger)) {
        Ok(adapter) => {
            info!("Successfully initialised Ollama LoggingLLMAdapter for compatibility");
            Arc::new(adapter)
        }
        Err(e) => {
            tracing::warn!(
                "Failed to initialise Ollama LoggingLLMAdapter: {}, trying Anthropic",
                e
            );
            match LoggingLLMAdapter::anthropic(Arc::clone(&llm_logger)) {
                Ok(adapter) => {
                    info!("Fallback: Using Anthropic LoggingLLMAdapter for compatibility");
                    Arc::new(adapter)
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to initialise Anthropic LoggingLLMAdapter: {}, trying OpenAI",
                        e
                    );
                    match LoggingLLMAdapter::openai(Arc::clone(&llm_logger)) {
                        Ok(adapter) => {
                            info!("Fallback: Using OpenAI LoggingLLMAdapter for compatibility");
                            Arc::new(adapter)
                        }
                        Err(e2) => {
                            tracing::error!(
                                "Failed to initialise any LoggingLLMAdapter: Ollama: {}, Anthropic: {}, OpenAI: {}",
                                e,
                                e,
                                e2
                            );
                            return Err(format!(
                                "No LLM providers available: Ollama: {e}, Anthropic: {e}, OpenAI: {e2}"
                            )
                            .into());
                        }
                    }
                }
            }
        }
    };

    info!("--- Initialising Enhanced Database ---");
    let db = connect("mem://").await?;
    db.use_ns("enhanced_demo").use_db("scribes").await?;
    info!("Successfully initialised SurrealDB with enhanced namespace");

    info!("--- Initialising Enhanced IAM Provider ---");
    let iam_provider = Arc::new(IdentityProvider::new().await?);

    info!("--- Initialising Local LLM Interface ---");

    let local_llm_interface =
        Arc::new(Mutex::new(LocalLLMInterface::new(Arc::clone(&llm_adapter))));

    info!("--- Testing Local LLM Connection ---");
    match llm_adapter
        .process_text("Is the local LLM running and responsive? Respond with a short confirmation.")
        .await
    {
        Ok(response) => info!("Local LLM test response: {}", response),
        Err(e) => {
            tracing::warn!(
                "Unified LLM test failed: {}, trying LocalLLMInterface fallback",
                e
            );

            match local_llm_interface
                .lock()
                .await
                .query(
                    "Is the local LLM running and responsive? Respond with a short confirmation.",
                )
                .await
            {
                Ok(response) => info!("LocalLLMInterface test response: {}", response),
                Err(e) => tracing::error!("All LLM tests failed: {}", e),
            }
        }
    }
    info!("--- End of Local LLM Connection Test ---");

    info!("--- Initialising Enhanced Processors ---");
    let enhanced_data_processor = Arc::new(
        DemoDataProcessor::new(
            Arc::clone(&llm_adapter),
            Some(Arc::clone(&logging_adapter)),
            Arc::clone(&local_llm_interface),
            Arc::new(db.clone()),
            Arc::clone(&llm_logger),
        )
        .await?,
    );

    info!("All enhanced systems initialised successfully");

    Ok(SystemConfig {
        session_id,
        llm_logger,
        llm_adapter,
        logging_adapter,
        iam_provider,
        local_llm_interface,
        enhanced_data_processor,
    })
}
