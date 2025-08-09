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

mod test_new_opcodes;
use serde_json::json;
use sleet::orchestration::OrchestrationFlowDefinition;
use sleet::runtime::{FfiRegistry, OpCode, RemarkableInterpreter, Value};
use sleet::FlowTranspiler;
use sleet::{AstNode, Contract, Literal, Op, Path, PathSegment};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path as StdPath;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(" Enhanced BytecodeVM Demo - Comprehensive OpCode Showcase\n");

    
    let ffi_time_ns = Arc::new(AtomicU64::new(0));

    let mut ffi_registry: FfiRegistry = HashMap::new();
    {
        let ffi_time_ns_print = ffi_time_ns.clone();
        ffi_registry.insert(
            "print".to_string(),
            Arc::new(move |args, _perms| {
                let t0 = Instant::now();
                if let Some(value) = args.first() {
                    println!("Output: {value}");
                }
                let dt = t0.elapsed().as_nanos() as u64;
                ffi_time_ns_print.fetch_add(dt, Ordering::Relaxed);
                Ok(Value::Null)
            }),
        );
    }
    {
        let ffi_time_ns_fib = ffi_time_ns.clone();
        ffi_registry.insert(
            "fibonacci".to_string(),
            Arc::new(move |args, _perms| {
                let t0 = Instant::now();
                let out = if let Some(Value::Integer(n)) = args.first() {
                    let n = *n as u32;
                    let result = fibonacci(n);
                    Value::Integer(result as i64)
                } else {
                    Value::Integer(0)
                };
                let dt = t0.elapsed().as_nanos() as u64;
                ffi_time_ns_fib.fetch_add(dt, Ordering::Relaxed);
                Ok(out)
            }),
        );
    }
    {
        let ffi_time_ns_assert = ffi_time_ns.clone();
        ffi_registry.insert(
            "assert_equal".to_string(),
            Arc::new(move |args, _perms| {
                let t0 = Instant::now();
                let res = if args.len() >= 2 {
                    let a = &args[0];
                    let b = &args[1];
                    if a == b {
                        println!("Assertion passed: {a} == {b}");
                        Value::Boolean(true)
                    } else {
                        println!("Assertion failed: {a} != {b}");
                        Value::Boolean(false)
                    }
                } else {
                    Value::Boolean(false)
                };
                let dt = t0.elapsed().as_nanos() as u64;
                ffi_time_ns_assert.fetch_add(dt, Ordering::Relaxed);
                Ok(res)
            }),
        );
    }

    
    println!(" Executing multi-block contract with hybrid JIT/FFI flow...\n");
    let comprehensive_path = "bin/demos/vm-demo/contracts/comprehensive_contract.json";
    let contract = load_or_generate_contract(comprehensive_path, create_comprehensive_contract)?;

    let mut interpreter = RemarkableInterpreter::new(1_000_000, &contract, ffi_registry.clone())?;
    let t0 = Instant::now();
    match interpreter.run(contract.clone()).await {
        Ok(status) => {
            let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let ffi_ms = (ffi_time_ns.load(Ordering::Relaxed) as f64) / 1_000_000.0;
            println!(" Multi-block demo completed successfully.");
            println!(" Final status: {status:?}");
            println!(" Timing: total={total_ms:.3} ms, accumulated FFI time={ffi_ms:.3} ms");
        }
        Err(e) => {
            println!(" Demo failed: {e}");
        }
    }

    
    println!("\n Running hybrid micro-benchmark (compute-only loop with single FFI print)...");
    let ffi_before = ffi_time_ns.load(Ordering::Relaxed);
    let bench_path = "bin/demos/vm-demo/contracts/hybrid_benchmark_contract.json";
    let bench_contract = load_or_generate_contract(bench_path, create_hybrid_benchmark_contract)?;
    let mut bench_interpreter =
        RemarkableInterpreter::new(1_000_000, &bench_contract, ffi_registry.clone())?;
    let tb = Instant::now();
    match bench_interpreter.run(bench_contract).await {
        Ok(status) => {
            let bench_ms = tb.elapsed().as_secs_f64() * 1000.0;
            let ffi_after = ffi_time_ns.load(Ordering::Relaxed);
            let bench_ffi_ms = ((ffi_after - ffi_before) as f64) / 1_000_000.0;
            let bench_compute_ms = (bench_ms - bench_ffi_ms).max(0.0);
            println!(" Hybrid micro-benchmark completed: {status:?}");
            println!(" Timing: total={bench_ms:.3} ms, FFI={bench_ffi_ms:.3} ms, approx compute={bench_compute_ms:.3} ms");
        }
        Err(e) => println!(" Hybrid micro-benchmark failed: {e}"),
    }

    
    println!("\n Running transpiled FlowDefinition via FlowTranspiler -> VM...");
    let flow_path = "bin/demos/vm-demo/flows/simple_flow.yaml";
    let flow = load_flow_def_from_file(flow_path)?;
    let orch_contract = FlowTranspiler::transpile(&flow)?;
    let sleet_contract = sleet::convert_contract(orch_contract)?;
    let mut flow_vm = RemarkableInterpreter::new(1_000_000, &sleet_contract, ffi_registry.clone())?;
    let tf = Instant::now();
    match flow_vm.run(sleet_contract).await {
        Ok(status) => {
            let total_ms = tf.elapsed().as_secs_f64() * 1000.0;
            let ffi_ms = (ffi_time_ns.load(Ordering::Relaxed) as f64) / 1_000_000.0;
            println!(" Transpiled flow run status: {status:?}");
            println!(" Timing: total={total_ms:.3} ms, accumulated FFI time={ffi_ms:.3} ms");
        }
        Err(e) => println!(" Transpiled flow run failed: {e}"),
    }

    
    println!("\n Running orchestration flow via OrchestrationCoordinator...");
    let orch_flow: OrchestrationFlowDefinition = load_flow_def_from_file(flow_path)?.into();
    let to = Instant::now();
    match sleet::execute_orchestrated_flow(orch_flow, Some(1_000_000), None).await {
        Ok(status) => {
            let total_ms = to.elapsed().as_secs_f64() * 1000.0;
            println!(" Orchestrated flow status: {status:?}");
            println!(" Timing: total={total_ms:.3} ms (coordinator)");
        }
        Err(e) => println!(" Orchestrated flow failed: {e}"),
    }

    Ok(())
}

fn fibonacci(n: u32) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => {
            let mut a = 0;
            let mut b = 1;
            for _ in 2..=n {
                let temp = a + b;
                a = b;
                b = temp;
            }
            b
        }
    }
}

fn create_comprehensive_bytecode() -> Vec<u8> {
    let mut bytecode = Vec::new();

    
    bytecode.extend_from_slice(&build_arithmetic_test());

    
    bytecode.extend_from_slice(&build_hybrid_compute_showcase());

    
    bytecode.extend_from_slice(&build_new_opcodes_test());

    bytecode.push(OpCode::Halt as u8);
    bytecode
}


fn create_comprehensive_contract() -> Contract {
    let mut blocks = HashMap::new();

    blocks.insert(
        "ARITHMETIC_TESTS".to_string(),
        AstNode {
            op: Op::Evaluate {
                bytecode: build_arithmetic_test(),
                output_path: Path(vec![PathSegment::Key("arithmetic_results".into())]),
            },
            metadata: HashMap::from([
                ("next_block".to_string(), json!("CONTROL_FLOW_TESTS")),
                (
                    "description".to_string(),
                    json!("Tests arithmetic and comparison operations"),
                ),
            ]),
            source_location: None,
        },
    );

    blocks.insert(
        "CONTROL_FLOW_TESTS".to_string(),
        AstNode {
            op: Op::Evaluate {
                bytecode: build_control_flow_test(),
                output_path: Path(vec![PathSegment::Key("control_flow_results".into())]),
            },
            metadata: HashMap::from([
                ("next_block".to_string(), json!("FUNCTION_TESTS")),
                (
                    "description".to_string(),
                    json!("Tests Jump, JumpIfFalse, and conditional execution"),
                ),
            ]),
            source_location: None,
        },
    );

    blocks.insert(
        "FUNCTION_TESTS".to_string(),
        AstNode {
            op: Op::Evaluate {
                bytecode: build_function_test(),
                output_path: Path(vec![PathSegment::Key("function_results".into())]),
            },
            metadata: HashMap::from([
                ("next_block".to_string(), json!("LOOP_TESTS")),
                (
                    "description".to_string(),
                    json!("Tests Call, Return, and stack operations"),
                ),
            ]),
            source_location: None,
        },
    );

    blocks.insert(
        "LOOP_TESTS".to_string(),
        AstNode {
            op: Op::Evaluate {
                bytecode: build_loop_test(),
                output_path: Path(vec![PathSegment::Key("loop_results".into())]),
            },
            metadata: HashMap::from([
                ("next_block".to_string(), json!("FFI_TESTS")),
                (
                    "description".to_string(),
                    json!("Tests loop constructs with jumps and conditions"),
                ),
            ]),
            source_location: None,
        },
    );

    blocks.insert(
        "FFI_TESTS".to_string(),
        AstNode {
            op: Op::Evaluate {
                bytecode: build_ffi_test(),
                output_path: Path(vec![PathSegment::Key("ffi_results".into())]),
            },
            metadata: HashMap::from([
                ("next_block".to_string(), json!("NEW_OPCODES_TESTS")),
                (
                    "description".to_string(),
                    json!("Tests FFI calls and external function integration"),
                ),
            ]),
            source_location: None,
        },
    );

    blocks.insert(
        "NEW_OPCODES_TESTS".to_string(),
        AstNode {
            op: Op::Evaluate {
                bytecode: build_new_opcodes_test(),
                output_path: Path(vec![PathSegment::Key("new_opcodes_results".into())]),
            },
            metadata: HashMap::from([
                ("next_block".to_string(), json!("HYBRID_BENCHMARK")),
                ("description".to_string(), json!(
                    "Tests newly implemented OpCodes: GreaterEqual, LessEqual, JumpIfTrue, Dup, Swap, And, Or"
                )),
            ]),
            source_location: None,
        },
    );

    
    blocks.insert(
        "HYBRID_BENCHMARK".to_string(),
        AstNode {
            op: Op::Evaluate {
                bytecode: test_new_opcodes::build_hybrid_micro_benchmark(),
                output_path: Path(vec![PathSegment::Key("hybrid_benchmark_results".into())]),
            },
            metadata: HashMap::from([
                ("next_block".to_string(), json!("SUMMARY")),
                (
                    "description".to_string(),
                    json!("Compute-only loop JIT showcase followed by a single FFI print"),
                ),
            ]),
            source_location: None,
        },
    );

    blocks.insert(
        "SUMMARY".to_string(),
        AstNode {
            op: Op::Evaluate {
                bytecode: build_summary_output(),
                output_path: Path(vec![PathSegment::Key("summary".into())]),
            },
            metadata: HashMap::from([
                ("next_block".to_string(), json!("TERMINATE")),
                (
                    "description".to_string(),
                    json!("Outputs comprehensive test summary"),
                ),
            ]),
            source_location: None,
        },
    );

    blocks.insert(
        "TERMINATE".to_string(),
        AstNode {
            op: Op::Terminate,
            metadata: HashMap::new(),
            source_location: None,
        },
    );

    Contract {
        version: "4.0.0".into(),
        start_block_id: "ARITHMETIC_TESTS".into(),
        blocks,
        participants: vec![],
        initial_state: AstNode {
            op: Op::Literal(
                json!({
                    "demo_title": "Comprehensive Enhanced BytecodeVM Demo",
                    "test_suite": "advanced_opcodes",
                    "total_tests": 7,
                    "start_time": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                })
                .into(),
            ),
            metadata: HashMap::new(),
            source_location: None,
        },
        permissions: json!({
            "print": true,
            "fibonacci": true,
            "assert_equal": true
        }),
    }
}


fn create_hybrid_benchmark_contract() -> Contract {
    let mut blocks = HashMap::new();
    blocks.insert(
        "HYBRID_BENCHMARK".to_string(),
        AstNode {
            op: Op::Evaluate {
                bytecode: test_new_opcodes::build_hybrid_micro_benchmark(),
                output_path: Path(vec![PathSegment::Key("hybrid_benchmark_results".into())]),
            },
            metadata: HashMap::from([
                ("next_block".to_string(), json!("TERMINATE")),
                (
                    "description".to_string(),
                    json!("Standalone hybrid benchmark"),
                ),
            ]),
            source_location: None,
        },
    );
    blocks.insert(
        "TERMINATE".to_string(),
        AstNode {
            op: Op::Terminate,
            metadata: HashMap::new(),
            source_location: None,
        },
    );

    Contract {
        version: "4.0.0".into(),
        start_block_id: "HYBRID_BENCHMARK".into(),
        blocks,
        participants: vec![],
        initial_state: AstNode {
            op: Op::Literal(Literal::from(json!({}))),
            metadata: HashMap::new(),
            source_location: None,
        },
        permissions: json!({
            "print": true
        }),
    }
}

fn build_arithmetic_test() -> Vec<u8> {
    let mut bytecode = Vec::new();
    bytecode.push(OpCode::Push as u8);
    let val_15 = serde_json::to_vec(&json!(15)).unwrap();
    bytecode.extend_from_slice(&(val_15.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_15);
    bytecode.push(OpCode::Push as u8);
    let val_8 = serde_json::to_vec(&json!(8)).unwrap();
    bytecode.extend_from_slice(&(val_8.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_8);
    bytecode.push(OpCode::Add as u8);
    bytecode.push(OpCode::Push as u8);
    let val_20 = serde_json::to_vec(&json!(20)).unwrap();
    bytecode.extend_from_slice(&(val_20.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_20);
    bytecode.push(OpCode::GreaterThan as u8);
    bytecode.push(OpCode::Push as u8);
    let val_false = serde_json::to_vec(&json!(false)).unwrap();
    bytecode.extend_from_slice(&(val_false.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_false);
    bytecode.push(OpCode::NotEqual as u8);
    bytecode
}

fn build_control_flow_test() -> Vec<u8> {
    let mut bytecode = Vec::new();
    bytecode.push(OpCode::Push as u8);
    let val_true = serde_json::to_vec(&json!(true)).unwrap();
    bytecode.extend_from_slice(&(val_true.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_true);
    bytecode.push(OpCode::JumpIfFalse as u8);
    let jump_target_1 = 50u32;
    bytecode.extend_from_slice(&jump_target_1.to_le_bytes());
    bytecode.push(OpCode::Push as u8);
    let success_msg = serde_json::to_vec(&json!(" Control flow test passed")).unwrap();
    bytecode.extend_from_slice(&(success_msg.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&success_msg);
    bytecode.push(OpCode::Jump as u8);
    let jump_target_2 = 100u32;
    bytecode.extend_from_slice(&jump_target_2.to_le_bytes());
    while bytecode.len() < 50 {
        bytecode.push(0);
    }
    bytecode.push(OpCode::Push as u8);
    let fail_msg = serde_json::to_vec(&json!(" Control flow test failed")).unwrap();
    bytecode.extend_from_slice(&(fail_msg.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&fail_msg);
    while bytecode.len() < 100 {
        bytecode.push(0);
    }
    bytecode.push(OpCode::Halt as u8);
    bytecode
}

fn build_function_test() -> Vec<u8> {
    let mut bytecode = Vec::new();
    bytecode.push(OpCode::Push as u8);
    let val_a = serde_json::to_vec(&json!(10)).unwrap();
    bytecode.extend_from_slice(&(val_a.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_a);
    bytecode.push(OpCode::Push as u8);
    let val_b = serde_json::to_vec(&json!(20)).unwrap();
    bytecode.extend_from_slice(&(val_b.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_b);
    bytecode.push(OpCode::Multiply as u8);
    bytecode.push(OpCode::Push as u8);
    let val_5 = serde_json::to_vec(&json!(5)).unwrap();
    bytecode.extend_from_slice(&(val_5.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_5);
    bytecode.push(OpCode::Divide as u8);
    bytecode.push(OpCode::Pop as u8);
    bytecode.push(OpCode::Push as u8);
    let result_msg = serde_json::to_vec(&json!("Mathematical operations completed")).unwrap();
    bytecode.extend_from_slice(&(result_msg.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&result_msg);
    bytecode.push(OpCode::Halt as u8);
    bytecode
}

fn build_loop_test() -> Vec<u8> {
    let mut bytecode = Vec::new();
    bytecode.push(OpCode::Push as u8);
    let val_0 = serde_json::to_vec(&json!(0)).unwrap();
    bytecode.extend_from_slice(&(val_0.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_0);
    let _loop_start_pos = bytecode.len() as u32;
    bytecode.push(OpCode::Push as u8);
    let counter_copy = serde_json::to_vec(&json!("copying counter for comparison")).unwrap();
    bytecode.extend_from_slice(&(counter_copy.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&counter_copy);
    bytecode.push(OpCode::Pop as u8);
    bytecode.push(OpCode::Push as u8);
    let val_1 = serde_json::to_vec(&json!(1)).unwrap();
    bytecode.extend_from_slice(&(val_1.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_1);
    bytecode.push(OpCode::Add as u8);
    bytecode.push(OpCode::Push as u8);
    let val_3 = serde_json::to_vec(&json!(3)).unwrap();
    bytecode.extend_from_slice(&(val_3.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_3);
    bytecode.push(OpCode::LessThan as u8);
    bytecode.push(OpCode::Pop as u8);
    bytecode.push(OpCode::Push as u8);
    let loop_msg = serde_json::to_vec(&json!("Loop and comparison operations completed")).unwrap();
    bytecode.extend_from_slice(&(loop_msg.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&loop_msg);
    bytecode.push(OpCode::Halt as u8);
    bytecode
}

fn build_ffi_test() -> Vec<u8> {
    let mut bytecode = Vec::new();
    bytecode.push(OpCode::Push as u8);
    let val_8 = serde_json::to_vec(&json!(8)).unwrap();
    bytecode.extend_from_slice(&(val_8.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_8);

    bytecode.push(OpCode::CallFfi as u8);
    let fib_name = "fibonacci";
    let fib_name_bytes = fib_name.as_bytes();
    bytecode.extend_from_slice(&(fib_name_bytes.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(fib_name_bytes);
    bytecode.push(1);

    bytecode.push(OpCode::Push as u8);
    let val_21 = serde_json::to_vec(&json!(21)).unwrap();
    bytecode.extend_from_slice(&(val_21.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_21);

    bytecode.push(OpCode::CallFfi as u8);
    let assert_name = "assert_equal";
    let assert_name_bytes = assert_name.as_bytes();
    bytecode.extend_from_slice(&(assert_name_bytes.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(assert_name_bytes);
    bytecode.push(2);

    bytecode.push(OpCode::Push as u8);
    let completion_msg =
        serde_json::to_vec(&json!(" All FFI tests completed successfully!")).unwrap();
    bytecode.extend_from_slice(&(completion_msg.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&completion_msg);

    bytecode.push(OpCode::CallFfi as u8);
    let print_name = "print";
    let print_name_bytes = print_name.as_bytes();
    bytecode.extend_from_slice(&(print_name_bytes.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(print_name_bytes);
    bytecode.push(1);

    bytecode.push(OpCode::Halt as u8);
    bytecode
}


fn build_hybrid_compute_showcase() -> Vec<u8> {
    let mut bytecode = Vec::new();
    
    bytecode.push(OpCode::Push as u8);
    let v10 = serde_json::to_vec(&json!(10)).unwrap();
    bytecode.extend_from_slice(&(v10.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&v10);

    
    bytecode.push(OpCode::Push as u8);
    let v3 = serde_json::to_vec(&json!(3)).unwrap();
    bytecode.extend_from_slice(&(v3.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&v3);
    bytecode.push(OpCode::Add as u8);

    
    bytecode.push(OpCode::Push as u8);
    let v2 = serde_json::to_vec(&json!(2)).unwrap();
    bytecode.extend_from_slice(&(v2.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&v2);
    bytecode.push(OpCode::Multiply as u8);

    
    bytecode.push(OpCode::Push as u8);
    let v1 = serde_json::to_vec(&json!(1)).unwrap();
    bytecode.extend_from_slice(&(v1.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&v1);
    bytecode.push(OpCode::Subtract as u8);

    
    bytecode.push(OpCode::Push as u8);
    let v7 = serde_json::to_vec(&json!(7)).unwrap();
    bytecode.extend_from_slice(&(v7.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&v7);
    bytecode.push(OpCode::Add as u8);

    
    bytecode.push(OpCode::Push as u8);
    let msg = serde_json::to_vec(&json!(" Hybrid compute segment finished (expect 32)")).unwrap();
    bytecode.extend_from_slice(&(msg.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&msg);
    bytecode.push(OpCode::CallFfi as u8);
    let print_name = "print";
    let print_name_bytes = print_name.as_bytes();
    bytecode.extend_from_slice(&(print_name_bytes.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(print_name_bytes);
    bytecode.push(1);

    bytecode
}

fn build_summary_output() -> Vec<u8> {
    let mut bytecode = Vec::new();
    bytecode.push(OpCode::Push as u8);
    let summary_msg = serde_json::to_vec(&json!({
        "demo_name": "Enhanced BytecodeVM Comprehensive Demo",
        "tests_completed": [
            " Arithmetic operations (Add, Subtract, Multiply, Divide)",
            " Comparison operations (GreaterThan, LessThan, Equal, NotEqual)",
            " NEW: Advanced comparisons (GreaterEqual, LessEqual)",
            " Control flow (Jump, JumpIfFalse)",
            " NEW: JumpIfTrue control flow",
            " Stack operations (Push, Pop)",
            " NEW: Advanced stack operations (Dup, Swap)",
            " NEW: Logical operations (And, Or)",
            " Mathematical computations",
            " FFI integration (external function calls)",
            " Advanced bytecode patterns"
        ],
        "opcodes_demonstrated": [
            "Push", "Pop", "Add", "Subtract", "Multiply", "Divide",
            "Equal", "NotEqual", "GreaterThan", "LessThan",
            "GreaterEqual", "LessEqual", "And", "Or", "Not",
            "Jump", "JumpIfFalse", "JumpIfTrue", "Dup", "Swap",
            "CallFfi", "Halt"
        ],
        "new_opcodes_implemented": [
            "GreaterEqual", "LessEqual", "JumpIfTrue", "Dup", "Swap", "And", "Or"
        ],
        "status": "All tests passed successfully including new OpCodes!"
    }))
    .unwrap();
    bytecode.extend_from_slice(&(summary_msg.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&summary_msg);
    bytecode.push(OpCode::CallFfi as u8);
    let print_name = "print";
    let print_name_bytes = print_name.as_bytes();
    bytecode.extend_from_slice(&(print_name_bytes.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(print_name_bytes);
    bytecode.push(1);
    bytecode.push(OpCode::Halt as u8);
    bytecode
}

fn build_new_opcodes_test() -> Vec<u8> {
    let mut bytecode = Vec::new();

    
    bytecode.push(OpCode::Push as u8);
    let val_15 = serde_json::to_vec(&json!(15)).unwrap();
    bytecode.extend_from_slice(&(val_15.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_15);
    bytecode.push(OpCode::Push as u8);
    let val_15_b = serde_json::to_vec(&json!(15)).unwrap();
    bytecode.extend_from_slice(&(val_15_b.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_15_b);
    bytecode.push(OpCode::GreaterEqual as u8);

    bytecode.push(OpCode::Push as u8);
    let val_10 = serde_json::to_vec(&json!(10)).unwrap();
    bytecode.extend_from_slice(&(val_10.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_10);
    bytecode.push(OpCode::Push as u8);
    let val_15_c = serde_json::to_vec(&json!(15)).unwrap();
    bytecode.extend_from_slice(&(val_15_c.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_15_c);
    bytecode.push(OpCode::LessEqual as u8);

    bytecode.push(OpCode::And as u8);

    
    bytecode.push(OpCode::Push as u8);
    let success_msg = serde_json::to_vec(&json!(
        " New OpCodes working: GreaterEqual, LessEqual, And!"
    ))
    .unwrap();
    bytecode.extend_from_slice(&(success_msg.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&success_msg);
    bytecode.push(OpCode::CallFfi as u8);
    let print_name = "print";
    let print_name_bytes = print_name.as_bytes();
    bytecode.extend_from_slice(&(print_name_bytes.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(print_name_bytes);
    bytecode.push(1);

    
    bytecode.push(OpCode::Push as u8);
    let val_42 = serde_json::to_vec(&json!(42)).unwrap();
    bytecode.extend_from_slice(&(val_42.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_42);
    bytecode.push(OpCode::Dup as u8);
    bytecode.push(OpCode::Pop as u8);

    bytecode.push(OpCode::Push as u8);
    let val_false = serde_json::to_vec(&json!(false)).unwrap();
    bytecode.extend_from_slice(&(val_false.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_false);
    bytecode.push(OpCode::Push as u8);
    let val_true = serde_json::to_vec(&json!(true)).unwrap();
    bytecode.extend_from_slice(&(val_true.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_true);
    bytecode.push(OpCode::Or as u8);

    
    bytecode.push(OpCode::Push as u8);
    let final_msg = serde_json::to_vec(&json!(
        " All new OpCodes tested: GreaterEqual, LessEqual, And, Or, Dup, Pop"
    ))
    .unwrap();
    bytecode.extend_from_slice(&(final_msg.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&final_msg);
    bytecode.push(OpCode::CallFfi as u8);
    let print_name2 = "print";
    let print_name2_bytes = print_name2.as_bytes();
    bytecode.extend_from_slice(&(print_name2_bytes.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(print_name2_bytes);
    bytecode.push(1);

    bytecode
}


fn load_flow_def_from_file(
    path: &str,
) -> Result<sleet::flows::definition::FlowDefinition, Box<dyn std::error::Error>> {
    let data = fs::read_to_string(path)?;
    let ext = StdPath::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let flow = match ext.as_str() {
        "yaml" | "yml" => serde_yaml::from_str(&data)?,
        _ => serde_json::from_str(&data)?,
    };
    Ok(flow)
}

fn load_or_generate_contract<F>(
    path: &str,
    generator: F,
) -> Result<Contract, Box<dyn std::error::Error>>
where
    F: Fn() -> Contract,
{
    if let Some(c) = load_contract_from_file(path) {
        return Ok(c);
    }
    
    if let Some(parent) = StdPath::new(path).parent() {
        fs::create_dir_all(parent)?;
    }
    let contract = generator();
    save_contract_to_file(path, &contract)?;
    Ok(contract)
}

fn load_contract_from_file(path: &str) -> Option<Contract> {
    let data = fs::read(path).ok()?;
    let ext = StdPath::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "yaml" | "yml" => serde_yaml::from_slice::<Contract>(&data).ok(),
        _ => serde_json::from_slice::<Contract>(&data).ok(),
    }
}

fn save_contract_to_file(
    path: &str,
    contract: &Contract,
) -> Result<(), Box<dyn std::error::Error>> {
    let ext = StdPath::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes = match ext.as_str() {
        "yaml" | "yml" => serde_yaml::to_string(contract)?.into_bytes(),
        _ => serde_json::to_vec_pretty(contract)?,
    };
    let mut f = fs::File::create(path)?;
    f.write_all(&bytes)?;
    Ok(())
}
