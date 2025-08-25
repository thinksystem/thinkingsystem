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


use anyhow::Context;
use serde_json::Value;



pub async fn assert_flow_functions_available(
    plan: &Value,
    engine: &mut stele::UnifiedFlowEngine,
) -> anyhow::Result<()> {
    let Some(flow) = plan.get("flow").and_then(|f| f.as_object()) else {
        return Ok(());
    };
    let Some(blocks) = flow.get("blocks").and_then(|b| b.as_array()) else {
        return Ok(());
    };
    let mut missing: Vec<String> = Vec::new();
    for b in blocks {
        if b.get("type").and_then(|v| v.as_str()) == Some("compute") {
            if let Some(expr) = b.get("expression").and_then(|v| v.as_str()) {
                if let Some(rest) = expr.strip_prefix("function:") {
                    let fn_name = rest.trim();
                    if engine.get_dynamic_function(fn_name).await.is_none() {
                        missing.push(fn_name.to_string());
                    }
                }
            }
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Flow references missing dynamic functions: {}",
            missing.join(", ")
        ))
        .with_context(|| "preflight flow check failed")
    }
}
