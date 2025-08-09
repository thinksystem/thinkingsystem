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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;
use parking_lot::RwLock as SyncRwLock;
use thiserror::Error;
use tracing::{debug, error, warn, instrument};
use crate::flows::flowgorithm::Binder;
const MAX_STATE_SIZE_BYTES: usize = 10 * 1024 * 1024;
const MAX_METADATA_SIZE_BYTES: usize = 1024 * 1024;
const DEFAULT_LOCK_TIMEOUT_SECS: u64 = 300;
const MAX_LOCK_RETRIES: u32 = 3;
const LOCK_RETRY_DELAY_MS: u64 = 100;
#[derive(Error, Debug)]
pub enum StateError {
    #[error("State size limit exceeded: current={current}, attempted={attempted}, limit={limit}")]
    StateSizeExceeded {
        current: usize,
        attempted: usize,
        limit: usize,
    },
    #[error("Lock acquisition failed: {reason}")]
    LockError { reason: String },
    #[error("Lock already held by: {owner}")]
    AlreadyLocked { owner: String },
    #[error("Maximum lock retries exceeded")]
    MaxRetriesExceeded,
    #[error("Lock not found: {flow_id}")]
    LockNotFound { flow_id: String },
    #[error("Invalid lock owner: expected={expected}, actual={actual}")]
    InvalidLockOwner { expected: String, actual: String },
    #[error("Lock expired for flow: {flow_id}")]
    LockExpired { flow_id: String },
    #[error("Serialisation error: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("State validation error: {0}")]
    ValidationError(String),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedState {
    pub user_id: String,
    pub operator_id: String,
    pub channel_id: String,
    pub flow_id: Option<String>,
    pub block_id: Option<String>,
    #[serde(skip)]
    pub data: Arc<SyncRwLock<HashMap<String, Value>>>,
    #[serde(skip)]
    pub metadata: Arc<SyncRwLock<HashMap<String, Value>>>,
    #[serde(rename = "data")]
    pub data_serializable: HashMap<String, Value>,
    #[serde(rename = "metadata")]
    pub metadata_serializable: HashMap<String, Value>,
    pub flow_context: Option<Value>,
    pub skill_context: Option<Value>,
    #[serde(skip)]
    pub binder: Option<Arc<Binder>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip)]
    pub metrics: Arc<SyncRwLock<StateMetrics>>,
    pub version: u64,
    pub checksum: Option<String>,
}
#[derive(Debug, Clone, Default)]
pub struct StateMetrics {
    pub data_size_bytes: usize,
    pub metadata_size_bytes: usize,
    pub access_count: u64,
    pub modification_count: u64,
    pub last_access: DateTime<Utc>,
    pub last_modification: DateTime<Utc>,
    pub serialization_count: u64,
    pub deserialization_count: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockType {
    Shared,
    Exclusive,
}
#[derive(Debug, Clone)]
pub struct FlowLock {
    pub id: u64,
    pub owner: String,
    pub flow_id: String,
    pub timestamp: DateTime<Utc>,
    pub lock_type: LockType,
    pub metadata: HashMap<String, String>,
}
pub struct LockHandle {
    flow_id: String,
    lock_id: u64,
    owner: String,
    manager: Weak<ConcurrencyManager>,
}
impl Drop for LockHandle {
    fn drop(&mut self) {
        if let Some(manager) = self.manager.upgrade() {
            let flow_id = self.flow_id.clone();
            let owner = self.owner.clone();
            tokio::spawn(async move {
                if let Err(e) = manager.release_lock(&flow_id, &owner).await {
                    error!("Failed to auto-release lock for {}: {:?}", flow_id, e);
                }
            });
        }
    }
}
pub struct ConcurrencyManager {
    flow_locks: RwLock<HashMap<String, FlowLock>>,
    timeout_duration: Duration,
    lock_counter: std::sync::atomic::AtomicU64,
    metrics: Arc<SyncRwLock<ConcurrencyMetrics>>,
}
#[derive(Debug, Clone, Default)]
pub struct ConcurrencyMetrics {
    pub total_locks_acquired: u64,
    pub total_locks_released: u64,
    pub lock_timeouts: u64,
    pub lock_conflicts: u64,
    pub average_lock_duration: Duration,
}
impl ConcurrencyManager {
    pub fn new(timeout_secs: u64) -> Self {
        Self {
            flow_locks: RwLock::new(HashMap::new()),
            timeout_duration: Duration::from_secs(timeout_secs),
            lock_counter: std::sync::atomic::AtomicU64::new(0),
            metrics: Arc::new(SyncRwLock::new(ConcurrencyMetrics::default())),
        }
    }
    #[instrument(skip(self), fields(flow_id = %flow_id, owner = %owner))]
    pub async fn acquire_lock_with_retry(
        &self,
        flow_id: &str,
        owner: String,
        lock_type: LockType,
    ) -> Result<LockHandle, StateError> {
        for attempt in 0..MAX_LOCK_RETRIES {
            match self.try_acquire_lock(flow_id, &owner, lock_type.clone()).await {
                Ok(handle) => {
                    debug!("Lock acquired on attempt {}", attempt + 1);
                    return Ok(handle);
                }
                Err(StateError::AlreadyLocked { .. }) if attempt < MAX_LOCK_RETRIES - 1 => {
                    let delay = Duration::from_millis(LOCK_RETRY_DELAY_MS * (attempt + 1) as u64);
                    warn!("Lock conflict, retrying in {:?}", delay);
                    tokio::time::sleep(delay).await;
                    self.metrics.write().lock_conflicts += 1;
                }
                Err(e) => return Err(e),
            }
        }
        error!("Failed to acquire lock after {} attempts", MAX_LOCK_RETRIES);
        Err(StateError::MaxRetriesExceeded)
    }
    async fn try_acquire_lock(
        &self,
        flow_id: &str,
        owner: &str,
        lock_type: LockType,
    ) -> Result<LockHandle, StateError> {
        let mut locks = self.flow_locks.write().await;
        let now = Utc::now();
        self.cleanup_expired_locks(&mut locks, now).await;
        if let Some(existing_lock) = locks.get(flow_id) {
            if existing_lock.timestamp + self.timeout_duration > now {
                return Err(StateError::AlreadyLocked {
                    owner: existing_lock.owner.clone(),
                });
            }
        }
        let lock_id = self.lock_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let lock = FlowLock {
            id: lock_id,
            owner: owner.to_string(),
            flow_id: flow_id.to_string(),
            timestamp: now,
            lock_type,
            metadata: HashMap::new(),
        };
        locks.insert(flow_id.to_string(), lock);
        self.metrics.write().total_locks_acquired += 1;
        Ok(LockHandle {
            flow_id: flow_id.to_string(),
            lock_id,
            owner: owner.to_string(),
            manager: Arc::downgrade(&Arc::new(self.clone())),
        })
    }
    #[instrument(skip(self), fields(flow_id = %flow_id, owner = %owner))]
    pub async fn release_lock(&self, flow_id: &str, owner: &str) -> Result<(), StateError> {
        let mut locks = self.flow_locks.write().await;
        match locks.get(flow_id) {
            Some(lock) if lock.owner == owner => {
                let lock_duration = Utc::now() - lock.timestamp;
                locks.remove(flow_id);
                let mut metrics = self.metrics.write();
                metrics.total_locks_released += 1;
                metrics.average_lock_duration =
                    (metrics.average_lock_duration + lock_duration) / 2;
                debug!("Lock released successfully");
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
    async fn cleanup_expired_locks(
        &self,
        locks: &mut HashMap<String, FlowLock>,
        now: DateTime<Utc>,
    ) {
        let expired_keys: Vec<String> = locks
            .iter()
            .filter(|(_, lock)| lock.timestamp + self.timeout_duration < now)
            .map(|(key, _)| key.clone())
            .collect();
        for key in expired_keys {
            locks.remove(&key);
            self.metrics.write().lock_timeouts += 1;
            warn!("Cleaned up expired lock for flow: {}", key);
        }
    }
    pub async fn get_lock_info(&self, flow_id: &str) -> Option<FlowLock> {
        self.flow_locks.read().await.get(flow_id).cloned()
    }
    pub fn get_metrics(&self) -> ConcurrencyMetrics {
        self.metrics.read().clone()
    }
}
impl Clone for ConcurrencyManager {
    fn clone(&self) -> Self {
        Self {
            flow_locks: RwLock::new(HashMap::new()),
            timeout_duration: self.timeout_duration,
            lock_counter: std::sync::atomic::AtomicU64::new(
                self.lock_counter.load(std::sync::atomic::Ordering::SeqCst)
            ),
            metrics: Arc::clone(&self.metrics),
        }
    }
}
impl UnifiedState {
    pub fn new(user_id: String, operator_id: String, channel_id: String) -> Self {
        let data = Arc::new(SyncRwLock::new(HashMap::new()));
        let metadata = Arc::new(SyncRwLock::new(HashMap::new()));
        Self {
            user_id,
            operator_id,
            channel_id,
            flow_id: None,
            block_id: None,
            data: Arc::clone(&data),
            metadata: Arc::clone(&metadata),
            data_serializable: HashMap::new(),
            metadata_serializable: HashMap::new(),
            flow_context: None,
            skill_context: None,
            binder: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metrics: Arc::new(SyncRwLock::new(StateMetrics::default())),
            version: 0,
            checksum: None,
        }
    }
    pub fn with_flow(mut self, flow_id: String) -> Self {
        self.flow_id = Some(flow_id);
        self.updated_at = Utc::now();
        self.version += 1;
        self
    }
    pub fn with_block(mut self, block_id: String) -> Self {
        self.block_id = Some(block_id);
        self.updated_at = Utc::now();
        self.version += 1;
        self
    }
    #[instrument(skip(self, value), fields(key = %key))]
    pub fn set_data(&mut self, key: String, value: Value) -> Result<(), StateError> {
        let estimated_size = self.estimate_value_size(&value);
        {
            let mut data = self.data.write();
            let current_size = self.calculate_data_size(&data);
            if current_size + estimated_size > MAX_STATE_SIZE_BYTES {
                return Err(StateError::StateSizeExceeded {
                    current: current_size,
                    attempted: estimated_size,
                    limit: MAX_STATE_SIZE_BYTES,
                });
            }
            data.insert(key.clone(), value);
        }
        self.update_metrics(|metrics| {
            metrics.modification_count += 1;
            metrics.last_modification = Utc::now();
            metrics.data_size_bytes = self.calculate_current_data_size();
        });
        self.updated_at = Utc::now();
        self.version += 1;
        self.update_checksum();
        debug!("Data updated for key: {}", key);
        Ok(())
    }
    pub fn get_data(&self, key: &str) -> Option<Value> {
        let data = self.data.read();
        let result = data.get(key).cloned();
        self.update_metrics(|metrics| {
            metrics.access_count += 1;
            metrics.last_access = Utc::now();
        });
        result
    }
    pub fn set_metadata(&mut self, key: String, value: Value) -> Result<(), StateError> {
        let estimated_size = self.estimate_value_size(&value);
        {
            let mut metadata = self.metadata.write();
            let current_size = self.calculate_metadata_size(&metadata);
            if current_size + estimated_size > MAX_METADATA_SIZE_BYTES {
                return Err(StateError::StateSizeExceeded {
                    current: current_size,
                    attempted: estimated_size,
                    limit: MAX_METADATA_SIZE_BYTES,
                });
            }
            metadata.insert(key.clone(), value);
        }
        self.update_metrics(|metrics| {
            metrics.modification_count += 1;
            metrics.last_modification = Utc::now();
            metrics.metadata_size_bytes = self.calculate_current_metadata_size();
        });
        self.updated_at = Utc::now();
        self.version += 1;
        self.update_checksum();
        debug!("Metadata updated for key: {}", key);
        Ok(())
    }
    pub fn get_metadata(&self, key: &str) -> Option<Value> {
        let metadata = self.metadata.read();
        let result = metadata.get(key).cloned();
        self.update_metrics(|metrics| {
            metrics.access_count += 1;
            metrics.last_access = Utc::now();
        });
        result
    }
    pub fn set_flow_context(&mut self, context: Value) {
        self.flow_context = Some(context);
        self.updated_at = Utc::now();
        self.version += 1;
        self.update_checksum();
    }
    pub fn set_skill_context(&mut self, context: Value) {
        self.skill_context = Some(context);
        self.updated_at = Utc::now();
        self.version += 1;
        self.update_checksum();
    }
    pub fn set_binder(&mut self, binder: Binder) {
        self.binder = Some(Arc::new(binder));
        self.updated_at = Utc::now();
        self.version += 1;
    }
    pub fn clear_flow_data(&mut self) {
        self.flow_id = None;
        self.block_id = None;
        self.flow_context = None;
        self.binder = None;
        {
            let mut data = self.data.write();
            data.clear();
        }
        self.update_metrics(|metrics| {
            metrics.modification_count += 1;
            metrics.last_modification = Utc::now();
            metrics.data_size_bytes = 0;
        });
        self.updated_at = Utc::now();
        self.version += 1;
        self.update_checksum();
        debug!("Flow data cleared");
    }
    pub fn is_in_flow(&self) -> bool {
        self.flow_id.is_some()
    }
    pub fn get_flow_state(&self) -> HashMap<String, Value> {
        let data = self.data.read();
        data.clone()
    }
    pub fn update_state(&mut self, new_data: HashMap<String, Value>) -> Result<(), StateError> {
        let estimated_size = new_data.iter()
            .map(|(_, v)| self.estimate_value_size(v))
            .sum::<usize>();
        if estimated_size > MAX_STATE_SIZE_BYTES {
            return Err(StateError::StateSizeExceeded {
                current: 0,
                attempted: estimated_size,
                limit: MAX_STATE_SIZE_BYTES,
            });
        }
        {
            let mut data = self.data.write();
            *data = new_data;
        }
        self.update_metrics(|metrics| {
            metrics.modification_count += 1;
            metrics.last_modification = Utc::now();
            metrics.data_size_bytes = estimated_size;
        });
        self.updated_at = Utc::now();
        self.version += 1;
        self.update_checksum();
        Ok(())
    }
    pub fn get_state_identifier(&self) -> String {
        format!("{}:{}:{}",
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
            return Err(StateError::ValidationError("user_id cannot be empty".to_string()));
        }
        if self.channel_id.is_empty() {
            return Err(StateError::ValidationError("channel_id cannot be empty".to_string()));
        }
        let data_size = self.calculate_current_data_size();
        if data_size > MAX_STATE_SIZE_BYTES {
            return Err(StateError::StateSizeExceeded {
                current: data_size,
                attempted: 0,
                limit: MAX_STATE_SIZE_BYTES,
            });
        }
        let metadata_size = self.calculate_current_metadata_size();
        if metadata_size > MAX_METADATA_SIZE_BYTES {
            return Err(StateError::StateSizeExceeded {
                current: metadata_size,
                attempted: 0,
                limit: MAX_METADATA_SIZE_BYTES,
            });
        }
        Ok(())
    }
    pub fn get_metrics(&self) -> StateMetrics {
        self.metrics.read().clone()
    }
    pub fn create_snapshot(&self) -> StateSnapshot {
        let data = self.data.read().clone();
        let metadata = self.metadata.read().clone();
        StateSnapshot {
            state_id: self.get_state_identifier(),
            version: self.version,
            data,
            metadata,
            flow_context: self.flow_context.clone(),
            skill_context: self.skill_context.clone(),
            timestamp: Utc::now(),
            checksum: self.checksum.clone(),
        }
    }
    pub fn restore_from_snapshot(&mut self, snapshot: StateSnapshot) -> Result<(), StateError> {
        if snapshot.state_id != self.get_state_identifier() {
            return Err(StateError::ValidationError(
                "Snapshot state_id does not match current state".to_string()
            ));
        }
        {
            let mut data = self.data.write();
            *data = snapshot.data;
        }
        {
            let mut metadata = self.metadata.write();
            *metadata = snapshot.metadata;
        }
        self.flow_context = snapshot.flow_context;
        self.skill_context = snapshot.skill_context;
        self.version = snapshot.version;
        self.checksum = snapshot.checksum;
        self.updated_at = Utc::now();
        debug!("State restored from snapshot at version {}", snapshot.version);
        Ok(())
    }
    pub fn prepare_for_serialization(&mut self) {
        self.data_serializable = self.data.read().clone();
        self.metadata_serializable = self.metadata.read().clone();
        self.update_metrics(|metrics| {
            metrics.serialization_count += 1;
        });
    }
    pub fn restore_after_deserialization(&mut self) {
        self.data = Arc::new(SyncRwLock::new(self.data_serializable.clone()));
        self.metadata = Arc::new(SyncRwLock::new(self.metadata_serializable.clone()));
        self.metrics = Arc::new(SyncRwLock::new(StateMetrics::default()));
        self.update_metrics(|metrics| {
            metrics.deserialization_count += 1;
            metrics.data_size_bytes = self.calculate_current_data_size();
            metrics.metadata_size_bytes = self.calculate_current_metadata_size();
        });
    }
    fn estimate_value_size(&self, value: &Value) -> usize {
        match value {
            Value::Null => 8,
            Value::Bool(_) => 9,
            Value::Number(_) => 24,
            Value::String(s) => 24 + s.len(),
            Value::Array(arr) => {
                24 + arr.iter().map(|v| self.estimate_value_size(v)).sum::<usize>()
            }
            Value::Object(obj) => {
                24 + obj.iter()
                    .map(|(k, v)| k.len() + self.estimate_value_size(v))
                    .sum::<usize>()
            }
        }
    }
    fn calculate_data_size(&self, data: &HashMap<String, Value>) -> usize {
        data.iter()
            .map(|(k, v)| k.len() + self.estimate_value_size(v))
            .sum()
    }
    fn calculate_metadata_size(&self, metadata: &HashMap<String, Value>) -> usize {
        metadata.iter()
            .map(|(k, v)| k.len() + self.estimate_value_size(v))
            .sum()
    }
    fn calculate_current_data_size(&self) -> usize {
        let data = self.data.read();
        self.calculate_data_size(&data)
    }
    fn calculate_current_metadata_size(&self) -> usize {
        let metadata = self.metadata.read();
        self.calculate_metadata_size(&metadata)
    }
    fn update_metrics<F>(&self, updater: F)
    where
        F: FnOnce(&mut StateMetrics),
    {
        let mut metrics = self.metrics.write();
        updater(&mut metrics);
    }
    fn update_checksum(&mut self) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        let data = self.data.read();
        for (key, value) in data.iter() {
            key.hash(&mut hasher);
            if let Ok(json_str) = serde_json::to_string(value) {
                json_str.hash(&mut hasher);
            }
        }
        self.version.hash(&mut hasher);
        self.checksum = Some(format!("{:x}", hasher.finish()));
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub state_id: String,
    pub version: u64,
    pub data: HashMap<String, Value>,
    pub metadata: HashMap<String, Value>,
    pub flow_context: Option<Value>,
    pub skill_context: Option<Value>,
    pub timestamp: DateTime<Utc>,
    pub checksum: Option<String>,
}
pub trait StateManager: Send + Sync {
    async fn save_state(&self, state: &UnifiedState) -> Result<(), StateError>;
    async fn load_state(&self, identifier: &str) -> Result<UnifiedState, StateError>;
    async fn delete_state(&self, identifier: &str) -> Result<(), StateError>;
    async fn cleanup_stale_states(&self, timeout: Duration) -> Result<Vec<String>, StateError>;
    async fn get_state_version(&self, identifier: &str) -> Result<u64, StateError>;
    async fn save_snapshot(&self, snapshot: &StateSnapshot) -> Result<(), StateError>;
    async fn load_snapshot(&self, state_id: &str, version: u64) -> Result<StateSnapshot, StateError>;
}
pub struct InMemoryStateManager {
    states: Arc<RwLock<HashMap<String, UnifiedState>>>,
    snapshots: Arc<RwLock<HashMap<String, Vec<StateSnapshot>>>>,
    concurrency_manager: Arc<ConcurrencyManager>,
    metrics: Arc<SyncRwLock<StateManagerMetrics>>,
}
#[derive(Debug, Clone, Default)]
pub struct StateManagerMetrics {
    pub total_saves: u64,
    pub total_loads: u64,
    pub total_deletes: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cleanup_runs: u64,
    pub states_cleaned: u64,
}
impl InMemoryStateManager {
    pub fn new() -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            concurrency_manager: Arc::new(ConcurrencyManager::new(DEFAULT_LOCK_TIMEOUT_SECS)),
            metrics: Arc::new(SyncRwLock::new(StateManagerMetrics::default())),
        }
    }
    pub async fn acquire_state_lock(
        &self,
        state_id: &str,
        owner: String,
        lock_type: LockType,
    ) -> Result<LockHandle, StateError> {
        self.concurrency_manager
            .acquire_lock_with_retry(state_id, owner, lock_type)
            .await
    }
    pub fn get_metrics(&self) -> StateManagerMetrics {
        self.metrics.read().clone()
    }
    pub fn get_concurrency_metrics(&self) -> ConcurrencyMetrics {
        self.concurrency_manager.get_metrics()
    }
}
#[async_trait::async_trait]
impl StateManager for InMemoryStateManager {
    #[instrument(skip(self, state), fields(state_id = %state.get_state_identifier()))]
    async fn save_state(&self, state: &UnifiedState) -> Result<(), StateError> {
        let key = state.get_state_identifier();
        state.validate()?;
        let mut states = self.states.write().await;
        let mut state_to_save = state.clone();
        state_to_save.prepare_for_serialization();
        states.insert(key.clone(), state_to_save);
        self.metrics.write().total_saves += 1;
        debug!("State saved: {}", key);
        Ok(())
    }
    #[instrument(skip(self), fields(identifier = %identifier))]
    async fn load_state(&self, identifier: &str) -> Result<UnifiedState, StateError> {
        let states = self.states.read().await;
        match states.get(identifier) {
            Some(mut state) => {
                state.restore_after_deserialization();
                self.metrics.write().cache_hits += 1;
                debug!("State loaded: {}", identifier);
                Ok(state.clone())
            }
            None => {
                self.metrics.write().cache_misses += 1;
                Err(StateError::ValidationError(format!("State not found: {identifier}")))
            }
        }
    }
    #[instrument(skip(self), fields(identifier = %identifier))]
    async fn delete_state(&self, identifier: &str) -> Result<(), StateError> {
        let mut states = self.states.write().await;
        let mut snapshots = self.snapshots.write().await;
        match states.remove(identifier) {
            Some(_) => {
                snapshots.remove(identifier);
                self.metrics.write().total_deletes += 1;
                debug!("State deleted: {}", identifier);
                Ok(())
            }
            None => Err(StateError::ValidationError(format!("State not found: {identifier}"))),
        }
    }
    #[instrument(skip(self), fields(timeout = ?timeout))]
    async fn cleanup_stale_states(&self, timeout: Duration) -> Result<Vec<String>, StateError> {
        let mut states = self.states.write().await;
        let mut snapshots = self.snapshots.write().await;
        let stale_keys: Vec<String> = states
            .iter()
            .filter(|(_, state)| state.is_stale(timeout))
            .map(|(key, _)| key.clone())
            .collect();
        let mut cleaned_count = 0;
        for key in &stale_keys {
            states.remove(key);
            snapshots.remove(key);
            cleaned_count += 1;
        }
        {
            let mut metrics = self.metrics.write();
            metrics.cleanup_runs += 1;
            metrics.states_cleaned += cleaned_count;
        }
        debug!("Cleaned up {} stale states", cleaned_count);
        Ok(stale_keys)
    }
    async fn get_state_version(&self, identifier: &str) -> Result<u64, StateError> {
        let states = self.states.read().await;
        states.get(identifier)
            .map(|state| state.version)
            .ok_or_else(|| StateError::ValidationError(format!("State not found: {identifier}")))
    }
    #[instrument(skip(self, snapshot), fields(state_id = %snapshot.state_id, version = snapshot.version))]
    async fn save_snapshot(&self, snapshot: &StateSnapshot) -> Result<(), StateError> {
        let mut snapshots = self.snapshots.write().await;
        let snapshots_for_state = snapshots
            .entry(snapshot.state_id.clone())
            .or_insert_with(Vec::new);
        const MAX_SNAPSHOTS_PER_STATE: usize = 10;
        if snapshots_for_state.len() >= MAX_SNAPSHOTS_PER_STATE {
            snapshots_for_state.remove(0);
        }
        snapshots_for_state.push(snapshot.clone());
        debug!("Snapshot saved for state: {} at version: {}", snapshot.state_id, snapshot.version);
        Ok(())
    }
    #[instrument(skip(self), fields(state_id = %state_id, version = version))]
    async fn load_snapshot(&self, state_id: &str, version: u64) -> Result<StateSnapshot, StateError> {
        let snapshots = self.snapshots.read().await;
        let snapshots_for_state = snapshots.get(state_id)
            .ok_or_else(|| StateError::ValidationError(format!("No snapshots found for state: {state_id}")))?;
        snapshots_for_state
            .iter()
            .find(|snapshot| snapshot.version == version)
            .cloned()
            .ok_or_else(|| StateError::ValidationError(
                format!("Snapshot not found for state: {state_id} at version: {version}")
            ))
    }
}
pub struct FileSystemStateManager {
    base_path: std::path::PathBuf,
    concurrency_manager: Arc<ConcurrencyManager>,
    metrics: Arc<SyncRwLock<StateManagerMetrics>>,
}
impl FileSystemStateManager {
    pub fn new<P: AsRef<std::path::Path>>(base_path: P) -> Result<Self, StateError> {
        let base_path = base_path.as_ref().to_path_buf();
        std::fs::create_dir_all(&base_path)
            .map_err(|e| StateError::ValidationError(format!("Failed to create base directory: {e}")))?;
        let snapshots_dir = base_path.join("snapshots");
        std::fs::create_dir_all(&snapshots_dir)
            .map_err(|e| StateError::ValidationError(format!("Failed to create snapshots directory: {e}")))?;
        Ok(Self {
            base_path,
            concurrency_manager: Arc::new(ConcurrencyManager::new(DEFAULT_LOCK_TIMEOUT_SECS)),
            metrics: Arc::new(SyncRwLock::new(StateManagerMetrics::default())),
        })
    }
    fn get_state_file_path(&self, identifier: &str) -> std::path::PathBuf {
        let safe_identifier = identifier.replace([':', '/', '\\'], "_");
        self.base_path.join(format!("{safe_identifier}.json"))
    }
    fn get_snapshot_file_path(&self, state_id: &str, version: u64) -> std::path::PathBuf {
        let safe_state_id = state_id.replace([':', '/', '\\'], "_");
        self.base_path
            .join("snapshots")
            .join(format!("{safe_state_id}_{version}.json"))
    }
}
#[async_trait::async_trait]
impl StateManager for FileSystemStateManager {
    #[instrument(skip(self, state), fields(state_id = %state.get_state_identifier()))]
    async fn save_state(&self, state: &UnifiedState) -> Result<(), StateError> {
        let identifier = state.get_state_identifier();
        let file_path = self.get_state_file_path(&identifier);
        state.validate()?;
        let mut state_to_save = state.clone();
        state_to_save.prepare_for_serialization();
        let json_data = serde_json::to_string_pretty(&state_to_save)?;
        tokio::fs::write(&file_path, json_data).await
            .map_err(|e| StateError::ValidationError(format!("Failed to write state file: {e}")))?;
        self.metrics.write().total_saves += 1;
        debug!("State saved to file: {:?}", file_path);
        Ok(())
    }
    #[instrument(skip(self), fields(identifier = %identifier))]
    async fn load_state(&self, identifier: &str) -> Result<UnifiedState, StateError> {
        let file_path = self.get_state_file_path(identifier);
        match tokio::fs::read_to_string(&file_path).await {
            Ok(json_data) => {
                let mut state: UnifiedState = serde_json::from_str(&json_data)?;
                state.restore_after_deserialization();
                self.metrics.write().cache_hits += 1;
                debug!("State loaded from file: {:?}", file_path);
                Ok(state)
            }
            Err(_) => {
                self.metrics.write().cache_misses += 1;
                Err(StateError::ValidationError(format!("State file not found: {identifier}")))
            }
        }
    }
    #[instrument(skip(self), fields(identifier = %identifier))]
    async fn delete_state(&self, identifier: &str) -> Result<(), StateError> {
        let file_path = self.get_state_file_path(identifier);
        match tokio::fs::remove_file(&file_path).await {
            Ok(_) => {
                let safe_identifier = identifier.replace([':', '/', '\\'], "_");
                let snapshots_pattern = format!("{safe_identifier}_*.json");
                let snapshots_dir = self.base_path.join("snapshots");
                if let Ok(mut entries) = tokio::fs::read_dir(&snapshots_dir).await {
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        if let Some(file_name) = entry.file_name().to_str() {
                            if file_name.starts_with(&safe_identifier) && file_name.ends_with(".json") {
                                let _ = tokio::fs::remove_file(entry.path()).await;
                            }
                        }
                    }
                }
                self.metrics.write().total_deletes += 1;
                debug!("State file deleted: {:?}", file_path);
                Ok(())
            }
            Err(_) => Err(StateError::ValidationError(format!("State file not found: {identifier}"))),
        }
    }
    #[instrument(skip(self), fields(timeout = ?timeout))]
    async fn cleanup_stale_states(&self, timeout: Duration) -> Result<Vec<String>, StateError> {
        let mut stale_identifiers = Vec::new();
        let mut cleaned_count = 0;
        let mut entries = tokio::fs::read_dir(&self.base_path).await
            .map_err(|e| StateError::ValidationError(format!("Failed to read directory: {e}")))?;
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(metadata) = entry.metadata().await {
                if let Ok(modified) = metadata.modified() {
                    let modified_time = DateTime::<Utc>::from(modified);
                    if modified_time + timeout < Utc::now() {
                        if let Some(file_name) = entry.file_name().to_str() {
                            if file_name.ends_with(".json") {
                                let identifier = file_name.trim_end_matches(".json").replace('_', ":");
                                if let Err(e) = tokio::fs::remove_file(entry.path()).await {
                                    warn!("Failed to remove stale state file: {:?}, error: {}", entry.path(), e);
                                } else {
                                    stale_identifiers.push(identifier);
                                    cleaned_count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        {
            let mut metrics = self.metrics.write();
            metrics.cleanup_runs += 1;
            metrics.states_cleaned += cleaned_count;
        }
        debug!("Cleaned up {} stale state files", cleaned_count);
        Ok(stale_identifiers)
    }
    async fn get_state_version(&self, identifier: &str) -> Result<u64, StateError> {
        let state = self.load_state(identifier).await?;
        Ok(state.version)
    }
    #[instrument(skip(self, snapshot), fields(state_id = %snapshot.state_id, version = snapshot.version))]
    async fn save_snapshot(&self, snapshot: &StateSnapshot) -> Result<(), StateError> {
        let file_path = self.get_snapshot_file_path(&snapshot.state_id, snapshot.version);
        let json_data = serde_json::to_string_pretty(snapshot)?;
        tokio::fs::write(&file_path, json_data).await
            .map_err(|e| StateError::ValidationError(format!("Failed to write snapshot file: {e}")))?;
        debug!("Snapshot saved to file: {:?}", file_path);
        Ok(())
    }
    #[instrument(skip(self), fields(state_id = %state_id, version = version))]
    async fn load_snapshot(&self, state_id: &str, version: u64) -> Result<StateSnapshot, StateError> {
        let file_path = self.get_snapshot_file_path(state_id, version);
        let json_data = tokio::fs::read_to_string(&file_path).await
            .map_err(|_| StateError::ValidationError(
                format!("Snapshot file not found for state: {state_id} at version: {version}")
            ))?;
        let snapshot: StateSnapshot = serde_json::from_str(&json_data)?;
        debug!("Snapshot loaded from file: {:?}", file_path);
        Ok(snapshot)
    }
}
pub mod utils {
    use super::*;
    pub fn create_state_manager(storage_type: &str, config: Option<&str>) -> Result<Box<dyn StateManager>, StateError> {
        match storage_type {
            "memory" => Ok(Box::new(InMemoryStateManager::new())),
            "filesystem" => {
                let base_path = config.unwrap_or("./state_storage");
                Ok(Box::new(FileSystemStateManager::new(base_path)?))
            }
            _ => Err(StateError::ValidationError(format!("Unsupported storage type: {storage_type}"))),
        }
    }
    pub async fn migrate_states<S1, S2>(
        source: &S1,
        target: &S2,
        identifiers: Vec<String>,
    ) -> Result<usize, StateError>
    where
        S1: StateManager,
        S2: StateManager,
    {
        let mut migrated_count = 0;
        for identifier in identifiers {
            match source.load_state(&identifier).await {
                Ok(state) => {
                    if let Err(e) = target.save_state(&state).await {
                        error!("Failed to migrate state {}: {:?}", identifier, e);
                    } else {
                        migrated_count += 1;
                        debug!("Migrated state: {}", identifier);
                    }
                }
                Err(e) => {
                    warn!("Failed to load state {} for migration: {:?}", identifier, e);
                }
            }
        }
        Ok(migrated_count)
    }
    pub fn validate_state_integrity(state: &UnifiedState) -> Result<(), StateError> {
        if let Some(expected_checksum) = &state.checksum {
            let mut temp_state = state.clone();
            temp_state.update_checksum();
            if let Some(actual_checksum) = &temp_state.checksum {
                if expected_checksum != actual_checksum {
                    return Err(StateError::ValidationError(
                        format!("Checksum mismatch: expected {expected_checksum}, got {actual_checksum}")
                    ));
                }
            }
        }
        state.validate()?;
        Ok(())
    }
}
