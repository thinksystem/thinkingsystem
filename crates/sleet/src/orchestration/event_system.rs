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

use super::{OrchestrationError, OrchestrationResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

pub struct EventSystem {
    subscribers: Arc<RwLock<HashMap<EventType, Vec<EventSubscriber>>>>,
    event_sender: broadcast::Sender<OrchestrationEvent>,
    _event_receiver: broadcast::Receiver<OrchestrationEvent>,
}

impl EventSystem {
    pub async fn new() -> OrchestrationResult<Self> {
        let (event_sender, event_receiver) = broadcast::channel(1000);

        Ok(Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            _event_receiver: event_receiver,
        })
    }

    pub async fn emit(&mut self, event: OrchestrationEvent) -> OrchestrationResult<()> {
        self.event_sender
            .send(event.clone())
            .map_err(|e| OrchestrationError::EventError(format!("Failed to emit event: {e}")))?;

        let event_type = event.event_type();
        let subscribers = self.subscribers.read().await;

        if let Some(event_subscribers) = subscribers.get(&event_type) {
            for subscriber in event_subscribers {
                if let Err(e) = subscriber.notify(&event).await {
                    log::warn!("Failed to notify subscriber {}: {}", subscriber.id, e);
                }
            }
        }

        Ok(())
    }

    pub async fn subscribe(
        &self,
        event_type: EventType,
        subscriber: EventSubscriber,
    ) -> OrchestrationResult<()> {
        let mut subscribers = self.subscribers.write().await;
        subscribers
            .entry(event_type)
            .or_insert_with(Vec::new)
            .push(subscriber);
        Ok(())
    }

    pub fn get_event_receiver(&self) -> broadcast::Receiver<OrchestrationEvent> {
        self.event_sender.subscribe()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrchestrationEvent {
    CoordinatorInitialised {
        config: super::coordinator::OrchestrationConfig,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    SessionStarted {
        session_id: String,
        flow_id: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    SessionCompleted {
        session_id: String,
        final_result: Value,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    BlockExecutionStarted {
        session_id: String,
        block_id: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    BlockExecutionCompleted {
        session_id: String,
        block_id: String,
        result: Value,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    AgentInteractionCompleted {
        session_id: String,
        agent_id: String,
        result: Value,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    LLMProcessingCompleted {
        session_id: String,
        llm_config: String,
        result: Value,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    TaskExecutionCompleted {
        session_id: String,
        task_id: String,
        result: Value,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    ResourceAllocated {
        session_id: String,
        resource_type: String,
        resource_id: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    ResourceReleased {
        session_id: String,
        resource_type: String,
        resource_id: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    ErrorOccurred {
        session_id: String,
        error: OrchestrationError,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    PerformanceMetric {
        session_id: String,
        metric_name: String,
        metric_value: f64,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    StateChanged {
        session_id: String,
        previous_state: Value,
        new_state: Value,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}

impl OrchestrationEvent {
    pub fn event_type(&self) -> EventType {
        match self {
            OrchestrationEvent::CoordinatorInitialised { .. } => EventType::CoordinatorInitialised,
            OrchestrationEvent::SessionStarted { .. } => EventType::SessionStarted,
            OrchestrationEvent::SessionCompleted { .. } => EventType::SessionCompleted,
            OrchestrationEvent::BlockExecutionStarted { .. } => EventType::BlockExecutionStarted,
            OrchestrationEvent::BlockExecutionCompleted { .. } => {
                EventType::BlockExecutionCompleted
            }
            OrchestrationEvent::AgentInteractionCompleted { .. } => {
                EventType::AgentInteractionCompleted
            }
            OrchestrationEvent::LLMProcessingCompleted { .. } => EventType::LLMProcessingCompleted,
            OrchestrationEvent::TaskExecutionCompleted { .. } => EventType::TaskExecutionCompleted,
            OrchestrationEvent::ResourceAllocated { .. } => EventType::ResourceAllocated,
            OrchestrationEvent::ResourceReleased { .. } => EventType::ResourceReleased,
            OrchestrationEvent::ErrorOccurred { .. } => EventType::ErrorOccurred,
            OrchestrationEvent::PerformanceMetric { .. } => EventType::PerformanceMetric,
            OrchestrationEvent::StateChanged { .. } => EventType::StateChanged,
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        match self {
            OrchestrationEvent::CoordinatorInitialised { .. } => None,
            OrchestrationEvent::SessionStarted { session_id, .. }
            | OrchestrationEvent::SessionCompleted { session_id, .. }
            | OrchestrationEvent::BlockExecutionStarted { session_id, .. }
            | OrchestrationEvent::BlockExecutionCompleted { session_id, .. }
            | OrchestrationEvent::AgentInteractionCompleted { session_id, .. }
            | OrchestrationEvent::LLMProcessingCompleted { session_id, .. }
            | OrchestrationEvent::TaskExecutionCompleted { session_id, .. }
            | OrchestrationEvent::ResourceAllocated { session_id, .. }
            | OrchestrationEvent::ResourceReleased { session_id, .. }
            | OrchestrationEvent::ErrorOccurred { session_id, .. }
            | OrchestrationEvent::PerformanceMetric { session_id, .. }
            | OrchestrationEvent::StateChanged { session_id, .. } => Some(session_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    CoordinatorInitialised,
    SessionStarted,
    SessionCompleted,
    BlockExecutionStarted,
    BlockExecutionCompleted,
    AgentInteractionCompleted,
    LLMProcessingCompleted,
    TaskExecutionCompleted,
    ResourceAllocated,
    ResourceReleased,
    ErrorOccurred,
    PerformanceMetric,
    StateChanged,
    All,
}

pub struct EventSubscriber {
    pub id: String,
    pub handler: Arc<dyn EventHandler>,
}

impl EventSubscriber {
    pub fn new(id: String, handler: Arc<dyn EventHandler>) -> Self {
        Self { id, handler }
    }

    pub async fn notify(
        &self,
        event: &OrchestrationEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.handler.handle_event(event).await
    }
}

#[async_trait::async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle_event(
        &self,
        event: &OrchestrationEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

pub struct LoggerEventHandler;

#[async_trait::async_trait]
impl EventHandler for LoggerEventHandler {
    async fn handle_event(
        &self,
        event: &OrchestrationEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match event {
            OrchestrationEvent::CoordinatorInitialised { timestamp, .. } => {
                log::info!("Orchestration coordinator initialised at {timestamp}");
            }
            OrchestrationEvent::SessionStarted {
                session_id,
                flow_id,
                timestamp,
            } => {
                log::info!("Session {session_id} started for flow {flow_id} at {timestamp}");
            }
            OrchestrationEvent::SessionCompleted {
                session_id,
                timestamp,
                ..
            } => {
                log::info!("Session {session_id} completed at {timestamp}");
            }
            OrchestrationEvent::ErrorOccurred {
                session_id,
                error,
                timestamp,
            } => {
                log::error!("Error in session {session_id} at {timestamp}: {error}");
            }
            _ => {
                log::debug!("Event: {event:?}");
            }
        }
        Ok(())
    }
}

pub struct MetricsEventHandler {
    metrics: Arc<RwLock<HashMap<String, Vec<MetricValue>>>>,
}

impl Default for MetricsEventHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsEventHandler {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get_metrics(&self) -> HashMap<String, Vec<MetricValue>> {
        let metrics = self.metrics.read().await;
        metrics.clone()
    }
}

#[async_trait::async_trait]
impl EventHandler for MetricsEventHandler {
    async fn handle_event(
        &self,
        event: &OrchestrationEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut metrics = self.metrics.write().await;

        match event {
            OrchestrationEvent::PerformanceMetric {
                metric_name,
                metric_value,
                timestamp,
                ..
            } => {
                let metric = MetricValue {
                    value: *metric_value,
                    timestamp: *timestamp,
                };
                metrics
                    .entry(metric_name.clone())
                    .or_insert_with(Vec::new)
                    .push(metric);
            }
            OrchestrationEvent::BlockExecutionCompleted {
                session_id,
                block_id,
                timestamp,
                ..
            } => {
                let metric = MetricValue {
                    value: 1.0,
                    timestamp: *timestamp,
                };
                let key = format!("block_executions.{session_id}.{block_id}");
                metrics.entry(key).or_insert_with(Vec::new).push(metric);
            }
            _ => {}
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValue {
    pub value: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub struct StateTrackingEventHandler {
    session_states: Arc<RwLock<HashMap<String, SessionState>>>,
}

impl Default for StateTrackingEventHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl StateTrackingEventHandler {
    pub fn new() -> Self {
        Self {
            session_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get_session_state(&self, session_id: &str) -> Option<SessionState> {
        let states = self.session_states.read().await;
        states.get(session_id).cloned()
    }
}

#[async_trait::async_trait]
impl EventHandler for StateTrackingEventHandler {
    async fn handle_event(
        &self,
        event: &OrchestrationEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut states = self.session_states.write().await;

        if let Some(session_id) = event.session_id() {
            let state = states
                .entry(session_id.to_string())
                .or_insert_with(|| SessionState::new(session_id.to_string()));

            match event {
                OrchestrationEvent::SessionStarted {
                    flow_id, timestamp, ..
                } => {
                    state.status = SessionStateStatus::Running;
                    state.flow_id = Some(flow_id.clone());
                    state.started_at = Some(*timestamp);
                }
                OrchestrationEvent::SessionCompleted { timestamp, .. } => {
                    state.status = SessionStateStatus::Completed;
                    state.completed_at = Some(*timestamp);
                }
                OrchestrationEvent::BlockExecutionStarted { block_id, .. } => {
                    state.current_block = Some(block_id.clone());
                    state.executed_blocks.push(block_id.clone());
                }
                OrchestrationEvent::ErrorOccurred { error, .. } => {
                    state.status = SessionStateStatus::Failed;
                    state.last_error = Some(error.to_string());
                }
                _ => {}
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub status: SessionStateStatus,
    pub flow_id: Option<String>,
    pub current_block: Option<String>,
    pub executed_blocks: Vec<String>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_error: Option<String>,
}

impl SessionState {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            status: SessionStateStatus::Pending,
            flow_id: None,
            current_block: None,
            executed_blocks: Vec::new(),
            started_at: None,
            completed_at: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStateStatus {
    Pending,
    Running,
    AwaitingInput,
    Completed,
    Failed,
}
