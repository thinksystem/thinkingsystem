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

use crate::blocks::registry::BlockRegistry;
use crate::blocks::rules::{BlockError, BlockResult, BlockType};
use crate::flows::core::{BlockDefinition, FlowDefinition};
use crate::flows::dynamic_executor::{DynamicExecutor, DynamicFunction};
use crate::flows::flowgorithm::FlowNavigator;
use crate::flows::flowgorithm::Flowgorithm;
use crate::flows::llm_prompt_service::LLMPromptService;
use crate::flows::security::{self, SecurityConfig};
use crate::flows::state::UnifiedState;
use crate::flows::Binder;
use crate::flows::PerformanceMetrics;
use crate::nlu::llm_processor::CustomLLMAdapter;
use crate::nlu::llm_processor::LLMAdapter;
use crate::nlu::orchestrator::ExtractedData;
use crate::nlu::query_processor::QueryProcessor;
use chrono::{DateTime, Utc};
use futures::stream::StreamExt;
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, span, warn, Level};
mod state_keys {
    pub const BLOCK_RESULT: &str = "block_result";
    pub const ERROR: &str = "error";
    pub const OVERRIDE_TARGET: &str = "override_target";
    pub const NAVIGATION_PRIORITY: &str = "navigation_priority";
    pub const AWAITING_INPUT: &str = "awaiting_input";
    pub const AWAIT_PROMPT: &str = "await_prompt";
    pub const AWAIT_OPTIONS: &str = "await_options";
    pub const AWAIT_STATE_KEY: &str = "await_state_key";
    pub const FLOW_TERMINATED: &str = "flow_terminated";
    pub const BLOCK_WEIGHT: &str = "block_weight";
}
#[derive(Debug, Clone)]
pub struct EngineMetrics {
    pub processing_time: Duration,
    pub blocks_processed: usize,
    pub memory_usage: usize,
    pub function_calls: HashMap<String, usize>,
    pub version_history: Vec<String>,
    pub last_reload: DateTime<Utc>,
}
pub struct UnifiedFlowEngine {
    registry: Arc<BlockRegistry>,
    query_processor: Arc<QueryProcessor>,
    llm_adapter: CustomLLMAdapter,
    navigator: Arc<Flowgorithm>,
    prompt_service: LLMPromptService,
    metrics: Arc<RwLock<EngineMetrics>>,
    dynamic_executor: Arc<RwLock<DynamicExecutor>>,
    dynamic_functions: Arc<RwLock<HashMap<String, DynamicFunction>>>,
    function_versions: Arc<RwLock<HashMap<String, Vec<DynamicFunction>>>>,
    hot_reload_enabled: bool,
    security_config: SecurityConfig,
    http_client: Client,
}
impl UnifiedFlowEngine {
    fn build_http_client(config: &SecurityConfig) -> Result<Client, reqwest::Error> {
        Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_seconds))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::limited(3))
            .user_agent("FlowEngine/1.0")
            .build()
    }
    pub fn new(
        registry: Arc<BlockRegistry>,
        query_processor: QueryProcessor,
        llm_adapter: CustomLLMAdapter,
        navigator: Flowgorithm,
        security_config: SecurityConfig,
    ) -> Self {
        let executor = DynamicExecutor::new().expect("Failed to initialise DynamicExecutor");
        let http_client =
            Self::build_http_client(&security_config).expect("Failed to build HTTP client");
        let prompt_service = LLMPromptService::new(registry.clone());

        Self {
            registry,
            query_processor: Arc::new(query_processor),
            llm_adapter,
            navigator: Arc::new(navigator),
            prompt_service,
            metrics: Arc::new(RwLock::new(EngineMetrics {
                processing_time: Duration::default(),
                blocks_processed: 0,
                memory_usage: 0,
                function_calls: HashMap::new(),
                version_history: Vec::new(),
                last_reload: Utc::now(),
            })),
            dynamic_executor: Arc::new(RwLock::new(executor)),
            dynamic_functions: Arc::new(RwLock::new(HashMap::new())),
            function_versions: Arc::new(RwLock::new(HashMap::new())),
            hot_reload_enabled: true,
            security_config,
            http_client,
        }
    }
    pub fn new_with_defaults(
        registry: Arc<BlockRegistry>,
        query_processor: QueryProcessor,
        llm_adapter: CustomLLMAdapter,
        navigator: Flowgorithm,
    ) -> Self {
        Self::new(
            registry,
            query_processor,
            llm_adapter,
            navigator,
            SecurityConfig::default(),
        )
    }
    pub fn update_security_config(&mut self, config: SecurityConfig) -> Result<(), BlockError> {
        self.http_client = Self::build_http_client(&config).map_err(|e| {
            BlockError::ProcessingError(format!("Failed to rebuild HTTP client: {e}"))
        })?;
        self.security_config = config;
        Ok(())
    }
    #[instrument(skip(self, batch), fields(batch_size = batch.len(), concurrency = concurrency_limit))]
    pub async fn process_flows_batch(
        &self,
        batch: Vec<(String, UnifiedState)>,
        concurrency_limit: usize,
    ) -> Vec<Result<(), BlockError>> {
        info!("Processing batch of flows.");
        futures::stream::iter(batch)
            .map(|(flow_id, mut state)| {
                let flow_processing_span =
                    span!(Level::INFO, "process_flow_in_batch", flow_id = %flow_id);
                async move {
                    let _enter = flow_processing_span.enter();
                    match timeout(
                        Duration::from_secs(30),
                        self.process_flow(&flow_id, &mut state),
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => {
                            error!("Flow execution timeout for flow_id: {}", flow_id);
                            Err(BlockError::ProcessingError(format!(
                                "Flow execution timeout for flow_id: {flow_id}"
                            )))
                        }
                    }
                }
            })
            .buffer_unordered(concurrency_limit)
            .collect()
            .await
    }
    #[instrument(skip(self, state), fields(flow_id = %flow_id, max_retries = max_retries))]
    pub async fn process_flow_with_retry(
        &self,
        flow_id: &str,
        state: &mut UnifiedState,
        max_retries: u32,
    ) -> Result<(), BlockError> {
        let mut attempts = 0;
        let mut last_error = None;
        while attempts < max_retries {
            info!("Attempt {} to process flow: {}", attempts + 1, flow_id);
            match self.process_flow(flow_id, state).await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    error!(
                        "Error processing flow {} on attempt {}: {:?}",
                        flow_id,
                        attempts + 1,
                        e
                    );
                    attempts += 1;
                    last_error = Some(e);
                    if attempts < max_retries {
                        let delay_secs = 2u64.pow(attempts.saturating_sub(1));
                        tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                        info!("Retrying flow {} after {}s delay.", flow_id, delay_secs);
                    }
                }
            }
        }
        error!("Max retries exceeded for flow: {}", flow_id);
        Err(last_error.unwrap_or_else(|| {
            BlockError::ProcessingError(format!("Max retries exceeded for flow: {flow_id}"))
        }))
    }
    #[instrument(skip(self, definition), fields(flow_id = %definition.id, num_blocks = definition.blocks.len()))]
    pub fn register_flow(&mut self, definition: FlowDefinition) -> Result<(), BlockError> {
        info!("Registering flow definition.");
        for block_def in &definition.blocks {
            debug!(
                "Creating block instance: {} of type {:?}",
                block_def.id, block_def.block_type
            );
            self.registry.create_block(
                block_def.block_type.clone(),
                block_def.id.clone(),
                block_def.properties.clone(),
            )?;
        }
        let binder = self.create_flow_binder(&definition)?;
        Arc::get_mut(&mut self.navigator)
            .ok_or_else(|| {
                error!("Failed to get mutable reference to navigator for registering binder.");
                BlockError::ProcessingError(
                    "Cannot modify navigator: Failed to get mutable reference.".into(),
                )
            })?
            .register_binder(definition.id.clone(), binder);
        info!("Flow {} registered successfully.", definition.id);
        Ok(())
    }
    #[instrument(skip(self, state), fields(flow_id = %flow_id))]
    pub async fn process_flow(
        &self,
        flow_id: &str,
        state: &mut UnifiedState,
    ) -> Result<(), BlockError> {
        let start_time = Instant::now();
        info!("Starting processing of flow.");
        let binder = self.navigator.get_binder(flow_id).ok_or_else(|| {
            error!("Flow binder not found for flow_id: {}", flow_id);
            BlockError::BlockNotFound(format!("Flow binder not found for flow_id: {flow_id}"))
        })?;
        state.set_binder(binder.clone());
        state.flow_id = Some(flow_id.to_string());
        let mut blocks_processed_count = 0;
        let mut current_block_id_opt = state
            .block_id
            .clone()
            .or_else(|| Some(binder.get_start_block().to_string()));
        while let Some(current_block_id) = current_block_id_opt.take() {
            state.block_id = Some(current_block_id.clone());
            let block_span =
                span!(Level::DEBUG, "process_block_in_flow", block_id = %current_block_id);
            let _enter_block_span = block_span.enter();
            debug!("Processing block.");
            state.data.remove(state_keys::AWAITING_INPUT);
            state.data.remove(state_keys::FLOW_TERMINATED);
            let processed_block_id = current_block_id.clone();
            if let Err(e) = self.handle_block(&processed_block_id, state).await {
                error!("Error handling block {}: {:?}", processed_block_id, e);
                return Err(e);
            }
            blocks_processed_count += 1;
            if state
                .data
                .get(state_keys::FLOW_TERMINATED)
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                info!(
                    "Flow {} terminated by block {}.",
                    flow_id, processed_block_id
                );
                state.clear_flow_data();
                break;
            }
            if state
                .data
                .get(state_keys::AWAITING_INPUT)
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                info!(
                    "Flow {} is awaiting input after block {}.",
                    flow_id, processed_block_id
                );
                break;
            }
            let next_block_from_state = state.block_id.clone();
            if next_block_from_state.as_ref() != Some(&processed_block_id)
                && next_block_from_state.is_some()
            {
                current_block_id_opt = next_block_from_state;
                debug!(
                    "Next block determined by block {} result: {:?}",
                    processed_block_id, current_block_id_opt
                );
            } else if let Some(navigator_next_block) =
                self.navigator.get_next_block(&processed_block_id, None)
            {
                current_block_id_opt = Some(navigator_next_block);
                state.block_id = current_block_id_opt.clone();
                debug!(
                    "Next block determined by navigator from {}: {:?}",
                    processed_block_id, current_block_id_opt
                );
            } else {
                info!(
                    "Flow {} completed. No next block after {}.",
                    flow_id, processed_block_id
                );
                state.clear_flow_data();
                break;
            }
        }
        let processing_time = start_time.elapsed();
        debug!("Flow {} processing took: {:?}", flow_id, processing_time);
        if let Ok(mut metrics) = self.metrics.write() {
            metrics.processing_time += processing_time;
            metrics.blocks_processed += blocks_processed_count;
        } else {
            error!("Failed to acquire lock for updating metrics.");
        }
        Ok(())
    }
    pub fn get_metrics(&self) -> Result<EngineMetrics, BlockError> {
        self.metrics.read().map(|guard| guard.clone()).map_err(|e| {
            error!("Failed to read metrics due to lock poison: {:?}", e);
            BlockError::LockError
        })
    }
    #[instrument(skip(self, args), fields(function_name = %function_name))]
    async fn execute_dynamic_function(
        &self,
        function_name: &str,
        args: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value, BlockError> {
        debug!("Executing dynamic function: {}", function_name);
        let function = self
            .dynamic_functions
            .read()
            .map_err(|_| {
                error!("Failed to acquire read lock on dynamic_functions");
                BlockError::LockError
            })?
            .get(function_name)
            .cloned()
            .ok_or_else(|| {
                error!("Dynamic function {} not found", function_name);
                BlockError::BlockNotFound(format!("Dynamic function not found: {function_name}"))
            })?;
        let start_time = Instant::now();
        let result = function.execute(&args)?;
        let execution_time = start_time.elapsed();
        debug!(
            "Function {} executed in {:?}",
            function_name, execution_time
        );
        if let Ok(mut metrics) = self.metrics.write() {
            *metrics
                .function_calls
                .entry(function_name.to_string())
                .or_insert(0) += 1;
        } else {
            error!(
                "Failed to update function call metrics for {}",
                function_name
            );
        }
        Ok(result)
    }
    #[instrument(skip(self, state), fields(block_id = %block_id))]
    async fn handle_block(
        &self,
        block_id: &str,
        state: &mut UnifiedState,
    ) -> Result<(), BlockError> {
        debug!("Executing block.");
        let block = self.registry.get_block(block_id)?;
        if let Some(binder) = &state.binder {
            if let Some(weight) = binder.get_weight(block_id) {
                state.set_data(state_keys::BLOCK_WEIGHT.to_string(), Value::from(weight));
            }
        }
        match block.process(&mut state.data).await? {
            BlockResult::Success(value) => {
                debug!("Block {} succeeded.", block_id);
                state.set_data(state_keys::BLOCK_RESULT.to_string(), value);
                Ok(())
            }
            BlockResult::Failure(error_msg) => {
                error!("Block {} failed: {}", block_id, error_msg);
                state.set_data(
                    state_keys::ERROR.to_string(),
                    Value::String(error_msg.clone()),
                );
                Err(BlockError::ProcessingError(error_msg))
            }
            BlockResult::Navigate {
                target,
                priority,
                is_override,
            } => {
                debug!(
                    "Block {} requests navigation to {} (priority: {}, override: {}).",
                    block_id, target, priority, is_override
                );
                if is_override {
                    state.set_data(
                        state_keys::OVERRIDE_TARGET.to_string(),
                        Value::String(target.clone()),
                    );
                }
                state.set_data(
                    state_keys::NAVIGATION_PRIORITY.to_string(),
                    Value::Number(priority.into()),
                );
                state.block_id = Some(target);
                Ok(())
            }
            BlockResult::FetchExternalData {
                url,
                data_path,
                output_key,
                next_block,
                priority,
                is_override,
            } => {
                debug!("Block {} requests external data from {}.", block_id, url);
                self.execute_external_fetch(state, url, data_path, output_key)
                    .await?;
                if is_override {
                    state.set_data(
                        state_keys::OVERRIDE_TARGET.to_string(),
                        Value::String(next_block.clone()),
                    );
                }
                state.set_data(
                    state_keys::NAVIGATION_PRIORITY.to_string(),
                    Value::Number(priority.into()),
                );
                state.block_id = Some(next_block);
                Ok(())
            }
            BlockResult::FetchExternalDataEnhanced {
                url,
                data_path,
                output_key,
                next_block,
                priority,
                is_override,
                enable_path_discovery,
            } => {
                debug!(
                    "Block {} requests enhanced external data from {} with path discovery: {}.",
                    block_id, url, enable_path_discovery
                );

                self.execute_external_fetch(state, url, data_path, output_key)
                    .await?;
                if is_override {
                    state.set_data(
                        state_keys::OVERRIDE_TARGET.to_string(),
                        Value::String(next_block.clone()),
                    );
                }
                state.set_data(
                    state_keys::NAVIGATION_PRIORITY.to_string(),
                    Value::Number(priority.into()),
                );
                state.block_id = Some(next_block);
                Ok(())
            }
            BlockResult::ApiResponseAnalysis {
                url,
                response_structure,
                discovered_paths,
                suggested_path,
                original_path,
                next_block,
            } => {
                debug!(
                    "Block {} provides API response analysis for {}",
                    block_id, url
                );
                state.set_data("api_response_structure".to_string(), response_structure);
                state.set_data(
                    "discovered_paths".to_string(),
                    Value::Array(discovered_paths.into_iter().map(Value::String).collect()),
                );
                if let Some(path) = suggested_path {
                    state.set_data("suggested_path".to_string(), Value::String(path));
                }
                state.set_data("original_path".to_string(), Value::String(original_path));
                state.set_data("analysis_url".to_string(), Value::String(url));
                state.block_id = Some(next_block);
                Ok(())
            }
            BlockResult::ExecuteFunction {
                function_name,
                args,
                output_key,
                next_block,
                priority,
                is_override,
            } => {
                debug!(
                    "Block {} requests function execution: {}",
                    block_id, function_name
                );
                let result = self.execute_dynamic_function(&function_name, args).await?;
                state.set_data(output_key, result);
                state.set_data(
                    "navigation_type".to_string(),
                    Value::String("function_execution".to_string()),
                );
                if is_override {
                    state.set_data(
                        state_keys::OVERRIDE_TARGET.to_string(),
                        Value::String(next_block.clone()),
                    );
                }
                state.set_data(
                    state_keys::NAVIGATION_PRIORITY.to_string(),
                    Value::Number(priority.into()),
                );
                state.block_id = Some(next_block);
                Ok(())
            }
            BlockResult::AwaitInput { prompt, state_key } => {
                debug!(
                    "Block {} is awaiting input (prompt: '{}', state_key: '{}').",
                    block_id, prompt, state_key
                );
                state.set_data(state_keys::AWAIT_PROMPT.to_string(), Value::String(prompt));
                state.set_data(
                    state_keys::AWAIT_STATE_KEY.to_string(),
                    Value::String(state_key),
                );
                state.set_data(state_keys::AWAITING_INPUT.to_string(), Value::Bool(true));
                Ok(())
            }
            BlockResult::AwaitChoice {
                question,
                options,
                state_key,
            } => {
                debug!(
                    "Block {} is awaiting choice (question: '{}', state_key: '{}').",
                    block_id, question, state_key
                );
                state.set_data(
                    state_keys::AWAIT_PROMPT.to_string(),
                    Value::String(question),
                );
                state.set_data(state_keys::AWAIT_OPTIONS.to_string(), Value::Array(options));
                state.set_data(
                    state_keys::AWAIT_STATE_KEY.to_string(),
                    Value::String(state_key),
                );
                state.set_data(state_keys::AWAITING_INPUT.to_string(), Value::Bool(true));
                Ok(())
            }
            BlockResult::Move(target) => {
                debug!("Block {} requests move to {}.", block_id, target);
                state.block_id = Some(target);
                Ok(())
            }
            BlockResult::Terminate => {
                debug!("Block {} requests flow termination.", block_id);
                state.set_data(state_keys::FLOW_TERMINATED.to_string(), Value::Bool(true));
                Ok(())
            }
        }
    }
    #[instrument(skip(self, state), fields(url = %url, data_path = %data_path))]
    async fn execute_external_fetch(
        &self,
        state: &mut UnifiedState,
        url: String,
        data_path: String,
        output_key: String,
    ) -> Result<(), BlockError> {
        let validated_url = security::validate_url(&url, &self.security_config)?;
        let response = self
            .http_client
            .get(validated_url)
            .send()
            .await
            .map_err(|e| {
                BlockError::ApiRequestError(format!("API request to {url} failed: {e}"))
            })?;
        let status = response.status();
        if !status.is_success() {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "No body".to_string());
            return Err(BlockError::ApiRequestError(format!(
                "API request to {url} returned status {status}: {error_body}"
            )));
        }
        let json_data: serde_json::Value = response.json().await.map_err(|e| {
            BlockError::JsonParseError(format!("Failed to parse JSON response from {url}: {e}"))
        })?;

        state.set_data("raw_json_response".to_string(), json_data.clone());
        state.set_data(
            "requested_data_path".to_string(),
            Value::String(data_path.clone()),
        );
        state.set_data("api_url".to_string(), Value::String(url.clone()));

        let data_to_insert = json_data.pointer(&data_path).unwrap_or(&json_data).clone();

        state.set_data(output_key, data_to_insert);
        state.set_data(
            "navigation_type".to_string(),
            Value::String("external_data".to_string()),
        );
        debug!(
            "Successfully fetched external data from {} and stored in state",
            url
        );
        Ok(())
    }
    #[instrument(skip(self, flow), fields(flow_id = %flow.id))]
    fn create_flow_binder(&self, flow: &FlowDefinition) -> Result<Binder, BlockError> {
        debug!("Creating flow binder.");
        let mut binder = Binder::default();

        binder.set_start_block(flow.start_block_id.clone());

        for block_def in &flow.blocks {
            if let Some(next_block_val) = block_def.properties.get("next_block") {
                if let Some(next_block_str) = next_block_val.as_str() {
                    binder.add_connection(block_def.id.clone(), next_block_str.to_string());
                }
            }
            if let Some(weight_val) = block_def.properties.get("weight") {
                if let Some(weight_f64) = weight_val.as_f64() {
                    binder.add_weight(block_def.id.clone(), weight_f64);
                }
            }
            if let Some(metadata_val) = block_def.properties.get("metadata") {
                binder.add_metadata(block_def.id.clone(), metadata_val.clone());
            }
        }
        Ok(binder)
    }
    #[instrument(skip(self, instruction), fields(instruction_len = instruction.len()))]
    pub async fn process_instruction(
        &self,
        instruction: &str,
    ) -> Result<FlowDefinition, BlockError> {
        info!("Processing natural language instruction to generate flow definition.");
        let nlu_result = self
            .query_processor
            .process_instruction(instruction)
            .await
            .map_err(|e| {
                error!("NLU processing via QueryProcessor failed: {:?}", e);
                BlockError::ProcessingError(format!("NLU processing failed: {e}"))
            })?;
        let extracted_data = nlu_result.extracted_data;
        let flow_logic = self.generate_flow_logic(&extracted_data).await?;
        self.create_flow_definition(flow_logic).await
    }
    #[instrument(skip(self, data))]
    async fn generate_flow_logic(&self, data: &ExtractedData) -> Result<Value, BlockError> {
        debug!("Generating flow logic from extracted data using dynamic prompt service.");

        let prompt = self.prompt_service.generate_flow_logic_prompt(data)?;

        let logic_json = self.llm_adapter.process_text(&prompt).await.map_err(|e| {
            error!("LLM processing failed: {:?}", e);
            BlockError::ProcessingError(format!("LLM processing failed: {e}"))
        })?;
        serde_json::from_str(&logic_json).or_else(|first_err| {
            if let Some(start) = logic_json.find('{') {
                if let Some(end) = logic_json.rfind('}') {
                    let json_substr = &logic_json[start..=end];
                    serde_json::from_str(json_substr).map_err(|second_err| {
                        error!(
                            "Failed to parse extracted JSON from LLM response: {}",
                            second_err
                        );
                        BlockError::ProcessingError(format!(
                            "Invalid JSON from LLM (after extraction): {second_err}"
                        ))
                    })
                } else {
                    error!(
                        "Failed to parse JSON from LLM response (no closing brace): {}",
                        first_err
                    );
                    Err(BlockError::ProcessingError(format!(
                        "Invalid JSON from LLM: {first_err}"
                    )))
                }
            } else {
                error!("No JSON object found in LLM response: {}", first_err);
                Err(BlockError::ProcessingError(format!(
                    "No JSON found in LLM response: {first_err}"
                )))
            }
        })
    }
    #[instrument(skip(self, flow_logic))]
    async fn create_flow_definition(
        &self,
        flow_logic: Value,
    ) -> Result<FlowDefinition, BlockError> {
        debug!("Creating flow definition from parsed logic.");
        let blocks = flow_logic["blocks"]
            .as_array()
            .ok_or_else(|| {
                error!("Invalid or missing 'blocks' array in flow logic.");
                BlockError::ProcessingError("Invalid blocks array in flow logic".into())
            })?
            .iter()
            .enumerate()
            .map(|(index, block_data)| {
                let block_type = self.parse_block_type(block_data).map_err(|e| {
                    error!("Failed to parse block type at index {}: {:?}", index, e);
                    e
                })?;
                Ok(BlockDefinition {
                    id: block_data["id"]
                        .as_str()
                        .ok_or_else(|| {
                            BlockError::ProcessingError(format!(
                                "Missing 'id' in block at index {index}"
                            ))
                        })?
                        .to_string(),
                    block_type,
                    properties: block_data["properties"]
                        .as_object()
                        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                        .unwrap_or_default(),
                })
            })
            .collect::<Result<Vec<BlockDefinition>, BlockError>>()?;
        let flow_def = FlowDefinition {
            id: flow_logic["id"]
                .as_str()
                .ok_or_else(|| BlockError::ProcessingError("Missing 'id' in flow logic".into()))?
                .to_string(),
            name: flow_logic["name"]
                .as_str()
                .ok_or_else(|| BlockError::ProcessingError("Missing 'name' in flow logic".into()))?
                .to_string(),
            start_block_id: flow_logic["start_block_id"]
                .as_str()
                .ok_or_else(|| {
                    BlockError::ProcessingError("Missing 'start_block_id' in flow logic".into())
                })?
                .to_string(),
            blocks,
        };
        debug!(
            "Flow definition created successfully with {} blocks.",
            flow_def.blocks.len()
        );
        Ok(flow_def)
    }
    fn parse_block_type(&self, block_data: &Value) -> Result<BlockType, BlockError> {
        let type_str = block_data["type"]
            .as_str()
            .ok_or_else(|| BlockError::ProcessingError("Missing 'type' in block data".into()))?;
        let block_id = block_data["id"]
            .as_str()
            .ok_or_else(|| BlockError::ProcessingError("Missing 'id' in block data".into()))?
            .to_string();
        match type_str.to_lowercase().as_str() {
            "conditional" => Ok(BlockType::Conditional),
            "decision" => Ok(BlockType::Decision),
            "display" => Ok(BlockType::Display),
            "externaldata" | "external_data" => Ok(BlockType::ExternalData),
            "goto" => Ok(BlockType::GoTo),
            "input" => Ok(BlockType::Input),
            "interactive" => Ok(BlockType::Interactive),
            "random" => Ok(BlockType::Random),
            "dynamicfunction" | "dynamic_function" => {
                let function_name = block_data["properties"]["function_name"]
                    .as_str()
                    .or_else(|| block_data["function_name"].as_str())
                    .ok_or_else(|| {
                        BlockError::ProcessingError(
                            "Missing 'function_name' for DynamicFunction block".into(),
                        )
                    })?
                    .to_string();
                Ok(BlockType::DynamicFunction(block_id, function_name))
            }
            unsupported_type => {
                error!("Unsupported block type: {}", unsupported_type);
                Err(BlockError::UnsupportedBlockType(
                    unsupported_type.to_string(),
                ))
            }
        }
    }
    #[instrument(skip(self, rust_code, metadata), fields(function_id = %id, has_version = version.is_some()))]
    pub async fn register_dynamic_function(
        &self,
        id: String,
        rust_code: String,
        signature: String,
        metadata: HashMap<String, Value>,
        version: Option<String>,
    ) -> Result<(), BlockError> {
        info!("Registering dynamic function.");
        let mut dynamic_fn = self
            .dynamic_executor
            .write()
            .map_err(|_| {
                error!("Failed to acquire write lock on dynamic_executor for compile");
                BlockError::LockError
            })?
            .compile_function(&rust_code, &signature)
            .map_err(|e| {
                error!("Failed to compile dynamic function {}: {:?}", id, e);
                e
            })?;
        dynamic_fn.metadata.extend(metadata);
        let version_id = version.unwrap_or_else(|| format!("v{}", Utc::now().timestamp()));
        if let Ok(mut versions) = self.function_versions.write() {
            versions
                .entry(id.clone())
                .or_insert_with(Vec::new)
                .push(dynamic_fn.clone());
        } else {
            error!("Failed to acquire lock for function versions.");
            return Err(BlockError::LockError);
        }
        if let Ok(mut functions) = self.dynamic_functions.write() {
            functions.insert(id.clone(), dynamic_fn);
        } else {
            error!("Failed to acquire lock for dynamic functions.");
            return Err(BlockError::LockError);
        }
        if let Ok(mut metrics) = self.metrics.write() {
            metrics.version_history.push(version_id);
            metrics.last_reload = Utc::now();
        } else {
            error!("Failed to acquire lock for metrics update.");
        }
        info!("Dynamic function {} registered successfully.", id);
        Ok(())
    }
    #[instrument(skip(self, new_code), fields(function_id = %id, hot_reload_enabled = self.hot_reload_enabled))]
    pub async fn hot_reload_function(
        &self,
        id: &str,
        new_code: &str,
        signature: &str,
    ) -> Result<(), BlockError> {
        if !self.hot_reload_enabled {
            error!("Hot reload is disabled.");
            return Err(BlockError::ProcessingError("Hot reload disabled".into()));
        }
        info!("Hot reloading function: {}", id);
        let new_fn = self
            .dynamic_executor
            .write()
            .map_err(|_| {
                error!("Failed to acquire write lock on dynamic_executor for hot reload");
                BlockError::LockError
            })?
            .compile_function(new_code, signature)
            .map_err(|e| {
                error!(
                    "Failed to compile function {} during hot reload: {:?}",
                    id, e
                );
                e
            })?;
        if let Ok(mut functions) = self.dynamic_functions.write() {
            functions.insert(id.to_string(), new_fn.clone());
        } else {
            error!("Failed to acquire lock for dynamic functions during hot reload.");
            return Err(BlockError::LockError);
        }
        if let Ok(mut versions) = self.function_versions.write() {
            versions
                .entry(id.to_string())
                .or_insert_with(Vec::new)
                .push(new_fn);
        } else {
            error!("Failed to acquire lock for function versions during hot reload.");
        }
        info!("Function {} hot reloaded successfully.", id);
        Ok(())
    }
    pub async fn get_function_metrics(&self, function_id: &str) -> Option<PerformanceMetrics> {
        self.dynamic_functions
            .read()
            .ok()?
            .get(function_id)
            .and_then(|f| f.performance_metrics.read().ok())
            .map(|metrics| metrics.clone())
    }
    pub fn get_function_versions(&self, function_id: &str) -> Option<Vec<DynamicFunction>> {
        self.function_versions
            .read()
            .ok()?
            .get(function_id)
            .cloned()
    }
    #[instrument(skip(self, initial_input), fields(chain_length = chain.len()))]
    pub async fn execute_function_chain(
        &self,
        chain: Vec<String>,
        initial_input: Value,
    ) -> Result<Value, BlockError> {
        info!("Executing function chain.");
        let mut current_value = initial_input;
        for function_id in chain {
            debug!("Executing function in chain: {}", function_id);
            if let Some(function) = self
                .dynamic_functions
                .read()
                .map_err(|_| BlockError::LockError)?
                .get(&function_id)
                .cloned()
            {
                let start_time = Instant::now();
                current_value = function.execute(&[current_value.clone()])?;
                let execution_time = start_time.elapsed();
                debug!("Function {} executed in {:?}", function_id, execution_time);
                if let Ok(mut metrics) = self.metrics.write() {
                    *metrics
                        .function_calls
                        .entry(function_id.clone())
                        .or_insert(0) += 1;
                } else {
                    error!("Failed to update function call metrics for {}", function_id);
                }
            } else {
                error!("Function {} not found in chain execution", function_id);
                return Err(BlockError::BlockNotFound(format!(
                    "Dynamic function not found: {function_id}"
                )));
            }
        }
        debug!("Function chain execution completed.");
        Ok(current_value)
    }
    pub fn enable_hot_reload(&mut self) {
        info!("Hot reload enabled.");
        self.hot_reload_enabled = true;
    }
    pub fn disable_hot_reload(&mut self) {
        info!("Hot reload disabled.");
        self.hot_reload_enabled = false;
    }
    pub fn is_hot_reload_enabled(&self) -> bool {
        self.hot_reload_enabled
    }
    pub fn clear_function_versions(&self) -> Result<(), BlockError> {
        info!("Clearing function versions.");
        if let Ok(mut versions) = self.function_versions.write() {
            versions.clear();
            Ok(())
        } else {
            error!("Failed to acquire lock for clearing function versions.");
            Err(BlockError::LockError)
        }
    }
    pub fn clear_dynamic_functions(&self) -> Result<(), BlockError> {
        info!("Clearing dynamic functions.");
        if let Ok(mut functions) = self.dynamic_functions.write() {
            functions.clear();
            Ok(())
        } else {
            error!("Failed to acquire lock for clearing dynamic functions.");
            Err(BlockError::LockError)
        }
    }
}
