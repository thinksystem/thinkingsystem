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
use futures::future::join_all;
use neo4rs::Graph;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock, Semaphore};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

type EventHandlerMap = Arc<RwLock<HashMap<String, Vec<Arc<RwLock<dyn EventHandler>>>>>>;
#[derive(Debug, Copy, Clone)]
pub struct Location(pub f32, pub f32, pub f32);
#[derive(Debug, Copy, Clone)]
pub struct Duration(pub u64, pub u64);
#[derive(Debug, Clone)]
pub struct Event {
    unique_id: String,
    user_id: Option<i64>,
    time: Option<i64>,
    header: EventHeader,
    event_type: EventType,
    id: Option<i64>,
    name: String,
    location: Option<Location>,
    start_time: Option<u64>,
    end_time: Option<u64>,
    significance: Option<f64>,
    attributes: HashMap<String, f64>,
    duration: Option<Duration>,
    dependencies: Vec<Arc<Event>>,
    start: i32,
    end: i32,
    resource: Option<String>,
    tags: Vec<String>,
}
impl Event {
    pub fn builder(unique_id: String, name: String, event_type: EventType) -> EventBuilder {
        EventBuilder::new(unique_id, name, event_type)
    }
    pub fn unique_id(&self) -> &str {
        &self.unique_id
    }
    pub fn user_id(&self) -> Option<i64> {
        self.user_id
    }
    pub fn time(&self) -> Option<i64> {
        self.time
    }
    pub fn header(&self) -> &EventHeader {
        &self.header
    }
    pub fn event_type(&self) -> &EventType {
        &self.event_type
    }
    pub fn id(&self) -> Option<i64> {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn location(&self) -> Option<Location> {
        self.location
    }
    pub fn start_time(&self) -> Option<u64> {
        self.start_time
    }
    pub fn end_time(&self) -> Option<u64> {
        self.end_time
    }
    pub fn significance(&self) -> Option<f64> {
        self.significance
    }
    pub fn attributes(&self) -> &HashMap<String, f64> {
        &self.attributes
    }
    pub fn duration(&self) -> Option<Duration> {
        self.duration
    }
    pub fn dependencies(&self) -> &Vec<Arc<Event>> {
        &self.dependencies
    }
    pub fn start(&self) -> i32 {
        self.start
    }
    pub fn end(&self) -> i32 {
        self.end
    }
    pub fn resource(&self) -> Option<&str> {
        self.resource.as_deref()
    }
    pub fn tags(&self) -> &Vec<String> {
        &self.tags
    }
}
#[derive(Debug, Default)]
pub struct EventBuilder {
    unique_id: Option<String>,
    user_id: Option<i64>,
    time: Option<i64>,
    header: Option<EventHeader>,
    event_type: Option<EventType>,
    id: Option<i64>,
    name: Option<String>,
    location: Option<Location>,
    start_time: Option<u64>,
    end_time: Option<u64>,
    significance: Option<f64>,
    attributes: HashMap<String, f64>,
    duration: Option<Duration>,
    dependencies: Vec<Arc<Event>>,
    start: Option<i32>,
    end: Option<i32>,
    resource: Option<String>,
    tags: Vec<String>,
}
#[derive(Debug, thiserror::Error)]
pub enum EventBuilderError {
    #[error("Missing required field: {0}")]
    MissingField(&'static str),
}
impl EventBuilder {
    pub fn new(unique_id: String, name: String, event_type: EventType) -> Self {
        Self {
            unique_id: Some(unique_id),
            name: Some(name),
            event_type: Some(event_type),
            ..Default::default()
        }
    }
    pub fn user_id(mut self, user_id: i64) -> Self {
        self.user_id = Some(user_id);
        self
    }
    pub fn time(mut self, time: i64) -> Self {
        self.time = Some(time);
        self
    }
    pub fn header(mut self, header: EventHeader) -> Self {
        self.header = Some(header);
        self
    }
    pub fn id(mut self, id: i64) -> Self {
        self.id = Some(id);
        self
    }
    pub fn location(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
    }
    pub fn start_time(mut self, start_time: u64) -> Self {
        self.start_time = Some(start_time);
        self
    }
    pub fn end_time(mut self, end_time: u64) -> Self {
        self.end_time = Some(end_time);
        self
    }
    pub fn significance(mut self, significance: f64) -> Self {
        self.significance = Some(significance);
        self
    }
    pub fn attributes(mut self, attributes: HashMap<String, f64>) -> Self {
        self.attributes = attributes;
        self
    }
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }
    pub fn dependencies(mut self, dependencies: Vec<Arc<Event>>) -> Self {
        self.dependencies = dependencies;
        self
    }
    pub fn start(mut self, start: i32) -> Self {
        self.start = Some(start);
        self
    }
    pub fn end(mut self, end: i32) -> Self {
        self.end = Some(end);
        self
    }
    pub fn resource(mut self, resource: String) -> Self {
        self.resource = Some(resource);
        self
    }
    pub fn tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
    pub fn build(self) -> Result<Event, EventBuilderError> {
        let unique_id = self
            .unique_id
            .ok_or(EventBuilderError::MissingField("unique_id"))?;
        let name = self.name.ok_or(EventBuilderError::MissingField("name"))?;
        let event_type = self
            .event_type
            .ok_or(EventBuilderError::MissingField("event_type"))?;
        Ok(Event {
            unique_id,
            name,
            event_type,
            user_id: self.user_id,
            time: self.time,
            header: self.header.unwrap_or_default(),
            id: self.id,
            location: self.location,
            start_time: self.start_time,
            end_time: self.end_time,
            significance: self.significance,
            attributes: self.attributes,
            duration: self.duration,
            dependencies: self.dependencies,
            start: self.start.unwrap_or_default(),
            end: self.end.unwrap_or_default(),
            resource: self.resource,
            tags: self.tags,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventType {
    SeismicAnomaly,
    ScheduledEvent,
    AgentPreference,
    AlertGraph,
    CustomEvent(String),
    Mentioned(String),
    Scheduled(String),
    PostedMedia(String),
    CasualMeetup,
    Conference,
    Workshop,
}
#[derive(Debug, Clone, Default)]
pub struct EventHeader {
    ip: Option<String>,
    device_type: Option<String>,
    trace_id: Option<String>,
    via_bot_id: bool,
}

impl EventHeader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_ip(mut self, ip: String) -> Self {
        self.ip = Some(ip);
        self
    }

    pub fn with_device_type(mut self, device_type: String) -> Self {
        self.device_type = Some(device_type);
        self
    }

    pub fn with_trace_id(mut self, trace_id: String) -> Self {
        self.trace_id = Some(trace_id);
        self
    }

    pub fn set_via_bot(mut self, via_bot: bool) -> Self {
        self.via_bot_id = via_bot;
        self
    }

    pub fn get_ip(&self) -> Option<&String> {
        self.ip.as_ref()
    }

    pub fn get_device_type(&self) -> Option<&String> {
        self.device_type.as_ref()
    }

    pub fn get_trace_id(&self) -> Option<&String> {
        self.trace_id.as_ref()
    }

    pub fn is_via_bot(&self) -> bool {
        self.via_bot_id
    }
}
impl EventType {
    pub fn name(&self) -> &'static str {
        match self {
            EventType::SeismicAnomaly => "SeismicAnomaly",
            EventType::ScheduledEvent => "ScheduledEvent",
            EventType::AgentPreference => "AgentPreference",
            EventType::AlertGraph => "AlertGraph",
            EventType::CustomEvent(_) => "CustomEvent",
            EventType::Mentioned(_) => "Mentioned",
            EventType::Scheduled(_) => "Scheduled",
            EventType::PostedMedia(_) => "PostedMedia",
            EventType::CasualMeetup => "CasualMeetup",
            EventType::Conference => "Conference",
            EventType::Workshop => "Workshop",
        }
    }
}
impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {})", self.0, self.1, self.2)
    }
}
#[async_trait]
pub trait EventHandler: Send + Sync {
    fn event_name(&self) -> &'static str;
    async fn handle(&mut self, event: Arc<Event>);
}
#[derive(Clone)]
pub struct EventManager {
    handlers: EventHandlerMap,
    event_sender: mpsc::UnboundedSender<Arc<Event>>,
    worker_handle: Arc<JoinHandle<()>>,
    shutdown_tx: broadcast::Sender<()>,
}
impl EventManager {
    pub fn new(max_concurrent_events: usize) -> Self {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        let handlers = Arc::new(RwLock::new(HashMap::new()));
        let handlers_clone = handlers.clone();
        let (shutdown_tx, _) = broadcast::channel(1);
        let shutdown_rx = shutdown_tx.subscribe();
        let worker_handle = tokio::spawn(async move {
            Self::event_worker(
                event_receiver,
                handlers_clone,
                max_concurrent_events,
                shutdown_rx,
            )
            .await;
        });
        Self {
            handlers,
            event_sender,
            worker_handle: Arc::new(worker_handle),
            shutdown_tx,
        }
    }
    pub async fn shutdown(self) {
        info!("Shutting down EventManager...");
        let _ = self.shutdown_tx.send(());
        if let Ok(handle) = Arc::try_unwrap(self.worker_handle) {
            if let Err(e) = handle.await {
                error!("EventManager worker task panicked during shutdown: {:?}", e);
            }
        } else {
            warn!("Could not exclusively own EventManager worker handle; shutdown may be incomplete. This can happen if EventManager is cloned.");
        }
        info!("EventManager shut down successfully.");
    }
    async fn event_worker(
        mut event_receiver: mpsc::UnboundedReceiver<Arc<Event>>,
        handlers: EventHandlerMap,
        max_concurrent_events: usize,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) {
        let semaphore = Arc::new(Semaphore::new(max_concurrent_events));
        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.recv() => {
                    info!("EventManager worker received shutdown signal. Draining in-flight events.");
                    break;
                }
                Some(event) = event_receiver.recv() => {
                    let permit = match semaphore.clone().acquire_owned().await {
                        Ok(p) => p,
                        Err(_) => {
                            warn!("Semaphore closed, likely during shutdown. Exiting worker loop.");
                            break;
                        }
                    };
                    let event_type = event.event_type().name().to_string();
                    let handlers_read = handlers.read().await;
                    if let Some(event_handlers) = handlers_read.get(&event_type) {
                        let event_handlers = event_handlers.clone();
                        let event_arc = event.clone();
                        tokio::spawn(async move {
                            Self::process_event_with_handlers(event_arc, event_handlers).await;
                            drop(permit);
                        });
                    }
                }
                else => {
                    info!("Event channel closed. Shutting down EventManager worker.");
                    break;
                }
            }
        }
    }
    async fn process_event_with_handlers(
        event: Arc<Event>,
        handlers: Vec<Arc<RwLock<dyn EventHandler>>>,
    ) {
        let handler_futures = handlers.into_iter().map(|handler| {
            let event_clone = event.clone();
            async move {
                let mut handler_guard = handler.write().await;
                handler_guard.handle(event_clone).await;
            }
        });
        join_all(handler_futures).await;
    }
    pub async fn register_handler(&self, handler: Arc<RwLock<dyn EventHandler>>) {
        let event_name = {
            let handler_guard = handler.read().await;
            handler_guard.event_name().to_string()
        };
        let mut handlers = self.handlers.write().await;
        handlers
            .entry(event_name)
            .or_insert_with(Vec::new)
            .push(handler);
    }
    pub async fn handle_event(&self, event: Event) -> Result<(), EventManagerError> {
        let event_arc = Arc::new(event);
        self.event_sender
            .send(event_arc)
            .map_err(|_| EventManagerError::EventChannelClosed)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum EventManagerError {
    #[error("Event channel is closed")]
    EventChannelClosed,
}
pub async fn register_event_handler(
    manager: &EventManager,
    handler: Arc<RwLock<dyn EventHandler>>,
) {
    manager.register_handler(handler).await;
}
pub async fn handle_event(manager: &EventManager, event: Event) -> Result<(), EventManagerError> {
    manager.handle_event(event).await
}
pub struct Neo4jEventHandler {
    neo_client: Arc<Graph>,
    event_name: &'static str,
}
impl Neo4jEventHandler {
    pub fn new(neo_client: Arc<Graph>, event_name: &'static str) -> Self {
        Self {
            neo_client,
            event_name,
        }
    }
}
#[async_trait]
impl EventHandler for Neo4jEventHandler {
    fn event_name(&self) -> &'static str {
        self.event_name
    }
    async fn handle(&mut self, event: Arc<Event>) {
        info!(
            "Handling event {} ({}) with Neo4j",
            event.unique_id(),
            self.event_name()
        );

        if let Err(e) = self.store_event_in_neo4j(&event).await {
            error!("Failed to store event in Neo4j: {}", e);
        }
    }
}

impl Neo4jEventHandler {
    async fn store_event_in_neo4j(
        &self,
        event: &Event,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cypher = "CREATE (e:Event {id: $id, event_type: $event_type, start_time: $start_time, name: $name}) RETURN e".to_string();

        let mut txn = self.neo_client.start_txn().await?;

        let result = txn
            .execute(
                neo4rs::query(&cypher)
                    .param("id", event.unique_id())
                    .param("event_type", event.event_type().name())
                    .param("start_time", event.start_time().unwrap_or(0) as i64)
                    .param("name", event.name()),
            )
            .await;

        match result {
            Ok(_) => {
                txn.commit().await?;
                debug!("Successfully stored event {} in Neo4j", event.unique_id());
            }
            Err(e) => {
                txn.rollback().await?;
                return Err(Box::new(e));
            }
        }

        if let Some(ip) = event.header().get_ip() {
            debug!("Event from IP: {}", ip);
        }

        if event.header().is_via_bot() {
            debug!("Event processed via bot");
        }

        Ok(())
    }
}
