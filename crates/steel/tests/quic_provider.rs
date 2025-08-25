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

use iroh::{Endpoint, RelayMode, SecretKey};
use iroh::Watcher as _;
use steel::data_exchange::data_streams::quic::QuicExchangeProvider;
use steel::data_exchange::DataExchangeImpl;
use steel::data_exchange::{ConnectionType, ProviderConfig};

const ALPN: &str = "steel/data-exchange/0";

#[tokio::test]
async fn quic_provider_happy_path() {
    
    let secret = SecretKey::generate(rand::rngs::OsRng);
    let server = Endpoint::builder()
        .secret_key(secret)
        .alpns(vec![ALPN.as_bytes().to_vec()])
        .relay_mode(RelayMode::Default)
        .bind()
        .await
        .expect("bind server");
    let server_id = server.node_id();
    let server_clone = server.clone();
    tokio::spawn(async move {
        let incoming = server_clone.accept().await.expect("incoming");
        let conn = incoming.await.expect("accept conn");
        let (mut send, mut recv) = conn.accept_bi().await.expect("accept_bi");
        let buf = recv.read_to_end(64 * 1024).await.expect("read_to_end");
        let _v: serde_json::Value = serde_json::from_slice(&buf).expect("json");
        let resp = serde_json::json!({"status":"ok"});
        let bytes = serde_json::to_vec(&resp).unwrap();
        send.write_all(&bytes).await.expect("write_all");
        send.finish().expect("finish");
        
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    });

    
    let relay_url = server.home_relay().initialized().await;
    let addrs = server.direct_addresses().initialized().await;
    let addrs_str = addrs
        .into_iter()
        .map(|a| a.addr.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let mut cfg = HashMap::new();
    cfg.insert("node_id".to_string(), server_id.to_string());
    cfg.insert("relay_mode".to_string(), "default".to_string());
    cfg.insert("alpn".to_string(), ALPN.to_string());
    cfg.insert("addrs".to_string(), addrs_str);
    cfg.insert("relay_url".to_string(), relay_url.to_string());
    let provider_cfg = ProviderConfig {
        name: "quic-test".into(),
        connection_type: ConnectionType::Quic,
        config: cfg,
    };
    let provider = QuicExchangeProvider::new(&provider_cfg)
        .await
        .expect("init quic provider");
    let res = provider
        .exchange_data("{\"ping\":true}".into())
        .await
        .expect("exchange");
    assert_eq!(res.get("status").cloned(), Some("ok".into()));
}

#[tokio::test]
async fn quic_provider_bad_node_id() {
    let mut cfg = HashMap::new();
    cfg.insert("node_id".to_string(), "not-a-node".to_string());
    let provider_cfg = ProviderConfig {
        name: "bad".into(),
        connection_type: ConnectionType::Quic,
        config: cfg,
    };
    let err = QuicExchangeProvider::new(&provider_cfg)
        .await
        .err()
        .expect("err");
    match err {
        steel::data_exchange::DataExchangeError::Configuration(msg) => {
            assert!(msg.contains("node_id"))
        }
        other => panic!("unexpected error: {other:?}"),
    }
}
