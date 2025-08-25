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

use crate::flows::flowgorithm::Binder;
use crate::flows::state_metrics::StateMetrics;
use chrono::{DateTime, Utc};
use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use serde::{Deserializer, Serializer};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{Notify, RwLock};
use tracing::{debug, error, instrument, warn};
use uuid::Uuid;
#[derive(Error, Debug)]
pub enum StateError {
    #[error("Lock acquisition failed: {reason}")]
    LockError { reason: String },
    #[error("Lock already held by: {owner}")]
    AlreadyLocked { owner: String },
    #[error("Lock not found: {flow_id}")]
    LockNotFound { flow_id: String },
    #[error("Invalid lock owner: expected={expected}, actual={actual}")]
    InvalidLockOwner { expected: String, actual: String },
    #[error("Lock expired for flow: {flow_id}")]
    LockExpired { flow_id: String },
    #[error("State validation error: {0}")]
    ValidationError(String),
    #[error("Barrier error: {0}")]
    BarrierError(String),
    #[error("Barrier timeout: {barrier_id}")]
    BarrierTimeout { barrier_id: String },
    #[error("State version mismatch: expected={expected}, actual={actual}")]
    VersionMismatch { expected: u64, actual: u64 },
    #[error("Compare and swap failed: current value differs")]
    CompareAndSwapFailed,
}
#[derive(Debug, Clone)]
pub struct UnifiedState {
    pub user_id: String,
    pub operator_id: String,
    pub channel_id: String,
    pub flow_id: Option<String>,
    pub block_id: Option<String>,
    pub data: HashMap<String, Value>,
    pub metadata: HashMap<String, Value>,
    pub flow_context: Option<Value>,
    pub skill_context: Option<Value>,
    pub binder: Option<Arc<Binder>>,
    pub version: u64,
    pub previous_versions: Vec<StateSnapshot>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metrics: Option<StateMetrics>,
    pub checksum: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub version: u64,
    pub data: HashMap<String, Value>,
    pub metadata: HashMap<String, Value>,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_context: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_context: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockType {
    Shared,
    Exclusive,
}
#[derive(Debug, Clone)]
pub struct FlowLock {
    pub owner: String,
    pub timestamp: DateTime<Utc>,
    pub lock_type: LockType,
}
#[derive(Debug, Clone)]
pub struct BarrierState {
    pub barrier_id: String,
    pub expected_participants: usize,
    pub arrived_participants: HashSet<String>,
    pub created_at: DateTime<Utc>,
    pub timeout_duration: Duration,
    pub notify: Arc<Notify>,
    pub completed: bool,
}
#[derive(Debug, Clone)]
pub struct StateChangeNotification {
    pub state_id: String,
    pub change_type: StateChangeType,
    pub old_value: Option<Value>,
    pub new_value: Value,
    pub timestamp: DateTime<Utc>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum StateChangeType {
    DataUpdate,
    MetadataUpdate,
    FlowContextUpdate,
    SkillContextUpdate,
    AtomicIncrement,
    AtomicAppend,
    CompareAndSwap,
}
pub type StateChangeObserver = Arc<dyn Fn(StateChangeNotification) + Send + Sync>;
pub struct ConcurrencyManager {
    flow_locks: RwLock<HashMap<String, FlowLock>>,
    barriers: RwLock<HashMap<String, BarrierState>>,
    timeout_duration: Duration,
    state_observers: RwLock<Vec<StateChangeObserver>>,
}
impl ConcurrencyManager {
    pub fn new(timeout_secs: u64) -> Self {
        Self {
            flow_locks: RwLock::new(HashMap::new()),
            barriers: RwLock::new(HashMap::new()),
            timeout_duration: Duration::from_secs(timeout_secs),
            state_observers: RwLock::new(Vec::new()),
        }
    }
    #[instrument(skip(self), fields(flow_id = %flow_id, owner = %owner))]
    pub async fn acquire_lock(
        &self,
        flow_id: &str,
        owner: String,
        lock_type: LockType,
    ) -> Result<(), StateError> {
        let mut locks = self.flow_locks.write().await;
        let now = Utc::now();
        self.cleanup_expired_locks(&mut locks, now);
        match locks.get(flow_id) {
            Some(lock) if lock.timestamp + self.timeout_duration > now => {
                Err(StateError::AlreadyLocked {
                    owner: lock.owner.clone(),
                })
            }
            _ => {
                locks.insert(
                    flow_id.to_string(),
                    FlowLock {
                        owner,
                        timestamp: now,
                        lock_type,
                    },
                );
                debug!("Lock acquired for flow: {}", flow_id);
                Ok(())
            }
        }
    }
    #[instrument(skip(self), fields(flow_id = %flow_id, owner = %owner))]
    pub async fn release_lock(&self, flow_id: &str, owner: &str) -> Result<(), StateError> {
        let mut locks = self.flow_locks.write().await;
        match locks.get(flow_id) {
            Some(lock) if lock.owner == owner => {
                locks.remove(flow_id);
                debug!("Lock released for flow: {}", flow_id);
                Ok(())
            }
            Some(lock) => Err(StateError::InvalidLockOwner {
                expected: owner.to_string(),
                actual: lock.owner.clone(),
            }),
            None => Err(StateError::LockNotFound {
                flow_id: flow_id.to_string(),
            }),
        }
    }
    #[instrument(skip(self), fields(barrier_id = %barrier_id, expected_participants = %expected_participants))]
    pub async fn create_barrier(
        &self,
        barrier_id: String,
        expected_participants: usize,
        timeout_duration: Duration,
    ) -> Result<(), StateError> {
        let mut barriers = self.barriers.write().await;
        if barriers.contains_key(&barrier_id) {
            return Err(StateError::BarrierError(format!(
                "Barrier {barrier_id} already exists"
            )));
        }
        let barrier_state = BarrierState {
            barrier_id: barrier_id.clone(),
            expected_participants,
            arrived_participants: HashSet::new(),
            created_at: Utc::now(),
            timeout_duration,
            notify: Arc::new(Notify::new()),
            completed: false,
        };
        barriers.insert(barrier_id.clone(), barrier_state);
        debug!(
            "Barrier created: {} (expecting {} participants)",
            barrier_id, expected_participants
        );
        Ok(())
    }
    #[instrument(skip(self), fields(barrier_id = %barrier_id, participant_id = %participant_id))]
    pub async fn wait_at_barrier(
        &self,
        barrier_id: &str,
        participant_id: String,
    ) -> Result<(), StateError> {
        let notify = {
            let mut barriers = self.barriers.write().await;
            let barrier = barriers.get_mut(barrier_id).ok_or_else(|| {
                StateError::BarrierError(format!("Barrier {barrier_id} not found"))
            })?;
            if barrier.completed {
                return Ok(());
            }
            if barrier.created_at + barrier.timeout_duration < Utc::now() {
                barrier.completed = true;
                return Err(StateError::BarrierTimeout {
                    barrier_id: barrier_id.to_string(),
                });
            }
            barrier.arrived_participants.insert(participant_id.clone());
            debug!(
                "Participant {} arrived at barrier {} ({}/{})",
                participant_id,
                barrier_id,
                barrier.arrived_participants.len(),
                barrier.expected_participants
            );
            if barrier.arrived_participants.len() >= barrier.expected_participants {
                barrier.completed = true;
                barrier.notify.notify_waiters();
                debug!(
                    "Barrier {} completed with all {} participants",
                    barrier_id, barrier.expected_participants
                );
                return Ok(());
            }
            barrier.notify.clone()
        };
        let timeout_future = tokio::time::sleep(
            self.barriers
                .read()
                .await
                .get(barrier_id)
                .map(|b| b.timeout_duration)
                .unwrap_or(self.timeout_duration),
        );
        tokio::select! {
            _ = notify.notified() => {
                debug!("Participant {} released from barrier {}", participant_id, barrier_id);
                Ok(())
            }
            _ = timeout_future => {
                warn!("Participant {} timed out waiting for barrier {}", participant_id, barrier_id);
                Err(StateError::BarrierTimeout { barrier_id: barrier_id.to_string() })
            }
        }
    }
    #[instrument(skip(self), fields(barrier_id = %barrier_id))]
    pub async fn release_barrier(&self, barrier_id: &str) -> Result<(), StateError> {
        let mut barriers = self.barriers.write().await;
        if let Some(mut barrier) = barriers.remove(barrier_id) {
            barrier.completed = true;
            barrier.notify.notify_waiters();
            debug!("Barrier {} manually released", barrier_id);
            Ok(())
        } else {
            Err(StateError::BarrierError(format!(
                "Barrier {barrier_id} not found"
            )))
        }
    }
    pub async fn cleanup_expired_barriers(&self) {
        let mut barriers = self.barriers.write().await;
        let now = Utc::now();
        let expired_barriers: Vec<String> = barriers
            .iter()
            .filter(|(_, barrier)| barrier.created_at + barrier.timeout_duration < now)
            .map(|(id, _)| id.clone())
            .collect();
        for barrier_id in expired_barriers {
            if let Some(mut barrier) = barriers.remove(&barrier_id) {
                barrier.completed = true;
                barrier.notify.notify_waiters();
                warn!("Cleaned up expired barrier: {}", barrier_id);
            }
        }
    }
    pub async fn add_observer(&self, observer: StateChangeObserver) {
        let mut observers = self.state_observers.write().await;
        observers.push(observer);
    }
    pub async fn notify_observers(&self, notification: StateChangeNotification) {
        let observers = self.state_observers.read().await;
        for observer in observers.iter() {
            observer(notification.clone());
        }
    }
    fn cleanup_expired_locks(&self, locks: &mut HashMap<String, FlowLock>, now: DateTime<Utc>) {
        let expired_keys: Vec<String> = locks
            .iter()
            .filter(|(_, lock)| lock.timestamp + self.timeout_duration < now)
            .map(|(key, _)| key.clone())
            .collect();
        for key in expired_keys {
            locks.remove(&key);
            warn!("Cleaned up expired lock for flow: {}", key);
        }
    }
    pub async fn is_locked(&self, flow_id: &str) -> bool {
        let locks = self.flow_locks.read().await;
        let now = Utc::now();
        locks
            .get(flow_id)
            .map(|lock| lock.timestamp + self.timeout_duration > now)
            .unwrap_or(false)
    }
}
impl UnifiedState {
    pub fn new(user_id: String, operator_id: String, channel_id: String) -> Self {
        Self {
            user_id,
            operator_id,
            channel_id,
            flow_id: None,
            block_id: None,
            data: HashMap::new(),
            metadata: HashMap::new(),
            flow_context: None,
            skill_context: None,
            binder: None,
            version: 1,
            previous_versions: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metrics: Some(StateMetrics::default()),
            checksum: None,
        }
    }
    pub fn with_flow(mut self, flow_id: String) -> Self {
        self.flow_id = Some(flow_id);
        self.updated_at = Utc::now();
        self
    }
    pub fn with_block(mut self, block_id: String) -> Self {
        self.block_id = Some(block_id);
        self.updated_at = Utc::now();
        self
    }
    fn create_snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            version: self.version,
            data: self.data.clone(),
            metadata: self.metadata.clone(),
            timestamp: self.updated_at,
            flow_context: self.flow_context.clone(),
            skill_context: self.skill_context.clone(),
            checksum: self.checksum.clone(),
        }
    }
    fn increment_version(&mut self) {
        if self.previous_versions.len() >= 10 {
            self.previous_versions.remove(0);
        }
        self.previous_versions.push(self.create_snapshot());
        self.version += 1;
        self.updated_at = Utc::now();
        if let Some(m) = self.metrics.as_mut() {
            m.modification_count += 1;
        }
        self.update_checksum();
        if let Some(m) = self.metrics.as_mut() {
            m.recalc_sizes(&self.data, &self.metadata);
        }
    }
    #[instrument(skip(self, value), fields(key = %key))]
    pub fn set_data(&mut self, key: String, value: Value) {
        self.increment_version();
        let old_value = self.data.get(&key).cloned();
        self.data.insert(key.clone(), value.clone());
        debug!(
            "Data updated for key: {} (old: {:?}, new: {:?})",
            key, old_value, value
        );
        if let Some(m) = self.metrics.as_mut() {
            m.access_count += 1;
        }
        if let Some(m) = self.metrics.as_mut() {
            m.recalc_sizes(&self.data, &self.metadata);
        }
    }
    pub fn get_data(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }
    pub fn set_metadata(&mut self, key: String, value: Value) {
        self.increment_version();
        self.metadata.insert(key, value);
        if let Some(m) = self.metrics.as_mut() {
            m.modification_count += 1;
        }
        if let Some(m) = self.metrics.as_mut() {
            m.recalc_sizes(&self.data, &self.metadata);
        }
    }
    pub fn get_metadata(&self, key: &str) -> Option<&Value> {
        self.metadata.get(key)
    }
    pub fn set_flow_context(&mut self, context: Value) {
        self.increment_version();
        self.flow_context = Some(context);
    }
    pub fn set_skill_context(&mut self, context: Value) {
        self.increment_version();
        self.skill_context = Some(context);
    }
    pub fn set_binder(&mut self, binder: Binder) {
        self.binder = Some(Arc::new(binder));
        self.updated_at = Utc::now();
    }
    pub fn clear_flow_data(&mut self) {
        self.increment_version();
        self.flow_id = None;
        self.block_id = None;
        self.flow_context = None;
        self.binder = None;
        self.data.clear();
        debug!("Flow data cleared");
        if let Some(m) = self.metrics.as_mut() {
            m.recalc_sizes(&self.data, &self.metadata);
        }
    }
    #[instrument(skip(self), fields(key = %key, delta = %delta))]
    pub fn atomic_increment(&mut self, key: &str, delta: i64) -> Result<i64, StateError> {
        self.increment_version();
        let current_value = self.data.get(key).and_then(|v| v.as_i64()).unwrap_or(0);
        let new_value = current_value + delta;
        self.data.insert(
            key.to_string(),
            Value::Number(serde_json::Number::from(new_value)),
        );
        debug!(
            "Atomic increment on key {}: {} + {} = {}",
            key, current_value, delta, new_value
        );
        if let Some(m) = self.metrics.as_mut() {
            m.recalc_sizes(&self.data, &self.metadata);
        }
        Ok(new_value)
    }
    #[instrument(skip(self, value), fields(key = %key))]
    pub fn atomic_append(&mut self, key: &str, value: Value) -> Result<usize, StateError> {
        self.increment_version();
        let mut array = self
            .data
            .get(key)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_else(Vec::new);
        array.push(value);
        let new_length = array.len();
        self.data.insert(key.to_string(), Value::Array(array));
        debug!("Atomic append to key {}: new length = {}", key, new_length);
        if let Some(m) = self.metrics.as_mut() {
            m.recalc_sizes(&self.data, &self.metadata);
        }
        Ok(new_length)
    }
    #[instrument(skip(self, expected, new_value), fields(key = %key))]
    pub fn compare_and_swap(
        &mut self,
        key: &str,
        expected: &Value,
        new_value: Value,
    ) -> Result<bool, StateError> {
        let current_value = self.data.get(key);
        match current_value {
            Some(current) if current == expected => {
                self.increment_version();
                self.data.insert(key.to_string(), new_value);
                debug!("Compare and swap succeeded for key: {}", key);
                if let Some(m) = self.metrics.as_mut() {
                    m.recalc_sizes(&self.data, &self.metadata);
                }
                Ok(true)
            }
            _ => {
                debug!("Compare and swap failed for key: {} (value mismatch)", key);
                Err(StateError::CompareAndSwapFailed)
            }
        }
    }
    pub fn rollback_to_version(&mut self, target_version: u64) -> Result<(), StateError> {
        let snapshot = self
            .previous_versions
            .iter()
            .find(|s| s.version == target_version)
            .ok_or_else(|| {
                StateError::ValidationError(format!("Version {target_version} not found"))
            })?;
        self.data = snapshot.data.clone();
        self.metadata = snapshot.metadata.clone();
        self.version = snapshot.version;
        self.updated_at = Utc::now();
        self.flow_context = snapshot.flow_context.clone();
        self.skill_context = snapshot.skill_context.clone();
        self.checksum = snapshot.checksum.clone();
        self.previous_versions
            .retain(|s| s.version < target_version);
        debug!("Rolled back to version: {}", target_version);
        if let Some(m) = self.metrics.as_mut() {
            m.recalc_sizes(&self.data, &self.metadata);
        }
        Ok(())
    }
    pub fn get_version_history(&self) -> &[StateSnapshot] {
        &self.previous_versions
    }
    pub fn get_current_version(&self) -> u64 {
        self.version
    }
    pub fn is_in_flow(&self) -> bool {
        self.flow_id.is_some()
    }
    pub fn get_flow_state(&self) -> HashMap<String, Value> {
        self.data.clone()
    }
    pub fn update_state(&mut self, data: HashMap<String, Value>) {
        self.increment_version();
        self.data = data;
    }
    pub fn get_state_identifier(&self) -> String {
        format!(
            "{}:{}:{}",
            self.user_id,
            self.channel_id,
            self.flow_id.clone().unwrap_or_default()
        )
    }
    pub fn is_stale(&self, timeout: Duration) -> bool {
        self.updated_at + timeout < Utc::now()
    }
    pub fn validate(&self) -> Result<(), StateError> {
        if self.user_id.is_empty() {
            return Err(StateError::ValidationError(
                "user_id cannot be empty".to_string(),
            ));
        }
        if self.channel_id.is_empty() {
            return Err(StateError::ValidationError(
                "channel_id cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}
impl UnifiedState {
    fn update_checksum(&mut self) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.version.hash(&mut hasher);
        for (k, v) in &self.data {
            k.hash(&mut hasher);
            if let Ok(js) = serde_json::to_string(v) {
                js.hash(&mut hasher);
            }
        }
        self.checksum = Some(format!("{:x}", hasher.finish()));
    }
}
#[async_trait::async_trait]
pub trait StateManager: Send + Sync {
    async fn save_state(&self, state: &UnifiedState) -> Result<(), StateError>;
    async fn load_state(&self, identifier: &str) -> Result<UnifiedState, StateError>;
    async fn delete_state(&self, identifier: &str) -> Result<(), StateError>;
    async fn cleanup_stale_states(&self, timeout: Duration) -> Result<Vec<String>, StateError>;
    async fn atomic_increment(
        &self,
        identifier: &str,
        key: &str,
        delta: i64,
    ) -> Result<i64, StateError>;
    async fn atomic_append(
        &self,
        identifier: &str,
        key: &str,
        value: Value,
    ) -> Result<usize, StateError>;
    async fn compare_and_swap(
        &self,
        identifier: &str,
        key: &str,
        expected: &Value,
        new_value: Value,
    ) -> Result<bool, StateError>;
    async fn get_state_version(&self, identifier: &str) -> Result<u64, StateError>;
    async fn rollback_state(&self, identifier: &str, target_version: u64)
        -> Result<(), StateError>;
}
pub struct InMemoryStateManager {
    states: Arc<RwLock<HashMap<String, UnifiedState>>>,
    concurrency_manager: Arc<ConcurrencyManager>,
}
impl Default for InMemoryStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryStateManager {
    pub fn new() -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            concurrency_manager: Arc::new(ConcurrencyManager::new(300)),
        }
    }
    pub async fn acquire_state_lock(
        &self,
        state_id: &str,
        owner: String,
    ) -> Result<(), StateError> {
        self.concurrency_manager
            .acquire_lock(state_id, owner, LockType::Exclusive)
            .await
    }
    pub async fn release_state_lock(&self, state_id: &str, owner: &str) -> Result<(), StateError> {
        self.concurrency_manager.release_lock(state_id, owner).await
    }
    pub async fn create_barrier(
        &self,
        barrier_id: String,
        expected_participants: usize,
        timeout: Duration,
    ) -> Result<(), StateError> {
        self.concurrency_manager
            .create_barrier(barrier_id, expected_participants, timeout)
            .await
    }
    pub async fn wait_at_barrier(
        &self,
        barrier_id: &str,
        participant_id: String,
    ) -> Result<(), StateError> {
        self.concurrency_manager
            .wait_at_barrier(barrier_id, participant_id)
            .await
    }
    pub async fn release_barrier(&self, barrier_id: &str) -> Result<(), StateError> {
        self.concurrency_manager.release_barrier(barrier_id).await
    }
    pub async fn add_state_observer(&self, observer: StateChangeObserver) {
        self.concurrency_manager.add_observer(observer).await;
    }
    async fn notify_state_change(&self, notification: StateChangeNotification) {
        self.concurrency_manager
            .notify_observers(notification)
            .await;
    }
    pub async fn start_cleanup_task(&self, cleanup_interval: Duration) {
        let concurrency_manager = self.concurrency_manager.clone();
        let states = self.states.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_interval);
            loop {
                interval.tick().await;
                concurrency_manager.cleanup_expired_barriers().await;
                let stale_timeout = Duration::from_secs(3600);
                let states_read = states.read().await;
                let stale_keys: Vec<String> = states_read
                    .iter()
                    .filter(|(_, state)| state.is_stale(stale_timeout))
                    .map(|(key, _)| key.clone())
                    .collect();
                drop(states_read);
                if !stale_keys.is_empty() {
                    let mut states_write = states.write().await;
                    for key in &stale_keys {
                        states_write.remove(key);
                    }
                    debug!("Cleaned up {} stale states", stale_keys.len());
                }
            }
        });
    }
}
#[async_trait::async_trait]
impl StateManager for InMemoryStateManager {
    #[instrument(skip(self, state), fields(state_id = %state.get_state_identifier()))]
    async fn save_state(&self, state: &UnifiedState) -> Result<(), StateError> {
        let key = state.get_state_identifier();
        state.validate()?;
        let mut states = self.states.write().await;
        states.insert(key.clone(), state.clone());
        debug!("State saved: {} (version: {})", key, state.version);
        Ok(())
    }
    #[instrument(skip(self), fields(identifier = %identifier))]
    async fn load_state(&self, identifier: &str) -> Result<UnifiedState, StateError> {
        let states = self.states.read().await;
        states
            .get(identifier)
            .cloned()
            .ok_or_else(|| StateError::ValidationError(format!("State not found: {identifier}")))
    }
    #[instrument(skip(self), fields(identifier = %identifier))]
    async fn delete_state(&self, identifier: &str) -> Result<(), StateError> {
        let mut states = self.states.write().await;
        states
            .remove(identifier)
            .map(|_| ())
            .ok_or_else(|| StateError::ValidationError(format!("State not found: {identifier}")))
    }
    #[instrument(skip(self), fields(timeout = ?timeout))]
    async fn cleanup_stale_states(&self, timeout: Duration) -> Result<Vec<String>, StateError> {
        let mut states = self.states.write().await;
        let stale_keys: Vec<String> = states
            .iter()
            .filter(|(_, state)| state.is_stale(timeout))
            .map(|(key, _)| key.clone())
            .collect();
        for key in &stale_keys {
            states.remove(key);
        }
        debug!("Cleaned up {} stale states", stale_keys.len());
        Ok(stale_keys)
    }
    async fn atomic_increment(
        &self,
        identifier: &str,
        key: &str,
        delta: i64,
    ) -> Result<i64, StateError> {
        let mut states = self.states.write().await;
        let state = states
            .get_mut(identifier)
            .ok_or_else(|| StateError::ValidationError(format!("State not found: {identifier}")))?;
        let result = state.atomic_increment(key, delta)?;
        let notification = StateChangeNotification {
            state_id: identifier.to_string(),
            change_type: StateChangeType::AtomicIncrement,
            old_value: Some(Value::Number(serde_json::Number::from(result - delta))),
            new_value: Value::Number(serde_json::Number::from(result)),
            timestamp: Utc::now(),
        };
        drop(states);
        self.notify_state_change(notification).await;
        Ok(result)
    }
    async fn atomic_append(
        &self,
        identifier: &str,
        key: &str,
        value: Value,
    ) -> Result<usize, StateError> {
        let mut states = self.states.write().await;
        let state = states
            .get_mut(identifier)
            .ok_or_else(|| StateError::ValidationError(format!("State not found: {identifier}")))?;
        let old_array = state.data.get(key).cloned();
        let result = state.atomic_append(key, value.clone())?;
        let notification = StateChangeNotification {
            state_id: identifier.to_string(),
            change_type: StateChangeType::AtomicAppend,
            old_value: old_array,
            new_value: state.data.get(key).cloned().unwrap_or(Value::Null),
            timestamp: Utc::now(),
        };
        drop(states);
        self.notify_state_change(notification).await;
        Ok(result)
    }
    async fn compare_and_swap(
        &self,
        identifier: &str,
        key: &str,
        expected: &Value,
        new_value: Value,
    ) -> Result<bool, StateError> {
        let mut states = self.states.write().await;
        let state = states
            .get_mut(identifier)
            .ok_or_else(|| StateError::ValidationError(format!("State not found: {identifier}")))?;
        let old_value = state.data.get(key).cloned();
        let result = state.compare_and_swap(key, expected, new_value.clone());
        match result {
            Ok(true) => {
                let notification = StateChangeNotification {
                    state_id: identifier.to_string(),
                    change_type: StateChangeType::CompareAndSwap,
                    old_value,
                    new_value,
                    timestamp: Utc::now(),
                };
                drop(states);
                self.notify_state_change(notification).await;
                Ok(true)
            }
            Ok(false) => Ok(false),
            Err(e) => Err(e),
        }
    }
    async fn get_state_version(&self, identifier: &str) -> Result<u64, StateError> {
        let states = self.states.read().await;
        let state = states
            .get(identifier)
            .ok_or_else(|| StateError::ValidationError(format!("State not found: {identifier}")))?;
        Ok(state.get_current_version())
    }
    async fn rollback_state(
        &self,
        identifier: &str,
        target_version: u64,
    ) -> Result<(), StateError> {
        let mut states = self.states.write().await;
        let state = states
            .get_mut(identifier)
            .ok_or_else(|| StateError::ValidationError(format!("State not found: {identifier}")))?;
        state.rollback_to_version(target_version)?;
        debug!(
            "State {} rolled back to version {}",
            identifier, target_version
        );
        Ok(())
    }
}
#[async_trait::async_trait]
pub trait DistributedStateCoordinator: Send + Sync {
    async fn register_node(&self, node_id: String) -> Result<(), StateError>;
    async fn unregister_node(&self, node_id: &str) -> Result<(), StateError>;
    async fn coordinate_atomic_operation(
        &self,
        operation: DistributedOperation,
    ) -> Result<Value, StateError>;
    async fn synchronise_state(&self, state_id: &str) -> Result<UnifiedState, StateError>;
    async fn broadcast_state_change(
        &self,
        notification: StateChangeNotification,
    ) -> Result<(), StateError>;
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DistributedOperation {
    AtomicIncrement {
        state_id: String,
        key: String,
        delta: i64,
    },
    AtomicAppend {
        state_id: String,
        key: String,
        value: Value,
    },
    CompareAndSwap {
        state_id: String,
        key: String,
        expected: Value,
        new_value: Value,
    },
}
pub struct LocalDistributedCoordinator {
    node_id: String,
    registered_nodes: Arc<RwLock<HashSet<String>>>,
    state_manager: Arc<dyn StateManager>,
}
impl LocalDistributedCoordinator {
    pub fn new(node_id: String, state_manager: Arc<dyn StateManager>) -> Self {
        Self {
            node_id,
            registered_nodes: Arc::new(RwLock::new(HashSet::new())),
            state_manager,
        }
    }
}
#[async_trait::async_trait]
impl DistributedStateCoordinator for LocalDistributedCoordinator {
    async fn register_node(&self, node_id: String) -> Result<(), StateError> {
        let mut nodes = self.registered_nodes.write().await;
        nodes.insert(node_id.clone());
        debug!("Coordinator {} registered node: {}", self.node_id, node_id);
        Ok(())
    }
    async fn unregister_node(&self, node_id: &str) -> Result<(), StateError> {
        let mut nodes = self.registered_nodes.write().await;
        nodes.remove(node_id);
        debug!(
            "Coordinator {} unregistered node: {}",
            self.node_id, node_id
        );
        Ok(())
    }
    async fn coordinate_atomic_operation(
        &self,
        operation: DistributedOperation,
    ) -> Result<Value, StateError> {
        match operation {
            DistributedOperation::AtomicIncrement {
                state_id,
                key,
                delta,
            } => {
                let result = self
                    .state_manager
                    .atomic_increment(&state_id, &key, delta)
                    .await?;
                Ok(Value::Number(serde_json::Number::from(result)))
            }
            DistributedOperation::AtomicAppend {
                state_id,
                key,
                value,
            } => {
                let result = self
                    .state_manager
                    .atomic_append(&state_id, &key, value)
                    .await?;
                Ok(Value::Number(serde_json::Number::from(result)))
            }
            DistributedOperation::CompareAndSwap {
                state_id,
                key,
                expected,
                new_value,
            } => {
                let result = self
                    .state_manager
                    .compare_and_swap(&state_id, &key, &expected, new_value)
                    .await?;
                Ok(Value::Bool(result))
            }
        }
    }
    async fn synchronise_state(&self, state_id: &str) -> Result<UnifiedState, StateError> {
        self.state_manager.load_state(state_id).await
    }
    async fn broadcast_state_change(
        &self,
        notification: StateChangeNotification,
    ) -> Result<(), StateError> {
        debug!("Broadcasting state change: {:?}", notification);
        Ok(())
    }
}
pub fn create_logging_observer() -> StateChangeObserver {
    Arc::new(|notification: StateChangeNotification| {
        debug!("State change observed: {:?}", notification);
    })
}
pub fn create_metrics_observer() -> StateChangeObserver {
    Arc::new(|notification: StateChangeNotification| {
        debug!(
            "Metrics updated for state change: {:?}",
            notification.change_type
        );
    })
}
pub async fn create_flow_barrier(
    manager: &InMemoryStateManager,
    flow_id: &str,
    participant_count: usize,
    timeout: Duration,
) -> Result<String, StateError> {
    let barrier_id = format!("flow_{}_{}", flow_id, Uuid::new_v4());
    manager
        .create_barrier(barrier_id.clone(), participant_count, timeout)
        .await?;
    Ok(barrier_id)
}
pub async fn wait_for_flow_participants(
    manager: &InMemoryStateManager,
    barrier_id: &str,
    participant_id: &str,
) -> Result<(), StateError> {
    manager
        .wait_at_barrier(barrier_id, participant_id.to_string())
        .await
}


impl Serialize for UnifiedState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("UnifiedState", 15)?; 
        state.serialize_field("user_id", &self.user_id)?;
        state.serialize_field("operator_id", &self.operator_id)?;
        state.serialize_field("channel_id", &self.channel_id)?;
        state.serialize_field("flow_id", &self.flow_id)?;
        state.serialize_field("block_id", &self.block_id)?;
        state.serialize_field("data", &self.data)?;
        state.serialize_field("metadata", &self.metadata)?;
        state.serialize_field("flow_context", &self.flow_context)?;
        state.serialize_field("skill_context", &self.skill_context)?;
        state.serialize_field("version", &self.version)?;
        state.serialize_field("previous_versions", &self.previous_versions)?;
        state.serialize_field("created_at", &self.created_at)?;
        state.serialize_field("updated_at", &self.updated_at)?;
        state.serialize_field("checksum", &self.checksum)?;
        state.end()
    }
}

struct UnifiedStateVisitor;

impl<'de> Visitor<'de> for UnifiedStateVisitor {
    type Value = UnifiedState;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a UnifiedState struct")
    }
    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        use serde::de::Error;
        let mut user_id = None;
        let mut operator_id = None;
        let mut channel_id = None;
        let mut flow_id = None;
        let mut block_id = None;
        let mut data = None;
        let mut metadata = None;
        let mut flow_context = None;
        let mut skill_context = None;
        let mut version = None;
        let mut previous_versions = None;
        let mut created_at = None;
        let mut updated_at = None;
        let mut checksum = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "user_id" => user_id = Some(map.next_value()?),
                "operator_id" => operator_id = Some(map.next_value()?),
                "channel_id" => channel_id = Some(map.next_value()?),
                "flow_id" => flow_id = Some(map.next_value()?),
                "block_id" => block_id = Some(map.next_value()?),
                "data" => data = Some(map.next_value()?),
                "metadata" => metadata = Some(map.next_value()?),
                "flow_context" => flow_context = Some(map.next_value()?),
                "skill_context" => skill_context = Some(map.next_value()?),
                "version" => version = Some(map.next_value()?),
                "previous_versions" => previous_versions = Some(map.next_value()?),
                "created_at" => created_at = Some(map.next_value()?),
                "updated_at" => updated_at = Some(map.next_value()?),
                "checksum" => checksum = Some(map.next_value()?),
                _ => {
                    let _: serde::de::IgnoredAny = map.next_value()?;
                }
            }
        }
        Ok(UnifiedState {
            user_id: user_id.ok_or_else(|| Error::missing_field("user_id"))?,
            operator_id: operator_id.ok_or_else(|| Error::missing_field("operator_id"))?,
            channel_id: channel_id.ok_or_else(|| Error::missing_field("channel_id"))?,
            flow_id: flow_id.unwrap_or(None),
            block_id: block_id.unwrap_or(None),
            data: data.unwrap_or_default(),
            metadata: metadata.unwrap_or_default(),
            flow_context: flow_context.unwrap_or(None),
            skill_context: skill_context.unwrap_or(None),
            binder: None,
            version: version.unwrap_or(1),
            previous_versions: previous_versions.unwrap_or_default(),
            created_at: created_at.unwrap_or_else(Utc::now),
            updated_at: updated_at.unwrap_or_else(Utc::now),
            metrics: Some(StateMetrics::default()),
            checksum: checksum.unwrap_or(None),
        })
    }
}

impl<'de> Deserialize<'de> for UnifiedState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct(
            "UnifiedState",
            &[
                "user_id",
                "operator_id",
                "channel_id",
                "flow_id",
                "block_id",
                "data",
                "metadata",
                "flow_context",
                "skill_context",
                "version",
                "previous_versions",
                "created_at",
                "updated_at",
                "checksum",
            ],
            UnifiedStateVisitor,
        )
    }
}
