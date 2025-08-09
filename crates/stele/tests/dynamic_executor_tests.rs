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
use stele::flows::dynamic_executor::executor::DynamicExecutor;

fn setup_executor() -> DynamicExecutor {
    DynamicExecutor::new().expect("Failed to create executor")
}

const ADD_ONE_WAT: &str = r#"
(module
  (func $execute (param f64) (result f64)
    local.get 0
    f64.const 1.0
    f64.add)
  (export "execute" (func $execute)))
"#;

const MULTIPLY_BY_TWO_WAT: &str = r#"
(module
  (func $execute (param f64) (result f64)
    local.get 0
    f64.const 2.0
    f64.mul)
  (export "execute" (func $execute)))
"#;

#[test]
fn test_compile_register_and_execute() {
    let executor = setup_executor();
    let function = executor.compile_function(ADD_ONE_WAT, "execute").unwrap();
    executor.register_function("add_one".to_string(), function);

    assert_eq!(executor.get_function_count(), 1);

    let result = executor
        .execute_function("add_one", &[json!(10.0)])
        .unwrap();
    assert_eq!(result, json!(11.0));
}

#[test]
fn test_get_function_by_version() {
    let executor = setup_executor();

    let mut v1 = executor.compile_function(ADD_ONE_WAT, "execute").unwrap();
    v1.version = "1.0.0".to_string();
    executor.register_function("my_func".to_string(), v1);

    let mut v2 = executor
        .compile_function(MULTIPLY_BY_TWO_WAT, "execute")
        .unwrap();
    v2.version = "2.0.0".to_string();
    executor.register_function("my_func".to_string(), v2);

    let latest_fn = executor.get_function("my_func", None).unwrap();
    assert_eq!(latest_fn.version, "2.0.0");
    let result_latest = latest_fn.execute(&[json!(10.0)]).unwrap();
    assert_eq!(result_latest, json!(20.0));

    let v1_fn = executor.get_function("my_func", Some("1.0.0")).unwrap();
    assert_eq!(v1_fn.version, "1.0.0");
    let result_v1 = v1_fn.execute(&[json!(10.0)]).unwrap();
    assert_eq!(result_v1, json!(11.0));
}

#[test]
fn test_function_composition() {
    let executor = setup_executor();

    let add_one = executor.compile_function(ADD_ONE_WAT, "execute").unwrap();
    executor.register_function("add_one".to_string(), add_one);

    let mul_two = executor
        .compile_function(MULTIPLY_BY_TWO_WAT, "execute")
        .unwrap();
    executor.register_function("mul_two".to_string(), mul_two);

    let chain = vec!["add_one".to_string(), "mul_two".to_string()];
    executor
        .compose_functions("pipeline".to_string(), chain)
        .unwrap();

    assert_eq!(executor.list_compositions(), vec!["pipeline"]);

    let result = executor
        .execute_composition("pipeline", &[json!(10.0)])
        .unwrap();
    assert_eq!(result, json!(22.0));
}

#[test]
fn test_versioning_and_rollback() {
    let executor = setup_executor();

    let v1 = executor.compile_function(ADD_ONE_WAT, "execute").unwrap();
    let v1_version = v1.version.clone();
    executor.register_function("test_func".to_string(), v1);

    let result_v1 = executor
        .execute_function("test_func", &[json!(5.0)])
        .unwrap();
    assert_eq!(result_v1, json!(6.0));

    let v2 = executor
        .compile_function(MULTIPLY_BY_TWO_WAT, "execute")
        .unwrap();
    executor.register_function("test_func".to_string(), v2);

    let result_v2 = executor
        .execute_function("test_func", &[json!(5.0)])
        .unwrap();
    assert_eq!(result_v2, json!(10.0));

    executor
        .rollback_function("test_func", &v1_version)
        .unwrap();

    let result_rolled_back = executor
        .execute_function("test_func", &[json!(5.0)])
        .unwrap();
    assert_eq!(result_rolled_back, json!(6.0));
}

#[test]
fn test_remove_function_and_cleanup_composition() {
    let executor = setup_executor();

    let add_one = executor.compile_function(ADD_ONE_WAT, "execute").unwrap();
    executor.register_function("add_one".to_string(), add_one);

    let mul_two = executor
        .compile_function(MULTIPLY_BY_TWO_WAT, "execute")
        .unwrap();
    executor.register_function("mul_two".to_string(), mul_two);

    let chain = vec!["add_one".to_string(), "mul_two".to_string()];
    executor
        .compose_functions("pipeline".to_string(), chain)
        .unwrap();

    assert!(executor.get_function("add_one", None).is_some());
    assert_eq!(executor.list_compositions(), vec!["pipeline"]);

    executor.remove_function("add_one").unwrap();

    assert!(executor.get_function("add_one", None).is_none());
    assert!(executor.list_compositions().is_empty());
}

#[test]
fn test_cleanup_old_versions() {
    let executor = setup_executor();

    for _ in 0..5 {
        let func = executor.compile_function(ADD_ONE_WAT, "execute").unwrap();
        executor.register_function("versioned_func".to_string(), func);
    }

    assert_eq!(executor.get_total_versions(), 5);

    let cleaned_count = executor.cleanup_old_versions(2);
    assert_eq!(cleaned_count, 3);
    assert_eq!(executor.get_total_versions(), 2);
}

#[test]
fn test_import_export_function() {
    let executor = setup_executor();

    let original_func = executor.compile_function(ADD_ONE_WAT, "execute").unwrap();
    executor.register_function("export_me".to_string(), original_func);

    let exported_data = executor.export_function("export_me", None).unwrap();

    executor.remove_function("export_me").unwrap();
    assert!(executor.get_function("export_me", None).is_none());

    let imported_name = executor.import_function(exported_data).unwrap();
    assert_eq!(imported_name, "export_me");
    assert!(executor.get_function("export_me", None).is_some());

    let result = executor
        .execute_function("export_me", &[json!(100.0)])
        .unwrap();
    assert_eq!(result, json!(101.0));
}
