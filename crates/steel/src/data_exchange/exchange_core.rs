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
use crate::data_exchange::data_streams::quic::QuicExchangeProvider;
use crate::data_exchange::error::DataExchangeError;
use crate::data_exchange::exchange_interfaces::{ConnectionType, DataExchangeImpl};
use async_trait::async_trait;
use rdkafka::producer::{FutureProducer, FutureRecord, Producer};
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
    provider_types: HashMap<String, ConnectionType>,
    provider_configs: HashMap<String, ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub name: String,
    pub connection_type: ConnectionType,
}

impl DataExchangeProcessor {
    pub async fn new(config: &DataExchangeConfig) -> Result<Self, DataExchangeError> {
        let mut providers: HashMap<String, Box<dyn DataExchangeImpl<_, _>>> = HashMap::new();
        let mut provider_types: HashMap<String, ConnectionType> = HashMap::new();
        let mut provider_configs: HashMap<String, ProviderConfig> = HashMap::new();
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
                    ConnectionType::Quic => {
                        let quic_provider = QuicExchangeProvider::new(provider_config).await?;
                        Box::new(quic_provider)
                    }
                };
            provider_types.insert(
                provider_name.clone(),
                provider_config.connection_type.clone(),
            );
            
            provider_configs.insert(provider_name.clone(), provider_config.clone());
            providers.insert(provider_name, provider_instance);
        }
        Ok(Self {
            providers,
            provider_types,
            provider_configs,
        })
    }

    pub fn list_providers(&self) -> Vec<ProviderInfo> {
        self.provider_types
            .iter()
            .map(|(name, ct)| ProviderInfo {
                name: name.clone(),
                connection_type: ct.clone(),
            })
            .collect()
    }

    pub async fn add_provider(&mut self, cfg: ProviderConfig) -> Result<(), DataExchangeError> {
        let name = cfg.name.clone();
        if self.providers.contains_key(&name) {
            return Err(DataExchangeError::Configuration(format!(
                "provider '{name}' exists"
            )));
        }
        let instance: Box<dyn DataExchangeImpl<_, _>> = match cfg.connection_type {
            ConnectionType::Rest | ConnectionType::Webhook => {
                Box::new(HttpDataExchangeImpl::new(&cfg)?)
            }
            ConnectionType::Mqtt => Box::new(MqttExchangeProvider::new(&cfg).await?),
            ConnectionType::Kafka => Box::new(KafkaExchangeProvider::new(&cfg)?),
            ConnectionType::Grpc => Box::new(GrpcExchangeProvider::new(&cfg).await?),
            ConnectionType::Quic => Box::new(QuicExchangeProvider::new(&cfg).await?),
        };
        self.provider_types
            .insert(name.clone(), cfg.connection_type.clone());
        self.provider_configs.insert(name.clone(), cfg);
        self.providers.insert(name, instance);
        Ok(())
    }

    pub fn remove_provider(&mut self, name: &str) -> bool {
        self.providers.remove(name).is_some()
            & self.provider_types.remove(name).is_some()
            & self.provider_configs.remove(name).is_some()
    }

    pub async fn provider_health(
        &self,
        provider_name: &str,
    ) -> Result<HashMap<String, String>, DataExchangeError> {
        if !self.providers.contains_key(provider_name) {
            return Err(DataExchangeError::ProviderNotFound(
                provider_name.to_string(),
            ));
        }
        let mut map = HashMap::new();
        let ct = self
            .provider_types
            .get(provider_name)
            .cloned()
            .unwrap_or(ConnectionType::Rest);
        map.insert("provider".into(), provider_name.to_string());
        map.insert("connection_type".into(), format!("{ct:?}"));
        let cfg = self.provider_configs.get(provider_name);
        match ct {
            ConnectionType::Rest | ConnectionType::Webhook => {
                if let Some(cfg) = cfg {
                    if let Some(url) = cfg.config.get("base_url") {
                        let client = reqwest::Client::new();
                        let res = tokio::time::timeout(
                            std::time::Duration::from_secs(2),
                            client.get(url).send(),
                        )
                        .await;
                        match res {
                            Ok(Ok(r)) => {
                                map.insert("status".into(), "ok".into());
                                map.insert("http_code".into(), r.status().as_u16().to_string());
                            }
                            Ok(Err(e)) => {
                                map.insert("status".into(), "error".into());
                                map.insert("error".into(), e.to_string());
                            }
                            Err(_) => {
                                map.insert("status".into(), "timeout".into());
                            }
                        }
                    } else {
                        map.insert("status".into(), "no_base_url".into());
                    }
                }
            }
            ConnectionType::Kafka => {
                if let Some(cfg) = cfg {
                    if let (Some(bootstrap), Some(topic)) =
                        (cfg.config.get("bootstrap.servers"), cfg.config.get("topic"))
                    {
                        let bootstrap = bootstrap.clone();
                        let topic_name = topic.clone();
                        let md = tokio::task::spawn_blocking(move || {
                            let producer: Result<FutureProducer, _> = ClientConfig::new()
                                .set("bootstrap.servers", &bootstrap)
                                .create();
                            match producer {
                                Ok(p) => p
                                    .client()
                                    .fetch_metadata(
                                        Some(&topic_name),
                                        std::time::Duration::from_millis(500),
                                    )
                                    .map(|m| (m.orig_broker_id(), m.brokers().len())),
                                Err(e) => Err(e),
                            }
                        })
                        .await;
                        match md {
                            Ok(Ok((_broker_id, broker_count))) => {
                                map.insert("status".into(), "ok".into());
                                map.insert("brokers".into(), broker_count.to_string());
                            }
                            Ok(Err(e)) => {
                                map.insert("status".into(), "error".into());
                                map.insert("error".into(), e.to_string());
                            }
                            Err(e) => {
                                map.insert("status".into(), "error".into());
                                map.insert("error".into(), e.to_string());
                            }
                        }
                    }
                }
            }
            ConnectionType::Mqtt => {
                
                map.insert("status".into(), "assumed_ok".into());
            }
            ConnectionType::Grpc | ConnectionType::Quic => {
                map.insert("status".into(), "unknown".into());
            }
        }
        Ok(map)
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
