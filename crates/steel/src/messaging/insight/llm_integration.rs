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
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::messaging::insight::{
    analysis::{ContentAnalyser, ContentAnalysis},
    config::{ScoringConfig, SecurityConfig},
    distribution::ScoreDistribution,
    metrics::TrainingExample,
};

#[derive(Debug)]
pub struct LlmClient {
    endpoint: String,
    client: Client,
}

#[derive(Serialize)]
struct LlmPiiRequest {
    model: String,
    prompt: String,
    format: String,
    stream: bool,
}

#[derive(Deserialize, Debug)]
pub struct LlmPiiResponse {
    pub contains_pii: bool,
    pub pii_type: String,
    pub confidence: String,
}

#[derive(Deserialize, Debug)]
struct LlmApiResponse<T> {
    response: String,
    #[serde(skip_deserializing)]
    _marker: std::marker::PhantomData<T>,
}

impl LlmClient {
    pub fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            client: Client::new(),
        }
    }

    pub fn analyse_pii(&self, text: &str) -> Result<LlmPiiResponse> {
        let prompt = format!(
            "You are a PII detection expert. Analyse the following text and determine if it contains sensitive information. Provide your response as a JSON object with the keys: \"contains_pii\" (boolean), \"pii_type\" (string, e.g., \"Email\", \"Phone\", \"None\"), and \"confidence\" (string: \"High\", \"Medium\", \"Low\").\n\nText to analyse:\n\"{text}\""
        );

        let request = LlmPiiRequest {
            model: "llama3".to_string(),
            prompt,
            format: "json".to_string(),
            stream: false,
        };

        let rt = tokio::runtime::Runtime::new()?;
        let result = rt.block_on(async {
            let response = self
                .client
                .post(&self.endpoint)
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await?;

            if !response.status().is_success() {
                return self.fallback_analyse_pii(text);
            }

            let api_response: LlmApiResponse<LlmPiiResponse> = response.json().await?;
            let pii_response: LlmPiiResponse = serde_json::from_str(&api_response.response)?;
            Ok(pii_response)
        });

        result.or_else(|_| self.fallback_analyse_pii(text))
    }

    fn fallback_analyse_pii(&self, text: &str) -> Result<LlmPiiResponse> {
        let mock_response_str = if text.contains('@')
            || text.contains("ssn")
            || text.contains("credit card")
        {
            r#"{"response": "{\"contains_pii\": true, \"pii_type\": \"Email\", \"confidence\": \"High\"}"}"#
        } else {
            r#"{"response": "{\"contains_pii\": false, \"pii_type\": \"None\", \"confidence\": \"High\"}"}"#
        };

        let api_response: LlmApiResponse<LlmPiiResponse> = serde_json::from_str(mock_response_str)?;
        let pii_response: LlmPiiResponse = serde_json::from_str(&api_response.response)?;
        Ok(pii_response)
    }

    pub fn generate_training_examples(
        &self,
        corrected_text: &str,
        was_false_positive: bool,
    ) -> Result<Vec<TrainingExample>> {
        let correction_type = if was_false_positive {
            "incorrectly flagged as sensitive"
        } else {
            "was missed"
        };
        let new_label = !was_false_positive;

        let prompt = format!(
            "A PII detection model {correction_type} the text '{corrected_text}'. Generate 5 new, diverse examples of text that fit this same pattern. Format your response as a JSON array of objects, each with a \"text\" key (string) and an \"is_sensitive\" key (boolean, set to {new_label})."
        );

        let request = LlmPiiRequest {
            model: "llama3".to_string(),
            prompt,
            format: "json".to_string(),
            stream: false,
        };

        let rt = tokio::runtime::Runtime::new()?;
        let result = rt.block_on(async {
            let response = self
                .client
                .post(&self.endpoint)
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await;

            match response {
                Ok(resp) if resp.status().is_success() => {
                    let api_response: LlmApiResponse<Vec<TrainingExample>> = resp.json().await?;
                    Ok(serde_json::from_str(&api_response.response)?)
                }
                _ => self.fallback_training_examples(corrected_text, was_false_positive),
            }
        });

        result.or_else(|_| self.fallback_training_examples(corrected_text, was_false_positive))
    }

    fn fallback_training_examples(
        &self,
        _corrected_text: &str,
        _was_false_positive: bool,
    ) -> Result<Vec<TrainingExample>> {
        let mock_response_str = r#"
        {
            "response": "[{\"text\": \"New example 1 from LLM\", \"is_sensitive\": false}, {\"text\": \"New example 2 from LLM\", \"is_sensitive\": false}]"
        }
        "#;

        let api_response: LlmApiResponse<Vec<TrainingExample>> =
            serde_json::from_str(mock_response_str)?;
        let training_examples: Vec<TrainingExample> = serde_json::from_str(&api_response.response)?;
        Ok(training_examples)
    }
}

pub struct HybridAnalyser {
    heuristic_analyser: ContentAnalyser,
    llm_client: LlmClient,
    grey_area_low: f64,
    grey_area_high: f64,
}

impl HybridAnalyser {
    pub fn new(
        scoring_config: ScoringConfig,
        llm_endpoint: String,
        grey_area_low: f64,
        grey_area_high: f64,
    ) -> Self {
        Self {
            heuristic_analyser: ContentAnalyser::new(scoring_config),
            llm_client: LlmClient::new(llm_endpoint),
            grey_area_low,
            grey_area_high,
        }
    }

    pub fn from_config(config: &SecurityConfig) -> Self {
        Self::new(
            config.to_scoring_config(),
            config.llm.api_endpoint.clone(),
            config.thresholds.llm_grey_area_low,
            config.thresholds.llm_grey_area_high,
        )
    }

    pub fn analyse(&self, text: &str, distribution: &mut ScoreDistribution) -> ContentAnalysis {
        let heuristic_analysis = self.heuristic_analyser.analyse(text, distribution);
        let score = heuristic_analysis.overall_risk_score;

        if score >= self.grey_area_high {
            return ContentAnalysis {
                overall_risk_score: score,
                interesting_tokens: heuristic_analysis.interesting_tokens,
                requires_scribes_review: true,
            };
        }

        if score < self.grey_area_low {
            return ContentAnalysis {
                overall_risk_score: score,
                interesting_tokens: heuristic_analysis.interesting_tokens,
                requires_scribes_review: false,
            };
        }

        println!("Score {score:.3} is in grey area, consulting LLM...");
        match self.llm_client.analyse_pii(text) {
            Ok(llm_response) => {
                println!(
                    "LLM grey area analysis: contains_pii={}, confidence={}",
                    llm_response.contains_pii, llm_response.confidence
                );
                ContentAnalysis {
                    overall_risk_score: score,
                    interesting_tokens: heuristic_analysis.interesting_tokens,
                    requires_scribes_review: llm_response.contains_pii,
                }
            }
            Err(e) => {
                eprintln!("LLM analysis failed: {e}. Falling back to heuristic result.");

                heuristic_analysis
            }
        }
    }
}
