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

use crate::agents::schemas::{
    AgentConfig, AgentMetadata, AgentStatus, GenerationMethod, PerformanceMetrics,
    RuntimeCapabilities, TechnicalSkill, TrustLevel,
};
use crate::agents::{Agent, AgentCapabilities, AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use stele::nlu::llm_processor::LLMAdapter;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackAgentConfig {
    pub default_gas_limit: u64,
    pub default_execution_timeout_secs: u64,
    pub default_trust_level: TrustLevel,
    pub default_ffi_permissions: Vec<String>,
    pub fallback_agents: Vec<FallbackAgentTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackAgentTemplate {
    pub name: String,
    pub role: String,
    pub specialisation: String,
    pub personality_traits: Vec<String>,
    pub strengths: Vec<String>,
    pub approach_style: String,
    pub competitive_edge: String,
    pub risk_tolerance: f64,
    pub collaboration_preference: String,
    pub technical_skills: Vec<FallbackTechnicalSkill>,
    pub expected_performance: LLMPerformanceExpectation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackTechnicalSkill {
    pub name: String,
    pub proficiency: f64,
    pub experience_description: String,
    pub domains: Vec<String>,
}

impl Default for FallbackAgentConfig {
    fn default() -> Self {
        Self {
            default_gas_limit: 15000,
            default_execution_timeout_secs: 600,
            default_trust_level: TrustLevel::Standard,
            default_ffi_permissions: vec!["log_progress".to_string(), "calculate".to_string()],
            fallback_agents: Self::default_fallback_agents(),
        }
    }
}

impl FallbackAgentConfig {
    fn default_fallback_agents() -> Vec<FallbackAgentTemplate> {
        vec![
            FallbackAgentTemplate {
                name: "Alex Strategist".to_string(),
                role: "Strategic Planner".to_string(),
                specialisation: "Systems Analysis and Planning".to_string(),
                personality_traits: vec![
                    "analytical".to_string(),
                    "visionary".to_string(),
                    "methodical".to_string(),
                ],
                strengths: vec![
                    "strategic thinking".to_string(),
                    "system design".to_string(),
                    "risk assessment".to_string(),
                ],
                approach_style: "top-down strategic analysis".to_string(),
                competitive_edge: "ability to see the big picture while managing details"
                    .to_string(),
                risk_tolerance: 6.5,
                collaboration_preference: "leads through vision and coordination".to_string(),
                technical_skills: vec![FallbackTechnicalSkill {
                    name: "Strategic Planning".to_string(),
                    proficiency: 0.9,
                    experience_description: "7 years of strategic planning".to_string(),
                    domains: vec![
                        "Business Strategy".to_string(),
                        "Systems Design".to_string(),
                    ],
                }],
                expected_performance: LLMPerformanceExpectation {
                    success_rate_estimate: 0.85,
                    quality_score_estimate: 0.9,
                    collaboration_score_estimate: 0.8,
                    innovation_score_estimate: 0.7,
                    completion_time_estimate: 45.0,
                },
            },
            FallbackAgentTemplate {
                name: "Morgan Implementer".to_string(),
                role: "Technical Implementer".to_string(),
                specialisation: "Software Development and Implementation".to_string(),
                personality_traits: vec![
                    "pragmatic".to_string(),
                    "detail-oriented".to_string(),
                    "efficient".to_string(),
                ],
                strengths: vec![
                    "rapid implementation".to_string(),
                    "debugging".to_string(),
                    "optimisation".to_string(),
                ],
                approach_style: "iterative development with continuous feedback".to_string(),
                competitive_edge: "exceptional speed and accuracy in implementation".to_string(),
                risk_tolerance: 4.0,
                collaboration_preference: "works closely with others, prefers clear specifications"
                    .to_string(),
                technical_skills: vec![FallbackTechnicalSkill {
                    name: "Software Development".to_string(),
                    proficiency: 0.95,
                    experience_description: "10 years of software development".to_string(),
                    domains: vec!["Rust".to_string(), "Systems Programming".to_string()],
                }],
                expected_performance: LLMPerformanceExpectation {
                    success_rate_estimate: 0.92,
                    quality_score_estimate: 0.88,
                    collaboration_score_estimate: 0.85,
                    innovation_score_estimate: 0.6,
                    completion_time_estimate: 30.0,
                },
            },
        ]
    }
}
pub struct AgentGenerator {
    config: GeneratorConfig,
    llm_adapter: Option<Box<dyn LLMAdapter>>,
    fallback_config: FallbackAgentConfig,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratorConfig {
    pub default_model: String,
    pub max_retries: u32,
    pub temperature: f64,
    pub max_tokens: usize,
    pub enable_caching: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    pub team_size: usize,
    pub task_context: String,
    pub requirements: Vec<TaskRequirement>,
    pub diversity_requirements: DiversityRequirements,
    pub performance_expectations: PerformanceExpectations,
    pub model_config: ModelConfig,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequirement {
    pub capability: String,
    pub min_proficiency: f64,
    pub critical: bool,
    pub alternatives: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiversityRequirements {
    pub min_specialisations: usize,
    pub diverse_approach_styles: bool,
    pub diverse_risk_tolerance: bool,
    pub diverse_collaboration_styles: bool,
    pub min_diversity_score: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceExpectations {
    pub min_success_rate: f64,
    pub min_quality_score: f64,
    pub min_collaboration_score: f64,
    pub min_innovation_score: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub model: String,
    pub temperature: f64,
    pub max_tokens: usize,
    pub custom_params: HashMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTeamResponse {
    pub agents: Vec<LLMAgentData>,
    pub team_dynamics: String,
    pub collaborative_potential: f64,
    pub diversity_score: f64,
    pub generation_reasoning: String,
    pub team_structure: TeamStructure,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMAgentData {
    pub name: String,
    pub role: String,
    pub specialisation: String,
    pub personality_traits: Vec<String>,
    pub strengths: Vec<String>,
    pub approach_style: String,
    pub competitive_edge: String,
    pub risk_tolerance: f64,
    pub collaboration_preference: String,
    pub technical_skills: Vec<LLMTechnicalSkill>,
    pub expected_performance: LLMPerformanceExpectation,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMTechnicalSkill {
    pub name: String,
    pub proficiency: f64,
    pub experience_description: String,
    pub domains: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMPerformanceExpectation {
    pub success_rate_estimate: f64,
    pub quality_score_estimate: f64,
    pub collaboration_score_estimate: f64,
    pub innovation_score_estimate: f64,
    pub completion_time_estimate: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamStructure {
    pub team_lead: Option<String>,
    pub collaboration_patterns: Vec<CollaborationPattern>,
    pub role_dependencies: HashMap<String, Vec<String>>,
    pub communication_structure: CommunicationStructure,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationPattern {
    pub agents: Vec<String>,
    pub pattern_type: CollaborationPatternType,
    pub description: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollaborationPatternType {
    Sequential,
    Parallel,
    ReviewValidation,
    Ideation,
    Consensus,
    Iterative,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicationStructure {
    pub hub_agent: Option<String>,
    pub direct_pairs: Vec<(String, String)>,
    pub broadcast_triggers: Vec<String>,
}
impl AgentGenerator {
    pub fn new() -> AgentResult<Self> {
        Ok(Self {
            config: GeneratorConfig::default(),
            llm_adapter: None,
            fallback_config: FallbackAgentConfig::default(),
        })
    }

    pub fn with_config(config: GeneratorConfig) -> Self {
        Self {
            config,
            llm_adapter: None,
            fallback_config: FallbackAgentConfig::default(),
        }
    }

    pub fn with_fallback_config(mut self, fallback_config: FallbackAgentConfig) -> Self {
        self.fallback_config = fallback_config;
        self
    }

    pub fn get_llm_adapter(&self) -> Option<&dyn LLMAdapter> {
        self.llm_adapter.as_ref().map(|a| a.as_ref())
    }
    pub async fn generate_agent(&self, config: AgentConfig) -> AgentResult<Agent> {
        let role = config
            .required_capabilities
            .first()
            .unwrap_or(&"General Agent".to_string())
            .clone();
        let specialisation = config
            .required_capabilities
            .get(1)
            .unwrap_or(&"Multi-purpose".to_string())
            .clone();
        let requirements = vec![];
        self.generate_single_agent(&role, &specialisation, requirements)
            .await
    }
    pub async fn generate_team(&self, config: GenerationConfig) -> AgentResult<Vec<Agent>> {
        let prompt = self.create_team_generation_prompt(&config);
        let response = self
            .call_llm_for_team_generation(&prompt, &config.model_config, None)
            .await?;
        self.convert_llm_response_to_agents(response, &config)
    }

    pub async fn generate_team_with_llm(
        &self,
        config: GenerationConfig,
        llm_adapter: &dyn LLMAdapter,
    ) -> AgentResult<Vec<Agent>> {
        let prompt = self.create_team_generation_prompt(&config);
        let response = self
            .call_llm_for_team_generation(&prompt, &config.model_config, Some(llm_adapter))
            .await?;
        self.convert_llm_response_to_agents(response, &config)
    }
    pub async fn generate_single_agent(
        &self,
        role: &str,
        specialisation: &str,
        requirements: Vec<TaskRequirement>,
    ) -> AgentResult<Agent> {
        let config = GenerationConfig {
            team_size: 1,
            task_context: format!("Generate a {role} specialised in {specialisation}"),
            requirements,
            diversity_requirements: DiversityRequirements::minimal(),
            performance_expectations: PerformanceExpectations::default(),
            model_config: ModelConfig::default(),
        };
        let mut team = self.generate_team(config).await?;
        team.pop().ok_or_else(|| {
            AgentError::GenerationFailed(Box::new(std::io::Error::other(
                "Failed to generate single agent",
            )))
        })
    }
    fn create_team_generation_prompt(&self, config: &GenerationConfig) -> String {
        let requirements_section = self.format_requirements_section(&config.requirements);
        let diversity_section = self.format_diversity_section(&config.diversity_requirements);
        let performance_section = self.format_performance_section(&config.performance_expectations);
        let json_schema = self.get_json_response_schema();

        format!(
            r#"You are an expert AI agent team designer. Create a team of {} diverse AI agents optimised for this task:

TASK CONTEXT: {}

{}

{}

{}

Generate agents with:
1. Complementary skills and approaches
2. Realistic expertise areas
3. Clear competitive advantages
4. Distinct personalities and working styles
5. Optimised team dynamics

CRITICAL: You MUST respond with valid JSON only. No additional text before or after.
All fields in the JSON schema are REQUIRED. Do not omit any fields.

{}"#,
            config.team_size,
            config.task_context,
            requirements_section,
            diversity_section,
            performance_section,
            json_schema
        )
    }

    fn format_requirements_section(&self, requirements: &[TaskRequirement]) -> String {
        if requirements.is_empty() {
            "REQUIREMENTS: None specified".to_string()
        } else {
            let requirements_text = requirements
                .iter()
                .map(|req| {
                    format!(
                        "- {} (min proficiency: {:.1}{})",
                        req.capability,
                        req.min_proficiency,
                        if req.critical { ", CRITICAL" } else { "" }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("REQUIREMENTS:\n{requirements_text}")
        }
    }

    fn format_diversity_section(&self, diversity: &DiversityRequirements) -> String {
        format!(
            r#"DIVERSITY REQUIREMENTS:
- Minimum {} different specialisations
- Diverse approach styles: {}
- Diverse risk tolerance: {}
- Diverse collaboration styles: {}
- Minimum diversity score: {:.1}"#,
            diversity.min_specialisations,
            diversity.diverse_approach_styles,
            diversity.diverse_risk_tolerance,
            diversity.diverse_collaboration_styles,
            diversity.min_diversity_score
        )
    }

    fn format_performance_section(&self, performance: &PerformanceExpectations) -> String {
        format!(
            r#"PERFORMANCE EXPECTATIONS:
- Success rate: {:.1}+
- Quality score: {:.1}+
- Collaboration score: {:.1}+
- Innovation score: {:.1}+"#,
            performance.min_success_rate,
            performance.min_quality_score,
            performance.min_collaboration_score,
            performance.min_innovation_score
        )
    }

    fn get_json_response_schema(&self) -> String {
        r#"Respond with valid JSON in this exact format:
{
    "agents": [
        {
            "name": "Agent Name",
            "role": "Role Title",
            "specialisation": "specific area of expertise",
            "personality_traits": ["trait1", "trait2", "trait3"],
            "strengths": ["strength1", "strength2", "strength3"],
            "approach_style": "how they approach problems",
            "competitive_edge": "what gives them an advantage",
            "risk_tolerance": 7.5,
            "collaboration_preference": "how they work with others",
            "technical_skills": [
                {
                    "name": "skill name",
                    "proficiency": 0.9,
                    "experience_description": "years and context",
                    "domains": ["domain1", "domain2"]
                }
            ],
            "expected_performance": {
                "success_rate_estimate": 0.85,
                "quality_score_estimate": 0.9,
                "collaboration_score_estimate": 0.8,
                "innovation_score_estimate": 0.7,
                "completion_time_estimate": 45.0
            }
        }
    ],
    "team_dynamics": "how the team works together effectively",
    "collaborative_potential": 8.5,
    "diversity_score": 9.2,
    "generation_reasoning": "why these specific agents were chosen",
    "team_structure": {
        "team_lead": "Agent Name",
        "collaboration_patterns": [
            {
                "agents": ["Agent1", "Agent2"],
                "pattern_type": "Sequential",
                "description": "Agent1 provides analysis, Agent2 implements"
            }
        ],
        "role_dependencies": {
            "Agent1": ["Agent2", "Agent3"]
        },
        "communication_structure": {
            "hub_agent": "Agent Name",
            "direct_pairs": [["Agent1", "Agent2"]],
            "broadcast_triggers": ["milestone_reached", "critical_decision"]
        }
    }
}"#
        .to_string()
    }
    async fn call_llm_for_team_generation(
        &self,
        prompt: &str,
        _model_config: &ModelConfig,
        llm_adapter: Option<&dyn LLMAdapter>,
    ) -> AgentResult<AgentTeamResponse> {
        tracing::debug!(
            "Generating team with config: model={}, temperature={}, max_tokens={}",
            self.config.default_model,
            self.config.temperature,
            self.config.max_tokens
        );

        if let Some(adapter) = llm_adapter {
            match adapter.generate_structured_response(
                "You are an expert AI agent team designer. Generate a JSON response that matches the specified schema exactly.",
                prompt
            ).await {
                Ok(response_value) => {

                    self.parse_agent_team_response_robustly(response_value)
                }
                Err(llm_error) => {
                    tracing::warn!(
                        "LLM call failed: {}. Using fallback team generation.",
                        llm_error
                    );
                    Ok(self.create_fallback_team_response())
                }
            }
        } else if let Some(adapter) = &self.llm_adapter {
            match adapter.generate_structured_response(
                "You are an expert AI agent team designer. Generate a JSON response that matches the specified schema exactly.",
                prompt
            ).await {
                Ok(response_value) => {

                    self.parse_agent_team_response_robustly(response_value)
                }
                Err(llm_error) => {
                    tracing::warn!(
                        "LLM call failed: {}. Using fallback team generation.",
                        llm_error
                    );
                    Ok(self.create_fallback_team_response())
                }
            }
        } else {
            tracing::debug!("No LLM adapter available, using fallback team generation");
            Ok(self.create_fallback_team_response())
        }
    }

    fn parse_agent_team_response_robustly(
        &self,
        response_value: Value,
    ) -> AgentResult<AgentTeamResponse> {
        tracing::debug!("Attempting to parse LLM response");

        if let Ok(team_response) =
            serde_json::from_value::<AgentTeamResponse>(response_value.clone())
        {
            tracing::debug!("Successfully parsed AgentTeamResponse directly");
            return Ok(team_response);
        }

        if let Some(extracted_json) = self.extract_json_from_string_response(&response_value) {
            if let Ok(team_response) =
                serde_json::from_value::<AgentTeamResponse>(extracted_json.clone())
            {
                tracing::debug!("Successfully parsed extracted JSON");
                return Ok(team_response);
            }

            if let Some(repaired_response) = self.repair_partial_json_response(&extracted_json) {
                if let Ok(team_response) =
                    serde_json::from_value::<AgentTeamResponse>(repaired_response)
                {
                    return Ok(team_response);
                }
            }
        }

        if let Some(repaired_response) = self.repair_partial_json_response(&response_value) {
            match serde_json::from_value::<AgentTeamResponse>(repaired_response) {
                Ok(team_response) => return Ok(team_response),
                Err(_) => tracing::warn!("Could not parse repaired response"),
            }
        }

        tracing::warn!("All parsing attempts failed, using fallback team");
        Ok(self.create_fallback_team_response())
    }

    fn extract_json_from_string_response(&self, response_value: &Value) -> Option<Value> {
        if let Some(response_str) = response_value.as_str() {
            tracing::debug!("Response is a string, attempting JSON extraction");
            if let Some(json_start) = response_str.find('{') {
                if let Some(json_end) = response_str.rfind('}') {
                    let json_str = &response_str[json_start..=json_end];
                    tracing::debug!("Extracted JSON substring");

                    return serde_json::from_str::<Value>(json_str)
                        .map_err(|e| tracing::warn!("Failed to parse extracted JSON: {}", e))
                        .ok();
                }
            }
        }
        None
    }

    fn repair_partial_json_response(&self, response_value: &Value) -> Option<Value> {
        if let Ok(mut partial) =
            serde_json::from_value::<serde_json::Map<String, Value>>(response_value.clone())
        {
            tracing::debug!("Response is a JSON object, checking for missing fields");

            let present_fields: Vec<&String> = partial.keys().collect();
            tracing::debug!("Present fields: {:?}", present_fields);

            self.add_missing_required_fields(&mut partial);

            if let Some(agents_array) = partial.get("agents").and_then(|a| a.as_array()) {
                if agents_array.is_empty() {
                    tracing::warn!("Empty agents array, will use fallback team");
                    return None;
                }
            }

            return Some(Value::Object(partial));
        }
        None
    }

    fn add_missing_required_fields(&self, partial: &mut serde_json::Map<String, Value>) {
        if !partial.contains_key("agents") {
            tracing::warn!("Missing 'agents' field, adding empty array");
            partial.insert("agents".to_string(), json!([]));
        }

        if !partial.contains_key("team_dynamics") {
            partial.insert(
                "team_dynamics".to_string(),
                json!("Generated team with diverse expertise"),
            );
        }

        if !partial.contains_key("collaborative_potential") {
            partial.insert("collaborative_potential".to_string(), json!(7.5));
        }

        if !partial.contains_key("diversity_score") {
            partial.insert("diversity_score".to_string(), json!(8.0));
        }

        if !partial.contains_key("generation_reasoning") {
            partial.insert(
                "generation_reasoning".to_string(),
                json!("AI-generated team based on requirements"),
            );
        }

        if !partial.contains_key("team_structure") {
            partial.insert(
                "team_structure".to_string(),
                self.create_default_team_structure_json(),
            );
        }
    }

    fn create_default_team_structure_json(&self) -> Value {
        json!({
            "team_lead": "Team Coordinator",
            "collaboration_patterns": [],
            "role_dependencies": {},
            "communication_structure": {
                "hub_agent": "Team Coordinator",
                "direct_pairs": [],
                "broadcast_triggers": ["milestone_reached"]
            }
        })
    }

    fn convert_llm_response_to_agents(
        &self,
        response: AgentTeamResponse,
        config: &GenerationConfig,
    ) -> AgentResult<Vec<Agent>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut agents = Vec::new();
        for llm_agent in response.agents {
            let technical_skills = llm_agent
                .technical_skills
                .into_iter()
                .map(|skill| TechnicalSkill {
                    name: skill.name,
                    proficiency: skill.proficiency,
                    experience_years: self.parse_experience_years(&skill.experience_description),
                    domains: skill.domains,
                })
                .collect();
            let performance_metrics = PerformanceMetrics {
                success_rate: llm_agent.expected_performance.success_rate_estimate,
                avg_completion_time: llm_agent.expected_performance.completion_time_estimate,
                quality_score: llm_agent.expected_performance.quality_score_estimate,
                collaboration_score: llm_agent.expected_performance.collaboration_score_estimate,
                tasks_completed: 0,
                innovation_score: llm_agent.expected_performance.innovation_score_estimate,
            };
            let capabilities = AgentCapabilities {
                personality_traits: llm_agent.personality_traits,
                strengths: llm_agent.strengths,
                approach_style: llm_agent.approach_style,
                competitive_edge: llm_agent.competitive_edge,
                risk_tolerance: llm_agent.risk_tolerance,
                collaboration_preference: llm_agent.collaboration_preference,
                technical_skills,
                performance_metrics,
                runtime_capabilities: RuntimeCapabilities {
                    can_create_subtasks: false,
                    can_modify_workflow: false,
                    can_access_external_apis: false,
                    can_delegate_tasks: false,
                    max_concurrent_executions: 1,
                    trusted_execution_level: self.fallback_config.default_trust_level.clone(),
                    ffi_permissions: self.fallback_config.default_ffi_permissions.clone(),
                    gas_limit: self.fallback_config.default_gas_limit,
                    execution_timeout_secs: self.fallback_config.default_execution_timeout_secs,
                },
            };
            let metadata = AgentMetadata {
                created_at: now,
                updated_at: now,
                version: 1,
                created_by: "llm_generator".to_string(),
                generation_method: GenerationMethod::LLMGenerated {
                    model: config.model_config.model.clone(),
                    prompt_version: "1.0".to_string(),
                },
                tags: vec!["llm_generated".to_string(), "team_member".to_string()],
                performance_tracking: true,
            };
            let agent = Agent {
                id: Uuid::new_v4().to_string(),
                name: llm_agent.name,
                role: llm_agent.role,
                specialisation: llm_agent.specialisation,
                capabilities,
                status: AgentStatus::Available,
                metadata,
                custom_properties: HashMap::new(),
            };
            agents.push(agent);
        }
        Ok(agents)
    }
    fn parse_experience_years(&self, description: &str) -> f64 {
        if description.contains("novice") || description.contains("beginner") {
            1.0
        } else if description.contains("intermediate") {
            3.0
        } else if description.contains("advanced") || description.contains("expert") {
            7.0
        } else if description.contains("senior") {
            10.0
        } else {
            5.0
        }
    }
    fn create_fallback_team_response(&self) -> AgentTeamResponse {
        let agents = self
            .fallback_config
            .fallback_agents
            .iter()
            .map(|template| self.convert_fallback_template_to_llm_agent(template))
            .collect();

        AgentTeamResponse {
            agents,
            team_dynamics: "Complementary strategic and implementation focus with strong collaborative potential".to_string(),
            collaborative_potential: 8.5,
            diversity_score: 7.8,
            generation_reasoning: "Selected from configured fallback templates for reliable team composition".to_string(),
            team_structure: self.create_default_team_structure(),
        }
    }

    fn convert_fallback_template_to_llm_agent(
        &self,
        template: &FallbackAgentTemplate,
    ) -> LLMAgentData {
        LLMAgentData {
            name: template.name.clone(),
            role: template.role.clone(),
            specialisation: template.specialisation.clone(),
            personality_traits: template.personality_traits.clone(),
            strengths: template.strengths.clone(),
            approach_style: template.approach_style.clone(),
            competitive_edge: template.competitive_edge.clone(),
            risk_tolerance: template.risk_tolerance,
            collaboration_preference: template.collaboration_preference.clone(),
            technical_skills: template
                .technical_skills
                .iter()
                .map(|skill| LLMTechnicalSkill {
                    name: skill.name.clone(),
                    proficiency: skill.proficiency,
                    experience_description: skill.experience_description.clone(),
                    domains: skill.domains.clone(),
                })
                .collect(),
            expected_performance: template.expected_performance.clone(),
        }
    }

    fn create_default_team_structure(&self) -> TeamStructure {
        let team_lead = self
            .fallback_config
            .fallback_agents
            .first()
            .map(|agent| agent.name.clone());

        let collaboration_patterns = if self.fallback_config.fallback_agents.len() >= 2 {
            vec![CollaborationPattern {
                agents: self
                    .fallback_config
                    .fallback_agents
                    .iter()
                    .take(2)
                    .map(|agent| agent.name.clone())
                    .collect(),
                pattern_type: CollaborationPatternType::Sequential,
                description: "Strategic analysis followed by technical implementation".to_string(),
            }]
        } else {
            vec![]
        };

        let role_dependencies = if self.fallback_config.fallback_agents.len() >= 2 {
            let mut deps = HashMap::new();
            deps.insert(
                self.fallback_config.fallback_agents[1].name.clone(),
                vec![self.fallback_config.fallback_agents[0].name.clone()],
            );
            deps
        } else {
            HashMap::new()
        };

        let direct_pairs = if self.fallback_config.fallback_agents.len() >= 2 {
            vec![(
                self.fallback_config.fallback_agents[0].name.clone(),
                self.fallback_config.fallback_agents[1].name.clone(),
            )]
        } else {
            vec![]
        };

        TeamStructure {
            team_lead: team_lead.clone(),
            collaboration_patterns,
            role_dependencies,
            communication_structure: CommunicationStructure {
                hub_agent: team_lead,
                direct_pairs,
                broadcast_triggers: vec![
                    "milestone_complete".to_string(),
                    "strategy_change".to_string(),
                ],
            },
        }
    }
}
impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            default_model: "claude-sonnet-4-20250514".to_string(),
            max_retries: 3,
            temperature: 0.7,
            max_tokens: 4096,
            enable_caching: true,
        }
    }
}
impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            team_size: 3,
            task_context: "General problem solving".to_string(),
            requirements: Vec::new(),
            diversity_requirements: DiversityRequirements::default(),
            performance_expectations: PerformanceExpectations::default(),
            model_config: ModelConfig::default(),
        }
    }
}
impl Default for DiversityRequirements {
    fn default() -> Self {
        Self {
            min_specialisations: 2,
            diverse_approach_styles: true,
            diverse_risk_tolerance: true,
            diverse_collaboration_styles: true,
            min_diversity_score: 0.7,
        }
    }
}
impl DiversityRequirements {
    pub fn minimal() -> Self {
        Self {
            min_specialisations: 1,
            diverse_approach_styles: false,
            diverse_risk_tolerance: false,
            diverse_collaboration_styles: false,
            min_diversity_score: 0.0,
        }
    }
}
impl Default for PerformanceExpectations {
    fn default() -> Self {
        Self {
            min_success_rate: 0.7,
            min_quality_score: 0.7,
            min_collaboration_score: 0.6,
            min_innovation_score: 0.5,
        }
    }
}
impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-20250514".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            custom_params: HashMap::new(),
        }
    }
}
