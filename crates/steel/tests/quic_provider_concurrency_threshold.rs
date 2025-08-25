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
use std::sync::Arc;
use std::time::{Duration, Instant};
use steel::data_exchange::data_streams::quic::QuicExchangeProvider;
use steel::data_exchange::{ConnectionType, DataExchangeImpl, ProviderConfig};

const ALPN: &str = "steel/data-exchange/0";

#[tokio::test]
async fn quic_provider_concurrency_threshold_bi() {
    
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
        name: "quic-bi".into(),
        connection_type: ConnectionType::Quic,
        config: cfg,
    };
    let provider = Arc::new(
        QuicExchangeProvider::new(&provider_cfg)
            .await
            .expect("provider"),
    );

    
    async fn run_attempt(p: Arc<QuicExchangeProvider>, parallel: usize) -> (usize, Vec<u128>) {
        let mut handles = Vec::with_capacity(parallel);
        for i in 0..parallel {
            let pr = p.clone();
            handles.push(tokio::spawn(async move {
                let payload = format!("{{\"ping\":{i}}}");
                let mut attempts = 0usize;
                loop {
                    let start = Instant::now();
                    match pr.exchange_data(payload.clone()).await {
                        Ok(res) => {
                            let ok = res.get("status") == Some(&"ok".to_string());
                            return if ok { Ok(start.elapsed()) } else { Err(()) };
                        }
                        Err(e) => {
                            let msg = format!("{e}");
                            if attempts < 2 && msg.contains("connection lost") {
                                attempts += 1;
                                tokio::time::sleep(Duration::from_millis(10 * (attempts as u64)))
                                    .await;
                                continue;
                            } else {
                                return Err(());
                            }
                        }
                    }
                }
            }));
        }
        let mut errors = 0usize;
        let mut latencies = Vec::new();
        for h in handles {
            match h.await {
                Ok(Ok(dur)) => latencies.push(dur.as_millis()),
                _ => errors += 1,
            }
        }
        (errors, latencies)
    }

    fn summarize(latencies: &[u128]) -> String {
        if latencies.is_empty() {
            return "n=0".into();
        }
        let mut v = latencies.to_vec();
        v.sort_unstable();
        let n = v.len();
        let mean: f64 = v.iter().sum::<u128>() as f64 / n as f64;
        let idx = |p: f64| -> usize { ((p * (n as f64 - 1.0)).round() as usize).min(n - 1) };
        let p50 = v[idx(0.50)];
        let p90 = v[idx(0.90)];
        let p99 = v[idx(0.99)];
        let max = *v.last().unwrap();
        format!("n={n} mean_ms={mean:.2} p50={p50} p90={p90} p99={p99} max={max}")
    }

    
    let max_cap: usize = std::env::var("STEEL_QUIC_MAX_PARALLEL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(128);
    let mut last_ok = 0usize;
    let mut first_fail = max_cap + 1; 
    let mut cur = 4usize;
    println!("THRESHOLD_SEARCH start base={cur} max_cap={max_cap}");
    while cur <= max_cap {
        let (errs, lats) = run_attempt(provider.clone(), cur).await;
        println!(
            "ATTEMPT parallel={cur} errors={errs} latency={}",
            summarize(&lats)
        );
        if errs == 0 {
            last_ok = cur;
            cur *= 2;
        } else {
            first_fail = cur;
            break;
        }
    }
    if first_fail == max_cap + 1 {
        
        println!("RESULT no failure observed up to max_cap={max_cap} (last_ok={last_ok})");
        assert!(last_ok >= 4, "unexpectedly low concurrency threshold");
        return;
    }

    
    if last_ok + 1 < first_fail {
        
        let mut low = last_ok + 1;
        let mut high = first_fail - 1;
        while low <= high {
            let mid = (low + high) / 2;
            let (errs, lats) = run_attempt(provider.clone(), mid).await;
            println!(
                "BINARY_ATTEMPT mid={mid} errors={errs} low={low} high={high} latency={}",
                summarize(&lats)
            );
            if errs == 0 {
                last_ok = mid;
                if mid == high {
                    break;
                }
                low = mid + 1;
            } else {
                if mid == 0 {
                    break;
                }
                if mid == low {
                    break;
                }
                high = mid - 1;
            }
        }
    }

    println!("RESULT max_reliable={last_ok} first_fail={first_fail}");
    
    assert!(last_ok >= 4, "max reliable concurrency too low: {last_ok}");
}
