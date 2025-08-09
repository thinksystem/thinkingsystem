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

use anyhow::Result;
use std::future::Future;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone, PartialEq)]
pub enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub recovery_timeout: Duration,
    pub half_open_max_calls: u32,
    pub minimum_throughput: u32,
    pub sliding_window_size: Duration,
    pub success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(60),
            half_open_max_calls: 3,
            minimum_throughput: 10,
            sliding_window_size: Duration::from_secs(60),
            success_threshold: 2,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CircuitBreakerError<E> {
    #[error("Circuit breaker is open - rejecting call")]
    CircuitOpen,

    #[error("Circuit breaker half-open call limit exceeded")]
    HalfOpenLimitExceeded,

    #[error("Operation failed: {0}")]
    CallFailed(E),

    #[error("Circuit breaker timeout: {0}")]
    Timeout(String),
}

#[derive(Debug)]
pub struct CircuitBreakerMetrics {
    pub total_calls: AtomicU64,
    pub successful_calls: AtomicU64,
    pub failed_calls: AtomicU64,
    pub rejected_calls: AtomicU64,
    pub state_transitions: AtomicU64,
    pub last_failure_time: Arc<RwLock<Option<Instant>>>,
    pub last_success_time: Arc<RwLock<Option<Instant>>>,
}

impl Default for CircuitBreakerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl CircuitBreakerMetrics {
    pub fn new() -> Self {
        Self {
            total_calls: AtomicU64::new(0),
            successful_calls: AtomicU64::new(0),
            failed_calls: AtomicU64::new(0),
            rejected_calls: AtomicU64::new(0),
            state_transitions: AtomicU64::new(0),
            last_failure_time: Arc::new(RwLock::new(None)),
            last_success_time: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn record_success(&self, _duration: Duration) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.successful_calls.fetch_add(1, Ordering::Relaxed);

        let mut last_success = self.last_success_time.write().await;
        *last_success = Some(Instant::now());
    }

    pub async fn record_failure(&self, _duration: Duration) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.failed_calls.fetch_add(1, Ordering::Relaxed);

        let mut last_failure = self.last_failure_time.write().await;
        *last_failure = Some(Instant::now());
    }

    pub fn record_rejected_call(&self) {
        self.rejected_calls.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_state_transition(&self) {
        self.state_transitions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_failure_rate(&self) -> f64 {
        let total = self.total_calls.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }

        let failures = self.failed_calls.load(Ordering::Relaxed);
        failures as f64 / total as f64
    }

    pub fn get_success_rate(&self) -> f64 {
        1.0 - self.get_failure_rate()
    }
}

#[derive(Debug)]
pub struct CircuitBreaker {
    name: String,
    state: Arc<RwLock<CircuitBreakerState>>,
    config: CircuitBreakerConfig,
    metrics: Arc<CircuitBreakerMetrics>,
    failure_count: Arc<AtomicU32>,
    success_count: Arc<AtomicU32>,
    half_open_calls: Arc<AtomicU32>,
    last_state_change: Arc<RwLock<Instant>>,
}

impl CircuitBreaker {
    pub fn new(name: String, config: CircuitBreakerConfig) -> Self {
        Self {
            name,
            state: Arc::new(RwLock::new(CircuitBreakerState::Closed)),
            config,
            metrics: Arc::new(CircuitBreakerMetrics::new()),
            failure_count: Arc::new(AtomicU32::new(0)),
            success_count: Arc::new(AtomicU32::new(0)),
            half_open_calls: Arc::new(AtomicU32::new(0)),
            last_state_change: Arc::new(RwLock::new(Instant::now())),
        }
    }

    pub fn with_default_config(name: String) -> Self {
        Self::new(name, CircuitBreakerConfig::default())
    }

    pub async fn call_async<T, E, F, Fut>(&self, operation: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::error::Error + Send + Sync + 'static,
    {
        match self.should_allow_call().await {
            Ok(false) => {
                self.metrics.record_rejected_call();
                return Err(CircuitBreakerError::CircuitOpen);
            }
            Err(CircuitBreakerError::HalfOpenLimitExceeded) => {
                self.metrics.record_rejected_call();
                return Err(CircuitBreakerError::HalfOpenLimitExceeded);
            }
            Err(CircuitBreakerError::CircuitOpen) => {
                self.metrics.record_rejected_call();
                return Err(CircuitBreakerError::CircuitOpen);
            }
            Ok(true) => {}
            _ => {
                self.metrics.record_rejected_call();
                return Err(CircuitBreakerError::CircuitOpen);
            }
        }

        let current_state = self.get_state().await;
        if matches!(current_state, CircuitBreakerState::HalfOpen) {
            self.half_open_calls.fetch_add(1, Ordering::Relaxed);
        }

        let start = Instant::now();
        match operation().await {
            Ok(result) => {
                self.on_success().await;
                self.metrics.record_success(start.elapsed()).await;
                Ok(result)
            }
            Err(error) => {
                self.on_failure().await;
                self.metrics.record_failure(start.elapsed()).await;
                Err(CircuitBreakerError::CallFailed(error))
            }
        }
    }

    pub async fn call_with_timeout<T, E, F, Fut>(
        &self,
        operation: F,
        timeout: Duration,
    ) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::error::Error + Send + Sync + 'static,
    {
        match tokio::time::timeout(timeout, self.call_async(operation)).await {
            Ok(result) => result,
            Err(_) => {
                self.on_failure().await;
                Err(CircuitBreakerError::Timeout(format!(
                    "Operation timed out after {timeout:?}"
                )))
            }
        }
    }

    async fn should_allow_call(&self) -> Result<bool, CircuitBreakerError<()>> {
        let current_state = self.get_state().await;

        match current_state {
            CircuitBreakerState::Closed => Ok(true),
            CircuitBreakerState::Open => {
                if self.should_attempt_reset().await {
                    self.transition_to_half_open().await;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            CircuitBreakerState::HalfOpen => {
                let current_half_open_calls = self.half_open_calls.load(Ordering::Relaxed);
                if current_half_open_calls < self.config.half_open_max_calls {
                    Ok(true)
                } else {
                    Err(CircuitBreakerError::HalfOpenLimitExceeded)
                }
            }
        }
    }

    async fn on_success(&self) {
        let current_state = self.get_state().await;
        self.success_count.fetch_add(1, Ordering::Relaxed);

        match current_state {
            CircuitBreakerState::HalfOpen => {
                let successes = self.success_count.load(Ordering::Relaxed);
                if successes >= self.config.success_threshold {
                    self.transition_to_closed().await;
                }
            }
            CircuitBreakerState::Closed => {
                self.failure_count.store(0, Ordering::Relaxed);
            }
            CircuitBreakerState::Open => {
                tracing::warn!(
                    "Circuit breaker '{}' received success in Open state",
                    self.name
                );
            }
        }

        if matches!(current_state, CircuitBreakerState::HalfOpen) {
            self.half_open_calls.store(0, Ordering::Relaxed);
        }
    }

    async fn on_failure(&self) {
        let current_state = self.get_state().await;
        self.failure_count.fetch_add(1, Ordering::Relaxed);

        match current_state {
            CircuitBreakerState::Closed => {
                let failures = self.failure_count.load(Ordering::Relaxed);
                if failures >= self.config.failure_threshold {
                    self.transition_to_open().await;
                }
            }
            CircuitBreakerState::HalfOpen => {
                self.transition_to_open().await;
            }
            CircuitBreakerState::Open => {}
        }

        if matches!(current_state, CircuitBreakerState::HalfOpen) {
            self.half_open_calls.store(0, Ordering::Relaxed);
        }
    }

    async fn should_attempt_reset(&self) -> bool {
        let last_change = *self.last_state_change.read().await;
        last_change.elapsed() >= self.config.recovery_timeout
    }

    async fn transition_to_open(&self) {
        let mut state = self.state.write().await;
        if !matches!(*state, CircuitBreakerState::Open) {
            tracing::warn!("Circuit breaker '{}' transitioning to OPEN", self.name);
            *state = CircuitBreakerState::Open;

            let mut last_change = self.last_state_change.write().await;
            *last_change = Instant::now();

            self.metrics.record_state_transition();
        }
    }

    async fn transition_to_half_open(&self) {
        let mut state = self.state.write().await;
        if !matches!(*state, CircuitBreakerState::HalfOpen) {
            tracing::info!("Circuit breaker '{}' transitioning to HALF_OPEN", self.name);
            *state = CircuitBreakerState::HalfOpen;

            let mut last_change = self.last_state_change.write().await;
            *last_change = Instant::now();

            self.success_count.store(0, Ordering::Relaxed);
            self.half_open_calls.store(0, Ordering::Relaxed);

            self.metrics.record_state_transition();
        }
    }

    async fn transition_to_closed(&self) {
        let mut state = self.state.write().await;
        if !matches!(*state, CircuitBreakerState::Closed) {
            tracing::info!("Circuit breaker '{}' transitioning to CLOSED", self.name);
            *state = CircuitBreakerState::Closed;

            let mut last_change = self.last_state_change.write().await;
            *last_change = Instant::now();

            self.failure_count.store(0, Ordering::Relaxed);
            self.success_count.store(0, Ordering::Relaxed);
            self.half_open_calls.store(0, Ordering::Relaxed);

            self.metrics.record_state_transition();
        }
    }

    pub async fn get_state(&self) -> CircuitBreakerState {
        let state = self.state.read().await;
        state.clone()
    }

    pub fn get_metrics(&self) -> Arc<CircuitBreakerMetrics> {
        Arc::clone(&self.metrics)
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub async fn force_open(&self) {
        self.transition_to_open().await;
    }

    pub async fn force_closed(&self) {
        self.transition_to_closed().await;
    }

    pub async fn force_half_open(&self) {
        self.transition_to_half_open().await;
    }

    pub async fn can_execute(&self) -> bool {
        matches!(self.should_allow_call().await, Ok(true))
    }

    pub async fn call<T, E, F, Fut>(&self, operation: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::error::Error + Send + Sync + 'static,
    {
        self.call_async(operation).await
    }
}

use std::collections::HashMap;

pub struct CircuitBreakerManager {
    breakers: Arc<RwLock<HashMap<String, Arc<CircuitBreaker>>>>,
}

impl CircuitBreakerManager {
    pub fn new() -> Self {
        Self {
            breakers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get_or_create(
        &self,
        name: String,
        config: CircuitBreakerConfig,
    ) -> Arc<CircuitBreaker> {
        let breakers = self.breakers.read().await;
        if let Some(breaker) = breakers.get(&name) {
            return Arc::clone(breaker);
        }
        drop(breakers);

        let mut breakers = self.breakers.write().await;

        if let Some(breaker) = breakers.get(&name) {
            return Arc::clone(breaker);
        }

        let breaker = Arc::new(CircuitBreaker::new(name.clone(), config));
        breakers.insert(name, Arc::clone(&breaker));
        breaker
    }

    pub async fn get_breaker(&self, name: &str) -> Option<Arc<CircuitBreaker>> {
        let breakers = self.breakers.read().await;
        breakers.get(name).map(Arc::clone)
    }

    pub async fn get_all_states(&self) -> HashMap<String, CircuitBreakerState> {
        let breakers = self.breakers.read().await;
        let mut states = HashMap::new();

        for (name, breaker) in breakers.iter() {
            states.insert(name.clone(), breaker.get_state().await);
        }

        states
    }

    pub async fn health_check(&self) -> bool {
        let breakers = self.breakers.read().await;

        for breaker in breakers.values() {
            let state = breaker.get_state().await;
            if matches!(state, CircuitBreakerState::Open) {
                return false;
            }
        }

        true
    }
}

impl Default for CircuitBreakerManager {
    fn default() -> Self {
        Self::new()
    }
}
