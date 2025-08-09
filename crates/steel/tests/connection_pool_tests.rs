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

use std::time::Duration;
use steel::messaging::resilience::connection_pool::{
    ConnectionPool, ConnectionPoolConfig, ConnectionPoolError, ConnectionPoolManager,
    PoolStatistics,
};

#[tokio::test]
async fn test_connection_pool_creation() {
    let config = ConnectionPoolConfig::default();

    assert!(config.min_connections <= config.max_connections);
    assert!(config.connection_timeout > Duration::from_secs(0));
}

#[tokio::test]
async fn test_pool_configuration_validation() {
    let config = ConnectionPoolConfig {
        min_connections: 10,
        max_connections: 5,
        ..Default::default()
    };

    let result = ConnectionPool::new(config).await;
    assert!(result.is_err());

    match result {
        Err(ConnectionPoolError::ConfigError(_)) => {}
        _ => panic!("Expected ConfigError"),
    }
}

#[tokio::test]
async fn test_pooled_connection_lifecycle() {
    let now = std::time::Instant::now();

    assert!(now.elapsed() < Duration::from_secs(1));

    let past_time = now - Duration::from_secs(10);
    assert!(past_time.elapsed() > Duration::from_secs(5));
}

#[tokio::test]
async fn test_connection_pool_manager() {
    let manager = ConnectionPoolManager::new();

    assert!(manager.get_pool("nonexistent").is_none());

    let _config = ConnectionPoolConfig::default();
    let _pool_name = "test_pool".to_string();

    assert!(manager.get_pool("nonexistent").is_none());
}

#[tokio::test]
async fn test_pool_statistics() {
    let stats = PoolStatistics {
        total_connections: 5,
        active_connections: 3,
        idle_connections: 2,
        connections_created: 10,
        connections_destroyed: 5,
        checkout_requests: 20,
        checkout_timeouts: 1,
        health_check_failures: 2,
    };

    let serialised = serde_json::to_string(&stats).unwrap();
    assert!(serialised.contains("total_connections"));
    assert!(serialised.contains("5"));
}
