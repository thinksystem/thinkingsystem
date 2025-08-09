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

pub mod analysis;
pub mod flow_loader;
pub mod schemas;
pub mod task_system;
pub use analysis::{
    CompositeAnalyser, ConfigurableTaskAnalyser, NerReadyAnalyser, SimpleTaskAnalyser,
    TaskAnalyser, TaskAnalysis,
};
pub use flow_loader::{load_flow_from_json, FlowLoader, FlowLoaderError};
pub use schemas::{
    AgentContribution, FinalDeliverable, OutputFormat, ResourceRequirement, Task, TaskConfig,
    TaskExecution, TaskOutput, TaskOutputSchema, TaskPriority, TaskProposal, TaskStatus,
};
pub use task_system::{
    create_task_from_input, create_task_from_input_simple, AgentExecution, AgentProposal,
    CompetitionManager, TaskCompletionResult, TaskManager, TaskSystem, TaskSystemConfig,
};
use thiserror::Error;
#[derive(Error, Debug, Clone)]
pub enum TaskError {
    #[error("Task not found: {0}")]
    TaskNotFound(String),
    #[error("Invalid task configuration: {0}")]
    InvalidConfiguration(String),
    #[error("Task execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Agent proposal error: {0}")]
    ProposalError(String),
    #[error("Resource allocation error: {0}")]
    ResourceError(String),
    #[error("Consensus not reached: {0}")]
    ConsensusError(String),
    #[error("Flow loading error: {0}")]
    FlowLoaderError(String),
    #[error("LLM integration error: {0}")]
    LlmError(String),
    #[error("Task analysis failed: {0}")]
    AnalysisFailed(String),
}
pub type TaskResult<T> = Result<T, TaskError>;
