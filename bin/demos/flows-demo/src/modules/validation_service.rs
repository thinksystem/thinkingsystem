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

#![allow(clippy::ptr_arg)]

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use stele::{
    blocks::registry::BlockRegistry,
    flows::core::{BlockDefinition, FlowDefinition},
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationError {
    pub error_type: ValidationErrorType,
    pub block_id: Option<String>,
    pub property: Option<String>,
    pub message: String,
    pub suggestion: String,
    pub severity: ErrorSeverity,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum ValidationErrorType {
    StructuralError,
    BlockValidationError,
    PropertyError,
    ReferenceError,
    TypeMismatch,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum ErrorSeverity {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationError>,
    pub flow_analysis: FlowAnalysis,
}

#[derive(Debug)]
pub struct FlowAnalysis {
    pub total_blocks: usize,
    pub block_types_used: HashMap<String, usize>,
    pub has_termination: bool,
    pub has_user_input: bool,
    pub has_external_data: bool,
    pub complexity_score: u32,
    pub reachable_blocks: HashSet<String>,
}

pub struct UnifiedValidator {
    registry: Arc<BlockRegistry>,
}

impl UnifiedValidator {
    pub fn new(registry: Arc<BlockRegistry>) -> Self {
        Self { registry }
    }

    pub fn validate(&self, flow_def: &FlowDefinition) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        self.validate_flow_structure(flow_def, &mut errors, &mut warnings);

        self.validate_blocks_with_registry(flow_def, &mut errors, &mut warnings);

        self.validate_block_references(flow_def, &mut errors, &mut warnings);

        let flow_analysis = self.analyse_flow(flow_def);

        self.validate_best_practices(flow_def, &flow_analysis, &mut warnings);

        let is_valid = errors
            .iter()
            .all(|e| !matches!(e.severity, ErrorSeverity::Critical | ErrorSeverity::High));

        ValidationResult {
            is_valid,
            errors,
            warnings,
            flow_analysis,
        }
    }

    fn validate_flow_structure(
        &self,
        flow_def: &FlowDefinition,
        errors: &mut Vec<ValidationError>,
        _warnings: &mut Vec<ValidationError>,
    ) {
        if flow_def.blocks.is_empty() {
            errors.push(ValidationError {
                error_type: ValidationErrorType::StructuralError,
                block_id: None,
                property: None,
                message: "Flow has no blocks".to_string(),
                suggestion: "Add at least one block to the flow with a valid block type and required properties".to_string(),
                severity: ErrorSeverity::Critical,
            });
            return;
        }

        let start_block_exists = flow_def
            .blocks
            .iter()
            .any(|block| block.id == flow_def.start_block_id);

        if !start_block_exists {
            errors.push(ValidationError {
                error_type: ValidationErrorType::StructuralError,
                block_id: Some(flow_def.start_block_id.clone()),
                property: None,
                message: format!("Start block '{}' not found in flow blocks", flow_def.start_block_id),
                suggestion: format!("Ensure there is a block with id '{}' in the blocks array, or change start_block_id to reference an existing block", flow_def.start_block_id),
                severity: ErrorSeverity::Critical,
            });
        }

        let mut seen_ids = HashSet::new();
        for block in &flow_def.blocks {
            if !seen_ids.insert(&block.id) {
                errors.push(ValidationError {
                    error_type: ValidationErrorType::StructuralError,
                    block_id: Some(block.id.clone()),
                    property: None,
                    message: format!("Duplicate block ID found: '{}'", block.id),
                    suggestion: format!(
                        "Change the duplicate block ID '{}' to a unique identifier",
                        block.id
                    ),
                    severity: ErrorSeverity::Critical,
                });
            }
        }

        let has_compute_blocks = flow_def
            .blocks
            .iter()
            .any(|b| b.block_type == stele::blocks::rules::BlockType::Compute);
        let has_default_block = flow_def.blocks.iter().any(|b| b.id == "default");

        if has_compute_blocks && !has_default_block {
            errors.push(ValidationError {
                error_type: ValidationErrorType::StructuralError,
                block_id: None,
                property: None,
                message: "Flow with Compute blocks must include a 'default' termination block"
                    .to_string(),
                suggestion:
                    "Add a block with id 'default' and type 'Compute' for reliable flow termination"
                        .to_string(),
                severity: ErrorSeverity::Critical,
            });
        }
    }

    fn validate_blocks_with_registry(
        &self,
        flow_def: &FlowDefinition,
        errors: &mut Vec<ValidationError>,
        _warnings: &mut Vec<ValidationError>,
    ) {
        for block in &flow_def.blocks {
            match self.registry.create_block(
                block.block_type.clone(),
                block.id.clone(),
                block.properties.clone(),
            ) {
                Ok(_) => {
                    continue;
                }
                Err(e) => {
                    let error_message = e.to_string();

                    if error_message.contains("Missing required property") {
                        let property = self.extract_property_from_error(&error_message);
                        errors.push(ValidationError {
                            error_type: ValidationErrorType::PropertyError,
                            block_id: Some(block.id.clone()),
                            property: property.clone(),
                            message: error_message.clone(),
                            suggestion: match property {
                                Some(prop) => format!("Add the required property '{}' to block '{}'. Check the block schema for the expected data type and format.", prop, block.id),
                                None => format!("Check the block schema for block type '{:?}' and ensure all required properties are present.", block.block_type),
                            },
                            severity: ErrorSeverity::Critical,
                        });
                    } else if error_message.contains("Invalid property type") {
                        errors.push(ValidationError {
                            error_type: ValidationErrorType::TypeMismatch,
                            block_id: Some(block.id.clone()),
                            property: None,
                            message: error_message.clone(),
                            suggestion: format!("Check the data type of properties in block '{}'. Ensure strings are quoted, numbers are numeric, and booleans are true/false.", block.id),
                            severity: ErrorSeverity::Critical,
                        });
                    } else {
                        errors.push(ValidationError {
                            error_type: ValidationErrorType::BlockValidationError,
                            block_id: Some(block.id.clone()),
                            property: None,
                            message: error_message.clone(),
                            suggestion: format!("Review the block definition for '{}' and ensure it matches the expected schema for block type '{:?}'.", block.id, block.block_type),
                            severity: ErrorSeverity::Critical,
                        });
                    }
                }
            }
        }
    }

    fn validate_block_references(
        &self,
        flow_def: &FlowDefinition,
        errors: &mut Vec<ValidationError>,
        _warnings: &mut Vec<ValidationError>,
    ) {
        let block_ids: HashSet<_> = flow_def.blocks.iter().map(|b| &b.id).collect();

        for block in &flow_def.blocks {
            self.check_block_reference(&block_ids, block, "next_block", errors);
            self.check_block_reference(&block_ids, block, "target", errors);
            self.check_block_reference(&block_ids, block, "true_block", errors);
            self.check_block_reference(&block_ids, block, "false_block", errors);
        }
    }

    fn check_block_reference(
        &self,
        valid_ids: &HashSet<&String>,
        block: &BlockDefinition,
        property: &str,
        errors: &mut Vec<ValidationError>,
    ) {
        if let Some(reference_value) = block.properties.get(property) {
            if let Some(reference_id) = reference_value.as_str() {
                if reference_id == "default" || reference_id.is_empty() {
                    return;
                }

                if !valid_ids.contains(&reference_id.to_string()) {
                    errors.push(ValidationError {
                        error_type: ValidationErrorType::ReferenceError,
                        block_id: Some(block.id.clone()),
                        property: Some(property.to_string()),
                        message: format!("Block '{}' references non-existent block '{}' in property '{}'", block.id, reference_id, property),
                        suggestion: format!("Either create a block with id '{}' or change the '{}' property in block '{}' to reference an existing block", reference_id, property, block.id),
                        severity: ErrorSeverity::High,
                    });
                }
            }
        }
    }

    fn analyse_flow(&self, flow_def: &FlowDefinition) -> FlowAnalysis {
        let mut block_types_used = HashMap::new();
        let mut has_termination = false;
        let mut has_user_input = false;
        let mut has_external_data = false;
        let mut complexity_score = 0;

        for block in &flow_def.blocks {
            let block_type_str = format!("{:?}", block.block_type);
            *block_types_used.entry(block_type_str).or_insert(0) += 1;

            match block.block_type {
                stele::blocks::rules::BlockType::Input => {
                    has_user_input = true;
                    complexity_score += 2;
                }
                stele::blocks::rules::BlockType::ExternalData => {
                    has_external_data = true;
                    complexity_score += 3;
                }
                stele::blocks::rules::BlockType::Conditional => {
                    complexity_score += 2;
                }
                stele::blocks::rules::BlockType::Compute => {
                    if !block.properties.contains_key("next_block")
                        || block
                            .properties
                            .get("next_block")
                            .and_then(|v| v.as_str())
                            .is_none_or(|s| s.is_empty())
                    {
                        has_termination = true;
                    }
                    complexity_score += 1;
                }
                _ => complexity_score += 1,
            }
        }

        let reachable_blocks = self.find_reachable_blocks(flow_def);

        FlowAnalysis {
            total_blocks: flow_def.blocks.len(),
            block_types_used,
            has_termination,
            has_user_input,
            has_external_data,
            complexity_score,
            reachable_blocks,
        }
    }

    fn find_reachable_blocks(&self, flow_def: &FlowDefinition) -> HashSet<String> {
        let mut reachable = HashSet::new();
        let mut to_visit = vec![flow_def.start_block_id.clone()];

        let block_map: HashMap<_, _> = flow_def.blocks.iter().map(|b| (b.id.clone(), b)).collect();

        while let Some(current_id) = to_visit.pop() {
            if reachable.contains(&current_id) {
                continue;
            }

            reachable.insert(current_id.clone());

            if let Some(block) = block_map.get(&current_id) {
                for property in &["next_block", "target", "true_block", "false_block"] {
                    if let Some(next_id) = block.properties.get(*property).and_then(|v| v.as_str())
                    {
                        if !next_id.is_empty() && next_id != "default" {
                            to_visit.push(next_id.to_string());
                        }
                    }
                }
            }
        }

        reachable
    }

    fn validate_best_practices(
        &self,
        flow_def: &FlowDefinition,
        analysis: &FlowAnalysis,
        warnings: &mut Vec<ValidationError>,
    ) {
        for block in &flow_def.blocks {
            if !analysis.reachable_blocks.contains(&block.id) {
                warnings.push(ValidationError {
                    error_type: ValidationErrorType::StructuralError,
                    block_id: Some(block.id.clone()),
                    property: None,
                    message: format!("Block '{}' is unreachable from the flow start", block.id),
                    suggestion: format!(
                        "Either remove block '{}' or add a path to reach it from other blocks",
                        block.id
                    ),
                    severity: ErrorSeverity::Medium,
                });
            }
        }

        if !analysis.has_termination {
            warnings.push(ValidationError {
                error_type: ValidationErrorType::StructuralError,
                block_id: None,
                property: None,
                message: "Flow may not have proper termination".to_string(),
                suggestion: "Consider adding a Compute block without next_block property for clean termination, or ensure the flow has a clear end point".to_string(),
                severity: ErrorSeverity::Medium,
            });
        }

        let has_default_block = flow_def.blocks.iter().any(|b| b.id == "default");
        if !has_default_block && analysis.block_types_used.contains_key("Compute") {
            warnings.push(ValidationError {
                error_type: ValidationErrorType::StructuralError,
                block_id: None,
                property: None,
                message: "Flow with Compute blocks should include a 'default' termination block"
                    .to_string(),
                suggestion:
                    "Add a block with id 'default' and type 'Compute' for reliable flow termination"
                        .to_string(),
                severity: ErrorSeverity::Low,
            });
        }
    }

    fn extract_property_from_error(&self, error_message: &str) -> Option<String> {
        if let Some(start) = error_message.find("'") {
            if let Some(end) = error_message[start + 1..].find("'") {
                return Some(error_message[start + 1..start + 1 + end].to_string());
            }
        }
        None
    }

    pub fn generate_llm_feedback(&self, result: &ValidationResult) -> Value {
        serde_json::json!({
            "validation_status": if result.is_valid { "valid" } else { "invalid" },
            "summary": {
                "total_errors": result.errors.len(),
                "critical_errors": result.errors.iter().filter(|e| matches!(e.severity, ErrorSeverity::Critical)).count(),
                "total_warnings": result.warnings.len(),
                "flow_complexity": result.flow_analysis.complexity_score
            },
            "errors": result.errors,
            "warnings": result.warnings,
            "flow_analysis": {
                "total_blocks": result.flow_analysis.total_blocks,
                "block_types_used": result.flow_analysis.block_types_used,
                "has_termination": result.flow_analysis.has_termination,
                "has_user_input": result.flow_analysis.has_user_input,
                "has_external_data": result.flow_analysis.has_external_data,
                "reachable_blocks": result.flow_analysis.reachable_blocks.len()
            },
            "next_steps": if result.is_valid {
                "Flow is valid and ready for execution"
            } else {
                "Address the critical and high severity errors before proceeding"
            }
        })
    }
}
