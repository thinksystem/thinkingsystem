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

use super::config::{ModelConfig, NLUConfig, ProcessingPolicy};
use super::error::OrchestratorError;
use tokio::time::Duration;
#[derive(Debug, Clone)]
pub struct ProcessingPlan {
    pub strategy_name: String,
    pub tasks: Vec<PlannedTask>,
    pub execution_order: Vec<Vec<String>>,
}
#[derive(Debug, Clone)]
pub struct PlannedTask {
    pub id: String,
    pub task_type: String,
    pub model_name: String,
    pub prompt_template: String,
    pub dependencies: Vec<String>,
    pub timeout: Duration,
    pub input_data: Option<String>,
    pub temperature: f32,
    pub max_tokens: usize,
}
pub fn create_plan(
    policy: &ProcessingPolicy,
    config: &NLUConfig,
    input: &str,
) -> Result<ProcessingPlan, OrchestratorError> {
    let mut tasks = Vec::new();
    let mut task_counter = 0;
    match policy.strategy.strategy_type.as_str() {
        "bundled" => {
            for action in &policy.strategy.actions {
                if !action.bundle.is_empty() {
                    let task_id = format!("bundled_{task_counter}");
                    let model = select_model_for_capability(&action.model_capability, config)?;
                    let bundled_prompt = create_bundled_prompt(&action.bundle, input, config)?;
                    tasks.push(PlannedTask {
                        id: task_id,
                        task_type: "bundled".to_string(),
                        model_name: model.name.clone(),
                        prompt_template: bundled_prompt,
                        dependencies: vec![],
                        timeout: Duration::from_secs(action.timeout),
                        input_data: Some(input.to_string()),
                        temperature: model.temperature,
                        max_tokens: model.max_tokens,
                    });
                    task_counter += 1;
                }
            }
        }
        "parallel" => {
            for action in &policy.strategy.actions {
                let task_id = format!("{}_{}", action.task, task_counter);
                let model = select_model_for_capability(&action.model_capability, config)?;
                let prompt = get_prompt_for_task(&action.task, config)?;
                tasks.push(PlannedTask {
                    id: task_id,
                    task_type: action.task.clone(),
                    model_name: model.name.clone(),
                    prompt_template: prompt,
                    dependencies: action.depends_on.clone(),
                    timeout: Duration::from_secs(action.timeout),
                    input_data: Some(input.to_string()),
                    temperature: model.temperature,
                    max_tokens: model.max_tokens,
                });
                task_counter += 1;
            }
        }
        "staged" => {
            for stage in &policy.strategy.stages {
                for action in &stage.actions {
                    let task_id = format!("{}_{}_{}", stage.name, action.task, task_counter);
                    let model = select_model_for_capability(&action.model_capability, config)?;
                    let prompt = get_prompt_for_task(&action.task, config)?;
                    let mut dependencies = action.depends_on.clone();
                    dependencies.extend(stage.depends_on.clone());
                    tasks.push(PlannedTask {
                        id: task_id,
                        task_type: action.task.clone(),
                        model_name: model.name.clone(),
                        prompt_template: prompt,
                        dependencies,
                        timeout: Duration::from_secs(action.timeout),
                        input_data: Some(input.to_string()),
                        temperature: model.temperature,
                        max_tokens: model.max_tokens,
                    });
                    task_counter += 1;
                }
            }
        }
        "question_optimised" => {
            for action in &policy.strategy.actions {
                let task_id = format!("q_{}_{}", action.task, task_counter);
                let model = select_model_for_capability(&action.model_capability, config)?;
                let prompt = get_prompt_for_task(&action.task, config)?;
                tasks.push(PlannedTask {
                    id: task_id,
                    task_type: action.task.clone(),
                    model_name: model.name.clone(),
                    prompt_template: prompt,
                    dependencies: action.depends_on.clone(),
                    timeout: Duration::from_secs(action.timeout),
                    input_data: Some(input.to_string()),
                    temperature: model.temperature,
                    max_tokens: model.max_tokens,
                });
                task_counter += 1;
            }
        }
        _ => {
            return Err(OrchestratorError::new(format!(
                "Unknown strategy type: {}",
                policy.strategy.strategy_type
            )))
        }
    }
    let execution_order = calculate_execution_order(&tasks)?;
    Ok(ProcessingPlan {
        strategy_name: policy.name.clone(),
        tasks,
        execution_order,
    })
}
fn select_model_for_capability<'a>(
    capability: &str,
    config: &'a NLUConfig,
) -> Result<&'a ModelConfig, OrchestratorError> {
    let candidates: Vec<&ModelConfig> = config
        .models
        .iter()
        .filter(|model| model.capabilities.contains(&capability.to_string()))
        .collect();
    if candidates.is_empty() {
        return Err(OrchestratorError::new(format!(
            "No model found with capability: {capability}"
        )));
    }
    if config
        .selection_strategy
        .get("prefer_speed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        candidates
            .iter()
            .min_by_key(|model| match model.speed_tier.as_str() {
                "fast" => 1,
                "medium" => 2,
                "slow" => 3,
                _ => 4,
            })
            .copied()
            .ok_or_else(|| OrchestratorError::new("No suitable model found".to_string()))
    } else {
        Ok(candidates[0])
    }
}
fn create_bundled_prompt(
    tasks: &[String],
    input: &str,
    config: &NLUConfig,
) -> Result<String, OrchestratorError> {
    let mut bundled_prompt = String::new();
    bundled_prompt.push_str("You are a multi-task NLU system. Perform the following tasks on the input and return a single JSON response:\n\n");
    for task in tasks {
        if let Some(template) = config.prompts.get(task) {
            bundled_prompt.push_str(&format!("Task {}: {}\n", task, template.system_message));
        }
    }
    bundled_prompt.push_str(&format!("\nInput: \"{input}\"\n\n"));
    bundled_prompt.push_str("Return a single JSON object with results for all tasks:\n");
    bundled_prompt.push_str("{\n");
    for (i, task) in tasks.iter().enumerate() {
        bundled_prompt.push_str(&format!("  \"{task}\": {{  }}"));
        if i < tasks.len() - 1 {
            bundled_prompt.push_str(",\n");
        } else {
            bundled_prompt.push('\n');
        }
    }
    bundled_prompt.push('}');
    Ok(bundled_prompt)
}
fn get_prompt_for_task(task: &str, config: &NLUConfig) -> Result<String, OrchestratorError> {
    config
        .prompts
        .get(task)
        .map(|template| format!("{}\n\n{}", template.system_message, template.user_template))
        .ok_or_else(|| OrchestratorError::new(format!("No prompt template found for task: {task}")))
}
fn calculate_execution_order(tasks: &[PlannedTask]) -> Result<Vec<Vec<String>>, OrchestratorError> {
    let mut order = Vec::new();
    let mut remaining_tasks: Vec<&PlannedTask> = tasks.iter().collect();
    let mut completed_tasks = std::collections::HashSet::new();
    while !remaining_tasks.is_empty() {
        let mut current_batch = Vec::new();
        let mut tasks_to_remove = Vec::new();
        for (index, task) in remaining_tasks.iter().enumerate() {
            let dependencies_met = task
                .dependencies
                .iter()
                .all(|dep| completed_tasks.contains(dep));
            if dependencies_met {
                current_batch.push(task.id.clone());
                tasks_to_remove.push(index);
            }
        }
        if current_batch.is_empty() {
            return Err(OrchestratorError::new(
                "Circular dependency detected in task plan".to_string(),
            ));
        }
        for &index in tasks_to_remove.iter().rev() {
            let task = remaining_tasks.remove(index);
            completed_tasks.insert(task.id.clone());
        }
        order.push(current_batch);
    }
    Ok(order)
}
