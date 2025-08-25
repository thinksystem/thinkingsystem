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
use tokio::io::{AsyncReadExt, AsyncWriteExt};








#[allow(dead_code)]
pub struct FramedBi<S, R> {
    send: S,
    recv: R,
    max_frame: usize,
}

#[allow(dead_code)]
impl<S, R> FramedBi<S, R>
where
    S: AsyncWriteExt + Unpin + Send,
    R: AsyncReadExt + Unpin + Send,
{
    pub fn new(send: S, recv: R) -> Self {
        Self {
            send,
            recv,
            max_frame: 512 * 1024,
        }
    }

    pub fn with_max_frame(send: S, recv: R, max_frame: usize) -> Self {
        Self {
            send,
            recv,
            max_frame,
        }
    }

    pub async fn send_frame(&mut self, data: &[u8]) -> Result<(), DataExchangeError> {
        if data.len() > u32::MAX as usize {
            return Err(DataExchangeError::Quic("frame too large".into()));
        }
        let len = (data.len() as u32).to_be_bytes();
        self.send
            .write_all(&len)
            .await
            .map_err(|e| DataExchangeError::Quic(format!("frame write header: {e}")))?;
        self.send
            .write_all(data)
            .await
            .map_err(|e| DataExchangeError::Quic(format!("frame write body: {e}")))?;
        Ok(())
    }

    pub async fn recv_frame(&mut self) -> Result<Option<Vec<u8>>, DataExchangeError> {
        let mut len_buf = [0u8; 4];
        
        if let Err(e) = self.recv.read_exact(&mut len_buf).await {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                return Ok(None);
            }
            return Err(DataExchangeError::Quic(format!("frame read header: {e}")));
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > self.max_frame {
            return Err(DataExchangeError::Quic(format!(
                "frame length {len} exceeds max_frame {}",
                self.max_frame
            )));
        }
        let mut data = vec![0u8; len];
        self.recv
            .read_exact(&mut data)
            .await
            .map_err(|e| DataExchangeError::Quic(format!("frame read body: {e}")))?;
        Ok(Some(data))
    }
}

#[async_trait]
#[allow(dead_code)]
pub trait SteelIrohProtocol: Send + Sync {
    type Request: Send + 'static;
    type Response: Send + 'static;
    type Error: std::error::Error + Send + Sync + 'static;

    async fn handle(&self, req: Self::Request) -> Result<Self::Response, Self::Error>;
}

/// Simple JSON request/response adapter over an inner handler already using serde_json::Value.
#[allow(dead_code)]
pub struct JsonValueProtocol<P> {
    inner: P,
}

#[allow(dead_code)]
impl<P> JsonValueProtocol<P> {
    pub fn new(inner: P) -> Self {
        Self { inner }
    }
}

#[async_trait]
#[allow(dead_code)]
impl<P> SteelIrohProtocol for JsonValueProtocol<P>
where
    P: Send
        + Sync
        + 'static
        + SteelIrohProtocol<Request = serde_json::Value, Response = serde_json::Value>,
{
    type Request = serde_json::Value;
    type Response = serde_json::Value;
    type Error = P::Error;

    async fn handle(&self, req: Self::Request) -> Result<Self::Response, Self::Error> {
        self.inner.handle(req).await
    }
}



pub async fn build_endpoint(secret: SecretKey, alpns: &[&[u8]]) -> anyhow::Result<Endpoint> {
    let ep = Endpoint::builder()
        .secret_key(secret)
        .alpns(alpns.iter().map(|a| a.to_vec()).collect())
        .relay_mode(RelayMode::Default)
        .discovery_n0()
        .bind()
        .await?;
    Ok(ep)
}













pub struct QuicExchangeProvider {
    
    endpoint: Endpoint,
    
    peer: QuicPeer,
    
    alpn: Vec<u8>,
    
    max_response_bytes: usize,
    
    mode: QuicMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuicMode {
    Bi,
    Uni,
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
        let addrs = parse_addrs(config.config.get("addrs"))?;
        let relay_url = match config.config.get("relay_url") {
            Some(raw) => Some(RelayUrl::from_str(raw).map_err(|e| {
                DataExchangeError::Configuration(format!("invalid relay_url: {e}"))
            })?),
            None => None,
        };
        let peer = if !addrs.is_empty() || relay_url.is_some() {
            QuicPeer::NodeAddr(NodeAddr::from_parts(node_id, relay_url, addrs))
        } else {
            QuicPeer::NodeId(node_id)
        };

        
        let max_response_bytes = if let Some(raw) = config.config.get("max_response_bytes") {
            raw.parse::<usize>().map_err(|e| {
                DataExchangeError::Configuration(format!("invalid max_response_bytes: {e}"))
            })?
        } else {
            512 * 1024
        };

        let mode = match config.config.get("mode").map(|s| s.as_str()) {
            Some("uni") => QuicMode::Uni,
            Some("bi") | None => QuicMode::Bi,
            Some(other) => {
                return Err(DataExchangeError::Configuration(format!(
                    "unsupported mode '{other}', expected 'bi' or 'uni'"
                )))
            }
        };

        Ok(Self {
            endpoint,
            peer,
            alpn,
            max_response_bytes,
            mode,
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

        let buf = match self.mode {
            QuicMode::Bi => {
                let (mut send, mut recv) = conn
                    .open_bi()
                    .await
                    .map_err(|e| DataExchangeError::Quic(format!("open_bi: {e}")))?;
                send.write_all(request.as_bytes())
                    .await
                    .map_err(|e| DataExchangeError::Quic(format!("send: {e}")))?;
                send.finish()
                    .map_err(|e| DataExchangeError::Quic(format!("finish: {e}")))?;
                recv.read_to_end(self.max_response_bytes)
                    .await
                    .map_err(|e| DataExchangeError::Quic(format!("read: {e}")))?
            }
            QuicMode::Uni => {
                
                let mut send = conn
                    .open_uni()
                    .await
                    .map_err(|e| DataExchangeError::Quic(format!("open_uni: {e}")))?;
                send.write_all(request.as_bytes())
                    .await
                    .map_err(|e| DataExchangeError::Quic(format!("uni send: {e}")))?;
                send.finish()
                    .map_err(|e| DataExchangeError::Quic(format!("uni finish: {e}")))?;
                
                let mut last_err: Option<DataExchangeError> = None;
                let mut attempt = 0u8;
                let recv_buf = loop {
                    match conn.accept_uni().await {
                        Ok(mut recv) => match recv.read_to_end(self.max_response_bytes).await {
                            Ok(b) => break b,
                            Err(e) => {
                                last_err = Some(DataExchangeError::Quic(format!("uni read: {e}")));
                                attempt += 1;
                            }
                        },
                        Err(e) => {
                            
                            let msg = format!("{e}");
                            if msg.contains("closed by peer") && attempt < 3 {
                                attempt += 1;
                                last_err = Some(DataExchangeError::Quic(format!(
                                    "accept_uni retry: {msg}"
                                )));
                            } else {
                                last_err =
                                    Some(DataExchangeError::Quic(format!("accept_uni: {msg}")));
                                attempt += 1;
                            }
                        }
                    }
                    if attempt > 3 {
                        break Vec::new();
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(25 * attempt as u64)).await;
                };
                if recv_buf.is_empty() {
                    if let Some(err) = last_err {
                        return Err(err);
                    }
                }
                recv_buf
            }
        };

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
