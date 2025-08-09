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

use async_trait::async_trait;
use rdkafka::{
    config::ClientConfig,
    consumer::{Consumer, StreamConsumer},
    message::{BorrowedMessage, Headers},
    producer::{FutureProducer, FutureRecord},
    util::Timeout,
    Message,
};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{broadcast, oneshot, Mutex};
use tracing::{error, info, warn};
use uuid::Uuid;
#[derive(Debug, thiserror::Error)]
pub enum KafkaError {
    #[error("Kafka producer error: {0}")]
    Producer(#[from] rdkafka::error::KafkaError),
    #[error("Kafka consumer error: {0}")]
    Consumer(rdkafka::error::KafkaError),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("UTF-8 conversion error: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("Timeout waiting for reply")]
    Timeout,
    #[error("Reply channel was closed unexpectedly")]
    ReplyChannelClosed,
    #[error("No correlation ID in reply message")]
    MissingCorrelationId,
    #[error("Reply topic not specified")]
    MissingReplyTopic,
}
#[async_trait]
pub trait Sink<T> {
    type Error;
    async fn consume(&self, item: T) -> Result<(), Self::Error>;
}
pub struct KafkaSink {
    producer: FutureProducer,
    topic: String,
}
impl KafkaSink {
    pub fn new(producer: FutureProducer, topic: String) -> Self {
        Self { producer, topic }
    }
}
#[async_trait]
impl<T> Sink<T> for KafkaSink
where
    T: Serialize + Send + Sync + 'static,
{
    type Error = KafkaError;
    async fn consume(&self, item: T) -> Result<(), Self::Error> {
        let payload = serde_json::to_vec(&item)?;
        let record = FutureRecord::to(&self.topic).payload(&payload).key("");
        self.producer
            .send(record, Duration::from_secs(30))
            .await
            .map_err(|(e, _)| KafkaError::Producer(e))?;
        Ok(())
    }
}
type PendingReplies = Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>;
pub struct KafkaRequestReply {
    producer: FutureProducer,
    reply_topic: String,
    pending_replies: PendingReplies,
    reply_consumer_handle: Arc<tokio::task::JoinHandle<()>>,
    shutdown_tx: broadcast::Sender<()>,
}
impl KafkaRequestReply {
    pub fn new(bootstrap_servers: &str, group_id: &str) -> Result<Self, KafkaError> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap_servers)
            .create()
            .map_err(KafkaError::Producer)?;
        let reply_topic = format!("reply-topic-{}", Uuid::new_v4());
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap_servers)
            .set("group.id", group_id)
            .set("auto.offset.reset", "earliest")
            .create()
            .map_err(KafkaError::Consumer)?;
        consumer
            .subscribe(&[&reply_topic])
            .map_err(KafkaError::Consumer)?;
        info!("Subscribed to unique reply topic: {}", reply_topic);
        let pending_replies: PendingReplies = Arc::new(Mutex::new(HashMap::new()));
        let pending_replies_clone = pending_replies.clone();
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let reply_consumer_handle = tokio::spawn(Self::run_reply_consumer(
            consumer,
            pending_replies_clone,
            shutdown_rx,
        ));
        Ok(Self {
            producer,
            reply_topic,
            pending_replies,
            reply_consumer_handle: Arc::new(reply_consumer_handle),
            shutdown_tx,
        })
    }
    async fn run_reply_consumer(
        consumer: StreamConsumer,
        pending_replies: PendingReplies,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.recv() => {
                    info!("Kafka reply consumer received shutdown signal. Exiting.");
                    break;
                }
                result = consumer.recv() => {
                    match result {
                        Ok(message) => {
                            if let Some(correlation_id) = extract_correlation_id(&message) {
                                if let Some(sender) = pending_replies.lock().await.remove(&correlation_id) {
                                    if let Some(payload) = message.payload() {
                                        match std::str::from_utf8(payload) {
                                            Ok(payload_str) => {
                                                if sender.send(payload_str.to_string()).is_err() {
                                                    warn!("Reply received for timed-out or dropped request: {}", correlation_id);
                                                }
                                            }
                                            Err(e) => error!("Failed to decode reply payload as UTF-8: {}", e),
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Error receiving reply from Kafka: {}. Stopping reply listener.", e);
                            break;
                        }
                    }
                }
            }
        }
        info!("Kafka reply consumer has shut down.");
    }

    pub async fn send_request(
        &self,
        topic: &str,
        message: &str,
        timeout: Duration,
    ) -> Result<String, KafkaError> {
        let correlation_id = Uuid::new_v4().to_string();
        let (reply_tx, reply_rx) = oneshot::channel();

        {
            let mut pending = self.pending_replies.lock().await;
            pending.insert(correlation_id.clone(), reply_tx);
        }

        let record = FutureRecord::to(topic)
            .key(&correlation_id)
            .payload(message)
            .headers(
                rdkafka::message::OwnedHeaders::new()
                    .insert(rdkafka::message::Header {
                        key: "correlation_id",
                        value: Some(&correlation_id),
                    })
                    .insert(rdkafka::message::Header {
                        key: "reply_topic",
                        value: Some(&self.reply_topic),
                    }),
            );

        self.producer
            .send(record, Timeout::After(Duration::from_secs(5)))
            .await
            .map_err(|(kafka_err, _)| KafkaError::Producer(kafka_err))?;

        match tokio::time::timeout(timeout, reply_rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(KafkaError::ReplyChannelClosed),
            Err(_) => Err(KafkaError::Timeout),
        }
    }

    pub async fn shutdown(self) {
        info!("Shutting down KafkaRequestReply service.");
        let _ = self.shutdown_tx.send(());
        let handle = self.reply_consumer_handle.clone();
        if let Ok(handle) = Arc::try_unwrap(handle) {
            if let Err(e) = handle.await {
                error!(
                    "Kafka reply consumer task panicked during shutdown: {:?}",
                    e
                );
            }
        } else {
            warn!("Could not exclusively own consumer handle; shutdown may be incomplete. Another handle may still exist.");
        }
    }
}
fn extract_correlation_id(msg: &BorrowedMessage<'_>) -> Option<String> {
    let headers = msg.headers()?;
    for i in 0..headers.count() {
        if let Ok(header) = headers.get_as::<str>(i) {
            if header.key == "correlation_id" {
                return header.value.map(|v| v.to_string());
            }
        }
    }
    None
}
impl Drop for KafkaRequestReply {
    fn drop(&mut self) {
        if self.shutdown_tx.send(()).is_ok() {
            warn!("KafkaRequestReply was dropped without calling shutdown(). The background task has been signalled to stop, but not awaited.");
        }
    }
}
