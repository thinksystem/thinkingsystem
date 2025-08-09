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

use sleet::runtime::*;
use std::io::{self, Write};

fn main() -> anyhow::Result<()> {
    println!("Hybrid FFI + JIT Demo - FFI via interpreter, compute via JIT when possible");
    println!("================================================================\n");

    
    let ffi_registry = setup_ffi_registry();
    println!(
        "FFI registry setup complete with {} functions",
        ffi_registry.len()
    );

    
    print!("Enter an integer value: ");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let user_value: i64 = input.trim().parse().expect("Please enter a valid integer");

    
    println!("Building bytecode with FFI calls...");
    let bytecode = create_ffi_bytecode(user_value);
    println!("Bytecode generation complete ({} bytes)", bytecode.len());

    
    print_planned_operations();

    
    println!("Bytecode hex dump:");
    for (i, chunk) in bytecode.chunks(16).enumerate() {
        print!("{:04x}: ", i * 16);
        for byte in chunk {
            print!("{byte:02x} ");
        }
        println!();
    }

    
    let mut vm = VM::new(1_000_000)?;

    println!("Executing bytecode (hybrid: JIT for compute-only segments, interpreter for FFI)");

    
    match vm.execute_with_ffi(&bytecode, &ffi_registry) {
        Ok(()) => {
            println!("\nDemo completed successfully.");
            if let Some(top) = vm.stack().last() {
                println!("Final result on stack: {top}");
            } else {
                println!("Stack is empty");
            }
            println!("Profiler stats: {:?}", vm.profiler_stats());
        }
        Err(e) => {
            println!("Execution failed: {e}");
        }
    }

    Ok(())
}

fn setup_ffi_registry() -> FfiRegistry {
    let mut registry = FfiRegistry::new();

    
    registry.insert(
        "double_value".to_string(),
        std::sync::Arc::new(|args, _ctx| {
            if let Some(Value::Integer(n)) = args.first() {
                println!("FFI call: double_value({}) -> {}", n, n * 2);
                Ok(Value::Integer(n * 2))
            } else {
                Err(InterpreterError::RuntimeError(
                    "double_value expects an integer".into(),
                ))
            }
        }),
    );

    registry.insert(
        "add_ten".to_string(),
        std::sync::Arc::new(|args, _ctx| {
            if let Some(Value::Integer(n)) = args.first() {
                println!("FFI call: add_ten({}) -> {}", n, n + 10);
                Ok(Value::Integer(n + 10))
            } else {
                Err(InterpreterError::RuntimeError(
                    "add_ten expects an integer".into(),
                ))
            }
        }),
    );

    registry.insert(
        "square".to_string(),
        std::sync::Arc::new(|args, _ctx| {
            if let Some(Value::Integer(n)) = args.first() {
                println!("FFI call: square({}) -> {}", n, n * n);
                Ok(Value::Integer(n * n))
            } else {
                Err(InterpreterError::RuntimeError(
                    "square expects an integer".into(),
                ))
            }
        }),
    );

    registry.insert(
        "subtract_five".to_string(),
        std::sync::Arc::new(|args, _ctx| {
            if let Some(Value::Integer(n)) = args.first() {
                println!("FFI call: subtract_five({}) -> {}", n, n - 5);
                Ok(Value::Integer(n - 5))
            } else {
                Err(InterpreterError::RuntimeError(
                    "subtract_five expects an integer".into(),
                ))
            }
        }),
    );

    registry.insert(
        "divide_by_two".to_string(),
        std::sync::Arc::new(|args, _ctx| {
            if let Some(Value::Integer(n)) = args.first() {
                println!("FFI call: divide_by_two({}) -> {}", n, n / 2);
                Ok(Value::Integer(n / 2))
            } else {
                Err(InterpreterError::RuntimeError(
                    "divide_by_two expects an integer".into(),
                ))
            }
        }),
    );

    registry
}

fn print_planned_operations() {
    println!("Planned operations sequence:");
    println!("  1. push initial value");
    println!("  2. FFI: double_value(x)");
    println!("  3. compute: +3");
    println!("  4. FFI: add_ten(x)");
    println!("  5. compute: *2");
    println!("  6. FFI: square(x)");
    println!("  7. compute: -1");
    println!("  8. FFI: subtract_five(x)");
    println!("  9. compute: +7");
    println!(" 10. FFI: divide_by_two(x)");
}

fn create_ffi_bytecode(initial_value: i64) -> Vec<u8> {
    let mut bytecode = Vec::new();

    
    

    
    bytecode.push(OpCode::Push as u8);
    bytecode.extend_from_slice(&(initial_value as i32).to_le_bytes());

    
    bytecode.push(OpCode::CallFfi as u8);
    let func_name = "double_value";
    bytecode.extend_from_slice(&(func_name.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(func_name.as_bytes());
    bytecode.push(1u8); 

    
    bytecode.push(OpCode::Push as u8);
    bytecode.extend_from_slice(&3i32.to_le_bytes());
    bytecode.push(OpCode::Add as u8);

    
    bytecode.push(OpCode::CallFfi as u8);
    let func_name = "add_ten";
    bytecode.extend_from_slice(&(func_name.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(func_name.as_bytes());
    bytecode.push(1u8); 

    
    bytecode.push(OpCode::Push as u8);
    bytecode.extend_from_slice(&2i32.to_le_bytes());
    bytecode.push(OpCode::Multiply as u8);

    
    bytecode.push(OpCode::CallFfi as u8);
    let func_name = "square";
    bytecode.extend_from_slice(&(func_name.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(func_name.as_bytes());
    bytecode.push(1u8); 

    
    bytecode.push(OpCode::Push as u8);
    bytecode.extend_from_slice(&1i32.to_le_bytes());
    bytecode.push(OpCode::Subtract as u8);

    
    bytecode.push(OpCode::CallFfi as u8);
    let func_name = "subtract_five";
    bytecode.extend_from_slice(&(func_name.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(func_name.as_bytes());
    bytecode.push(1u8); 

    
    bytecode.push(OpCode::Push as u8);
    bytecode.extend_from_slice(&7i32.to_le_bytes());
    bytecode.push(OpCode::Add as u8);

    
    bytecode.push(OpCode::CallFfi as u8);
    let func_name = "divide_by_two";
    bytecode.extend_from_slice(&(func_name.len() as u32).to_le_bytes());
    bytecode.extend_from_slice(func_name.as_bytes());
    bytecode.push(1u8); 

    
    bytecode.push(OpCode::Halt as u8);

    bytecode
}
