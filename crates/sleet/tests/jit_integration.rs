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
use sleet::runtime::{OpCode, Value, VM};

#[test]
fn test_basic_vm_operations() -> Result<()> {
    println!("Testing basic VM operations...");
    let mut vm = VM::new(1000)?;

    
    let bytecode = vec![
        OpCode::Push as u8,
        10,
        0,
        0,
        0, 
        OpCode::Push as u8,
        20,
        0,
        0,
        0,                  
        OpCode::Add as u8,  
        OpCode::Halt as u8, 
    ];

    vm.execute(&bytecode)?;

    
    let stack = vm.stack();
    println!("Stack length: {}, contents: {:?}", stack.len(), stack);
    assert!(!stack.is_empty(), "Stack should not be empty");

    
    match &stack[stack.len() - 1] {
        Value::Integer(val) => {
            println!("Result: {val}");
            assert_eq!(*val, 30);
        },
        _ => panic!("Expected integer value"),
    }

    Ok(())
}

#[test]
fn test_jit_compilation() -> Result<()> {
    println!("Testing JIT compilation...");

    
    let bytecode = vec![
        OpCode::Push as u8,
        5,
        0,
        0,
        0, 
        OpCode::Push as u8,
        3,
        0,
        0,
        0,                      
        OpCode::Multiply as u8, 
        OpCode::Halt as u8,     
    ];

    
    for i in 0..3 {
        let mut vm = VM::new(1000)?;
        vm.set_jit_threshold(1); 
        vm.execute(&bytecode)?;

        let stack = vm.stack();
        println!("Iteration {}: Stack length: {}, contents: {:?}", i, stack.len(), stack);
        assert!(!stack.is_empty(), "Stack should not be empty on iteration {i}");

        match &stack[stack.len() - 1] {
            Value::Integer(val) => {
                println!("Iteration {i}: Result = {val}");
                assert_eq!(*val, 15);
            },
            _ => panic!("Expected integer value on iteration {i}"),
        }
    }

    Ok(())
}

#[test]
fn test_new_opcodes() -> Result<()> {
    println!("Testing new opcodes (division)...");
    let mut vm = VM::new(1000)?;

    
    let bytecode = vec![
        OpCode::Push as u8,
        20,
        0,
        0,
        0, 
        OpCode::Push as u8,
        4,
        0,
        0,
        0,                    
        OpCode::Divide as u8, 
        OpCode::Halt as u8,   
    ];

    vm.execute(&bytecode)?;

    let stack = vm.stack();
    println!("Stack length: {}, contents: {:?}", stack.len(), stack);
    assert!(!stack.is_empty(), "Stack should not be empty");

    match &stack[stack.len() - 1] {
        Value::Integer(val) => {
            println!("Division result: {val}");
            assert_eq!(*val, 5);
        },
        _ => panic!("Expected integer value"),
    }

    Ok(())
}

#[test]
fn test_comparison_opcodes() -> Result<()> {
    println!("Testing comparison opcodes...");
    let mut vm = VM::new(1000)?;

    
    let bytecode = vec![
        OpCode::Push as u8,
        10,
        0,
        0,
        0, 
        OpCode::Push as u8,
        5,
        0,
        0,
        0,                         
        OpCode::GreaterThan as u8, 
        OpCode::Halt as u8,        
    ];

    vm.execute(&bytecode)?;

    let stack = vm.stack();
    println!("Stack length: {}, contents: {:?}", stack.len(), stack);
    assert!(!stack.is_empty(), "Stack should not be empty");

    match &stack[stack.len() - 1] {
        Value::Boolean(val) => {
            println!("Comparison result: {val}");
            assert!(*val);
        },
        _ => panic!("Expected boolean value"),
    }

    Ok(())
}
