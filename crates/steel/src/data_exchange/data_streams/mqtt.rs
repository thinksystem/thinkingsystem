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

use futures::Stream;
use paho_mqtt::{
    AsyncClient, ConnectOptionsBuilder, CreateOptionsBuilder, Message, MessageBuilder,
};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tracing::{debug, error, warn};
use uuid::Uuid;
#[derive(Debug)]
pub enum Reply<T> {
    Ok(T),
    Err(Box<dyn std::error::Error + Send + Sync>),
}
#[derive(Debug)]
pub struct Envelope<T> {
    pub data: T,
    pub raw_msg: String,
    pub tx: UnboundedSender<Reply<Message>>,
}
#[derive(Debug, thiserror::Error)]
pub enum MqttError {
    #[error("MQTT client error: {0}")]
    Client(String),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Subscription error: {0}")]
    Subscription(String),
    #[error("Publish error: {0}")]
    Publish(String),
    #[error("Message parsing error: {0}")]
    MessageParsing(String),
}
#[derive(Clone, Debug)]
pub struct MQTTConfig {
    pub broker_host: String,
    pub broker_port: u16,
    pub client_id_prefix: String,
    pub keep_alive_interval: u64,
    pub clean_session: bool,
    pub automatic_reconnect: bool,
    pub username: Option<String>,
    pub password: Option<String>,
}
impl Default for MQTTConfig {
    fn default() -> Self {
        Self {
            broker_host: "localhost".to_string(),
            broker_port: 1883,
            client_id_prefix: "bigbot".to_string(),
            keep_alive_interval: 60,
            clean_session: true,
            automatic_reconnect: true,
            username: None,
            password: None,
        }
    }
}
impl MQTTConfig {
    fn generate_client_id(&self) -> String {
        format!("{}-{}", self.client_id_prefix, Uuid::new_v4())
    }
}
pub struct MQTTStream {
    message_rx: UnboundedReceiver<Message>,
    reply_tx: UnboundedSender<Reply<Message>>,
}
impl Stream for MQTTStream {
    type Item = Result<Envelope<String>, MqttError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match this.message_rx.poll_recv(cx) {
            Poll::Ready(Some(msg)) => {
                let payload = String::from_utf8_lossy(msg.payload()).to_string();
                let envelope = Envelope {
                    data: payload.clone(),
                    raw_msg: payload,
                    tx: this.reply_tx.clone(),
                };
                Poll::Ready(Some(Ok(envelope)))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
pub struct DataExchangeMQTTStream {
    client: AsyncClient,
    config: MQTTConfig,
    message_tx: Option<UnboundedSender<Message>>,
}
impl DataExchangeMQTTStream {
    pub fn new(config: MQTTConfig) -> Result<Self, MqttError> {
        let client_id = config.generate_client_id();
        let server_uri = format!("tcp://{}:{}", config.broker_host, config.broker_port);
        let create_opts = CreateOptionsBuilder::new()
            .server_uri(server_uri)
            .client_id(client_id)
            .finalize();
        let client = AsyncClient::new(create_opts)
            .map_err(|e| MqttError::Client(format!("Failed to create MQTT client: {e}")))?;
        Ok(Self {
            client,
            config,
            message_tx: None,
        })
    }
    pub async fn connect(&mut self) -> Result<(), MqttError> {
        let mut base_builder = ConnectOptionsBuilder::new();
        let mut conn_opts_builder = base_builder
            .keep_alive_interval(std::time::Duration::from_secs(
                self.config.keep_alive_interval,
            ))
            .clean_session(self.config.clean_session);
        if self.config.automatic_reconnect {
            conn_opts_builder = conn_opts_builder.automatic_reconnect(
                std::time::Duration::from_secs(1),
                std::time::Duration::from_secs(30),
            );
        }
        if let Some(username) = &self.config.username {
            conn_opts_builder = conn_opts_builder.user_name(username);
        }
        if let Some(password) = &self.config.password {
            conn_opts_builder = conn_opts_builder.password(password);
        }
        let conn_opts = conn_opts_builder.finalize();
        self.client
            .connect(conn_opts)
            .await
            .map_err(|e| MqttError::Connection(format!("Failed to connect to MQTT broker: {e}")))?;
        debug!(
            "Connected to MQTT broker at {}:{}",
            self.config.broker_host, self.config.broker_port
        );
        Ok(())
    }
    pub async fn subscribe(&self, topic: &str, qos: i32) -> Result<(), MqttError> {
        self.client.subscribe(topic, qos).await.map_err(|e| {
            MqttError::Subscription(format!("Failed to subscribe to topic {topic}: {e}"))
        })?;
        debug!("Subscribed to topic: {} with QoS: {}", topic, qos);
        Ok(())
    }
    pub async fn subscribe_multiple(&self, topics: &[(&str, i32)]) -> Result<(), MqttError> {
        let topics_vec: Vec<String> = topics.iter().map(|(topic, _)| topic.to_string()).collect();
        let qos_vec: Vec<i32> = topics.iter().map(|(_, qos)| *qos).collect();
        self.client
            .subscribe_many(&topics_vec, &qos_vec)
            .await
            .map_err(|e| {
                MqttError::Subscription(format!("Failed to subscribe to multiple topics: {e}"))
            })?;
        debug!("Subscribed to {} topics", topics.len());
        Ok(())
    }
    pub fn stream(mut self) -> impl Stream<Item = Result<Envelope<String>, MqttError>> {
        let (message_tx, message_rx) = unbounded_channel::<Message>();
        let (reply_tx, mut reply_rx) = unbounded_channel::<Reply<Message>>();
        self.message_tx = Some(message_tx.clone());
        self.client.set_message_callback(move |_client, msg| {
            if let Some(message) = msg {
                debug!("Received message on topic: {}", message.topic());
                if let Err(e) = message_tx.send(message) {
                    error!("Failed to send message to stream: {}", e);
                }
            }
        });
        self.client.set_connection_lost_callback(|_client| {
            warn!("Connection to MQTT broker lost");
        });
        tokio::spawn(async move {
            while let Some(reply) = reply_rx.recv().await {
                match reply {
                    Reply::Ok(msg) => {
                        debug!(
                            "Message acknowledged successfully from topic: {}",
                            msg.topic()
                        );
                    }
                    Reply::Err(e) => {
                        error!("Error processing message: {}", e);
                    }
                }
            }
        });
        MQTTStream {
            message_rx,
            reply_tx,
        }
    }
    pub async fn publish(&self, topic: &str, payload: &[u8], qos: i32) -> Result<(), MqttError> {
        let msg = Message::new(topic, payload, qos);
        self.client
            .publish(msg)
            .await
            .map_err(|e| MqttError::Publish(format!("Failed to publish to topic {topic}: {e}")))?;
        debug!("Published message to topic: {}", topic);
        Ok(())
    }
    pub async fn publish_json<T: serde::Serialize>(
        &self,
        topic: &str,
        data: &T,
        qos: i32,
    ) -> Result<(), MqttError> {
        let payload = serde_json::to_vec(data)
            .map_err(|e| MqttError::MessageParsing(format!("Failed to serialise JSON: {e}")))?;
        self.publish(topic, &payload, qos).await
    }
    pub async fn publish_with_retain(
        &self,
        topic: &str,
        payload: &[u8],
        qos: i32,
        retain: bool,
    ) -> Result<(), MqttError> {
        let msg = MessageBuilder::new()
            .topic(topic)
            .payload(payload)
            .qos(qos)
            .retained(retain)
            .finalize();
        self.client
            .publish(msg)
            .await
            .map_err(|e| MqttError::Publish(format!("Failed to publish to topic {topic}: {e}")))?;
        debug!("Published message to topic: {} (retain: {})", topic, retain);
        Ok(())
    }
    pub async fn unsubscribe(&self, topic: &str) -> Result<(), MqttError> {
        self.client.unsubscribe(topic).await.map_err(|e| {
            MqttError::Subscription(format!("Failed to unsubscribe from topic {topic}: {e}"))
        })?;
        debug!("Unsubscribed from topic: {}", topic);
        Ok(())
    }
    pub async fn disconnect(&self) -> Result<(), MqttError> {
        self.client.disconnect(None).await.map_err(|e| {
            MqttError::Connection(format!("Failed to disconnect from MQTT broker: {e}"))
        })?;
        debug!("Disconnected from MQTT broker");
        Ok(())
    }
    pub fn is_connected(&self) -> bool {
        self.client.is_connected()
    }
}
pub mod utils {
    pub fn extract_topic_levels(topic: &str) -> Vec<&str> {
        topic.split('/').collect()
    }
    pub fn is_wildcard_topic(topic: &str) -> bool {
        topic.contains('+') || topic.contains('#')
    }
    pub fn topic_matches(topic: &str, pattern: &str) -> bool {
        if pattern == "#" {
            return true;
        }
        let topic_levels = extract_topic_levels(topic);
        let pattern_levels = extract_topic_levels(pattern);
        if pattern_levels.last() == Some(&"#") {
            return topic_levels.len() >= pattern_levels.len() - 1
                && topic_levels
                    .iter()
                    .zip(pattern_levels.iter().take(pattern_levels.len() - 1))
                    .all(|(t, p)| *p == "+" || t == p);
        }
        if topic_levels.len() != pattern_levels.len() {
            return false;
        }
        topic_levels
            .iter()
            .zip(pattern_levels.iter())
            .all(|(t, p)| *p == "+" || t == p)
    }
    pub fn sanitise_topic(topic: &str) -> String {
        topic
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '/' || *c == '-' || *c == '_')
            .collect()
    }
}
