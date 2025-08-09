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
use sleet::runtime::OpCode;

pub fn build_comparison_test() -> Vec<u8> {
    let mut bytecode = Vec::new();
    bytecode.push(OpCode::Push as u8);
    let val_15 = serde_json::to_vec(&json!(15)).unwrap();
    bytecode.extend_from_slice(&(val_15.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_15);
    bytecode.push(OpCode::Push as u8);
    let val_15_again = serde_json::to_vec(&json!(15)).unwrap();
    bytecode.extend_from_slice(&(val_15_again.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_15_again);
    bytecode.push(OpCode::GreaterEqual as u8);
    bytecode.push(OpCode::Push as u8);
    let val_10 = serde_json::to_vec(&json!(10)).unwrap();
    bytecode.extend_from_slice(&(val_10.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_10);
    bytecode.push(OpCode::Push as u8);
    let val_15_again2 = serde_json::to_vec(&json!(15)).unwrap();
    bytecode.extend_from_slice(&(val_15_again2.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_15_again2);
    bytecode.push(OpCode::LessEqual as u8);
    bytecode.push(OpCode::And as u8);
    bytecode.push(OpCode::Halt as u8);
    bytecode
}

pub fn build_stack_test() -> Vec<u8> {
    let mut bytecode = Vec::new();
    bytecode.push(OpCode::Push as u8);
    let val_42 = serde_json::to_vec(&json!(42)).unwrap();
    bytecode.extend_from_slice(&(val_42.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_42);
    bytecode.push(OpCode::Push as u8);
    let val_99 = serde_json::to_vec(&json!(99)).unwrap();
    bytecode.extend_from_slice(&(val_99.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_99);
    bytecode.push(OpCode::Dup as u8);
    bytecode.push(OpCode::Swap as u8);
    bytecode.push(OpCode::Pop as u8);
    bytecode.push(OpCode::Halt as u8);
    bytecode
}

pub fn build_logical_test() -> Vec<u8> {
    let mut bytecode = Vec::new();
    bytecode.push(OpCode::Push as u8);
    let val_false = serde_json::to_vec(&json!(false)).unwrap();
    bytecode.extend_from_slice(&(val_false.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_false);
    bytecode.push(OpCode::Push as u8);
    let val_true = serde_json::to_vec(&json!(true)).unwrap();
    bytecode.extend_from_slice(&(val_true.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_true);
    bytecode.push(OpCode::Or as u8);
    bytecode.push(OpCode::Halt as u8);
    bytecode
}

pub fn build_jump_if_true_test() -> Vec<u8> {
    let mut bytecode = Vec::new();
    bytecode.push(OpCode::Push as u8);
    let val_true = serde_json::to_vec(&json!(true)).unwrap();
    bytecode.extend_from_slice(&(val_true.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&val_true);
    bytecode.push(OpCode::JumpIfTrue as u8);
    let jump_target = 50u32;
    bytecode.extend_from_slice(&jump_target.to_le_bytes());
    bytecode.push(OpCode::Push as u8);
    let fail_msg = serde_json::to_vec(&json!("FAILED: Should not execute")).unwrap();
    bytecode.extend_from_slice(&(fail_msg.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&fail_msg);
    while bytecode.len() < 50 {
        bytecode.push(0);
    }
    bytecode.push(OpCode::Push as u8);
    let success_msg = serde_json::to_vec(&json!("SUCCESS: JumpIfTrue worked")).unwrap();
    bytecode.extend_from_slice(&(success_msg.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&success_msg);
    bytecode.push(OpCode::Halt as u8);
    bytecode
}



pub fn build_hybrid_micro_benchmark() -> Vec<u8> {
    let mut bytecode = Vec::new();

    
    bytecode.push(OpCode::Push as u8);
    let zero = serde_json::to_vec(&json!(0)).unwrap();
    bytecode.extend_from_slice(&(zero.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&zero);

    
    let loop_start = bytecode.len() as u32;

    
    bytecode.push(OpCode::Push as u8);
    let one = serde_json::to_vec(&json!(1)).unwrap();
    bytecode.extend_from_slice(&(one.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&one);
    bytecode.push(OpCode::Add as u8);

    
    bytecode.push(OpCode::Push as u8);
    let hundred = serde_json::to_vec(&json!(100)).unwrap();
    bytecode.extend_from_slice(&(hundred.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&hundred);
    bytecode.push(OpCode::LessThan as u8);

    
    bytecode.push(OpCode::JumpIfTrue as u8);
    bytecode.extend_from_slice(&loop_start.to_le_bytes());

    
    bytecode.push(OpCode::Push as u8);
    let done_msg =
        serde_json::to_vec(&json!(" Hybrid micro-benchmark complete: counter=100")).unwrap();
    bytecode.extend_from_slice(&(done_msg.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(&done_msg);

    bytecode.push(OpCode::CallFfi as u8);
    let print_name = "print";
    let print_name_bytes = print_name.as_bytes();
    bytecode.extend_from_slice(&(print_name_bytes.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(print_name_bytes);
    bytecode.push(1);

    bytecode
}
