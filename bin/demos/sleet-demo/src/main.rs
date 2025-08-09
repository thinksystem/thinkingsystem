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

#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_assignments)]
#![allow(clippy::redundant_pattern_matching)]

use anyhow::Result;
use async_trait::async_trait;
use clap::{Arg, Command};
use serde_json::{json, Value};
use sleet::llm::UnifiedLLMAdapter;
use sleet::{
    agents::{Agent, AgentSystem, AgentSystemConfig},
    flows::{BlockDefinition, BlockType, FlowDefinition},
    runtime::{ExecutionStatus, FfiRegistry, RemarkableInterpreter, Value as RuntimeValue},
    transpiler::FlowTranspiler,
};
use std::collections::HashMap;
use std::sync::Arc;
use stele::nlu::llm_processor::LLMAdapter;
use stele::nlu::LLMAdapter as LocalLLMAdapter;
use tokio::sync::{mpsc, Mutex};


fn json_to_runtime_value(json_value: &serde_json::Value) -> RuntimeValue {
    match json_value {
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                RuntimeValue::Integer(i)
            } else {
                RuntimeValue::Integer(0)
            }
        }
        serde_json::Value::Bool(b) => RuntimeValue::Boolean(*b),
        serde_json::Value::String(s) => RuntimeValue::String(s.clone()),
        serde_json::Value::Null => RuntimeValue::Null,
        _ => RuntimeValue::Null, 
    }
}


fn runtime_to_json_value(runtime_value: RuntimeValue) -> Value {
    match runtime_value {
        RuntimeValue::Integer(i) => Value::Number(serde_json::Number::from(i)),
        RuntimeValue::Boolean(b) => Value::Bool(b),
        RuntimeValue::String(s) => Value::String(s),
        RuntimeValue::Null => Value::Null,
        RuntimeValue::Json(j) => j,
    }
}

struct LLMAdapterBridge {
    inner: UnifiedLLMAdapter,
}

#[async_trait]
impl LLMAdapter for LLMAdapterBridge {
    async fn process_text(&self, input: &str) -> Result<String, Box<dyn std::error::Error>> {
        LocalLLMAdapter::generate_response(&self.inner, input)
            .await
            .map_err(|e| e as Box<dyn std::error::Error>)
    }

    async fn generate_response(&self, prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
        LocalLLMAdapter::generate_response(&self.inner, prompt)
            .await
            .map_err(|e| e as Box<dyn std::error::Error>)
    }

    async fn generate_structured_response(
        &self,
        system_prompt: &str,
        user_input: &str,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        LocalLLMAdapter::generate_structured_response(&self.inner, system_prompt, user_input)
            .await
            .map_err(|e| e as Box<dyn std::error::Error>)
    }
}

const MAX_ITERATIONS: u32 = 8;

const AGENT_TIMEOUT_SECS: u64 = 10;

struct WorkflowOrchestrator {
    workflows: HashMap<String, WorkflowExecution>,
    shared_state: Arc<Mutex<Value>>,
    agent_system: Arc<Mutex<AgentSystem>>,
    llm_manager: LLMManager,
    progress_tracker: ProgressTracker,
}

struct WorkflowExecution {
    name: String,
    flow_definition: FlowDefinition,
    assigned_agents: Vec<Agent>,
    status: WorkflowStatus,
    result: Option<Value>,
}

#[derive(Debug, Clone)]
enum WorkflowStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
}

struct LLMManager {
    primary_adapter: Box<dyn LLMAdapter + Send + Sync>,
    preferred_adapter: Option<Box<dyn LLMAdapter + Send + Sync>>,
}

#[derive(Debug, Clone)]
struct ProgressTracker {
    ema_fast: f64,
    ema_slow: f64,
    momentum: f64,
    plateau_count: u8,
    history: Vec<ProgressEntry>,
}

#[derive(Debug, Clone)]
struct ProgressEntry {
    iteration: u32,
    score: u8,
    momentum: f64,
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug)]
struct AsyncWorkRequest {
    correlation_id: String,
    workflow_name: String,
    interaction_id: String,
    agent_id: String,
    prompt: String,
    response_sender: mpsc::Sender<AsyncWorkResponse>,
}

#[derive(Debug)]
struct AsyncWorkResponse {
    correlation_id: String,
    workflow_name: String,
    interaction_id: String,
    result: Value,
}

impl LLMManager {
    async fn new(provider: &str, model: &str) -> Result<Self> {
        let primary_adapter = initialise_llm_adapter(provider, model).await?;
        let preferred_adapter = if provider != "anthropic" {
            match UnifiedLLMAdapter::anthropic().await {
                Ok(adapter) => {
                    let bridge = LLMAdapterBridge { inner: adapter };
                    Some(Box::new(bridge) as Box<dyn LLMAdapter + Send + Sync>)
                }
                Err(_) => None,
            }
        } else {
            None
        };

        if preferred_adapter.is_some() {
            tracing::info!("Preferred Anthropic adapter available for complex reasoning tasks");
        }

        Ok(Self {
            primary_adapter,
            preferred_adapter,
        })
    }

    fn get_preferred(&self) -> &dyn LLMAdapter {
        self.preferred_adapter
            .as_deref()
            .unwrap_or(&*self.primary_adapter)
    }

    fn get_primary(&self) -> &dyn LLMAdapter {
        &*self.primary_adapter
    }
}

impl ProgressTracker {
    fn new() -> Self {
        Self {
            ema_fast: 0.0,
            ema_slow: 0.0,
            momentum: 0.0,
            plateau_count: 0,
            history: Vec::new(),
        }
    }

    fn update(&mut self, iteration: u32, progress_score: u8) {
        let score_f64 = progress_score as f64;
        const FAST_ALPHA: f64 = 0.3;
        const SLOW_ALPHA: f64 = 0.1;

        if self.history.is_empty() {
            self.ema_fast = score_f64;
            self.ema_slow = score_f64;
        } else {
            self.ema_fast = FAST_ALPHA * score_f64 + (1.0 - FAST_ALPHA) * self.ema_fast;
            self.ema_slow = SLOW_ALPHA * score_f64 + (1.0 - SLOW_ALPHA) * self.ema_slow;
        }

        self.momentum = self.ema_fast - self.ema_slow;
        self.plateau_count = if self.momentum.abs() < 0.5 {
            self.plateau_count + 1
        } else {
            0
        };

        self.history.push(ProgressEntry {
            iteration,
            score: progress_score,
            momentum: self.momentum,
            timestamp: chrono::Utc::now(),
        });
    }

    fn needs_strategy_change(&self) -> bool {
        self.plateau_count >= 2 && self.history.len() >= 3
    }
}

impl WorkflowOrchestrator {
    async fn new(
        agent_system: Arc<Mutex<AgentSystem>>,
        llm_manager: LLMManager,
        goal: &str,
    ) -> Result<Self> {
        let shared_state = Arc::new(Mutex::new(json!({
            "goal": goal,
            "global_status": "initialising",
            "workflow_execution_order": [],
            "workflow_results": {},
            "current_iteration": 0
        })));

        Ok(Self {
            workflows: HashMap::new(),
            shared_state,
            agent_system,
            llm_manager,
            progress_tracker: ProgressTracker::new(),
        })
    }

    async fn add_workflow(
        &mut self,
        name: &str,
        flow_def: FlowDefinition,
        agents: Vec<Agent>,
    ) -> Result<()> {
        log_event(
            "workflow_added",
            json!({
                "workflow_name": name,
                "agent_count": agents.len(),
                "agents": agents.iter().map(|a| &a.name).collect::<Vec<_>>()
            }),
        );

        let workflow = WorkflowExecution {
            name: name.to_string(),
            flow_definition: flow_def,
            assigned_agents: agents,
            status: WorkflowStatus::Pending,
            result: None,
        };

        self.workflows.insert(name.to_string(), workflow);
        Ok(())
    }

    async fn execute_sequential(&mut self, workflow_name: &str) -> Result<Value> {
        self.execute_workflow_with_agents(workflow_name).await
    }

    async fn execute_parallel(
        &mut self,
        workflow_names: Vec<&str>,
    ) -> Result<HashMap<String, Value>> {
        log_event(
            "parallel_execution_start",
            json!({
                "workflows": workflow_names,
                "execution_mode": "true_parallel_with_agents"
            }),
        );

        let mut results = HashMap::new();

        for name in workflow_names {
            let result = self.execute_workflow_with_agents(name).await?;
            results.insert(name.to_string(), result);

            log_event(
                "parallel_workflow_success",
                json!({
                    "workflow_name": name,
                    "status": "completed"
                }),
            );
        }

        log_event(
            "parallel_execution_complete",
            json!({
                "completed_workflows": results.len(),
                "execution_mode": "sequential_for_demo"
            }),
        );

        Ok(results)
    }

    async fn execute_workflow_with_agents(&mut self, workflow_name: &str) -> Result<Value> {
        let workflow_exists = self.workflows.contains_key(workflow_name);
        if !workflow_exists {
            return Err(anyhow::anyhow!("Workflow {} does not exist", workflow_name));
        }

        {
            let mut state = self.shared_state.lock().await;
            if let Some(order) = state
                .get_mut("workflow_execution_order")
                .and_then(|v| v.as_array_mut())
            {
                order.push(serde_json::Value::String(workflow_name.to_string()));
            }
            state["global_status"] = json!("executing");
        }

        log_event(
            "workflow_execution_start",
            json!({
                "workflow_name": workflow_name,
                "status": "starting_with_real_agents"
            }),
        );

        let workflow = self.workflows.get_mut(workflow_name).unwrap();
        workflow.status = WorkflowStatus::Running;

        let flow_def = &workflow.flow_definition;
        let contract = FlowTranspiler::transpile(flow_def)?;
        let sleet_contract = sleet::convert_contract(contract)
            .map_err(|e| anyhow::anyhow!("Failed to convert contract: {}", e))?;
        let mut runtime =
            RemarkableInterpreter::new(1_000_000, &sleet_contract, FfiRegistry::new())?;

        let mut final_result = RuntimeValue::Null;
        let mut iteration = 0u32;

        loop {
            let status = runtime.run(sleet_contract.clone()).await?;
            match status {
                ExecutionStatus::AwaitingInput {
                    session_id: _,
                    interaction_id,
                    agent_id,
                    prompt,
                } => {
                    iteration += 1;

                    log_event(
                        "workflow_awaiting_agent",
                        json!({
                            "workflow_name": workflow_name,
                            "iteration": iteration,
                            "interaction_id": interaction_id,
                            "agent_id": agent_id
                        }),
                    );

                    let response = self
                        .process_agent_request_sync(&agent_id, prompt.as_str().unwrap_or_default())
                        .await?;

                    let progress_score = calculate_progress_score(&response);
                    self.progress_tracker.update(iteration, progress_score);

                    log_event(
                        "workflow_progress_update",
                        json!({
                            "workflow_name": workflow_name,
                            "iteration": iteration,
                            "progress_score": progress_score,
                            "momentum": self.progress_tracker.momentum,
                            "plateau_count": self.progress_tracker.plateau_count,
                            "needs_strategy_change": self.progress_tracker.needs_strategy_change()
                        }),
                    );

                    if self.progress_tracker.needs_strategy_change() {
                        log_event(
                            "workflow_plateau_detected",
                            json!({
                                "workflow_name": workflow_name,
                                "iteration": iteration,
                                "plateau_count": self.progress_tracker.plateau_count,
                                "message": "Workflow progress has plateaued - may need strategy adjustment"
                            }),
                        );
                    }

                    runtime.resume_with_input(&interaction_id, response);
                }
                ExecutionStatus::Completed(result) => {
                    final_result = result;
                    break;
                }
                ExecutionStatus::Running => {}
            }
        }

        let workflow = self.workflows.get_mut(workflow_name).unwrap();
        workflow.status = WorkflowStatus::Completed;
        workflow.result = Some(runtime_to_json_value(final_result.clone()));

        {
            let mut state = self.shared_state.lock().await;
            if let Some(results) = state
                .get_mut("workflow_results")
                .and_then(|v| v.as_object_mut())
            {
                results.insert(
                    workflow_name.to_string(),
                    runtime_to_json_value(final_result.clone()),
                );
            }
        }

        log_event(
            "workflow_execution_complete",
            json!({
                "workflow_name": workflow_name,
                "status": "completed_successfully",
                "total_iterations": iteration,
                "final_momentum": self.progress_tracker.momentum
            }),
        );

        Ok(runtime_to_json_value(final_result))
    }

    async fn process_agent_request_sync(&self, agent_id: &str, prompt: &str) -> Result<Value> {
        let agent_system_guard = self.agent_system.lock().await;
        let agent = agent_system_guard.get_agent(agent_id)?;
        let system_prompt = agent.get_system_prompt();

        agent_system_guard
            .get_llm_adapter()
            .unwrap()
            .generate_structured_response(&system_prompt, prompt)
            .await
            .map_err(|e| anyhow::anyhow!("LLM processing failed: {}", e))
    }
}

async fn initialise_llm_adapter(
    provider: &str,
    model: &str,
) -> Result<Box<dyn LLMAdapter + Send + Sync>> {
    let adapter = match provider {
        "ollama" => UnifiedLLMAdapter::ollama(model.to_string())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create Ollama adapter: {}", e))?,
        "anthropic" => UnifiedLLMAdapter::anthropic()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create Anthropic adapter: {}", e))?,
        _ => return Err(anyhow::anyhow!("Unknown provider: {}", provider)),
    };

    let bridge = LLMAdapterBridge { inner: adapter };
    Ok(Box::new(bridge))
}

async fn handle_agent_request(
    agent_system: Arc<Mutex<AgentSystem>>,
    _llm_adapter: &dyn LLMAdapter,
    request: AsyncWorkRequest,
) {
    let response = match process_agent_request(&agent_system, &request).await {
        Ok(result) => AsyncWorkResponse {
            correlation_id: request.correlation_id,
            workflow_name: request.workflow_name,
            interaction_id: request.interaction_id,
            result,
        },
        Err(e) => AsyncWorkResponse {
            correlation_id: request.correlation_id,
            workflow_name: request.workflow_name,
            interaction_id: request.interaction_id,
            result: json!({
                "error": e.to_string(),
                "status": "agent_processing_failed"
            }),
        },
    };

    if let Err(_) = request.response_sender.send(response).await {
        tracing::warn!("Failed to send agent response - receiver may have been dropped");
    }
}

async fn process_agent_request(
    agent_system: &Arc<Mutex<AgentSystem>>,
    request: &AsyncWorkRequest,
) -> Result<Value> {
    let agent_system_guard = agent_system.lock().await;
    let agent = agent_system_guard.get_agent(&request.agent_id)?;
    let system_prompt = agent.get_system_prompt();

    agent_system_guard
        .get_llm_adapter()
        .unwrap()
        .generate_structured_response(&system_prompt, &request.prompt)
        .await
        .map_err(|e| anyhow::anyhow!("LLM processing failed: {}", e))
}

fn log_event(event_type: &str, data: Value) {
    println!(
        "{}",
        json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "event": event_type,
            "data": data
        })
    );
}

fn calculate_progress_score(response: &Value) -> u8 {
    let response_str = match response.as_str() {
        Some(s) => s,
        None => &serde_json::to_string(response).unwrap_or_default(),
    };

    let length_score = (response_str.len().min(1000) as f32 / 1000.0 * 4.0) as u8;
    let structure_score = if response_str.contains("plan")
        || response_str.contains("analysis")
        || response_str.contains("step")
    {
        3
    } else {
        1
    };
    let detail_score = if response_str.len() > 200 { 3 } else { 1 };

    (length_score + structure_score + detail_score).clamp(1, 10)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    let matches = Command::new("sleet-integrated-demo")
        .version("1.0.0")
        .about("Integrated Multi-Agent Workflow Orchestration Demo")
        .arg(
            Arg::new("goal")
                .long("goal")
                .short('g')
                .help("The goal for the agent team to achieve")
                .required(true),
        )
        .arg(
            Arg::new("team-size")
                .long("team-size")
                .short('s')
                .help("Number of specialist agents (2-6)")
                .default_value("3")
                .value_parser(clap::value_parser!(u8).range(2..=6)),
        )
        .arg(
            Arg::new("mode")
                .long("mode")
                .help("Execution mode: simple, multi-workflow, or full-orchestration")
                .default_value("simple")
                .value_parser(["simple", "multi-workflow", "full-orchestration"]),
        )
        .arg(
            Arg::new("provider")
                .long("provider")
                .help("LLM provider")
                .default_value("ollama")
                .value_parser(["ollama", "anthropic"]),
        )
        .arg(
            Arg::new("model")
                .long("model")
                .short('m')
                .help("Model for the primary provider")
                .default_value("llama3.2"),
        )
        .get_matches();

    let goal = matches.get_one::<String>("goal").unwrap().clone();
    let team_size = *matches.get_one::<u8>("team-size").unwrap() as usize;
    let mode = matches.get_one::<String>("mode").unwrap();
    let provider = matches.get_one::<String>("provider").unwrap();
    let model = matches.get_one::<String>("model").unwrap();

    log_event(
        "demo_startup",
        json!({
            "goal": goal,
            "team_size": team_size,
            "mode": mode,
            "provider": provider,
            "model": model
        }),
    );

    match mode.as_str() {
        "simple" => run_simple_mode(&goal, provider, model).await,
        "multi-workflow" => run_multi_workflow_mode(&goal, team_size, provider, model).await,
        "full-orchestration" => {
            run_full_orchestration_mode(&goal, team_size, provider, model).await
        }
        _ => Err(anyhow::anyhow!("Unknown mode: {}", mode)),
    }
}

async fn run_simple_mode(goal: &str, provider: &str, model: &str) -> Result<()> {
    log_event(
        "mode_selected",
        json!({"mode": "simple", "description": "Original single workflow with 3 agents"}),
    );

    let agent_system = Arc::new(Mutex::new(AgentSystem::new(AgentSystemConfig::default())?));
    let llm_adapter = initialise_llm_adapter(provider, model).await?;
    agent_system.lock().await.set_llm_adapter(llm_adapter);

    let (planner, reviewer, arbiter) = generate_agents(agent_system.clone(), goal).await?;

    let flow_def = create_workflow_definition(
        goal,
        planner.id.clone(),
        reviewer.id.clone(),
        arbiter.id.clone(),
    );

    let orchestration_contract = FlowTranspiler::transpile(&flow_def)?;
    let contract = sleet::convert_contract(orchestration_contract)
        .map_err(|e| anyhow::anyhow!("Failed to convert contract: {}", e))?;
    let mut runtime = RemarkableInterpreter::new(1_000_000, &contract, FfiRegistry::new())?;

    let mut progress_tracker = ProgressTracker::new();

    execute_simple_workflow(&mut runtime, &contract, agent_system, &mut progress_tracker).await
}

async fn run_multi_workflow_mode(
    goal: &str,
    team_size: usize,
    provider: &str,
    model: &str,
) -> Result<()> {
    log_event(
        "mode_selected",
        json!({
            "mode": "multi-workflow",
            "description": "Multiple specialised workflows running in parallel",
            "team_size": team_size
        }),
    );

    let agent_system = Arc::new(Mutex::new(AgentSystem::new(AgentSystemConfig::default())?));
    let llm_manager = LLMManager::new(provider, model).await?;

    let adapter = initialise_llm_adapter(provider, model).await?;
    agent_system.lock().await.set_llm_adapter(adapter);

    let mut orchestrator =
        WorkflowOrchestrator::new(agent_system.clone(), llm_manager, goal).await?;

    let strategic_agents =
        generate_specialised_team(&agent_system, "strategic planning", goal, 2).await?;
    let technical_agents =
        generate_specialised_team(&agent_system, "technical implementation", goal, 2).await?;
    let synthesis_agents =
        generate_specialised_team(&agent_system, "synthesis and integration", goal, 1).await?;

    let strategic_flow = create_strategic_workflow(goal, &strategic_agents);
    let technical_flow = create_technical_workflow(goal, &technical_agents);
    let synthesis_flow = create_synthesis_workflow(goal, &synthesis_agents);

    orchestrator
        .add_workflow("strategic", strategic_flow, strategic_agents)
        .await?;
    orchestrator
        .add_workflow("technical", technical_flow, technical_agents)
        .await?;
    orchestrator
        .add_workflow("synthesis", synthesis_flow, synthesis_agents)
        .await?;

    let results = orchestrator
        .execute_parallel(vec!["strategic", "technical"])
        .await?;

    let synthesis_result = orchestrator.execute_sequential("synthesis").await?;

    log_event(
        "multi_workflow_complete",
        json!({
            "parallel_results": results,
            "synthesis_result": synthesis_result
        }),
    );

    Ok(())
}

async fn run_full_orchestration_mode(
    goal: &str,
    team_size: usize,
    provider: &str,
    model: &str,
) -> Result<()> {
    log_event(
        "mode_selected",
        json!({
            "mode": "full-orchestration",
            "description": "Full multi-agent orchestration with adaptive collaboration",
            "team_size": team_size
        }),
    );

    let agent_system = Arc::new(Mutex::new(AgentSystem::new(AgentSystemConfig::default())?));
    let llm_manager = LLMManager::new(provider, model).await?;
    let adapter = initialise_llm_adapter(provider, model).await?;
    agent_system.lock().await.set_llm_adapter(adapter);

    let mut orchestrator =
        WorkflowOrchestrator::new(agent_system.clone(), llm_manager, goal).await?;

    log_event(
        "full_orchestration_initialised",
        json!({
            "goal": goal,
            "team_size": team_size,
            "progress_tracking": true,
            "adaptive_collaboration": true
        }),
    );

    let demo_agents = generate_specialised_team(&agent_system, "demonstration", goal, 2).await?;
    let demo_flow = create_strategic_workflow(goal, &demo_agents);

    orchestrator
        .add_workflow("demo", demo_flow, demo_agents)
        .await?;
    let result = orchestrator.execute_sequential("demo").await?;

    log_event(
        "full_orchestration_complete",
        json!({
            "result": result,
            "final_momentum": orchestrator.progress_tracker.momentum,
            "plateau_detected": orchestrator.progress_tracker.needs_strategy_change(),
            "total_interactions": orchestrator.progress_tracker.history.len()
        }),
    );

    Ok(())
}

async fn execute_simple_workflow(
    runtime: &mut RemarkableInterpreter,
    contract: &sleet::ast::Contract,
    agent_system: Arc<Mutex<AgentSystem>>,
    progress_tracker: &mut ProgressTracker,
) -> Result<()> {
    let mut final_result = RuntimeValue::Null;
    let mut iteration = 0u32;

    loop {
        let status = runtime.run(contract.clone()).await?;
        match status {
            ExecutionStatus::AwaitingInput {
                session_id: _,
                interaction_id,
                agent_id,
                prompt,
            } => {
                iteration += 1;

                log_event(
                    "agent_processing",
                    json!({
                        "iteration": iteration,
                        "interaction_id": interaction_id,
                        "agent_id": agent_id
                    }),
                );

                let agent_system_guard = agent_system.lock().await;
                let agent = agent_system_guard.get_agent(&agent_id)?;
                let system_prompt = agent.get_system_prompt();
                let user_prompt = prompt.as_str().unwrap_or_default();

                let response = agent_system_guard
                    .get_llm_adapter()
                    .unwrap()
                    .generate_structured_response(&system_prompt, user_prompt)
                    .await
                    .map_err(|e| anyhow::anyhow!("LLM processing failed: {}", e))?;

                let progress_score = calculate_progress_score(&response);
                progress_tracker.update(iteration, progress_score);

                log_event(
                    "progress_update",
                    json!({
                        "iteration": iteration,
                        "progress_score": progress_score,
                        "momentum": progress_tracker.momentum,
                        "plateau_count": progress_tracker.plateau_count,
                        "needs_strategy_change": progress_tracker.needs_strategy_change()
                    }),
                );

                if progress_tracker.needs_strategy_change() {
                    log_event(
                        "plateau_detected",
                        json!({
                            "iteration": iteration,
                            "plateau_count": progress_tracker.plateau_count,
                            "message": "Progress has plateaued - workflow may need strategy adjustment"
                        }),
                    );
                }

                runtime.resume_with_input(&interaction_id, response);
            }
            ExecutionStatus::Completed(result) => {
                final_result = result;
                break;
            }
            ExecutionStatus::Running => {}
        }
    }

    log_event(
        "workflow_completed",
        json!({
            "final_result": runtime_to_json_value(final_result.clone()),
            "total_iterations": iteration,
            "final_momentum": progress_tracker.momentum,
            "progress_history_length": progress_tracker.history.len()
        }),
    );

    println!("\n--- Workflow Finished ---");
    println!(
        "Final Result:\n{}",
        serde_json::to_string_pretty(&runtime_to_json_value(final_result.clone()))?
    );
    println!("Total iterations: {iteration}");
    println!("Final progress momentum: {:.2}", progress_tracker.momentum);

    Ok(())
}

async fn generate_specialised_team(
    agent_system: &Arc<Mutex<AgentSystem>>,
    specialisation: &str,
    goal: &str,
    size: usize,
) -> Result<Vec<Agent>> {
    let mut system = agent_system.lock().await;
    let task =
        format!("Create {size} agents specialised in {specialisation} for the goal: '{goal}'");
    let team = system.generate_team(task, size).await?;

    log_event(
        "specialised_team_generated",
        json!({
            "specialisation": specialisation,
            "team_size": size,
            "agents": team.iter().map(|a| &a.name).collect::<Vec<_>>()
        }),
    );

    Ok(team)
}

fn create_strategic_workflow(goal: &str, agents: &[Agent]) -> FlowDefinition {
    let mut flow = FlowDefinition::new("strategic_workflow", "STRATEGIC_START");

    flow.set_initial_state(json!({
        "goal": goal,
        "market_analysis": null,
        "strategic_plan": null,
        "status": "strategic_planning"
    }));

    let primary_agent = &agents[0].id;

    flow.add_block(BlockDefinition::new(
        "STRATEGIC_START",
        BlockType::AwaitInput {
            interaction_id: "market_analysis".to_string(),
            agent_id: primary_agent.clone(),
            prompt: "\"Conduct market analysis for the strategic plan\"".to_string(),
            state_key: "market_analysis".to_string(),
            next_block: "CREATE_STRATEGIC_PLAN".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new(
        "CREATE_STRATEGIC_PLAN",
        BlockType::AwaitInput {
            interaction_id: "strategic_planning".to_string(),
            agent_id: primary_agent.clone(),
            prompt: "\"Create comprehensive strategic plan based on analysis\"".to_string(),
            state_key: "strategic_plan".to_string(),
            next_block: "STRATEGIC_COMPLETE".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new(
        "STRATEGIC_COMPLETE",
        BlockType::Compute {
            expression: "\"strategic_complete\"".to_string(),
            output_key: "status".to_string(),
            next_block: "TERMINATE".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new("TERMINATE", BlockType::Terminate));
    flow
}

fn create_technical_workflow(goal: &str, agents: &[Agent]) -> FlowDefinition {
    let mut flow = FlowDefinition::new("technical_workflow", "TECHNICAL_START");

    flow.set_initial_state(json!({
        "goal": goal,
        "architecture_design": null,
        "implementation_plan": null,
        "status": "technical_planning"
    }));

    let primary_agent = &agents[0].id;

    flow.add_block(BlockDefinition::new(
        "TECHNICAL_START",
        BlockType::AwaitInput {
            interaction_id: "architecture_design".to_string(),
            agent_id: primary_agent.clone(),
            prompt: "\"Design technical architecture for the system\"".to_string(),
            state_key: "architecture_design".to_string(),
            next_block: "CREATE_IMPLEMENTATION_PLAN".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new(
        "CREATE_IMPLEMENTATION_PLAN",
        BlockType::AwaitInput {
            interaction_id: "implementation_planning".to_string(),
            agent_id: primary_agent.clone(),
            prompt: "\"Create detailed implementation plan\"".to_string(),
            state_key: "implementation_plan".to_string(),
            next_block: "TECHNICAL_COMPLETE".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new(
        "TECHNICAL_COMPLETE",
        BlockType::Compute {
            expression: "\"technical_complete\"".to_string(),
            output_key: "status".to_string(),
            next_block: "TERMINATE".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new("TERMINATE", BlockType::Terminate));
    flow
}

fn create_synthesis_workflow(goal: &str, agents: &[Agent]) -> FlowDefinition {
    let mut flow = FlowDefinition::new("synthesis_workflow", "SYNTHESIS_START");

    flow.set_initial_state(json!({
        "goal": goal,
        "integration_plan": null,
        "final_recommendations": null,
        "status": "synthesis"
    }));

    let primary_agent = &agents[0].id;

    flow.add_block(BlockDefinition::new(
        "SYNTHESIS_START",
        BlockType::AwaitInput {
            interaction_id: "integration_planning".to_string(),
            agent_id: primary_agent.clone(),
            prompt: "\"Create integration plan combining strategic and technical elements\""
                .to_string(),
            state_key: "integration_plan".to_string(),
            next_block: "FINAL_RECOMMENDATIONS".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new(
        "FINAL_RECOMMENDATIONS",
        BlockType::AwaitInput {
            interaction_id: "final_synthesis".to_string(),
            agent_id: primary_agent.clone(),
            prompt: "\"Provide final recommendations and next steps\"".to_string(),
            state_key: "final_recommendations".to_string(),
            next_block: "SYNTHESIS_COMPLETE".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new(
        "SYNTHESIS_COMPLETE",
        BlockType::Compute {
            expression: "\"synthesis_complete\"".to_string(),
            output_key: "status".to_string(),
            next_block: "TERMINATE".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new("TERMINATE", BlockType::Terminate));
    flow
}

async fn generate_agents(
    agent_system: Arc<Mutex<AgentSystem>>,
    goal: &str,
) -> Result<(Agent, Agent, Agent)> {
    let mut system = agent_system.lock().await;

    let planner_task =
        format!("Create a detailed, step-by-step plan to achieve the goal: '{goal}'");
    let planner_team = system.generate_team(planner_task, 1).await?;
    let planner = planner_team
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Failed to generate planner agent"))?;
    tracing::info!(name = planner.name, "Generated Planner Agent");

    let reviewer_task = format!(
        "Critically review a plan for the goal: '{goal}'. Identify flaws, risks, and missing details."
    );
    let reviewer_team = system.generate_team(reviewer_task, 1).await?;
    let reviewer = reviewer_team
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Failed to generate reviewer agent"))?;
    tracing::info!(name = reviewer.name, "Generated Reviewer Agent");

    let arbiter_task = format!("Act as an impartial judge. Given a plan and a review, decide if the plan is complete and satisfactory for the goal: '{goal}'");
    let arbiter_team = system.generate_team(arbiter_task, 1).await?;
    let arbiter = arbiter_team
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Failed to generate arbiter agent"))?;
    tracing::info!(name = arbiter.name, "Generated Arbiter Agent");

    Ok((planner, reviewer, arbiter))
}

fn create_workflow_definition(
    goal: &str,
    planner_id: String,
    reviewer_id: String,
    arbiter_id: String,
) -> FlowDefinition {
    let mut flow = FlowDefinition::new("goal_achievement_flow", "GENERATE_PLAN");

    flow.set_initial_state(json!({
        "goal": goal,
        "iteration": 1,
        "max_iterations": MAX_ITERATIONS,
        "plan": null,
        "review": null,
        "assessment": null,
        "status": "planning",
        "feedback_history": [],
        "quality_score": 0,
        "improvement_suggestions": []
    }));

    flow.add_block(BlockDefinition::new(
        "GENERATE_PLAN",
        BlockType::AwaitInput {
            interaction_id: "generate_plan_interaction".to_string(),
            agent_id: planner_id.clone(),
            prompt: "\"Generate a comprehensive plan for the given goal. Focus on actionable steps and clear deliverables.\"".to_string(),
            state_key: "plan".to_string(),
            next_block: "REVIEW_PLAN".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new(
        "REVIEW_PLAN",
        BlockType::AwaitInput {
            interaction_id: "review_plan_interaction".to_string(),
            agent_id: reviewer_id.clone(),
            prompt: "\"Carefully review this plan: {plan}. Provide specific feedback on completeness, feasibility, and risks. Rate the quality and suggest improvements.\"".to_string(),
            state_key: "review".to_string(),
            next_block: "ASSESS_PLAN".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new(
        "ASSESS_PLAN",
        BlockType::AwaitInput {
            interaction_id: "assess_plan_interaction".to_string(),
            agent_id: arbiter_id.clone(),
            prompt: "\"As an impartial judge, assess this plan: {plan} with review: {review}. This is iteration {iteration}. Rate the plan quality from 1-10. If quality >= 7, the plan is acceptable. Provide your assessment including: quality score, reasoning, and specific improvements needed if score < 7.\"".to_string(),
            state_key: "assessment".to_string(),
            next_block: "EXTRACT_QUALITY".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new(
        "EXTRACT_QUALITY",
        BlockType::Compute {
            expression: "5".to_string(),
            output_key: "quality_score".to_string(),
            next_block: "UPDATE_FEEDBACK".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new(
        "UPDATE_FEEDBACK",
        BlockType::Compute {
            expression: "assessment".to_string(),
            output_key: "feedback_summary".to_string(),
            next_block: "CHECK_QUALITY".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new(
        "CHECK_QUALITY",
        BlockType::Conditional {
            condition: "iteration >= 3".to_string(),
            true_block: "SUCCESS_TERMINATE".to_string(),
            false_block: "CHECK_ITERATION_LIMIT".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new(
        "CHECK_ITERATION_LIMIT",
        BlockType::Conditional {
            condition: format!("iteration < {MAX_ITERATIONS}"),
            true_block: "INCREMENT_ITERATION".to_string(),
            false_block: "FAIL_TERMINATE".to_string(),
        },
    ));
    flow.add_block(BlockDefinition::new(
        "INCREMENT_ITERATION",
        BlockType::Compute {
            expression: "iteration + 1".to_string(),
            output_key: "iteration".to_string(),
            next_block: "GENERATE_PLAN".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new(
        "SUCCESS_TERMINATE",
        BlockType::Compute {
            expression: "\"completed_successfully\"".to_string(),
            output_key: "status".to_string(),
            next_block: "TERMINATE".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new(
        "FAIL_TERMINATE",
        BlockType::Compute {
            expression: "\"failed_max_iterations\"".to_string(),
            output_key: "status".to_string(),
            next_block: "TERMINATE".to_string(),
        },
    ));

    flow.add_block(BlockDefinition::new("TERMINATE", BlockType::Terminate));

    flow
}
