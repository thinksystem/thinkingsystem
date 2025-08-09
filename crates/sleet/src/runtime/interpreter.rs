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

use crate::runtime::{FfiRegistry, InterpreterError, OpCode, Value};
use anyhow::Result as AnyhowResult;

pub struct Interpreter {
    stack: Vec<Value>,
    variables: std::collections::HashMap<String, Value>,
    gas: u64,
}

impl Interpreter {
    pub fn new(gas_limit: u64) -> Self {
        Self {
            stack: Vec::new(),
            variables: std::collections::HashMap::new(),
            gas: gas_limit,
        }
    }

    pub fn set_gas(&mut self, gas: u64) {
        self.gas = gas;
    }

    pub fn gas(&self) -> u64 {
        self.gas
    }

    pub fn stack(&self) -> &[Value] {
        &self.stack
    }

    pub fn push_value(&mut self, value: Value) {
        self.stack.push(value);
    }

    pub fn pop_value(&mut self) -> AnyhowResult<Value> {
        self.stack
            .pop()
            .ok_or_else(|| InterpreterError::StackUnderflow.into())
    }

    pub fn clear_stack(&mut self) {
        self.stack.clear();
    }

    pub fn variables(&self) -> &std::collections::HashMap<String, Value> {
        &self.variables
    }

    pub fn execute_bytecode(&mut self, bytecode: &[u8]) -> AnyhowResult<()> {
        let mut ip = 0;

        while ip < bytecode.len() {
            if self.gas == 0 {
                return Err(InterpreterError::OutOfGas.into());
            }
            self.gas -= 1;

            let opcode = OpCode::try_from(bytecode[ip])?;
            ip += 1;

            match opcode {
                OpCode::Push => {
                    if ip + 4 > bytecode.len() {
                        return Err(InterpreterError::InvalidBytecode(
                            "Incomplete push instruction".to_string(),
                        )
                        .into());
                    }
                    
                    let value = i32::from_le_bytes([
                        bytecode[ip],
                        bytecode[ip + 1],
                        bytecode[ip + 2],
                        bytecode[ip + 3],
                    ]) as i64;
                    ip += 4;
                    self.stack.push(Value::Integer(value));
                }
                OpCode::Pop => {
                    if self.stack.is_empty() {
                        return Err(InterpreterError::StackUnderflow.into());
                    }
                    self.stack.pop();
                }
                OpCode::Add => {
                    let b = self.pop_value_internal()?;
                    let a = self.pop_value_internal()?;
                    match (a, b) {
                        (Value::Integer(a), Value::Integer(b)) => {
                            self.stack.push(Value::Integer(a.saturating_add(b)));
                        }
                        (Value::String(a), Value::String(b)) => {
                            self.stack.push(Value::String(format!("{a}{b}")));
                        }
                        (Value::String(a), b) => {
                            self.stack
                                .push(Value::String(format!("{a}{b}")));
                        }
                        (a, Value::String(b)) => {
                            self.stack
                                .push(Value::String(format!("{a}{b}")));
                        }
                        _ => {
                            return Err(InterpreterError::InvalidOperation(
                                "Cannot add these value types".to_string(),
                            )
                            .into())
                        }
                    }
                }
                OpCode::Subtract => {
                    let b = self.pop_value_internal()?;
                    let a = self.pop_value_internal()?;
                    match (a, b) {
                        (Value::Integer(a), Value::Integer(b)) => {
                            self.stack.push(Value::Integer(a.saturating_sub(b)));
                        }
                        _ => {
                            return Err(InterpreterError::InvalidOperation(
                                "Cannot subtract non-integer values".to_string(),
                            )
                            .into())
                        }
                    }
                }
                OpCode::Multiply => {
                    let b = self.pop_value_internal()?;
                    let a = self.pop_value_internal()?;
                    match (a, b) {
                        (Value::Integer(a), Value::Integer(b)) => {
                            self.stack.push(Value::Integer(a.saturating_mul(b)));
                        }
                        _ => {
                            return Err(InterpreterError::InvalidOperation(
                                "Cannot multiply non-integer values".to_string(),
                            )
                            .into())
                        }
                    }
                }
                OpCode::Divide => {
                    let b = self.pop_value_internal()?;
                    let a = self.pop_value_internal()?;
                    match (a, b) {
                        (Value::Integer(a), Value::Integer(b)) => {
                            if b == 0 {
                                return Err(InterpreterError::DivisionByZero.into());
                            }
                            self.stack.push(Value::Integer(a / b));
                        }
                        _ => {
                            return Err(InterpreterError::InvalidOperation(
                                "Cannot divide non-integer values".to_string(),
                            )
                            .into())
                        }
                    }
                }
                OpCode::Modulo => {
                    let b = self.pop_value_internal()?;
                    let a = self.pop_value_internal()?;
                    match (a, b) {
                        (Value::Integer(a), Value::Integer(b)) => {
                            if b == 0 {
                                return Err(InterpreterError::DivisionByZero.into());
                            }
                            self.stack.push(Value::Integer(a % b));
                        }
                        _ => {
                            return Err(InterpreterError::InvalidOperation(
                                "Cannot modulo non-integer values".to_string(),
                            )
                            .into())
                        }
                    }
                }
                OpCode::Negate => {
                    let a = self.pop_value_internal()?;
                    match a {
                        Value::Integer(a) => {
                            self.stack.push(Value::Integer(-a));
                        }
                        _ => {
                            return Err(InterpreterError::InvalidOperation(
                                "Cannot negate non-integer value".to_string(),
                            )
                            .into())
                        }
                    }
                }
                OpCode::Equal => {
                    let b = self.pop_value_internal()?;
                    let a = self.pop_value_internal()?;
                    let result = a == b;
                    self.stack.push(Value::Boolean(result));
                }
                OpCode::NotEqual => {
                    let b = self.pop_value_internal()?;
                    let a = self.pop_value_internal()?;
                    let result = a != b;
                    self.stack.push(Value::Boolean(result));
                }
                OpCode::GreaterThan => {
                    let b = self.pop_value_internal()?;
                    let a = self.pop_value_internal()?;
                    match (a, b) {
                        (Value::Integer(a), Value::Integer(b)) => {
                            self.stack.push(Value::Boolean(a > b));
                        }
                        _ => {
                            return Err(InterpreterError::InvalidOperation(
                                "Cannot compare non-integer values".to_string(),
                            )
                            .into())
                        }
                    }
                }
                OpCode::LessThan => {
                    let b = self.pop_value_internal()?;
                    let a = self.pop_value_internal()?;
                    match (a, b) {
                        (Value::Integer(a), Value::Integer(b)) => {
                            self.stack.push(Value::Boolean(a < b));
                        }
                        _ => {
                            return Err(InterpreterError::InvalidOperation(
                                "Cannot compare non-integer values".to_string(),
                            )
                            .into())
                        }
                    }
                }
                OpCode::GreaterEqual => {
                    let b = self.pop_value_internal()?;
                    let a = self.pop_value_internal()?;
                    match (a, b) {
                        (Value::Integer(a), Value::Integer(b)) => {
                            self.stack.push(Value::Boolean(a >= b));
                        }
                        _ => {
                            return Err(InterpreterError::InvalidOperation(
                                "Cannot compare non-integer values".to_string(),
                            )
                            .into())
                        }
                    }
                }
                OpCode::LessEqual => {
                    let b = self.pop_value_internal()?;
                    let a = self.pop_value_internal()?;
                    match (a, b) {
                        (Value::Integer(a), Value::Integer(b)) => {
                            self.stack.push(Value::Boolean(a <= b));
                        }
                        _ => {
                            return Err(InterpreterError::InvalidOperation(
                                "Cannot compare non-integer values".to_string(),
                            )
                            .into())
                        }
                    }
                }
                OpCode::And => {
                    let b = self.pop_value_internal()?;
                    let a = self.pop_value_internal()?;
                    let result = self.to_boolean(&a) && self.to_boolean(&b);
                    self.stack.push(Value::Boolean(result));
                }
                OpCode::Or => {
                    let b = self.pop_value_internal()?;
                    let a = self.pop_value_internal()?;
                    let result = self.to_boolean(&a) || self.to_boolean(&b);
                    self.stack.push(Value::Boolean(result));
                }
                OpCode::Not => {
                    let a = self.pop_value_internal()?;
                    let result = !self.to_boolean(&a);
                    self.stack.push(Value::Boolean(result));
                }
                OpCode::Dup => {
                    if self.stack.is_empty() {
                        return Err(InterpreterError::StackUnderflow.into());
                    }
                    let value = self.stack.last().unwrap().clone();
                    self.stack.push(value);
                }
                OpCode::Swap => {
                    if self.stack.len() < 2 {
                        return Err(InterpreterError::StackUnderflow.into());
                    }
                    let len = self.stack.len();
                    self.stack.swap(len - 1, len - 2);
                }
                OpCode::Halt => break,
                _ => {
                    return Err(InterpreterError::UnsupportedOpcode(format!("{opcode:?}")).into());
                }
            }
        }

        Ok(())
    }

    pub fn execute_bytecode_with_ffi(
        &mut self,
        bytecode: &[u8],
        ffi_registry: &FfiRegistry,
    ) -> AnyhowResult<()> {
        let mut ip = 0;

        while ip < bytecode.len() {
            if self.gas == 0 {
                return Err(InterpreterError::OutOfGas.into());
            }
            self.gas -= 1;

            let opcode = OpCode::try_from(bytecode[ip])?;
            ip += 1;

            match opcode {
                OpCode::CallFfi => {
                    
                    if ip + 4 > bytecode.len() {
                        return Err(InterpreterError::InvalidBytecode(
                            "Incomplete FFI function name length".to_string(),
                        )
                        .into());
                    }
                    let name_len = u32::from_le_bytes([
                        bytecode[ip],
                        bytecode[ip + 1],
                        bytecode[ip + 2],
                        bytecode[ip + 3],
                    ]) as usize;
                    ip += 4;

                    
                    if ip + name_len > bytecode.len() {
                        return Err(InterpreterError::InvalidBytecode(
                            "Incomplete FFI function name".to_string(),
                        )
                        .into());
                    }
                    let name = String::from_utf8_lossy(&bytecode[ip..ip + name_len]).to_string();
                    ip += name_len;

                    
                    if ip >= bytecode.len() {
                        return Err(InterpreterError::InvalidBytecode(
                            "Missing FFI argument count".to_string(),
                        )
                        .into());
                    }
                    let arg_count = bytecode[ip] as usize;
                    ip += 1;

                    
                    let mut args = Vec::new();
                    for _ in 0..arg_count {
                        args.push(self.pop_value_internal()?);
                    }
                    args.reverse(); 

                    
                    if let Some(ffi_fn) = ffi_registry.get(&name) {
                        let permissions = Value::Null; 
                        match ffi_fn(&args, &permissions) {
                            Ok(result) => {
                                self.stack.push(result);
                            }
                            Err(e) => {
                                return Err(InterpreterError::RuntimeError(format!(
                                    "FFI function '{name}' failed: {e}"
                                ))
                                .into());
                            }
                        }
                    } else {
                        return Err(InterpreterError::RuntimeError(format!(
                            "FFI function '{name}' not found"
                        ))
                        .into());
                    }
                }
                _ => {
                    
                    
                    let _single_instruction = [opcode as u8];
                    let current_gas = self.gas;
                    self.gas += 1; 

                    
                    match opcode {
                        OpCode::Push => {
                            
                            if ip + 4 > bytecode.len() {
                                return Err(InterpreterError::InvalidBytecode(
                                    "Incomplete push instruction".to_string(),
                                )
                                .into());
                            }
                            
                            let value = i32::from_le_bytes([
                                bytecode[ip],
                                bytecode[ip + 1],
                                bytecode[ip + 2],
                                bytecode[ip + 3],
                            ]) as i64;
                            ip += 4;
                            self.stack.push(Value::Integer(value));
                        }
                        OpCode::Dup => {
                            if let Some(value) = self.stack.last() {
                                let value = value.clone();
                                self.stack.push(value);
                            } else {
                                return Err(InterpreterError::StackUnderflow.into());
                            }
                        }
                        OpCode::Pop => {
                            if self.stack.is_empty() {
                                return Err(InterpreterError::StackUnderflow.into());
                            }
                            self.stack.pop();
                        }
                        OpCode::Halt => {
                            break;
                        }
                        OpCode::Add => {
                            if self.stack.len() < 2 {
                                return Err(InterpreterError::StackUnderflow.into());
                            }
                            let b = self.stack.pop().unwrap();
                            let a = self.stack.pop().unwrap();
                            match (a, b) {
                                (Value::Integer(a), Value::Integer(b)) => {
                                    self.stack.push(Value::Integer(a.saturating_add(b)));
                                }
                                (Value::String(a), Value::String(b)) => {
                                    self.stack.push(Value::String(format!("{a}{b}")));
                                }
                                (Value::String(a), Value::Integer(b)) => {
                                    self.stack
                                        .push(Value::String(format!("{a}{b}")));
                                }
                                (Value::Integer(a), Value::String(b)) => {
                                    self.stack
                                        .push(Value::String(format!("{a}{b}")));
                                }
                                _ => {
                                    return Err(InterpreterError::TypeMismatch {
                                        expected: "integer or string".to_string(),
                                        found: "incompatible types for addition".to_string(),
                                    }.into());
                                }
                            }
                        }
                        OpCode::Subtract => {
                            if self.stack.len() < 2 {
                                return Err(InterpreterError::StackUnderflow.into());
                            }
                            let b = self.stack.pop().unwrap();
                            let a = self.stack.pop().unwrap();
                            match (a, b) {
                                (Value::Integer(a), Value::Integer(b)) => {
                                    self.stack.push(Value::Integer(a.saturating_sub(b)));
                                }
                                _ => {
                                    return Err(InterpreterError::TypeMismatch {
                                        expected: "integer".to_string(),
                                        found: "incompatible types for subtraction".to_string(),
                                    }.into());
                                }
                            }
                        }
                        OpCode::Multiply => {
                            if self.stack.len() < 2 {
                                return Err(InterpreterError::StackUnderflow.into());
                            }
                            let b = self.stack.pop().unwrap();
                            let a = self.stack.pop().unwrap();
                            match (a, b) {
                                (Value::Integer(a), Value::Integer(b)) => {
                                    self.stack.push(Value::Integer(a.saturating_mul(b)));
                                }
                                _ => {
                                    return Err(InterpreterError::TypeMismatch {
                                        expected: "integer".to_string(),
                                        found: "incompatible types for multiplication".to_string(),
                                    }.into());
                                }
                            }
                        }
                        _ => {
                            return Err(InterpreterError::UnsupportedOpcode(format!(
                                "{opcode:?}"
                            ))
                            .into());
                        }
                    }

                    self.gas = current_gas - 1; 
                }
            }
        }

        Ok(())
    }

    fn pop_value_internal(&mut self) -> AnyhowResult<Value> {
        self.stack
            .pop()
            .ok_or_else(|| InterpreterError::StackUnderflow.into())
    }

    fn to_boolean(&self, value: &Value) -> bool {
        match value {
            Value::Boolean(b) => *b,
            Value::Integer(i) => *i != 0,
            Value::String(s) => !s.is_empty(),
            Value::Null => false,
            Value::Json(j) => match j {
                serde_json::Value::Bool(b) => *b,
                serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
                serde_json::Value::String(s) => !s.is_empty(),
                serde_json::Value::Array(a) => !a.is_empty(),
                serde_json::Value::Object(o) => !o.is_empty(),
                serde_json::Value::Null => false,
            },
        }
    }
}
