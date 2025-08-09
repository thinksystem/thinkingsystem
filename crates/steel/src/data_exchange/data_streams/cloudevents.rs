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

use crate::data_exchange::data_streams::kafka::KafkaError;
use crate::data_exchange::data_streams::mqtt::{DataExchangeMQTTStream, Reply};
use async_trait::async_trait;
use futures::stream::StreamExt;
use rdkafka::producer::{FutureProducer, FutureRecord};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;
use tracing::{error, warn};
#[async_trait]
pub trait Sink {
    type Error;
    async fn consume(&self, data: FutureRecord<'_, str, str>) -> Result<(), Self::Error>;
}
#[derive(Debug, Clone)]
pub struct EventConfig {
    pub kafka_topic: String,
    pub allowed_classifications: HashSet<String>,
}
impl EventConfig {
    pub fn new(kafka_topic: String) -> Self {
        Self {
            kafka_topic,
            allowed_classifications: HashSet::new(),
        }
    }
    pub fn with_allowed_classifications(mut self, classifications: HashSet<String>) -> Self {
        self.allowed_classifications = classifications;
        self
    }
}
#[derive(Debug, Deserialize, Serialize)]
pub struct IncomingEventData {
    pub classification: String,
    pub data: String,
}
#[derive(Debug)]
pub struct Event {
    pub data: IncomingEventData,
    pub raw_payload: String,
}
impl Event {
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        let data: IncomingEventData = serde_json::from_str(json_str)?;
        Ok(Event {
            data,
            raw_payload: json_str.to_string(),
        })
    }
}
#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("MQTT stream error: {0}")]
    MqttStream(#[from] std::io::Error),
    #[error("Payload deserialization error: {0}")]
    Deserialization(#[from] serde_json::Error),
    #[error("Invalid classification: {0}")]
    InvalidClassification(String),
    #[error("Missing required field in payload: {0}")]
    MissingField(String),
    #[error("Kafka sink error: {0}")]
    KafkaSink(#[from] KafkaError),
    #[error("Event data payload is empty")]
    MissingPayload,
}
pub struct EventHandler {
    producer: FutureProducer,
    mqtt_stream: DataExchangeMQTTStream,
    config: EventConfig,
}
impl EventHandler {
    pub fn new(
        kafka_producer: FutureProducer,
        mqtt_stream: DataExchangeMQTTStream,
        config: EventConfig,
    ) -> Self {
        Self {
            producer: kafka_producer,
            mqtt_stream,
            config,
        }
    }
    pub async fn handle_events(self) {
        let EventHandler {
            producer,
            mqtt_stream,
            config,
        } = self;
        let mut stream = mqtt_stream.stream();
        while let Some(envelope_result) = stream.next().await {
            match envelope_result {
                Ok(envelope) => {
                    let reply_tx = envelope.tx;
                    match Event::from_json(&envelope.raw_msg) {
                        Ok(event) => match process_event(&producer, &config, &event).await {
                            Ok(_) => {
                                let dummy_msg = paho_mqtt::Message::new("", "", 0);
                                let _ = reply_tx.send(Reply::Ok(dummy_msg));
                            }
                            Err(e) => {
                                error!("Error processing event, sending NACK: {}", e);
                                let _ = reply_tx.send(Reply::Err(Box::new(e)));
                            }
                        },
                        Err(e) => {
                            error!("Failed to parse event JSON: {}", e);
                            let _ =
                                reply_tx.send(Reply::Err(Box::new(EventError::Deserialization(e))));
                        }
                    }
                }
                Err(e) => {
                    error!("Error from MQTT stream, message skipped: {}", e);
                }
            }
        }
        warn!("MQTT stream for events has ended.");
    }
    #[allow(dead_code)]
    async fn process_event(&self, event: &Event) -> Result<(), EventError> {
        let incoming_data = &event.data;
        if !self.config.allowed_classifications.is_empty()
            && !self
                .config
                .allowed_classifications
                .contains(&incoming_data.classification)
        {
            return Err(EventError::InvalidClassification(
                incoming_data.classification.clone(),
            ));
        }
        if incoming_data.data.is_empty() {
            return Err(EventError::MissingField("data".to_string()));
        }
        if incoming_data.classification.is_empty() {
            return Err(EventError::MissingField("classification".to_string()));
        }
        self.route_message(&incoming_data.classification, &incoming_data.data)
            .await?;
        Ok(())
    }
    #[allow(dead_code)]
    async fn route_message(&self, classification: &str, message: &str) -> Result<(), EventError> {
        let topic = self.build_topic_name(classification);
        let record = FutureRecord::to(&topic)
            .payload(message)
            .key(classification);
        self.producer
            .send(record, Duration::from_secs(30))
            .await
            .map_err(|(e, _)| KafkaError::Producer(e))?;
        Ok(())
    }
    #[allow(dead_code)]
    fn build_topic_name(&self, classification: &str) -> String {
        let sanitised = classification
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect::<String>();
        if sanitised.is_empty() {
            warn!("Empty classification after sanitisation, using default topic");
            return self.config.kafka_topic.clone();
        }
        let truncated = if sanitised.len() > 100 {
            warn!("Classification too long, truncating: {}", sanitised);
            &sanitised[..100]
        } else {
            &sanitised
        };
        format!("{}-{}", self.config.kafka_topic, truncated)
    }
}
async fn process_event(
    producer: &FutureProducer,
    config: &EventConfig,
    event: &Event,
) -> Result<(), EventError> {
    let incoming_data = &event.data;
    if !config.allowed_classifications.is_empty()
        && !config
            .allowed_classifications
            .contains(&incoming_data.classification)
    {
        return Err(EventError::InvalidClassification(
            incoming_data.classification.clone(),
        ));
    }
    if incoming_data.data.is_empty() {
        return Err(EventError::MissingField("data".to_string()));
    }
    if incoming_data.classification.is_empty() {
        return Err(EventError::MissingField("classification".to_string()));
    }
    route_message(
        producer,
        config,
        &incoming_data.classification,
        &incoming_data.data,
    )
    .await?;
    Ok(())
}
async fn route_message(
    producer: &FutureProducer,
    config: &EventConfig,
    classification: &str,
    message: &str,
) -> Result<(), EventError> {
    let topic = build_topic_name(config, classification);
    let record = FutureRecord::to(&topic)
        .payload(message)
        .key(classification);
    producer
        .send(record, Duration::from_secs(30))
        .await
        .map_err(|(e, _)| KafkaError::Producer(e))?;
    Ok(())
}
fn build_topic_name(config: &EventConfig, classification: &str) -> String {
    let sanitised = classification
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>();
    if sanitised.is_empty() {
        warn!("Empty classification after sanitisation, using default topic");
        return config.kafka_topic.clone();
    }
    let truncated = if sanitised.len() > 100 {
        warn!("Classification too long, truncating: {}", sanitised);
        &sanitised[..100]
    } else {
        &sanitised
    };
    format!("{}-{}", config.kafka_topic, truncated)
}
