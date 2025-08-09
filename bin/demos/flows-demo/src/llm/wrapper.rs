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

use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;
use stele::llm::unified_adapter::UnifiedLLMAdapter;
use stele::nlu::llm_processor::LLMAdapter;
use tracing::{debug, error, info, warn};

pub struct DemoLLMWrapper {
    llm_adapter: Arc<UnifiedLLMAdapter>,
}

impl DemoLLMWrapper {
    pub async fn new() -> Result<Self> {
        let llm_adapter = Arc::new(
            UnifiedLLMAdapter::with_preferences("ollama", "llama3.2")
                .await
                .map_err(|e| anyhow::anyhow!("Failed to initialise UnifiedLLMAdapter: {}", e))?,
        );
        Ok(Self { llm_adapter })
    }

    pub async fn with_preferences(provider: &str, model: &str) -> Result<Self> {
        let llm_adapter = Arc::new(
            UnifiedLLMAdapter::with_preferences(provider, model)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to initialise UnifiedLLMAdapter: {}", e))?,
        );
        Ok(Self { llm_adapter })
    }

    pub async fn generate_response(
        &self,
        prompt: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        debug!(
            "Generating LLM response for prompt length: {}",
            prompt.len()
        );

        match self.generate_simple(prompt).await {
            Ok(response) => {
                debug!("Primary generation method succeeded");
                Ok(response)
            }
            Err(primary_error) => {
                warn!(
                    "Primary generation failed, trying robust fallback: {}",
                    primary_error
                );

                match self.query_robust(prompt).await {
                    Ok(response) => {
                        info!("Fallback generation method succeeded");
                        Ok(response)
                    }
                    Err(fallback_error) => {
                        error!(
                            "Both generation methods failed - Primary: {}, Fallback: {}",
                            primary_error, fallback_error
                        );
                        Err(format!(
                            "LLM generation failed - Primary: {primary_error}, Fallback: {fallback_error}"
                        )
                        .into())
                    }
                }
            }
        }
    }

    async fn generate_simple(
        &self,
        prompt: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        debug!("Attempting simple generation method");

        let response = self
            .llm_adapter
            .generate_response(prompt)
            .await
            .map_err(|e| format!("Simple generation failed: {e}"))?;

        let cleaned_response = self.extract_json_from_response(&response);
        Ok(cleaned_response)
    }

    async fn query_robust(
        &self,
        prompt: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        debug!("Attempting robust generation method");

        let response = self
            .llm_adapter
            .generate_response(prompt)
            .await
            .map_err(|e| format!("Robust generation failed: {e}"))?;

        let cleaned_response = self.extract_json_from_response(&response);
        Ok(cleaned_response)
    }

    pub async fn analyse_content(
        &self,
        content: &str,
        analysis_type: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        info!("Analysing content of type: {}", analysis_type);

        let analysis_prompt = format!(
            "Analyse the {analysis_type} content and provide structured insights:\n\n{content}\n\nYour analysis must be structured, succinct and easy to parse."
        );

        self.generate_response(&analysis_prompt).await
    }

    fn extract_json_from_response(&self, response: &str) -> String {
        debug!("Cleaning and extracting JSON from response");

        if let Some(json) = self.extract_from_code_block(response) {
            debug!("Extracted JSON from code block");
            return json;
        }

        if let Some(json) = self.extract_json_object(response) {
            debug!("Extracted JSON object from response");
            return json;
        }

        debug!("ℹNo JSON structure found, returning cleaned response");
        self.clean_response(response)
    }

    fn extract_from_code_block(&self, response: &str) -> Option<String> {
        if let Some(start) = response.find("```json") {
            if let Some(end) = response[start..].find("```") {
                let json_start = start + 7;
                let json_end = start + end;
                if json_start < json_end {
                    return Some(response[json_start..json_end].trim().to_string());
                }
            }
        }

        if let Some(start) = response.find("```") {
            let content_start = start + 3;
            if let Some(end) = response[content_start..].find("```") {
                let content = response[content_start..content_start + end].trim();

                if content.starts_with('{') && content.ends_with('}') {
                    return Some(content.to_string());
                }
            }
        }

        None
    }

    fn extract_json_object(&self, response: &str) -> Option<String> {
        if let Some(start) = response.find('{') {
            let mut brace_count = 0;
            let mut end_pos = start;

            for (i, ch) in response[start..].char_indices() {
                match ch {
                    '{' => brace_count += 1,
                    '}' => {
                        brace_count -= 1;
                        if brace_count == 0 {
                            end_pos = start + i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }

            if brace_count == 0 && end_pos > start {
                return Some(response[start..end_pos].to_string());
            }
        }

        None
    }

    fn clean_response(&self, response: &str) -> String {
        response
            .trim()
            .replace("```json", "")
            .replace("```", "")
            .replace("Here's the", "")
            .replace("Here is the", "")
            .trim()
            .to_string()
    }

    pub async fn health_check(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        info!("Performing LLM health check");

        let simple_prompt = "Respond with exactly: OK";

        match self.generate_response(simple_prompt).await {
            Ok(response) => {
                let is_healthy = response.trim().to_uppercase().contains("OK");
                if is_healthy {
                    info!("LLM health check passed");
                } else {
                    warn!("LLM health check questionable - Response: {}", response);
                }
                Ok(is_healthy)
            }
            Err(e) => {
                error!("LLM health check failed: {}", e);
                Ok(false)
            }
        }
    }

    pub async fn construct_api_call_prompt(
        &self,
        api_structure: &Value,
        goal: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        info!("️ Constructing intelligent API call prompt");

        let construction_prompt = format!(
            "Given this API structure and response format:\n\n{}\n\nGoal: {}\n\nAnalyse the API structure and construct an optimised API call to achieve the goal. Consider:\n1. Required parameters and data paths\n2. Most efficient data retrieval strategy\n3. Specific fields needed for the goal\n4. Any filtering or querying capabilities\n\nProvide your response as a structured plan with the specific API call details.",
            serde_json::to_string_pretty(api_structure)?,
            goal
        );

        self.generate_response(&construction_prompt).await
    }
}
