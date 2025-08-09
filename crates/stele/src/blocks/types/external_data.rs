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

use crate::blocks::base::BaseBlock;
use crate::blocks::rules::{BlockBehaviour, BlockError, BlockResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use tracing::{debug, warn};

const DEFAULT_OUTPUT_KEY: &str = "external_data";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathDiscoveryResult {
    pub found_paths: Vec<String>,
    pub suggested_path: Option<String>,
    pub raw_response: Value,
    pub structure_analysis: HashMap<String, Value>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct ExternalDataBlock {
    #[serde(flatten)]
    base: BaseBlock,
}
impl ExternalDataBlock {
    pub fn new(id: String, properties: HashMap<String, serde_json::Value>) -> Self {
        Self {
            base: BaseBlock::new(id, properties),
        }
    }

    fn analyse_json_structure(json: &Value, prefix: &str) -> Vec<String> {
        let mut paths = Vec::new();

        match json {
            Value::Object(map) => {
                for (key, value) in map {
                    let current_path = if prefix.is_empty() {
                        format!("/{key}")
                    } else {
                        format!("{prefix}/{key}")
                    };

                    paths.push(current_path.clone());

                    if matches!(value, Value::Object(_) | Value::Array(_)) {
                        paths.extend(Self::analyse_json_structure(value, &current_path));
                    }
                }
            }
            Value::Array(arr) => {
                if !arr.is_empty() {
                    for (idx, item) in arr.iter().take(3).enumerate() {
                        let indexed_path = format!("{prefix}/{idx}");
                        paths.push(indexed_path.clone());

                        if matches!(item, Value::Object(_) | Value::Array(_)) {
                            paths.extend(Self::analyse_json_structure(item, &indexed_path));
                        }
                    }
                }
            }
            _ => {}
        }

        paths
    }

    pub fn discover_data_paths(json: &Value) -> PathDiscoveryResult {
        let all_paths = Self::analyse_json_structure(json, "");

        let common_patterns = vec![
            "/message",
            "/data",
            "/result",
            "/results",
            "/items",
            "/records",
            "/response",
            "/payload",
            "/content",
            "/body",
            "/data/items",
            "/result/records",
            "/response/data",
        ];

        let mut found_paths = Vec::new();
        let mut suggested_path = None;

        for pattern in &common_patterns {
            if json.pointer(pattern).is_some() {
                found_paths.push(pattern.to_string());
                if suggested_path.is_none() {
                    suggested_path = Some(pattern.to_string());
                }
            }
        }

        for path in &all_paths {
            if !found_paths.contains(path) {
                found_paths.push(path.clone());

                if suggested_path.is_none() && path.split('/').count() <= 3 {
                    if let Some(Value::Array(_) | Value::String(_) | Value::Number(_)) =
                        json.pointer(path)
                    {
                        suggested_path = Some(path.clone());
                    }
                }
            }
        }

        let mut structure_analysis = HashMap::new();
        structure_analysis.insert(
            "total_paths".to_string(),
            Value::Number(found_paths.len().into()),
        );
        structure_analysis.insert(
            "root_type".to_string(),
            Value::String(
                match json {
                    Value::Object(_) => "object",
                    Value::Array(_) => "array",
                    Value::String(_) => "string",
                    Value::Number(_) => "number",
                    Value::Bool(_) => "boolean",
                    Value::Null => "null",
                }
                .to_string(),
            ),
        );

        if let Value::Object(map) = json {
            let keys: Vec<String> = map.keys().cloned().collect();
            structure_analysis.insert(
                "root_keys".to_string(),
                Value::Array(keys.into_iter().map(Value::String).collect()),
            );
        }

        PathDiscoveryResult {
            found_paths,
            suggested_path,
            raw_response: json.clone(),
            structure_analysis,
        }
    }

    pub fn extract_data_with_fallbacks(
        json: &Value,
        data_path: &str,
    ) -> Result<(Value, Option<PathDiscoveryResult>), BlockError> {
        if let Some(data) = json.pointer(data_path) {
            debug!(
                "Successfully extracted data using specified path: {}",
                data_path
            );
            return Ok((data.clone(), None));
        }

        warn!(
            "Specified data path '{}' not found, attempting path discovery",
            data_path
        );

        let discovery = Self::discover_data_paths(json);

        if let Some(suggested_path) = &discovery.suggested_path {
            if let Some(data) = json.pointer(suggested_path) {
                debug!(
                    "Successfully extracted data using suggested path: {}",
                    suggested_path
                );
                return Ok((data.clone(), Some(discovery)));
            }
        }

        if let Value::Object(map) = json {
            if map.len() <= 5 {
                debug!("Returning entire response as fallback (small object)");
                return Ok((json.clone(), Some(discovery)));
            }
        }

        Err(BlockError::DataPathNotFound(format!(
            "Data path '{}' not found. Discovery found {} possible paths: {}. Suggested: {:?}",
            data_path,
            discovery.found_paths.len(),
            discovery.found_paths.join(", "),
            discovery.suggested_path
        )))
    }
}
impl BlockBehaviour for ExternalDataBlock {
    fn id(&self) -> &str {
        &self.base.id
    }
    fn process<'life0, 'async_trait>(
        &'life0 self,
        state: &'life0 mut HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<BlockResult, BlockError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            if let Some(raw_json) = state.get("raw_json_response") {
                let requested_path = state
                    .get("requested_data_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("/");
                let api_url = state
                    .get("api_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                let output_key = self
                    .base
                    .get_optional_string("output_key")?
                    .unwrap_or_else(|| DEFAULT_OUTPUT_KEY.to_string());

                match Self::extract_data_with_fallbacks(raw_json, requested_path) {
                    Ok((data_to_insert, discovery_result)) => {
                        if let Some(discovery) = discovery_result {
                            if discovery.suggested_path.is_some() {
                                warn!(
                                    "Used path discovery for {}: original path '{}' failed, used suggested path '{:?}' instead",
                                    api_url, requested_path, discovery.suggested_path
                                );
                                debug!("Available paths: {}", discovery.found_paths.join(", "));

                                state.insert(
                                    "path_discovery_metadata".to_string(),
                                    serde_json::to_value(discovery).unwrap_or_default(),
                                );
                            }
                        }

                        state.insert(output_key, data_to_insert);

                        state.remove("raw_json_response");
                        state.remove("requested_data_path");
                        state.remove("api_url");

                        return Ok(BlockResult::Success(Value::String(
                            "External data processed with enhanced path discovery".to_string(),
                        )));
                    }
                    Err(e) => return Err(e),
                }
            }

            let url = self.base.get_required_string("api_url")?;
            let data_path = self.base.get_required_string("data_path")?;
            let next_block = self.base.get_required_string("next_block")?;
            let output_key = self
                .base
                .get_optional_string("output_key")?
                .unwrap_or_else(|| DEFAULT_OUTPUT_KEY.to_string());
            Ok(BlockResult::FetchExternalData {
                url,
                data_path,
                output_key,
                next_block,
                priority: self.base.priority,
                is_override: self.base.is_override,
            })
        })
    }
    fn clone_box(&self) -> Box<dyn BlockBehaviour> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn validate(&self) -> Result<(), BlockError> {
        self.base.get_required_string("api_url")?;
        self.base.get_required_string("data_path")?;
        self.base.get_required_string("next_block")?;
        self.base.get_optional_string("output_key")?;
        self.base.get_optional_f64("priority")?;
        self.base.get_optional_bool("is_override")?;
        Ok(())
    }
}
