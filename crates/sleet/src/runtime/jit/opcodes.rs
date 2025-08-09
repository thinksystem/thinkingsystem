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

use crate::runtime::InterpreterError;
use anyhow::Result as AnyhowResult;
use cranelift::prelude::*;
use std::collections::HashMap;


pub struct JitContext<'a> {
    pub builder: FunctionBuilder<'a>,
    pub stack: Vec<cranelift::prelude::Value>,
    pub blocks: HashMap<usize, Block>, 
}

impl<'a> JitContext<'a> {
    pub fn new(
        ctx: &'a mut cranelift_codegen::Context,
        builder_context: &'a mut FunctionBuilderContext,
    ) -> Self {
        Self {
            builder: FunctionBuilder::new(&mut ctx.func, builder_context),
            stack: Vec::new(),
            blocks: HashMap::new(),
        }
    }

    pub fn push(&mut self, val: cranelift::prelude::Value) {
        self.stack.push(val);
    }

    pub fn pop(&mut self) -> Result<cranelift::prelude::Value, InterpreterError> {
        self.stack.pop().ok_or(InterpreterError::InternalVMError(
            "JIT stack underflow".to_string(),
        ))
    }
}


pub struct OpcodeImpl;

impl OpcodeImpl {
    pub fn implement_push(
        ctx: &mut JitContext,
        bytecode: &[u8],
        ip: &mut usize,
    ) -> AnyhowResult<()> {
        
        if *ip + 4 > bytecode.len() {
            return Err(anyhow::anyhow!("Incomplete push instruction"));
        }

        let value = i32::from_le_bytes([
            bytecode[*ip],
            bytecode[*ip + 1],
            bytecode[*ip + 2],
            bytecode[*ip + 3],
        ]) as i64;
        *ip += 4;

        let val = ctx.builder.ins().iconst(types::I64, value);
        ctx.push(val);
        Ok(())
    }

    pub fn implement_pop(ctx: &mut JitContext) -> AnyhowResult<()> {
        ctx.pop()?;
        Ok(())
    }

    pub fn implement_add(ctx: &mut JitContext) -> AnyhowResult<()> {
        let b = ctx.pop()?;
        let a = ctx.pop()?;
        let res = ctx.builder.ins().iadd(a, b);
        ctx.push(res);
        Ok(())
    }

    pub fn implement_subtract(ctx: &mut JitContext) -> AnyhowResult<()> {
        let b = ctx.pop()?;
        let a = ctx.pop()?;
        let res = ctx.builder.ins().isub(a, b);
        ctx.push(res);
        Ok(())
    }

    pub fn implement_multiply(ctx: &mut JitContext) -> AnyhowResult<()> {
        let b = ctx.pop()?;
        let a = ctx.pop()?;
        let res = ctx.builder.ins().imul(a, b);
        ctx.push(res);
        Ok(())
    }

    
    pub fn implement_divide(ctx: &mut JitContext) -> AnyhowResult<()> {
        let b = ctx.pop()?;
        let a = ctx.pop()?;

        
        let zero = ctx.builder.ins().iconst(types::I64, 0);
        let is_zero = ctx.builder.ins().icmp(IntCC::Equal, b, zero);

        let div_by_zero_block = ctx.builder.create_block();
        let continue_block = ctx.builder.create_block();

        ctx.builder
            .ins()
            .brif(is_zero, div_by_zero_block, &[], continue_block, &[]);

        
        ctx.builder.switch_to_block(div_by_zero_block);
        let error_code = ctx.builder.ins().iconst(types::I64, -2); 
        ctx.builder.ins().return_(&[error_code]);
        ctx.builder.seal_block(div_by_zero_block);

        
        ctx.builder.switch_to_block(continue_block);
        let res = ctx.builder.ins().sdiv(a, b);
        ctx.push(res);
        ctx.builder.seal_block(continue_block);

        Ok(())
    }

    pub fn implement_modulo(ctx: &mut JitContext) -> AnyhowResult<()> {
        let b = ctx.pop()?;
        let a = ctx.pop()?;

        
        let zero = ctx.builder.ins().iconst(types::I64, 0);
        let is_zero = ctx.builder.ins().icmp(IntCC::Equal, b, zero);

        let mod_by_zero_block = ctx.builder.create_block();
        let continue_block = ctx.builder.create_block();

        ctx.builder
            .ins()
            .brif(is_zero, mod_by_zero_block, &[], continue_block, &[]);

        
        ctx.builder.switch_to_block(mod_by_zero_block);
        let error_code = ctx.builder.ins().iconst(types::I64, -3); 
        ctx.builder.ins().return_(&[error_code]);
        ctx.builder.seal_block(mod_by_zero_block);

        
        ctx.builder.switch_to_block(continue_block);
        let res = ctx.builder.ins().srem(a, b);
        ctx.push(res);
        ctx.builder.seal_block(continue_block);

        Ok(())
    }

    pub fn implement_negate(ctx: &mut JitContext) -> AnyhowResult<()> {
        let val = ctx.pop()?;
        let zero = ctx.builder.ins().iconst(types::I64, 0);
        let res = ctx.builder.ins().isub(zero, val);
        ctx.push(res);
        Ok(())
    }

    
    pub fn implement_equal(ctx: &mut JitContext) -> AnyhowResult<()> {
        let b = ctx.pop()?;
        let a = ctx.pop()?;
        let res = ctx.builder.ins().icmp(IntCC::Equal, a, b);
        let res_int = ctx.builder.ins().uextend(types::I64, res);
        ctx.push(res_int);
        Ok(())
    }

    pub fn implement_not_equal(ctx: &mut JitContext) -> AnyhowResult<()> {
        let b = ctx.pop()?;
        let a = ctx.pop()?;
        let res = ctx.builder.ins().icmp(IntCC::NotEqual, a, b);
        let res_int = ctx.builder.ins().uextend(types::I64, res);
        ctx.push(res_int);
        Ok(())
    }

    pub fn implement_greater_than(ctx: &mut JitContext) -> AnyhowResult<()> {
        let b = ctx.pop()?;
        let a = ctx.pop()?;
        let res = ctx.builder.ins().icmp(IntCC::SignedGreaterThan, a, b);
        let res_int = ctx.builder.ins().uextend(types::I64, res);
        ctx.push(res_int);
        Ok(())
    }

    pub fn implement_less_than(ctx: &mut JitContext) -> AnyhowResult<()> {
        let b = ctx.pop()?;
        let a = ctx.pop()?;
        let res = ctx.builder.ins().icmp(IntCC::SignedLessThan, a, b);
        let res_int = ctx.builder.ins().uextend(types::I64, res);
        ctx.push(res_int);
        Ok(())
    }

    pub fn implement_greater_equal(ctx: &mut JitContext) -> AnyhowResult<()> {
        let b = ctx.pop()?;
        let a = ctx.pop()?;
        let res = ctx
            .builder
            .ins()
            .icmp(IntCC::SignedGreaterThanOrEqual, a, b);
        let res_int = ctx.builder.ins().uextend(types::I64, res);
        ctx.push(res_int);
        Ok(())
    }

    pub fn implement_less_equal(ctx: &mut JitContext) -> AnyhowResult<()> {
        let b = ctx.pop()?;
        let a = ctx.pop()?;
        let res = ctx.builder.ins().icmp(IntCC::SignedLessThanOrEqual, a, b);
        let res_int = ctx.builder.ins().uextend(types::I64, res);
        ctx.push(res_int);
        Ok(())
    }

    
    pub fn implement_and(ctx: &mut JitContext) -> AnyhowResult<()> {
        let b = ctx.pop()?;
        let a = ctx.pop()?;

        
        let zero = ctx.builder.ins().iconst(types::I64, 0);
        let a_bool = ctx.builder.ins().icmp(IntCC::NotEqual, a, zero);
        let b_bool = ctx.builder.ins().icmp(IntCC::NotEqual, b, zero);

        let res = ctx.builder.ins().band(a_bool, b_bool);
        let res_int = ctx.builder.ins().uextend(types::I64, res);
        ctx.push(res_int);
        Ok(())
    }

    pub fn implement_or(ctx: &mut JitContext) -> AnyhowResult<()> {
        let b = ctx.pop()?;
        let a = ctx.pop()?;

        
        let zero = ctx.builder.ins().iconst(types::I64, 0);
        let a_bool = ctx.builder.ins().icmp(IntCC::NotEqual, a, zero);
        let b_bool = ctx.builder.ins().icmp(IntCC::NotEqual, b, zero);

        let res = ctx.builder.ins().bor(a_bool, b_bool);
        let res_int = ctx.builder.ins().uextend(types::I64, res);
        ctx.push(res_int);
        Ok(())
    }

    pub fn implement_not(ctx: &mut JitContext) -> AnyhowResult<()> {
        let val = ctx.pop()?;
        let zero = ctx.builder.ins().iconst(types::I64, 0);
        let is_zero = ctx.builder.ins().icmp(IntCC::Equal, val, zero);
        let res_int = ctx.builder.ins().uextend(types::I64, is_zero);
        ctx.push(res_int);
        Ok(())
    }

    
    pub fn implement_dup(ctx: &mut JitContext) -> AnyhowResult<()> {
        if ctx.stack.is_empty() {
            return Err(anyhow::anyhow!("Cannot duplicate from empty stack"));
        }
        let val = *ctx.stack.last().unwrap();
        ctx.push(val);
        Ok(())
    }

    pub fn implement_swap(ctx: &mut JitContext) -> AnyhowResult<()> {
        if ctx.stack.len() < 2 {
            return Err(anyhow::anyhow!(
                "Cannot swap with less than 2 items on stack"
            ));
        }
        let len = ctx.stack.len();
        ctx.stack.swap(len - 1, len - 2);
        Ok(())
    }

    pub fn implement_call_ffi(
        _ctx: &mut JitContext,
        _bytecode: &[u8],
        _ip: &mut usize,
        _ffi_registry_ptr: cranelift::prelude::Value,
    ) -> AnyhowResult<()> {
        
        
        Err(anyhow::anyhow!("CallFfi is not supported in JIT"))
    }

    pub fn implement_halt(
        ctx: &mut JitContext,
        result_ptr: cranelift::prelude::Value,
    ) -> AnyhowResult<bool> {
        
        let result = if !ctx.stack.is_empty() {
            ctx.pop()?
        } else {
            ctx.builder.ins().iconst(types::I64, 0)
        };

        
        ctx.builder
            .ins()
            .store(MemFlags::trusted(), result, result_ptr, 0);

        
        let success_code = ctx.builder.ins().iconst(types::I64, 1);
        ctx.builder.ins().return_(&[success_code]);
        Ok(true) 
    }

    pub fn implement_halt_store_only(
        ctx: &mut JitContext,
        result_ptr: cranelift::prelude::Value,
    ) -> AnyhowResult<()> {
        
        let result = if !ctx.stack.is_empty() {
            ctx.pop()?
        } else {
            ctx.builder.ins().iconst(types::I64, 0)
        };
        ctx.builder
            .ins()
            .store(MemFlags::trusted(), result, result_ptr, 0);
        Ok(())
    }
}
