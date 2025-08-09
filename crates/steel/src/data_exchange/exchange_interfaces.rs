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

use crate::data_exchange::error::DataExchangeError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionType {
    Grpc,
    Kafka,
    Mqtt,
    Rest,
    Webhook,
}
impl FromStr for ConnectionType {
    type Err = DataExchangeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "grpc" => Ok(ConnectionType::Grpc),
            "kafka" => Ok(ConnectionType::Kafka),
            "mqtt" => Ok(ConnectionType::Mqtt),
            "rest" => Ok(ConnectionType::Rest),
            "webhook" => Ok(ConnectionType::Webhook),
            _ => Err(DataExchangeError::InvalidConnectionType(s.to_string())),
        }
    }
}
#[async_trait]
pub trait DataExchangeImpl<Req, Res>: Send + Sync {
    async fn exchange_data(&self, request: Req) -> Res;
}
