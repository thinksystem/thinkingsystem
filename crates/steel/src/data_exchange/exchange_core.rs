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

use crate::data_exchange::data_bridging::MqttExchangeProvider;
use crate::data_exchange::data_streams::event::EventType;
use crate::data_exchange::data_streams::grpc::{
    create_grpc_data_exchange, DataExchange as GrpcDataExchangeTrait, GrpcApiClientImpl,
    GrpcDataExchange,
};
use crate::data_exchange::error::DataExchangeError;
use crate::data_exchange::exchange_interfaces::{ConnectionType, DataExchangeImpl};
use async_trait::async_trait;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::ClientConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

type DataExchangeProvider =
    Box<dyn DataExchangeImpl<String, Result<HashMap<String, String>, DataExchangeError>>>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    Command,
    Query,
    Event,
    Unknown,
}
impl From<Classification> for &'static str {
    fn from(classification: Classification) -> &'static str {
        match classification {
            Classification::Command => "command",
            Classification::Query => "query",
            Classification::Event => "event",
            Classification::Unknown => "unknown",
        }
    }
}
impl From<Classification> for EventType {
    fn from(classification: Classification) -> EventType {
        match classification {
            Classification::Command => EventType::CustomEvent("command".to_string()),
            Classification::Query => EventType::CustomEvent("query".to_string()),
            Classification::Event => EventType::CustomEvent("event".to_string()),
            Classification::Unknown => EventType::CustomEvent("unknown".to_string()),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MetadataValue {
    String(String),
    Number(f64),
    Boolean(bool),
}
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MessageMetadata {
    #[serde(default)]
    pub metadata: HashMap<String, MetadataValue>,
}
impl MessageMetadata {
    pub fn with_type(mut self, message_type: &str) -> Self {
        self.metadata.insert(
            "type".to_string(),
            MetadataValue::String(message_type.to_string()),
        );
        self
    }
    pub fn with_metadata(mut self, key: String, value: MetadataValue) -> Self {
        self.metadata.insert(key, value);
        self
    }
}
#[derive(Clone, Debug, Deserialize)]
pub struct DataExchangeConfig {
    pub providers: Vec<ProviderConfig>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub connection_type: ConnectionType,
    pub config: HashMap<String, String>,
}
pub struct HttpDataExchangeImpl {
    client: reqwest::Client,
    base_url: String,
}
impl HttpDataExchangeImpl {
    pub fn new(config: &ProviderConfig) -> Result<Self, DataExchangeError> {
        let base_url = config
            .config
            .get("base_url")
            .ok_or_else(|| {
                DataExchangeError::Configuration(format!(
                    "Missing 'base_url' for HTTP provider '{}'",
                    config.name
                ))
            })?
            .clone();
        Ok(Self {
            client: reqwest::Client::new(),
            base_url,
        })
    }
}
#[async_trait]
impl DataExchangeImpl<String, Result<HashMap<String, String>, DataExchangeError>>
    for HttpDataExchangeImpl
{
    async fn exchange_data(
        &self,
        request: String,
    ) -> Result<HashMap<String, String>, DataExchangeError> {
        let response = self
            .client
            .post(&self.base_url)
            .header("Content-Type", "application/json")
            .body(request)
            .send()
            .await?;
        let response_body = response.json().await?;
        Ok(response_body)
    }
}
pub struct KafkaExchangeProvider {
    producer: FutureProducer,
    topic: String,
}
impl KafkaExchangeProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self, DataExchangeError> {
        let bootstrap_servers = config.config.get("bootstrap.servers").ok_or_else(|| {
            DataExchangeError::Configuration(format!(
                "Missing 'bootstrap.servers' for Kafka provider '{}'",
                config.name
            ))
        })?;
        let topic = config.config.get("topic").ok_or_else(|| {
            DataExchangeError::Configuration(format!(
                "Missing 'topic' for Kafka provider '{}'",
                config.name
            ))
        })?;
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap_servers)
            .create()?;
        Ok(Self {
            producer,
            topic: topic.clone(),
        })
    }
}
#[async_trait]
impl DataExchangeImpl<String, Result<HashMap<String, String>, DataExchangeError>>
    for KafkaExchangeProvider
{
    async fn exchange_data(
        &self,
        request: String,
    ) -> Result<HashMap<String, String>, DataExchangeError> {
        let topic = &self.topic;
        let key = Uuid::new_v4().to_string();
        let record = FutureRecord::to(topic).payload(&request).key(&key);
        self.producer
            .send(record, std::time::Duration::from_secs(0))
            .await
            .map_err(|(e, _)| e)?;
        let mut response = HashMap::new();
        response.insert("status".to_string(), "published".to_string());
        response.insert("topic".to_string(), self.topic.clone());
        Ok(response)
    }
}
#[derive(Debug, Serialize, Deserialize)]
struct GrpcRequestPayload {
    operator_id: String,
    package: String,
    data: String,
}
pub struct GrpcExchangeProvider {
    grpc_exchange: Arc<Mutex<GrpcDataExchange<GrpcApiClientImpl>>>,
}
impl GrpcExchangeProvider {
    pub async fn new(config: &ProviderConfig) -> Result<Self, DataExchangeError> {
        let grpc_address = config.config.get("grpc_address").ok_or_else(|| {
            DataExchangeError::Configuration(format!(
                "Missing 'grpc_address' for gRPC provider '{}'",
                config.name
            ))
        })?;
        let grpc_exchange = create_grpc_data_exchange(grpc_address.clone())
            .await
            .map_err(|e| DataExchangeError::Grpc(e.to_string()))?;
        Ok(Self {
            grpc_exchange: Arc::new(Mutex::new(grpc_exchange)),
        })
    }
}
#[async_trait]
impl DataExchangeImpl<String, Result<HashMap<String, String>, DataExchangeError>>
    for GrpcExchangeProvider
{
    async fn exchange_data(
        &self,
        request: String,
    ) -> Result<HashMap<String, String>, DataExchangeError> {
        let payload: GrpcRequestPayload = serde_json::from_str(&request)?;
        let grpc_exchange = self.grpc_exchange.lock().await;
        let result = grpc_exchange
            .call(payload.operator_id, payload.package, payload.data)
            .await
            .map_err(|e| DataExchangeError::Grpc(e.to_string()))?;
        Ok(result)
    }
}
pub struct DataExchangeProcessor {
    providers: HashMap<String, DataExchangeProvider>,
}
impl DataExchangeProcessor {
    pub async fn new(config: &DataExchangeConfig) -> Result<Self, DataExchangeError> {
        let mut providers: HashMap<String, Box<dyn DataExchangeImpl<_, _>>> = HashMap::new();
        for provider_config in &config.providers {
            let provider_name = provider_config.name.clone();
            let provider_instance: Box<dyn DataExchangeImpl<_, _>> =
                match provider_config.connection_type {
                    ConnectionType::Rest | ConnectionType::Webhook => {
                        let http_provider = HttpDataExchangeImpl::new(provider_config)?;
                        Box::new(http_provider)
                    }
                    ConnectionType::Mqtt => {
                        let mqtt_provider = MqttExchangeProvider::new(provider_config).await?;
                        Box::new(mqtt_provider)
                    }
                    ConnectionType::Kafka => {
                        let kafka_provider = KafkaExchangeProvider::new(provider_config)?;
                        Box::new(kafka_provider)
                    }
                    ConnectionType::Grpc => {
                        let grpc_provider = GrpcExchangeProvider::new(provider_config).await?;
                        Box::new(grpc_provider)
                    }
                };
            providers.insert(provider_name, provider_instance);
        }
        Ok(Self { providers })
    }
    pub async fn exchange_data(
        &self,
        provider_name: &str,
        request: String,
    ) -> Result<HashMap<String, String>, DataExchangeError> {
        let provider = self
            .providers
            .get(provider_name)
            .ok_or_else(|| DataExchangeError::ProviderNotFound(provider_name.to_string()))?;
        provider.exchange_data(request).await
    }
    pub async fn classify_and_exchange(
        &self,
        provider_name: &str,
        message_body: String,
        metadata: &MessageMetadata,
    ) -> Result<HashMap<String, String>, DataExchangeError> {
        let classification = Self::classify_message(&metadata.metadata);
        let message = SimpleMessage {
            unique_id: Uuid::new_v4().to_string(),
            classification: classification.into(),
            content: message_body,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        let message_string = serde_json::to_string(&message)?;
        self.exchange_data(provider_name, message_string).await
    }
    fn classify_message(metadata: &HashMap<String, MetadataValue>) -> Classification {
        match metadata.get("type") {
            Some(MetadataValue::String(msg_type)) => match msg_type.as_str() {
                "command" => Classification::Command,
                "query" => Classification::Query,
                "event" => Classification::Event,
                _ => Classification::Unknown,
            },
            _ => Classification::Unknown,
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
struct SimpleMessage {
    unique_id: String,
    classification: String,
    content: String,
    timestamp: u64,
}
impl From<Classification> for String {
    fn from(classification: Classification) -> String {
        let s: &'static str = classification.into();
        s.to_string()
    }
}
