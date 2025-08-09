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
use sleet::runtime::{FfiRegistry, OpCode, RemarkableInterpreter};
use std::time::Instant;

fn create_simple_arithmetic_contract() -> Contract {
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

    add_push_u64(&mut bytecode, 10);
    add_push_u64(&mut bytecode, 5);
    bytecode.push(OpCode::Add as u8);
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
async fn test_jit_compilation_with_reused_interpreter() {
    let contract = create_simple_arithmetic_contract();

    println!("\n--- JIT Compilation Test ---");

    
    for i in 1..=10 {
        
        let mut interpreter =
            RemarkableInterpreter::new(1_000_000, &contract, FfiRegistry::new()).unwrap();

        let start_time = Instant::now();
        let result = interpreter.run(contract.clone()).await;
        let duration = start_time.elapsed();

        println!("Iteration {i}: {duration:?} -> {result:?}");
        assert!(result.is_ok(), "Interpreter run failed");
    }

    println!("--- JIT Test Complete ---");
}
