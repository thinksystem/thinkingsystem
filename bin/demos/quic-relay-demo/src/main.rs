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


use anyhow::{anyhow, Context, Result};
use iroh::endpoint::Connection;
use iroh::{Endpoint, NodeAddr, NodeId, RelayMode, SecretKey, Watcher as _};
use llm_contracts::{GenerationConfig, LLMRequest, ModelRequirements};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{net::SocketAddr, str::FromStr, time::Duration};
use stele::llm::core::LLMAdapter as _;
use stele::llm::unified_adapter::UnifiedLLMAdapter;
use tokio::time::sleep;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Msg {
    from: String,
    to: String,
    text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LlmReq {
    prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LlmResp {
    output: String,
}

fn parse_key(hex_key: &str) -> Result<SecretKey> {
    let bytes = hex::decode(hex_key.trim()).context("invalid hex key")?;
    SecretKey::try_from(bytes.as_slice()).map_err(|e| anyhow!("invalid secret key: {e}"))
}

async fn mk_endpoint(name: &str, sk: SecretKey, relay: RelayMode, alpn: &str) -> Result<Endpoint> {
    let ep = Endpoint::builder()
        .secret_key(sk)
        .alpns(vec![alpn.as_bytes().to_vec()])
        .relay_mode(relay)
        .discovery_n0()
        .bind()
        .await
        .context("bind endpoint")?;
    
    let addrs = ep.direct_addresses().initialized().await;
    let relay = ep.home_relay().initialized().await;
    let addrs_str = addrs
        .into_iter()
        .map(|a| a.addr.to_string())
        .collect::<Vec<_>>()
        .join(",");
    println!("{name} node_id: {}", ep.node_id());
    println!("{name} relay: {relay}");
    println!("{name} addrs: {addrs_str}");
    info!(%name, node_id=%ep.node_id(), "endpoint ready");
    Ok(ep)
}

async fn accept_loop(name: String, ep: Endpoint, llm: Arc<UnifiedLLMAdapter>) -> Result<()> {
    while let Some(incoming) = ep.accept().await {
        let conn = incoming.await.context("accept connection")?;
        let llm2 = llm.clone();
        tokio::spawn(handle_conn(name.clone(), conn, llm2));
    }
    Ok(())
}

async fn handle_conn(name: String, conn: Connection, llm: Arc<UnifiedLLMAdapter>) -> Result<()> {
    let (mut send, mut recv) = conn.accept_bi().await.context("accept_bi")?;
    let buf = recv.read_to_end(64 * 1024).await.context("read_to_end")?;
    let msg: Msg = serde_json::from_slice(&buf).context("json decode")?;
    info!(%name, from=%msg.from, to=%msg.to, text=%msg.text, "received");
    let out = match generate_with_llm(&llm, &msg.text).await {
        Ok(s) => s,
        Err(_) => llm_stub(&msg.text),
    };
    let resp = serde_json::to_vec(&LlmResp { output: out })?;
    send.write_all(&resp).await?;
    send.finish()?;
    
    sleep(Duration::from_millis(50)).await;
    Ok(())
}

fn llm_stub(prompt: &str) -> String {
    
    let mut s = prompt.chars().rev().collect::<String>();
    s.make_ascii_uppercase();
    s
}

#[derive(Debug)]
struct Args {
    role: String, 
    secret_key_hex: String,
    peer: Option<String>,       
    peer_addrs: Option<String>, 
    alpn: String,
    triad: Option<String>, 
}

fn parse_args() -> Result<Args> {
    let mut role = String::from("alice");
    let mut secret_key_hex = String::new();
    let mut peer = None;
    let mut peer_addrs = None;
    let mut alpn = String::from("steel/data-exchange/0");
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--role" => role = it.next().unwrap_or_else(|| role.clone()),
            "--secret" => secret_key_hex = it.next().unwrap_or_default(),
            "--peer" => peer = it.next(),
            "--peer-addrs" => peer_addrs = it.next(),
            "--alpn" => alpn = it.next().unwrap_or(alpn),
            "--triad" => {
                
                let v = it.next();
                if let Some(s) = v {
                    
                    let _ = s.clone();
                    
                    std::env::set_var("QUIC_DEMO_TRIAD", s);
                }
            }
            _ => {}
        }
    }
    let triad_env = std::env::var("QUIC_DEMO_TRIAD").ok();
    let triad = triad_env.clone();
    if secret_key_hex.is_empty() && triad_env.is_none() {
        return Err(anyhow!("--secret <hex> is required"));
    }
    Ok(Args {
        role,
        secret_key_hex,
        peer,
        peer_addrs,
        alpn,
        triad,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let args = parse_args()?;
    if let Some(triad) = &args.triad {
        
        let parts: Vec<&str> = triad.split(',').collect();
        if parts.len() != 3 {
            return Err(anyhow!(
                "--triad expects three hex keys separated by commas"
            ));
        }
        let sk_a = parse_key(parts[0])?;
        let sk_b = parse_key(parts[1])?;
        let sk_c = parse_key(parts[2])?;
        let alpn = args.alpn.clone();
        
        let llm = Arc::new(UnifiedLLMAdapter::with_defaults().await?);
        let ep_a = mk_endpoint("alice", sk_a, RelayMode::Default, &alpn).await?;
        let ep_b = mk_endpoint("bob", sk_b, RelayMode::Default, &alpn).await?;
        let ep_c = mk_endpoint("charlie", sk_c, RelayMode::Default, &alpn).await?;
        
        tokio::spawn(accept_loop("alice".into(), ep_a.clone(), llm.clone()));
        tokio::spawn(accept_loop("bob".into(), ep_b.clone(), llm.clone()));
        tokio::spawn(accept_loop("charlie".into(), ep_c.clone(), llm.clone()));
        
        let mut i: u64 = 0;
        loop {
            i += 1;
            let (sender_name, sender_ep, receiver_name, receiver_ep) = match i % 3 {
                0 => ("bob", &ep_b, "alice", &ep_a),
                1 => ("charlie", &ep_c, "bob", &ep_b),
                _ => ("alice", &ep_a, "charlie", &ep_c),
            };
            let recv_id = receiver_ep.node_id();
            let recv_addrs = receiver_ep
                .direct_addresses()
                .initialized()
                .await
                .into_iter()
                .map(|a| a.addr)
                .collect::<Vec<SocketAddr>>();
            let recv_addr = NodeAddr::from_parts(recv_id, None, recv_addrs);
            if let Ok(conn) = sender_ep.connect(recv_addr, alpn.as_bytes()).await {
                if let Ok((mut send, mut recv)) = conn.open_bi().await {
                    let text = format!("msg#{i} from {sender_name} to {receiver_name}");
                    let msg = Msg {
                        from: sender_name.into(),
                        to: receiver_name.into(),
                        text,
                    };
                    let _ = send.write_all(&serde_json::to_vec(&msg).unwrap()).await;
                    let _ = send.finish();
                    if let Ok(out) = recv.read_to_end(64 * 1024).await {
                        if let Ok(resp) = serde_json::from_slice::<LlmResp>(&out) {
                            println!("{sender_name} <- {receiver_name} reply: {}", resp.output);
                        }
                    }
                }
            }
            sleep(Duration::from_millis(750)).await;
        }
    }

    let sk = parse_key(&args.secret_key_hex)?;

    let relay_mode = RelayMode::Default;

    let ep = mk_endpoint(&args.role, sk, relay_mode, &args.alpn).await?;

    
    let llm = Arc::new(UnifiedLLMAdapter::with_defaults().await?);
    let ep_accept = ep.clone();
    let name = args.role.clone();
    tokio::spawn(async move {
        let _ = accept_loop(name, ep_accept, llm).await;
    });

    
    if let Some(peer_hex) = args.peer {
        let peer_id = NodeId::from_str(peer_hex.trim()).context("peer node id")?;
        
        let addr = if let Some(addrs) = args.peer_addrs.clone() {
            let s_addrs: Vec<SocketAddr> =
                addrs.split(',').filter_map(|s| s.parse().ok()).collect();
            NodeAddr::from_parts(peer_id, None, s_addrs)
        } else {
            
            peer_id.into()
        };
        let conn = ep
            .connect(addr, args.alpn.as_bytes())
            .await
            .context("connect")?;
        let (mut send, mut recv) = conn.open_bi().await.context("open_bi")?;
        let msg = Msg {
            from: args.role.clone(),
            to: "peer".into(),
            text: "hello from demo".into(),
        };
        let payload = serde_json::to_vec(&msg)?;
        send.write_all(&payload).await?;
        send.finish()?;
        let out = recv.read_to_end(64 * 1024).await?;
        let resp: LlmResp = serde_json::from_slice(&out)?;
        info!(reply=%resp.output, "got reply");
    }

    
    tokio::signal::ctrl_c().await?;
    Ok(())
}

async fn generate_with_llm(llm: &UnifiedLLMAdapter, prompt: &str) -> Result<String> {
    let req = LLMRequest {
        id: uuid::Uuid::new_v4(),
        prompt: prompt.to_string(),
        system_prompt: None,
        model_requirements: ModelRequirements {
            capabilities: vec!["reasoning".into()],
            preferred_speed_tier: None,
            max_cost_tier: None,
            min_max_tokens: None,
        },
        generation_config: GenerationConfig {
            max_tokens: Some(64),
            temperature: Some(0.3),
            top_p: None,
            stop_sequences: None,
            stream: Some(false),
        },
        context: None,
    };
    let resp = llm.generate_response(req).await?;
    Ok(resp.content)
}
