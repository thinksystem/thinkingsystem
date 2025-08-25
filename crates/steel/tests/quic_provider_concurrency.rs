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
use iroh::{Endpoint, RelayMode, SecretKey};
use iroh::Watcher as _;
use steel::data_exchange::data_streams::quic::QuicExchangeProvider;
use steel::data_exchange::{ConnectionType, ProviderConfig, DataExchangeImpl};

const ALPN: &str = "steel/data-exchange/0";

#[tokio::test]
async fn quic_provider_concurrency_bi() {
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
        while let Some(incoming) = server_clone.accept().await {
            if let Ok(conn) = incoming.await {
                tokio::spawn(async move {
                    if let Ok((mut send, mut recv)) = conn.accept_bi().await {
                        if let Ok(_buf) = recv.read_to_end(64 * 1024).await {
                            let _ = send.write_all(b"{\"status\":\"ok\"}").await;
                            let _ = send.finish();
                            
                            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                        }
                    }
                });
            }
        }
    });
    
    let relay_url = server.home_relay().initialized().await;
    let addrs = server.direct_addresses().initialized().await;
    let addrs_str = addrs.into_iter().map(|a| a.addr.to_string()).collect::<Vec<_>>().join(",");
    let mut cfg = HashMap::new();
    cfg.insert("node_id".to_string(), server_id.to_string());
    cfg.insert("relay_mode".to_string(), "default".to_string());
    cfg.insert("alpn".to_string(), ALPN.to_string());
    cfg.insert("addrs".to_string(), addrs_str);
    cfg.insert("relay_url".to_string(), relay_url.to_string());
    let provider_cfg = ProviderConfig { name: "quic-bi".into(), connection_type: ConnectionType::Quic, config: cfg };
    let provider = Arc::new(QuicExchangeProvider::new(&provider_cfg).await.expect("provider"));

    let parallel = 16usize; 
    let mut handles = Vec::new();
    for i in 0..parallel {
        let p = provider.clone();
        handles.push(tokio::spawn(async move {
            let payload = format!("{{\"ping\":{i}}}");
            
            let mut attempts = 0;
            loop {
                match p.exchange_data(payload.clone()).await {
                    Ok(res) => { assert_eq!(res.get("status"), Some(&"ok".to_string())); break; }
                    Err(e) => {
                        let msg = format!("{e}");
                        if attempts < 3 && msg.contains("connection lost") {
                            attempts += 1;
                            tokio::time::sleep(std::time::Duration::from_millis(15 * attempts)).await;
                            continue;
                        } else { panic!("exchange failure: {msg}"); }
                    }
                }
            }
        }));
    }
    for h in handles { h.await.expect("join"); }
}
