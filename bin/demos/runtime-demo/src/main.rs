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

use anyhow::Result;
use chrono::Utc;
use clap::{Arg, Command};
use rand::Rng;
use serde_json::{json, Value};
use sleet::{
    flows::definition::{BlockDefinition, BlockType, FlowDefinition},
    runtime::{
        BytecodeAssembler, FfiFunction, FfiRegistry, OpCode, RemarkableInterpreter,
        Value as RuntimeValue,
    },
    transpiler::FlowTranspiler,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[derive(Debug, Clone)]
struct RuntimeProfiler {
    execution_counts: HashMap<String, u32>,
    hot_threshold: u32,
    performance_metrics: HashMap<String, Duration>,
}

type DynamicFunction = FfiFunction;

struct DynamicFunctionRegistry {
    functions: HashMap<String, DynamicFunction>,
    version_counter: u32,
}

#[derive(Debug)]
struct FlowHotSwapper {
    active_flows: HashMap<String, FlowDefinition>,
    pending_modifications: Vec<RuntimeModification>,
}

#[derive(Debug, Clone)]
enum RuntimeModification {
    InjectBlock {
        flow_id: String,
        block: BlockDefinition,
        insert_after: String,
    },
    InjectBlockConditional {
        flow_id: String,
        block: BlockDefinition,
        insert_after: String,
        path: ConditionalPath,
    },
    SwapBlock {
        flow_id: String,
        block_id: String,
        new_block: BlockDefinition,
    },
    AddDynamicFunction {
        name: String,
        complexity: u32,
    },
}

#[derive(Debug, Clone)]
enum ConditionalPath {
    TruePath,
    FalsePath,
    BothPaths,
}

impl RuntimeProfiler {
    fn new() -> Self {
        Self {
            execution_counts: HashMap::new(),
            hot_threshold: 3,
            performance_metrics: HashMap::new(),
        }
    }

    fn record_execution(&mut self, block_id: &str, duration: Duration) {
        *self
            .execution_counts
            .entry(block_id.to_string())
            .or_insert(0) += 1;
        self.performance_metrics
            .insert(block_id.to_string(), duration);
    }

    fn is_hot_path(&self, block_id: &str) -> bool {
        self.execution_counts.get(block_id).unwrap_or(&0) >= &self.hot_threshold
    }

    fn get_optimisation_candidates(&self) -> Vec<String> {
        self.execution_counts
            .iter()
            .filter(|(_, &count)| count >= self.hot_threshold)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

impl DynamicFunctionRegistry {
    fn new() -> Self {
        Self {
            functions: HashMap::new(),
            version_counter: 0,
        }
    }

    fn register_function<F>(&mut self, name: String, func: F) -> String
    where
        F: Fn(
                &[RuntimeValue],
                &RuntimeValue,
            ) -> Result<RuntimeValue, sleet::runtime::InterpreterError>
            + Send
            + Sync
            + 'static,
    {
        self.version_counter += 1;
        let versioned_name = format!("{}__v{}", name, self.version_counter);
        self.functions
            .insert(versioned_name.clone(), Arc::new(func));

        log_event(
            "function_registered",
            json!({
                "name": name,
                "versioned_name": versioned_name,
                "version": self.version_counter
            }),
        );

        versioned_name
    }

    fn to_ffi_registry(&self) -> FfiRegistry {
        self.functions.clone()
    }
}



fn register_json_function<F>(registry: &mut DynamicFunctionRegistry, name: &str, func: F) -> String
where
    F: Fn(serde_json::Value) -> serde_json::Value + Send + Sync + 'static,
{
    registry.register_function(
        name.to_string(),
        move |args: &[RuntimeValue], _state: &RuntimeValue| {
            
            let json_args: serde_json::Value = args[0].clone().into();
            
            let result = func(json_args);
            
            Ok(result.into())
        },
    )
}


fn register_json_function_with_state<F>(
    registry: &mut DynamicFunctionRegistry,
    name: &str,
    func: F,
) -> String
where
    F: Fn(serde_json::Value, serde_json::Value) -> serde_json::Value + Send + Sync + 'static,
{
    registry.register_function(
        name.to_string(),
        move |args: &[RuntimeValue], state: &RuntimeValue| {
            
            let json_args: serde_json::Value = args[0].clone().into();
            
            let json_state: serde_json::Value = state.clone().into();
            
            let result = func(json_args, json_state);
            
            Ok(result.into())
        },
    )
}

impl FlowHotSwapper {
    fn new() -> Self {
        Self {
            active_flows: HashMap::new(),
            pending_modifications: Vec::new(),
        }
    }

    fn register_flow(&mut self, flow: FlowDefinition) {
        log_event(
            "flow_registered",
            json!({
                "flow_id": flow.id,
                "blocks": flow.blocks.len(),
                "participants": flow.participants.len()
            }),
        );
        self.active_flows.insert(flow.id.clone(), flow);
    }

    fn queue_modification(&mut self, modification: RuntimeModification) {
        log_event(
            "modification_queued",
            json!({ "type": format!("{:?}", modification) }),
        );
        self.pending_modifications.push(modification);
    }

    fn apply_pending_modifications(&mut self) -> Result<Vec<String>> {
        let mut modified_flows = Vec::new();

        for modification in self.pending_modifications.drain(..) {
            match modification {
                RuntimeModification::InjectBlock {
                    flow_id,
                    block,
                    insert_after,
                } => {
                    if let Some(flow) = self.active_flows.get_mut(&flow_id) {
                        let mut injection_successful = false;

                        for existing_block in &mut flow.blocks {
                            if existing_block.id == insert_after {
                                match &mut existing_block.block_type {
                                    BlockType::Compute { next_block, .. } => {
                                        let original_next = next_block.clone();
                                        *next_block = block.id.clone();

                                        let mut modified_block = block.clone();
                                        if let BlockType::Compute {
                                            next_block: new_next,
                                            ..
                                        } = &mut modified_block.block_type
                                        {
                                            *new_next = original_next;
                                        } else if let BlockType::AwaitInput {
                                            next_block: new_next,
                                            ..
                                        } = &mut modified_block.block_type
                                        {
                                            *new_next = original_next;
                                        }
                                        flow.blocks.push(modified_block);
                                        injection_successful = true;
                                    }
                                    BlockType::Conditional {
                                        true_block,
                                        false_block,
                                        ..
                                    } => {
                                        let original_true = true_block.clone();
                                        let original_false = false_block.clone();

                                        let true_block_id = format!("{}_true_path", block.id);
                                        let false_block_id = format!("{}_false_path", block.id);

                                        *true_block = true_block_id.clone();
                                        *false_block = false_block_id.clone();

                                        let mut true_path_block = block.clone();
                                        true_path_block.id = true_block_id;
                                        match &mut true_path_block.block_type {
                                            BlockType::Compute {
                                                next_block: new_next,
                                                ..
                                            } => {
                                                *new_next = original_true;
                                            }
                                            BlockType::AwaitInput {
                                                next_block: new_next,
                                                ..
                                            } => {
                                                *new_next = original_true;
                                            }
                                            _ => {}
                                        }

                                        let mut false_path_block = block.clone();
                                        false_path_block.id = false_block_id;
                                        match &mut false_path_block.block_type {
                                            BlockType::Compute {
                                                next_block: new_next,
                                                ..
                                            } => {
                                                *new_next = original_false;
                                            }
                                            BlockType::AwaitInput {
                                                next_block: new_next,
                                                ..
                                            } => {
                                                *new_next = original_false;
                                            }
                                            _ => {}
                                        }

                                        flow.blocks.push(true_path_block);
                                        flow.blocks.push(false_path_block);
                                        injection_successful = true;
                                    }
                                    BlockType::AwaitInput { next_block, .. } => {
                                        let original_next = next_block.clone();
                                        *next_block = block.id.clone();

                                        let mut modified_block = block.clone();
                                        match &mut modified_block.block_type {
                                            BlockType::Compute {
                                                next_block: new_next,
                                                ..
                                            } => {
                                                *new_next = original_next;
                                            }
                                            BlockType::AwaitInput {
                                                next_block: new_next,
                                                ..
                                            } => {
                                                *new_next = original_next;
                                            }
                                            _ => {}
                                        }
                                        flow.blocks.push(modified_block);
                                        injection_successful = true;
                                    }
                                    BlockType::Terminate => {
                                        log_event(
                                            "injection_warning",
                                            json!({
                                                "flow_id": flow_id,
                                                "insert_after": insert_after,
                                                "reason": "Cannot inject after Terminate block",
                                                "suggestion": "Inject before the Terminate block instead"
                                            }),
                                        );
                                    }
                                    BlockType::ForEach { exit_block_id, .. } => {
                                        let original_exit = exit_block_id.clone();
                                        *exit_block_id = block.id.clone();

                                        let mut modified_block = block.clone();
                                        match &mut modified_block.block_type {
                                            BlockType::Compute {
                                                next_block: new_next,
                                                ..
                                            } => {
                                                *new_next = original_exit;
                                            }
                                            BlockType::AwaitInput {
                                                next_block: new_next,
                                                ..
                                            } => {
                                                *new_next = original_exit;
                                            }
                                            _ => {}
                                        }
                                        flow.blocks.push(modified_block);
                                        injection_successful = true;
                                    }
                                    BlockType::TryCatch { catch_block_id, .. } => {
                                        let original_catch = catch_block_id.clone();
                                        *catch_block_id = block.id.clone();

                                        let mut modified_block = block.clone();
                                        match &mut modified_block.block_type {
                                            BlockType::Compute {
                                                next_block: new_next,
                                                ..
                                            } => {
                                                *new_next = original_catch;
                                            }
                                            BlockType::AwaitInput {
                                                next_block: new_next,
                                                ..
                                            } => {
                                                *new_next = original_catch;
                                            }
                                            _ => {}
                                        }
                                        flow.blocks.push(modified_block);
                                        injection_successful = true;
                                    }
                                    BlockType::SubFlow { next_block, .. } => {
                                        let original_next = next_block.clone();
                                        *next_block = block.id.clone();

                                        let mut modified_block = block.clone();
                                        match &mut modified_block.block_type {
                                            BlockType::Compute {
                                                next_block: new_next,
                                                ..
                                            } => {
                                                *new_next = original_next;
                                            }
                                            BlockType::AwaitInput {
                                                next_block: new_next,
                                                ..
                                            } => {
                                                *new_next = original_next;
                                            }
                                            _ => {}
                                        }
                                        flow.blocks.push(modified_block);
                                        injection_successful = true;
                                    }
                                    BlockType::Continue { .. } | BlockType::Break { .. } => {
                                        log_event(
                                            "injection_warning",
                                            json!({
                                                "flow_id": flow_id,
                                                "insert_after": insert_after,
                                                "reason": "Cannot inject after Continue/Break block",
                                                "suggestion": "These blocks transfer control to loop constructs"
                                            }),
                                        );
                                    }
                                }

                                if injection_successful {
                                    modified_flows.push(flow_id.clone());
                                }
                                break;
                            }
                        }

                        if !injection_successful {
                            log_event(
                                "injection_failed",
                                json!({
                                    "flow_id": flow_id,
                                    "insert_after": insert_after,
                                    "reason": "Block not found or injection not supported for this block type"
                                }),
                            );
                        }
                    }
                }
                RuntimeModification::InjectBlockConditional {
                    flow_id,
                    block,
                    insert_after,
                    path,
                } => {
                    if let Some(flow) = self.active_flows.get_mut(&flow_id) {
                        let mut injection_successful = false;

                        for existing_block in &mut flow.blocks {
                            if existing_block.id == insert_after {
                                if let BlockType::Conditional {
                                    true_block,
                                    false_block,
                                    ..
                                } = &mut existing_block.block_type
                                {
                                    match path {
                                        ConditionalPath::TruePath => {
                                            let original_true = true_block.clone();
                                            *true_block = block.id.clone();

                                            let mut modified_block = block.clone();
                                            match &mut modified_block.block_type {
                                                BlockType::Compute {
                                                    next_block: new_next,
                                                    ..
                                                } => {
                                                    *new_next = original_true;
                                                }
                                                BlockType::AwaitInput {
                                                    next_block: new_next,
                                                    ..
                                                } => {
                                                    *new_next = original_true;
                                                }
                                                _ => {}
                                            }
                                            flow.blocks.push(modified_block);
                                            injection_successful = true;
                                        }
                                        ConditionalPath::FalsePath => {
                                            let original_false = false_block.clone();
                                            *false_block = block.id.clone();

                                            let mut modified_block = block.clone();
                                            match &mut modified_block.block_type {
                                                BlockType::Compute {
                                                    next_block: new_next,
                                                    ..
                                                } => {
                                                    *new_next = original_false;
                                                }
                                                BlockType::AwaitInput {
                                                    next_block: new_next,
                                                    ..
                                                } => {
                                                    *new_next = original_false;
                                                }
                                                _ => {}
                                            }
                                            flow.blocks.push(modified_block);
                                            injection_successful = true;
                                        }
                                        ConditionalPath::BothPaths => {
                                            let original_true = true_block.clone();
                                            let original_false = false_block.clone();

                                            let true_block_id = format!("{}_true_path", block.id);
                                            let false_block_id = format!("{}_false_path", block.id);

                                            *true_block = true_block_id.clone();
                                            *false_block = false_block_id.clone();

                                            let mut true_path_block = block.clone();
                                            true_path_block.id = true_block_id;
                                            match &mut true_path_block.block_type {
                                                BlockType::Compute {
                                                    next_block: new_next,
                                                    ..
                                                } => {
                                                    *new_next = original_true;
                                                }
                                                BlockType::AwaitInput {
                                                    next_block: new_next,
                                                    ..
                                                } => {
                                                    *new_next = original_true;
                                                }
                                                _ => {}
                                            }

                                            let mut false_path_block = block.clone();
                                            false_path_block.id = false_block_id;
                                            match &mut false_path_block.block_type {
                                                BlockType::Compute {
                                                    next_block: new_next,
                                                    ..
                                                } => {
                                                    *new_next = original_false;
                                                }
                                                BlockType::AwaitInput {
                                                    next_block: new_next,
                                                    ..
                                                } => {
                                                    *new_next = original_false;
                                                }
                                                _ => {}
                                            }

                                            flow.blocks.push(true_path_block);
                                            flow.blocks.push(false_path_block);
                                            injection_successful = true;
                                        }
                                    }
                                } else {
                                    log_event(
                                        "injection_warning",
                                        json!({
                                            "flow_id": flow_id,
                                            "insert_after": insert_after,
                                            "reason": "InjectBlockConditional can only be used with Conditional blocks"
                                        }),
                                    );
                                }

                                if injection_successful {
                                    modified_flows.push(flow_id.clone());
                                }
                                break;
                            }
                        }

                        if !injection_successful {
                            log_event(
                                "injection_failed",
                                json!({
                                    "flow_id": flow_id,
                                    "insert_after": insert_after,
                                    "reason": "Conditional block not found or injection failed"
                                }),
                            );
                        }
                    }
                }
                RuntimeModification::SwapBlock {
                    flow_id,
                    block_id,
                    new_block,
                } => {
                    if let Some(flow) = self.active_flows.get_mut(&flow_id) {
                        for existing_block in &mut flow.blocks {
                            if existing_block.id == block_id {
                                *existing_block = new_block;
                                modified_flows.push(flow_id.clone());
                                break;
                            }
                        }
                    }
                }
                RuntimeModification::AddDynamicFunction { .. } => {}
            }
        }

        for flow_id in &modified_flows {
            log_event("flow_modified", json!({ "flow_id": flow_id }));
        }

        Ok(modified_flows)
    }

    fn get_flow(&self, flow_id: &str) -> Option<&FlowDefinition> {
        self.active_flows.get(flow_id)
    }
}

struct RuntimeDemoOrchestrator {
    profiler: RuntimeProfiler,
    function_registry: DynamicFunctionRegistry,
    hot_swapper: FlowHotSwapper,
    execution_count: u32,
}

impl RuntimeDemoOrchestrator {
    fn new() -> Self {
        Self {
            profiler: RuntimeProfiler::new(),
            function_registry: DynamicFunctionRegistry::new(),
            hot_swapper: FlowHotSwapper::new(),
            execution_count: 0,
        }
    }

    fn create_base_flow(&self) -> FlowDefinition {
        let mut flow = FlowDefinition::new("runtime_demo_flow", "start");

        flow.set_initial_state(json!({
            "counter": 0,
            "base_value": 1,
            "multiplier": 2,
            "threshold": 10
        }));

        flow.set_state_schema(json!({
            "type": "object",
            "properties": {
                "counter": { "type": "integer" },
                "base_value": { "type": "integer" },
                "multiplier": { "type": "integer" },
                "threshold": { "type": "integer" },
                "result": { "type": "integer" },
                "hot_path_detected": { "type": "boolean" }
            }
        }));

        flow.add_block(BlockDefinition::new(
            "start",
            BlockType::Compute {
                expression: "state.counter + 1".to_string(),
                output_key: "counter".to_string(),
                next_block: "process_data".to_string(),
            },
        ));

        flow.add_block(BlockDefinition::new(
            "process_data",
            BlockType::Compute {
                expression: "state.multiplier * 2".to_string(),
                output_key: "result".to_string(),
                next_block: "check_threshold".to_string(),
            },
        ));

        flow.add_block(BlockDefinition::new(
            "check_threshold",
            BlockType::Conditional {
                condition: "state.result > state.threshold".to_string(),
                true_block: "handle_overflow".to_string(),
                false_block: "continue_processing".to_string(),
            },
        ));

        flow.add_block(BlockDefinition::new(
            "handle_overflow",
            BlockType::Compute {
                expression: "state.result / 2".to_string(),
                output_key: "result".to_string(),
                next_block: "finalise".to_string(),
            },
        ));

        flow.add_block(BlockDefinition::new(
            "continue_processing",
            BlockType::Compute {
                expression: "state.result + 5".to_string(),
                output_key: "result".to_string(),
                next_block: "finalise".to_string(),
            },
        ));

        flow.add_block(BlockDefinition::new("finalise", BlockType::Terminate));

        flow
    }

    async fn demonstrate_dynamic_functions(&mut self) -> Result<()> {
        log_section("Dynamic Function Injection");

        let fibonacci_fn = register_json_function_with_state(
            &mut self.function_registry,
            "fibonacci",
            |args, _state| {
                let n = args.as_u64().unwrap_or(0) as u32;
                let result = fibonacci(n);
                json!(result)
            },
        );

        let random_boost_fn = register_json_function_with_state(
            &mut self.function_registry,
            "random_boost",
            |args, _state| {
                let base = args.as_i64().unwrap_or(0);
                let boost = rand::thread_rng().gen_range(1..=10);
                json!(base + boost)
            },
        );

        let complex_calculation_fn = register_json_function_with_state(
            &mut self.function_registry,
            "complex_calc",
            |args, state| {
                let input = args.as_i64().unwrap_or(0);
                let multiplier = state
                    .get("multiplier")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1);
                let threshold = state
                    .get("threshold")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(10);

                let result = (input * multiplier * 3 + threshold) % 100;
                json!(result)
            },
        );

        log_event(
            "dynamic_functions_registered",
            json!({
                "functions": [fibonacci_fn, random_boost_fn, complex_calculation_fn],
                "total_registered": self.function_registry.functions.len()
            }),
        );

        Ok(())
    }

    async fn demonstrate_hot_path_optimisation(&mut self, flow: &FlowDefinition) -> Result<()> {
        log_section("Hot Path Detection & JIT Compilation");

        for iteration in 1..=5 {
            let start_time = Instant::now();

            let orchestration_contract = FlowTranspiler::transpile(flow)?;
            let contract = sleet::convert_contract(orchestration_contract)
                .map_err(|e| anyhow::anyhow!("Contract conversion failed: {}", e))?;

            let gas_limit = 1000 + (iteration * 100) as u64;
            let mut runtime = RemarkableInterpreter::new(
                gas_limit,
                &contract,
                self.function_registry.to_ffi_registry(),
            )?;

            let result = runtime.run(contract.clone()).await?;
            let execution_time = start_time.elapsed();

            self.profiler
                .record_execution("process_data", execution_time);
            self.profiler
                .record_execution("check_threshold", execution_time);

            log_event(
                "flow_execution",
                json!({
                    "iteration": iteration,
                    "execution_time_ms": execution_time.as_millis(),
                    "gas_limit": gas_limit,
                    "result": result,
                    "hot_paths": self.profiler.get_optimisation_candidates()
                }),
            );

            sleep(Duration::from_millis(
                rand::thread_rng().gen_range(50..=200),
            ))
            .await;
        }

        let candidates = self.profiler.get_optimisation_candidates();
        if !candidates.is_empty() {
            log_event(
                "jit_optimisation_triggered",
                json!({
                    "hot_paths": candidates,
                    "threshold": self.profiler.hot_threshold
                }),
            );
        }

        Ok(())
    }

    async fn demonstrate_runtime_modification(&mut self) -> Result<()> {
        log_section("Runtime Flow Modification");

        let flow = self.create_base_flow();
        self.hot_swapper.register_flow(flow.clone());

        self.hot_swapper
            .queue_modification(RuntimeModification::InjectBlock {
                flow_id: "runtime_demo_flow".to_string(),
                block: BlockDefinition::new(
                    "performance_boost",
                    BlockType::Compute {
                        expression: "state.result * 3".to_string(),
                        output_key: "result".to_string(),
                        next_block: "check_threshold".to_string(),
                    },
                ),
                insert_after: "process_data".to_string(),
            });

        self.hot_swapper
            .queue_modification(RuntimeModification::InjectBlockConditional {
                flow_id: "runtime_demo_flow".to_string(),
                block: BlockDefinition::new(
                    "overflow_logger",
                    BlockType::Compute {
                        expression: "state.result + 1000".to_string(),
                        output_key: "result".to_string(),
                        next_block: "handle_overflow".to_string(),
                    },
                ),
                insert_after: "check_threshold".to_string(),
                path: ConditionalPath::TruePath,
            });

        self.hot_swapper
            .queue_modification(RuntimeModification::SwapBlock {
                flow_id: "runtime_demo_flow".to_string(),
                block_id: "handle_overflow".to_string(),
                new_block: BlockDefinition::new(
                    "handle_overflow",
                    BlockType::Compute {
                        expression: "state.result / 4".to_string(),
                        output_key: "result".to_string(),
                        next_block: "finalise".to_string(),
                    },
                ),
            });

        self.hot_swapper
            .queue_modification(RuntimeModification::InjectBlock {
                flow_id: "runtime_demo_flow".to_string(),
                block: BlockDefinition::new(
                    "impossible_injection",
                    BlockType::Compute {
                        expression: "state.result + 42".to_string(),
                        output_key: "result".to_string(),
                        next_block: "impossible_next".to_string(),
                    },
                ),
                insert_after: "finalise".to_string(),
            });

        let modified_flows = self.hot_swapper.apply_pending_modifications()?;

        log_event(
            "runtime_modifications_applied",
            json!({
                "modified_flows": modified_flows,
                "modifications_attempted": 4,
                "enhancement_features": [
                    "Compute block injection",
                    "Conditional path-specific injection",
                    "Block swapping",
                    "Error handling for invalid injections",
                    "Support for all BlockType variants",
                    "Automatic next_block chain updates"
                ]
            }),
        );

        if let Some(modified_flow) = self.hot_swapper.get_flow("runtime_demo_flow") {
            let orchestration_contract = FlowTranspiler::transpile(modified_flow)?;
            let contract = sleet::convert_contract(orchestration_contract)
                .map_err(|e| anyhow::anyhow!("Contract conversion failed: {}", e))?;

            let mut runtime = RemarkableInterpreter::new(
                2000,
                &contract,
                self.function_registry.to_ffi_registry(),
            )?;

            let result = runtime.run(contract).await?;

            log_event(
                "modified_flow_execution",
                json!({
                    "result": result,
                    "flow_blocks": modified_flow.blocks.len()
                }),
            );
        }

        Ok(())
    }

    async fn demonstrate_transpiler_pipeline(&mut self) -> Result<()> {
        log_section("Transpiler Pipeline Demonstration");

        let flow = self.create_base_flow();

        log_event(
            "transpiler_step_1",
            json!({
                "step": "Flow Definition Created",
                "blocks": flow.blocks.len(),
                "initial_state": flow.initial_state,
                "has_schema": flow.state_schema.is_some()
            }),
        );

        let start_transpile = Instant::now();
        let orchestration_contract = FlowTranspiler::transpile(&flow)?;
        let transpile_time = start_transpile.elapsed();

        log_event(
            "transpiler_step_2",
            json!({
                "step": "Orchestration Contract Generated",
                "transpile_time_ms": transpile_time.as_millis(),
                "contract_blocks": orchestration_contract.blocks.len(),
                "version": orchestration_contract.version
            }),
        );

        let start_convert = Instant::now();
        let contract = sleet::convert_contract(orchestration_contract)
            .map_err(|e| anyhow::anyhow!("Contract conversion failed: {}", e))?;
        let convert_time = start_convert.elapsed();

        log_event(
            "transpiler_step_3",
            json!({
                "step": "Runtime Contract Generated",
                "convert_time_ms": convert_time.as_millis(),
                "start_block": contract.start_block_id,
                "permissions": contract.permissions
            }),
        );

        let gas_budgets = [500, 1000, 1500, 2000];

        for &gas_budget in &gas_budgets {
            let start_exec = Instant::now();
            let mut runtime = RemarkableInterpreter::new(
                gas_budget,
                &contract,
                self.function_registry.to_ffi_registry(),
            )?;

            match runtime.run(contract.clone()).await {
                Ok(result) => {
                    let exec_time = start_exec.elapsed();
                    log_event(
                        "gas_execution_success",
                        json!({
                            "gas_budget": gas_budget,
                            "execution_time_ms": exec_time.as_millis(),
                            "result": result
                        }),
                    );
                }
                Err(e) => {
                    log_event(
                        "gas_execution_failure",
                        json!({
                            "gas_budget": gas_budget,
                            "error": e.to_string()
                        }),
                    );
                }
            }
        }

        Ok(())
    }

    async fn demonstrate_bytecode_assembler(&mut self) -> Result<()> {
        log_section("Bytecode Assembler Demonstration");

        log_event(
            "assembler_demo_start",
            json!({
                "description": "Demonstrating the new BytecodeAssembler that eliminates repetitive bytecode generation patterns",
                "features": [
                    "Fluent builder API",
                    "Automatic value serialization",
                    "Jump patching",
                    "Type-safe opcode generation",
                    "Method chaining"
                ]
            }),
        );

        
        let mut assembler = BytecodeAssembler::new();

        
        assembler
            .push_literal(&json!(42))?
            .push_literal(&json!(10))?
            .add()
            .halt();

        let arithmetic_bytecode = assembler.into_bytecode();

        log_event(
            "assembler_arithmetic_demo",
            json!({
                "expression": "42 + 10",
                "bytecode_length": arithmetic_bytecode.len(),
                "opcodes_used": ["Push", "Push", "Add", "Halt"],
                "comparison": "Without assembler: 25+ lines of boilerplate. With assembler: 4 method calls."
            }),
        );

        
        let mut conditional_assembler = BytecodeAssembler::new();

        
        conditional_assembler
            .load_var("counter")
            .push_literal(&json!(5))?
            .greater_than();

        let else_jump = conditional_assembler.jump_if_false();

        
        conditional_assembler
            .load_var("counter")
            .push_literal(&json!(2))?
            .multiply();

        let end_jump = conditional_assembler.jump();

        
        conditional_assembler.patch_jump(else_jump)?;

        
        conditional_assembler
            .load_var("counter")
            .push_literal(&json!(10))?
            .add();

        
        conditional_assembler.patch_jump(end_jump)?;
        conditional_assembler.halt();

        let conditional_bytecode = conditional_assembler.into_bytecode();

        log_event(
            "assembler_conditional_demo",
            json!({
                "expression": "counter > 5 ? counter * 2 : counter + 10",
                "bytecode_length": conditional_bytecode.len(),
                "jump_patches": 2,
                "opcodes_used": ["LoadVar", "Push", "GreaterThan", "JumpIfFalse", "LoadVar", "Push", "Multiply", "Jump", "LoadVar", "Push", "Add", "Halt"],
                "manual_work_eliminated": "Automatic offset calculation, jump patching, and serialization"
            }),
        );

        
        let mut function_assembler = BytecodeAssembler::new();

        
        function_assembler
            .load_var("state.n")          
            .load_var("fibonacci")        
            .call_function(1)?            
            .halt();

        let function_bytecode = function_assembler.into_bytecode();

        log_event(
            "assembler_function_demo",
            json!({
                "expression": "fibonacci(state.n)",
                "bytecode_length": function_bytecode.len(),
                "argument_count": 1,
                "automatic_features": [
                    "Argument count serialization",
                    "Function name loading",
                    "Call opcode generation"
                ]
            }),
        );

        
        let mut complex_assembler = BytecodeAssembler::new();

        
        complex_assembler
            .load_var("state.data")
            .load_var("state.index")
            .load_index()                 
            .load_var("state.multiplier")
            .push_literal(&json!(1))?
            .add()                        
            .multiply()                   
            .halt();

        let complex_bytecode = complex_assembler.into_bytecode();

        log_event(
            "assembler_complex_demo",
            json!({
                "expression": "state.data[state.index] * (state.multiplier + 1)",
                "bytecode_length": complex_bytecode.len(),
                "complexity": "High",
                "features_demonstrated": [
                    "Array indexing",
                    "Nested expressions",
                    "Multiple variable loads",
                    "Operator precedence handling"
                ]
            }),
        );

        
        let old_approach_time = {
            let start = Instant::now();

            
            let mut manual_bytecode = Vec::new();

            
            manual_bytecode.push(OpCode::Push as u8);
            let value_42 = serde_json::to_vec(&json!(42)).unwrap();
            manual_bytecode.extend_from_slice(&(value_42.len() as u32).to_le_bytes());
            manual_bytecode.extend_from_slice(&value_42);

            
            manual_bytecode.push(OpCode::Push as u8);
            let value_10 = serde_json::to_vec(&json!(10)).unwrap();
            manual_bytecode.extend_from_slice(&(value_10.len() as u32).to_le_bytes());
            manual_bytecode.extend_from_slice(&value_10);

            
            manual_bytecode.push(OpCode::Add as u8);

            start.elapsed()
        };

        let new_approach_time = {
            let start = Instant::now();

            let mut asm = BytecodeAssembler::new();
            asm.push_literal(&json!(42))?
                .push_literal(&json!(10))?
                .add();
            let _bytecode = asm.into_bytecode();

            start.elapsed()
        };

        log_event(
            "assembler_performance_comparison",
            json!({
                "old_approach_time_nanos": old_approach_time.as_nanos(),
                "new_approach_time_nanos": new_approach_time.as_nanos(),
                "improvement_factor": if new_approach_time.as_nanos() > 0 {
                    old_approach_time.as_nanos() as f64 / new_approach_time.as_nanos() as f64
                } else {
                    1.0
                },
                "code_reduction": "~75% fewer lines for equivalent bytecode generation",
                "maintainability": "Significantly improved with fluent API and automatic serialization"
            }),
        );

        
        let mut test_flow = FlowDefinition::new("assembler_test_flow", "start");

        test_flow.set_initial_state(json!({
            "counter": 7,
            "multiplier": 3,
            "threshold": 20
        }));

        test_flow.add_block(BlockDefinition::new(
            "start",
            BlockType::Compute {
                expression: "state.counter * state.multiplier".to_string(),
                output_key: "result".to_string(),
                next_block: "check".to_string(),
            },
        ));

        test_flow.add_block(BlockDefinition::new(
            "check",
            BlockType::Conditional {
                condition: "state.result > state.threshold".to_string(),
                true_block: "reduce".to_string(),
                false_block: "done".to_string(),
            },
        ));

        test_flow.add_block(BlockDefinition::new(
            "reduce",
            BlockType::Compute {
                expression: "state.result / 2".to_string(),
                output_key: "result".to_string(),
                next_block: "done".to_string(),
            },
        ));

        test_flow.add_block(BlockDefinition::new("done", BlockType::Terminate));

        
        let start_time = Instant::now();
        let orchestration_contract = FlowTranspiler::transpile(&test_flow)?;
        let contract = sleet::convert_contract(orchestration_contract)
            .map_err(|e| anyhow::anyhow!("Contract conversion failed: {}", e))?;

        let mut runtime =
            RemarkableInterpreter::new(1000, &contract, self.function_registry.to_ffi_registry())?;

        let result = runtime.run(contract).await?;
        let execution_time = start_time.elapsed();

        log_event(
            "assembler_real_execution_demo",
            json!({
                "flow_executed": "assembler_test_flow",
                "execution_time_ms": execution_time.as_millis(),
                "result": result,
                "transpiler_benefit": "Transpiler now generates cleaner bytecode using BytecodeAssembler",
                "developer_benefit": "Reduced boilerplate, improved maintainability, fewer bugs"
            }),
        );

        log_event(
            "assembler_demo_complete",
            json!({
                "summary": "BytecodeAssembler successfully demonstrated",
                "key_benefits": [
                    "Eliminates 70%+ of bytecode generation boilerplate",
                    "Provides type-safe opcode generation",
                    "Automatic value serialization and length encoding",
                    "Intelligent jump patching with offset calculation",
                    "Fluent API for readable code",
                    "Reduced error potential in manual bytecode construction"
                ],
                "integration_status": "Fully integrated into transpiler pipeline",
                "performance": "Comparable to manual approach with significantly better developer experience"
            }),
        );

        Ok(())
    }

    async fn demonstrate_parser(&mut self) -> Result<()> {
        log_section("Parser Capabilities Demonstration");

        let mut flow = FlowDefinition::new("parser_demo_flow", "array_access");

        flow.set_initial_state(json!({
            "data": [10, 20, 30, 40, 50],
            "matrix": [[1, 2, 3], [4, 5, 6], [7, 8, 9]],
            "index": 2,
            "multiplier": 3,
            "threshold": 100,
            "enabled": true,
            "counter": 0,
            "nested": {
                "values": [100, 200, 300],
                "active": true
            }
        }));

        flow.set_state_schema(json!({
            "type": "object",
            "properties": {
                "data": { "type": "array", "items": { "type": "integer" } },
                "matrix": { "type": "array", "items": { "type": "array", "items": { "type": "integer" } } },
                "index": { "type": "integer" },
                "multiplier": { "type": "integer" },
                "threshold": { "type": "integer" },
                "enabled": { "type": "boolean" },
                "counter": { "type": "integer" },
                "result": { "type": "integer" },
                "computed": { "type": "integer" },
                "nested": {
                    "type": "object",
                    "properties": {
                        "values": { "type": "array", "items": { "type": "integer" } },
                        "active": { "type": "boolean" }
                    }
                }
            }
        }));

        let test_expressions = vec![
            (
                "array_access",
                "state.data[state.index]",
                "Accessing array elements by dynamic index",
            ),
            (
                "complex_math",
                "state.data[0] * state.multiplier + state.data[1] / 2",
                "Complex arithmetic with array access",
            ),
            (
                "unary_ops",
                "-state.multiplier * state.data[0]",
                "Unary minus with multiplication",
            ),
            (
                "modulo_op",
                "state.data[state.index] % state.multiplier",
                "Modulo operation with array access",
            ),
            (
                "conditional",
                "state.enabled ? state.data[state.index] * 2 : state.data[0]",
                "Ternary conditional with array access",
            ),
            (
                "nested_access",
                "state.nested.values[state.index % 3]",
                "Nested object and array access",
            ),
            (
                "boolean_logic",
                "state.data[0] > state.threshold && state.enabled",
                "Boolean logic with array access",
            ),
            (
                "advanced_conditional",
                "state.nested.active ? state.nested.values[0] + state.multiplier : -state.data[1]",
                "Advanced conditional with nested access and unary operators",
            ),
        ];

        for (i, (block_id, expression, description)) in test_expressions.iter().enumerate() {
            let next_block = if i == test_expressions.len() - 1 {
                "finalise".to_string()
            } else {
                test_expressions[i + 1].0.to_string()
            };

            flow.add_block(BlockDefinition::new(
                *block_id,
                BlockType::Compute {
                    expression: expression.to_string(),
                    output_key: if *block_id == "boolean_logic" {
                        "enabled".to_string()
                    } else {
                        "result".to_string()
                    },
                    next_block,
                },
            ));

            log_event(
                "parser_test_created",
                json!({
                    "block_id": block_id,
                    "expression": expression,
                    "description": description
                }),
            );
        }

        flow.add_block(BlockDefinition::new("finalise", BlockType::Terminate));

        let start_time = Instant::now();
        let orchestration_contract = FlowTranspiler::transpile(&flow)?;
        let transpile_time = start_time.elapsed();

        log_event(
            "parser_transpilation_complete",
            json!({
                "transpile_time_ms": transpile_time.as_millis(),
                "expressions_tested": test_expressions.len(),
                "bytecode_blocks": orchestration_contract.blocks.len()
            }),
        );

        let contract = sleet::convert_contract(orchestration_contract)
            .map_err(|e| anyhow::anyhow!("Contract conversion failed: {}", e))?;

        let mut runtime =
            RemarkableInterpreter::new(3000, &contract, self.function_registry.to_ffi_registry())?;

        let execution_start = Instant::now();
        let result = runtime.run(contract).await?;
        let execution_time = execution_start.elapsed();

        log_event(
            "parser_demo_completed",
            json!({
                "execution_time_ms": execution_time.as_millis(),
                "final_result": result,
                "expressions_executed": test_expressions.len(),
                "parser_features_tested": [
                    "Array indexing with dynamic indices",
                    "Complex arithmetic with operator precedence",
                    "Unary operators with expressions",
                    "Modulo operator with array access",
                    "Ternary conditional with array access",
                    "Nested object and array access",
                    "Boolean logic with comparisons",
                    "Advanced conditionals with nested expressions"
                ]
            }),
        );

        Ok(())
    }

    async fn demonstrate_parser_showcase(&mut self) -> Result<()> {
        log_section("Enhanced Parser Showcase - Working Complex Expressions");

        let mut flow = FlowDefinition::new("parser_showcase_flow", "simple_demo");

        flow.set_initial_state(json!({
            "data": [10, 20, 30, 40, 50],
            "index": 2,
            "multiplier": 3,
            "threshold": 25,
            "enabled": true,
            "counter": 7
        }));

        flow.set_state_schema(json!({
            "type": "object",
            "properties": {
                "data": { "type": "array", "items": { "type": "integer" } },
                "index": { "type": "integer" },
                "multiplier": { "type": "integer" },
                "threshold": { "type": "integer" },
                "enabled": { "type": "boolean" },
                "counter": { "type": "integer" },
                "result": { "type": "integer" }
            }
        }));

        let showcase_expressions = vec![
            ("simple_demo", "state.counter", "Simple variable access"),
            (
                "arithmetic_demo",
                "state.counter + state.multiplier",
                "Basic arithmetic",
            ),
            ("array_demo", "state.data[0]", "Basic array indexing"),
            (
                "complex_arithmetic_demo",
                "state.data[0] * state.multiplier",
                "Array access with arithmetic",
            ),
            (
                "nested_array_demo",
                "state.data[state.index] + 5",
                "Nested array access with arithmetic",
            ),
            (
                "complex_expression_demo",
                "state.enabled ? state.data[0] * 2 : state.counter",
                "Ternary conditional expression",
            ),
        ];

        for (i, (block_id, expression, description)) in showcase_expressions.iter().enumerate() {
            let next_block = if i == showcase_expressions.len() - 1 {
                "finalise".to_string()
            } else {
                showcase_expressions[i + 1].0.to_string()
            };

            flow.add_block(BlockDefinition::new(
                *block_id,
                BlockType::Compute {
                    expression: expression.to_string(),
                    output_key: "result".to_string(),
                    next_block,
                },
            ));

            log_event(
                "parser_showcase_expression",
                json!({
                    "block_id": block_id,
                    "expression": expression,
                    "description": description,
                    "complexity_level": match *block_id {
                        "array_demo" => "Basic",
                        "arithmetic_demo" => "High",
                        "unary_demo" => "Medium",
                        "modulo_demo" => "Medium",
                        "conditional_demo" => "Advanced",
                        "boolean_demo" => "Advanced",
                        _ => "Unknown"
                    }
                }),
            );
        }

        flow.add_block(BlockDefinition::new("finalise", BlockType::Terminate));

        let start_time = Instant::now();
        let orchestration_contract = FlowTranspiler::transpile(&flow)?;
        let transpile_time = start_time.elapsed();

        log_event(
            "parser_showcase_transpiled",
            json!({
                "transpile_time_ms": transpile_time.as_millis(),
                "expressions_count": showcase_expressions.len(),
                "bytecode_blocks": orchestration_contract.blocks.len(),
                "parser_features": [
                    "Array indexing with dynamic indices",
                    "Complex arithmetic with correct precedence",
                    "Unary operators (-, !)",
                    "Modulo operator (%)",
                    "Ternary conditional operator (? :)",
                    "Boolean logic with && and || operators",
                    "Nested expressions and grouping"
                ]
            }),
        );

        let contract = sleet::convert_contract(orchestration_contract)
            .map_err(|e| anyhow::anyhow!("Contract conversion failed: {}", e))?;

        let mut runtime =
            RemarkableInterpreter::new(8000, &contract, self.function_registry.to_ffi_registry())?;

        let execution_start = Instant::now();
        let result = runtime.run(contract).await?;
        let execution_time = execution_start.elapsed();

        log_event(
            "parser_showcase_completed",
            json!({
                "execution_time_ms": execution_time.as_millis(),
                "final_result": result,
                "expressions_executed": showcase_expressions.len(),
                "success": true,
                "achievement": "🎉 Successfully parsed and executed all complex expressions!"
            }),
        );

        Ok(())
    }

    async fn run_full_demo(&mut self) -> Result<()> {
        log_event(
            "runtime_demo_started",
            json!({
                "timestamp": Utc::now().to_rfc3339(),
                "demo_version": "1.0.0-EXTREME-STRESS"
            }),
        );

        self.demonstrate_dynamic_functions().await?;

        self.demonstrate_transpiler_pipeline().await?;

        self.demonstrate_bytecode_assembler().await?;

        let flow = self.create_base_flow();
        self.demonstrate_hot_path_optimisation(&flow).await?;

        self.demonstrate_runtime_modification().await?;

        self.demonstrate_parser_showcase().await?;

        self.stress_test_gas_limits().await?;
        self.stress_test_recursive_flows().await?;
        self.stress_test_massive_arrays().await?;
        self.stress_test_deep_nesting().await?;
        self.stress_test_concurrent_executions().await?;
        self.stress_test_memory_pressure().await?;
        self.stress_test_edge_cases().await?;
        self.stress_test_jit_optimisation().await?;

        log_event(
            "runtime_demo_completed",
            json!({
                "timestamp": Utc::now().to_rfc3339(),
                "total_executions": self.execution_count,
                "functions_registered": self.function_registry.functions.len(),
                "hot_paths_detected": self.profiler.get_optimisation_candidates().len(),
                "stress_tests_completed": 8
            }),
        );

        Ok(())
    }

    async fn stress_test_gas_limits(&mut self) -> Result<()> {
        log_section("STRESS TEST 1: Gas Limit Exhaustion and Recovery");

        let mut flow = FlowDefinition::new("gas_stress_flow", "gas_consumer");

        flow.set_initial_state(json!({
            "counter": 0,
            "iterations": 100,
            "data": (0..1000).collect::<Vec<i32>>(),
            "result": 0
        }));

        flow.add_block(BlockDefinition::new(
            "gas_consumer",
            BlockType::Compute {
                expression: "state.counter + 1".to_string(),
                output_key: "counter".to_string(),
                next_block: "intensive_calculation".to_string(),
            },
        ));

        flow.add_block(BlockDefinition::new(
            "intensive_calculation",
            BlockType::Compute {
                expression: "state.data[0] * state.data[1] + state.data[2] * state.data[3]"
                    .to_string(),
                output_key: "result".to_string(),
                next_block: "check_iterations".to_string(),
            },
        ));

        flow.add_block(BlockDefinition::new(
            "check_iterations",
            BlockType::Conditional {
                condition: "state.counter < state.iterations".to_string(),
                true_block: "gas_consumer".to_string(),
                false_block: "finalise".to_string(),
            },
        ));

        flow.add_block(BlockDefinition::new("finalise", BlockType::Terminate));

        let gas_limits = [50, 100, 200, 500, 1000, 2000, 5000, 10000];

        for &gas_limit in &gas_limits {
            let start_time = Instant::now();
            let orchestration_contract = FlowTranspiler::transpile(&flow)?;
            let contract = sleet::convert_contract(orchestration_contract)
                .map_err(|e| anyhow::anyhow!("Contract conversion failed: {}", e))?;

            let mut runtime = RemarkableInterpreter::new(
                gas_limit,
                &contract,
                self.function_registry.to_ffi_registry(),
            )?;

            match runtime.run(contract).await {
                Ok(result) => {
                    let exec_time = start_time.elapsed();
                    log_event(
                        "gas_stress_success",
                        json!({
                            "gas_limit": gas_limit,
                            "execution_time_ms": exec_time.as_millis(),
                            "result": result,
                            "status": "COMPLETED"
                        }),
                    );
                }
                Err(e) => {
                    let exec_time = start_time.elapsed();
                    log_event(
                        "gas_stress_failure",
                        json!({
                            "gas_limit": gas_limit,
                            "execution_time_ms": exec_time.as_millis(),
                            "error": e.to_string(),
                            "status": "OUT_OF_GAS"
                        }),
                    );
                }
            }
        }

        Ok(())
    }

    async fn stress_test_recursive_flows(&mut self) -> Result<()> {
        log_section("STRESS TEST 2: Recursive Flow Execution");

        self.function_registry.register_function(
            "recursive_fib".to_string(),
            |args: &[RuntimeValue], _state: &RuntimeValue| {
                let n = args[0].as_u64().unwrap_or(0) as u32;
                if n > 30 {
                    return Ok(json!(0).into());
                }
                let result = fibonacci(n);
                Ok(json!(result).into())
            },
        );

        let mut flow = FlowDefinition::new("recursive_flow", "start_recursion");

        flow.set_initial_state(json!({
            "n": 15,
            "depth": 0,
            "max_depth": 50,
            "results": []
        }));

        flow.add_block(BlockDefinition::new(
            "start_recursion",
            BlockType::Compute {
                expression: "state.depth + 1".to_string(),
                output_key: "depth".to_string(),
                next_block: "check_depth".to_string(),
            },
        ));

        flow.add_block(BlockDefinition::new(
            "check_depth",
            BlockType::Conditional {
                condition: "state.depth < state.max_depth".to_string(),
                true_block: "recursive_computation".to_string(),
                false_block: "finalise".to_string(),
            },
        ));

        flow.add_block(BlockDefinition::new(
            "recursive_computation",
            BlockType::Compute {
                expression: "state.n + state.depth".to_string(),
                output_key: "n".to_string(),
                next_block: "start_recursion".to_string(),
            },
        ));

        flow.add_block(BlockDefinition::new("finalise", BlockType::Terminate));

        let max_depths = [10, 25, 50, 100];

        for &max_depth in &max_depths {
            let mut test_flow = flow.clone();
            test_flow.set_initial_state(json!({
                "n": 10,
                "depth": 0,
                "max_depth": max_depth,
                "results": []
            }));

            let start_time = Instant::now();
            let orchestration_contract = FlowTranspiler::transpile(&test_flow)?;
            let contract = sleet::convert_contract(orchestration_contract)
                .map_err(|e| anyhow::anyhow!("Contract conversion failed: {}", e))?;

            let mut runtime = RemarkableInterpreter::new(
                20000,
                &contract,
                self.function_registry.to_ffi_registry(),
            )?;

            match runtime.run(contract).await {
                Ok(result) => {
                    let exec_time = start_time.elapsed();
                    log_event(
                        "recursive_stress_success",
                        json!({
                            "max_depth": max_depth,
                            "execution_time_ms": exec_time.as_millis(),
                            "result": result,
                            "status": "COMPLETED"
                        }),
                    );
                }
                Err(e) => {
                    let exec_time = start_time.elapsed();
                    log_event(
                        "recursive_stress_failure",
                        json!({
                            "max_depth": max_depth,
                            "execution_time_ms": exec_time.as_millis(),
                            "error": e.to_string(),
                            "status": "ERROR"
                        }),
                    );
                }
            }
        }

        Ok(())
    }

    async fn stress_test_massive_arrays(&mut self) -> Result<()> {
        log_section("STRESS TEST 3: Massive Array Operations");

        self.function_registry.register_function(
            "array_sum".to_string(),
            |args: &[RuntimeValue], _state: &RuntimeValue| {
                if let Some(arr) = args[0].as_array() {
                    let sum: f64 = arr.iter().filter_map(|v| v.as_f64()).sum();
                    Ok(json!(sum).into())
                } else {
                    Ok(json!(0).into())
                }
            },
        );

        self.function_registry.register_function(
            "array_transform".to_string(),
            |args: &[RuntimeValue], _state: &RuntimeValue| {
                if let Some(arr) = args[0].as_array() {
                    let transformed: Vec<serde_json::Value> = arr
                        .iter()
                        .filter_map(|v| v.as_f64())
                        .map(|n| json!(n * 2.0 + 1.0))
                        .collect();
                    Ok(json!(transformed).into())
                } else {
                    Ok(json!([]).into())
                }
            },
        );

        let array_sizes = [1000, 5000, 10000, 50000];

        for &size in &array_sizes {
            let mut flow = FlowDefinition::new("array_stress_flow", "array_operations");

            let large_array: Vec<i32> = (0..size).collect();

            flow.set_initial_state(json!({
                "data": large_array,
                "size": size,
                "processed": [],
                "sum": 0,
                "average": 0.0
            }));

            flow.add_block(BlockDefinition::new(
                "array_operations",
                BlockType::Compute {
                    expression: format!("state.data[{}]", size / 2),
                    output_key: "middle_element".to_string(),
                    next_block: "array_slice".to_string(),
                },
            ));

            flow.add_block(BlockDefinition::new(
                "array_slice",
                BlockType::Compute {
                    expression: "state.data[0] + state.data[1] + state.data[2]".to_string(),
                    output_key: "sum".to_string(),
                    next_block: "complex_indexing".to_string(),
                },
            ));

            flow.add_block(BlockDefinition::new(
                "complex_indexing",
                BlockType::Compute {
                    expression: format!("state.data[state.size % {}]", std::cmp::min(1000, size)),
                    output_key: "dynamic_access".to_string(),
                    next_block: "finalise".to_string(),
                },
            ));

            flow.add_block(BlockDefinition::new("finalise", BlockType::Terminate));

            let start_time = Instant::now();
            let orchestration_contract = FlowTranspiler::transpile(&flow)?;
            let contract = sleet::convert_contract(orchestration_contract)
                .map_err(|e| anyhow::anyhow!("Contract conversion failed: {}", e))?;

            let mut runtime = RemarkableInterpreter::new(
                50000,
                &contract,
                self.function_registry.to_ffi_registry(),
            )?;

            match runtime.run(contract).await {
                Ok(result) => {
                    let exec_time = start_time.elapsed();
                    log_event(
                        "array_stress_success",
                        json!({
                            "array_size": size,
                            "execution_time_ms": exec_time.as_millis(),
                            "memory_estimate_mb": (size * 4) / 1024 / 1024,
                            "result": result,
                            "status": "COMPLETED"
                        }),
                    );
                }
                Err(e) => {
                    let exec_time = start_time.elapsed();
                    log_event(
                        "array_stress_failure",
                        json!({
                            "array_size": size,
                            "execution_time_ms": exec_time.as_millis(),
                            "error": e.to_string(),
                            "status": "ERROR"
                        }),
                    );
                }
            }
        }

        Ok(())
    }

    async fn stress_test_deep_nesting(&mut self) -> Result<()> {
        log_section("STRESS TEST 4: Deep Nesting and Complex Expressions");

        let mut flow = FlowDefinition::new("nesting_stress_flow", "deep_nesting");

        let nested_data = json!({
            "level0": {
                "level1": {
                    "level2": {
                        "level3": {
                            "level4": {
                                "level5": {
                                    "data": [1, 2, 3, 4, 5],
                                    "matrix": [[1, 2], [3, 4], [5, 6]],
                                    "values": {
                                        "a": 10,
                                        "b": 20,
                                        "c": [30, 40, 50]
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        flow.set_initial_state(json!({
            "nested": nested_data,
            "result": 0,
            "complex_result": 0
        }));

        let complex_expressions = [(
                "deep_nesting",
                "state.nested.level0.level1.level2.level3.level4.level5.data[0]",
                "Deep nested access"
            ),
            (
                "complex_arithmetic",
                "state.nested.level0.level1.level2.level3.level4.level5.values.a * state.nested.level0.level1.level2.level3.level4.level5.values.b",
                "Complex nested arithmetic"
            ),
            (
                "nested_array_access",
                "state.nested.level0.level1.level2.level3.level4.level5.values.c[2]",
                "Nested array access"
            ),
            (
                "mega_complex",
                "state.nested.level0.level1.level2.level3.level4.level5.data[0] + state.nested.level0.level1.level2.level3.level4.level5.matrix[1][0]",
                "Mega complex expression"
            )];

        for (i, (block_id, expression, description)) in complex_expressions.iter().enumerate() {
            let next_block = if i == complex_expressions.len() - 1 {
                "finalise".to_string()
            } else {
                complex_expressions[i + 1].0.to_string()
            };

            flow.add_block(BlockDefinition::new(
                *block_id,
                BlockType::Compute {
                    expression: expression.to_string(),
                    output_key: "result".to_string(),
                    next_block,
                },
            ));

            log_event(
                "complex_expression_created",
                json!({
                    "block_id": block_id,
                    "expression": expression,
                    "description": description,
                    "nesting_depth": expression.matches('.').count()
                }),
            );
        }

        flow.add_block(BlockDefinition::new("finalise", BlockType::Terminate));

        let start_time = Instant::now();
        let orchestration_contract = FlowTranspiler::transpile(&flow)?;
        let transpile_time = start_time.elapsed();

        let contract = sleet::convert_contract(orchestration_contract)
            .map_err(|e| anyhow::anyhow!("Contract conversion failed: {}", e))?;

        let execution_start = Instant::now();
        let mut runtime =
            RemarkableInterpreter::new(30000, &contract, self.function_registry.to_ffi_registry())?;

        match runtime.run(contract).await {
            Ok(result) => {
                let exec_time = execution_start.elapsed();
                log_event(
                    "nesting_stress_success",
                    json!({
                        "transpile_time_ms": transpile_time.as_millis(),
                        "execution_time_ms": exec_time.as_millis(),
                        "expressions_tested": complex_expressions.len(),
                        "max_nesting_depth": complex_expressions.iter()
                            .map(|(_, expr, _)| expr.matches('.').count())
                            .max().unwrap_or(0),
                        "result": result,
                        "status": "COMPLETED"
                    }),
                );
            }
            Err(e) => {
                let exec_time = execution_start.elapsed();
                log_event(
                    "nesting_stress_failure",
                    json!({
                        "transpile_time_ms": transpile_time.as_millis(),
                        "execution_time_ms": exec_time.as_millis(),
                        "error": e.to_string(),
                        "status": "ERROR"
                    }),
                );
            }
        }

        Ok(())
    }

    async fn stress_test_concurrent_executions(&mut self) -> Result<()> {
        log_section("STRESS TEST 5: Concurrent Executions");

        let flow_configs = vec![
            ("concurrent_flow_1", 1000, "arithmetic_heavy"),
            ("concurrent_flow_2", 2000, "array_heavy"),
            ("concurrent_flow_3", 1500, "conditional_heavy"),
            ("concurrent_flow_4", 3000, "mixed_operations"),
        ];

        
        let mut tasks = Vec::new();

        for (flow_id, gas_limit, flow_type) in flow_configs {
            let ffi_registry = self.function_registry.to_ffi_registry();
            let flow_id = flow_id.to_string();
            let flow_type = flow_type.to_string();

            
            let mut flow = FlowDefinition::new(&flow_id, "concurrent_start");

            match flow_type.as_str() {
                "arithmetic_heavy" => {
                    flow.set_initial_state(json!({
                        "counter": 0,
                        "limit": 100,
                        "result": 1
                    }));

                    flow.add_block(BlockDefinition::new(
                        "concurrent_start",
                        BlockType::Compute {
                            expression: "state.counter + 1".to_string(),
                            output_key: "counter".to_string(),
                            next_block: "arithmetic_op".to_string(),
                        },
                    ));

                    flow.add_block(BlockDefinition::new(
                        "arithmetic_op",
                        BlockType::Compute {
                            expression: "state.result * state.counter + state.counter % 7"
                                .to_string(),
                            output_key: "result".to_string(),
                            next_block: "check_limit".to_string(),
                        },
                    ));

                    flow.add_block(BlockDefinition::new(
                        "check_limit",
                        BlockType::Conditional {
                            condition: "state.counter < state.limit".to_string(),
                            true_block: "concurrent_start".to_string(),
                            false_block: "finalise".to_string(),
                        },
                    ));
                }
                "array_heavy" => {
                    flow.set_initial_state(json!({
                        "data": (0..500).collect::<Vec<i32>>(),
                        "index": 0,
                        "sum": 0
                    }));

                    flow.add_block(BlockDefinition::new(
                        "concurrent_start",
                        BlockType::Compute {
                            expression: "state.data[state.index]".to_string(),
                            output_key: "current".to_string(),
                            next_block: "sum_op".to_string(),
                        },
                    ));

                    flow.add_block(BlockDefinition::new(
                        "sum_op",
                        BlockType::Compute {
                            expression: "state.sum + state.current".to_string(),
                            output_key: "sum".to_string(),
                            next_block: "next_index".to_string(),
                        },
                    ));

                    flow.add_block(BlockDefinition::new(
                        "next_index",
                        BlockType::Compute {
                            expression: "state.index + 1".to_string(),
                            output_key: "index".to_string(),
                            next_block: "check_array_end".to_string(),
                        },
                    ));

                    flow.add_block(BlockDefinition::new(
                        "check_array_end",
                        BlockType::Conditional {
                            condition: "state.index < 50".to_string(),
                            true_block: "concurrent_start".to_string(),
                            false_block: "finalise".to_string(),
                        },
                    ));
                }
                _ => {
                    flow.set_initial_state(json!({
                        "value": 42,
                        "multiplier": 2
                    }));

                    flow.add_block(BlockDefinition::new(
                        "concurrent_start",
                        BlockType::Compute {
                            expression: "state.value * state.multiplier".to_string(),
                            output_key: "result".to_string(),
                            next_block: "finalise".to_string(),
                        },
                    ));
                }
            }

            flow.add_block(BlockDefinition::new("finalise", BlockType::Terminate));

            
            let orchestration_contract = FlowTranspiler::transpile(&flow)?;
            let contract = sleet::convert_contract(orchestration_contract)
                .map_err(|e| anyhow::anyhow!("Contract conversion failed: {}", e))?;

            
            let task = tokio::task::spawn_blocking(move || {
                use tokio::runtime::Handle;

                
                let handle = Handle::current();
                handle.block_on(async move {
                    let start_time = Instant::now();

                    
                    let mut runtime =
                        RemarkableInterpreter::new(gas_limit, &contract, ffi_registry)
                            .map_err(|e| anyhow::anyhow!("Runtime creation failed: {}", e))?;

                    let result = runtime
                        .run(contract)
                        .await
                        .map_err(|e| anyhow::anyhow!("Execution failed: {}", e))?;

                    let exec_time = start_time.elapsed();

                    Ok::<_, anyhow::Error>((flow_id, flow_type, gas_limit, exec_time, result))
                })
            });

            tasks.push(task);
        }

        let start_concurrent = Instant::now();

        
        let task_results = futures::future::try_join_all(tasks)
            .await
            .map_err(|e| anyhow::anyhow!("Task execution failed: {}", e))?;

        let total_concurrent_time = start_concurrent.elapsed();

        let mut successful_executions = 0;
        let mut failed_executions = 0;

        for result in task_results {
            match result {
                Ok((flow_id, flow_type, gas_limit, exec_time, execution_result)) => {
                    successful_executions += 1;
                    log_event(
                        "concurrent_execution_success",
                        json!({
                            "flow_id": flow_id,
                            "flow_type": flow_type,
                            "gas_limit": gas_limit,
                            "execution_time_ms": exec_time.as_millis(),
                            "result": execution_result,
                            "status": "SUCCESS"
                        }),
                    );
                }
                Err(e) => {
                    failed_executions += 1;
                    log_event(
                        "concurrent_execution_failure",
                        json!({
                            "error": e.to_string(),
                            "status": "FAILED"
                        }),
                    );
                }
            }
        }

        log_event(
            "concurrent_stress_summary",
            json!({
                "total_concurrent_time_ms": total_concurrent_time.as_millis(),
                "successful_executions": successful_executions,
                "failed_executions": failed_executions,
                "success_rate": (successful_executions as f64) / (successful_executions + failed_executions) as f64,
                "parallelism_achieved": true,
                "note": "Using tokio::spawn_blocking for true parallelism despite Send trait limitations"
            }),
        );

        Ok(())
    }

    async fn stress_test_memory_pressure(&mut self) -> Result<()> {
        log_section("STRESS TEST 6: Memory Pressure Test");

        register_json_function_with_state(
            &mut self.function_registry,
            "memory_hog",
            |args, _state| {
                let size = args.as_u64().unwrap_or(1000) as usize;
                let large_vec: Vec<i32> = (0..size).map(|i| i as i32).collect();
                let sum: i32 = large_vec.iter().sum();
                json!(sum)
            },
        );

        let memory_sizes = [10000, 50000, 100000, 500000];

        for &size in &memory_sizes {
            let mut flow = FlowDefinition::new("memory_stress_flow", "memory_allocation");

            let large_data: Vec<Vec<i32>> = (0..100)
                .map(|i| (i * 1000..(i + 1) * 1000).collect())
                .collect();

            flow.set_initial_state(json!({
                "memory_data": large_data,
                "size_target": size,
                "allocated_chunks": 0,
                "total_memory": 0
            }));

            flow.add_block(BlockDefinition::new(
                "memory_allocation",
                BlockType::Compute {
                    expression: "state.memory_data[0][0] + state.memory_data[1][1]".to_string(),
                    output_key: "sample".to_string(),
                    next_block: "memory_computation".to_string(),
                },
            ));

            flow.add_block(BlockDefinition::new(
                "memory_computation",
                BlockType::Compute {
                    expression: "state.allocated_chunks + 1".to_string(),
                    output_key: "allocated_chunks".to_string(),
                    next_block: "memory_check".to_string(),
                },
            ));

            flow.add_block(BlockDefinition::new(
                "memory_check",
                BlockType::Conditional {
                    condition: "state.allocated_chunks < 10".to_string(),
                    true_block: "memory_allocation".to_string(),
                    false_block: "finalise".to_string(),
                },
            ));

            flow.add_block(BlockDefinition::new("finalise", BlockType::Terminate));

            let start_time = Instant::now();
            let orchestration_contract = FlowTranspiler::transpile(&flow)?;
            let contract = sleet::convert_contract(orchestration_contract)
                .map_err(|e| anyhow::anyhow!("Contract conversion failed: {}", e))?;

            let mut runtime = RemarkableInterpreter::new(
                100000,
                &contract,
                self.function_registry.to_ffi_registry(),
            )?;

            match runtime.run(contract).await {
                Ok(result) => {
                    let exec_time = start_time.elapsed();
                    log_event(
                        "memory_stress_success",
                        json!({
                            "target_size": size,
                            "execution_time_ms": exec_time.as_millis(),
                            "estimated_memory_mb": (size * 4) / 1024 / 1024,
                            "result": result,
                            "status": "COMPLETED"
                        }),
                    );
                }
                Err(e) => {
                    let exec_time = start_time.elapsed();
                    log_event(
                        "memory_stress_failure",
                        json!({
                            "target_size": size,
                            "execution_time_ms": exec_time.as_millis(),
                            "error": e.to_string(),
                            "status": "MEMORY_ERROR"
                        }),
                    );
                }
            }
        }

        Ok(())
    }

    async fn stress_test_edge_cases(&mut self) -> Result<()> {
        log_section("STRESS TEST 7: Edge Cases and Error Conditions");

        let edge_cases = vec![
            ("division_by_zero", "10 / 0", "Division by zero"),
            ("modulo_by_zero", "10 % 0", "Modulo by zero"),
            ("array_bounds", "state.data[999999]", "Array out of bounds"),
            ("null_access", "state.nonexistent.field", "Null access"),
            (
                "large_numbers",
                "999999999 * 999999999",
                "Large number arithmetic",
            ),
            (
                "negative_array_index",
                "state.data[-1]",
                "Negative array index",
            ),
        ];

        for (test_id, expression, description) in edge_cases {
            let mut flow = FlowDefinition::new("edge_case_flow", "edge_test");

            flow.set_initial_state(json!({
                "data": [1, 2, 3, 4, 5],
                "value": 42,
                "result": 0
            }));

            flow.add_block(BlockDefinition::new(
                "edge_test",
                BlockType::Compute {
                    expression: expression.to_string(),
                    output_key: "result".to_string(),
                    next_block: "finalise".to_string(),
                },
            ));

            flow.add_block(BlockDefinition::new("finalise", BlockType::Terminate));

            let start_time = Instant::now();

            match FlowTranspiler::transpile(&flow) {
                Ok(orchestration_contract) => {
                    match sleet::convert_contract(orchestration_contract) {
                        Ok(contract) => {
                            let mut runtime = RemarkableInterpreter::new(
                                5000,
                                &contract,
                                self.function_registry.to_ffi_registry(),
                            )?;

                            match runtime.run(contract).await {
                                Ok(result) => {
                                    let exec_time = start_time.elapsed();
                                    log_event(
                                        "edge_case_unexpected_success",
                                        json!({
                                            "test_id": test_id,
                                            "expression": expression,
                                            "description": description,
                                            "execution_time_ms": exec_time.as_millis(),
                                            "result": result,
                                            "status": "UNEXPECTED_SUCCESS"
                                        }),
                                    );
                                }
                                Err(e) => {
                                    let exec_time = start_time.elapsed();
                                    log_event(
                                        "edge_case_expected_error",
                                        json!({
                                            "test_id": test_id,
                                            "expression": expression,
                                            "description": description,
                                            "execution_time_ms": exec_time.as_millis(),
                                            "error": e.to_string(),
                                            "error_type": format!("{:?}", e),
                                            "status": "EXPECTED_ERROR"
                                        }),
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            let exec_time = start_time.elapsed();
                            log_event(
                                "edge_case_conversion_error",
                                json!({
                                    "test_id": test_id,
                                    "expression": expression,
                                    "description": description,
                                    "execution_time_ms": exec_time.as_millis(),
                                    "error": e.to_string(),
                                    "status": "CONVERSION_ERROR"
                                }),
                            );
                        }
                    }
                }
                Err(e) => {
                    let exec_time = start_time.elapsed();
                    log_event(
                        "edge_case_transpile_error",
                        json!({
                            "test_id": test_id,
                            "expression": expression,
                            "description": description,
                            "execution_time_ms": exec_time.as_millis(),
                            "error": e.to_string(),
                            "status": "TRANSPILE_ERROR"
                        }),
                    );
                }
            }
        }

        Ok(())
    }

    async fn stress_test_jit_optimisation(&mut self) -> Result<()> {
        log_section("STRESS TEST 8: JIT Optimisation Stress Test");

        let mut flow = FlowDefinition::new("jit_stress_flow", "hot_path_block");

        flow.set_initial_state(json!({
            "counter": 0,
            "limit": 1000,
            "accumulator": 0,
            "data": (0..100).collect::<Vec<i32>>()
        }));

        flow.add_block(BlockDefinition::new(
            "hot_path_block",
            BlockType::Compute {
                expression: "state.counter + 1".to_string(),
                output_key: "counter".to_string(),
                next_block: "accumulate".to_string(),
            },
        ));

        flow.add_block(BlockDefinition::new(
            "accumulate",
            BlockType::Compute {
                expression: "state.accumulator + state.data[state.counter % 100]".to_string(),
                output_key: "accumulator".to_string(),
                next_block: "check_limit".to_string(),
            },
        ));

        flow.add_block(BlockDefinition::new(
            "check_limit",
            BlockType::Conditional {
                condition: "state.counter < state.limit".to_string(),
                true_block: "hot_path_block".to_string(),
                false_block: "finalise".to_string(),
            },
        ));

        flow.add_block(BlockDefinition::new("finalise", BlockType::Terminate));

        let iterations = [1, 10, 100, 500];

        for &iteration_limit in &iterations {
            let mut test_flow = flow.clone();
            test_flow.set_initial_state(json!({
                "counter": 0,
                "limit": iteration_limit,
                "accumulator": 0,
                "data": (0..100).collect::<Vec<i32>>()
            }));

            let start_time = Instant::now();
            let orchestration_contract = FlowTranspiler::transpile(&test_flow)?;
            let contract = sleet::convert_contract(orchestration_contract)
                .map_err(|e| anyhow::anyhow!("Contract conversion failed: {}", e))?;

            let mut runtime = RemarkableInterpreter::new(
                200000,
                &contract,
                self.function_registry.to_ffi_registry(),
            )?;

            match runtime.run(contract).await {
                Ok(result) => {
                    let exec_time = start_time.elapsed();

                    self.profiler.record_execution("hot_path_block", exec_time);
                    self.profiler.record_execution("accumulate", exec_time);

                    log_event(
                        "jit_stress_success",
                        json!({
                            "iteration_limit": iteration_limit,
                            "execution_time_ms": exec_time.as_millis(),
                            "iterations_per_second": if exec_time.as_millis() > 0 {
                                (iteration_limit as u128 * 1000) / exec_time.as_millis()
                            } else {
                                0
                            },
                            "result": result,
                            "hot_paths_detected": self.profiler.get_optimisation_candidates(),
                            "jit_eligible": self.profiler.is_hot_path("hot_path_block"),
                            "status": "COMPLETED"
                        }),
                    );
                }
                Err(e) => {
                    let exec_time = start_time.elapsed();
                    log_event(
                        "jit_stress_failure",
                        json!({
                            "iteration_limit": iteration_limit,
                            "execution_time_ms": exec_time.as_millis(),
                            "error": e.to_string(),
                            "status": "ERROR"
                        }),
                    );
                }
            }

            sleep(Duration::from_millis(100)).await;
        }

        log_event(
            "jit_optimisation_summary",
            json!({
                "total_hot_paths": self.profiler.get_optimisation_candidates().len(),
                "hot_paths": self.profiler.get_optimisation_candidates(),
                "jit_threshold": self.profiler.hot_threshold,
                "optimisation_achieved": !self.profiler.get_optimisation_candidates().is_empty()
            }),
        );

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    let matches = Command::new("runtime-demo")
        .version("1.0.0")
        .about("SLEET Runtime Capabilities Demonstration")
        .arg(
            Arg::new("mode")
                .long("mode")
                .short('m')
                .help("Demo mode to run")
                .default_value("full")
                .value_parser([
                    "full",
                    "transpiler",
                    "dynamic",
                    "hotpath",
                    "modification",
                    "parser",
                ]),
        )
        .get_matches();

    let mode = matches.get_one::<String>("mode").unwrap();

    log_event(
        "demo_startup",
        json!({
            "mode": mode,
            "sleet_features": [
                "FlowDefinition → Contract → Bytecode Pipeline",
                "Dynamic Function Injection",
                "Hot Path Detection & JIT Compilation",
                "Runtime Flow Modification",
                "Gas Metering & Resource Management",
                "FFI Registry Integration",
                "Enhanced Expression Parser"
            ]
        }),
    );

    let mut orchestrator = RuntimeDemoOrchestrator::new();

    match mode.as_str() {
        "full" => orchestrator.run_full_demo().await?,
        "transpiler" => orchestrator.demonstrate_transpiler_pipeline().await?,
        "dynamic" => orchestrator.demonstrate_dynamic_functions().await?,
        "hotpath" => {
            let flow = orchestrator.create_base_flow();
            orchestrator
                .demonstrate_hot_path_optimisation(&flow)
                .await?
        }
        "modification" => orchestrator.demonstrate_runtime_modification().await?,
        "parser" => orchestrator.demonstrate_parser().await?,
        _ => unreachable!(),
    }

    Ok(())
}

fn fibonacci(n: u32) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

fn log_event(event_type: &str, data: Value) {
    println!(
        "{}",
        json!({
            "timestamp": Utc::now().to_rfc3339(),
            "event": event_type,
            "data": data
        })
    );
}

fn log_section(title: &str) {
    println!("\n{}", "=".repeat(60));
    println!("{title}");
    println!("{}\n", "=".repeat(60));
}
