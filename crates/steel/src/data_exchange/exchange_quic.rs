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
use async_trait::async_trait;
use iroh::{Endpoint, NodeAddr, NodeId, RelayMode, RelayUrl, SecretKey};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;













pub struct QuicExchangeProvider {
    
    endpoint: Endpoint,
    
    peer: QuicPeer,
    
    alpn: Vec<u8>,
}


enum QuicPeer {
    
    NodeId(NodeId),
    
    NodeAddr(NodeAddr),
}

impl QuicExchangeProvider {
    
    pub async fn new(config: &ProviderConfig) -> Result<Self, DataExchangeError> {
        
        let alpn = config
            .config
            .get("alpn")
            .cloned()
            .unwrap_or_else(|| "steel/data-exchange/0".to_string())
            .into_bytes();

        
        let secret_key = match config.config.get("secret_key") {
            Some(sk) => SecretKey::from_str(sk).map_err(|e| {
                DataExchangeError::Configuration(format!("invalid secret_key: {e}"))
            })?,
            None => SecretKey::generate(rand::rngs::OsRng),
        };

        let relay_mode = match config.config.get("relay_mode").map(|s| s.as_str()) {
            Some("disabled") => RelayMode::Disabled,
            Some("default") | None => RelayMode::Default,
            Some(other) => {
                return Err(DataExchangeError::Configuration(format!(
                    "Unsupported relay_mode: {other}"
                )))
            }
        };

        let mut builder = Endpoint::builder()
            .secret_key(secret_key)
            .alpns(vec![alpn.clone()])
            .relay_mode(relay_mode);

        
        let discovery_enabled = config
            .config
            .get("discovery_n0")
            .map(|v| v == "true")
            .unwrap_or(true);
        if discovery_enabled {
            builder = builder.discovery_n0();
        }

        let endpoint = builder
            .bind()
            .await
            .map_err(|e| DataExchangeError::Quic(format!("endpoint bind: {e}")))?;

        
        let node_id_str = config.config.get("node_id").ok_or_else(|| {
            DataExchangeError::Configuration("Missing 'node_id' for QUIC provider".to_string())
        })?;
        let node_id = NodeId::from_str(node_id_str)
            .map_err(|e| DataExchangeError::Configuration(format!("invalid node_id: {e}")))?;
        let peer = if let Some(relay_url_raw) = config.config.get("relay_url") {
            let relay_url = RelayUrl::from_str(relay_url_raw)
                .map_err(|e| DataExchangeError::Configuration(format!("invalid relay_url: {e}")))?;
            let addrs = parse_addrs(config.config.get("addrs"))?;
            QuicPeer::NodeAddr(NodeAddr::from_parts(node_id, Some(relay_url), addrs))
        } else {
            QuicPeer::NodeId(node_id)
        };

        Ok(Self {
            endpoint,
            peer,
            alpn,
        })
    }
}

#[async_trait]
impl DataExchangeImpl<String, Result<HashMap<String, String>, DataExchangeError>>
    for QuicExchangeProvider
{
    async fn exchange_data(
        &self,
        request: String,
    ) -> Result<HashMap<String, String>, DataExchangeError> {
        
        let conn = match &self.peer {
            QuicPeer::NodeId(node_id) => self
                .endpoint
                .connect(*node_id, &self.alpn)
                .await
                .map_err(|e| DataExchangeError::Quic(format!("connect by node_id: {e}")))?,
            QuicPeer::NodeAddr(node_addr) => self
                .endpoint
                .connect(node_addr.clone(), &self.alpn)
                .await
                .map_err(|e| DataExchangeError::Quic(format!("connect by node_addr: {e}")))?,
        };

        
        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .map_err(|e| DataExchangeError::Quic(format!("open_bi: {e}")))?;

        
        send.write_all(request.as_bytes())
            .await
            .map_err(|e| DataExchangeError::Quic(format!("send: {e}")))?;
        send.finish()
            .map_err(|e| DataExchangeError::Quic(format!("finish: {e}")))?;

        let buf = recv
            .read_to_end(64 * 1024)
            .await
            .map_err(|e| DataExchangeError::Quic(format!("read: {e}")))?;

        let response: HashMap<String, String> =
            serde_json::from_slice(&buf).map_err(DataExchangeError::Serialization)?;

        Ok(response)
    }
}


fn parse_addrs(raw: Option<&String>) -> Result<Vec<SocketAddr>, DataExchangeError> {
    let Some(raw) = raw else {
        return Ok(vec![]);
    };
    let parts = raw
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty());
    let mut addrs = Vec::new();
    for part in parts {
        let addr = part.parse::<SocketAddr>().map_err(|e| {
            DataExchangeError::Configuration(format!("invalid socket addr '{part}': {e}"))
        })?;
        addrs.push(addr);
    }
    Ok(addrs)
}
