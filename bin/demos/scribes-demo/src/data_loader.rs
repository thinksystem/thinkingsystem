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
use std::fs;

pub struct DataLoader;

#[derive(Debug)]
pub struct ConsolidatedTestData {
    pub scenarios: Value,
}

impl DataLoader {
    fn get_test_data_path(filename: &str) -> String {
        let possible_paths = [
            format!("test_data/{filename}"),
            format!("bin/demos/scribes-demo/test_data/{filename}"),
            format!("./bin/demos/scribes-demo/test_data/{filename}"),
        ];

        for path in &possible_paths {
            if std::path::Path::new(path).exists() {
                return path.to_string();
            }
        }

        possible_paths[0].clone()
    }

    fn load_consolidated_scenarios() -> Result<ConsolidatedTestData, String> {
        let path = Self::get_test_data_path("consolidated_test_scenarios.json");
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read consolidated scenarios from {path}: {e}"))?;

        let scenarios: Value = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse consolidated scenarios JSON: {e}"))?;

        Ok(ConsolidatedTestData { scenarios })
    }

    fn extract_scenario_data(scenarios: &Value, category: &str) -> Result<Vec<Value>, String> {
        scenarios
            .get("scenarios")
            .and_then(|s| s.get(category))
            .ok_or_else(|| format!("Category '{category}' not found in consolidated scenarios"))?
            .as_array()
            .ok_or_else(|| format!("Category '{category}' is not an array"))?
            .iter()
            .map(|scenario| {
                scenario
                    .get("data")
                    .cloned()
                    .ok_or_else(|| "Scenario missing 'data' field".to_string())
            })
            .collect()
    }

    fn extract_stress_scenarios(
        scenarios: &Value,
        subcategory: &str,
    ) -> Result<Vec<Value>, String> {
        scenarios
            .get("scenarios")
            .and_then(|s| s.get("stress_testing"))
            .and_then(|st| st.get(subcategory))
            .ok_or_else(|| format!("Stress testing subcategory '{subcategory}' not found"))?
            .as_array()
            .ok_or_else(|| format!("Stress testing subcategory '{subcategory}' is not an array"))?
            .iter()
            .map(|scenario| {
                scenario
                    .get("data")
                    .cloned()
                    .ok_or_else(|| "Stress scenario missing 'data' field".to_string())
            })
            .collect()
    }

    pub fn load_knowledge_specialist_scenarios() -> Result<Vec<Value>, String> {
        let consolidated = Self::load_consolidated_scenarios()?;
        Self::extract_scenario_data(&consolidated.scenarios, "knowledge_specialist")
    }

    pub fn load_enhanced_data_processor_scenarios() -> Result<Vec<Value>, String> {
        let consolidated = Self::load_consolidated_scenarios()?;
        Self::extract_scenario_data(&consolidated.scenarios, "data_processor")
    }

    pub fn load_identity_verifier_contexts() -> Result<Vec<Value>, String> {
        let consolidated = Self::load_consolidated_scenarios()?;
        Self::extract_scenario_data(&consolidated.scenarios, "identity_verifier")
    }

    pub fn load_high_volume_scenarios() -> Result<Vec<Value>, String> {
        let consolidated = Self::load_consolidated_scenarios()?;
        Self::extract_stress_scenarios(&consolidated.scenarios, "high_volume")
    }

    pub fn load_edge_case_scenarios() -> Result<Vec<Value>, String> {
        let consolidated = Self::load_consolidated_scenarios()?;
        let mut scenarios = Self::extract_scenario_data(&consolidated.scenarios, "edge_cases")?;

        for scenario in &mut scenarios {
            if let Some(text) = scenario.get_mut("text") {
                if text.as_str() == Some("PLACEHOLDER_FOR_LONG_TEXT") {
                    *text = serde_json::Value::String(
                        "AI ".repeat(1000) + "systems process data efficiently.",
                    );
                }
            }
        }

        Ok(scenarios)
    }

    pub fn load_concurrent_scenarios() -> Result<Vec<Value>, String> {
        let consolidated = Self::load_consolidated_scenarios()?;
        Self::extract_stress_scenarios(&consolidated.scenarios, "concurrent")
    }

    pub fn load_failure_scenarios() -> Result<Vec<Value>, String> {
        let consolidated = Self::load_consolidated_scenarios()?;
        Self::extract_scenario_data(&consolidated.scenarios, "failure_recovery")
    }

    pub fn load_coordination_scenarios() -> Result<Vec<Value>, String> {
        let consolidated = Self::load_consolidated_scenarios()?;
        Self::extract_scenario_data(&consolidated.scenarios, "coordination")
    }

    pub fn load_all_stress_scenarios() -> Result<Vec<Value>, String> {
        let consolidated = Self::load_consolidated_scenarios()?;
        let mut all_stress = Vec::new();

        all_stress.extend(Self::extract_stress_scenarios(
            &consolidated.scenarios,
            "high_volume",
        )?);
        all_stress.extend(Self::extract_stress_scenarios(
            &consolidated.scenarios,
            "concurrent",
        )?);

        Ok(all_stress)
    }

    pub fn load_scenario_by_id(scenario_id: &str) -> Result<Value, String> {
        let consolidated = Self::load_consolidated_scenarios()?;
        let scenarios = consolidated
            .scenarios
            .get("scenarios")
            .ok_or("No scenarios found in consolidated data")?;

        for (_, category) in scenarios.as_object().unwrap() {
            if let Some(scenarios_array) = category.as_array() {
                for scenario in scenarios_array {
                    if scenario.get("id").and_then(|id| id.as_str()) == Some(scenario_id) {
                        return scenario
                            .get("data")
                            .cloned()
                            .ok_or_else(|| format!("Scenario {scenario_id} missing 'data' field"));
                    }
                }
            } else if let Some(subcategories) = category.as_object() {
                for (_, subcategory) in subcategories {
                    if let Some(scenarios_array) = subcategory.as_array() {
                        for scenario in scenarios_array {
                            if scenario.get("id").and_then(|id| id.as_str()) == Some(scenario_id) {
                                return scenario.get("data").cloned().ok_or_else(|| {
                                    format!("Scenario {scenario_id} missing 'data' field")
                                });
                            }
                        }
                    }
                }
            }
        }

        Err(format!("Scenario with ID '{scenario_id}' not found"))
    }

    pub fn get_scenario_metadata(scenario_id: &str) -> Result<Value, String> {
        let consolidated = Self::load_consolidated_scenarios()?;
        let scenarios = consolidated
            .scenarios
            .get("scenarios")
            .ok_or("No scenarios found in consolidated data")?;

        for (_, category) in scenarios.as_object().unwrap() {
            if let Some(scenarios_array) = category.as_array() {
                for scenario in scenarios_array {
                    if scenario.get("id").and_then(|id| id.as_str()) == Some(scenario_id) {
                        return Ok(scenario.clone());
                    }
                }
            } else if let Some(subcategories) = category.as_object() {
                for (_, subcategory) in subcategories {
                    if let Some(scenarios_array) = subcategory.as_array() {
                        for scenario in scenarios_array {
                            if scenario.get("id").and_then(|id| id.as_str()) == Some(scenario_id) {
                                return Ok(scenario.clone());
                            }
                        }
                    }
                }
            }
        }

        Err(format!("Scenario with ID '{scenario_id}' not found"))
    }

    fn load_json_file(path: &str) -> Result<Vec<Value>, String> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("Failed to read {path}: {e}"))?;

        let scenarios: Vec<Value> = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse JSON from {path}: {e}"))?;

        Ok(scenarios)
    }
}
