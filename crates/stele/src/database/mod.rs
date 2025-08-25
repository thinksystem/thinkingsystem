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

pub mod connection;
pub mod data_interpreter;
pub mod data_processor;
pub mod dynamic_access;
pub mod dynamic_query_generator;
pub mod dynamic_storage;
pub mod diff;
pub mod health_monitor;
pub mod intent_analyser;
pub mod knowledge_adapter;
pub mod operations;
pub mod prompt_builder;
pub mod query_builder;
pub mod query_generator;
pub mod query_kg;
pub mod query_metrics;
pub mod query_router;
pub mod query_validator;
pub mod regulariser;
pub mod sanitize;
pub mod schema_analyser;
pub mod structured_store;
pub mod surreal_token;
pub mod tokens;
pub mod types;
pub use connection::DatabaseConnection;
pub use surreal_token::{SurrealToken, SurrealTokenParser};
pub use types::{DatabaseError, DatabaseMetrics, QueryResult};
