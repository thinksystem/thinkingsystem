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

use crate::agents::Agent;
use crate::tasks::Task;
use serde_json::Value;

pub mod fields {
    pub const GOAL_ACHIEVED: &str = "goal_achieved";
    pub const CONFIDENCE: &str = "confidence";
    pub const REASONING: &str = "reasoning";
    pub const MISSING_ELEMENTS: &str = "missing_elements";
    pub const CHANGES_MADE: &str = "changes_made";
    pub const DETAILS: &str = "details";
    pub const CONCEPT: &str = "concept";
    pub const SUMMARY: &str = "summary";
    pub const SCORE: &str = "score";
    pub const STRENGTHS: &str = "strengths";
    pub const CONCERNS: &str = "concerns";
    pub const SUGGESTIONS: &str = "suggestions";
    pub const PRIORITISED_CONCERNS: &str = "prioritised_concerns";
    pub const ACTIONABLE_SUGGESTIONS: &str = "actionable_suggestions";
    pub const SITUATION_SUMMARY: &str = "situation_summary";
    pub const CRITICAL_BLOCKERS: &str = "critical_blockers";
    pub const RECOMMENDED_FOCUS: &str = "recommended_focus";
}

pub mod json_utils {
    use serde_json::Value;

    pub fn safe_json_serialise(value: &Value) -> anyhow::Result<String> {
        serde_json::to_string_pretty(value)
            .map_err(|e| anyhow::anyhow!("JSON serialisation failed: {}", e))
    }

    pub fn safe_json_serialise_with_fallback(value: &Value) -> String {
        safe_json_serialise(value).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn validate_refinement_changes(
        current_proposal: &Value,
        refined_proposal: &Value,
    ) -> anyhow::Result<()> {
        let changes_made = refined_proposal
            .get(super::fields::CHANGES_MADE)
            .and_then(|c| c.as_array())
            .map(|arr| arr.len())
            .unwrap_or(0);

        if changes_made == 0 {
            return Err(anyhow::anyhow!(
                "Refinement failed: No changes were made to the proposal. LLM did not provide any changes_made entries."
            ));
        }

        let current_details_str = safe_json_serialise(
            current_proposal
                .get(super::fields::DETAILS)
                .unwrap_or(&Value::Null),
        )?;
        let refined_details_str = safe_json_serialise(
            refined_proposal
                .get(super::fields::DETAILS)
                .unwrap_or(&Value::Null),
        )?;

        if current_details_str == refined_details_str {
            return Err(anyhow::anyhow!(
                "Refinement failed: LLM claimed changes, but proposal 'details' are identical. Halting to prevent hallucinated progress."
            ));
        }

        Ok(())
    }
}

pub struct CollaborationPrompts;

impl CollaborationPrompts {
    pub fn distil_feedback_prompt(
        task: &Task,
        proposal: &Value,
        all_feedback: &[Value],
    ) -> (String, String) {
        let system_prompt = "You are a Feedback Distillation Agent. Your task is to analyse feedback from multiple specialists and synthesise it into a single, non-redundant, and prioritised set of actionable instructions. Consolidate similar points, identify contradictions, and summarise the core message.".to_string();

        let user_prompt = format!(
            "GOAL: {}\n\nPROPOSAL CONCEPT:\n{}\n\nRAW TEAM FEEDBACK:\n{}\n\nSynthesize the raw feedback into a unified summary. De-duplicate points and prioritise the most critical concerns that block progress. \n\nRespond with JSON: {{ \"summary\": \"(string) one-paragraph overview of the team's sentiment\", \"prioritised_concerns\": [\"(string) list of the most critical issues to fix\"], \"actionable_suggestions\": [\"(string) list of concrete, consolidated suggestions\"] }}",
            task.description,
            proposal.get(fields::CONCEPT).unwrap_or(&Value::Null),
            json_utils::safe_json_serialise_with_fallback(&Value::Array(all_feedback.to_vec()))
        );

        (system_prompt, user_prompt)
    }

    pub fn initial_proposal_prompt(lead_agent: &Agent, task: &Task) -> (String, String) {
        let system_prompt = format!(
            "You are {}, a {} specialising in {}. Your task is to create the team's INITIAL proposal.",
            lead_agent.name, lead_agent.role, lead_agent.specialisation
        );

        let breakout_focus = task
            .metadata
            .get("breakout_summary")
            .and_then(|s| s.get(fields::RECOMMENDED_FOCUS))
            .and_then(|f| f.as_str())
            .map(|f| format!("\nCRITICAL FOCUS (from previous stalemate): {f}"))
            .unwrap_or_default();

        let user_prompt = format!(
            "TASK: {}{}\n\nCreate a SINGLE, SPECIFIC, and DETAILED initial proposal to achieve the task. This will be the foundation for team collaboration. Be concrete and implementable. \n\nRespond with JSON containing: 'concept' (string), 'details' (object with specific elements), 'rationale' (string explaining the approach).",
            task.description,
            breakout_focus
        );

        (system_prompt, user_prompt)
    }

    pub fn refine_proposal_prompt(
        lead_specialist: &Agent,
        task: &Task,
        current_proposal: &Value,
        distilled_feedback: &Value,
        iteration: u32,
    ) -> (String, String) {
        let system_prompt = format!(
            "You are a refinement specialist, embodying the role of {}. Your task is to intelligently upgrade a proposal based on team feedback.",
            lead_specialist.name
        );

        let user_prompt = format!(
            "TASK: {}\nITERATION: {}\n\nCURRENT PROPOSAL:\n{}\n\nDISTILLED TEAM FEEDBACK (Prioritised):\n{}\n\nYou MUST refine the proposal by addressing the specific concerns and suggestions from the feedback. Make concrete, technical changes to the 'details' object.\n\nCRITICAL REQUIREMENTS:\n1. The `details` object MUST be changed. Do not just change the rationale.\n2. The `changes_made` array MUST list the specific, technical modifications (e.g., 'Changed colour from #FFFFFF to #000000').\n3. Do NOT list conceptual changes like 'conducted research'.\n\nRespond with JSON containing: 'concept' (string), 'details' (UPDATED object), 'rationale' (string explaining changes), 'changes_made' (array of strings describing technical changes).",
            task.description,
            iteration,
            json_utils::safe_json_serialise_with_fallback(current_proposal),
            json_utils::safe_json_serialise_with_fallback(distilled_feedback)
        );

        (system_prompt, user_prompt)
    }

    pub fn specialist_feedback_prompt(
        agent: &Agent,
        task: &Task,
        proposal: &Value,
        iteration: u32,
    ) -> (String, String) {
        let system_prompt = format!(
            "You are {}, a {} specialising in {}. Provide expert feedback on a proposal from your domain.",
            agent.name, agent.role, agent.specialisation
        );

        let user_prompt = format!(
            "TASK: {}\nITERATION: {}\n\nTEAM PROPOSAL TO REVIEW:\n{}\n\nAnalyse this from your specific area of expertise ({}). Provide constructive, actionable feedback. \n\nRespond with JSON containing: 'strengths' (array of strings), 'concerns' (array of strings), 'suggestions' (array of strings).",
            task.description,
            iteration,
            json_utils::safe_json_serialise_with_fallback(proposal),
            agent.specialisation
        );

        (system_prompt, user_prompt)
    }

    pub fn assess_proposal_prompt(
        arbiter: &Agent,
        task: &Task,
        proposal: &Value,
        distilled_feedback: &Value,
        iteration: u32,
    ) -> (String, String) {
        let system_prompt = format!(
            "You are {}, an arbiter with expertise in {}. Your role is to be a STRICT and IMPARTIAL judge of whether a proposal definitively achieves the goal. YOU MUST RESPOND ONLY WITH VALID JSON - NO OTHER TEXT.",
            arbiter.name, arbiter.specialisation
        );

        let user_prompt = format!(
            "GOAL: {}\nITERATION: {}\n\nPROPOSAL TO ASSESS:\n{}\n\nDISTILLED TEAM FEEDBACK:\n{}\n\nPerform a RIGOROUS assessment. The goal is achieved ONLY if all critical concerns from the team feedback have been demonstrably resolved in the proposal details and the solution is complete and robust. Surface-level coherence is insufficient.\n\nCRITICAL: You MUST respond with ONLY valid JSON in this exact format, with no additional text, explanations, or markdown formatting:\n\n{{\n  \"goal_achieved\": true,\n  \"confidence\": 0.85,\n  \"reasoning\": \"Your detailed analysis here\",\n  \"missing_elements\": [\"List any remaining gaps\"]\n}}\n\nThe goal_achieved field MUST be either true or false (boolean).\nThe confidence field MUST be a number between 0.0 and 1.0.\nDo not include any text before or after the JSON.",
            task.description,
            iteration,
            json_utils::safe_json_serialise_with_fallback(proposal),
            json_utils::safe_json_serialise_with_fallback(distilled_feedback)
        );

        (system_prompt, user_prompt)
    }

    pub fn breakout_strategy_prompt(
        task: &Task,
        current_proposal: &Option<Value>,
        all_feedback_history: &[Value],
        iteration: u32,
    ) -> (String, String) {
        let system_prompt = "You are a Summarization Agent for breaking collaboration deadlocks. Analyse a stalled process and create a concise, actionable summary to help the team refocus.".to_string();

        let user_prompt = format!(
            "GOAL: {}\nSTALLED AT ITERATION: {}\n\nThe team is stuck. Analyse the situation based on the final proposal and the history of feedback. Identify the core disagreements and a clear path forward.\n\nFINAL PROPOSAL:\n{}\n\nFEEDBACK HISTORY (Summaries):\n{}\n\nCreate a summary identifying:\n1. Core unresolved issues.\n2. A clear, single recommendation for the team's next focus.\n\nRespond with JSON: {{ 'situation_summary': \"(string)\", 'critical_blockers': [\"(string)\"], 'recommended_focus': \"(string) directive for the team's next attempt\" }}",
            task.description,
            iteration,
            json_utils::safe_json_serialise_with_fallback(current_proposal.as_ref().unwrap_or(&Value::Null)),
            json_utils::safe_json_serialise_with_fallback(&Value::Array(all_feedback_history.to_vec()))
        );

        (system_prompt, user_prompt)
    }

    pub fn progress_evaluation_prompt(
        previous_proposal: Option<&Value>,
        previous_feedback: Option<&Value>,
        current_proposal: &Value,
        current_feedback: &Value,
        iteration: u32,
    ) -> (String, String) {
        let system_prompt = "You are a Progress Evaluation Agent. Objectively score the meaningful progress between two iterations on a scale of 1-10. 1=Regressive/Superficial, 5=Moderate, 10=Exceptional.".to_string();

        let user_prompt = format!(
            "ITERATION: {iteration}\n\nEvaluate if the CURRENT PROPOSAL meaningfully addresses the PREVIOUS FEEDBACK compared to the PREVIOUS PROPOSAL. Focus on concrete changes listed in 'changes_made'.\n\nPREVIOUS PROPOSAL:\n{}\n\nPREVIOUS FEEDBACK:\n{}\n\nCURRENT PROPOSAL'S CLAIMED CHANGES:\n{}\n\nCURRENT FEEDBACK SUMMARY:\n{}\n\nVerify if the changes are real and address the core of the feedback. Superficial changes (wording, version bumps) are low-score. Significant technical improvements that resolve concerns are high-score.\n\nRespond with JSON: {{ 'score': (integer 1-10), 'reasoning': \"(string) Justify the score based on change verification.\" }}",
            json_utils::safe_json_serialise_with_fallback(previous_proposal.unwrap_or(&Value::Null)),
            json_utils::safe_json_serialise_with_fallback(previous_feedback.unwrap_or(&Value::Null)),
            current_proposal.get(fields::CHANGES_MADE).unwrap_or(&Value::Null),
            json_utils::safe_json_serialise_with_fallback(current_feedback)
        );

        (system_prompt, user_prompt)
    }
}
