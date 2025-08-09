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

use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use stele::blocks::rules::BlockType;
use stele::flows::{
    engine::UnifiedFlowEngine,
    flows::{BlockDefinition, FlowDefinition},
};
use tracing::{error, info, warn};

pub struct FlowValidator {}

impl FlowValidator {
    pub async fn new() -> Result<Self> {
        Ok(Self {})
    }

    pub async fn validate_template_file(&self, template_path: &str) -> Result<ValidationReport> {
        info!("Validating template file: {}", template_path);

        let template_content = std::fs::read_to_string(template_path)?;
        let template_json: Value = serde_json::from_str(&template_content)?;

        let mut report = ValidationReport {
            file_path: template_path.to_string(),
            flows: Vec::new(),
            overall_valid: true,
        };

        if let Some(flows_obj) = template_json.as_object() {
            for (flow_name, flow_data) in flows_obj {
                info!("Validating flow: {}", flow_name);

                let flow_result = self.validate_single_flow(flow_name, flow_data).await;
                match flow_result {
                    Ok(flow_report) => {
                        if !flow_report.valid {
                            report.overall_valid = false;
                        }
                        report.flows.push(flow_report);
                    }
                    Err(e) => {
                        error!("❌ Failed to validate flow {}: {}", flow_name, e);
                        report.overall_valid = false;
                        report.flows.push(FlowValidationReport {
                            name: flow_name.clone(),
                            valid: false,
                            errors: vec![format!("Validation failed: {e}")],
                            warnings: Vec::new(),
                            block_count: 0,
                        });
                    }
                }
            }
        }

        Ok(report)
    }

    async fn validate_single_flow(
        &self,
        flow_name: &str,
        flow_data: &Value,
    ) -> Result<FlowValidationReport> {
        let mut report = FlowValidationReport {
            name: flow_name.to_string(),
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            block_count: 0,
        };

        let flow_name = flow_data["name"].as_str().unwrap_or(flow_name);
        let start_block_id = flow_data["start_block_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing start_block_id"))?;

        let blocks_array = flow_data["blocks"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Missing blocks array"))?;

        report.block_count = blocks_array.len();

        let mut block_definitions = Vec::new();
        let mut block_ids = std::collections::HashSet::new();

        for (i, block_data) in blocks_array.iter().enumerate() {
            match self.validate_block(block_data, i) {
                Ok((block_def, block_id)) => {
                    if block_ids.contains(&block_id) {
                        report
                            .errors
                            .push(format!("Duplicate block ID: {block_id}"));
                        report.valid = false;
                    } else {
                        block_ids.insert(block_id);
                        block_definitions.push(block_def);
                    }
                }
                Err(e) => {
                    report.errors.push(format!("Block {i}: {e}"));
                    report.valid = false;
                }
            }
        }

        if !block_ids.contains(start_block_id) {
            report.errors.push(format!(
                "start_block_id '{start_block_id}' not found in blocks"
            ));
            report.valid = false;
        }

        if report.valid {
            let flow_def = FlowDefinition {
                id: format!("validation_{}", flow_name.replace(" ", "_")),
                name: flow_name.to_string(),
                start_block_id: start_block_id.to_string(),
                blocks: block_definitions,
            };

            info!("✅ Flow '{}' structure is valid", flow_name);
        } else {
            warn!("❌ Flow '{}' has validation errors", flow_name);
        }

        Ok(report)
    }

    fn validate_block(
        &self,
        block_data: &Value,
        index: usize,
    ) -> Result<(BlockDefinition, String)> {
        let block_id = block_data["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing block id"))?
            .to_string();

        let block_type_str = block_data["type"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing block type"))?;

        let block_type = match block_type_str {
            "Display" => BlockType::Display,
            "Input" => BlockType::Input,
            "InputBlock" => BlockType::Input,
            "Compute" => BlockType::Compute,
            "ComputeBlock" => BlockType::Compute,
            "ExternalData" => BlockType::ExternalData,
            "Conditional" => BlockType::Conditional,
            "ConditionBlock" => BlockType::Conditional,
            "Decision" => BlockType::Decision,
            "GoTo" => BlockType::GoTo,
            "Random" => BlockType::Random,
            "Interactive" => BlockType::Interactive,

            "EndBlock" => BlockType::Display,
            "EventBlock" => BlockType::ExternalData,
            "OAuthProvider" => BlockType::ExternalData,

            _ => {
                warn!(
                    "Unknown block type '{}' for block '{}' - this may cause runtime issues",
                    block_type_str, block_id
                );
                BlockType::Display
            }
        };

        let mut properties = HashMap::new();
        if let Some(props) = block_data["properties"].as_object() {
            for (key, value) in props {
                properties.insert(key.clone(), value.clone());
            }
        }

        let block_def = BlockDefinition {
            id: block_id.clone(),
            block_type,
            properties,
        };

        Ok((block_def, block_id))
    }
}

#[derive(Debug)]
pub struct ValidationReport {
    pub file_path: String,
    pub flows: Vec<FlowValidationReport>,
    pub overall_valid: bool,
}

#[derive(Debug)]
pub struct FlowValidationReport {
    pub name: String,
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub block_count: usize,
}

impl ValidationReport {
    pub fn print_summary(&self) {
        info!("Validation Report for: {}", self.file_path);
        info!(
            "Overall Status: {}",
            if self.overall_valid {
                "✅ VALID"
            } else {
                "❌ INVALID"
            }
        );

        for flow in &self.flows {
            info!(
                "Flow '{}': {} ({} blocks)",
                flow.name,
                if flow.valid { "✅" } else { "❌" },
                flow.block_count
            );

            for error in &flow.errors {
                error!("Error: {}", error);
            }

            for warning in &flow.warnings {
                warn!("Warning: {}", warning);
            }
        }
    }
}
