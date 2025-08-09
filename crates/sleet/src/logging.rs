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
use tracing::{debug, error, info};
#[derive(Debug, Clone)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}
pub fn log_transpiler_event(event: &str, payload: Value) {
    debug!(
        event = event,
        payload = %serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()),
        "Transpiler event"
    );
}
pub fn log_interpreter_event(event: &str, payload: Value) {
    debug!(
        event = event,
        payload = %serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()),
        "Interpreter event"
    );
}
pub fn log_runtime_event(event: &str, payload: Value) {
    debug!(
        event = event,
        payload = %serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()),
        "Runtime event"
    );
}
pub fn log_execution_step(block_id: &str, operation: &str, gas_remaining: u64) {
    debug!(
        block_id = block_id,
        operation = operation,
        gas_remaining = gas_remaining,
        "Execution step"
    );
}
pub fn log_error(context: &str, error: &dyn std::error::Error) {
    error!(
        context = context,
        error = %error,
        "Execution error"
    );
}
pub fn log_performance_metric(metric_name: &str, value: f64, unit: &str) {
    info!(
        metric = metric_name,
        value = value,
        unit = unit,
        "Performance metric"
    );
}
