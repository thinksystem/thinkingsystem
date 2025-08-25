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



use serde_json::json;
use stele::flows::dynamic_executor::{DynamicExecutor, DynamicSource};

fn setup() -> DynamicExecutor {
    DynamicExecutor::new().expect("executor init")
}

#[cfg(feature = "dynamic-wasi")]
#[test]
fn dynamic_rust_basic_sum_of_squares() {
    let ex = setup();
    let body = "(1..=5).map(|x| (x * x) as f64).sum::<f64>()"; 
    let dyn_fn = ex
        .register_dynamic_source(DynamicSource::RustWasiExpression {
            name: "sum_squares",
            body,
        })
        .expect("compile rust wasi expression");
    let out = dyn_fn.execute(&[json!(0.0)]).expect("execute"); 
    assert_eq!(out, json!(55.0));
}

#[cfg(feature = "dynamic-wasi")]
#[test]
fn dynamic_rust_pow_autofix_sum_cubes() {
    let ex = setup();
    
    
    let body = "(1..=10).map(|x| x.pow(3)).sum::<u64>() as f64";
    let dyn_fn = ex
        .register_dynamic_source(DynamicSource::RustWasiExpression {
            name: "sum_cubes",
            body,
        })
        .expect("compile rust wasi expression with pow fix");
    let out = dyn_fn.execute(&[json!(0.0)]).expect("execute");
    assert_eq!(out, json!(3025.0));
}


#[cfg(feature = "dynamic-native")]
#[test]
fn dynamic_rust_native_basic_sum_of_squares() {
    let ex = setup();
    let body = "(1..=5).map(|x| (x * x) as f64).sum::<f64>()"; 
    let dyn_fn = ex
        .register_dynamic_source(DynamicSource::RustExpression {
            name: "sum_squares_native",
            body,
        })
        .expect("compile rust native expression");
    let out = dyn_fn.execute(&[json!(0.0)]).expect("execute");
    assert_eq!(out, json!(55.0));
}

#[cfg(feature = "dynamic-native")]
#[test]
fn dynamic_rust_native_pow_autofix_sum_cubes() {
    let ex = setup();
    let body = "(1..=10).map(|x| x.pow(3)).sum::<u64>() as f64"; 
    let dyn_fn = ex
        .register_dynamic_source(DynamicSource::RustExpression {
            name: "sum_cubes_native",
            body,
        })
        .expect("compile rust native expression with pow fix");
    let out = dyn_fn.execute(&[json!(0.0)]).expect("execute");
    assert_eq!(out, json!(3025.0));
}
