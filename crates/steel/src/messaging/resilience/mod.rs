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

pub mod circuit_breaker;
pub mod connection_pool;

pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitBreakerManager,
    CircuitBreakerMetrics, CircuitBreakerState,
};
pub use connection_pool::{
    ConnectionPool, ConnectionPoolConfig, ConnectionPoolError, ConnectionPoolManager,
    DatabaseAuthConfig, PoolStatistics, PooledConnection,
};

pub type Result<T, E> = std::result::Result<T, CircuitBreakerError<E>>;
