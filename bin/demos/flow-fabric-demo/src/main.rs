// SPDX-License-Identifier: AGPL-3.0-only

use anyhow::Result;
use clap::Parser;
use llm_contracts::{GenerationConfig, LLMRequest, ModelRequirements};
use serde_json::Value;
use std::sync::Arc;
use stele::llm::dynamic_selector::DynamicModelSelector;
use stele::llm::{core::LLMAdapter, unified_adapter::UnifiedLLMAdapter};
use tracing::{debug, info, warn};
use uuid::Uuid;


use gtr_core::{ConsumerFactors, DynamicParameters, PublishedOffering, TrustScore};

#[derive(serde::Serialize, serde::Deserialize)]
struct ApiPlan {
    optimised_url: String,
    method: String,
    expected_format: String,
    data_paths: Vec<String>,
    reasoning: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FabricSelection {
    provider: String,
    price: f64,
    latency_ms: u64,
    trust_score: f64,
    vc_issuer: String,
    vc_id: String,
    final_url: String,
    utility: f64,
}


#[derive(serde::Deserialize, Debug)]
struct ProviderDto {
    id: String,
    trust: f64,
    #[serde(default)]
    utility: Option<f64>,
    #[serde(default)]
    offering: Option<serde_json::Value>,
}

#[derive(Parser, Debug)]
struct Args {
    
    #[arg(long)]
    endpoint: String,

    
    #[arg(long)]
    goal: String,

    
    #[arg(long, default_value_t = false)]
    direct: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let _ = dotenvy::dotenv();

    let args = Args::parse();
    info!(endpoint = %args.endpoint, goal = %args.goal, direct = args.direct, "Starting Flow + Fabric demo");

    
    let models_path = if std::path::Path::new("crates/stele/src/nlu/config/llm_models.yml").exists()
    {
        "crates/stele/src/nlu/config/llm_models.yml"
    } else {
        "../../../crates/stele/src/nlu/config/llm_models.yml"
    };
    let selector = Arc::new(DynamicModelSelector::from_config_path(models_path)?);
    let llm = Arc::new(UnifiedLLMAdapter::new(selector).await?);

    let sample = fetch_endpoint_sample(&args.endpoint)
        .await
        .unwrap_or(serde_json::json!({"note":"fetch failed; planning may still proceed"}));
    let plan = plan_api(&llm, &args.endpoint, &args.goal, &sample).await?;
    info!(optimised_url = %plan.optimised_url, "Plan generated");

    
    let mut selection = if args.direct {
        None
    } else {
        Some(select_via_fabric(&plan).await?)
    };

    let (resp, latency) = execute_call(
        selection
            .as_ref()
            .map(|s| &s.final_url)
            .unwrap_or(&plan.optimised_url),
    )
    .await?;

    if let Some(sel) = selection.as_mut() {
        sel.latency_ms = latency;
    }

    info!("Execution complete");
    println!("=== Plan ===\n{}", serde_json::to_string_pretty(&plan)?);
    if let Some(sel) = selection {
        println!(
            "=== Fabric Selection ===\n{}",
            serde_json::to_string_pretty(&sel)?
        );
    }
    
    println!("=== Response (truncated) ===\n{resp}");

    Ok(())
}

async fn plan_api(
    llm: &Arc<UnifiedLLMAdapter>,
    endpoint: &str,
    goal: &str,
    sample: &Value,
) -> Result<ApiPlan> {
    let prompt = format!(
        "Analyse API response and propose an optimised call plan.\nEndpoint: {endpoint}\nGoal: {goal}\nSample: {}\nReturn JSON with keys: optimised_url, method, expected_format, data_paths (array), reasoning.",
        serde_json::to_string_pretty(sample)?
    );

    
    let mut capabilities = vec!["reasoning".into()];
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        capabilities.insert(0, "anthropic_flow".into());
    }

    let req = LLMRequest { id: Uuid::new_v4(), prompt, system_prompt: Some("You are a planner that must output a single JSON object only. No markdown, no backticks, no prose.".into()), model_requirements: ModelRequirements { capabilities, preferred_speed_tier: None, max_cost_tier: None, min_max_tokens: Some(8000) }, generation_config: GenerationConfig::default(), context: None };

    match llm.generate_response(req).await {
        Ok(resp) => {
            let raw = resp.content;
            debug!(raw = %raw);
            let plan = serde_json::from_str::<ApiPlan>(&raw)
                .or_else(|_| serde_json::from_str::<ApiPlan>(&strip_code_fences(&raw)))
                .ok()
                .or_else(|| {
                    try_extract_json_object(&raw)
                        .and_then(|s| serde_json::from_str::<ApiPlan>(&s).ok())
                })
                .unwrap_or(ApiPlan {
                    optimised_url: endpoint.to_string(),
                    method: "GET".into(),
                    expected_format: "JSON".into(),
                    data_paths: vec![],
                    reasoning: raw,
                });
            Ok(plan)
        }
        Err(e) => {
            warn!(error = %e, "LLM unavailable; fallback plan");
            Ok(ApiPlan {
                optimised_url: endpoint.to_string(),
                method: "GET".into(),
                expected_format: "JSON".into(),
                data_paths: vec![],
                reasoning: "Fallback plan: call endpoint directly; refine client-side.".into(),
            })
        }
    }
}

async fn select_via_fabric(plan: &ApiPlan) -> Result<FabricSelection> {
    let params = DynamicParameters::default();
    let consumer = ConsumerFactors {
        risk_aversion: 0.3,
        budget: 2_000,
        cost_of_failure: 5_000.0,
    };
    let providers = fetch_fabric_providers().await?; 
    if providers.is_empty() {
        anyhow::bail!("no providers returned by fabric");
    }

    
    let mut best: Option<(&ProviderDto, f64, f64, f64)> = None; 
    for p in providers.iter() {
        let trust = TrustScore {
            value: p.trust,
            last_updated_ts: 0,
        };
        
        let (price_per_call_raw, staked_collateral_raw) = p
            .offering
            .as_ref()
            .and_then(|off| {
                let price = off.get("price_per_call").and_then(|v| v.as_f64());
                let stake = off.get("staked_collateral").and_then(|v| v.as_f64());
                match (price, stake) {
                    (Some(pv), Some(sv)) => Some((pv, sv)),
                    _ => None,
                }
            })
            .unwrap_or_else(|| {
                
                let (p_micro, s_col) = gtr_core::core::calculate_supplier_offering(&trust, &params);
                (p_micro as f64, s_col as f64)
            });

        let util = p.utility.unwrap_or_else(|| {
            let offering = PublishedOffering {
                staked_collateral: staked_collateral_raw as u64,
                price_per_call: price_per_call_raw as u64,
            };
            gtr_core::core::calculate_consumer_utility(&offering, &trust, &consumer)
        });

        if best.as_ref().map(|(_, u, _, _)| util > *u).unwrap_or(true) {
            best = Some((p, util, price_per_call_raw, staked_collateral_raw));
        }
    }

    let (chosen, util, price_raw, collateral_raw) = best.expect("at least one provider");

    
    let issuer_secret =
        std::env::var("FABRIC_ISSUER_SECRET").unwrap_or_else(|_| "dev_issuer_secret".into());
    let issuer_aud =
        std::env::var("FABRIC_ISSUER_AUDIENCE").unwrap_or_else(|_| "gtr-fabric-consumer".into());
    let issuer_did =
        std::env::var("FABRIC_ISSUER_DID").unwrap_or_else(|_| "did:steel:issuer".into());
    let jwt_manager =
        steel::iam::jwt::JwtManager::new(&issuer_secret, issuer_did.clone(), issuer_aud.clone());
    let issuer_token = jwt_manager
        .create_token(
            "issuer",
            "issuer@example.com",
            "Fabric Issuer",
            Some(issuer_did.clone()),
            vec!["issuer".into(), "admin".into()],
            1,
        )
        .unwrap();
    let vc_manager = steel::iam::vc::VcManager::new(jwt_manager, issuer_did.clone());

    let perf_map: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();
    let subject_did = format!("did:gtr:supplier:{}", chosen.id);
    let steel_vc = vc_manager
        .create_trust_score_credential(&subject_did, chosen.trust, &perf_map, &issuer_token)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let final_url = if plan.optimised_url.contains('?') {
        format!("{}&provider={}", plan.optimised_url, chosen.id)
    } else {
        format!("{}?provider={}", plan.optimised_url, chosen.id)
    };

    info!(provider = %chosen.id, price_per_call = price_raw, staked = collateral_raw, trust = chosen.trust, utility = util, "Fabric selection summary");

    
    let price_normalised = if price_raw > 1_000.0 {
        price_raw / 1_000_000.0
    } else {
        price_raw
    };

    Ok(FabricSelection {
        provider: chosen.id.clone(),
        price: price_normalised,
        latency_ms: 0, 
        trust_score: chosen.trust,
        vc_issuer: steel_vc.proof.verification_method.clone(),
        vc_id: steel_vc.id.unwrap_or_default(),
        final_url,
        utility: util,
    })
}

async fn fetch_fabric_providers() -> Result<Vec<ProviderDto>> {
    let url = std::env::var("FABRIC_PROVIDER_URL")
        .unwrap_or_else(|_| "http://localhost:4000/providers".into());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let res = client.get(&url).send().await?;
    if !res.status().is_success() {
        anyhow::bail!("status {}", res.status());
    }
    let list = res.json::<Vec<ProviderDto>>().await?;
    Ok(list)
}

async fn execute_call(url: &str) -> Result<(String, u64)> {
    let start = std::time::Instant::now();
    let client = reqwest::Client::builder()
        .user_agent("thinkingsystem-flow-fabric-demo/0.1")
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let res = client.get(url).send().await?;
    let text = res.text().await.unwrap_or_default();
    let elapsed = start.elapsed().as_millis() as u64;
    tracing::info!(latency_ms = elapsed, "API call latency measured");
    Ok((text.chars().take(1000).collect(), elapsed))
}

async fn fetch_endpoint_sample(url: &str) -> Result<Value> {
    let client = reqwest::Client::builder()
        .user_agent("thinkingsystem-flow-fabric-demo/0.1")
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let res = client.get(url).send().await?;
    let val = res
        .json::<Value>()
        .await
        .unwrap_or(serde_json::json!({"note":"non-json"}));
    Ok(val)
}

fn strip_code_fences(s: &str) -> String {
    let s = s.trim();
    if s.starts_with("```") && s.ends_with("```") {
        let inner = s.trim_start_matches("```").trim_end_matches("```").trim();
        if let Some(pos) = inner.find('\n') {
            inner[pos + 1..].to_string()
        } else {
            inner.to_string()
        }
    } else {
        s.to_string()
    }
}

fn try_extract_json_object(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut start = None;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'{' {
            if depth == 0 {
                start = Some(i);
            }
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
            if depth == 0 {
                if let Some(st) = start {
                    return Some(s[st..=i].to_string());
                }
            }
        }
    }
    None
}
