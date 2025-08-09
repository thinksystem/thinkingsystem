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

use std::sync::Arc;
use steel::messaging::resilience::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitBreakerManager,
    CircuitBreakerState,
};
use tokio::time::{sleep, Duration};

#[derive(Debug, thiserror::Error)]
#[error("Test error: {message}")]
struct TestError {
    message: String,
}

impl TestError {
    fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

#[tokio::test]
async fn test_circuit_breaker_closed_state() {
    let breaker = CircuitBreaker::with_default_config("test".to_string());

    let result = breaker
        .call_async(|| async { Ok::<i32, TestError>(42) })
        .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
}

#[tokio::test]
async fn test_circuit_breaker_opens_on_failures() {
    let config = CircuitBreakerConfig {
        failure_threshold: 2,
        ..Default::default()
    };
    let breaker = CircuitBreaker::new("test".to_string(), config);

    let _ = breaker
        .call_async(|| async { Err::<i32, TestError>(TestError::new("error")) })
        .await;
    assert_eq!(breaker.get_state().await, CircuitBreakerState::Closed);

    let _ = breaker
        .call_async(|| async { Err::<i32, TestError>(TestError::new("error")) })
        .await;
    assert_eq!(breaker.get_state().await, CircuitBreakerState::Open);

    let result = breaker
        .call_async(|| async { Ok::<i32, TestError>(42) })
        .await;
    assert!(matches!(result, Err(CircuitBreakerError::CircuitOpen)));
}

#[tokio::test]
async fn test_circuit_breaker_half_open_transition() {
    let config = CircuitBreakerConfig {
        failure_threshold: 1,
        recovery_timeout: Duration::from_millis(100),
        ..Default::default()
    };
    let breaker = CircuitBreaker::new("test".to_string(), config);

    let _ = breaker
        .call_async(|| async { Err::<i32, TestError>(TestError::new("error")) })
        .await;
    assert_eq!(breaker.get_state().await, CircuitBreakerState::Open);

    sleep(Duration::from_millis(150)).await;

    let result = breaker
        .call_async(|| async { Ok::<i32, TestError>(42) })
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_circuit_breaker_manager() {
    let manager = CircuitBreakerManager::new();

    let breaker1 = manager
        .get_or_create("service1".to_string(), CircuitBreakerConfig::default())
        .await;
    let breaker2 = manager
        .get_or_create("service2".to_string(), CircuitBreakerConfig::default())
        .await;

    assert_eq!(breaker1.get_name(), "service1");
    assert_eq!(breaker2.get_name(), "service2");

    let breaker1_again = manager
        .get_or_create("service1".to_string(), CircuitBreakerConfig::default())
        .await;
    assert!(Arc::ptr_eq(&breaker1, &breaker1_again));
}
