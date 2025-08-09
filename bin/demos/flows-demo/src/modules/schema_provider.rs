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

use crate::config::ConfigLoader;
use anyhow::Result;
use serde_json::{json, Value};
use std::sync::Arc;
use stele::blocks::registry::BlockRegistry;

pub struct SchemaProvider {
    registry: Arc<BlockRegistry>,
    flow_patterns: Vec<FlowPattern>,
    config_loader: ConfigLoader,
}

#[derive(Debug, Clone)]
pub struct FlowPattern {
    pub name: String,
    pub description: String,
    pub use_case: String,
    pub template_path: String,
    pub complexity: PatternComplexity,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PatternComplexity {
    Simple,
    Moderate,
    Complex,
}

impl SchemaProvider {
    pub fn new(registry: Arc<BlockRegistry>, config_loader: ConfigLoader) -> Self {
        let mut provider = Self {
            registry,
            flow_patterns: Vec::new(),
            config_loader,
        };

        provider.load_flow_patterns();
        provider
    }

    pub fn get_block_schemas_as_json(&self) -> Value {
        let schemas = json!({
            "Display": {
                "description": "Shows a message to the user and navigates to the next block",
                "purpose": "User communication, status updates, results display",
                "properties": {
                    "message": {
                        "type": "string",
                        "required": true,
                        "description": "The message to display to the user"
                    },
                    "next_block": {
                        "type": "string",
                        "required": true,
                        "description": "ID of the next block to navigate to"
                    }
                },
                "example": {
                    "id": "welcome_message",
                    "type": "Display",
                    "properties": {
                        "message": "Welcome to the system!",
                        "next_block": "get_user_input"
                    }
                }
            },
            "Input": {
                "description": "Requests input from the user and waits for response",
                "purpose": "Gathering user data, interactive workflows",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "required": true,
                        "description": "The prompt shown to request user input"
                    }
                },
                "example": {
                    "id": "get_appointment_details",
                    "type": "Input",
                    "properties": {
                        "prompt": "Please provide your appointment details"
                    }
                },
                "notes": "Input blocks automatically await user response and halt flow execution until input is received"
            },
            "Conditional": {
                "description": "Evaluates a condition and branches to different blocks",
                "purpose": "Decision making, branching logic",
                "properties": {
                    "condition": {
                        "type": "string",
                        "required": true,
                        "description": "Boolean expression to evaluate"
                    },
                    "true_block": {
                        "type": "string",
                        "required": true,
                        "description": "Block ID to navigate to if condition is true"
                    },
                    "false_block": {
                        "type": "string",
                        "required": true,
                        "description": "Block ID to navigate to if condition is false"
                    }
                },
                "example": {
                    "id": "check_user_status",
                    "type": "Conditional",
                    "properties": {
                        "condition": "state.user.is_premium == true",
                        "true_block": "show_premium_offer",
                        "false_block": "show_regular_offer"
                    }
                }
            },
            "Compute": {
                "description": "Performs computation and optionally navigates to next block",
                "purpose": "Data processing, calculations, flow termination",
                "properties": {
                    "expression": {
                        "type": "string",
                        "required": true,
                        "description": "Expression to evaluate or string to compute"
                    },
                    "output_key": {
                        "type": "string",
                        "required": false,
                        "description": "Key to store the computation result in state"
                    },
                    "next_block": {
                        "type": "string",
                        "required": false,
                        "description": "Next block ID. If omitted, uses 'default' for termination"
                    }
                },
                "example": {
                    "id": "process_complete",
                    "type": "Compute",
                    "properties": {
                        "expression": "\"Processing completed successfully.\""
                    }
                },
                "notes": "Compute blocks without next_block will terminate the flow naturally"
            },
            "GoTo": {
                "description": "Unconditionally navigates to another block",
                "purpose": "Flow control, jumping to specific blocks",
                "properties": {
                    "target": {
                        "type": "string",
                        "required": true,
                        "description": "ID of the target block to navigate to"
                    }
                },
                "example": {
                    "id": "end_flow",
                    "type": "GoTo",
                    "properties": {
                        "target": "completion_message"
                    }
                }
            },
            "ExternalData": {
                "description": "Fetch or interact with external data sources and APIs",
                "purpose": "API calls, external service integration, data retrieval",
                "properties": {
                    "api_url": {
                        "type": "string",
                        "required": true,
                        "description": "URL endpoint to call"
                    },
                    "data_path": {
                        "type": "string",
                        "required": true,
                        "description": "JSON Pointer path to extract data from response (e.g., '/result/records')"
                    },
                    "method": {
                        "type": "string",
                        "required": false,
                        "description": "HTTP method (GET, POST, etc.)",
                        "default": "GET"
                    },
                    "headers": {
                        "type": "object",
                        "required": false,
                        "description": "HTTP headers to include"
                    },
                    "params": {
                        "type": "object",
                        "required": false,
                        "description": "Query parameters or request body"
                    },
                    "next_block": {
                        "type": "string",
                        "required": true,
                        "description": "ID of the next block to navigate to"
                    }
                },
                "example": {
                    "id": "fetch_user_data",
                    "type": "ExternalData",
                    "properties": {
                        "api_url": "https://api.example.com/users",
                        "data_path": "/users",
                        "method": "GET",
                        "next_block": "process_data"
                    }
                }
            }
        });

        let termination_info = json!({
            "termination_patterns": {
                "compute_termination": {
                    "description": "Use Compute block without next_block for clean termination",
                    "example": {
                        "id": "flow_complete",
                        "type": "Compute",
                        "properties": {
                            "expression": "\"Flow completed successfully.\""
                        }
                    },
                    "note": "This creates a 'default' termination that works reliably"
                },
                "default_block": {
                    "description": "Always include a 'default' block for fallback termination",
                    "example": {
                        "id": "default",
                        "type": "Compute",
                        "properties": {
                            "expression": "\"Flow terminated.\""
                        }
                    },
                    "note": "Compute blocks without next_block automatically target 'default'"
                }
            }
        });

        json!({
            "block_types": schemas,
            "termination": termination_info,
            "validation_rules": {
                "flow_structure": {
                    "must_have_start_block": "The start_block_id must reference an existing block",
                    "no_duplicate_ids": "All block IDs must be unique within the flow",
                    "reachable_blocks": "All blocks should be reachable from the start block"
                },
                "block_validation": {
                    "required_properties": "Each block type has specific required properties",
                    "property_types": "Properties must match expected data types",
                    "block_references": "next_block, target, true_block, false_block must reference existing block IDs"
                }
            }
        })
    }

    pub fn get_flow_patterns_as_json(&self) -> Value {
        let patterns = self
            .flow_patterns
            .iter()
            .map(|pattern| {
                json!({
                    "name": pattern.name,
                    "description": pattern.description,
                    "use_case": pattern.use_case,
                    "complexity": format!("{:?}", pattern.complexity),
                    "template_path": pattern.template_path
                })
            })
            .collect::<Vec<_>>();

        json!({
            "available_patterns": patterns,
            "pattern_guidance": {
                "start_simple": "Begin with Simple patterns (linear flows) before attempting Complex ones",
                "proven_templates": "These patterns are validated and known to execute successfully",
                "termination_important": "Pay special attention to flow termination patterns"
            }
        })
    }

    pub fn get_flow_patterns(&self) -> Vec<String> {
        self.flow_patterns
            .iter()
            .map(|p| {
                format!(
                    "Pattern: {}\nDescription: {}\nUse Case: {}",
                    p.name, p.description, p.use_case
                )
            })
            .collect()
    }

    fn load_flow_patterns(&mut self) -> Vec<String> {
        self.flow_patterns.extend(vec![
            FlowPattern {
                name: "Linear Greeting Flow".to_string(),
                description: "Simple conditional greeting with premium/regular user branching"
                    .to_string(),
                use_case: "User onboarding, personalised messaging".to_string(),
                template_path: "crates/stele/src/blocks/templates/flow_template.json".to_string(),
                complexity: PatternComplexity::Simple,
            },
            FlowPattern {
                name: "Input-Driven Flow".to_string(),
                description: "Starts with user input and processes through external data"
                    .to_string(),
                use_case: "Data collection, appointment scheduling, form processing".to_string(),
                template_path: "crates/stele/src/blocks/templates/sample_flow.json".to_string(),
                complexity: PatternComplexity::Moderate,
            },
            FlowPattern {
                name: "Processing Logic Flow".to_string(),
                description: "Input -> OAuth -> Schedule -> Availability Check -> Compute"
                    .to_string(),
                use_case: "Complex business logic, multi-step processing".to_string(),
                template_path: "crates/stele/src/blocks/templates/scheduling_logic.json"
                    .to_string(),
                complexity: PatternComplexity::Complex,
            },
        ]);

        self.flow_patterns
            .iter()
            .map(|p| {
                format!(
                    "Pattern: {}\nDescription: {}\nUse Case: {}",
                    p.name, p.description, p.use_case
                )
            })
            .collect()
    }

    pub fn get_patterns_by_complexity(&self, complexity: PatternComplexity) -> Vec<&FlowPattern> {
        self.flow_patterns
            .iter()
            .filter(|p| matches!(p.complexity, complexity))
            .collect()
    }

    pub fn load_template_content(&self, template_path: &str) -> Result<Value> {
        let content = std::fs::read_to_string(template_path)?;
        let json_content: Value = serde_json::from_str(&content)?;
        Ok(json_content)
    }

    pub fn generate_llm_context(&self) -> Value {
        json!({
            "schemas": self.get_block_schemas_as_json(),
            "patterns": self.get_flow_patterns_as_json(),
            "best_practices": {
                "flow_design": [
                    "Always include a clear termination strategy",
                    "Use Input blocks for user interaction points",
                    "Compute blocks are excellent for both processing and termination",
                    "Include a 'default' block for reliable termination"
                ],
                "block_naming": [
                    "Use descriptive, action-oriented block IDs",
                    "Follow snake_case naming convention",
                    "Make block flow obvious from names"
                ],
                "error_prevention": [
                    "Ensure all referenced block IDs exist",
                    "Include all required properties for each block type",
                    "Test termination paths carefully"
                ]
            },
            "common_mistakes": [
                "Forgetting required properties like 'message' in Display blocks",
                "Creating unreachable blocks",
                "Missing termination blocks or improper termination",
                "Using invalid block types or property names"
            ]
        })
    }
}
