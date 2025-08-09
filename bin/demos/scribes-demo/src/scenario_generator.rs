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

use crate::local_llm_interface::LocalLLMInterface;
use serde_json::Value;
use std::fmt;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LLMStatus {
    Idle,
    LocalProcessing,
    FallbackProcessing,
    Failed,
}

impl fmt::Display for LLMStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LLMStatus::Idle => write!(f, "🟢 Ready"),
            LLMStatus::LocalProcessing => write!(f, "🔵 Local LLM"),
            LLMStatus::FallbackProcessing => write!(f, "🟠 Cloud LLM"),
            LLMStatus::Failed => write!(f, "🔴 Failed"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GeneratedScenario {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub priority: String,
    pub data: Value,
    pub expected_outcome: Value,
}

impl fmt::Display for GeneratedScenario {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "ID: {}", self.id)?;
        writeln!(f, "Name: {}", self.name)?;
        writeln!(f, "Category: {}", self.category)?;
        writeln!(f, "Priority: {}", self.priority)?;
        writeln!(f, "Description: {}", self.description)?;
        writeln!(
            f,
            "Data: {}",
            serde_json::to_string_pretty(&self.data).unwrap_or_else(|_| "Invalid JSON".to_string())
        )?;
        writeln!(
            f,
            "Expected Outcome: {}",
            serde_json::to_string_pretty(&self.expected_outcome)
                .unwrap_or_else(|_| "Invalid JSON".to_string())
        )
    }
}

#[derive(Debug)]
struct ScenarioRequest {
    topic: String,
    scenario_type: String,
    complexity: String,
    count: usize,
}

pub struct InteractiveScenarioGenerator {
    llm_interface: Arc<Mutex<LocalLLMInterface>>,
    current_status: Arc<Mutex<LLMStatus>>,
}

impl InteractiveScenarioGenerator {
    pub fn new(llm_interface: Arc<Mutex<LocalLLMInterface>>) -> Self {
        Self {
            llm_interface,
            current_status: Arc::new(Mutex::new(LLMStatus::Idle)),
        }
    }

    pub async fn get_current_status(&self) -> LLMStatus {
        *self.current_status.lock().await
    }

    pub async fn generate_scenarios_for_topic(
        &self,
        topic: &str,
    ) -> Result<Vec<GeneratedScenario>, String> {
        *self.current_status.lock().await = LLMStatus::LocalProcessing;

        let request = ScenarioRequest {
            topic: topic.to_string(),
            scenario_type: "sophisticated".to_string(),
            complexity: "high".to_string(),
            count: 4,
        };

        let prompt = self.build_generation_prompt(&request);

        let response = match self
            .llm_interface
            .lock()
            .await
            .generate_simple(&prompt, None)
            .await
        {
            Ok(resp) => {
                tracing::info!("✓ Local LLM (Ollama) generation successful");

                resp
            }
            Err(e) => {
                tracing::warn!("Local LLM (Ollama) generation failed: {}, trying robust method with Anthropic fallback", e);

                *self.current_status.lock().await = LLMStatus::FallbackProcessing;

                match self
                    .llm_interface
                    .lock()
                    .await
                    .query_robust(&prompt, None)
                    .await
                {
                    Ok(resp) => resp,
                    Err(e2) => {
                        *self.current_status.lock().await = LLMStatus::Failed;
                        return Err(format!(
                            "All LLM providers failed - Ollama: {e} | Anthropic: {e2}"
                        ));
                    }
                }
            }
        };

        let parsed_response = match self.extract_json_from_response(&response) {
            Ok(json) => json,
            Err(e) => {
                *self.current_status.lock().await = LLMStatus::Failed;
                return Err(format!(
                    "JSON parsing failed after successful LLM response: {e}"
                ));
            }
        };

        let result = match self.parse_scenarios(&parsed_response, &request) {
            Ok(scenarios) => {
                if scenarios.is_empty() {
                    *self.current_status.lock().await = LLMStatus::Failed;
                    Err("No valid scenarios could be parsed from LLM response".to_string())
                } else {
                    *self.current_status.lock().await = LLMStatus::Idle;
                    Ok(scenarios)
                }
            }
            Err(e) => {
                *self.current_status.lock().await = LLMStatus::Failed;
                Err(format!("Scenario parsing failed: {e}"))
            }
        };

        result
    }

    fn parse_scenarios(
        &self,
        response: &Value,
        request: &ScenarioRequest,
    ) -> Result<Vec<GeneratedScenario>, String> {
        let scenarios_array = response
            .as_array()
            .ok_or_else(|| "Response is not a JSON array".to_string())?;

        let mut scenarios = Vec::new();
        for (i, scenario_value) in scenarios_array.iter().enumerate() {
            let scenario = GeneratedScenario {
                id: scenario_value["id"]
                    .as_str()
                    .unwrap_or(&format!("gen-{:03}", i + 1))
                    .to_string(),
                name: scenario_value["name"]
                    .as_str()
                    .unwrap_or("Generated Scenario")
                    .to_string(),
                description: scenario_value["description"]
                    .as_str()
                    .unwrap_or("Auto-generated test scenario")
                    .to_string(),
                category: scenario_value["category"]
                    .as_str()
                    .unwrap_or(&request.scenario_type)
                    .to_string(),
                priority: scenario_value["priority"]
                    .as_str()
                    .unwrap_or("medium")
                    .to_string(),
                data: scenario_value["data"].clone(),
                expected_outcome: scenario_value["expected_outcome"].clone(),
            };
            scenarios.push(scenario);
        }
        Ok(scenarios)
    }

    fn extract_json_from_response(&self, response: &str) -> Result<Value, String> {
        let cleaned_response = response
            .chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\r' || *c == '\t')
            .collect::<String>();

        tracing::debug!("Cleaned response length: {}", cleaned_response.len());
        tracing::debug!(
            "First 200 chars: {}",
            &cleaned_response.chars().take(200).collect::<String>()
        );

        if let Some(start) = cleaned_response.find('[') {
            if let Some(end) = cleaned_response.rfind(']') {
                let json_str = &cleaned_response[start..=end];
                tracing::debug!(
                    "Extracted JSON: {}",
                    &json_str.chars().take(500).collect::<String>()
                );

                match serde_json::from_str::<Value>(json_str) {
                    Ok(value) => return Ok(value),
                    Err(e) => {
                        tracing::warn!("Failed to parse extracted JSON: {}", e);

                        let super_clean = json_str
                            .replace("\u{0000}", "")
                            .replace("\u{0001}", "")
                            .replace("\u{0002}", "")
                            .replace("\u{0003}", "")
                            .replace("\u{0004}", "")
                            .replace("\u{0005}", "")
                            .replace("\u{0006}", "")
                            .replace("\u{0007}", "")
                            .replace("\u{0008}", "")
                            .replace("\u{000B}", "")
                            .replace("\u{000C}", "")
                            .replace("\u{000E}", "")
                            .replace("\u{000F}", "")
                            .replace("\u{0010}", "")
                            .replace("\u{0011}", "")
                            .replace("\u{0012}", "")
                            .replace("\u{0013}", "")
                            .replace("\u{0014}", "")
                            .replace("\u{0015}", "")
                            .replace("\u{0016}", "")
                            .replace("\u{0017}", "")
                            .replace("\u{0018}", "")
                            .replace("\u{0019}", "")
                            .replace("\u{001A}", "")
                            .replace("\u{001B}", "")
                            .replace("\u{001C}", "")
                            .replace("\u{001D}", "")
                            .replace("\u{001E}", "")
                            .replace("\u{001F}", "");

                        match serde_json::from_str::<Value>(&super_clean) {
                            Ok(value) => return Ok(value),
                            Err(e2) => {
                                tracing::error!("Super clean parse failed: {}", e2);

                                let repaired = Self::repair_json(&super_clean);
                                match serde_json::from_str::<Value>(&repaired) {
                                    Ok(value) => {
                                        tracing::info!("JSON repair successful");
                                        return Ok(value);
                                    }
                                    Err(e3) => {
                                        tracing::error!("JSON repair also failed: {}", e3);
                                        return Err(format!(
                                            "Failed to parse JSON after cleaning and repair: {e3} | Super clean: {e2} | Original: {e}"
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        match serde_json::from_str::<Value>(&cleaned_response) {
            Ok(value) => Ok(value),
            Err(e) => {
                tracing::error!("Full response parse failed: {}", e);
                tracing::error!(
                    "Response was: {}",
                    &cleaned_response.chars().take(1000).collect::<String>()
                );
                Err(format!("Failed to parse JSON: {e}"))
            }
        }
    }

    fn build_generation_prompt(&self, request: &ScenarioRequest) -> String {
        format!(
            r#"Generate a detailed, sophisticated test scenario for the STELE cognitive system demo based on the user's topic.

TOPIC: "{}"
COMPLEXITY: {}
REQUIRED COUNT: {} scenario(s)

You are creating test scenarios for an advanced AI system that processes data through multiple specialist "scribes":
- Knowledge Scribe: Extracts entities and relationships, builds knowledge graphs
- Data Scribe: Processes complex text using LLM analysis and structured data extraction
- Identity Scribe: Verifies sources and manages identity authentication

Create scenarios that demonstrate realistic, complex AI processing challenges related to the user's topic.

REQUIREMENTS:
1. Make scenarios technically sophisticated and realistic to the topic
2. Include rich, detailed text content (100-300 words) that would challenge AI systems
3. Extract 4-8 relevant entities related to the topic
4. Use realistic source IDs (urn:topic:system:xxx format)
5. Set appropriate urgency levels (0.1-0.9)
6. Create meaningful expected outcomes with specific metrics
7. Categories: "data_processing", "knowledge_extraction", "identity_verification", or "coordination"
8. Priorities: "low", "medium", "high", "critical"

EXAMPLE QUALITY LEVEL (adapt this structure to your topic):
{{
  "id": "gen-001",
  "name": "Advanced Space Manufacturing Systems",
  "description": "Complex analysis of zero-gravity manufacturing processes and equipment systems",
  "category": "data_processing",
  "priority": "high",
  "data": {{
    "text": "Advanced zero-gravity manufacturing systems for ice cream production in orbital facilities utilise specialised cryogenic chambers, micro-gravity crystallisation processes, and automated robotic assembly lines. The systems must account for thermal management in vacuum conditions, contamination prevention protocols, and precise ingredient mixing ratios adapted for space environments. Quality control mechanisms include real-time molecular analysis, texture consistency monitoring, and packaging integrity verification systems designed for long-term space storage and transport.",
    "entities": ["zero_gravity_manufacturing", "cryogenic_chambers", "micro_gravity_crystallisation", "robotic_assembly", "thermal_management", "contamination_prevention", "molecular_analysis", "space_storage"],
    "source_id": "urn:space:manufacturing:001",
    "urgency": 0.7
  }},
  "expected_outcome": {{
    "success": true,
    "entities_processed": 8,
    "processing_method": "llm_structured_analysis",
    "min_processing_time_ms": 800,
    "complexity_handled": "high"
  }}
}}

Generate {} detailed scenario(s) in this format as a JSON array. Focus on the technical complexity and real-world applications of "{}" while ensuring the content would be challenging and interesting for advanced AI processing systems.

Respond with ONLY the JSON array, no additional text:
"#,
            request.topic, request.complexity, request.count, request.count, request.topic
        )
    }

    fn repair_json(json_str: &str) -> String {
        let mut repaired = json_str.to_string();

        repaired = repaired.replace("}\n  {", "},\n  {");
        repaired = repaired.replace("}\n\n  {", "},\n\n  {");
        repaired = repaired.replace("}\n    {", "},\n    {");

        if repaired.contains("\"priority\"") && !repaired.contains("\"data\"") {
            repaired = repaired.replace(
                "\"priority\": \"",
                "\"data\": {\"text\": \"\", \"entities\": [], \"source_id\": \"\", \"urgency\": 0.5}, \"priority\": \""
            );
        }

        let lines: Vec<&str> = repaired.lines().collect();
        let mut fixed_lines = Vec::new();
        let mut brace_count = 0;
        let mut bracket_count = 0;
        let mut in_string = false;

        for (i, line) in lines.iter().enumerate() {
            for ch in line.chars() {
                match ch {
                    '"' if !in_string => in_string = true,
                    '"' if in_string => in_string = false,
                    '[' if !in_string => bracket_count += 1,
                    ']' if !in_string => bracket_count -= 1,
                    '{' if !in_string => brace_count += 1,
                    '}' if !in_string => brace_count -= 1,
                    _ => {}
                }
            }

            if bracket_count >= 0 && brace_count >= 0 {
                fixed_lines.push(*line);
            }
        }

        repaired = fixed_lines.join("\n");

        if !repaired.trim_end().ends_with(']') {
            let mut open_braces = 0;
            let mut open_brackets = 0;
            let mut in_string = false;

            for ch in repaired.chars() {
                match ch {
                    '"' if !in_string => in_string = true,
                    '"' if in_string => in_string = false,
                    '[' if !in_string => open_brackets += 1,
                    ']' if !in_string => open_brackets -= 1,
                    '{' if !in_string => open_braces += 1,
                    '}' if !in_string => open_braces -= 1,
                    _ => {}
                }
            }

            for _ in 0..open_braces {
                repaired.push_str("\n    }");
            }
            for _ in 0..open_brackets {
                repaired.push_str("\n]");
            }
        }

        if !repaired.trim_start().starts_with('[') {
            repaired = format!("[{repaired}]");
        }

        repaired
    }
}
