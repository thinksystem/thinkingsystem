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

use crate::runtime::{OpCode, Value as RuntimeValue};
use anyhow::Result as AnyhowResult;
use serde_json::Value;



pub struct BytecodeAssembler {
    bytecode: Vec<u8>,
}

impl BytecodeAssembler {
    
    pub fn new() -> Self {
        Self {
            bytecode: Vec::new(),
        }
    }

    
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bytecode: Vec::with_capacity(capacity),
        }
    }

    
    pub fn bytecode(&self) -> &[u8] {
        &self.bytecode
    }

    
    pub fn into_bytecode(self) -> Vec<u8> {
        self.bytecode
    }

    
    pub fn len(&self) -> usize {
        self.bytecode.len()
    }

    
    pub fn is_empty(&self) -> bool {
        self.bytecode.is_empty()
    }

    
    pub fn opcode(&mut self, opcode: OpCode) -> &mut Self {
        self.bytecode.push(opcode as u8);
        self
    }

    
    pub fn opcode_with_json(&mut self, opcode: OpCode, value: &Value) -> AnyhowResult<&mut Self> {
        self.bytecode.push(opcode as u8);
        let serialized = serde_json::to_vec(value)
            .map_err(|e| anyhow::anyhow!("Failed to serialize value: {}", e))?;
        self.bytecode
            .extend_from_slice(&(serialized.len() as u32).to_le_bytes());
        self.bytecode.extend_from_slice(&serialized);
        Ok(self)
    }

    
    pub fn opcode_with_value(
        &mut self,
        opcode: OpCode,
        value: &RuntimeValue,
    ) -> AnyhowResult<&mut Self> {
        self.bytecode.push(opcode as u8);
        let json_value: Value = value.clone().into();
        let serialized = serde_json::to_vec(&json_value)
            .map_err(|e| anyhow::anyhow!("Failed to serialize runtime value: {}", e))?;
        self.bytecode
            .extend_from_slice(&(serialized.len() as u32).to_le_bytes());
        self.bytecode.extend_from_slice(&serialized);
        Ok(self)
    }

    
    pub fn opcode_with_string(&mut self, opcode: OpCode, s: &str) -> &mut Self {
        self.bytecode.push(opcode as u8);
        let bytes = s.as_bytes();
        self.bytecode
            .extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        self.bytecode.extend_from_slice(bytes);
        self
    }

    
    pub fn opcode_with_bytes(&mut self, opcode: OpCode, bytes: &[u8]) -> &mut Self {
        self.bytecode.push(opcode as u8);
        self.bytecode
            .extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        self.bytecode.extend_from_slice(bytes);
        self
    }

    
    pub fn push_literal(&mut self, value: &Value) -> AnyhowResult<&mut Self> {
        self.opcode_with_json(OpCode::Push, value)
    }

    
    pub fn push_runtime_value(&mut self, value: &RuntimeValue) -> AnyhowResult<&mut Self> {
        self.opcode_with_value(OpCode::Push, value)
    }

    
    pub fn load_var(&mut self, path: &str) -> &mut Self {
        self.opcode_with_string(OpCode::LoadVar, path)
    }

    
    pub fn load_var_path(&mut self, path_components: &[&str]) -> &mut Self {
        let path = path_components.join(".");
        self.load_var(&path)
    }

    
    
    pub fn jump_if_false(&mut self) -> usize {
        self.bytecode.push(OpCode::JumpIfFalse as u8);
        let pos = self.bytecode.len();
        self.bytecode.extend_from_slice(&[0, 0, 0, 0]); 
        pos
    }

    
    
    pub fn jump(&mut self) -> usize {
        self.bytecode.push(OpCode::Jump as u8);
        let pos = self.bytecode.len();
        self.bytecode.extend_from_slice(&[0, 0, 0, 0]); 
        pos
    }

    
    
    pub fn patch_jump(&mut self, jump_pos: usize) -> AnyhowResult<&mut Self> {
        if jump_pos + 4 > self.bytecode.len() {
            return Err(anyhow::anyhow!("Invalid jump position: {}", jump_pos));
        }
        let offset = (self.bytecode.len() - jump_pos - 4) as u32;
        self.bytecode[jump_pos..jump_pos + 4].copy_from_slice(&offset.to_le_bytes());
        Ok(self)
    }

    
    pub fn call_function(&mut self, arg_count: usize) -> AnyhowResult<&mut Self> {
        let count_value = Value::Number(serde_json::Number::from(arg_count));
        self.opcode_with_json(OpCode::Push, &count_value)?;
        self.opcode(OpCode::Call);
        Ok(self)
    }

    
    pub fn add(&mut self) -> &mut Self {
        self.opcode(OpCode::Add)
    }
    pub fn subtract(&mut self) -> &mut Self {
        self.opcode(OpCode::Subtract)
    }
    pub fn multiply(&mut self) -> &mut Self {
        self.opcode(OpCode::Multiply)
    }
    pub fn divide(&mut self) -> &mut Self {
        self.opcode(OpCode::Divide)
    }
    pub fn modulo(&mut self) -> &mut Self {
        self.opcode(OpCode::Modulo)
    }

    
    pub fn equal(&mut self) -> &mut Self {
        self.opcode(OpCode::Equal)
    }
    pub fn not_equal(&mut self) -> &mut Self {
        self.opcode(OpCode::NotEqual)
    }
    pub fn greater_than(&mut self) -> &mut Self {
        self.opcode(OpCode::GreaterThan)
    }
    pub fn greater_equal(&mut self) -> &mut Self {
        self.opcode(OpCode::GreaterEqual)
    }
    pub fn less_than(&mut self) -> &mut Self {
        self.opcode(OpCode::LessThan)
    }
    pub fn less_equal(&mut self) -> &mut Self {
        self.opcode(OpCode::LessEqual)
    }

    
    pub fn and(&mut self) -> &mut Self {
        self.opcode(OpCode::And)
    }
    pub fn or(&mut self) -> &mut Self {
        self.opcode(OpCode::Or)
    }
    pub fn not(&mut self) -> &mut Self {
        self.opcode(OpCode::Not)
    }
    pub fn negate(&mut self) -> &mut Self {
        self.opcode(OpCode::Negate)
    }

    
    pub fn load_index(&mut self) -> &mut Self {
        self.opcode(OpCode::LoadIndex)
    }
    pub fn halt(&mut self) -> &mut Self {
        self.opcode(OpCode::Halt)
    }
}

impl Default for BytecodeAssembler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_basic_assembly() {
        let mut asm = BytecodeAssembler::new();
        asm.push_literal(&json!(42))
            .unwrap()
            .push_literal(&json!(10))
            .unwrap()
            .add();

        let bytecode = asm.into_bytecode();
        assert!(!bytecode.is_empty());
        assert_eq!(bytecode[0], OpCode::Push as u8);
    }

    #[test]
    fn test_jump_patching() {
        let mut asm = BytecodeAssembler::new();
        let jump_pos = asm.jump_if_false();
        asm.push_literal(&json!(1)).unwrap();
        asm.patch_jump(jump_pos).unwrap();

        let bytecode = asm.into_bytecode();
        assert_eq!(bytecode[0], OpCode::JumpIfFalse as u8);
        
        let offset = u32::from_le_bytes([bytecode[1], bytecode[2], bytecode[3], bytecode[4]]);
        assert!(offset > 0);
    }

    #[test]
    fn test_variable_loading() {
        let mut asm = BytecodeAssembler::new();
        asm.load_var("state.counter")
            .load_var_path(&["state", "limit"]);

        let bytecode = asm.into_bytecode();
        assert_eq!(bytecode[0], OpCode::LoadVar as u8);
    }
}
