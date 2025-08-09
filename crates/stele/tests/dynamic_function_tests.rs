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

use serde_json::{json, Value};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use stele::blocks::rules::BlockError;
use stele::flows::dynamic_executor::function::DynamicFunction;

fn create_success_function(version: &str) -> DynamicFunction {
    let func = Arc::new(|args: &[Value]| -> Result<Value, BlockError> {
        let arg1 = args.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
        Ok(json!(arg1 + 10.0))
    });
    DynamicFunction::new(func, version.to_string(), "source".to_string())
}

fn create_failure_function(version: &str) -> DynamicFunction {
    let func = Arc::new(|_: &[Value]| -> Result<Value, BlockError> {
        Err(BlockError::ProcessingError(
            "Intentional failure".to_string(),
        ))
    });
    DynamicFunction::new(func, version.to_string(), "source".to_string())
}

fn create_slow_function(version: &str, duration: Duration) -> DynamicFunction {
    let func = Arc::new(move |_: &[Value]| -> Result<Value, BlockError> {
        thread::sleep(duration);
        Ok(json!("done"))
    });
    DynamicFunction::new(func, version.to_string(), "source".to_string())
}

#[test]
fn test_new_dynamic_function() {
    let func = create_success_function("1.0");
    assert_eq!(func.get_version(), "1.0");
    assert_eq!(func.source_code, "source");
    assert_eq!(func.get_call_count(), 0);
    assert!(func.get_metadata().is_empty());
    assert!(func.get_dependencies().is_empty());
}

#[test]
fn test_execute_success_and_metrics() {
    let func = create_success_function("1.0");
    let result = func.execute(&[json!(5.0)]).unwrap();
    assert_eq!(result, json!(15.0));
    assert_eq!(func.get_call_count(), 1);
    assert_eq!(func.get_error_count(), 0);
    assert_eq!(func.get_success_rate(), 1.0);
    assert!(func.get_average_execution_time() > Duration::from_nanos(0));
    assert!(func.get_last_executed().is_some());
}

#[test]
fn test_execute_failure_and_metrics() {
    let func = create_failure_function("1.0");
    let result = func.execute(&[]);
    assert!(result.is_err());
    assert_eq!(func.get_call_count(), 1);
    assert_eq!(func.get_error_count(), 1);
    assert_eq!(func.get_success_rate(), 0.0);
    assert!(func.has_failures());
    assert_eq!(func.get_failure_rate(), 1.0);
}

#[tokio::test]
async fn test_execute_with_timeout_success() {
    let func = create_slow_function("1.0", Duration::from_millis(10));
    let result = func
        .execute_with_timeout(&[], Duration::from_millis(100))
        .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), json!("done"));
    assert_eq!(func.get_call_count(), 1);
    assert_eq!(func.get_error_count(), 0);
}

#[tokio::test]
async fn test_execute_with_timeout_failure() {
    let func = create_slow_function("1.0", Duration::from_millis(100));
    let result = func
        .execute_with_timeout(&[], Duration::from_millis(10))
        .await;
    assert!(result.is_err());
    match result.unwrap_err() {
        BlockError::ProcessingError(msg) => assert_eq!(msg, "Function execution timeout"),
        _ => panic!("Wrong error type"),
    }
    assert_eq!(func.get_call_count(), 1);
    assert_eq!(func.get_error_count(), 1);
}

#[tokio::test]
async fn test_execute_safe_wrappers() {
    let success_func = create_success_function("1.0");
    let failure_func = create_failure_function("1.0");

    assert_eq!(success_func.execute_safe(&[json!(20.0)]), json!(30.0));
    assert_eq!(failure_func.execute_safe(&[]), json!(0));

    assert_eq!(
        success_func
            .execute_safe_with_timeout(&[json!(20.0)], Duration::from_secs(1))
            .await,
        json!(30.0)
    );
    assert_eq!(
        failure_func
            .execute_safe_with_timeout(&[], Duration::from_secs(1))
            .await,
        json!(0)
    );
}

#[test]
fn test_metadata_management() {
    let mut func = create_success_function("1.0");
    func.set_metadata("key1".to_string(), json!("value1"));
    func.set_metadata("key2".to_string(), json!(42));

    assert_eq!(func.get_metadata().get("key1"), Some(&json!("value1")));
    assert_eq!(func.get_metadata().get("key2"), Some(&json!(42)));
    assert_eq!(func.get_metadata().len(), 2);
}

#[test]
fn test_dependency_management() {
    let mut func = create_success_function("1.0");
    func.add_dependency("dep1".to_string());
    func.add_dependency("dep2".to_string());
    func.add_dependency("dep1".to_string());

    assert!(func.has_dependency("dep1"));
    assert!(func.has_dependency("dep2"));
    assert!(!func.has_dependency("dep3"));
    assert_eq!(func.get_dependencies().len(), 2);

    func.remove_dependency("dep1");
    assert!(!func.has_dependency("dep1"));
    assert!(func.has_dependency("dep2"));
    assert_eq!(func.get_dependencies().len(), 1);
}

#[test]
fn test_reset_performance_metrics() {
    let func = create_success_function("1.0");
    let _result = func.execute(&[json!(1.0)]);
    assert_eq!(func.get_call_count(), 1);

    func.reset_performance_metrics();
    assert_eq!(func.get_call_count(), 0);
    assert_eq!(func.get_error_count(), 0);
    assert_eq!(func.get_success_rate(), 0.0);
}

#[test]
fn test_get_error_statistics() {
    let func = create_failure_function("1.0");
    let _result = func.execute(&[]);
    let stats = func.get_error_statistics().unwrap();
    assert_eq!(stats.0, 1);
    assert_eq!(stats.1, 1);
    assert_eq!(stats.2, 0.0);
}

#[test]
fn test_peak_memory_usage() {
    let func = create_success_function("1.0");
    func.update_peak_memory_usage(1024);
    assert_eq!(func.get_peak_memory_usage(), 1024);

    func.update_peak_memory_usage(512);
    assert_eq!(func.get_peak_memory_usage(), 1024);

    func.update_peak_memory_usage(2048);
    assert_eq!(func.get_peak_memory_usage(), 2048);
}

#[test]
fn test_clone_with_new_version() {
    let original = create_success_function("1.0");
    let cloned = original.clone_with_new_version("2.0".to_string());

    assert_eq!(original.get_version(), "1.0");
    assert_eq!(cloned.get_version(), "2.0");
    assert_eq!(cloned.source_code, original.source_code);
    assert_eq!(cloned.get_call_count(), 0);
}

#[tokio::test]
async fn test_concurrent_execution_metrics() {
    let func = Arc::new(create_success_function("1.0"));
    let mut handles = Vec::new();

    for i in 0..10 {
        let func_clone = func.clone();
        let handle = tokio::spawn(async move { func_clone.execute(&[json!(i as f64)]).unwrap() });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    assert_eq!(func.get_call_count(), 10);
    assert_eq!(func.get_error_count(), 0);
    assert_eq!(func.get_success_rate(), 1.0);
}
