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


use iroh::Watcher as _;
use iroh::{Endpoint, RelayMode, SecretKey};
use std::collections::HashMap;
use steel::data_exchange::data_streams::quic::QuicExchangeProvider;
use steel::data_exchange::{ConnectionType, DataExchangeImpl, ProviderConfig};

const ALPN: &str = "steel/data-exchange/0";

#[tokio::test]
async fn quic_provider_uni_mode() {
    
    
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
        if let Some(incoming) = server_clone.accept().await {
            if let Ok(conn) = incoming.await {
                
                if let Ok(mut recv) = conn.accept_uni().await {
                    if let Ok(_buf) = recv.read_to_end(64 * 1024).await {
                        
                        if let Ok(mut send) = conn.open_uni().await {
                            let _ = send.write_all(b"{\"status\":\"ok\"}").await;
                            let _ = send.finish();
                            
                            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                        }
                    }
                }
            }
        }
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
    cfg.insert("mode".to_string(), "uni".to_string());
    let provider_cfg = ProviderConfig {
        name: "quic-uni".into(),
        connection_type: ConnectionType::Quic,
        config: cfg,
    };
    let provider = QuicExchangeProvider::new(&provider_cfg)
        .await
        .expect("init provider");
    let res = provider
        .exchange_data("{}".into())
        .await
        .expect("exchange uni");
    assert_eq!(res.get("status").cloned(), Some("ok".into()));
}
