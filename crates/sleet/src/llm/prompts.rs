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

use crate::llm::{LLMError, LLMResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    pub name: String,
    pub description: String,
    pub system_template: String,
    pub user_template: String,

    pub variables: Vec<String>,

    pub metadata: HashMap<String, Value>,
}

impl PromptTemplate {
    pub fn new(
        name: impl Into<String>,
        system_template: impl Into<String>,
        user_template: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            system_template: system_template.into(),
            user_template: user_template.into(),
            variables: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_variables(mut self, variables: Vec<String>) -> Self {
        self.variables = variables;
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

pub type PromptContext = HashMap<String, Value>;

#[derive(Debug, Default)]
pub struct PromptBuilder {
    templates: HashMap<String, PromptTemplate>,
}

impl PromptBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_agent_templates() -> Self {
        let mut builder = Self::new();
        builder.add_agent_collaboration_templates();
        builder
    }

    pub fn add_template(&mut self, template: PromptTemplate) -> &mut Self {
        self.templates.insert(template.name.clone(), template);
        self
    }

    pub fn remove_template(&mut self, name: &str) -> Option<PromptTemplate> {
        self.templates.remove(name)
    }

    pub fn build_prompt(
        &self,
        template_name: &str,
        context: &PromptContext,
    ) -> LLMResult<(String, String)> {
        let template = self.templates.get(template_name).ok_or_else(|| {
            LLMError::ConfigError(format!("Template '{template_name}' not found"))
        })?;

        let system_prompt = self.substitute_variables(&template.system_template, context)?;
        let user_prompt = self.substitute_variables(&template.user_template, context)?;

        debug!(
            "Built prompt from template '{}' with {} context variables",
            template_name,
            context.len()
        );
        Ok((system_prompt, user_prompt))
    }

    pub fn list_templates(&self) -> Vec<String> {
        self.templates.keys().cloned().collect()
    }

    pub fn get_template(&self, name: &str) -> Option<&PromptTemplate> {
        self.templates.get(name)
    }

    pub fn validate_context(&self, template_name: &str, context: &PromptContext) -> LLMResult<()> {
        let template = self.templates.get(template_name).ok_or_else(|| {
            LLMError::ConfigError(format!("Template '{template_name}' not found"))
        })?;

        let missing_vars: Vec<&String> = template
            .variables
            .iter()
            .filter(|var| !context.contains_key(*var))
            .collect();

        if !missing_vars.is_empty() {
            return Err(LLMError::ConfigError(format!(
                "Missing required variables for template '{template_name}': {missing_vars:?}"
            )));
        }

        Ok(())
    }

    fn substitute_variables(&self, template: &str, context: &PromptContext) -> LLMResult<String> {
        let mut result = template.to_string();

        for (key, value) in context {
            let placeholder = format!("{{{{{key}}}}}");
            let substitution = self.value_to_string(value);
            result = result.replace(&placeholder, &substitution);
        }

        if result.contains("{{") && result.contains("}}") {
            warn!("Template contains unsubstituted placeholders: {}", result);
        }

        Ok(result)
    }

    fn value_to_string(&self, value: &Value) -> String {
        match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".to_string(),
            Value::Array(_) | Value::Object(_) => {
                serde_json::to_string_pretty(value).unwrap_or_else(|_| "invalid_json".to_string())
            }
        }
    }

    pub fn add_agent_collaboration_templates(&mut self) {
        self.add_template(PromptTemplate::new(
            "distil_feedback",
            "You are a Feedback Distillation Agent. Your task is to analyse feedback from multiple specialists and synthesise it into a single, non-redundant, and prioritised set of actionable instructions. Consolidate similar points, identify contradictions, and summarise the core message.",
            r#"GOAL: {{goal}}

PROPOSAL CONCEPT:
{{proposal_concept}}

RAW TEAM FEEDBACK:
{{raw_feedback}}

Synthesise the raw feedback into a unified summary. De-duplicate points and prioritise the most critical concerns that block progress.

Respond with JSON: {
  "summary": "(string) one-paragraph overview of the team's sentiment",
  "prioritised_concerns": ["(string) list of the most critical issues to fix"],
  "actionable_suggestions": ["(string) list of concrete, consolidated suggestions"]
}"#
        ).with_variables(vec!["goal".to_string(), "proposal_concept".to_string(), "raw_feedback".to_string()]));

        self.add_template(PromptTemplate::new(
            "initial_proposal",
            "You are {{agent_name}}, a {{agent_role}} specialising in {{agent_specialisation}}. Your task is to create the team's INITIAL proposal.",
            r#"TASK: {{task_description}}
{{focus_directive}}

Create a SINGLE, SPECIFIC, and DETAILED initial proposal to achieve the task. This will be the foundation for team collaboration. Be concrete and implementable.

Respond with JSON containing:
- 'concept' (string): High-level approach
- 'details' (object): Specific implementation elements
- 'rationale' (string): Explanation of the approach"#
        ).with_variables(vec![
            "agent_name".to_string(),
            "agent_role".to_string(),
            "agent_specialisation".to_string(),
            "task_description".to_string(),
            "focus_directive".to_string()
        ]));

        self.add_template(PromptTemplate::new(
            "refine_proposal",
            "You are a refinement specialist, embodying the role of {{agent_name}}. Your task is to intelligently upgrade a proposal based on team feedback.",
            r#"TASK: {{task_description}}
ITERATION: {{iteration}}

CURRENT PROPOSAL:
{{current_proposal}}

DISTILLED TEAM FEEDBACK (Prioritised):
{{distilled_feedback}}

You MUST refine the proposal by addressing the specific concerns and suggestions from the feedback. Make concrete, technical changes to the 'details' object.

CRITICAL REQUIREMENTS:
1. The `details` object MUST be changed. Do not just change the rationale.
2. The `changes_made` array MUST list the specific, technical modifications.
3. Do NOT list conceptual changes like 'conducted research'.

Respond with JSON containing:
- 'concept' (string): Updated concept
- 'details' (UPDATED object): Modified implementation details
- 'rationale' (string): Explanation of changes
- 'changes_made' (array): List of specific technical modifications"#
        ).with_variables(vec![
            "agent_name".to_string(),
            "task_description".to_string(),
            "iteration".to_string(),
            "current_proposal".to_string(),
            "distilled_feedback".to_string()
        ]));

        self.add_template(PromptTemplate::new(
            "specialist_feedback",
            "You are {{agent_name}}, a {{agent_role}} specialising in {{agent_specialisation}}. Provide expert feedback on a proposal from your domain.",
            r#"TASK: {{task_description}}
ITERATION: {{iteration}}

TEAM PROPOSAL TO REVIEW:
{{proposal}}

Analyse this from your specific area of expertise ({{agent_specialisation}}). Provide constructive, actionable feedback.

Respond with JSON containing:
- 'strengths' (array): Positive aspects of the proposal
- 'concerns' (array): Issues or problems identified
- 'suggestions' (array): Specific recommendations for improvement"#
        ).with_variables(vec![
            "agent_name".to_string(),
            "agent_role".to_string(),
            "agent_specialisation".to_string(),
            "task_description".to_string(),
            "iteration".to_string(),
            "proposal".to_string()
        ]));

        self.add_template(PromptTemplate::new(
            "assess_proposal",
            "You are {{arbiter_name}}, an arbiter with expertise in {{arbiter_specialisation}}. Your role is to be a STRICT and IMPARTIAL judge of whether a proposal definitively achieves the goal. YOU MUST RESPOND ONLY WITH VALID JSON - NO OTHER TEXT.",
            r#"GOAL: {{goal}}
ITERATION: {{iteration}}

PROPOSAL TO ASSESS:
{{proposal}}

DISTILLED TEAM FEEDBACK:
{{distilled_feedback}}

Perform a RIGOROUS assessment. The goal is achieved ONLY if all critical concerns from the team feedback have been demonstrably resolved in the proposal details and the solution is complete and robust. Surface-level coherence is insufficient.

CRITICAL: You MUST respond with ONLY valid JSON in this exact format, with no additional text, explanations, or markdown formatting:

{
  "goal_achieved": true,
  "confidence": 0.85,
  "reasoning": "Your detailed analysis here",
  "missing_elements": ["List any remaining gaps"]
}

The goal_achieved field MUST be either true or false (boolean).
The confidence field MUST be a number between 0.0 and 1.0.
Do not include any text before or after the JSON."#
        ).with_variables(vec![
            "arbiter_name".to_string(),
            "arbiter_specialisation".to_string(),
            "goal".to_string(),
            "iteration".to_string(),
            "proposal".to_string(),
            "distilled_feedback".to_string()
        ]));

        self.add_template(PromptTemplate::new(
            "evaluate_progress",
            "You are a Progress Evaluation Agent. Objectively score the meaningful progress between two iterations on a scale of 1-10. 1=Regressive/Superficial, 5=Moderate, 10=Exceptional.",
            r#"ITERATION: {{iteration}}

Evaluate if the CURRENT PROPOSAL meaningfully addresses the PREVIOUS FEEDBACK compared to the PREVIOUS PROPOSAL. Focus on concrete changes listed in 'changes_made'.

PREVIOUS PROPOSAL:
{{previous_proposal}}

PREVIOUS FEEDBACK:
{{previous_feedback}}

CURRENT PROPOSAL'S CLAIMED CHANGES:
{{claimed_changes}}

CURRENT FEEDBACK SUMMARY:
{{current_feedback}}

Verify if the changes are real and address the core of the feedback. Superficial changes (wording, version bumps) are low-score. Significant technical improvements that resolve concerns are high-score.

Respond with JSON:
{
  "score": (integer 1-10),
  "reasoning": "(string) Justify the score based on change verification."
}"#
        ).with_variables(vec![
            "iteration".to_string(),
            "previous_proposal".to_string(),
            "previous_feedback".to_string(),
            "claimed_changes".to_string(),
            "current_feedback".to_string()
        ]));

        self.add_template(PromptTemplate::new(
            "breakout_strategy",
            "You are a Summarization Agent for breaking collaboration deadlocks. Analyse a stalled process and create a concise, actionable summary to help the team refocus.",
            r#"GOAL: {{goal}}
STALLED AT ITERATION: {{iteration}}

The team is stuck. Analyse the situation based on the final proposal and the history of feedback. Identify the core disagreements and a clear path forward.

FINAL PROPOSAL:
{{final_proposal}}

FEEDBACK HISTORY (Summaries):
{{feedback_history}}

Create a summary identifying:
1. Core unresolved issues.
2. A clear, single recommendation for the team's next focus.

Respond with JSON:
{
  "situation_summary": "(string)",
  "critical_blockers": ["(string)"],
  "recommended_focus": "(string) directive for the team's next attempt"
}"#
        ).with_variables(vec![
            "goal".to_string(),
            "iteration".to_string(),
            "final_proposal".to_string(),
            "feedback_history".to_string()
        ]));
    }
}

pub mod context_builders {
    use super::*;
    use serde_json::json;

    pub fn agent_context(
        agent_name: &str,
        agent_role: &str,
        agent_specialisation: &str,
    ) -> PromptContext {
        let mut context = PromptContext::new();
        context.insert("agent_name".to_string(), json!(agent_name));
        context.insert("agent_role".to_string(), json!(agent_role));
        context.insert(
            "agent_specialisation".to_string(),
            json!(agent_specialisation),
        );
        context
    }

    pub fn task_context(task_description: &str, iteration: u32) -> PromptContext {
        let mut context = PromptContext::new();
        context.insert("task_description".to_string(), json!(task_description));
        context.insert("iteration".to_string(), json!(iteration));
        context
    }

    pub fn proposal_context(proposal: &Value, feedback: Option<&Value>) -> PromptContext {
        let mut context = PromptContext::new();
        context.insert("proposal".to_string(), proposal.clone());
        if let Some(feedback) = feedback {
            context.insert("feedback".to_string(), feedback.clone());
        }
        context
    }

    pub fn merge_contexts(contexts: &[PromptContext]) -> PromptContext {
        let mut merged = PromptContext::new();
        for context in contexts {
            merged.extend(context.clone());
        }
        merged
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_prompt_template_creation() {
        let template = PromptTemplate::new(
            "test_template",
            "You are a test assistant.",
            "Please help with {{task}}",
        )
        .with_description("A test template")
        .with_variables(vec!["task".to_string()]);

        assert_eq!(template.name, "test_template");
        assert_eq!(template.description, "A test template");
        assert_eq!(template.variables, vec!["task"]);
    }

    #[test]
    fn test_prompt_builder() {
        let mut builder = PromptBuilder::new();

        let template = PromptTemplate::new(
            "greeting",
            "You are a helpful assistant.",
            "Hello {{name}}, how can I help you with {{task}}?",
        )
        .with_variables(vec!["name".to_string(), "task".to_string()]);

        builder.add_template(template);

        let mut context = PromptContext::new();
        context.insert("name".to_string(), json!("Alice"));
        context.insert("task".to_string(), json!("coding"));

        let (system_prompt, user_prompt) = builder.build_prompt("greeting", &context).unwrap();

        assert_eq!(system_prompt, "You are a helpful assistant.");
        assert_eq!(user_prompt, "Hello Alice, how can I help you with coding?");
    }

    #[test]
    fn test_agent_templates() {
        let builder = PromptBuilder::with_agent_templates();
        let templates = builder.list_templates();

        assert!(templates.contains(&"distil_feedback".to_string()));
        assert!(templates.contains(&"initial_proposal".to_string()));
        assert!(templates.contains(&"assess_proposal".to_string()));
    }

    #[test]
    fn test_context_validation() {
        let mut builder = PromptBuilder::new();

        let template = PromptTemplate::new("test", "System", "User: {{required_var}}")
            .with_variables(vec!["required_var".to_string()]);

        builder.add_template(template);

        let mut context = PromptContext::new();

        assert!(builder.validate_context("test", &context).is_err());

        context.insert("required_var".to_string(), json!("value"));
        assert!(builder.validate_context("test", &context).is_ok());
    }
}
