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
use crate::data_exchange::exchange_core::ProviderConfig;
use crate::data_exchange::exchange_interfaces::DataExchangeImpl;
use paho_mqtt::{AsyncClient, ConnectOptionsBuilder, CreateOptionsBuilder, Message};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;
pub struct MqttExchangeProvider {
    client: AsyncClient,
    topic: String,
}
impl MqttExchangeProvider {
    pub async fn new(config: &ProviderConfig) -> Result<Self, DataExchangeError> {
        let broker_url = config.config.get("broker_url").ok_or_else(|| {
            DataExchangeError::Configuration("Missing 'broker_url' for MQTT provider".to_string())
        })?;
        let topic = config.config.get("topic").ok_or_else(|| {
            DataExchangeError::Configuration("Missing 'topic' for MQTT provider".to_string())
        })?;
        let client_id = format!("ts-mqtt-provider-{}", Uuid::new_v4());
        let create_opts = CreateOptionsBuilder::new()
            .server_uri(broker_url)
            .client_id(client_id)
            .finalize();
        let client = AsyncClient::new(create_opts).map_err(DataExchangeError::Mqtt)?;
        let conn_opts = ConnectOptionsBuilder::new()
            .keep_alive_interval(Duration::from_secs(20))
            .clean_session(true)
            .finalize();
        client
            .connect(conn_opts)
            .await
            .map_err(DataExchangeError::Mqtt)?;
        Ok(Self {
            client,
            topic: topic.clone(),
        })
    }
}
#[async_trait::async_trait]
impl DataExchangeImpl<String, Result<HashMap<String, String>, DataExchangeError>>
    for MqttExchangeProvider
{
    async fn exchange_data(
        &self,
        request: String,
    ) -> Result<HashMap<String, String>, DataExchangeError> {
        let message_id = Uuid::new_v4().to_string();
        let message = Message::new_retained(&self.topic, request.as_bytes(), 1);
        self.client
            .publish(message)
            .await
            .map_err(DataExchangeError::Mqtt)?;
        let mut response = HashMap::new();
        response.insert("status".to_string(), "published".to_string());
        response.insert("message_id".to_string(), message_id);
        response.insert("topic".to_string(), self.topic.clone());
        Ok(response)
    }
}
