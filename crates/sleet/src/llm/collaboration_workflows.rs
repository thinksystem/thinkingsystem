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
use crate::llm::LLMManager;
use crate::tasks::Task;
use anyhow::Result;
use serde_json::Value;

use super::collaboration::CollaborationPrompts;

pub async fn distil_feedback(
    llm_manager: &LLMManager,
    task: &Task,
    proposal: &Value,
    all_feedback: &[Value],
) -> Result<Value> {
    let (system_prompt, user_prompt) =
        CollaborationPrompts::distil_feedback_prompt(task, proposal, all_feedback);

    llm_manager
        .generate_structured_response_with_fallback(&system_prompt, &user_prompt)
        .await
        .map_err(|e| anyhow::anyhow!("Feedback distillation failed: {e}"))
}

pub async fn get_initial_proposal(
    llm_manager: &LLMManager,
    lead_agent: &Agent,
    task: &Task,
) -> Result<Value> {
    let (system_prompt, user_prompt) =
        CollaborationPrompts::initial_proposal_prompt(lead_agent, task);

    llm_manager
        .generate_structured_response_with_fallback(&system_prompt, &user_prompt)
        .await
        .map_err(|e| anyhow::anyhow!("Initial proposal failed: {e}"))
}

pub async fn refine_proposal(
    llm_manager: &LLMManager,
    specialists: &[Agent],
    task: &Task,
    current_proposal: &Value,
    distilled_feedback: &Value,
    iteration: u32,
) -> Result<Value> {
    let lead_specialist = &specialists[0];
    let (system_prompt, user_prompt) = CollaborationPrompts::refine_proposal_prompt(
        lead_specialist,
        task,
        current_proposal,
        distilled_feedback,
        iteration,
    );

    llm_manager
        .generate_structured_response_with_fallback(&system_prompt, &user_prompt)
        .await
        .map_err(|e| anyhow::anyhow!("Proposal refinement failed: {e}"))
}

pub async fn get_specialist_feedback(
    llm_manager: &LLMManager,
    agent: &Agent,
    task: &Task,
    proposal: &Value,
    iteration: u32,
) -> Result<Value> {
    let (system_prompt, user_prompt) =
        CollaborationPrompts::specialist_feedback_prompt(agent, task, proposal, iteration);

    llm_manager
        .generate_structured_response_with_fallback(&system_prompt, &user_prompt)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Specialist feedback from {} failed: {}",
                agent.name,
                e.to_string()
            )
        })
}

pub async fn assess_proposal_quality(
    llm_manager: &LLMManager,
    arbiter: &Agent,
    task: &Task,
    proposal: &Value,
    distilled_feedback: &Value,
    iteration: u32,
) -> Result<Value> {
    let (system_prompt, user_prompt) = CollaborationPrompts::assess_proposal_prompt(
        arbiter,
        task,
        proposal,
        distilled_feedback,
        iteration,
    );

    llm_manager
        .generate_structured_response_with_fallback(&system_prompt, &user_prompt)
        .await
        .map_err(|e| anyhow::anyhow!("Arbiter assessment failed: {e}"))
}

pub async fn apply_breakout_strategy(
    llm_manager: &LLMManager,
    task: &Task,
    current_proposal: &Option<Value>,
    all_feedback_history: &[Value],
    iteration: u32,
) -> Result<Value> {
    let (system_prompt, user_prompt) = CollaborationPrompts::breakout_strategy_prompt(
        task,
        current_proposal,
        all_feedback_history,
        iteration,
    );

    llm_manager
        .generate_structured_response_with_fallback(&system_prompt, &user_prompt)
        .await
        .map_err(|e| anyhow::anyhow!("Breakout strategy failed: {e}"))
}

pub async fn evaluate_progress_score(
    llm_manager: &LLMManager,
    previous_proposal: Option<&Value>,
    previous_feedback: Option<&Value>,
    current_proposal: &Value,
    current_feedback: &Value,
    iteration: u32,
) -> Result<u8> {
    let (system_prompt, user_prompt) = CollaborationPrompts::progress_evaluation_prompt(
        previous_proposal,
        previous_feedback,
        current_proposal,
        current_feedback,
        iteration,
    );

    let response = llm_manager
        .generate_structured_response_with_fallback(&system_prompt, &user_prompt)
        .await
        .map_err(|e| anyhow::anyhow!("Progress evaluation failed: {}", e))?;

    use super::collaboration::fields;
    Ok(response
        .get(fields::SCORE)
        .and_then(Value::as_u64)
        .map(|s| s as u8)
        .unwrap_or(1))
}
