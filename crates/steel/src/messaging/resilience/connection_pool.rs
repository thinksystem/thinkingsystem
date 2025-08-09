#![cfg(feature = "surrealdb")]
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

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use surrealdb::engine::remote::ws::{Client, Ws};
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;
use thiserror::Error;
use tokio::sync::{RwLock, Semaphore};
use tokio::time::timeout;

use super::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};

#[derive(Error, Debug)]
pub enum ConnectionPoolError {
    #[error("Connection pool exhausted")]
    PoolExhausted,
    #[error("Connection timeout: {0}")]
    Timeout(String),
    #[error("Database connection failed: {0}")]
    DatabaseError(String),
    #[error("Pool configuration error: {0}")]
    ConfigError(String),
    #[error("Connection validation failed: {0}")]
    ValidationError(String),
    #[error("Circuit breaker is open")]
    CircuitBreakerOpen,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPoolConfig {
    pub max_connections: usize,

    pub min_connections: usize,

    pub connection_timeout: Duration,

    pub idle_timeout: Duration,

    pub cleanup_interval: Duration,

    pub max_connection_lifetime: Duration,

    pub max_retries: u32,

    pub retry_delay: Duration,

    pub database_url: String,

    pub auth_config: DatabaseAuthConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseAuthConfig {
    pub username: String,
    pub password: String,
    pub namespace: String,
    pub database: String,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 2,
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(300),
            cleanup_interval: Duration::from_secs(60),
            max_connection_lifetime: Duration::from_secs(3600),
            max_retries: 3,
            retry_delay: Duration::from_millis(500),
            database_url: "ws://localhost:8000".to_string(),
            auth_config: DatabaseAuthConfig {
                username: "root".to_string(),
                password: "root".to_string(),
                namespace: "test".to_string(),
                database: "test".to_string(),
            },
        }
    }
}

#[derive(Debug)]
pub struct PooledConnection {
    pub connection: Surreal<Client>,
    pub created_at: Instant,
    pub last_used: Instant,
    pub use_count: u64,
    pub is_healthy: bool,
}

impl PooledConnection {
    pub fn new(connection: Surreal<Client>) -> Self {
        let now = Instant::now();
        Self {
            connection,
            created_at: now,
            last_used: now,
            use_count: 0,
            is_healthy: true,
        }
    }

    pub fn touch(&mut self) {
        self.last_used = Instant::now();
        self.use_count += 1;
    }

    pub fn is_expired(&self, max_lifetime: Duration) -> bool {
        self.created_at.elapsed() > max_lifetime
    }

    pub fn is_idle(&self, idle_timeout: Duration) -> bool {
        self.last_used.elapsed() > idle_timeout
    }

    pub async fn health_check(&mut self) -> bool {
        match self.connection.health().await {
            Ok(_) => {
                self.is_healthy = true;
                true
            }
            Err(_) => {
                self.is_healthy = false;
                false
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PoolStatistics {
    pub total_connections: usize,
    pub active_connections: usize,
    pub idle_connections: usize,
    pub connections_created: u64,
    pub connections_destroyed: u64,
    pub checkout_requests: u64,
    pub checkout_timeouts: u64,
    pub health_check_failures: u64,
}

#[derive(Debug)]
pub struct ConnectionPool {
    config: ConnectionPoolConfig,
    available: Arc<RwLock<Vec<PooledConnection>>>,
    active: Arc<DashMap<String, PooledConnection>>,
    semaphore: Arc<Semaphore>,
    circuit_breaker: Arc<CircuitBreaker>,
    statistics: Arc<RwLock<PoolStatistics>>,
}

impl ConnectionPool {
    pub async fn new(config: ConnectionPoolConfig) -> Result<Self, ConnectionPoolError> {
        if config.min_connections > config.max_connections {
            return Err(ConnectionPoolError::ConfigError(
                "min_connections cannot be greater than max_connections".to_string(),
            ));
        }

        let circuit_breaker_config = CircuitBreakerConfig {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(60),
            half_open_max_calls: 3,
            minimum_throughput: 10,
            sliding_window_size: Duration::from_secs(60),
            success_threshold: 2,
        };

        let circuit_breaker = Arc::new(CircuitBreaker::new(
            "database_pool".to_string(),
            circuit_breaker_config,
        ));

        let semaphore = Arc::new(Semaphore::new(config.max_connections));
        let available = Arc::new(RwLock::new(Vec::new()));
        let active = Arc::new(DashMap::new());
        let statistics = Arc::new(RwLock::new(PoolStatistics {
            total_connections: 0,
            active_connections: 0,
            idle_connections: 0,
            connections_created: 0,
            connections_destroyed: 0,
            checkout_requests: 0,
            checkout_timeouts: 0,
            health_check_failures: 0,
        }));

        let pool = Self {
            config: config.clone(),
            available,
            active,
            semaphore,
            circuit_breaker,
            statistics,
        };

        pool.ensure_min_connections().await?;

        pool.start_cleanup_task().await;

        Ok(pool)
    }

    pub async fn get_connection(&self) -> Result<String, ConnectionPoolError> {
        {
            let mut stats = self.statistics.write().await;
            stats.checkout_requests += 1;
        }

        if !self.circuit_breaker.can_execute().await {
            return Err(ConnectionPoolError::CircuitBreakerOpen);
        }

        let _permit = timeout(self.config.connection_timeout, self.semaphore.acquire())
            .await
            .map_err(|_| {
                tokio::spawn({
                    let stats = Arc::clone(&self.statistics);
                    async move {
                        let mut stats = stats.write().await;
                        stats.checkout_timeouts += 1;
                    }
                });
                ConnectionPoolError::Timeout("Failed to acquire connection permit".to_string())
            })?
            .map_err(|_| ConnectionPoolError::PoolExhausted)?;

        if let Some(mut connection) = self.take_available_connection().await {
            if connection.health_check().await {
                connection.touch();
                let connection_id = uuid::Uuid::new_v4().to_string();
                self.active.insert(connection_id.clone(), connection);

                {
                    let mut stats = self.statistics.write().await;
                    stats.active_connections = self.active.len();
                    stats.idle_connections = self.available.read().await.len();
                }

                return Ok(connection_id);
            } else {
                self.destroy_connection(connection).await;
            }
        }

        let connection = self.create_connection().await?;
        let connection_id = uuid::Uuid::new_v4().to_string();
        self.active.insert(connection_id.clone(), connection);

        {
            let mut stats = self.statistics.write().await;
            stats.active_connections = self.active.len();
            stats.idle_connections = self.available.read().await.len();
        }

        Ok(connection_id)
    }

    pub async fn return_connection(
        &self,
        connection_id: String,
    ) -> Result<(), ConnectionPoolError> {
        if let Some((_, mut connection)) = self.active.remove(&connection_id) {
            if connection.is_healthy
                && !connection.is_expired(self.config.max_connection_lifetime)
                && connection.health_check().await
            {
                let mut available = self.available.write().await;
                available.push(connection);
            } else {
                self.destroy_connection(connection).await;
            }

            {
                let mut stats = self.statistics.write().await;
                stats.active_connections = self.active.len();
                stats.idle_connections = self.available.read().await.len();
            }

            Ok(())
        } else {
            Err(ConnectionPoolError::ValidationError(
                "Connection ID not found".to_string(),
            ))
        }
    }

    pub fn get_connection_ref(
        &self,
        connection_id: &str,
    ) -> Option<dashmap::mapref::one::Ref<String, PooledConnection>> {
        self.active.get(connection_id)
    }

    async fn create_connection(&self) -> Result<PooledConnection, ConnectionPoolError> {
        let operation = || async {
            let db = Surreal::new::<Ws>(&self.config.database_url)
                .await
                .map_err(|e| ConnectionPoolError::DatabaseError(e.to_string()))?;

            db.signin(Root {
                username: &self.config.auth_config.username,
                password: &self.config.auth_config.password,
            })
            .await
            .map_err(|e| ConnectionPoolError::DatabaseError(e.to_string()))?;

            db.use_ns(&self.config.auth_config.namespace)
                .use_db(&self.config.auth_config.database)
                .await
                .map_err(|e| ConnectionPoolError::DatabaseError(e.to_string()))?;

            Ok(PooledConnection::new(db))
        };

        match self.circuit_breaker.call(operation).await {
            Ok(connection) => {
                {
                    let mut stats = self.statistics.write().await;
                    stats.connections_created += 1;
                    stats.total_connections += 1;
                }
                Ok(connection)
            }
            Err(circuit_err) => match circuit_err {
                super::circuit_breaker::CircuitBreakerError::CircuitOpen => {
                    Err(ConnectionPoolError::CircuitBreakerOpen)
                }
                super::circuit_breaker::CircuitBreakerError::CallFailed(inner_err) => {
                    Err(inner_err)
                }
                super::circuit_breaker::CircuitBreakerError::Timeout(msg) => {
                    Err(ConnectionPoolError::Timeout(msg))
                }
                super::circuit_breaker::CircuitBreakerError::HalfOpenLimitExceeded => {
                    Err(ConnectionPoolError::CircuitBreakerOpen)
                }
            },
        }
    }

    async fn take_available_connection(&self) -> Option<PooledConnection> {
        let mut available = self.available.write().await;
        available.pop()
    }

    async fn destroy_connection(&self, _connection: PooledConnection) {
        let mut stats = self.statistics.write().await;
        stats.connections_destroyed += 1;
        stats.total_connections = stats.total_connections.saturating_sub(1);
    }

    async fn ensure_min_connections(&self) -> Result<(), ConnectionPoolError> {
        let available_count = self.available.read().await.len();
        let needed = self.config.min_connections.saturating_sub(available_count);

        for _ in 0..needed {
            let connection = self.create_connection().await?;
            let mut available = self.available.write().await;
            available.push(connection);
        }

        Ok(())
    }

    async fn start_cleanup_task(&self) {
        let available = Arc::clone(&self.available);
        let statistics = Arc::clone(&self.statistics);
        let cleanup_interval = self.config.cleanup_interval;
        let idle_timeout = self.config.idle_timeout;
        let max_lifetime = self.config.max_connection_lifetime;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_interval);

            loop {
                interval.tick().await;

                let mut available_guard = available.write().await;
                let mut stats = statistics.write().await;

                available_guard.retain(|conn| {
                    let keep = !conn.is_expired(max_lifetime) && !conn.is_idle(idle_timeout);
                    if !keep {
                        stats.connections_destroyed += 1;
                        stats.total_connections = stats.total_connections.saturating_sub(1);
                    }
                    keep
                });

                stats.idle_connections = available_guard.len();
            }
        });
    }

    pub async fn get_statistics(&self) -> PoolStatistics {
        let stats = self.statistics.read().await;
        let mut current_stats = stats.clone();
        current_stats.active_connections = self.active.len();
        current_stats.idle_connections = self.available.read().await.len();
        current_stats
    }

    pub async fn shutdown(&mut self) {
        self.active.clear();

        let mut available = self.available.write().await;
        available.clear();
    }
}

pub struct ConnectionPoolManager {
    pools: DashMap<String, Arc<ConnectionPool>>,
}

impl ConnectionPoolManager {
    pub fn new() -> Self {
        Self {
            pools: DashMap::new(),
        }
    }

    pub async fn create_pool(
        &self,
        name: String,
        config: ConnectionPoolConfig,
    ) -> Result<(), ConnectionPoolError> {
        let pool = Arc::new(ConnectionPool::new(config).await?);
        self.pools.insert(name, pool);
        Ok(())
    }

    pub fn get_pool(&self, name: &str) -> Option<Arc<ConnectionPool>> {
        self.pools.get(name).map(|entry| Arc::clone(entry.value()))
    }

    pub async fn remove_pool(&self, name: &str) -> Option<Arc<ConnectionPool>> {
        self.pools.remove(name).map(|(_, pool)| pool)
    }

    pub async fn get_all_statistics(&self) -> HashMap<String, PoolStatistics> {
        let mut all_stats = HashMap::new();

        for entry in self.pools.iter() {
            let pool_name = entry.key().clone();
            let pool = entry.value();
            let stats = pool.get_statistics().await;
            all_stats.insert(pool_name, stats);
        }

        all_stats
    }

    pub async fn shutdown_all(&self) {
        for _entry in self.pools.iter() {}
        self.pools.clear();
    }
}

impl Default for ConnectionPoolManager {
    fn default() -> Self {
        Self::new()
    }
}
