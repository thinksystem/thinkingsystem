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
use gliner::model::pipeline::span::SpanMode;
use gliner::model::{input::text::TextInput, params::Parameters, GLiNER};
use orp::params::RuntimeParameters;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::messaging::insight::config::SecurityConfig;

fn resolve_model_path(relative_path: &str) -> PathBuf {
    let mut current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    loop {
        let cargo_toml = current_dir.join("Cargo.toml");
        if cargo_toml.exists() {
            let models_dir = current_dir.join("models");
            if models_dir.exists() {
                return current_dir.join(relative_path);
            }
        }

        match current_dir.parent() {
            Some(parent) => current_dir = parent.to_path_buf(),
            None => break,
        }
    }

    PathBuf::from(relative_path)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NerConfig {
    pub model_path: String,
    pub enabled: bool,
    pub entity_labels: Vec<String>,
    pub min_confidence_threshold: f64,
    pub entity_weights: HashMap<String, f64>,
    pub max_text_length: usize,
}

impl Default for NerConfig {
    fn default() -> Self {
        let mut entity_weights = HashMap::new();
        entity_weights.insert("person".to_string(), 0.8);
        entity_weights.insert("email".to_string(), 0.9);
        entity_weights.insert("phone".to_string(), 0.9);
        entity_weights.insert("organisation".to_string(), 0.6);
        entity_weights.insert("location".to_string(), 0.5);
        entity_weights.insert("credit_card".to_string(), 1.0);
        entity_weights.insert("ssn".to_string(), 1.0);
        entity_weights.insert("ip_address".to_string(), 0.7);
        entity_weights.insert("date".to_string(), 0.3);
        entity_weights.insert("url".to_string(), 0.4);

        Self {
            model_path: resolve_model_path("models/gliner-x-small")
                .to_string_lossy()
                .to_string(),
            enabled: true,
            entity_labels: vec![
                "person".to_string(),
                "email".to_string(),
                "phone".to_string(),
                "organisation".to_string(),
                "location".to_string(),
                "credit_card".to_string(),
                "ssn".to_string(),
                "ip_address".to_string(),
                "date".to_string(),
                "url".to_string(),
            ],
            min_confidence_threshold: 0.5,
            entity_weights,
            max_text_length: 2048,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedEntity {
    pub text: String,
    pub label: String,
    pub start: usize,
    pub end: usize,
    pub confidence: f64,
    pub risk_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NerAnalysisResult {
    pub entities: Vec<DetectedEntity>,
    pub overall_ner_score: f64,
    pub processing_time_ms: f64,
    pub text_truncated: bool,
}

pub struct NerAnalyser {
    model: Option<Arc<GLiNER<SpanMode>>>,
    config: NerConfig,
}

impl NerAnalyser {
    pub fn new(config: NerConfig) -> Self {
        Self {
            model: None,
            config,
        }
    }

    pub fn from_security_config(security_config: &SecurityConfig) -> Self {
        let mut config = NerConfig::default();

        let email_weight = (security_config.scoring.at_symbol_bonus / 10.0).clamp(0.1, 1.0);
        let credit_card_weight =
            (security_config.scoring.all_digits_len_bonus_10 / 5.0).clamp(0.1, 1.0);
        let ssn_weight = (security_config.scoring.all_digits_bonus / 5.0).clamp(0.1, 1.0);

        config.min_confidence_threshold = security_config.thresholds.llm_grey_area_low;

        if let Some(weight) = config.entity_weights.get_mut("email") {
            *weight = email_weight;
        }
        if let Some(weight) = config.entity_weights.get_mut("credit_card") {
            *weight = credit_card_weight;
        }
        if let Some(weight) = config.entity_weights.get_mut("ssn") {
            *weight = ssn_weight;
        }

        let model_path = format!("{}/gliner-x-small", security_config.paths.state_dir);
        if Path::new(&model_path).exists() {
            config.model_path = model_path;
        }

        Self::new(config)
    }

    pub fn initialise_model(&mut self) -> Result<()> {
        if self.model.is_some() || !self.config.enabled {
            return Ok(());
        }

        let model_path = Path::new(&self.config.model_path);
        if !model_path.exists() {
            tracing::warn!(
                "GLiNER model path does not exist: {}",
                self.config.model_path
            );
            return Err(anyhow::anyhow!("GLiNER model path does not exist"));
        }

        let tokenizer_path = model_path.join("tokenizer.json");
        let onnx_path = model_path.join("onnx/model.onnx");

        if !tokenizer_path.exists() || !onnx_path.exists() {
            return Err(anyhow::anyhow!(
                "Required GLiNER model files not found (tokenizer.json or onnx/model.onnx)"
            ));
        }

        tracing::info!("Loading GLiNER model from: {}", model_path.display());

        let model = GLiNER::<SpanMode>::new(
            Parameters::default(),
            RuntimeParameters::default(),
            tokenizer_path.to_str().unwrap(),
            onnx_path.to_str().unwrap(),
        )
        .map_err(|e| anyhow::anyhow!("Failed to create GLiNER model: {}", e))?;

        self.model = Some(Arc::new(model));
        tracing::info!("GLiNER model loaded successfully");
        Ok(())
    }

    pub fn analyse_text(&mut self, text: &str) -> Result<NerAnalysisResult> {
        let start_time = Instant::now();

        if !self.config.enabled || text.trim().is_empty() {
            return Ok(NerAnalysisResult {
                entities: Vec::new(),
                overall_ner_score: 0.0,
                processing_time_ms: start_time.elapsed().as_secs_f64() * 1000.0,
                text_truncated: false,
            });
        }

        if self.model.is_none() {
            if let Err(e) = self.initialise_model() {
                tracing::warn!(
                    "Failed to initialise GLiNER model: {}. Falling back to empty result.",
                    e
                );
                return Ok(NerAnalysisResult {
                    entities: Vec::new(),
                    overall_ner_score: 0.0,
                    processing_time_ms: start_time.elapsed().as_secs_f64() * 1000.0,
                    text_truncated: false,
                });
            }
        }

        let (analysis_text, text_truncated) = if text.len() > self.config.max_text_length {
            (&text[..self.config.max_text_length], true)
        } else {
            (text, false)
        };

        let entities = match &self.model {
            Some(model) => match self.run_ner_inference(model, analysis_text) {
                Ok(entities) => entities,
                Err(e) => {
                    tracing::warn!("GLiNER inference failed: {}. Returning empty result.", e);
                    Vec::new()
                }
            },
            None => Vec::new(),
        };

        let overall_ner_score = self.calculate_overall_score(&entities);
        let processing_time_ms = start_time.elapsed().as_secs_f64() * 1000.0;

        Ok(NerAnalysisResult {
            entities,
            overall_ner_score,
            processing_time_ms,
            text_truncated,
        })
    }

    fn run_ner_inference(
        &self,
        model: &GLiNER<SpanMode>,
        text: &str,
    ) -> Result<Vec<DetectedEntity>> {
        let labels: Vec<&str> = self
            .config
            .entity_labels
            .iter()
            .map(|s| s.as_str())
            .collect();
        let input = TextInput::from_str(&[text], &labels)
            .map_err(|e| anyhow::anyhow!("Failed to create TextInput: {}", e))?;
        let output = model
            .inference(input)
            .map_err(|e| anyhow::anyhow!("GLiNER inference failed: {}", e))?;

        let mut entities = Vec::new();

        if let Some(doc_spans) = output.spans.into_iter().next() {
            for span in doc_spans {
                let confidence = span.probability();

                if confidence as f64 >= self.config.min_confidence_threshold {
                    let label = span.class().to_string();
                    let risk_score = self.calculate_entity_risk_score(&label, confidence as f64);

                    entities.push(DetectedEntity {
                        text: span.text().to_string(),
                        label,
                        start: span.sequence(),
                        end: span.sequence() + span.text().len(),
                        confidence: confidence as f64,
                        risk_score,
                    });
                }
            }
        }

        entities.sort_by(|a, b| b.risk_score.partial_cmp(&a.risk_score).unwrap());

        Ok(entities)
    }

    fn calculate_entity_risk_score(&self, label: &str, confidence: f64) -> f64 {
        let base_weight = self.config.entity_weights.get(label).unwrap_or(&0.5);
        (base_weight * confidence).min(1.0)
    }

    #[cfg(test)]
    pub fn calculate_entity_risk_score_test(&self, label: &str, confidence: f64) -> f64 {
        self.calculate_entity_risk_score(label, confidence)
    }

    fn calculate_overall_score(&self, entities: &[DetectedEntity]) -> f64 {
        if entities.is_empty() {
            return 0.0;
        }

        entities.iter().map(|e| e.risk_score).fold(0.0, f64::max)
    }

    pub fn update_config(&mut self, config: NerConfig) {
        let model_path_changed = self.config.model_path != config.model_path;
        self.config = config;

        if model_path_changed {
            self.model = None;
        }
    }

    pub fn is_ready(&self) -> bool {
        self.config.enabled && (self.model.is_some() || Path::new(&self.config.model_path).exists())
    }

    pub fn get_config(&self) -> &NerConfig {
        &self.config
    }

    #[cfg(test)]
    pub fn is_model_loaded(&self) -> bool {
        self.model.is_some()
    }
}

impl Default for NerAnalyser {
    fn default() -> Self {
        Self::new(NerConfig::default())
    }
}
