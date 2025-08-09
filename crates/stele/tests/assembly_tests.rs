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

use serde_json::Value;
use stele::blocks::rules::BlockError;
use stele::flows::dynamic_executor::assembly::AssemblyGenerator;

#[test]
fn test_wasm_simple_add() {
    let wat = r#"
    (module
        (func $add (param $a f64) (param $b f64) (result f64)
            local.get $a
            local.get $b
            f64.add)
        (func (export "execute") (param f64) (result f64)
            local.get 0
            f64.const 5.0
            call $add)
    )"#;
    let wasm_bytes = wat::parse_str(wat).unwrap();
    let generator = AssemblyGenerator::new().unwrap();
    let function = generator
        .compile_function(&wasm_bytes, "execute", wat)
        .unwrap();
    let args = [Value::Number(serde_json::Number::from_f64(10.0).unwrap())];
    let result = function.execute(&args).unwrap();
    assert_eq!(
        result,
        Value::Number(serde_json::Number::from_f64(15.0).unwrap())
    );
}

#[test]
fn test_wasm_timeout_with_infinite_loop() {
    let wat = r#"
    (module
        (func $loop (loop br 0))
        (func (export "execute") (param f64) (result f64)
            call $loop
            f64.const 0)
    )"#;
    let wasm_bytes = wat::parse_str(wat).unwrap();
    let generator = AssemblyGenerator::new().unwrap();
    let function = generator
        .compile_function(&wasm_bytes, "execute", wat)
        .unwrap();
    let args = [Value::Number(serde_json::Number::from_f64(0.0).unwrap())];
    let result = function.execute(&args);
    assert!(result.is_err());
    let error = result.unwrap_err();
    match error {
        BlockError::ProcessingError(msg) => {
            assert!(
                msg.contains("Function execution timeout"),
                "Error message should indicate a timeout, but was: {msg}"
            );
        }
        _ => panic!("Expected a ProcessingError"),
    }
}
