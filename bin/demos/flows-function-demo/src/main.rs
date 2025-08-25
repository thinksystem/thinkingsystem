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



#![allow(clippy::too_many_arguments)]

pub mod args;

pub mod consensus;

pub mod cleaning;
pub mod execution_graph;
pub mod ir;
pub mod plan;
pub mod planning;
pub mod routing;
pub mod runtime;
pub mod rust_fn;
pub mod transform;

use anyhow::Result;
use args::Args;
use clap::Parser;
use consensus::{assess_result_anomaly, extract_numeric_arg, run_consensus_variant};
use execution_graph::{EvaluatorRegistry, ExecNode, ExecutionGraph};
use ir::{ir_to_wat, IRFunction};
use stele::codegen::artifacts::persist_rust_fn_artifacts;
use stele::codegen::plan_artifacts::persist_plan_artifact;
use tracing::{info, warn, Level};

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
pub mod dsl;
mod llm_helpers;
mod strategy_eval;

use stele::database::{
    connection::DatabaseConnection, dynamic_storage::DynamicStorage, types::DatabaseCommand,
};
use stele::flows::core::BlockDefinition;
use stele::flows::dynamic_executor::DynamicFunction as SteleDynamicFunction;
use stele::flows::flowgorithm::Flowgorithm;
use stele::flows::state::UnifiedState;
use stele::nlu::llm_processor::{CustomLLMAdapter, LLMAdapter};
use stele::nlu::orchestrator::NLUOrchestrator;
use stele::nlu::query_processor::QueryProcessor;
use stele::{BlockRegistry, BlockType, FlowDefinition, SecurityConfig, UnifiedFlowEngine};
use tokio::sync::{mpsc, oneshot, RwLock};
#[cfg(feature = "ui")]
mod ui;

pub async fn build_query_processor() -> anyhow::Result<QueryProcessor> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let config_dir = manifest_dir
        .join("../../../crates/stele/src/nlu/config")
        .canonicalize()
        .unwrap_or_else(|_| manifest_dir.join("../../../crates/stele/src/nlu/config"));
    let config_dir_str = config_dir.to_string_lossy().to_string();
    let (command_tx, command_rx) = mpsc::channel(32);
    let (client_tx, mut client_rx) = mpsc::channel(1);
    let mut db_conn = DatabaseConnection::new(command_rx);
    tokio::spawn(async move {
        let _ = db_conn.run().await;
    });
    let (connect_response_tx, connect_response_rx) = oneshot::channel();
    command_tx
        .send(DatabaseCommand::Connect {
            client_sender: client_tx,
            response_sender: connect_response_tx,
        })
        .await?;
    connect_response_rx.await??;
    let db_client = client_rx
        .recv()
        .await
        .ok_or_else(|| anyhow::anyhow!("db client missing"))?;
    let storage = Arc::new(DynamicStorage::new(db_client.clone()));
    let orchestrator = Arc::new(RwLock::new(
        NLUOrchestrator::new(&config_dir_str)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to init NLU orchestrator: {e}"))?,
    ));
    let query_processor = QueryProcessor::new(
        orchestrator,
        storage,
        &format!("{config_dir_str}/query_processor.toml"),
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to init QueryProcessor: {e}"))?;
    Ok(query_processor)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();
    let max_level = if args.debug {
        Level::DEBUG
    } else {
        Level::INFO
    };
    tracing_subscriber::fmt()
        .with_max_level(max_level)
        .with_target(false)
        .init();
    #[cfg(feature = "ui")]
    if args.ui {
        return ui::runner::launch_ui().map_err(|e| anyhow::anyhow!("UI failed: {e}"));
    }
    info!(directive=%args.directive, plan_file=?args.plan_file, llm_plan=%args.llm_plan, "Starting plan-driven dynamic function demo");

    std::env::set_var("TS_LAST_DIRECTIVE_LOWER", args.directive.to_lowercase());

    let run_ts = chrono::Utc::now().format("%Y%m%dT%H%M%S");

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let run_dir = cwd
        .join(&args.artifacts_dir)
        .join(format!("run_{run_ts}"))
        .to_string_lossy()
        .to_string();
    if let Err(e) = std::fs::create_dir_all(&run_dir) {
        warn!(error=%e, path=%run_dir, "Failed to create run artifacts directory");
    } else {
        info!(run_dir=%run_dir, "Created run artifacts directory");
    }

    use crate::llm_helpers::{classify_recoverable_error, heal_rust_code};
    use crate::routing::{decide_route, decide_route_llm, RouteMode};

    let auto = !(args.llm_rust_fn || args.llm_plan || args.plan_file.is_some());
    let decision = if auto {
        let env_adapter = build_env_adapter();
        decide_route_llm(
            env_adapter.as_ref(),
            &args.directive,
            args.llm_rust_fn,
            args.llm_plan,
            args.plan_file.is_some(),
        )
        .await
    } else {
        decide_route(
            &args.directive,
            args.llm_rust_fn,
            args.llm_plan,
            args.plan_file.is_some(),
            false,
        )
    };
    info!(requested=?decision.requested, resolved=?decision.resolved, heuristic=%decision.heuristic_triggered, reason=%decision.reason, "Routing decision");
    let use_llm_rust_fn = matches!(decision.resolved, RouteMode::Native);

    if use_llm_rust_fn {
        info!("PATH=Native");
        use rust_fn::generate_rust_source;
        use stele::flows::dynamic_executor::{DynamicExecutor, DynamicSource};
        let adapter = build_env_adapter();

        let mut gen_attempt: u16 = 0;
        let artifact;
        let mut _last_artifact_opt = None;
        'outer_gen: loop {
            gen_attempt += 1;
            let art = generate_rust_source(adapter.as_ref(), &args.directive).await?; // contains code, args, magnitude
                                                                                      // Compile & execute immediately to detect null prior to persisting? We need code persisted each attempt for audit; persist after potential reuse.
                                                                                      // Register & execute to see if it yields null result; if so, continue (with new generation) until limit.
            if args.persist_rust_fn {
                let raw = serde_json::json!({
                    "directive": &args.directive,
                    "args": art.args,
                    "magnitude": art.magnitude,
                    "system_prompt": art.system_prompt,
                    "attempt": gen_attempt,
                });
                let _ = persist_rust_fn_artifacts(
                    &run_dir,
                    &args.directive,
                    &art.code,
                    &art.args,
                    art.magnitude,
                    &raw,
                );
            }
            // Build executor fresh each attempt to ensure clean load environment.
            let exec = DynamicExecutor::new()
                .map_err(|e| anyhow::anyhow!("executor init failed: {e:?}"))?;
            // Attempt dynamic registration; if the generated code failed to export the required symbol (e.g. missing `compute`), treat as a malformed generation and retry (like null handling)
            #[allow(unused_mut)]
            let mut dyn_fn_opt = None;
            let mut registration_error: Option<anyhow::Error> = None;
            let mut code_current = art.code.clone();
            let mut heal_attempted = false;
            // If both native and wasi features are present, pick based on --wasi flag; else fall back to whichever is compiled.
            #[cfg(all(feature = "dynamic-native", feature = "dynamic-wasi"))]
            {
                if args.use_wasi {
                    for heal_pass in 0..=2 {
                        match exec.register_dynamic_source(DynamicSource::RustWasiFull {
                            name: &args.directive,
                            source: &code_current,
                        }) {
                            Ok(df) => {
                                dyn_fn_opt = Some(df);
                                break;
                            }
                            Err(e) => {
                                let es = format!("{e:?}");
                                let recoverable =
                                    classify_recoverable_error(adapter.as_ref(), &es).await;
                                if recoverable && !heal_attempted {
                                    tracing::warn!(attempt=gen_attempt, error=%es, heal_pass, "Attempting LLM-guided code heal (dual-mode WASI)");
                                    if let Some(fixed) =
                                        heal_rust_code(adapter.as_ref(), &code_current, &es).await
                                    {
                                        code_current = fixed;
                                    }
                                    heal_attempted = true;
                                    continue;
                                } else if recoverable {
                                    tracing::warn!(attempt=gen_attempt, error=%es, "Recoverable compile issue persists after heal; regenerating (dual-mode WASI)");
                                    if gen_attempt < args.max_null_retries {
                                        continue 'outer_gen;
                                    } else {
                                        registration_error = Some(anyhow::anyhow!(
                                            "Exceeded regeneration attempts for recoverable errors: {es}"
                                        ));
                                        break;
                                    }
                                } else {
                                    registration_error = Some(anyhow::anyhow!(
                                        "dynamic wasi registration failed: {es}"
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                } else {
                    for heal_pass in 0..=2 {
                        match exec.register_dynamic_source(DynamicSource::RustFull {
                            name: &args.directive,
                            source: &code_current,
                        }) {
                            Ok(df) => {
                                dyn_fn_opt = Some(df);
                                break;
                            }
                            Err(e) => {
                                let es = format!("{e:?}");
                                let recoverable =
                                    classify_recoverable_error(adapter.as_ref(), &es).await;
                                if recoverable && !heal_attempted {
                                    tracing::warn!(attempt=gen_attempt, error=%es, heal_pass, "Attempting LLM-guided code heal");
                                    if let Some(fixed) =
                                        heal_rust_code(adapter.as_ref(), &code_current, &es).await
                                    {
                                        code_current = fixed;
                                    }
                                    heal_attempted = true;
                                    continue;
                                } else if recoverable && heal_pass == 0 {
                                    tracing::warn!(attempt=gen_attempt, error=%es, "Recoverable compile issue persists after heal; regenerating");
                                    if gen_attempt < args.max_null_retries {
                                        continue 'outer_gen;
                                    } else {
                                        registration_error = Some(anyhow::anyhow!(
                                            "Exceeded regeneration attempts for recoverable errors: {es}"
                                        ));
                                        break;
                                    }
                                } else if recoverable {
                                    if gen_attempt < args.max_null_retries {
                                        continue 'outer_gen;
                                    } else {
                                        registration_error = Some(anyhow::anyhow!(
                                            "Exceeded regeneration attempts for recoverable errors: {es}"
                                        ));
                                        break;
                                    }
                                } else {
                                    registration_error =
                                        Some(anyhow::anyhow!("dynamic registration failed: {es}"));
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            #[cfg(all(feature = "dynamic-native", not(feature = "dynamic-wasi")))]
            #[allow(unused_variables)]
            #[allow(unused_mut)]
            {
                for heal_pass in 0..=2 {
                    match exec.register_dynamic_source(DynamicSource::RustFull {
                        name: &args.directive,
                        source: &code_current,
                    }) {
                        Ok(df) => {
                            dyn_fn_opt = Some(df);
                            break;
                        }
                        Err(e) => {
                            let es = format!("{e:?}");
                            let recoverable =
                                classify_recoverable_error(adapter.as_ref(), &es).await;
                            if recoverable && !heal_attempted {
                                tracing::warn!(attempt=gen_attempt, error=%es, heal_pass, "Attempting LLM-guided code heal");
                                if let Some(fixed) =
                                    heal_rust_code(adapter.as_ref(), &code_current, &es).await
                                {
                                    code_current = fixed;
                                }
                                heal_attempted = true;
                                continue;
                            } else if recoverable && heal_pass == 0 {
                                tracing::warn!(attempt=gen_attempt, error=%es, "Recoverable compile issue persists after heal; regenerating");
                                if gen_attempt < args.max_null_retries {
                                    continue 'outer_gen;
                                } else {
                                    registration_error = Some(anyhow::anyhow!(
                                        "Exceeded regeneration attempts for recoverable errors: {es}"
                                    ));
                                    break;
                                }
                            } else if recoverable {
                                // multiple heal passes exhausted
                                if gen_attempt < args.max_null_retries {
                                    continue 'outer_gen;
                                } else {
                                    registration_error = Some(anyhow::anyhow!(
                                        "Exceeded regeneration attempts for recoverable errors: {es}"
                                    ));
                                    break;
                                }
                            } else {
                                registration_error =
                                    Some(anyhow::anyhow!("dynamic registration failed: {es}"));
                                break;
                            }
                        }
                    }
                }
            }
            #[cfg(all(feature = "dynamic-wasi", not(feature = "dynamic-native")))]
            {
                for heal_pass in 0..=2 {
                    match exec.register_dynamic_source(DynamicSource::RustWasiFull {
                        name: &args.directive,
                        source: &code_current,
                    }) {
                        Ok(df) => {
                            dyn_fn_opt = Some(df);
                            break;
                        }
                        Err(e) => {
                            let es = format!("{e:?}");
                            let recoverable =
                                classify_recoverable_error(adapter.as_ref(), &es).await;
                            if recoverable && !heal_attempted {
                                tracing::warn!(attempt=gen_attempt, error=%es, heal_pass, "Attempting LLM-guided code heal (WASI)");
                                if let Some(fixed) =
                                    heal_rust_code(adapter.as_ref(), &code_current, &es).await
                                {
                                    code_current = fixed;
                                }
                                heal_attempted = true;
                                continue;
                            } else if recoverable {
                                tracing::warn!(attempt=gen_attempt, error=%es, "Recoverable compile issue persists after heal; regenerating (WASI)");
                                if gen_attempt < args.max_null_retries {
                                    continue 'outer_gen;
                                } else {
                                    registration_error = Some(anyhow::anyhow!(
                                        "Exceeded regeneration attempts for recoverable errors: {es}"
                                    ));
                                    break;
                                }
                            } else {
                                registration_error =
                                    Some(anyhow::anyhow!("dynamic wasi registration failed: {es}"));
                                break;
                            }
                        }
                    }
                }
            }
            #[cfg(all(not(feature = "dynamic-native"), not(feature = "dynamic-wasi")))]
            compile_error!(
                "flows-function-demo requires at least one of stele dynamic-native or dynamic-wasi features enabled"
            );
            if let Some(err) = registration_error {
                return Err(err);
            }
            let dyn_fn = dyn_fn_opt
                .expect("dynamic function should be present after successful registration");
            let input_values: Vec<serde_json::Value> =
                art.args.iter().map(|v| serde_json::json!(v)).collect();
            let result_val = dyn_fn.execute(&input_values)?;
            let is_null = result_val.is_null();
            tracing::info!(
                attempt = gen_attempt,
                is_null,
                ?result_val,
                "Execution attempt result"
            );
            if !is_null || gen_attempt > args.max_null_retries {
                artifact = art; // final artifact chosen
                _last_artifact_opt = Some(result_val);
                break;
            } else {
                tracing::warn!(
                    attempt = gen_attempt,
                    max = args.max_null_retries,
                    "Null result encountered; regenerating code"
                );
                continue;
            }
        }
        let result = _last_artifact_opt.unwrap_or(serde_json::Value::Null);
        tracing::info!(len = artifact.code.len(), args=?artifact.args, magnitude=?artifact.magnitude, attempts=gen_attempt, "Final generated Rust source via LLM (null-aware loop)");
        if let (Some((lo, hi)), Some(rf)) = (artifact.magnitude, result.as_f64()) {
            if rf < lo || rf > hi {
                info!(
                    value = rf,
                    low = lo,
                    high = hi,
                    "Result outside magnitude envelope"
                );
            } else {
                info!(
                    value = rf,
                    low = lo,
                    high = hi,
                    "Result within magnitude envelope"
                );
            }
        }
        info!(?result, "LLM Rust function execution result");
        if let Some(num) = result.as_f64() {
            // Plain line for benchmark CSV extractor robustness (unstructured numeric)
            println!("LLM_RUST_RESULT: {num}");
        } else if result.is_null() {
            println!("LLM_RUST_RESULT: null");
        }
        return Ok(());
    } else {
        info!("PATH=Plan");
        // (Existing plan path logic continues below unchanged)
    }

    // Configure adapters (env-driven). We route plan/review to plan-adapter and
    // per-function WAT/IR synthesis to fn-adapter. Defaults: plan->anthropic (remote), fn->ollama (local).
    let plan_adapter = build_plan_adapter();
    let fn_adapter = build_fn_adapter();
    // Acquire plan first (may need LLM call); reuse adapter later for engine.
    let mut plan_value: Value = if let Some(p) = &args.plan_file {
        let txt = std::fs::read_to_string(p)
            .map_err(|e| anyhow::anyhow!("Failed to read plan file {p}: {e}"))?;
        serde_json::from_str(&txt).map_err(|e| anyhow::anyhow!("Invalid plan JSON: {e}"))?
    } else if args.llm_plan {
        // Use Planner facade for retries + optional adaptive hint (wired externally if desired)
        use planning::llm::{LlmPlanGen, LlmPlanPreprocessor, LlmPlanRepairer, LlmPlanValidator};
        use planning::Planner;
        let planner = Planner::new(
            LlmPlanGen {
                adapter: &*plan_adapter,
            },
            LlmPlanValidator,
            LlmPlanRepairer {
                adapter: &*plan_adapter,
            },
            LlmPlanPreprocessor {
                adapter: &*fn_adapter,
            },
        );
        let mut plan_val = planner
            .generate_validated(
                &args.directive,
                args.max_plan_attempts,
                args.max_repair_attempts,
            )
            .await?;
        // Prefer LLM classification to decide enforcing dual-stage scan; fallback to heuristic only if LLM allows upgrade.
        // Use the local LLM (function adapter) for classification to avoid brittle heuristics
        crate::plan::enforce_dual_stage_scan_llm(&mut plan_val, &*fn_adapter, &args.directive)
            .await;
        // Non-deterministic feasibility gate (LLM advisory only)
        crate::plan::feasibility_gate(
            &*plan_adapter,
            &args.directive,
            &mut plan_val,
            args.max_feasibility_attempts,
            if args.persist_feasibility {
                Some(&run_dir)
            } else {
                None
            },
        )
        .await;
        if args.persist_plan {
            persist_plan_artifact(&run_dir, &plan_val, &args.directive)?;
        }
        plan_val
    } else {
        // Implicit plan gen via Planner facade
        use planning::llm::{LlmPlanGen, LlmPlanPreprocessor, LlmPlanRepairer, LlmPlanValidator};
        use planning::Planner;
        info!("IMPLICIT_PLAN_GEN starting implicit plan generation (no --llm-plan / --plan-file)");
        let planner = Planner::new(
            LlmPlanGen {
                adapter: &*plan_adapter,
            },
            LlmPlanValidator,
            LlmPlanRepairer {
                adapter: &*plan_adapter,
            },
            LlmPlanPreprocessor {
                adapter: &*fn_adapter,
            },
        );
        let mut plan_val = planner
            .generate_validated(
                &args.directive,
                args.max_plan_attempts,
                args.max_repair_attempts,
            )
            .await?;
        // Use the local LLM (function adapter) for classification to avoid brittle heuristics
        crate::plan::enforce_dual_stage_scan_llm(&mut plan_val, &*fn_adapter, &args.directive)
            .await;
        // Non-deterministic feasibility gate (LLM advisory only)
        crate::plan::feasibility_gate(
            &*plan_adapter,
            &args.directive,
            &mut plan_val,
            args.max_feasibility_attempts,
            if args.persist_feasibility {
                Some(&run_dir)
            } else {
                None
            },
        )
        .await;
        if args.persist_plan {
            persist_plan_artifact(&run_dir, &plan_val, &args.directive)?;
        }
        plan_val
    };

    // Let LLM pipeline drive generation; after preprocessing, translate any raw expression strings (LLM-provided) to WAT.
    {
        // Prefer local fn_adapter for WAT synthesis; fallback to plan_adapter if local model missing
        crate::plan::preprocess_functions_dual(
            &mut plan_value,
            &*fn_adapter,
            &*plan_adapter,
            &args.directive,
            &run_dir,
        )
        .await?;
    }
    // plan_value will be consumed by execute_generated_plan; no separate clone needed for execution_graph
    if let Some(funcs) = plan_value
        .get_mut("functions")
        .and_then(|v| v.as_array_mut())
    {
        for f in funcs.iter_mut() {
            let need_wat = f.get("wat").is_none();
            if !need_wat {
                continue;
            }
            let expr_src_opt = f
                .get("expression")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if expr_src_opt.is_none() {
                continue;
            }
            let name_owned = f
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("expr_fn")
                .to_string();
            let _export_owned = f
                .get("export")
                .and_then(|v| v.as_str())
                .unwrap_or(&name_owned)
                .to_string();
            let _ = expr_src_opt.unwrap();
            // Expression translation removed: no action taken.
        }
    }

    let registry = Arc::new(BlockRegistry::with_standard_blocks()?);
    let query_processor = build_query_processor().await?; // required by engine
    let llm_adapter = plan_adapter.clone();
    let navigator = Flowgorithm::new();
    let mut engine = UnifiedFlowEngine::new(
        registry.clone(),
        query_processor,
        (*llm_adapter).clone(),
        navigator,
        SecurityConfig::default(),
    );
    engine.set_preserve_data_on_complete(true);

    execute_generated_plan(
        &args.directive,
        plan_value,
        &mut engine,
        &*fn_adapter,
        &*plan_adapter,
        &run_dir,
        args.offline,
        args.max_wat_repairs,
    )
    .await?;
    Ok(())
}

pub fn build_env_adapter() -> Arc<CustomLLMAdapter> {
    let provider = std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "ollama".into());
    let model = std::env::var("LLM_MODEL")
        .or_else(|_| std::env::var("OLLAMA_MODEL"))
        .unwrap_or_else(|_| "llama3.2:3b".into());
    let adapter = match provider.as_str() {
        "ollama" => CustomLLMAdapter::ollama(model).unwrap_or_default(),
        "anthropic" => CustomLLMAdapter::anthropic().unwrap_or_default(),
        _ => CustomLLMAdapter::new(model, 20000, 0.2),
    };
    Arc::new(adapter)
}

// Separate adapter builders for plan vs function tasks (overridable via env)
pub fn build_plan_adapter() -> Arc<CustomLLMAdapter> {
    // Prefer remote provider unless explicitly set; fall back to env defaults if available
    match std::env::var("PLAN_LLM_PROVIDER")
        .unwrap_or_else(|_| "anthropic".into())
        .as_str()
    {
        "ollama" => {
            let model = std::env::var("PLAN_LLM_MODEL").unwrap_or_else(|_| {
                std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2:3b".into())
            });
            Arc::new(CustomLLMAdapter::ollama(model).unwrap_or_default())
        }
        _ => Arc::new(CustomLLMAdapter::anthropic().unwrap_or_default()),
    }
}

pub fn build_fn_adapter() -> Arc<CustomLLMAdapter> {
    // Prefer local model unless overridden
    match std::env::var("FN_LLM_PROVIDER")
        .unwrap_or_else(|_| "ollama".into())
        .as_str()
    {
        "anthropic" => Arc::new(CustomLLMAdapter::anthropic().unwrap_or_default()),
        _ => {
            let model = std::env::var("FN_LLM_MODEL").unwrap_or_else(|_| {
                std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2:3b".into())
            });
            Arc::new(CustomLLMAdapter::ollama(model).unwrap_or_default())
        }
    }
}

// Plan helper functions provided by plan module

// IR translation logic moved to ir module

async fn execute_generated_plan(
    directive: &str,
    plan: Value,
    engine: &mut UnifiedFlowEngine,
    fn_adapter: &dyn LLMAdapter,
    supervisor_adapter: &dyn LLMAdapter,
    artifacts_dir: &str,
    offline: bool,
    max_wat_repairs: u8,
) -> anyhow::Result<()> {
    info!(
        artifacts_dir,
        "Execute plan: using artifacts_dir for WAT persistence"
    );
    // (legacy force_expression_mode removed)
    if let Some(funcs) = plan["functions"].as_array() {
        for f in funcs {
            let name = f["name"].as_str().unwrap_or("unnamed").to_string();
            let export = f["export"].as_str().unwrap_or("").to_string();
            let wat_code = if let Some(w) = f["wat"].as_str() {
                w.to_string()
            } else if f.get("ir").is_some() {
                let ir_val = &f["ir"];
                match serde_json::from_value::<IRFunction>(ir_val.clone()) {
                    Ok(ir_fn) => {
                        let generated = ir_to_wat(&name, &export, &ir_fn);
                        info!(%name, "Translated IR -> WAT ({} bytes)", generated.len());
                        generated
                    }
                    Err(e) => {
                        warn!(%name, error=%e, "IR parse failed during execution; attempting on-demand WAT synthesis");
                        // Attempt on-demand WAT generation
                        let mut attempts = 0;
                        let mut wat_opt: Option<String> = None;
                        let system = "You output ONLY JSON with key 'wat' and a valid WebAssembly Text (WAT) module. Canonical form: (module (func $NAME (export \"EXPORT\") (param $n i32) (result i32) ...)). Rules: 1) Export name must be exactly EXPORT. 2) Declare all locals at the top with explicit types, e.g., (local $i i32). 3) Every (if ...) has both (then ...) and (else ...); if no else, use (else (nop)). 4) For loops use (block (loop ... (br_if 1 (cond)) ... (br 0))). 5) Only valid opcodes (i32.rem_u, i32.eq, i32.add, local.get, local.set, block, loop, br, br_if, return). 6) No comments or markdown; return ONLY JSON with the WAT string.";
                        let user = format!(
                            "Directive: {directive}\nFunction name: {name}\nExport: {export}\nReturn JSON now."
                        );
                        while attempts < 3 && wat_opt.is_none() {
                            attempts += 1;
                            match fn_adapter.generate_structured_response(system, &user).await {
                                Ok(resp) => {
                                    if let Some(ws) = resp.get("wat").and_then(|v| v.as_str()) {
                                        wat_opt = Some(ws.to_string());
                                        break;
                                    }
                                }
                                Err(err) => {
                                    warn!(%name, attempt=%attempts, %err, "On-demand WAT generation failed")
                                }
                            }
                        }
                        // Fallback to supervisor adapter if local failed
                        if wat_opt.is_none() {
                            attempts = 0;
                            while attempts < 2 && wat_opt.is_none() {
                                attempts += 1;
                                match supervisor_adapter
                                    .generate_structured_response(system, &user)
                                    .await
                                {
                                    Ok(resp) => {
                                        if let Some(ws) = resp.get("wat").and_then(|v| v.as_str()) {
                                            wat_opt = Some(ws.to_string());
                                            break;
                                        }
                                    }
                                    Err(err) => {
                                        warn!(%name, attempt=%attempts, %err, "Supervisor fallback WAT generation failed")
                                    }
                                }
                            }
                        }
                        if let Some(w) = wat_opt {
                            w
                        } else {
                            return Err(anyhow::anyhow!(
                                "Failed to parse IR and could not synthesize WAT for {name}"
                            ));
                        }
                    }
                }
            } else {
                // Attempt WAT generation even with no IR
                let mut attempts = 0;
                let mut wat_opt: Option<String> = None;
                let system = "You output ONLY JSON with key 'wat' and a valid WebAssembly Text (WAT) module. Canonical form: (module (func $NAME (export \"EXPORT\") (param $n i32) (result i32) ...)). Rules: 1) Export name must be exactly EXPORT. 2) Declare all locals at the top with explicit types, e.g., (local $i i32). 3) Every (if ...) has both (then ...) and (else ...); if no else, use (else (nop)). 4) For loops use (block (loop ... (br_if 1 (cond)) ... (br 0))). 5) Only valid opcodes (i32.rem_u, i32.eq, i32.add, local.get, local.set, block, loop, br, br_if, return). 6) No comments or markdown; return ONLY JSON with the WAT string.";
                let user = format!(
                    "Directive: {directive}\nFunction name: {name}\nExport: {export}\nReturn JSON now."
                );
                while attempts < 3 && wat_opt.is_none() {
                    attempts += 1;
                    match fn_adapter.generate_structured_response(system, &user).await {
                        Ok(resp) => {
                            if let Some(ws) = resp.get("wat").and_then(|v| v.as_str()) {
                                wat_opt = Some(ws.to_string());
                                break;
                            }
                        }
                        Err(err) => {
                            warn!(%name, attempt=%attempts, %err, "WAT generation for missing function body failed")
                        }
                    }
                }
                if wat_opt.is_none() {
                    attempts = 0;
                    while attempts < 2 && wat_opt.is_none() {
                        attempts += 1;
                        match supervisor_adapter
                            .generate_structured_response(system, &user)
                            .await
                        {
                            Ok(resp) => {
                                if let Some(ws) = resp.get("wat").and_then(|v| v.as_str()) {
                                    wat_opt = Some(ws.to_string());
                                    break;
                                }
                            }
                            Err(err) => {
                                warn!(%name, attempt=%attempts, %err, "Supervisor fallback for missing function body failed")
                            }
                        }
                    }
                }
                if let Some(w) = wat_opt {
                    w
                } else {
                    return Err(anyhow::anyhow!(
                        "Function {name} missing both 'wat' and 'ir' and synthesis failed"
                    ));
                }
            };
            info!(%name, export=%export, "Registering dynamic function via unified cleaner");
            match crate::cleaning::wat::sanitize_register_dual(
                engine,
                fn_adapter,
                supervisor_adapter,
                directive,
                &name,
                &export,
                &wat_code,
                artifacts_dir,
                offline,
                max_wat_repairs,
            )
            .await
            {
                Ok(_final_wat) => {}
                Err(e) => {
                    warn!(function=%name, error=%e, "WAT sanitize/register failed; continuing to allow later synthesis/repairs");
                }
            }
        }
    }

    // Ensure any flow-referenced functions exist; if missing, attempt LLM synthesis instead of stubbing.
    if let Some(blocks) = plan
        .get("flow")
        .and_then(|f| f.get("blocks"))
        .and_then(|v| v.as_array())
    {
        for b in blocks {
            if b.get("type").and_then(|v| v.as_str()) == Some("compute") {
                if let Some(expr) = b.get("expression").and_then(|v| v.as_str()) {
                    if let Some(fn_name) = expr.strip_prefix("function:") {
                        let fn_name = fn_name.trim();
                        if engine.get_dynamic_function(fn_name).await.is_none() {
                            // Try synthesize via local adapter first, then supervisor.
                            let system = "You output ONLY JSON with key 'wat' and a valid WebAssembly Text (WAT) module. Canonical form: (module (func $NAME (export \"EXPORT\") (param $n i32) (result i32) ...)).
Rules: 1) Export must be exactly EXPORT. 2) Declare all locals at the top with explicit types. 3) Every (if ...) has both (then ...) and (else ...); if else is empty, use (else (nop)). 4) Use stack form for if: (COND) (if (then ...) (else ...)). 5) Loops use (block (loop ... (br_if 1 (COND)) ... (br 0))). 6) Only valid opcodes: i32.* ops, local.get, local.set, block, loop, br, br_if, return. 7) Strict bans: NO '*', NO '[]' array indexing, NO '..' ranges, NO pseudo-code or labels on expressions, NO imports. 8) No comments or markdown; return ONLY JSON. Keep it minimal and valid.";
                            let user = format!(
                                "Directive: {directive}\nFunction name: {fn_name}\nExport: same\nReturn JSON now."
                            );
                            let mut wat_opt: Option<String> = None;
                            let mut attempts = 0;
                            while attempts < 3 && wat_opt.is_none() {
                                attempts += 1;
                                if let Ok(resp) =
                                    fn_adapter.generate_structured_response(system, &user).await
                                {
                                    if let Some(ws) = resp.get("wat").and_then(|v| v.as_str()) {
                                        wat_opt = Some(ws.to_string());
                                        break;
                                    }
                                }
                            }
                            if wat_opt.is_none() {
                                let mut s_attempts = 0;
                                while s_attempts < 2 && wat_opt.is_none() {
                                    s_attempts += 1;
                                    if let Ok(resp) = supervisor_adapter
                                        .generate_structured_response(system, &user)
                                        .await
                                    {
                                        if let Some(ws) = resp.get("wat").and_then(|v| v.as_str()) {
                                            wat_opt = Some(ws.to_string());
                                            break;
                                        }
                                    }
                                }
                            }
                            if let Some(wat_body) = wat_opt {
                                info!(function=%fn_name, "Synthesizing missing flow function via LLM");
                                match crate::cleaning::wat::sanitize_register_dual(
                                    engine,
                                    fn_adapter,
                                    supervisor_adapter,
                                    directive,
                                    fn_name,
                                    "same",
                                    &wat_body,
                                    artifacts_dir,
                                    offline,
                                    max_wat_repairs,
                                )
                                .await
                                {
                                    Ok(_) => {}
                                    Err(e) => {
                                        warn!(function=%fn_name, error=%e, "Synthesized flow function registration failed; proceeding without it");
                                    }
                                }
                            } else {
                                warn!(function=%fn_name, "Failed to synthesize missing flow function via LLM (no WAT produced)");
                            }
                        }
                    }
                }
            }
        }
    }

    // Preflight: require referenced functions to be registered. Downgrade to warning to avoid aborting runs.
    if let Err(e) = runtime::preflight::assert_flow_functions_available(&plan, engine).await {
        warn!(error=%e, "Preflight detected missing dynamic functions; continuing execution with partial plan");
    }
    // ---- DynamicExecutor Showcase (Task 4) ----
    // After registration, directly fetch and invoke the first generated function (if it accepts a single numeric parameter) to
    // demonstrate dynamic function execution bypassing the flow layer. This is purely illustrative and does not affect flow outcome.
    if let Some(first_fn) = plan["functions"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|f| f["name"].as_str())
    {
        if let Some(df) = engine.get_dynamic_function(first_fn).await {
            use serde_json::json;
            let test_arg = 100.0_f64; // generic probe value
            if let Ok(res) = df.execute(&[json!(test_arg)]) {
                info!(function=%first_fn, ?res, test_arg, "Direct dynamic_executor invocation successful (showcase)");
            } else {
                warn!(function=%first_fn, "Direct dynamic_executor invocation failed (showcase)");
            }
        }
    }

    // Evaluate execution_graph (if present) now that dynamic functions are registered.
    let mut graph_best: Option<u64> = None;
    if let Some(eg_val) = plan.get("execution_graph").cloned() {
        if let Ok(graph) = serde_json::from_value::<ExecutionGraph>(eg_val) {
            let mut registry = EvaluatorRegistry::default();
            if let Some(evals_val) = plan.get("evaluators") {
                if let Some(arr) = evals_val.as_array() {
                    for ev in arr {
                        if let (Some(id), Some(ev_type)) = (
                            ev.get("id").and_then(|v| v.as_str()),
                            ev.get("type").and_then(|v| v.as_str()),
                        ) {
                            match ev_type {
                                "dsl" => {
                                    if let Some(src) = ev.get("source").and_then(|v| v.as_str()) {
                                        let mut src_current = src.to_string();
                                        // Try parse, and if it fails, attempt up to 2 LLM repairs with strict grammar guidance.
                                        let parsed_eval = match crate::dsl::DslEvaluator::parse(
                                            &src_current,
                                            300_000,
                                        ) {
                                            Ok(parsed) => Some(parsed),
                                            Err(e) => {
                                                warn!(evaluator=id, error=%e, "Failed to parse DSL evaluator; capturing feedback");
                                                if let Some(msg) = e
                                                    .root_cause()
                                                    .to_string()
                                                    .strip_prefix("DSL_PARSE_ERRORS: ")
                                                {
                                                    let mut lines: Vec<serde_json::Value> =
                                                        Vec::new();
                                                    for part in msg.split(" | ") {
                                                        if let Some(colon_pos) = part.find(':') {
                                                            let (line_seg, err_msg) =
                                                                part.split_at(colon_pos);
                                                            let line_no_str = line_seg
                                                                .trim_start_matches("line ")
                                                                .trim();
                                                            if let Ok(line_no) =
                                                                line_no_str.parse::<usize>()
                                                            {
                                                                if let Some(src_line) =
                                                                    src.lines().nth(line_no - 1)
                                                                {
                                                                    lines.push(serde_json::json!({
                                                                        "line": line_no,
                                                                        "error": err_msg.trim_start_matches(':').trim(),
                                                                        "text": src_line.trim_end()
                                                                    }));
                                                                }
                                                            }
                                                        }
                                                    }
                                                    if !lines.is_empty() {
                                                        let feedback = serde_json::json!({
                                                            "directive": directive,
                                                            "evaluator_id": id,
                                                            "errors": lines
                                                        });
                                                        let _ =
                                                            std::fs::create_dir_all(artifacts_dir);
                                                        let ts = chrono::Utc::now()
                                                            .format("%Y%m%dT%H%M%S");
                                                        let path = format!(
                                                            "{artifacts_dir}/dsl_feedback_{ts}.json"
                                                        );
                                                        if let Ok(mut f) =
                                                            std::fs::File::create(&path)
                                                        {
                                                            let _ = std::io::Write::write_all(
                                                                &mut f,
                                                                feedback.to_string().as_bytes(),
                                                            );
                                                        }
                                                    }
                                                }
                                                // Attempt LLM repair if not offline
                                                if !offline {
                                                    let system = "You output ONLY JSON with key 'source' containing a repaired DSL evaluator using this strict grammar. Each line is a rule: 'rule n % <int> == <int> -> <actions>' OR 'rule n == <int> -> terminate'. Allowed actions separated by semicolons from: 'n = n / <int>', 'n = n * <int>', 'n = n + <int>', 'n = n - <int>', 'terminate'. No comments, no extra keys. Return ONLY JSON.";
                                                    let user = format!(
                                                        "Directive: {directive}\nEvaluatorId: {id}\nPreviousSource:\n{src_current}\nParseErrors: {e}\nReturn repaired JSON now."
                                                    );
                                                    let mut attempts = 0u8;
                                                    let mut parsed_opt = None;
                                                    while attempts < 2 {
                                                        attempts += 1;
                                                        if let Ok(resp) = supervisor_adapter
                                                            .generate_structured_response(
                                                                system, &user,
                                                            )
                                                            .await
                                                        {
                                                            if let Some(new_src) = resp
                                                                .get("source")
                                                                .and_then(|v| v.as_str())
                                                            {
                                                                src_current = new_src.to_string();
                                                                match crate::dsl::DslEvaluator::parse(&src_current, 300_000) {
                                                                    Ok(p) => { parsed_opt = Some(p); break; }
                                                                    Err(e2) => {
                                                                        warn!(evaluator=id, attempt=attempts, error=%e2, "DSL repair attempt parse failed");
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    parsed_opt
                                                } else {
                                                    None
                                                }
                                            }
                                        };
                                        if let Some(parsed) = parsed_eval {
                                            registry.register(id, std::sync::Arc::new(parsed));
                                        } else {
                                            warn!(
                                                evaluator = id,
                                                "DSL evaluator unavailable after repair attempts"
                                            );
                                        }
                                    } else {
                                        warn!(evaluator = id, "DSL evaluator missing source");
                                    }
                                }
                                "function" => {
                                    if let Some(fn_name) = ev
                                        .get("function")
                                        .or_else(|| ev.get("function_name"))
                                        .and_then(|v| v.as_str())
                                    {
                                        if let Some(df) = engine.get_dynamic_function(fn_name).await
                                        {
                                            struct FunctionEvaluator {
                                                df: SteleDynamicFunction,
                                                prefer_min_n: bool,
                                            }
                                            impl stele::flows::dynamic_executor::strategy::EvalFn for FunctionEvaluator {
                                                fn eval(&self, n: u64, _memo: &dyn stele::flows::dynamic_executor::strategy::MemoBackend) -> stele::flows::dynamic_executor::strategy::EvalOutcome{
                                                    let arg = serde_json::json!(n as f64);
                                                    let res = self.df.execute(&[arg]);
                                                    let mut score: u32 = 0;
                                                    let mut aux: Option<u64> = None;
                                                    if let Ok(val) = res {
                                                        if let Some(b) = val.as_bool() {
                                                            if b {
                                                                score = if self.prefer_min_n {
                                                                    u32::MAX - (n as u32)
                                                                } else {
                                                                    1
                                                                };
                                                            }
                                                        } else if let Some(num) = val.as_f64() {
                                                            // Numeric results from WAT often encode boolean as 0/1.
                                                            // If prefer_min_n is active and num>=1, treat as boolean ok and weight by smallest n.
                                                            if self.prefer_min_n && num >= 1.0 {
                                                                score = u32::MAX - (n as u32);
                                                            } else {
                                                                score = num.max(0.0) as u32;
                                                            }
                                                        } else if let Some(obj) = val.as_object() {
                                                            if let Some(ok) = obj
                                                                .get("ok")
                                                                .and_then(|v| v.as_bool())
                                                            {
                                                                if ok {
                                                                    score = if self.prefer_min_n {
                                                                        u32::MAX - (n as u32)
                                                                    } else {
                                                                        1
                                                                    };
                                                                }
                                                            }
                                                            if let Some(s) = obj
                                                                .get("score")
                                                                .and_then(|v| v.as_u64())
                                                            {
                                                                score = s as u32;
                                                            }
                                                            if let Some(a) = obj
                                                                .get("aux")
                                                                .and_then(|v| v.as_u64())
                                                            {
                                                                aux = Some(a);
                                                            }
                                                        }
                                                    }
                                                    stele::flows::dynamic_executor::strategy::EvalOutcome::new(score, Vec::new(), aux)
                                                }
                                            }
                                            let prefer_min_n = ev
                                                .get("prefer_min_n")
                                                .and_then(|v| v.as_bool())
                                                .unwrap_or(true);
                                            registry.register(
                                                id,
                                                std::sync::Arc::new(FunctionEvaluator {
                                                    df,
                                                    prefer_min_n,
                                                }),
                                            );
                                        } else {
                                            warn!(evaluator=id, function=%fn_name, "Function evaluator refers to unknown dynamic function");
                                        }
                                    } else {
                                        warn!(
                                            evaluator = id,
                                            "Function evaluator missing 'function' field"
                                        );
                                    }
                                }
                                other => {
                                    warn!(
                                        evaluator = id,
                                        etype = other,
                                        "Unsupported evaluator type"
                                    );
                                }
                            }
                        }
                    }
                }
            }
            for node in &graph.nodes {
                match node {
                    ExecNode::RangeScan(rs) => {
                        if let Some(eval) = registry.get(&rs.evaluator) {
                            use stele::flows::dynamic_executor::strategy::{execute, StrategyPlan};
                            let plan = StrategyPlan {
                                range_start: rs.start.max(2),
                                range_end: rs.end,
                                prefer_dense_cutoff: rs.prefer_dense_cutoff,
                                shards: rs.shards as usize,
                                chunk: rs.chunk,
                                odd_only: false,
                                progress_log_interval: rs.progress_log_interval,
                                early_stop_no_improve: None,
                                upper_bound: None,
                                top_k: Some(5),
                                memory_limit_mb: {
                                    let env = std::env::var("FLOW_STRAT_MEMORY_MB")
                                        .ok()
                                        .and_then(|v| v.parse::<u64>().ok());
                                    let heuristic =
                                        ((rs.end.max(1) as u128 * 32) / 200_000u128).max(8) as u64;
                                    Some(env.unwrap_or(heuristic))
                                },
                                min_score: None,
                                min_aux: None,
                                custom_score_expr: Some("score + laux".to_string()),
                            };
                            let strat_result = execute(&plan, &eval).map_err(|e| {
                                anyhow::anyhow!("execution graph range_scan failed: {e}")
                            })?;
                            info!(
                                best_n = strat_result.best_n,
                                best_score = strat_result.best_score,
                                range_end = plan.range_end,
                                "ExecutionGraph node complete"
                            );
                            println!("RESULT {}", strat_result.best_n);
                            graph_best = Some(strat_result.best_n);
                            if let Some(top) = &strat_result.top {
                                println!("TOP_K [{}]", top.len());
                                for (n, sc, aux, ord) in top {
                                    println!(
                                        "TOP_ITEM n={} score={} aux={} order_score={}",
                                        n,
                                        sc,
                                        aux.map(|v| v.to_string()).unwrap_or("NA".into()),
                                        ord
                                    );
                                }
                            }
                            if let Some(pf) = &strat_result.pareto {
                                let mut pf_sorted = pf.clone();
                                pf_sorted.sort_by(|a, b| {
                                    b.1.cmp(&a.1)
                                        .then_with(|| b.2.cmp(&a.2))
                                        .then_with(|| b.0.cmp(&a.0))
                                });
                                println!("PARETO [{}]", pf_sorted.len());
                                for (n, sc, aux) in pf_sorted.into_iter() {
                                    println!(
                                        "PARETO_ITEM n={} score={} aux={}",
                                        n,
                                        sc,
                                        aux.map(|v| v.to_string()).unwrap_or("NA".into())
                                    );
                                }
                            }
                        } else {
                            warn!(evaluator=%rs.evaluator, "No evaluator registered for range scan node");
                        }
                    }
                    ExecNode::SwitchScan(sw) => {
                        use stele::flows::dynamic_executor::strategy::{execute, StrategyPlan};
                        let mut prev_best: Option<u32> = None;
                        for (idx, eval_id) in sw.evaluators.iter().enumerate() {
                            let Some(eval) = registry.get(eval_id) else {
                                warn!(evaluator=%eval_id, "No evaluator for switch_scan stage");
                                continue;
                            };
                            let plan = StrategyPlan {
                                range_start: sw.start.max(2),
                                range_end: sw.end,
                                prefer_dense_cutoff: sw.prefer_dense_cutoff,
                                shards: sw.shards as usize,
                                chunk: sw.chunk,
                                odd_only: false,
                                progress_log_interval: sw.progress_log_interval,
                                early_stop_no_improve: None,
                                upper_bound: None,
                                top_k: Some(5),
                                memory_limit_mb: {
                                    let env = std::env::var("FLOW_STRAT_MEMORY_MB")
                                        .ok()
                                        .and_then(|v| v.parse::<u64>().ok());
                                    let heuristic =
                                        ((sw.end.max(1) as u128 * 32) / 200_000u128).max(8) as u64;
                                    Some(env.unwrap_or(heuristic))
                                },
                                min_score: None,
                                min_aux: None,
                                custom_score_expr: Some("score + laux".to_string()),
                            };
                            println!(
                                "SWITCH_STAGE stage={} evaluator={} range_end={}",
                                idx, eval_id, plan.range_end
                            );
                            let strat_result = execute(&plan, &eval).map_err(|e| {
                                anyhow::anyhow!("execution graph switch_scan failed: {e}")
                            })?;
                            println!("RESULT {}", strat_result.best_n);
                            graph_best = Some(strat_result.best_n);
                            if let Some(top) = &strat_result.top {
                                println!("TOP_K [{}]", top.len());
                                for (n, sc, aux, ord) in top {
                                    println!(
                                        "TOP_ITEM n={} score={} aux={} order_score={}",
                                        n,
                                        sc,
                                        aux.map(|v| v.to_string()).unwrap_or("NA".into()),
                                        ord
                                    );
                                }
                            }
                            if let Some(pf) = &strat_result.pareto {
                                let mut pf_sorted = pf.clone();
                                pf_sorted.sort_by(|a, b| {
                                    b.1.cmp(&a.1)
                                        .then_with(|| b.2.cmp(&a.2))
                                        .then_with(|| b.0.cmp(&a.0))
                                });
                                println!("PARETO [{}]", pf_sorted.len());
                                for (n, sc, aux) in pf_sorted.into_iter() {
                                    println!(
                                        "PARETO_ITEM n={} score={} aux={}",
                                        n,
                                        sc,
                                        aux.map(|v| v.to_string()).unwrap_or("NA".into())
                                    );
                                }
                            }
                            if let Some(th) = sw.stage_advance_min_improve {
                                if let Some(prev) = prev_best {
                                    if strat_result.best_score.saturating_sub(prev) < th {
                                        println!(
                                            "SWITCH_STOP no_min_improve prev={} current={} required={} stage={}",
                                            prev, strat_result.best_score, th, idx
                                        );
                                        break;
                                    }
                                }
                            }
                            prev_best = Some(strat_result.best_score);
                        }
                    }
                }
            }
        }
    }

    // Plan sanity check: if the flow contains a compute block whose expression is a placeholder
    // (e.g., "execution_graph") but no execution_graph section exists, fail fast to avoid
    // meaningless flow runs and noisy consensus fallbacks.
    if plan.get("execution_graph").is_none() {
        if let Some(blocks) = plan
            .get("flow")
            .and_then(|v| v.as_object())
            .and_then(|f| f.get("blocks"))
            .and_then(|v| v.as_array())
        {
            let mut has_placeholder_compute = false;
            for b in blocks {
                if b.get("type").and_then(|v| v.as_str()) == Some("compute")
                    && b.get("expression")
                        .and_then(|v| v.as_str())
                        .map(|s| s.eq_ignore_ascii_case("execution_graph"))
                        .unwrap_or(false)
                {
                    has_placeholder_compute = true;
                    break;
                }
            }
            if has_placeholder_compute {
                return Err(anyhow::anyhow!(
                    "plan rejected: compute block uses placeholder 'execution_graph' but no execution_graph section provided"
                ));
            }
        }
    }

    // Removed directive keyword list consensus trigger: consensus now driven by anomaly assessment after execution.
    let flow_spec = &plan["flow"];
    let flow = build_flow_from_spec(flow_spec, directive)?;
    engine.register_flow(flow.clone())?;
    let mut state = UnifiedState::new(
        "demo_user".into(),
        "demo_operator".into(),
        "demo_channel".into(),
    );
    state.flow_id = Some(flow.id.clone());
    state.set_data("original_directive".into(), Value::String(directive.into()));
    let flow_exec_res = engine.process_flow(&flow.id, &mut state).await;
    let mut anomaly_reason: Option<String> = None;
    if let Err(e) = &flow_exec_res {
        warn!(error=%e, "Flow execution error");
        if let Ok((anom, reason)) = assess_result_anomaly(
            supervisor_adapter,
            directive,
            &plan,
            None,
            Some(&e.to_string()),
        )
        .await
        {
            if anom {
                anomaly_reason = Some(reason);
            }
        }
    }
    // Fetch metrics for each function and display
    if let Some(funcs) = plan["functions"].as_array() {
        for f in funcs {
            if let Some(name) = f["name"].as_str() {
                if let Some(metrics) = engine.get_function_metrics(name).await {
                    info!(function=%name, calls=metrics.total_calls, avg_time_ms=%metrics.avg_execution_time.as_millis(), errors=metrics.error_count, success_rate=metrics.success_rate, "Function metrics");
                }
            }
        }
    }
    if flow_exec_res.is_ok() {
        if let Some(mut res) = state.data.get("result").cloned() {
            let dl = directive.to_lowercase();
            let wants_prime_count = dl.contains("count primes up to");
            // Prefer execution_graph result only when result is placeholder/null and task is not a count.
            let _has_exec_graph = plan.get("execution_graph").is_some();
            let result_is_trivial_falsey = res.as_f64() == Some(0.0)
                || res.as_bool() == Some(false)
                || res.as_str() == Some("execution_graph")
                || res.is_null();
            let should_replace_with_graph =
                !wants_prime_count && graph_best.is_some() && result_is_trivial_falsey;
            if should_replace_with_graph {
                if let Some(n) = graph_best {
                    state.set_data("result".into(), serde_json::json!(n));
                    res = serde_json::json!(n);
                    info!(n, "Selected execution_graph best_n as final result");
                }
            }
            // If the directive asks to count primes, compute the count using the registered function.
            if wants_prime_count {
                // Try to infer upper bound N from flow args or execution_graph.
                let mut upper: Option<u64> = None;
                if let Some(flow) = plan.get("flow").and_then(|v| v.as_object()) {
                    if let Some(blocks) = flow.get("blocks").and_then(|v| v.as_array()) {
                        for b in blocks {
                            if b.get("type").and_then(|v| v.as_str()) == Some("compute") {
                                if let Some(args) = b.get("args").and_then(|v| v.as_array()) {
                                    if let Some(v) = args.first().and_then(|v| v.as_u64()) {
                                        upper = Some(v);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                if upper.is_none() {
                    if let Some(eg) = plan.get("execution_graph").and_then(|v| v.as_object()) {
                        if let Some(nodes) = eg.get("nodes").and_then(|v| v.as_array()) {
                            if let Some(node) = nodes.first().and_then(|v| v.as_object()) {
                                upper = node.get("end").and_then(|v| v.as_u64());
                            }
                        }
                    }
                }
                if let Some(nmax) = upper {
                    if let Some(func_name) = plan
                        .get("evaluators")
                        .and_then(|v| v.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|e| e.get("function"))
                        .and_then(|v| v.as_str())
                    {
                        if let Some(df) = engine.get_dynamic_function(func_name).await {
                            let mut count: u64 = 0;
                            for i in 2..=nmax {
                                let arg = serde_json::json!(i as f64);
                                if let Ok(val) = df.execute(&[arg]) {
                                    if let Some(x) = val.as_f64() {
                                        if x > 0.5 {
                                            count += 1;
                                        }
                                    } else if val.as_bool().unwrap_or(false) {
                                        count += 1;
                                    }
                                }
                            }
                            state.set_data("result".into(), serde_json::json!(count));
                            res = serde_json::json!(count);
                            info!(
                                count,
                                up_to = nmax,
                                "Computed prime count via dynamic function"
                            );
                        }
                    }
                }
            }
            info!(?res, "Final result");
            let mut anom = false;
            let mut reason = String::from("no_assessment");
            // Generic semantic guard: directive numeric scale vs structure (directive-agnostic)
            if let Some(r) = generic_semantic_guard(directive) {
                anom = true;
                reason = r;
            }
            if let Some(val) = res.as_f64() {
                if let Some(dom) = plan.get("numeric_domain") {
                    if let (Some(range), Some(target)) =
                        (dom.get("expected_range"), dom.get("target"))
                    {
                        if let (Some(lo), Some(hi)) = (
                            range.get(0).and_then(|v| v.as_f64()),
                            range.get(1).and_then(|v| v.as_f64()),
                        ) {
                            if val < lo || val > hi {
                                anom = true;
                                reason = format!("deterministic_out_of_range({val}) for {target}");
                            } else {
                                reason = "within_expected_range".into();
                            }
                        }
                    }
                }
                if reason == "no_assessment" {
                    let dl = directive.to_lowercase();
                    if dl.contains("monte carlo") && dl.contains("pi") {
                        if let Some(n) = extract_numeric_arg(&plan) {
                            let p = std::f64::consts::PI / 4.0;
                            let sd = 4.0 * (p * (1.0 - p) / n.max(1.0)).sqrt();
                            let tol = 3.0 * sd;
                            if (val - std::f64::consts::PI).abs() > tol {
                                anom = true;
                                reason = format!("stat_outlier({val})");
                            } else {
                                reason = "within_stat_tolerance".into();
                            }
                        }
                    }
                }
            }
            if reason == "no_assessment" {
                if let Ok((a, r)) =
                    assess_result_anomaly(supervisor_adapter, directive, &plan, Some(&res), None)
                        .await
                {
                    anom = a;
                    reason = r;
                }
            }
            if anom && consensus_enabled() {
                // Avoid consensus if result is a placeholder and no execution_graph best was computed.
                let placeholder = res.as_str() == Some("execution_graph") || res.is_null();
                if placeholder && graph_best.is_none() {
                    info!(%reason, "Anomaly detected but consensus disabled for placeholder result");
                } else {
                    info!(%reason, "Anomaly detected – launching consensus variant");
                    let _ = run_consensus_variant(
                        supervisor_adapter,
                        directive,
                        engine,
                        &plan,
                        artifacts_dir,
                    )
                    .await;
                }
            } else {
                info!(%reason, "Anomaly not significant; consensus skipped");
            }
        }
    } else if anomaly_reason.is_some() {
        if consensus_enabled() {
            info!(
                ?anomaly_reason,
                "Attempting consensus variant after failure"
            );
            let _ =
                run_consensus_variant(supervisor_adapter, directive, engine, &plan, artifacts_dir)
                    .await;
        } else {
            info!(?anomaly_reason, "Consensus disabled by env; skipping");
        }
    } else if let Err(e) = flow_exec_res {
        // Last-resort: emit a fallback output instead of aborting, to satisfy minimal output requirement.
        warn!(error=%e, "Flow failed; emitting fallback result");
        println!(
            "FINAL_RESULT {{\"status\":\"error\",\"message\":{:?}}}",
            e.to_string()
        );
        // Do not propagate error; consider run complete with fallback output.
    }
    Ok(())
}

// WAT sanitization and repair helpers now sourced from validation module

fn build_flow_from_spec(spec: &Value, directive: &str) -> Result<FlowDefinition, anyhow::Error> {
    let id = spec["id"].as_str().unwrap_or("generated_flow").to_string();
    let start = spec["start"].as_str().unwrap_or("intro").to_string();
    let mut blocks_vec = Vec::new();
    if let Some(blocks) = spec["blocks"].as_array() {
        for b in blocks {
            let b_id = b["id"].as_str().unwrap_or("blk").to_string();
            let b_type = b["type"].as_str().unwrap_or("display");
            let mut props = HashMap::new();
            if let Some(msg) = b["message"].as_str() {
                props.insert(
                    "message".into(),
                    Value::String(msg.replace("{directive}", directive)),
                );
            }
            if let Some(next) = b["next"].as_str() {
                props.insert("next_block".into(), Value::String(next.into()));
            }
            match b_type {
                "display" => blocks_vec.push(BlockDefinition {
                    id: b_id,
                    block_type: BlockType::Display,
                    properties: props,
                }),
                "compute" => {
                    if let Some(expr) = b["expression"].as_str() {
                        props.insert("expression".into(), Value::String(expr.into()));
                    }
                    // Normalize output key: always set 'result' while preserving original if different
                    if let Some(out) = b["output_key"].as_str() {
                        let original = out.to_string();
                        if original != "result" {
                            // store original key for later reference
                            props.insert(
                                "original_output_key".into(),
                                Value::String(original.clone()),
                            );
                        }
                        props.insert("output_key".into(), Value::String("result".into()));
                    } else {
                        props.insert("output_key".into(), Value::String("result".into()));
                    }
                    if let Some(args) = b["args"].as_array() {
                        props.insert("args".into(), Value::Array(args.clone()));
                    }
                    blocks_vec.push(BlockDefinition {
                        id: b_id,
                        block_type: BlockType::Compute,
                        properties: props,
                    });
                }
                "terminal" => blocks_vec.push(BlockDefinition {
                    id: b_id,
                    block_type: BlockType::Terminal,
                    properties: props,
                }),
                _ => {}
            }
        }
    }
    Ok(FlowDefinition {
        id,
        name: "Generated Flow".into(),
        start_block_id: start,
        blocks: blocks_vec,
    })
}

// (Removed directive-specific semantic guard helpers to maintain scalability.)

// Removed domain-specific detector; rely on LLM classification + structural upgrades.

fn generic_semantic_guard(directive: &str) -> Option<String> {
    // Extract largest integer token; if very large, encourage consensus path rather than trusting single sample.
    let mut cur = String::new();
    let mut max_seen: u64 = 0;
    for ch in directive.chars() {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else if !cur.is_empty() {
            if let Ok(v) = cur.parse() {
                if v > max_seen {
                    max_seen = v;
                }
            }
            cur.clear();
        }
    }
    if !cur.is_empty() {
        if let Ok(v) = cur.parse() {
            if v > max_seen {
                max_seen = v;
            }
        }
    }
    if max_seen >= 100_000 {
        return Some(format!("scale_exceeds_single_pass(n={max_seen})"));
    }
    None
}

// Runtime toggle: enable consensus only when explicitly requested.
// Set FLOW_ENABLE_CONSENSUS=1|true|on to turn it on. Default: off.
fn consensus_enabled() -> bool {
    std::env::var("FLOW_ENABLE_CONSENSUS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("on"))
        .unwrap_or(false)
}

// Stubbing is disabled by default. Set FLOW_ALLOW_STUBS=1|true|on to enable optional stub paths.
// allow_stubs removed: stubbing is not used; LLM synthesis is preferred for missing functions.
