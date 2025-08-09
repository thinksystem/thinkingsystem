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

use super::schema_provider::SchemaProvider;
use super::validation_service::UnifiedValidator;
use crate::config::ConfigLoader;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use stele::{
    blocks::registry::BlockRegistry,
    blocks::rules::BlockType,
    database::{
        connection::DatabaseConnection, dynamic_storage::DynamicStorage, types::DatabaseCommand,
    },
    flows::core::{BlockDefinition, FlowDefinition},
    flows::engine::UnifiedFlowEngine,
    flows::flowgorithm::Flowgorithm,
    flows::security::SecurityConfig,
    nlu::orchestrator::NLUOrchestrator,
    nlu::query_processor::QueryProcessor,
};
use tokio::sync::RwLock;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, warn};
use uuid;

pub struct CoreFlowOrchestrator {
    registered_flows: HashMap<String, FlowDefinition>,
    schema_provider: SchemaProvider,
    validator: UnifiedValidator,
    registry: Arc<BlockRegistry>,
    config_loader: ConfigLoader,
    engine: UnifiedFlowEngine,
}

impl CoreFlowOrchestrator {
    fn json_to_hashmap(value: serde_json::Value) -> HashMap<String, serde_json::Value> {
        match value {
            serde_json::Value::Object(map) => map.into_iter().collect(),
            _ => HashMap::new(),
        }
    }

    pub async fn new(config_loader: ConfigLoader) -> Result<Self> {
        let registry = Arc::new(BlockRegistry::with_standard_blocks()?);

        let (command_tx, command_rx) = mpsc::channel(32);
        let (client_tx, mut client_rx) = mpsc::channel(1);

        let mut db_conn = DatabaseConnection::new(command_rx);

        tokio::spawn(async move {
            if let Err(e) = db_conn.run().await {
                error!("Database connection handler error: {}", e);
            }
        });

        let (connect_response_tx, connect_response_rx) = oneshot::channel();
        command_tx
            .send(DatabaseCommand::Connect {
                client_sender: client_tx,
                response_sender: connect_response_tx,
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send connect command: {}", e))?;

        connect_response_rx
            .await
            .map_err(|e| anyhow::anyhow!("Failed to receive connect response: {}", e))?
            .map_err(|e| anyhow::anyhow!("Database connection failed: {}", e))?;

        let database_client = client_rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("Failed to receive database client from channel"))?;

        let config_path = "crates/stele/src/nlu/config";
        let orchestrator = Arc::new(RwLock::new(
            NLUOrchestrator::new(config_path)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create NLU orchestrator: {}", e))?,
        ));

        let storage = Arc::new(DynamicStorage::new(database_client.clone()));

        let query_processor_config_path = "crates/stele/src/nlu/config/query_processor.toml";
        let query_processor = QueryProcessor::new(
            orchestrator.clone(),
            storage.clone(),
            query_processor_config_path,
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create QueryProcessor: {}", e))?;

        let navigator = Flowgorithm::new();
        let security_config = SecurityConfig::default();

        let llm_adapter =
            stele::nlu::llm_processor::CustomLLMAdapter::new("llama3.2".to_string(), 4096, 0.7);

        let engine = UnifiedFlowEngine::new(
            registry.clone(),
            query_processor,
            llm_adapter,
            navigator,
            security_config,
        );

        let schema_provider = SchemaProvider::new(registry.clone(), config_loader.clone());
        let validator = UnifiedValidator::new(registry.clone());

        let registered_flows = HashMap::new();

        Ok(Self {
            registered_flows,
            schema_provider,
            validator,
            registry,
            config_loader,
            engine,
        })
    }

    pub fn show_system_info(&self) -> Result<()> {
        Ok(())
    }

    pub async fn execute_flow_task(&mut self, task: &str, _iteration: usize) -> Result<Value> {
        let flow_definition = self.select_appropriate_flow(task)?;

        let validation_result = self.validator.validate(&flow_definition);
        if !validation_result.is_valid {
            error!("Flow validation failed: {:?}", validation_result.errors);
            return Err(anyhow::anyhow!("Flow validation failed"));
        }

        let execution_result = self
            .execute_flow_with_engine(&flow_definition, task)
            .await?;

        Ok(execution_result)
    }

    pub async fn execute_api_demo(
        &mut self,
        api_type: &str,
        name: &str,
        _iteration: usize,
    ) -> Result<Value> {
        let flow_definition = match api_type {
            "nationality" => self.create_clean_nationality_flow(name)?,
            "weather" => self.create_clean_weather_flow(name)?,
            _ => return Err(anyhow::anyhow!("Unknown API type: {}", api_type)),
        };

        let validation_result = self.validator.validate(&flow_definition);
        if !validation_result.is_valid {
            error!("Flow validation failed: {:?}", validation_result.errors);
            return Err(anyhow::anyhow!("Flow validation failed"));
        }

        let execution_result = self
            .execute_flow_with_engine(&flow_definition, name)
            .await?;

        Ok(execution_result)
    }

    pub async fn execute_flow_with_engine(
        &mut self,
        flow: &FlowDefinition,
        task: &str,
    ) -> Result<Value> {
        let mut state = stele::flows::state::UnifiedState::new(
            "demo_user".to_string(),
            "demo_operator".to_string(),
            "demo_channel".to_string(),
        );
        state.flow_id = Some(flow.id.clone());
        state.set_data(
            "task".to_string(),
            serde_json::Value::String(task.to_string()),
        );

        self.engine.register_flow(flow.clone())?;

        match self.engine.process_flow(&flow.id, &mut state).await {
            Ok(_) => Ok(json!({
                "status": "success",
                "task": task,
                "flow_id": flow.id,
                "flow_name": flow.name,
                "execution_data": state.data
            })),
            Err(e) => {
                error!("Flow execution failed: {}", e);
                Err(anyhow::anyhow!("Flow execution failed: {}", e))
            }
        }
    }

    fn select_appropriate_flow(&self, task: &str) -> Result<FlowDefinition> {
        let task_lower = task.to_lowercase();

        if task_lower.contains("nationality") || task_lower.contains("nationalise") {
            self.create_nationality_api_flow(task)
        } else if task_lower.contains("weather") {
            self.create_weather_api_flow(task)
        } else {
            self.create_nationality_api_flow(task)
        }
    }

    fn create_nationality_api_flow(&self, task: &str) -> Result<FlowDefinition> {
        let flow_id = format!("nationality_api_flow_{}", uuid::Uuid::new_v4().simple());
        Ok(FlowDefinition {
            id: flow_id,
            name: format!("Nationality Prediction Flow: {task}"),
            start_block_id: "api_intro".to_string(),
            blocks: vec![
                BlockDefinition {
                    id: "api_intro".to_string(),
                    block_type: BlockType::Display,
                    properties: Self::json_to_hashmap(json!({
                        "message": "Fetching nationality prediction for name: kim jong moon",
                        "next_block": "fetch_nationality"
                    })),
                },
                BlockDefinition {
                    id: "fetch_nationality".to_string(),
                    block_type: BlockType::ExternalData,
                    properties: Self::json_to_hashmap(json!({
                        "api_url": "https://api.nationalise.io?name=kim%20jong%20moon",
                        "data_path": "/country/0/country_id",
                        "output_key": "predicted_country",
                        "next_block": "process_data"
                    })),
                },
                BlockDefinition {
                    id: "process_data".to_string(),
                    block_type: BlockType::Compute,
                    properties: Self::json_to_hashmap(json!({
                        "expression": "concat('Predicted nationality for kim jong moon: ', {{predicted_country}})",
                        "result_variable": "processed_result",
                        "next_block": "display_results"
                    })),
                },
                BlockDefinition {
                    id: "display_results".to_string(),
                    block_type: BlockType::Display,
                    properties: Self::json_to_hashmap(json!({
                        "message": "Nationality API Result: {{processed_result}}",
                        "next_block": "terminate"
                    })),
                },
                BlockDefinition {
                    id: "terminate".to_string(),
                    block_type: BlockType::Terminal,
                    properties: Self::json_to_hashmap(json!({
                        "message": "Nationality prediction flow completed successfully"
                    })),
                },
            ],
        })
    }

    fn create_weather_api_flow(&self, task: &str) -> Result<FlowDefinition> {
        let flow_id = format!("weather_api_flow_{}", uuid::Uuid::new_v4().simple());
        Ok(FlowDefinition {
            id: flow_id,
            name: format!("Weather API Flow: {task}"),
            start_block_id: "weather_intro".to_string(),
            blocks: vec![
                BlockDefinition {
                    id: "weather_intro".to_string(),
                    block_type: BlockType::Display,
                    properties: Self::json_to_hashmap(json!({
                        "message": format!("Getting real weather data for: {task}"),
                        "next_block": "fetch_weather"
                    })),
                },
                BlockDefinition {
                    id: "fetch_weather".to_string(),
                    block_type: BlockType::ExternalData,
                    properties: Self::json_to_hashmap(json!({
                        "api_url": "https://api.open-meteo.com/v1/forecast?latitude=52.52&longitude=13.41&current_weather=true",
                        "data_path": "/current_weather/temperature",
                        "output_key": "temperature",
                        "next_block": "weather_analysis"
                    })),
                },
                BlockDefinition {
                    id: "weather_analysis".to_string(),
                    block_type: BlockType::Compute,
                    properties: Self::json_to_hashmap(json!({
                        "expression": "concat('Current temperature: ', {{temperature}}, '°C')",
                        "result_variable": "weather_report",
                        "next_block": "weather_display"
                    })),
                },
                BlockDefinition {
                    id: "weather_display".to_string(),
                    block_type: BlockType::Display,
                    properties: Self::json_to_hashmap(json!({
                        "message": "Weather Report: {{weather_report}}",
                        "next_block": "end"
                    })),
                },
                BlockDefinition {
                    id: "end".to_string(),
                    block_type: BlockType::Terminal,
                    properties: Self::json_to_hashmap(json!({
                        "message": "Weather forecast flow completed successfully"
                    })),
                },
            ],
        })
    }

    fn create_clean_nationality_flow(&self, name: &str) -> Result<FlowDefinition> {
        let flow_id = format!("nationality_api_{}", uuid::Uuid::new_v4().simple());
        Ok(FlowDefinition {
            id: flow_id,
            name: "Nationality Prediction API".to_string(),
            start_block_id: "api_intro".to_string(),
            blocks: vec![
                BlockDefinition {
                    id: "api_intro".to_string(),
                    block_type: BlockType::Display,
                    properties: Self::json_to_hashmap(json!({
                        "message": format!("Fetching nationality prediction for name: {name}"),
                        "next_block": "fetch_nationality"
                    })),
                },
                BlockDefinition {
                    id: "fetch_nationality".to_string(),
                    block_type: BlockType::ExternalData,
                    properties: Self::json_to_hashmap(json!({
                        "api_url": format!("https://api.nationalise.io?name={name}"),
                        "method": "GET",
                        "data_path": "/country/0/country_id",
                        "output_key": "predicted_nationality",
                        "next_block": "display_result"
                    })),
                },
                BlockDefinition {
                    id: "display_result".to_string(),
                    block_type: BlockType::Display,
                    properties: Self::json_to_hashmap(json!({
                        "message": "Nationality prediction result: {{predicted_nationality}}",
                        "next_block": "end"
                    })),
                },
                BlockDefinition {
                    id: "end".to_string(),
                    block_type: BlockType::Terminal,
                    properties: Self::json_to_hashmap(json!({
                        "message": "Nationality prediction completed successfully"
                    })),
                },
            ],
        })
    }

    fn create_clean_weather_flow(&self, name: &str) -> Result<FlowDefinition> {
        let flow_id = format!("weather_api_{}", uuid::Uuid::new_v4().simple());
        Ok(FlowDefinition {
            id: flow_id,
            name: "Weather Forecast API".to_string(),
            start_block_id: "weather_intro".to_string(),
            blocks: vec![
                BlockDefinition {
                    id: "weather_intro".to_string(),
                    block_type: BlockType::Display,
                    properties: Self::json_to_hashmap(json!({
                        "message": format!("Fetching weather forecast (demo for: {name})"),
                        "next_block": "fetch_weather"
                    })),
                },
                BlockDefinition {
                    id: "fetch_weather".to_string(),
                    block_type: BlockType::ExternalData,
                    properties: Self::json_to_hashmap(json!({
                        "api_url": "https://api.open-meteo.com/v1/forecast?latitude=35.6762&longitude=139.6503&current_weather=true",
                        "method": "GET",
                        "data_path": "/current_weather/temperature",
                        "output_key": "current_temperature",
                        "next_block": "display_weather"
                    })),
                },
                BlockDefinition {
                    id: "display_weather".to_string(),
                    block_type: BlockType::Display,
                    properties: Self::json_to_hashmap(json!({
                        "message": "Current temperature in Tokyo: {{current_temperature}}°C",
                        "next_block": "end"
                    })),
                },
                BlockDefinition {
                    id: "end".to_string(),
                    block_type: BlockType::Terminal,
                    properties: Self::json_to_hashmap(json!({
                        "message": "Weather forecast completed successfully"
                    })),
                },
            ],
        })
    }

    pub async fn execute_api_exploration(&mut self, endpoint_url: &str) -> Result<Value> {
        let flow_definition = self.create_api_exploration_flow(endpoint_url)?;

        let validation_result = self.validator.validate(&flow_definition);
        if !validation_result.is_valid {
            error!("Flow validation failed: {:?}", validation_result.errors);
            return Err(anyhow::anyhow!("Flow validation failed"));
        }

        let execution_result = self
            .execute_flow_with_api_capture(&flow_definition, endpoint_url)
            .await?;

        Ok(execution_result)
    }

    fn create_api_exploration_flow(&self, endpoint_url: &str) -> Result<FlowDefinition> {
        Ok(FlowDefinition {
            id: "api_exploration_flow".to_string(),
            name: "API Exploration Flow".to_string(),
            start_block_id: "explore_api".to_string(),
            blocks: vec![
                BlockDefinition {
                    id: "explore_api".to_string(),
                    block_type: BlockType::ExternalData,
                    properties: Self::json_to_hashmap(json!({
                        "api_url": endpoint_url,
                        "method": "GET",
                        "data_path": "",
                        "output_key": "api_response",
                        "next_block": "preserve_data"
                    })),
                },
                BlockDefinition {
                    id: "preserve_data".to_string(),
                    block_type: BlockType::Compute,
                    properties: Self::json_to_hashmap(json!({
                        "expression": "{{api_response}}",
                        "result_variable": "preserved_api_data",
                        "next_block": "end"
                    })),
                },
                BlockDefinition {
                    id: "end".to_string(),
                    block_type: BlockType::Compute,
                    properties: Self::json_to_hashmap(json!({
                        "expression": "\"API exploration completed\"",
                        "result_variable": "completion_message"

                    })),
                },
                BlockDefinition {
                    id: "default".to_string(),
                    block_type: BlockType::Compute,
                    properties: Self::json_to_hashmap(json!({
                        "expression": "\"Flow terminated.\""
                    })),
                },
            ],
        })
    }

    async fn execute_flow_with_api_capture(
        &mut self,
        flow: &FlowDefinition,
        task: &str,
    ) -> Result<Value> {
        let mut state = stele::flows::state::UnifiedState::new(
            "demo_user".to_string(),
            "demo_operator".to_string(),
            "demo_channel".to_string(),
        );
        state.flow_id = Some(flow.id.clone());
        state.set_data(
            "task".to_string(),
            serde_json::Value::String(task.to_string()),
        );

        self.engine.register_flow(flow.clone())?;

        let mut captured_api_response: Option<Value> = None;

        match self.engine.process_flow(&flow.id, &mut state).await {
            Ok(_) => {
                let mut result = json!({
                    "status": "success",
                    "task": task,
                    "flow_id": flow.id,
                    "flow_name": flow.name,
                    "execution_data": state.data
                });

                if let Some(api_response) = state.data.get("api_response") {
                    captured_api_response = Some(api_response.clone());
                }

                if let Some(api_response) = captured_api_response {
                    if let Some(result_obj) = result.as_object_mut() {
                        result_obj.insert("captured_api_response".to_string(), api_response);
                    }
                }

                Ok(result)
            }
            Err(e) => {
                if let Some(api_response) = state.data.get("api_response") {
                    warn!("Flow execution failed but API response was captured: {}", e);
                    return Ok(json!({
                        "status": "partial_success",
                        "task": task,
                        "flow_id": flow.id,
                        "flow_name": flow.name,
                        "execution_data": {},
                        "captured_api_response": api_response,
                        "error": e.to_string()
                    }));
                }

                error!("Flow execution failed: {}", e);
                Err(anyhow::anyhow!("Flow execution failed: {}", e))
            }
        }
    }
}
