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


use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use stele::codegen::artifacts::persist_wat_artifact;
use stele::codegen::wat_pipeline::{attempt_wat_repair, sanitize_validate_with_llm};
use stele::flows::core::BlockDefinition;
use stele::flows::state::UnifiedState;
use stele::nlu::llm_processor::LLMAdapter;
use stele::{BlockType, FlowDefinition, UnifiedFlowEngine};
use tracing::{info, warn};


pub async fn assess_result_anomaly(
    adapter: &dyn LLMAdapter,
    directive: &str,
    plan: &Value,
    result: Option<&Value>,
    error: Option<&str>,
) -> Result<(bool, String)> {
    let system = r#"You are an analytical verifier. Output ONLY JSON:{"anomaly":<bool>,"reason":"<text>","consensus_recommended":<bool>}.
Judge if the observed result appears incorrect, trivially constant, structurally unrelated, or if an error indicates logic flaws.
Only mark anomaly true with a concrete reason. Recommend consensus when an independent alternate implementation could expose divergence."#;
    let mut user = format!(
        "Directive: {directive}\nPlanSummary: {}\n",
        summarize_plan(plan)
    );
    if let Some(e) = error {
        user.push_str(&format!("ExecutionError: {e}\n"));
    }
    if let Some(r) = result {
        user.push_str(&format!("ObservedResultJSON: {r}\n"));
    }
    user.push_str("Respond with JSON now.");
    match adapter.generate_structured_response(system, &user).await {
        Ok(resp) => {
            let anomaly = resp
                .get("anomaly")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let reason = resp
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let consensus = resp
                .get("consensus_recommended")
                .and_then(|v| v.as_bool())
                .unwrap_or(anomaly);
            Ok((consensus, reason))
        }
        Err(_) => Ok((false, "assessment_failed".into())),
    }
}

fn summarize_plan(plan: &Value) -> String {
    let mut s = String::new();
    if let Some(funcs) = plan.get("functions").and_then(|v| v.as_array()) {
        s.push_str(&format!("functions:{} ", funcs.len()));
        for f in funcs.iter().take(3) {
            if let Some(n) = f.get("name").and_then(|v| v.as_str()) {
                s.push_str(n);
                s.push(' ');
            }
        }
    }
    if let Some(flow_id) = plan
        .get("flow")
        .and_then(|f| f.get("id"))
        .and_then(|v| v.as_str())
    {
        s.push_str(&format!("flow:{flow_id}"));
    }
    s.truncate(160);
    s
}


fn primary_function_name(plan: &Value) -> Option<String> {
    plan.get("flow")
        .and_then(|f| f.get("blocks"))
        .and_then(|b| b.as_array())
        .and_then(|blocks| {
            blocks.iter().find_map(|blk| {
                if blk.get("type").and_then(|v| v.as_str()) == Some("compute") {
                    if let Some(expr) = blk.get("expression").and_then(|v| v.as_str()) {
                        if let Some(fn_name) = expr.strip_prefix("function:") {
                            return Some(fn_name.to_string());
                        }
                    }
                }
                None
            })
        })
        .or_else(|| {
            plan.get("functions")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|f| f.get("name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
}

pub fn extract_numeric_arg(plan: &Value) -> Option<f64> {
    plan.get("flow")
        .and_then(|f| f.get("blocks"))
        .and_then(|b| b.as_array())
        .and_then(|blocks| {
            blocks.iter().find_map(|blk| {
                if blk.get("type").and_then(|v| v.as_str()) == Some("compute") {
                    return blk
                        .get("args")
                        .and_then(|a| a.as_array())
                        .and_then(|a| a.first())
                        .and_then(|v| v.as_f64());
                }
                None
            })
        })
}


pub async fn run_consensus_variant(
    adapter: &dyn LLMAdapter,
    directive: &str,
    engine: &mut UnifiedFlowEngine,
    plan: &Value,
    artifacts_dir: &str,
) -> Result<()> {
    let Some(primary) = primary_function_name(plan) else {
        warn!("No primary function for consensus");
        return Ok(());
    };
    let alt_name = format!("{primary}_alt");
    let system = "You output ONLY JSON {\"wat\":\"<module>\"} providing an ALTERNATE WebAssembly Text implementation of the directive. Differ sufficiently in structure (looping style / variable naming / control arrangement) while preserving semantics. Requirements: single (module ...), NO imports, NO undefined helper calls (inline any randomness via deterministic i64 LCG), export alt function with its own name and also with label 'same'.";
    let user = format!("Directive: {directive}\nPrimaryFunction:{primary}\nAltFunction:{alt_name}\nReturn ONLY JSON now.");
    if let Ok(mut resp) = adapter.generate_structured_response(system, &user).await {
        
        let mut schema_attempts = 0u8;
        while resp.get("wat").and_then(|v| v.as_str()).is_none() && schema_attempts < 2 {
            schema_attempts += 1;
            let reinforce_system =
                "STRICT: Respond ONLY JSON with key 'wat' containing (module ...). No other keys.";
            if let Ok(r2) = adapter
                .generate_structured_response(reinforce_system, &user)
                .await
            {
                resp = r2;
            } else {
                break;
            }
        }
        if let Some(mut wat) = resp
            .get("wat")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        {
            let _ = persist_wat_artifact(artifacts_dir, &alt_name, &wat, "alt_raw");
            let mut attempts = 0u8;
            loop {
                attempts += 1;
                match sanitize_validate_with_llm(adapter, directive, &alt_name, "same", &wat, 2)
                    .await
                {
                    Ok(cleaned) => {
                        let _ = persist_wat_artifact(artifacts_dir, &alt_name, &cleaned, "alt");
                        match engine.register_dynamic_source(
                            &alt_name,
                            stele::flows::dynamic_executor::DynamicSource::Wat {
                                name: &alt_name,
                                export: "same",
                                wat: &cleaned,
                            },
                        ) {
                            Ok(_) => info!(alt=%alt_name, "Consensus alt function registered"),
                            Err(e) => {
                                warn!(alt=%alt_name, error=%e, "Failed to register consensus alt")
                            }
                        }
                        if let Some(arg) = extract_numeric_arg(plan) {
                            let v_primary = harness_invoke(engine, &primary, arg)
                                .await
                                .unwrap_or(f64::NAN);
                            let v_alt = harness_invoke(engine, &alt_name, arg)
                                .await
                                .unwrap_or(f64::NAN);
                            if v_primary.is_finite() && v_alt.is_finite() {
                                let avg = ((v_primary.abs() + v_alt.abs()) / 2.0).max(1e-12);
                                let rel = (v_primary - v_alt).abs() / avg;
                                info!(primary=%v_primary, alternate=%v_alt, rel_diff=%rel, "Consensus comparison");
                                if rel > 0.05 {
                                    warn!(%rel, "High variance between implementations");
                                }
                            } else {
                                warn!(primary=%v_primary, alt=%v_alt, "Consensus values non-finite");
                            }
                        }
                        break;
                    }
                    Err(e) => {
                        if attempts >= 3 {
                            warn!(alt=%alt_name, error=%e, "Consensus alt sanitize failed after retries");
                            break;
                        }
                        match attempt_wat_repair(
                            adapter,
                            directive,
                            &alt_name,
                            "same",
                            &wat,
                            &e.to_string(),
                        )
                        .await
                        {
                            Ok(new_wat) => {
                                wat = new_wat;
                                let _ = persist_wat_artifact(
                                    artifacts_dir,
                                    &alt_name,
                                    &wat,
                                    &format!("alt_repair_attempt{attempts}"),
                                );
                                continue;
                            }
                            Err(rerr) => {
                                warn!(alt=%alt_name, error=%e, repair_error=%rerr, "Consensus alt repair attempt failed");
                                break;
                            }
                        }
                    }
                }
            }
        } else {
            let report_path = format!(
                "{artifacts_dir}/consensus_incident_{}.json",
                chrono::Utc::now().format("%Y%m%dT%H%M%S")
            );
            let report = serde_json::json!({"type":"consensus_schema_failure","directive":directive,"primary_function":primary,"attempts":schema_attempts,"response_keys":resp.as_object().map(|o|o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default()});
            if let Err(e) = std::fs::write(
                &report_path,
                serde_json::to_vec_pretty(&report).unwrap_or_default(),
            ) {
                warn!(error=%e, "Failed to persist consensus incident report");
            } else {
                warn!(path=%report_path, "Consensus schema failure incident recorded");
            }
        }
    } else {
        warn!("Consensus variant LLM call failed");
    }
    Ok(())
}

async fn harness_invoke(engine: &mut UnifiedFlowEngine, func: &str, arg: f64) -> Result<f64> {
    use rand::{distributions::Alphanumeric, Rng};
    let rid: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(6)
        .map(char::from)
        .collect();
    let flow_id = format!("consensus_{rid}");
    let blocks = vec![
        BlockDefinition {
            id: "run".into(),
            block_type: BlockType::Compute,
            properties: {
                let mut m = HashMap::new();
                m.insert(
                    "expression".into(),
                    Value::String(format!("function:{func}")),
                );
                m.insert("args".into(), Value::Array(vec![Value::from(arg)]));
                m.insert("output_key".into(), Value::String("result".into()));
                m.insert("next_block".into(), Value::String("end".into()));
                m
            },
        },
        BlockDefinition {
            id: "end".into(),
            block_type: BlockType::Terminal,
            properties: HashMap::new(),
        },
    ];
    let flow = FlowDefinition {
        id: flow_id.clone(),
        name: "ConsensusHarness".into(),
        start_block_id: "run".into(),
        blocks,
    };
    engine.register_flow(flow.clone())?;
    let mut state = UnifiedState::new("cuser".into(), "cop".into(), "cchan".into());
    state.flow_id = Some(flow_id.clone());
    engine.process_flow(&flow_id, &mut state).await?;
    let val = state
        .data
        .get("result")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| anyhow::anyhow!("no numeric result"))?;
    Ok(val)
}
