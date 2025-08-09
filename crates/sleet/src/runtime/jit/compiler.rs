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

use super::cache::JittedFunction;
use super::opcodes::{JitContext, OpcodeImpl};
use crate::runtime::OpCode;
use anyhow::Result as AnyhowResult;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};
use std::mem;
use std::sync::{Arc, Mutex};


struct SendSafeJitModule {
    module: JITModule,
}

unsafe impl Send for SendSafeJitModule {}
unsafe impl Sync for SendSafeJitModule {}

pub struct JitCompiler {
    module: Arc<Mutex<SendSafeJitModule>>,
}

impl JitCompiler {
    pub fn new() -> AnyhowResult<Self> {
        let mut builder = JITBuilder::new(cranelift_module::default_libcall_names())?;

        
        #[cfg(target_os = "macos")]
        builder.symbol_lookup_fn(Box::new(|_name| None));

        let module = JITModule::new(builder);
        Ok(Self {
            module: Arc::new(Mutex::new(SendSafeJitModule { module })),
        })
    }

    pub fn compile_with_ffi(
        &mut self,
        bytecode: &[u8],
        name: &str,
        _ffi_registry: Option<&crate::runtime::FfiRegistry>,
    ) -> AnyhowResult<JittedFunction> {
        let mut module_guard = self.module.lock().unwrap();
        let module = &mut module_guard.module;

        let mut ctx = module.make_context();
        let pointer_type = module.target_config().pointer_type();

        
        ctx.func.signature.params.push(AbiParam::new(pointer_type)); 
        ctx.func.signature.params.push(AbiParam::new(pointer_type)); 
        ctx.func.signature.params.push(AbiParam::new(pointer_type)); 
        ctx.func.signature.returns.push(AbiParam::new(types::I64));

        let func_id = module.declare_function(name, Linkage::Local, &ctx.func.signature)?;

        
        let mut builder_context = FunctionBuilderContext::new();
        let mut jit_context = JitContext::new(&mut ctx, &mut builder_context);
        let entry_block = jit_context.builder.create_block();
        jit_context
            .builder
            .append_block_params_for_function_params(entry_block);
        jit_context.builder.switch_to_block(entry_block);
        jit_context.builder.seal_block(entry_block);

        let gas_ptr = jit_context.builder.block_params(entry_block)[0];
        let result_ptr = jit_context.builder.block_params(entry_block)[1];
        let _ffi_registry_ptr = jit_context.builder.block_params(entry_block)[2];

        Self::translate_bytecode(
            &mut jit_context,
            bytecode,
            gas_ptr,
            result_ptr,
            _ffi_registry_ptr,
        )?;

        jit_context.builder.finalize();

        
        if let Err(err) = cranelift_codegen::verify_function(
            &ctx.func,
            &cranelift_codegen::settings::Flags::new(cranelift_codegen::settings::builder()),
        ) {
            
            eprintln!("Cranelift verify error ({name}): {err}");
            
            
            let _ = std::panic::catch_unwind(|| {
                eprintln!("{}", ctx.func.display());
            });
            return Err(anyhow::anyhow!("Verifier errors: {}", err));
        }

        
        

        module.define_function(func_id, &mut ctx)?;
        module.clear_context(&mut ctx);
        module.finalize_definitions()?;

        let code_ptr = module.get_finalized_function(func_id);
        Ok(unsafe { mem::transmute::<*const u8, JittedFunction>(code_ptr) })
    }

    pub fn compile_with_stack(
        &mut self,
        bytecode: &[u8],
        name: &str,
    ) -> AnyhowResult<super::cache::JittedFunctionWithStack> {
        let mut module_guard = self.module.lock().unwrap();
        let module = &mut module_guard.module;

        let mut ctx = module.make_context();
        let pointer_type = module.target_config().pointer_type();

        
        
        ctx.func.signature.params.push(AbiParam::new(pointer_type)); 
        ctx.func.signature.params.push(AbiParam::new(pointer_type)); 
        ctx.func.signature.params.push(AbiParam::new(pointer_type)); 
        ctx.func.signature.params.push(AbiParam::new(pointer_type)); 
        ctx.func.signature.params.push(AbiParam::new(types::I64)); 
        ctx.func.signature.params.push(AbiParam::new(pointer_type)); 
        ctx.func.signature.params.push(AbiParam::new(types::I64)); 
        ctx.func.signature.params.push(AbiParam::new(pointer_type)); 
        ctx.func.signature.returns.push(AbiParam::new(types::I64));

        let func_id = module.declare_function(name, Linkage::Local, &ctx.func.signature)?;

        
        let mut builder_context = FunctionBuilderContext::new();
        let mut jit_context = JitContext::new(&mut ctx, &mut builder_context);
        let entry_block = jit_context.builder.create_block();
        jit_context
            .builder
            .append_block_params_for_function_params(entry_block);
        jit_context.builder.switch_to_block(entry_block);
        jit_context.builder.seal_block(entry_block);

        let gas_ptr = jit_context.builder.block_params(entry_block)[0];
        let result_ptr = jit_context.builder.block_params(entry_block)[1];
        let _ffi_registry_ptr = jit_context.builder.block_params(entry_block)[2];
        let in_stack_ptr = jit_context.builder.block_params(entry_block)[3];
        let in_stack_len = jit_context.builder.block_params(entry_block)[4];
        let out_stack_ptr = jit_context.builder.block_params(entry_block)[5];
        let out_stack_cap = jit_context.builder.block_params(entry_block)[6];
        let out_stack_len_ptr = jit_context.builder.block_params(entry_block)[7];

        
        let min_required = Self::analyse_min_required_inputs(bytecode)? as i64;
        let min_req_val = jit_context.builder.ins().iconst(types::I64, min_required);

        
        let has_enough = jit_context.builder.ins().icmp(
            IntCC::SignedGreaterThanOrEqual,
            in_stack_len,
            min_req_val,
        );
        let not_enough_block = jit_context.builder.create_block();
        let proceed_block = jit_context.builder.create_block();
        jit_context
            .builder
            .ins()
            .brif(has_enough, proceed_block, &[], not_enough_block, &[]);

        
        jit_context.builder.switch_to_block(not_enough_block);
        let err_seed = jit_context.builder.ins().iconst(types::I64, -6);
        jit_context.builder.ins().return_(&[err_seed]);
        jit_context.builder.seal_block(not_enough_block);

        
        jit_context.builder.switch_to_block(proceed_block);
        
        let base_index = jit_context.builder.ins().isub(in_stack_len, min_req_val);
        let eight = jit_context.builder.ins().iconst(types::I64, 8);
        let min_req_usize = min_required as usize;
        for i in 0..min_req_usize {
            let i_iconst = jit_context.builder.ins().iconst(types::I64, i as i64);
            let idx_off = jit_context.builder.ins().iadd(base_index, i_iconst);
            let byte_off_i64 = jit_context.builder.ins().imul(idx_off, eight);
            let byte_off_ptr = if pointer_type == types::I64 {
                byte_off_i64
            } else {
                jit_context
                    .builder
                    .ins()
                    .ireduce(pointer_type, byte_off_i64)
            };
            let addr = jit_context.builder.ins().iadd(in_stack_ptr, byte_off_ptr);
            let val = jit_context
                .builder
                .ins()
                .load(types::I64, MemFlags::trusted(), addr, 0);
            jit_context.push(val);
        }
        
        jit_context.builder.seal_block(proceed_block);

        Self::translate_bytecode_stack(&mut jit_context, bytecode, gas_ptr, result_ptr)?;

        
        
        let depth_usize = jit_context.stack.len();
        let depth_val = jit_context
            .builder
            .ins()
            .iconst(types::I64, depth_usize as i64);
        let cap_ok = jit_context.builder.ins().icmp(
            IntCC::SignedGreaterThanOrEqual,
            out_stack_cap,
            depth_val,
        );
        let cap_bad_block = jit_context.builder.create_block();
        let cap_ok_block = jit_context.builder.create_block();
        jit_context
            .builder
            .ins()
            .brif(cap_ok, cap_ok_block, &[], cap_bad_block, &[]);

        
        jit_context.builder.switch_to_block(cap_bad_block);
        let err = jit_context.builder.ins().iconst(types::I64, -4);
        jit_context.builder.ins().return_(&[err]);
        jit_context.builder.seal_block(cap_bad_block);

        
        jit_context.builder.switch_to_block(cap_ok_block);
        if let Some(&top_val) = jit_context.stack.last() {
            jit_context
                .builder
                .ins()
                .store(MemFlags::trusted(), top_val, result_ptr, 0);
        } else {
            let zero = jit_context.builder.ins().iconst(types::I64, 0);
            jit_context
                .builder
                .ins()
                .store(MemFlags::trusted(), zero, result_ptr, 0);
        }
        for (i, val) in jit_context.stack.iter().enumerate() {
            let byte_off: i32 = (i as i32) * 8;
            jit_context
                .builder
                .ins()
                .store(MemFlags::trusted(), *val, out_stack_ptr, byte_off);
        }
        jit_context
            .builder
            .ins()
            .store(MemFlags::trusted(), depth_val, out_stack_len_ptr, 0);
        let ok = jit_context.builder.ins().iconst(types::I64, 1);
        jit_context.builder.ins().return_(&[ok]);
        jit_context.builder.seal_block(cap_ok_block);

        jit_context.builder.finalize();

        
        if let Err(err) = cranelift_codegen::verify_function(
            &ctx.func,
            &cranelift_codegen::settings::Flags::new(cranelift_codegen::settings::builder()),
        ) {
            
            eprintln!("Cranelift verify error ({name}): {err}");
            let _ = std::panic::catch_unwind(|| {
                eprintln!("{}", ctx.func.display());
            });
            return Err(anyhow::anyhow!("Verifier errors: {}", err));
        }

        module.define_function(func_id, &mut ctx)?;
        module.clear_context(&mut ctx);
        module.finalize_definitions()?;

        let code_ptr = module.get_finalized_function(func_id);
        Ok(unsafe { mem::transmute::<*const u8, super::cache::JittedFunctionWithStack>(code_ptr) })
    }

    
    
    
    fn analyse_min_required_inputs(bytecode: &[u8]) -> AnyhowResult<usize> {
        use crate::runtime::OpCode::*;
        let mut ip = 0usize;
        let mut depth: i64 = 0;
        let mut min_depth: i64 = 0;

        
        let mut apply = |r: i64, p: i64| {
            depth -= r;
            if depth < min_depth {
                min_depth = depth;
            }
            depth += p;
        };

        while ip < bytecode.len() {
            let opcode = OpCode::try_from(bytecode[ip])?;
            ip += 1;
            match opcode {
                Push => {
                    
                    if ip + 4 > bytecode.len() {
                        break;
                    }
                    ip += 4;
                    apply(0, 1);
                }
                Pop => apply(1, 0),
                Add | Subtract | Multiply | Divide | Modulo => apply(2, 1),
                Negate | Not => apply(1, 1),
                Equal | NotEqual | GreaterThan | LessThan | GreaterEqual | LessEqual => apply(2, 1),
                And | Or => apply(2, 1),
                Dup => apply(1, 2),
                Swap => apply(2, 2),
                Halt => break,
                CallFfi => {
                    
                    break;
                }
                _ => {
                    
                    break;
                }
            }
        }
        let min_required = (-min_depth).max(0) as usize;
        Ok(min_required)
    }

    fn translate_bytecode(
        ctx: &mut JitContext,
        bytecode: &[u8],
        gas_ptr: cranelift::prelude::Value,
        result_ptr: cranelift::prelude::Value,
        _ffi_registry_ptr: cranelift::prelude::Value,
    ) -> AnyhowResult<()> {
        let mut ip = 0;
        let mut has_terminated = false;

        while ip < bytecode.len() {
            let opcode = OpCode::try_from(bytecode[ip])?;
            ip += 1;

            
            Self::consume_gas(ctx, gas_ptr, 1)?;

            match opcode {
                OpCode::Push => OpcodeImpl::implement_push(ctx, bytecode, &mut ip)?,
                OpCode::Pop => OpcodeImpl::implement_pop(ctx)?,
                OpCode::Add => OpcodeImpl::implement_add(ctx)?,
                OpCode::Subtract => OpcodeImpl::implement_subtract(ctx)?,
                OpCode::Multiply => OpcodeImpl::implement_multiply(ctx)?,

                
                OpCode::Divide => OpcodeImpl::implement_divide(ctx)?,
                OpCode::Modulo => OpcodeImpl::implement_modulo(ctx)?,
                OpCode::Negate => OpcodeImpl::implement_negate(ctx)?,
                OpCode::Equal => OpcodeImpl::implement_equal(ctx)?,
                OpCode::NotEqual => OpcodeImpl::implement_not_equal(ctx)?,
                OpCode::GreaterThan => OpcodeImpl::implement_greater_than(ctx)?,
                OpCode::LessThan => OpcodeImpl::implement_less_than(ctx)?,
                OpCode::GreaterEqual => OpcodeImpl::implement_greater_equal(ctx)?,
                OpCode::LessEqual => OpcodeImpl::implement_less_equal(ctx)?,
                OpCode::And => OpcodeImpl::implement_and(ctx)?,
                OpCode::Or => OpcodeImpl::implement_or(ctx)?,
                OpCode::Not => OpcodeImpl::implement_not(ctx)?,
                OpCode::Dup => OpcodeImpl::implement_dup(ctx)?,
                OpCode::Swap => OpcodeImpl::implement_swap(ctx)?,

                OpCode::Halt => {
                    has_terminated = OpcodeImpl::implement_halt(ctx, result_ptr)?;
                    break;
                }

                OpCode::CallFfi => {
                    
                    return Err(anyhow::anyhow!("CallFfi encountered in JIT segment"));
                }

                _ => {
                    
                    return Err(anyhow::anyhow!("Opcode {:?} not supported in JIT", opcode));
                }
            }
        }

        
        if !has_terminated {
            let success_code = ctx.builder.ins().iconst(types::I64, 0);
            ctx.builder.ins().return_(&[success_code]);
        }

        Ok(())
    }

    fn translate_bytecode_stack(
        ctx: &mut JitContext,
        bytecode: &[u8],
        gas_ptr: cranelift::prelude::Value,
        result_ptr: cranelift::prelude::Value,
    ) -> AnyhowResult<()> {
        let mut ip = 0;

        while ip < bytecode.len() {
            let opcode = OpCode::try_from(bytecode[ip])?;
            ip += 1;

            
            Self::consume_gas(ctx, gas_ptr, 1)?;

            match opcode {
                OpCode::Push => OpcodeImpl::implement_push(ctx, bytecode, &mut ip)?,
                OpCode::Pop => OpcodeImpl::implement_pop(ctx)?,
                OpCode::Add => OpcodeImpl::implement_add(ctx)?,
                OpCode::Subtract => OpcodeImpl::implement_subtract(ctx)?,
                OpCode::Multiply => OpcodeImpl::implement_multiply(ctx)?,
                OpCode::Divide => OpcodeImpl::implement_divide(ctx)?,
                OpCode::Modulo => OpcodeImpl::implement_modulo(ctx)?,
                OpCode::Negate => OpcodeImpl::implement_negate(ctx)?,
                OpCode::Equal => OpcodeImpl::implement_equal(ctx)?,
                OpCode::NotEqual => OpcodeImpl::implement_not_equal(ctx)?,
                OpCode::GreaterThan => OpcodeImpl::implement_greater_than(ctx)?,
                OpCode::LessThan => OpcodeImpl::implement_less_than(ctx)?,
                OpCode::GreaterEqual => OpcodeImpl::implement_greater_equal(ctx)?,
                OpCode::LessEqual => OpcodeImpl::implement_less_equal(ctx)?,
                OpCode::And => OpcodeImpl::implement_and(ctx)?,
                OpCode::Or => OpcodeImpl::implement_or(ctx)?,
                OpCode::Not => OpcodeImpl::implement_not(ctx)?,
                OpCode::Dup => OpcodeImpl::implement_dup(ctx)?,
                OpCode::Swap => OpcodeImpl::implement_swap(ctx)?,
                OpCode::Halt => {
                    
                    OpcodeImpl::implement_halt_store_only(ctx, result_ptr)?;
                    break;
                }
                OpCode::CallFfi => {
                    return Err(anyhow::anyhow!("CallFfi encountered in JIT segment"));
                }
                _ => {
                    return Err(anyhow::anyhow!("Opcode {:?} not supported in JIT", opcode));
                }
            }
        }

        
        Ok(())
    }

    fn consume_gas(
        ctx: &mut JitContext,
        gas_ptr: cranelift::prelude::Value,
        amount: i64,
    ) -> AnyhowResult<()> {
        let current_gas = ctx
            .builder
            .ins()
            .load(types::I64, MemFlags::trusted(), gas_ptr, 0);
        let amount_val = ctx.builder.ins().iconst(types::I64, amount);
        let new_gas = ctx.builder.ins().isub(current_gas, amount_val);

        let out_of_gas_block = ctx.builder.create_block();
        let continue_block = ctx.builder.create_block();

        let zero = ctx.builder.ins().iconst(types::I64, 0);
        let is_out_of_gas = ctx.builder.ins().icmp(IntCC::SignedLessThan, new_gas, zero);

        ctx.builder
            .ins()
            .brif(is_out_of_gas, out_of_gas_block, &[], continue_block, &[]);

        ctx.builder.switch_to_block(out_of_gas_block);
        let error_code = ctx.builder.ins().iconst(types::I64, -1); 
        ctx.builder.ins().return_(&[error_code]);
        ctx.builder.seal_block(out_of_gas_block);

        ctx.builder.switch_to_block(continue_block);
        ctx.builder
            .ins()
            .store(MemFlags::trusted(), new_gas, gas_ptr, 0);
        ctx.builder.seal_block(continue_block);

        Ok(())
    }
}


unsafe impl Send for JitCompiler {}
unsafe impl Sync for JitCompiler {}
