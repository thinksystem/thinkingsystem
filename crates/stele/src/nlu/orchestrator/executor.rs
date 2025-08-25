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

use super::{OrchestratorError, ProcessingPlan, TaskOutput};
use crate::nlu::llm_processor::LLMAdapter;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use steel::messaging::insight::ner_analysis::{DetectedEntity, NerAnalyser, NerConfig};
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};
pub async fn execute(
    plan: &ProcessingPlan,
    llm_adapters: &HashMap<String, Arc<dyn LLMAdapter + Send + Sync>>,
    input_text: &str,
) -> Result<Vec<TaskOutput>, OrchestratorError> {
    if plan.tasks.is_empty() {
        warn!("Execution plan contains no tasks");
        return Ok(Vec::new());
    }
    info!(
        "Executing plan with {} tasks using {} strategy",
        plan.tasks.len(),
        plan.strategy_name
    );
    let strategy_name = plan.strategy_name.to_lowercase();
    
    let ner_hints = compute_ner_hints(input_text);
    if strategy_name.contains("bundled") {
        execute_bundled_strategy(plan, llm_adapters, input_text, &ner_hints).await
    } else if strategy_name.contains("parallel") {
        execute_parallel_strategy(plan, llm_adapters, input_text, &ner_hints).await
    } else if strategy_name.contains("staged") {
        execute_staged_strategy(plan, llm_adapters, input_text, &ner_hints).await
    } else if strategy_name == "sequential" || strategy_name.contains("sequential") {
        execute_sequential_strategy(plan, llm_adapters, input_text, &ner_hints).await
    } else {
        info!(
            "Using default batched execution for strategy: {}",
            plan.strategy_name
        );
        execute_batched(plan, llm_adapters, input_text, &ner_hints).await
    }
}
async fn execute_sequential_strategy(
    plan: &ProcessingPlan,
    llm_adapters: &HashMap<String, Arc<dyn LLMAdapter + Send + Sync>>,
    input_text: &str,
    ner_hints: &str,
) -> Result<Vec<TaskOutput>, OrchestratorError> {
    let mut results = Vec::new();
    for task in &plan.tasks {
        debug!(
            "Executing sequential task: {} (type: {})",
            task.id, task.task_type
        );
        let adapter = match find_adapter_for_model(&task.model_name, llm_adapters) {
            Some(adapter) => adapter,
            None => {
                warn!("No adapter found for model: {}", task.model_name);
                continue;
            }
        };
        let actual_input = match &task.input_data {
            Some(task_input) => task_input.as_str(),
            None => input_text,
        };
        let result = execute_single_task(task, adapter, actual_input, ner_hints).await;
        results.push(result);
    }
    info!(
        "Sequential execution completed with {} results",
        results.len()
    );
    Ok(results)
}
async fn execute_bundled_strategy(
    plan: &ProcessingPlan,
    llm_adapters: &HashMap<String, Arc<dyn LLMAdapter + Send + Sync>>,
    input_text: &str,
    ner_hints: &str,
) -> Result<Vec<TaskOutput>, OrchestratorError> {
    execute_batched(plan, llm_adapters, input_text, ner_hints).await
}
async fn execute_parallel_strategy(
    plan: &ProcessingPlan,
    llm_adapters: &HashMap<String, Arc<dyn LLMAdapter + Send + Sync>>,
    input_text: &str,
    ner_hints: &str,
) -> Result<Vec<TaskOutput>, OrchestratorError> {
    execute_batched(plan, llm_adapters, input_text, ner_hints).await
}
async fn execute_staged_strategy(
    plan: &ProcessingPlan,
    llm_adapters: &HashMap<String, Arc<dyn LLMAdapter + Send + Sync>>,
    input_text: &str,
    ner_hints: &str,
) -> Result<Vec<TaskOutput>, OrchestratorError> {
    validate_dependencies(plan)?;

    let execution_stages = create_execution_order(&plan.tasks)?;

    let mut results = Vec::new();
    let input_text_clone = input_text.to_string();

    info!(
        "Executing staged strategy with {} stages",
        execution_stages.len()
    );

    for (stage_idx, stage) in execution_stages.iter().enumerate() {
        debug!(
            "Executing stage {} with {} tasks",
            stage_idx + 1,
            stage.len()
        );
        let mut stage_futures = Vec::new();

        for task_id in stage {
            if let Some(task) = plan.tasks.iter().find(|t| &t.id == task_id) {
                debug!("Preparing task: {} (type: {})", task.id, task.task_type);
                let adapter = match find_adapter_for_model(&task.model_name, llm_adapters) {
                    Some(adapter) => adapter,
                    None => {
                        warn!("No adapter found for model: {}", task.model_name);
                        continue;
                    }
                };
                let actual_input = task.input_data.as_ref().unwrap_or(&input_text_clone);
                let future = execute_single_task(task, adapter, actual_input, ner_hints);
                stage_futures.push(future);
            }
        }

        let stage_results = futures::future::join_all(stage_futures).await;
        results.extend(stage_results);
    }

    info!(
        "Executed {} tasks across {} stages successfully",
        results.len(),
        execution_stages.len()
    );
    Ok(results)
}
async fn execute_batched(
    plan: &ProcessingPlan,
    llm_adapters: &HashMap<String, Arc<dyn LLMAdapter + Send + Sync>>,
    input_text: &str,
    ner_hints: &str,
) -> Result<Vec<TaskOutput>, OrchestratorError> {
    let mut results = Vec::new();
    let input_text_clone = input_text.to_string();
    for batch in &plan.execution_order {
        let mut batch_futures = Vec::new();
        for task_id in batch {
            if let Some(task) = plan.tasks.iter().find(|t| &t.id == task_id) {
                debug!("Preparing task: {} (type: {})", task.id, task.task_type);
                let adapter = match find_adapter_for_model(&task.model_name, llm_adapters) {
                    Some(adapter) => adapter,
                    None => {
                        warn!("No adapter found for model: {}", task.model_name);
                        continue;
                    }
                };
                let actual_input = task.input_data.as_ref().unwrap_or(&input_text_clone);
                let future = execute_single_task(task, adapter, actual_input, ner_hints);
                batch_futures.push(future);
            }
        }
        let batch_results = futures::future::join_all(batch_futures).await;
        results.extend(batch_results);
    }
    info!("Executed {} tasks successfully", results.len());
    Ok(results)
}
async fn execute_single_task(
    task: &super::planner::PlannedTask,
    adapter: &Arc<dyn LLMAdapter + Send + Sync>,
    input_text: &str,
    ner_hints: &str,
) -> TaskOutput {
    let start_time = std::time::Instant::now();
    debug!(
        "Executing task: {} with model: {}",
        task.id, task.model_name
    );
    let prompt = compose_prompt(&task.prompt_template, input_text, ner_hints);
    let execution_result =
        timeout(task.timeout, async { adapter.process_text(&prompt).await }).await;
    let execution_time = start_time.elapsed();
    match execution_result {
        Ok(Ok(response)) => {
            debug!(
                "Task {} completed successfully in {:?}",
                task.id, execution_time
            );
            let task_type = if task.task_type == "bundled" {
                "bundled"
            } else {
                task.task_type.split('_').next().unwrap_or(&task.task_type)
            };
            match parse_task_response(task_type, &response) {
                Ok(parsed_data) => TaskOutput {
                    task_name: task.id.clone(),
                    data: parsed_data,
                    model_used: task.model_name.clone(),
                    execution_time: Duration::from_millis(execution_time.as_millis() as u64),
                    success: true,
                    error: None,
                },
                Err(e) => {
                    error!("Failed to parse response for task {}: {}", task.id, e);
                    TaskOutput {
                        task_name: task.id.clone(),
                        data: serde_json::json!({"error": "Response parsing failed", "raw_response": response}),
                        model_used: task.model_name.clone(),
                        execution_time: Duration::from_millis(execution_time.as_millis() as u64),
                        success: false,
                        error: Some(format!("Response parsing failed: {e}")),
                    }
                }
            }
        }
        Ok(Err(e)) => {
            error!("Task {} failed: {}", task.id, e);
            TaskOutput {
                task_name: task.id.clone(),
                data: serde_json::json!({"error": e.to_string()}),
                model_used: task.model_name.clone(),
                execution_time: Duration::from_millis(execution_time.as_millis() as u64),
                success: false,
                error: Some(e.to_string()),
            }
        }
        Err(_) => {
            error!("Task {} timed out after {:?}", task.id, task.timeout);
            TaskOutput {
                task_name: task.id.clone(),
                data: serde_json::json!({"error": "Task timed out"}),
                model_used: task.model_name.clone(),
                execution_time: Duration::from_millis(execution_time.as_millis() as u64),
                success: false,
                error: Some("Task execution timed out".to_string()),
            }
        }
    }
}

fn compose_prompt(template: &str, input: &str, ner_hints: &str) -> String {
    let mut prompt = template.replace("{input}", input);
    let now = Utc::now().to_rfc3339();
    prompt = prompt.replace("{current_time}", &now);
    prompt = prompt.replace("{ner_hints}", ner_hints);
    prompt
}

fn compute_ner_hints(input_text: &str) -> String {
    
    let enabled = std::env::var("STELE_ENABLE_NER_HINTS")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !enabled {
        return "[]".to_string();
    }
    let mut analyser = NerAnalyser::new(NerConfig::default());
    match analyser.analyse_text(input_text) {
        Ok(result) => {
            let filtered: Vec<&DetectedEntity> = result
                .entities
                .iter()
                .filter(|e| {
                    matches!(
                        e.label.to_lowercase().as_str(),
                        "person" | "location" | "date"
                    )
                })
                .collect();
            serde_json::to_string(&filtered).unwrap_or_else(|_| "[]".to_string())
        }
        Err(_) => "[]".to_string(),
    }
}
fn find_adapter_for_model<'a>(
    model_name: &str,
    adapters: &'a HashMap<String, Arc<dyn LLMAdapter + Send + Sync>>,
) -> Option<&'a Arc<dyn LLMAdapter + Send + Sync>> {
    if let Some(adapter) = adapters.get(model_name) {
        return Some(adapter);
    }
    for (adapter_name, adapter) in adapters {
        if model_name.contains("claude") && adapter_name.contains("claude") {
            return Some(adapter);
        }
        if model_name.contains("gpt") && adapter_name.contains("gpt") {
            return Some(adapter);
        }
        if model_name.contains("haiku") && adapter_name.contains("haiku") {
            return Some(adapter);
        }
        if model_name.contains("sonnet") && adapter_name.contains("sonnet") {
            return Some(adapter);
        }
        if model_name.contains("opus") && adapter_name.contains("opus") {
            return Some(adapter);
        }
    }
    adapters.values().next()
}
fn parse_task_response(
    task_type: &str,
    response: &str,
) -> Result<serde_json::Value, OrchestratorError> {
    println!(
        "DEBUG: Parsing response for task type '{}'. Response length: {}",
        task_type,
        response.len()
    );
    if task_type == "bundled" || task_type == "bundled_extraction" {
        println!("DEBUG: Raw bundled extraction response: {response}");
    }
    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(response) {
        println!(
            "DEBUG: Direct JSON parse success for task_type '{}' (top-level type: {})",
            task_type,
            match &json_value {
                serde_json::Value::Object(_) => "object",
                serde_json::Value::Array(_) => "array",
                serde_json::Value::String(_) => "string",
                serde_json::Value::Number(_) => "number",
                serde_json::Value::Bool(_) => "bool",
                serde_json::Value::Null => "null",
            }
        );
        if task_type == "bundled" || task_type == "bundled_extraction" {
            println!(
                "DEBUG: Bundled extraction parsed JSON: {}",
                serde_json::to_string_pretty(&json_value).unwrap_or_default()
            );
        }
        return Ok(json_value);
    }

    println!("DEBUG: Failed to parse as JSON, task_type: {task_type}");
    debug!(
        "Response is not valid JSON, attempting structured parsing for task type '{}'",
        task_type
    );

    if let Some(json_content) = extract_json_from_markdown(response) {
        println!(
            "DEBUG: Extracted JSON content length: {}",
            json_content.len()
        );
        match serde_json::from_str::<serde_json::Value>(&json_content) {
            Ok(parsed_json) => {
                info!("Successfully extracted and parsed JSON from response text");
                return Ok(parsed_json);
            }
            Err(e) => {
                println!(
                    "DEBUG: Extracted content failed JSON parsing (initial): {}",
                    json_content.chars().take(500).collect::<String>()
                );
                println!("DEBUG: JSON parsing error: {e}");

                // Attempt salvage if the JSON looks truncated
                if looks_truncated(&json_content) {
                    if let Some(salvaged) = salvage_truncated_json(&json_content) {
                        println!(
                            "DEBUG: Salvaged truncated JSON (len {} -> {})",
                            json_content.len(),
                            salvaged.len()
                        );
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&salvaged) {
                            println!(
                                "DEBUG: Salvage JSON parse success for task_type '{}' (salvaged length {}, top-level type: {})",
                                task_type,
                                salvaged.len(),
                                match &parsed {
                                    serde_json::Value::Object(_) => "object",
                                    serde_json::Value::Array(_) => "array",
                                    serde_json::Value::String(_) => "string",
                                    serde_json::Value::Number(_) => "number",
                                    serde_json::Value::Bool(_) => "bool",
                                    serde_json::Value::Null => "null",
                                }
                            );
                            info!("Successfully parsed salvaged truncated JSON");
                            return Ok(parsed);
                        } else {
                            println!("DEBUG: Salvage attempt still not valid JSON");
                        }
                    } else {
                        println!("DEBUG: No salvageable truncated JSON structure identified");
                    }
                }
            }
        }
    }
    match task_type {
        "bundled_extraction" | "bundled" => {
            warn!("Could not parse bundled extraction response, creating fallback structure. Response preview: {}",
                  response.chars().take(200).collect::<String>());
            Ok(serde_json::json!({
                "segments": [{
                    "text": response.chars().take(100).collect::<String>(),
                    "segment_type": {"Statement": {"intent": "unknown"}},
                    "priority": 50,
                    "dependencies": [],
                    "metadata": {"fallback": serde_json::Value::String("true".to_string())},
                    "tokens": []
                }],
                "extracted_data": {
                    "nodes": [],
                    "relationships": []
                },
                "processing_metadata": {
                    "strategy_used": "bundled_fallback",
                    "models_used": [],
                    "execution_time_ms": 0,
                    "total_cost_estimate": 0.0,
                    "confidence_scores": {},
                    "topics": [],
                    "sentiment_score": 0.0
                }
            }))
        }
        "segmentation" => {
            let segments = if response.contains("```json") {
                extract_json_from_markdown(response)
                    .and_then(|json| serde_json::from_str(&json).ok())
                    .unwrap_or_else(|| {
                        serde_json::json!([{
                            "text": response.trim(),
                            "segment_type": {"Statement": {"intent": "general"}},
                            "priority": 50,
                            "dependencies": [],
                            "metadata": {},
                            "tokens": []
                        }])
                    })
            } else {
                serde_json::json!([{
                    "text": response.trim(),
                    "segment_type": {"Statement": {"intent": "general"}},
                    "priority": 50,
                    "dependencies": [],
                    "metadata": {},
                    "tokens": []
                }])
            };
            Ok(serde_json::json!({"segments": segments}))
        }
        "entity" => {
            let entities = if response.contains("```json") {
                extract_json_from_markdown(response)
                    .and_then(|json| serde_json::from_str(&json).ok())
                    .unwrap_or_else(|| serde_json::json!([]))
            } else {
                let words: Vec<serde_json::Value> = response
                    .split_whitespace()
                    .filter(|word| word.chars().any(|c| c.is_uppercase()))
                    .map(|word| {
                        serde_json::json!({
                            "name": word,
                            "entity_type": "UNKNOWN",
                            "aliases": [],
                            "metadata": {}
                        })
                    })
                    .collect();
                serde_json::Value::Array(words)
            };
            Ok(serde_json::json!({"entities": entities}))
        }
        "temporal" => {
            let temporal_markers = if response.contains("```json") {
                extract_json_from_markdown(response)
                    .and_then(|json| serde_json::from_str(&json).ok())
                    .unwrap_or_else(|| serde_json::json!([]))
            } else {
                serde_json::json!([])
            };
            Ok(serde_json::json!({"temporal_markers": temporal_markers}))
        }
        "numerical" => {
            let numerical_values = if response.contains("```json") {
                extract_json_from_markdown(response)
                    .and_then(|json| serde_json::from_str(&json).ok())
                    .unwrap_or_else(|| serde_json::json!([]))
            } else {
                let numbers: Vec<serde_json::Value> = response
                    .split_whitespace()
                    .filter_map(|word| {
                        if let Ok(num) = word.parse::<f64>() {
                            Some(serde_json::json!({
                                "value": num,
                                "unit": "",
                                "category": "number",
                                "context": response
                            }))
                        } else {
                            None
                        }
                    })
                    .collect();
                serde_json::Value::Array(numbers)
            };
            Ok(serde_json::json!({"numerical_values": numerical_values}))
        }
        "relationship" => {
            let relationships = if response.contains("```json") {
                extract_json_from_markdown(response)
                    .and_then(|json| serde_json::from_str(&json).ok())
                    .unwrap_or_else(|| serde_json::json!([]))
            } else {
                serde_json::json!([])
            };
            Ok(serde_json::json!({"relationships": relationships}))
        }
        "intent" => {
            let intent_data = if response.contains("```json") {
                extract_json_from_markdown(response)
                    .and_then(|json| serde_json::from_str(&json).ok())
                    .unwrap_or_else(|| {
                        serde_json::json!({
                            "intent": "general",
                            "confidence": 0.5
                        })
                    })
            } else {
                serde_json::json!({
                    "intent": response.trim(),
                    "confidence": 0.7
                })
            };
            Ok(intent_data)
        }
        "topic" => {
            let topics = if response.contains("```json") {
                extract_json_from_markdown(response)
                    .and_then(|json| serde_json::from_str(&json).ok())
                    .unwrap_or_else(|| serde_json::json!([]))
            } else {
                let topic_words: Vec<serde_json::Value> = response
                    .split_whitespace()
                    .filter(|word| word.len() > 3 && word.chars().all(|c| c.is_alphabetic()))
                    .take(5)
                    .map(|word| serde_json::json!(word.to_lowercase()))
                    .collect();
                serde_json::Value::Array(topic_words)
            };
            Ok(serde_json::json!({"topics": topics}))
        }
        "sentiment" => {
            let sentiment_data = if response.contains("```json") {
                extract_json_from_markdown(response)
                    .and_then(|json| serde_json::from_str(&json).ok())
                    .unwrap_or_else(|| {
                        serde_json::json!({
                            "sentiment_score": 0.0
                        })
                    })
            } else {
                let score = if response.to_lowercase().contains("positive") {
                    0.5
                } else if response.to_lowercase().contains("negative") {
                    -0.5
                } else {
                    0.0
                };
                serde_json::json!({
                    "sentiment_score": score
                })
            };
            Ok(sentiment_data)
        }
        _ => {
            warn!(
                "Unknown task type '{}', using generic response format",
                task_type
            );
            Ok(serde_json::json!({
                "result": response.trim(),
                "task_type": task_type
            }))
        }
    }
}
fn extract_json_from_markdown(text: &str) -> Option<String> {
    if let Some(start) = text.find("```json") {
        let content_start = start + 7;
        if let Some(end) = text[content_start..].find("```") {
            let json_content = &text[content_start..content_start + end];
            return Some(json_content.trim().to_string());
        }
    }

    if let Some(start) = text.find("```") {
        let content_start = start + 3;
        if let Some(end) = text[content_start..].find("```") {
            let content = &text[content_start..content_start + end];
            if serde_json::from_str::<serde_json::Value>(content.trim()).is_ok() {
                return Some(content.trim().to_string());
            }
        }
    }

    let trimmed = text.trim();
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
            return Some(trimmed.to_string());
        }

        let fixed_json = fix_common_json_errors(trimmed);
        if serde_json::from_str::<serde_json::Value>(&fixed_json).is_ok() {
            return Some(fixed_json);
        }
    }

    None
}

fn fix_common_json_errors(json: &str) -> String {
    let mut fixed = json.replace(
        "}}, \"processing_metadata\":",
        "}, \"processing_metadata\":",
    );

    fixed = fixed.replace("},}", "}");
    fixed = fixed.replace("],]", "]");

    fixed = fixed.replace(
        "\"extracted_data\": {\n    \"nodes\":",
        "\"extracted_data\": {\n      \"nodes\":",
    );
    fixed = fixed.replace("    ]\n  }},", "    ]\n  },");

    if let Some(pos) = fixed.find("\"extracted_data\":") {
        if let Some(end_pos) = fixed[pos..].find("}},") {
            let actual_end = pos + end_pos;

            if fixed[actual_end + 3..]
                .trim_start()
                .starts_with("\"processing_metadata\":")
            {
                let before = &fixed[..actual_end + 1];
                let after = &fixed[actual_end + 3..];
                fixed = format!("{before},{after}");
            }
        }
    }

    fixed
}

// Heuristic: detect if JSON likely truncated mid-string/object/array by imbalance of braces/brackets or dangling quotes.
fn looks_truncated(s: &str) -> bool {
    let mut brace = 0i32;
    let mut bracket = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for c in s.chars() {
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                // escape next
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        } else if c == '"' {
            in_string = true;
            continue;
        }
        match c {
            '{' => brace += 1,
            '}' => brace -= 1,
            '[' => bracket += 1,
            ']' => bracket -= 1,
            _ => {}
        }
    }
    // If we're still inside a string or unmatched containers remain positive, and the text does not properly end with } or ]
    in_string || brace > 0 || bracket > 0 || !s.trim_end().ends_with(['}', ']'])
}


fn salvage_truncated_json(s: &str) -> Option<String> {
    let mut result = String::with_capacity(s.len() + 16);
    result.push_str(s);

    
    let mut in_string = false;
    let mut escape = false;
    for c in s.chars() {
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
        }
    }
    if in_string {
        result.push('"');
    }

    
    let mut brace = 0i32;
    let mut bracket = 0i32;
    let mut in_string2 = false;
    let mut escape2 = false;
    for c in result.chars() {
        if in_string2 {
            if escape2 {
                escape2 = false;
            } else if c == '\\' {
                escape2 = true;
            } else if c == '"' {
                in_string2 = false;
            }
            continue;
        } else if c == '"' {
            in_string2 = true;
            continue;
        }
        match c {
            '{' => brace += 1,
            '}' => brace -= 1,
            '[' => bracket += 1,
            ']' => bracket -= 1,
            _ => {}
        }
    }

    while brace > 0 {
        result.push('}');
        brace -= 1;
    }
    while bracket > 0 {
        result.push(']');
        bracket -= 1;
    }

    
    if serde_json::from_str::<serde_json::Value>(&result).is_ok() {
        Some(result)
    } else {
        None
    }
}
fn validate_dependencies(plan: &ProcessingPlan) -> Result<(), OrchestratorError> {
    let task_ids: std::collections::HashSet<String> =
        plan.tasks.iter().map(|t| t.id.clone()).collect();
    for task in &plan.tasks {
        for dep in &task.dependencies {
            if !task_ids.contains(dep) {
                return Err(OrchestratorError::new(format!(
                    "Task '{}' depends on non-existent task '{}'",
                    task.id, dep
                )));
            }
        }
    }
    Ok(())
}
fn dependencies_satisfied(
    task: &super::planner::PlannedTask,
    completed_tasks: &std::collections::HashSet<String>,
) -> bool {
    task.dependencies
        .iter()
        .all(|dep| completed_tasks.contains(dep))
}
fn create_execution_order(
    tasks: &[super::planner::PlannedTask],
) -> Result<Vec<Vec<String>>, OrchestratorError> {
    let mut execution_order = Vec::new();
    let mut completed = std::collections::HashSet::new();
    let mut remaining: Vec<_> = tasks.iter().collect();
    let mut iterations = 0;
    let max_iterations = tasks.len() * 2;
    while !remaining.is_empty() {
        iterations += 1;
        if iterations > max_iterations {
            return Err(OrchestratorError::new(
                "Failed to resolve task dependencies - possible circular dependency".to_string(),
            ));
        }
        let mut current_batch = Vec::new();
        let mut newly_completed = Vec::new();
        for (i, task) in remaining.iter().enumerate() {
            if dependencies_satisfied(task, &completed) {
                current_batch.push(task.id.clone());
                newly_completed.push(i);
            }
        }
        if current_batch.is_empty() {
            return Err(OrchestratorError::new(
                "Cannot progress - no tasks have satisfied dependencies".to_string(),
            ));
        }
        for &index in newly_completed.iter().rev() {
            let task = remaining.remove(index);
            completed.insert(task.id.clone());
        }
        execution_order.push(current_batch);
    }
    Ok(execution_order)
}
