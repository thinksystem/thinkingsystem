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

use thiserror::Error;
#[derive(Error, Debug)]
pub enum DataExchangeError {
    #[error("Provider '{0}' not found")]
    ProviderNotFound(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("Serialization failed")]
    Serialization(#[from] serde_json::Error),
    #[error("Kafka client error")]
    Kafka(#[from] rdkafka::error::KafkaError),
    #[error("MQTT client error")]
    Mqtt(#[from] paho_mqtt::Error),
    #[error("HTTP request failed")]
    Http(#[from] reqwest::Error),
    #[error("gRPC call failed: {0}")]
    Grpc(String),
    #[error("QUIC exchange failed: {0}")]
    Quic(String),
    #[error("Invalid connection type string: {0}")]
    InvalidConnectionType(String),
}
