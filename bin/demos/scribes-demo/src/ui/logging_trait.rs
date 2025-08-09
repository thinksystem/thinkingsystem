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

use crate::ui::core::UIBridge;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;

pub trait UILogged {
    fn get_ui_bridge(&self) -> &Arc<UIBridge>;
    fn get_component_name(&self) -> &str;

    fn log_operation_start(&self, operation: &str, input: &Value, is_llm_call: bool) {
        self.get_ui_bridge().log_scribe_operation(
            self.get_component_name(),
            operation,
            input.clone(),
            None,
            None,
            is_llm_call,
        );
    }

    fn log_operation_complete(
        &self,
        operation: &str,
        input: &Value,
        result: &Result<Value, String>,
        processing_time: u128,
        is_llm_call: bool,
    ) {
        let output = match result {
            Ok(value) => Some(value.clone()),
            Err(e) => Some(serde_json::json!({"error": e.to_string()})),
        };

        self.get_ui_bridge().log_scribe_operation(
            self.get_component_name(),
            operation,
            input.clone(),
            output,
            Some(processing_time),
            is_llm_call,
        );
    }

    async fn log_async_operation<F, T, E>(
        &self,
        operation: &str,
        input: &Value,
        is_llm_call: bool,
        op: F,
    ) -> Result<T, E>
    where
        F: std::future::Future<Output = Result<T, E>>,
        T: serde::Serialize,
        E: ToString,
    {
        let start_time = Instant::now();
        self.log_operation_start(operation, input, is_llm_call);

        let result = op.await;
        let processing_time = start_time.elapsed().as_millis();

        let log_result = match &result {
            Ok(value) => {
                Ok(serde_json::to_value(value).unwrap_or(serde_json::json!({"success": true})))
            }
            Err(e) => Err(e.to_string()),
        };

        self.log_operation_complete(operation, input, &log_result, processing_time, is_llm_call);

        result
    }

    fn log_sync_operation<F, T, E>(
        &self,
        operation: &str,
        input: &Value,
        is_llm_call: bool,
        op: F,
    ) -> Result<T, E>
    where
        F: FnOnce() -> Result<T, E>,
        T: serde::Serialize,
        E: ToString,
    {
        let start_time = Instant::now();
        self.log_operation_start(operation, input, is_llm_call);

        let result = op();
        let processing_time = start_time.elapsed().as_millis();

        let log_result = match &result {
            Ok(value) => {
                Ok(serde_json::to_value(value).unwrap_or(serde_json::json!({"success": true})))
            }
            Err(e) => Err(e.to_string()),
        };

        self.log_operation_complete(operation, input, &log_result, processing_time, is_llm_call);

        result
    }
}
