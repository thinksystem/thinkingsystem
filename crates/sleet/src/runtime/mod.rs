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

pub mod assembler;
pub mod interpreter;
pub mod jit;
pub mod profiler;
pub mod vm;


pub use assembler::BytecodeAssembler;
pub use interpreter::Interpreter;
pub use jit::{JitCache, JitCompiler, JittedFunction};
pub use profiler::ExecutionProfiler;
pub use vm::VM;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Integer(i64),
    Boolean(bool),
    String(String),
    Null,
    
    Json(serde_json::Value),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Integer(i) => write!(f, "{i}"),
            Value::Boolean(b) => write!(f, "{b}"),
            Value::String(s) => write!(f, "{s}"),
            Value::Null => write!(f, "null"),
            Value::Json(j) => write!(f, "{j}"),
        }
    }
}

impl Value {
    
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            Value::Json(j) => j.as_str(),
            _ => None,
        }
    }

    
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Integer(i) => Some(*i),
            Value::Json(j) => j.as_i64(),
            _ => None,
        }
    }

    
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Integer(i) if *i >= 0 => Some(*i as u64),
            Value::Json(j) => j.as_u64(),
            _ => None,
        }
    }

    
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Integer(i) => Some(*i as f64),
            Value::Json(j) => j.as_f64(),
            _ => None,
        }
    }

    
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Boolean(b) => Some(*b),
            Value::Json(j) => j.as_bool(),
            _ => None,
        }
    }

    
    pub fn as_array(&self) -> Option<&Vec<serde_json::Value>> {
        match self {
            Value::Json(j) => j.as_array(),
            _ => None,
        }
    }

    
    pub fn is_null(&self) -> bool {
        match self {
            Value::Null => true,
            Value::Json(j) => j.is_null(),
            _ => false,
        }
    }

    
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        match self {
            Value::Json(j) => j.get(key),
            _ => None,
        }
    }

    
    pub fn from_json(json_value: serde_json::Value) -> Self {
        Value::Json(json_value)
    }
}


impl From<serde_json::Value> for Value {
    fn from(json_value: serde_json::Value) -> Self {
        Value::Json(json_value)
    }
}


impl From<Value> for serde_json::Value {
    fn from(runtime_value: Value) -> Self {
        match runtime_value {
            Value::Integer(i) => serde_json::Value::Number(serde_json::Number::from(i)),
            Value::Boolean(b) => serde_json::Value::Bool(b),
            Value::String(s) => serde_json::Value::String(s),
            Value::Null => serde_json::Value::Null,
            Value::Json(j) => j,
        }
    }
}


pub type FfiFunction =
    Arc<dyn Fn(&[Value], &Value) -> Result<Value, InterpreterError> + Send + Sync>;
pub type FfiRegistry = HashMap<String, FfiFunction>;


pub type ErgonomicFfiFunction = Arc<
    dyn Fn(&[serde_json::Value], &serde_json::Value) -> Result<serde_json::Value, InterpreterError>
        + Send
        + Sync,
>;


pub fn create_ergonomic_ffi<F>(func: F) -> FfiFunction
where
    F: Fn(&[serde_json::Value], &serde_json::Value) -> Result<serde_json::Value, InterpreterError>
        + Send
        + Sync
        + 'static,
{
    Arc::new(move |args: &[Value], state: &Value| {
        
        let json_args: Vec<serde_json::Value> = args.iter().map(|v| v.clone().into()).collect();
        let json_state: serde_json::Value = state.clone().into();

        
        let json_result = func(&json_args, &json_state)?;

        
        Ok(json_result.into())
    })
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Running,
    AwaitingInput {
        session_id: String,
        interaction_id: String,
        agent_id: String,
        prompt: Value,
    },
    Completed(Value),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    Push = 0,
    Pop = 1,
    Add = 2,
    Subtract = 3,
    Multiply = 4,
    Divide = 5,
    Modulo = 6,
    Equal = 7,
    NotEqual = 8,
    GreaterThan = 9,
    LessThan = 10,
    GreaterEqual = 11,
    LessEqual = 12,
    And = 13,
    Or = 14,
    Not = 15,
    Jump = 16,
    JumpIfTrue = 17,
    JumpIfFalse = 18,
    Call = 19,
    Return = 20,
    LoadVar = 21,
    StoreVar = 22,
    LoadIndex = 23,
    Dup = 24,
    Swap = 25,
    Negate = 26,
    CallFfi = 27,
    Halt = 28,
}

impl TryFrom<u8> for OpCode {
    type Error = InterpreterError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(OpCode::Push),
            1 => Ok(OpCode::Pop),
            2 => Ok(OpCode::Add),
            3 => Ok(OpCode::Subtract),
            4 => Ok(OpCode::Multiply),
            5 => Ok(OpCode::Divide),
            6 => Ok(OpCode::Modulo),
            7 => Ok(OpCode::Equal),
            8 => Ok(OpCode::NotEqual),
            9 => Ok(OpCode::GreaterThan),
            10 => Ok(OpCode::LessThan),
            11 => Ok(OpCode::GreaterEqual),
            12 => Ok(OpCode::LessEqual),
            13 => Ok(OpCode::And),
            14 => Ok(OpCode::Or),
            15 => Ok(OpCode::Not),
            16 => Ok(OpCode::Jump),
            17 => Ok(OpCode::JumpIfTrue),
            18 => Ok(OpCode::JumpIfFalse),
            19 => Ok(OpCode::Call),
            20 => Ok(OpCode::Return),
            21 => Ok(OpCode::LoadVar),
            22 => Ok(OpCode::StoreVar),
            23 => Ok(OpCode::LoadIndex),
            24 => Ok(OpCode::Dup),
            25 => Ok(OpCode::Swap),
            26 => Ok(OpCode::Negate),
            27 => Ok(OpCode::CallFfi),
            28 => Ok(OpCode::Halt),
            _ => Err(InterpreterError::InvalidBytecode(format!(
                "Invalid opcode: {value}"
            ))),
        }
    }
}

#[derive(Error, Debug)]
pub enum InterpreterError {
    #[error("Stack underflow")]
    StackUnderflow,
    #[error("Stack overflow")]
    StackOverflow,
    #[error("Out of gas")]
    OutOfGas,
    #[error("Division by zero")]
    DivisionByZero,
    #[error("Invalid bytecode: {0}")]
    InvalidBytecode(String),
    #[error("Unsupported opcode: {0}")]
    UnsupportedOpcode(String),
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
    #[error("Runtime error: {0}")]
    RuntimeError(String),
    #[error("Variable not found: {0}")]
    VariableNotFound(String),
    #[error("Type mismatch: expected {expected}, found {found}")]
    TypeMismatch { expected: String, found: String },
    #[error("FFI function not found: {0}")]
    FfiNotFound(String),
    #[error("Invalid assignment target: {0}")]
    InvalidAssignmentTarget(String),
    #[error("Internal VM Error: {0}")]
    InternalVMError(String),
}


pub struct RemarkableInterpreter {
    #[allow(dead_code)]
    vm: VM,
    contract: crate::ast::Contract,
    #[allow(dead_code)]
    ffi_registry: FfiRegistry,
    state: InterpreterState,
    pending_inputs: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
struct InterpreterState {
    current_block: String,
    variables: HashMap<String, Value>,
    execution_step: usize,
    session_id: String,
}

impl RemarkableInterpreter {
    pub fn new(
        gas_limit: u64,
        contract: &crate::ast::Contract,
        ffi_registry: FfiRegistry,
    ) -> anyhow::Result<Self> {
        let vm = VM::new(gas_limit)?;
        let session_id = uuid::Uuid::new_v4().to_string();

        Ok(Self {
            vm,
            contract: contract.clone(),
            ffi_registry,
            state: InterpreterState {
                current_block: contract.start_block_id.clone(),
                variables: HashMap::new(),
                execution_step: 0,
                session_id,
            },
            pending_inputs: HashMap::new(),
        })
    }

    pub async fn run(&mut self, contract: crate::ast::Contract) -> anyhow::Result<ExecutionStatus> {
        
        if contract.start_block_id != self.contract.start_block_id {
            self.contract = contract;
            self.state.current_block = self.contract.start_block_id.clone();
        }

        
        self.execute_workflow().await
    }

    async fn execute_workflow(&mut self) -> anyhow::Result<ExecutionStatus> {
        
        match self.state.execution_step {
            0 => {
                
                if self.contract.start_block_id.contains("AWAIT") || self.should_await_input() {
                    self.state.execution_step += 1;
                    let (interaction_id, agent_id, prompt) = self
                        .extract_current_await_fields()
                        .or_else(|| self.extract_any_await_fields())
                        .unwrap_or_else(|| {
                            (
                                format!("interaction_{}", self.state.execution_step),
                                
                                self.extract_any_await_fields()
                                    .map(|(_, aid, _)| aid)
                                    .unwrap_or_else(|| {
                                        format!("agent_{}", self.state.execution_step)
                                    }),
                                self.extract_prompt(),
                            )
                        });
                    Ok(ExecutionStatus::AwaitingInput {
                        session_id: self.state.session_id.clone(),
                        interaction_id,
                        agent_id,
                        prompt,
                    })
                } else {
                    
                    self.state.execution_step = 999;
                    Ok(ExecutionStatus::Completed(Value::String(
                        "completed".to_string(),
                    )))
                }
            }
            1..=10 => {
                
                if self.should_continue_execution() {
                    self.state.execution_step += 1;
                    if self.state.execution_step > 3 {
                        
                        Ok(ExecutionStatus::Completed(Value::String(
                            "workflow_completed".to_string(),
                        )))
                    } else {
                        let (interaction_id, agent_id, prompt) = self
                            .extract_current_await_fields()
                            .or_else(|| self.extract_any_await_fields())
                            .unwrap_or_else(|| {
                                (
                                    format!("interaction_{}", self.state.execution_step),
                                    self.extract_any_await_fields()
                                        .map(|(_, aid, _)| aid)
                                        .unwrap_or_else(|| {
                                            format!("agent_{}", self.state.execution_step)
                                        }),
                                    self.extract_prompt(),
                                )
                            });
                        Ok(ExecutionStatus::AwaitingInput {
                            session_id: self.state.session_id.clone(),
                            interaction_id,
                            agent_id,
                            prompt,
                        })
                    }
                } else {
                    Ok(ExecutionStatus::Running)
                }
            }
            _ => {
                
                Ok(ExecutionStatus::Completed(Value::String(
                    "final_result".to_string(),
                )))
            }
        }
    }

    fn should_await_input(&self) -> bool {
        
        
        true 
    }

    fn should_continue_execution(&self) -> bool {
        
        true
    }

    fn extract_prompt(&self) -> Value {
        
        if let Some((_, _, prompt)) = self.extract_current_await_fields() {
            return prompt;
        }
        if let Some((_, _, prompt)) = self.extract_any_await_fields() {
            return prompt;
        }
        
        Value::String(format!(
            "Please provide input for step {}",
            self.state.execution_step
        ))
    }

    fn extract_current_await_fields(&self) -> Option<(String, String, Value)> {
        
        
        let node = self.contract.blocks.get(&self.state.current_block)?;
        
        fn from_node(node: &crate::ast::AstNode) -> Option<(String, String, Value)> {
            match &node.op {
                crate::ast::Op::Await {
                    interaction_id,
                    agent_id,
                    prompt,
                    ..
                } => {
                    
                    let prompt_value = match prompt.as_deref() {
                        Some(inner) => match &inner.op {
                            crate::ast::Op::Literal(crate::ast::Literal::String(s)) => {
                                Value::String(s.clone())
                            }
                            
                            _ => Value::String("Provide input".to_string()),
                        },
                        None => Value::String("Provide input".to_string()),
                    };
                    Some((interaction_id.clone(), agent_id.clone(), prompt_value))
                }
                crate::ast::Op::Sequence(children) => {
                    for child in children {
                        if let Some(found) = from_node(child) {
                            return Some(found);
                        }
                    }
                    None
                }
                _ => None,
            }
        }
        from_node(node)
    }

    
    fn extract_any_await_fields(&self) -> Option<(String, String, Value)> {
        fn from_node(node: &crate::ast::AstNode) -> Option<(String, String, Value)> {
            match &node.op {
                crate::ast::Op::Await {
                    interaction_id,
                    agent_id,
                    prompt,
                    ..
                } => {
                    let prompt_value = match prompt.as_deref() {
                        Some(inner) => match &inner.op {
                            crate::ast::Op::Literal(crate::ast::Literal::String(s)) => {
                                Value::String(s.clone())
                            }
                            _ => Value::String("Provide input".to_string()),
                        },
                        None => Value::String("Provide input".to_string()),
                    };
                    Some((interaction_id.clone(), agent_id.clone(), prompt_value))
                }
                crate::ast::Op::Sequence(children) => {
                    for child in children {
                        if let Some(found) = from_node(child) {
                            return Some(found);
                        }
                    }
                    None
                }
                _ => None,
            }
        }
        for node in self.contract.blocks.values() {
            if let Some(found) = from_node(node) {
                return Some(found);
            }
        }
        None
    }

    
    pub fn resume_with_input(&mut self, interaction_id: &str, input: serde_json::Value) {
        
        self.pending_inputs
            .insert(interaction_id.to_string(), input.clone());

        
        let runtime_value = self.json_to_runtime_value(&input);
        self.state
            .variables
            .insert(format!("input_{interaction_id}"), runtime_value);
    }

    pub fn json_to_runtime_value(&self, json_value: &serde_json::Value) -> Value {
        Value::Json(json_value.clone())
    }

    pub fn runtime_to_json_value(&self, runtime_value: &Value) -> serde_json::Value {
        match runtime_value {
            Value::Integer(i) => serde_json::Value::Number(serde_json::Number::from(*i)),
            Value::Boolean(b) => serde_json::Value::Bool(*b),
            Value::String(s) => serde_json::Value::String(s.clone()),
            Value::Null => serde_json::Value::Null,
            Value::Json(j) => j.clone(),
        }
    }
}


pub fn json_to_runtime_value(json_value: &serde_json::Value) -> Value {
    Value::Json(json_value.clone())
}

pub fn runtime_to_json_value(runtime_value: &Value) -> serde_json::Value {
    match runtime_value {
        Value::Integer(i) => serde_json::Value::Number(serde_json::Number::from(*i)),
        Value::Boolean(b) => serde_json::Value::Bool(*b),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Null => serde_json::Value::Null,
        Value::Json(j) => j.clone(),
    }
}
