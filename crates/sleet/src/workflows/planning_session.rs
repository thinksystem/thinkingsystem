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
use crate::llm::{
    apply_breakout_strategy, assess_proposal_quality, distil_feedback, evaluate_progress_score,
    get_initial_proposal, get_specialist_feedback, refine_proposal, LLMManager, ProgressConfig,
    ProgressTracker,
};
use crate::tasks::{Task, TaskStatus};
use anyhow::Result;
use futures::future::join_all;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct PlanningSessionConfig {
    pub max_iterations: usize,
    pub min_confidence_for_completion: f64,
    pub fast_ema_alpha: f64,
    pub slow_ema_alpha: f64,
    pub momentum_threshold: f64,
    pub plateau_threshold: f64,
    pub min_history_for_plateau: usize,
}

impl Default for PlanningSessionConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            min_confidence_for_completion: 0.8,
            fast_ema_alpha: 0.3,
            slow_ema_alpha: 0.1,
            momentum_threshold: 0.02,
            plateau_threshold: 0.01,
            min_history_for_plateau: 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlanningSessionState {
    pub iteration: usize,
    pub current_proposal: Option<Value>,
    pub all_feedback_history: Vec<Value>,
    pub config: PlanningSessionConfig,
}

impl Default for PlanningSessionState {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanningSessionState {
    pub fn new() -> Self {
        Self::with_config(PlanningSessionConfig::default())
    }

    pub fn with_config(config: PlanningSessionConfig) -> Self {
        Self {
            iteration: 0,
            current_proposal: None,
            all_feedback_history: Vec::new(),
            config,
        }
    }

    pub fn increment_iteration(&mut self) {
        self.iteration += 1;
    }

    pub fn is_first_iteration(&self) -> bool {
        self.iteration == 1
    }

    pub fn has_reached_max_iterations(&self) -> bool {
        self.iteration >= self.config.max_iterations
    }

    pub fn set_current_proposal(&mut self, proposal: Value) {
        self.current_proposal = Some(proposal);
    }

    pub fn add_feedback(&mut self, feedback: Value) {
        self.all_feedback_history.push(feedback);
    }

    pub fn clear_feedback_history(&mut self) {
        self.all_feedback_history.clear();
    }
}

pub mod events {
    pub const DEMO_STARTUP: &str = "demo_startup";
    pub const DEMO_COMPLETED: &str = "demo_completed";
    pub const PLATEAU_DETECTED: &str = "plateau_detected";
    pub const INITIAL_PROPOSAL_GENERATED: &str = "initial_proposal_generated";
    pub const PROPOSAL_REFINED: &str = "proposal_refined";
    pub const RAW_FEEDBACK_GATHERED: &str = "raw_feedback_gathered";
    pub const FEEDBACK_DISTILLED: &str = "feedback_distilled";
    pub const ARBITER_ASSESSMENT: &str = "arbiter_assessment";
    pub const PROGRESS_EVALUATION: &str = "progress_evaluation";
    pub const GOAL_ACHIEVED: &str = "goal_achieved";
}

pub mod metadata_keys {
    pub const PROPOSAL_PREFIX: &str = "proposal_iteration_";
    pub const FEEDBACK_PREFIX: &str = "feedback_iteration_";
    pub const ASSESSMENT_PREFIX: &str = "assessment_iteration_";
    pub const BREAKOUT_SUMMARY: &str = "breakout_summary";
}

pub mod fields {
    pub const SUMMARY: &str = "summary";
    pub const CHANGES_MADE: &str = "changes_made";
    pub const GOAL_ACHIEVED: &str = "goal_achieved";
    pub const CONFIDENCE: &str = "confidence";
}

pub struct PlanningSession {
    pub task: Task,
    pub specialists: Vec<Agent>,
    pub arbiter: Agent,
    pub llm_manager: LLMManager,
    pub progress_tracker: ProgressTracker,
    pub state: PlanningSessionState,
}

impl PlanningSession {
    pub fn new(
        task: Task,
        specialists: Vec<Agent>,
        arbiter: Agent,
        llm_manager: LLMManager,
    ) -> Self {
        Self::with_config(
            task,
            specialists,
            arbiter,
            llm_manager,
            PlanningSessionConfig::default(),
        )
    }

    pub fn with_config(
        task: Task,
        specialists: Vec<Agent>,
        arbiter: Agent,
        llm_manager: LLMManager,
        config: PlanningSessionConfig,
    ) -> Self {
        let progress_config = ProgressConfig {
            fast_ema_alpha: config.fast_ema_alpha,
            slow_ema_alpha: config.slow_ema_alpha,
            momentum_threshold: config.momentum_threshold,
            plateau_threshold: config.plateau_threshold as u8,
            min_history_for_plateau: config.min_history_for_plateau,
            max_history_size: 50,
        };

        Self {
            task,
            specialists,
            arbiter,
            llm_manager,
            progress_tracker: ProgressTracker::with_config(progress_config),
            state: PlanningSessionState::with_config(config),
        }
    }

    pub async fn run(&mut self) -> Result<Value> {
        self.task.metadata.clear();
        tracing::info!(
            "Kicking off new planning session. Cleared task metadata for a fresh start."
        );

        while !self.state.has_reached_max_iterations() {
            self.state.increment_iteration();
            tracing::info!(
                iteration = self.state.iteration,
                "--- Starting Iteration ---"
            );

            if self.progress_tracker.needs_strategy_change() {
                self.execute_plateau_handling().await?;
            }

            let proposal = self.execute_proposal_phase().await?;
            let distilled_feedback = self.execute_feedback_phase(&proposal).await?;
            let assessment = self
                .execute_assessment_phase(&proposal, &distilled_feedback)
                .await?;

            let progress_score = self
                .execute_progress_evaluation(&proposal, &distilled_feedback)
                .await?;
            self.progress_tracker
                .update(self.state.iteration as u32, progress_score);

            self.log_progress_evaluation(progress_score);

            if self.is_goal_achieved(&assessment)? {
                return self.complete_session(assessment, proposal);
            }
        }

        self.fail_session()
    }

    pub async fn execute_plateau_handling(&mut self) -> Result<()> {
        self.log_event(
            events::PLATEAU_DETECTED,
            json!({
                "iteration": self.state.iteration,
                "momentum": self.progress_tracker.get_momentum(),
                "plateau_count": self.progress_tracker.get_plateau_count(),
                "strategy": "summarise_and_refocus"
            }),
        );

        let summary = apply_breakout_strategy(
            &self.llm_manager,
            &self.task,
            &self.state.current_proposal,
            &self.state.all_feedback_history,
            self.state.iteration as u32,
        )
        .await?;

        self.task.metadata.retain(|k, _| {
            !k.starts_with(metadata_keys::PROPOSAL_PREFIX)
                && !k.starts_with(metadata_keys::FEEDBACK_PREFIX)
                && !k.starts_with(metadata_keys::ASSESSMENT_PREFIX)
        });
        self.task
            .metadata
            .insert(metadata_keys::BREAKOUT_SUMMARY.to_string(), summary);

        let progress_config = ProgressConfig {
            fast_ema_alpha: self.state.config.fast_ema_alpha,
            slow_ema_alpha: self.state.config.slow_ema_alpha,
            momentum_threshold: self.state.config.momentum_threshold,
            plateau_threshold: self.state.config.plateau_threshold as u8,
            min_history_for_plateau: self.state.config.min_history_for_plateau,
            max_history_size: 50,
        };
        self.progress_tracker = ProgressTracker::with_config(progress_config);
        self.state.clear_feedback_history();

        tracing::info!("Applied 'Summarise & Refocus' breakout strategy. Context cleared and summary injected.");
        Ok(())
    }

    pub async fn execute_proposal_phase(&mut self) -> Result<Value> {
        let proposal = self.generate_or_refine_proposal().await?;
        self.state.set_current_proposal(proposal.clone());
        self.task.metadata.insert(
            format!("{}{}", metadata_keys::PROPOSAL_PREFIX, self.state.iteration),
            proposal.clone(),
        );
        Ok(proposal)
    }

    pub async fn execute_feedback_phase(&mut self, proposal: &Value) -> Result<Value> {
        let distilled_feedback = self.gather_and_distil_feedback(proposal).await?;
        self.state.add_feedback(distilled_feedback.clone());
        self.task.metadata.insert(
            format!("{}{}", metadata_keys::FEEDBACK_PREFIX, self.state.iteration),
            distilled_feedback.clone(),
        );
        Ok(distilled_feedback)
    }

    pub async fn execute_assessment_phase(
        &mut self,
        proposal: &Value,
        distilled_feedback: &Value,
    ) -> Result<Value> {
        let assessment = self
            .assess_proposal_quality(proposal, distilled_feedback)
            .await?;
        self.task.metadata.insert(
            format!(
                "{}{}",
                metadata_keys::ASSESSMENT_PREFIX,
                self.state.iteration
            ),
            assessment.clone(),
        );
        Ok(assessment)
    }

    pub async fn execute_progress_evaluation(
        &self,
        proposal: &Value,
        distilled_feedback: &Value,
    ) -> Result<u8> {
        self.evaluate_iteration_progress(proposal, distilled_feedback)
            .await
    }

    pub fn log_progress_evaluation(&self, progress_score: u8) {
        self.log_event(
            events::PROGRESS_EVALUATION,
            json!({
                "iteration": self.state.iteration,
                "score": progress_score,
                "momentum": self.progress_tracker.get_momentum(),
                "plateau_count": self.progress_tracker.get_plateau_count(),
            }),
        );
    }

    pub fn complete_session(&mut self, assessment: Value, proposal: Value) -> Result<Value> {
        self.task.status = TaskStatus::Completed;
        self.log_event(
            events::GOAL_ACHIEVED,
            json!({"iteration": self.state.iteration, "final_proposal": proposal}),
        );
        Ok(json!({
            "goal_achieved": true,
            "iterations": self.state.iteration,
            "final_assessment": assessment,
            "final_proposal": proposal,
        }))
    }

    pub fn fail_session(&mut self) -> Result<Value> {
        self.task.status = TaskStatus::Failed;
        Err(anyhow::anyhow!(
            "Maximum iterations ({}) reached without achieving the goal.",
            self.state.config.max_iterations
        ))
    }

    pub async fn generate_or_refine_proposal(&mut self) -> Result<Value> {
        if self.state.is_first_iteration()
            || self
                .task
                .metadata
                .contains_key(metadata_keys::BREAKOUT_SUMMARY)
        {
            let lead_specialist = &self.specialists[0];
            let initial_proposal =
                get_initial_proposal(&self.llm_manager, lead_specialist, &self.task).await?;
            self.log_event(
                events::INITIAL_PROPOSAL_GENERATED,
                json!({"iteration": self.state.iteration, "agent": lead_specialist.name}),
            );
            self.task.metadata.remove(metadata_keys::BREAKOUT_SUMMARY);
            Ok(initial_proposal)
        } else {
            let previous_feedback = self
                .task
                .metadata
                .get(&format!(
                    "{}{}",
                    metadata_keys::FEEDBACK_PREFIX,
                    self.state.iteration - 1
                ))
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing previous feedback for iteration {}",
                        self.state.iteration - 1
                    )
                })?;
            let current_proposal = self.state.current_proposal.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Missing current proposal for refinement at iteration {}",
                    self.state.iteration
                )
            })?;

            let refined_proposal = refine_proposal(
                &self.llm_manager,
                &self.specialists,
                &self.task,
                current_proposal,
                previous_feedback,
                self.state.iteration as u32,
            )
            .await?;

            self.validate_refinement_changes(current_proposal, &refined_proposal)?;

            self.log_event(
                events::PROPOSAL_REFINED,
                json!({"iteration": self.state.iteration, "changes": refined_proposal.get(fields::CHANGES_MADE)}),
            );
            Ok(refined_proposal)
        }
    }

    pub async fn gather_and_distil_feedback(&self, proposal: &Value) -> Result<Value> {
        let feedback_futures = self.specialists.iter().map(|specialist| {
            get_specialist_feedback(
                &self.llm_manager,
                specialist,
                &self.task,
                proposal,
                self.state.iteration as u32,
            )
        });

        let all_feedback: Vec<Value> = join_all(feedback_futures)
            .await
            .into_iter()
            .collect::<Result<_, _>>()?;

        self.log_event(
            events::RAW_FEEDBACK_GATHERED,
            json!({"iteration": self.state.iteration, "feedback_count": all_feedback.len()}),
        );

        let distilled_feedback =
            distil_feedback(&self.llm_manager, &self.task, proposal, &all_feedback).await?;

        self.log_event(
            events::FEEDBACK_DISTILLED,
            json!({"iteration": self.state.iteration, "summary": distilled_feedback.get(fields::SUMMARY)}),
        );
        Ok(distilled_feedback)
    }

    pub async fn assess_proposal_quality(
        &self,
        proposal: &Value,
        distilled_feedback: &Value,
    ) -> Result<Value> {
        let assessment = assess_proposal_quality(
            &self.llm_manager,
            &self.arbiter,
            &self.task,
            proposal,
            distilled_feedback,
            self.state.iteration as u32,
        )
        .await?;

        let validated_assessment = self.validate_arbiter_assessment(assessment);

        self.log_event(
            events::ARBITER_ASSESSMENT,
            json!({"iteration": self.state.iteration, "goal_achieved": validated_assessment.get(fields::GOAL_ACHIEVED), "confidence": validated_assessment.get(fields::CONFIDENCE)}),
        );
        Ok(validated_assessment)
    }

    pub async fn evaluate_iteration_progress(
        &self,
        current_proposal: &Value,
        current_feedback: &Value,
    ) -> Result<u8> {
        if self.state.is_first_iteration() {
            return Ok(5);
        }

        let prev_proposal = self.task.metadata.get(&format!(
            "{}{}",
            metadata_keys::PROPOSAL_PREFIX,
            self.state.iteration - 1
        ));
        let prev_feedback = self.task.metadata.get(&format!(
            "{}{}",
            metadata_keys::FEEDBACK_PREFIX,
            self.state.iteration - 1
        ));

        evaluate_progress_score(
            &self.llm_manager,
            prev_proposal,
            prev_feedback,
            current_proposal,
            current_feedback,
            self.state.iteration as u32,
        )
        .await
    }

    pub fn is_goal_achieved(&self, assessment: &Value) -> Result<bool> {
        let goal_achieved = assessment
            .get(fields::GOAL_ACHIEVED)
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let confidence = assessment
            .get(fields::CONFIDENCE)
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        Ok(goal_achieved && confidence >= self.state.config.min_confidence_for_completion)
    }

    pub fn log_event(&self, event_type: &str, data: Value) {
        use chrono::Utc;
        println!(
            "{}",
            json!({ "timestamp": Utc::now().to_rfc3339(), "event": event_type, "data": data })
        );
    }

    fn validate_refinement_changes(
        &self,
        current_proposal: &Value,
        refined_proposal: &Value,
    ) -> Result<()> {
        if current_proposal == refined_proposal {
            return Err(anyhow::anyhow!("Refinement did not produce any changes"));
        }
        Ok(())
    }

    fn validate_arbiter_assessment(&self, mut assessment: Value) -> Value {
        if assessment.get(fields::GOAL_ACHIEVED).is_none() {
            assessment[fields::GOAL_ACHIEVED] = json!(false);
        }
        if assessment.get(fields::CONFIDENCE).is_none() {
            assessment[fields::CONFIDENCE] = json!(0.0);
        }
        assessment
    }
}
