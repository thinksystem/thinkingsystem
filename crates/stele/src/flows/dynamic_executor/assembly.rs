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

use super::function::DynamicFunction;
use crate::blocks::rules::BlockError;
use serde_json::Value;
use std::error::Error as StdError;
use std::sync::Arc;
use wasmtime::{Config, Engine, Func, Instance, Module, Store, Val};
pub const MAX_EXECUTION_CYCLES: u64 = 100_000_000; 

fn fuel_limit() -> u64 {
    
    
    std::env::var("STELE_WASM_FUEL")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .or_else(|| std::env::var("WASM_FUEL").ok().and_then(|v| v.parse::<u64>().ok()))
        .or_else(|| std::env::var("FLOW_WASM_FUEL").ok().and_then(|v| v.parse::<u64>().ok()))
        .unwrap_or(MAX_EXECUTION_CYCLES)
}
#[derive(Clone)]
pub struct AssemblyGenerator {
    engine: Engine,
}
impl AssemblyGenerator {
    pub fn new() -> Result<Self, BlockError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config).map_err(|e| {
            BlockError::ProcessingError(format!("Failed to create Wasmtime engine: {e}"))
        })?;
        Ok(Self { engine })
    }
    pub fn compile_function(
        &self,
        wasm_bytes: &[u8],
        exported_fn_name: &str,
        source_code: &str,
    ) -> Result<DynamicFunction, BlockError> {
        let version = format!("v{}", chrono::Utc::now().timestamp());
        let engine = self.engine.clone();
        
        if let Err(e) = Module::validate(&engine, wasm_bytes) {
            let chain = Self::format_error_chain(&e);
            return Err(BlockError::ProcessingError(format!(
                "Wasm validation failed: {chain}\n--- snippet ---\n{}\n--------------",
                &source_code.lines().take(12).collect::<Vec<_>>().join("\n")
            )));
        }
        let module = Module::new(&engine, wasm_bytes).map_err(|e| {
            let chain = Self::format_error_chain(&e);
            BlockError::ProcessingError(format!(
                "Failed to compile Wasm module: {chain}\n--- snippet ---\n{}\n--------------",
                &source_code.lines().take(12).collect::<Vec<_>>().join("\n")
            ))
        })?;
        let exported_fn_name_owned = exported_fn_name.to_string();
        let compiled_fn = Arc::new(move |args: &[Value]| -> Result<Value, BlockError> {
            
            let (numbers, _, _) = Self::marshal_args(args);
            let input_arg = numbers.first().cloned().unwrap_or(0.0);
            let mut store = Store::new(&engine, ());
            store.set_fuel(fuel_limit()).map_err(|e| {
                BlockError::ProcessingError(format!("Failed to set Wasm fuel: {e}"))
            })?;
            let instance = Instance::new(&mut store, &module, &[]).map_err(|e| {
                BlockError::ProcessingError(format!("Failed to instantiate Wasm module: {e}"))
            })?;

            
            let func: Func = instance
                .get_func(&mut store, &exported_fn_name_owned)
                .ok_or_else(|| {
                    BlockError::ProcessingError(format!(
                        "Exported function '{exported_fn_name_owned}' not found"
                    ))
                })?;
            let ty = func.ty(&store);
            let params: Vec<_> = ty.params().collect();
            let results: Vec<_> = ty.results().collect();
            if params.len() > 1 {
                return Err(BlockError::ProcessingError(format!(
                    "Unsupported param count {} (only 0 or 1 supported)",
                    params.len()
                )));
            }
            if results.len() > 1 {
                return Err(BlockError::ProcessingError(format!(
                    "Unsupported result count {} (only 0 or 1 supported)",
                    results.len()
                )));
            }
            let mut call_args: Vec<Val> = Vec::new();
            if let Some(p) = params.first() {
                let v = match p {
                    wasmtime::ValType::I32 => Val::I32(input_arg as i32),
                    wasmtime::ValType::I64 => Val::I64(input_arg as i64),
                    wasmtime::ValType::F32 => Val::F32((input_arg as f32).to_bits()),
                    wasmtime::ValType::F64 => Val::F64(input_arg.to_bits()),
                    other => {
                        return Err(BlockError::ProcessingError(format!(
                            "Unsupported param type {other:?}"
                        )))
                    }
                };
                call_args.push(v);
            }
            let mut result_vals: Vec<Val> = vec![Val::I32(0); results.len()];
            match func.call(&mut store, &call_args, &mut result_vals) {
                Ok(()) => {
                    if let Some(first) = result_vals.first() {
                        let num = match first {
                            Val::I32(v) => *v as f64,
                            Val::I64(v) => *v as f64,
                            Val::F32(bits) => f32::from_bits(*bits) as f64,
                            Val::F64(bits) => f64::from_bits(*bits),
                            _ => 0.0,
                        };
                        Ok(Self::unmarshal_result(num))
                    } else {
                        Ok(Self::unmarshal_result(0.0))
                    }
                }
                Err(e) => Self::map_wasm_err(e, &store),
            }
        });
        Ok(DynamicFunction::new(
            compiled_fn,
            version,
            source_code.to_string(),
        ))
    }

    
    pub fn compile_and_wrap(
        &self,
        wasm_bytes: &[u8],
        exported_fn_name: &str,
        source_code: &str,
    ) -> Result<DynamicFunction, BlockError> {
        self.compile_function(wasm_bytes, exported_fn_name, source_code)
    }
    fn marshal_args(args: &[Value]) -> (Vec<f64>, Vec<String>, Vec<bool>) {
        let mut numbers = Vec::new();
        let mut strings = Vec::new();
        let mut bools = Vec::new();
        for arg in args {
            match arg {
                Value::Number(n) => numbers.push(n.as_f64().unwrap_or(0.0)),
                Value::String(s) => strings.push(s.clone()),
                Value::Bool(b) => bools.push(*b),
                _ => {}
            }
        }
        (numbers, strings, bools)
    }
    fn unmarshal_result(value: f64) -> Value {
        Value::Number(
            serde_json::Number::from_f64(value).unwrap_or_else(|| serde_json::Number::from(0)),
        )
    }

    fn format_error_chain(e: &wasmtime::Error) -> String {
        let mut out = e.to_string();
        let mut current: Option<&dyn StdError> = e.source();
        while let Some(src) = current {
            out.push_str(" | cause: ");
            out.push_str(&src.to_string());
            current = src.source();
        }
        
        let debug_repr = format!("{e:?}");
        if debug_repr != out {
            out.push_str(" | debug: ");
            out.push_str(&debug_repr);
        }
        out
    }

    fn map_wasm_err<E: std::fmt::Display>(e: E, store: &Store<()>) -> Result<Value, BlockError> {
        let error_msg = e.to_string();
        if error_msg.contains("all fuel consumed")
            || error_msg.contains("fuel")
            || error_msg.contains("interrupt")
            || store.get_fuel().unwrap_or(1) == 0
        {
            return Err(BlockError::ProcessingError(
                "Function execution timeout".into(),
            ));
        }
        Err(BlockError::ProcessingError(format!(
            "Wasm execution error: {error_msg}"
        )))
    }
}
