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
use std::sync::Arc;
use wasmtime::{Config, Engine, Instance, Module, Store};
pub const MAX_EXECUTION_CYCLES: u64 = 1_000_000;
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
        let module = Module::new(&engine, wasm_bytes).map_err(|e| {
            BlockError::ProcessingError(format!("Failed to compile Wasm module: {e}"))
        })?;
        let exported_fn_name_owned = exported_fn_name.to_string();
        let compiled_fn = Arc::new(move |args: &[Value]| -> Result<Value, BlockError> {
            if args.is_empty() {
                return Err(BlockError::ProcessingError(
                    "Insufficient arguments provided".into(),
                ));
            }
            let (numbers, _, _) = Self::marshal_args(args);
            let input_arg = numbers.first().cloned().unwrap_or(0.0);
            let mut store = Store::new(&engine, ());
            store.set_fuel(MAX_EXECUTION_CYCLES).map_err(|e| {
                BlockError::ProcessingError(format!("Failed to set Wasm fuel: {e}"))
            })?;
            let instance = Instance::new(&mut store, &module, &[]).map_err(|e| {
                BlockError::ProcessingError(format!("Failed to instantiate Wasm module: {e}"))
            })?;
            let wasm_func = instance
                .get_typed_func::<f64, f64>(&mut store, &exported_fn_name_owned)
                .map_err(|e| {
                    BlockError::ProcessingError(format!("Exported function '{exported_fn_name_owned}' not found or has wrong signature: {e}"))
                })?;
            let result = wasm_func.call(&mut store, input_arg);
            match result {
                Ok(value) => Ok(Self::unmarshal_result(value)),
                Err(e) => {
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
                        "Wasm execution error: {e}"
                    )))
                }
            }
        });
        Ok(DynamicFunction::new(
            compiled_fn,
            version,
            source_code.to_string(),
        ))
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
}
