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

use serde_json::json;
use sleet::ast::{Contract, Op};
use sleet::runtime::{
    FfiFunction, FfiRegistry, InterpreterError, OpCode, RemarkableInterpreter, Value,
};
use std::sync::Arc;
use std::time::Instant;


fn fibonacci_ffi(args: &[Value], _permissions: &Value) -> Result<Value, InterpreterError> {
    let n = args
        .first()
        .and_then(|v| v.as_i64())
        .ok_or_else(|| InterpreterError::TypeMismatch {
            expected: "i64".to_string(),
            found: "other".to_string(),
        })? as u64;

    fn fib(n: u64) -> u64 {
        match n {
            0 => 0,
            1 => 1,
            _ => fib(n - 1) + fib(n - 2),
        }
    }

    Ok(Value::Integer(fib(n) as i64))
}

fn create_test_contract(fib_n: u64) -> Contract {
    let contract_json = json!({
        "version": "1.0",
        "start_block_id": "start",
        "initial_state": {
            "op": {
                "Literal": {
                    "JsonValue": { "result": 0 }
                }
            },
            "metadata": {}
        },
        "permissions": { "ffi": ["calculate_fib"] },
        "participants": [],
        "blocks": {
            "start": {
                "op": {
                    "Evaluate": {
                        "output_path": [{ "Key": "result" }],
                        "bytecode": []
                    }
                },
                "metadata": {}
            }
        }
    });

    
    let mut contract: Contract = serde_json::from_value(contract_json).unwrap();
    if let Some(block) = contract.blocks.get_mut("start") {
        if let Op::Evaluate { bytecode, .. } = &mut block.op {
            let n_val = json!(fib_n);
            let n_bytes = serde_json::to_vec(&n_val).unwrap();
            let n_len = n_bytes.len() as u32;

            
            let mut new_bytecode = Vec::new();
            new_bytecode.push(OpCode::Push as u8);
            new_bytecode.extend_from_slice(&n_len.to_le_bytes());
            new_bytecode.extend_from_slice(&n_bytes);
            new_bytecode.push(OpCode::CallFfi as u8);
            let name = "calculate_fib";
            let name_len = name.len() as u32;
            new_bytecode.extend_from_slice(&name_len.to_le_bytes());
            new_bytecode.extend_from_slice(name.as_bytes());
            new_bytecode.push(1); 
            new_bytecode.push(OpCode::Halt as u8);

            *bytecode = new_bytecode;
        }
    }

    contract
}

#[tokio::test]
async fn benchmark_sleet_interpreter_with_ffi() {
    const ITERATIONS: u32 = 10;
    const FIB_NUMBER: u64 = 20; 

    
    let mut ffi_registry = FfiRegistry::new();
    ffi_registry.insert(
        "calculate_fib".to_string(),
        Arc::new(fibonacci_ffi) as FfiFunction,
    );

    
    let contract = create_test_contract(FIB_NUMBER);

    
    let mut total_duration = std::time::Duration::new(0, 0);

    for i in 0..ITERATIONS {
        let mut interpreter =
            RemarkableInterpreter::new(1_000_000, &contract, ffi_registry.clone()).unwrap();

        let start_time = Instant::now();
        let result = interpreter.run(contract.clone()).await;
        let duration = start_time.elapsed();

        total_duration += duration;

        println!("Iteration {}: {:?} -> {:?}", i + 1, duration, result);
        assert!(result.is_ok(), "Interpreter run failed");
    }

    let avg_duration = total_duration / ITERATIONS;
    println!("\n--- Sleet Performance Benchmark ---");
    println!("Fibonacci Number: {FIB_NUMBER}");
    println!("Iterations: {ITERATIONS}");
    println!("Average Execution Time: {avg_duration:?}");
    println!("---------------------------------");

    
    
    assert!(
        avg_duration.as_millis() < 50,
        "Execution is slower than expected baseline."
    );
}

fn create_fib_in_bytecode_contract() -> Contract {
    let contract_json = json!({
        "version": "1.0",
        "start_block_id": "start",
        "initial_state": {
            "op": { "Literal": { "JsonValue": { "result": 0 } } },
            "metadata": {}
        },
        "permissions": {},
        "participants": [],
        "blocks": {
            "start": {
                "op": {
                    "Evaluate": {
                        "output_path": [{ "Key": "result" }],
                        "bytecode": [] 
                    }
                },
                "metadata": {}
            }
        }
    });
    let mut contract: Contract = serde_json::from_value(contract_json).unwrap();

    
    let mut bytecode = Vec::new();
    let add_push_u64 = |bc: &mut Vec<u8>, val: u64| {
        let n_val = json!(val);
        let n_bytes = serde_json::to_vec(&n_val).unwrap();
        let n_len = n_bytes.len() as u32;
        bc.push(OpCode::Push as u8);
        bc.extend_from_slice(&n_len.to_le_bytes());
        bc.extend_from_slice(&n_bytes);
    };

    
    for i in 0..1000 {
        
        add_push_u64(&mut bytecode, 2);
        add_push_u64(&mut bytecode, 3);
        bytecode.push(OpCode::Add as u8);
        add_push_u64(&mut bytecode, 4);
        bytecode.push(OpCode::Multiply as u8);
        add_push_u64(&mut bytecode, 1);
        bytecode.push(OpCode::Subtract as u8);
        if i < 999 {
            
            bytecode.push(OpCode::Pop as u8);
        }
    }

    bytecode.push(OpCode::Halt as u8);

    if let Some(block) = contract.blocks.get_mut("start") {
        if let Op::Evaluate {
            bytecode: block_bytecode,
            ..
        } = &mut block.op
        {
            *block_bytecode = bytecode;
        }
    }

    contract
}

#[tokio::test]

async fn benchmark_sleet_interpreter_with_bytecode() {
    const ITERATIONS: u32 = 5;
    const FIB_NUMBER: u64 = 15; 

    
    let contract = create_fib_in_bytecode_contract(); 
    println!("\n--- Pure Bytecode Benchmark (Expect SLOW) ---");
    let mut total_duration = std::time::Duration::new(0, 0);

    for i in 0..ITERATIONS {
        let mut interpreter =
            RemarkableInterpreter::new(10_000_000, &contract, FfiRegistry::new()).unwrap();

        let start_time = Instant::now();
        let result = interpreter.run(contract.clone()).await;
        let duration = start_time.elapsed();

        total_duration += duration;
        println!("Iteration {}: {:?} -> {:?}", i + 1, duration, result);
        assert!(result.is_ok(), "Interpreter run failed");
    }

    let avg_duration = total_duration / ITERATIONS;
    println!("\n--- Sleet Pure Bytecode Performance ---");
    println!("Fibonacci Number: {FIB_NUMBER}");
    println!("Iterations: {ITERATIONS}");
    println!("Average Execution Time: {avg_duration:?}");
    println!("---------------------------------------");
}
