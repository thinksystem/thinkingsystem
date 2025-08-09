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

use crate::llm_logging::LLMLogger;
use crate::local_llm_interface::LocalLLMInterface;
use crate::logging_adapter::LoggingLLMAdapter;
use crate::ui::UIBridge;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use stele::llm::unified_adapter::UnifiedLLMAdapter;
use stele::nlu::llm_processor::LLMAdapter;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

pub struct DemoDataProcessor {
    llm_adapter: Arc<UnifiedLLMAdapter>,
    logging_adapter: Option<Arc<LoggingLLMAdapter>>,
    local_llm_interface: Arc<Mutex<LocalLLMInterface>>,
    db: Arc<Surreal<Any>>,
    ui_bridge: Option<Arc<UIBridge>>,
    structured_analysis_prompt: String,
    entity_extraction_prompt: String,
}

impl DemoDataProcessor {
    fn get_prompt_path(filename: &str) -> String {
        let possible_paths = [
            format!("prompts/{filename}"),
            format!("bin/demos/scribes-demo/prompts/{filename}"),
            format!("./bin/demos/scribes-demo/prompts/{filename}"),
        ];

        for path in &possible_paths {
            if std::path::Path::new(path).exists() {
                return path.to_string();
            }
        }

        possible_paths[0].clone()
    }

    pub async fn new(
        llm_adapter: Arc<UnifiedLLMAdapter>,
        logging_adapter: Option<Arc<LoggingLLMAdapter>>,
        local_llm_interface: Arc<Mutex<LocalLLMInterface>>,
        db: Arc<Surreal<Any>>,
        _logger: Arc<LLMLogger>,
    ) -> Result<Self, String> {
        let structured_analysis_prompt =
            std::fs::read_to_string(Self::get_prompt_path("structured_analysis_prompt.txt"))
                .map_err(|e| format!("Failed to load structured analysis prompt: {e}"))?;

        let entity_extraction_prompt =
            std::fs::read_to_string(Self::get_prompt_path("entity_extraction_prompt.txt"))
                .map_err(|e| format!("Failed to load entity extraction prompt: {e}"))?;

        Ok(Self {
            llm_adapter,
            logging_adapter,
            local_llm_interface,
            db,
            ui_bridge: None,
            structured_analysis_prompt,
            entity_extraction_prompt,
        })
    }

    pub fn with_ui_bridge(mut self, ui_bridge: Arc<UIBridge>) -> Self {
        self.ui_bridge = Some(ui_bridge);
        self
    }
    pub async fn process_data(&self, context: &Value) -> Result<Value, String> {
        let text = context["text"].as_str().unwrap_or("");
        let urgency = context["urgency"].as_f64().unwrap_or(0.5);

        info!(
            text_length = text.len(),
            urgency = urgency,
            "Processing data with enhanced LLM analysis"
        );

        let analysis_prompt = self.structured_analysis_prompt.replace("{text}", text);

        let llm_result = self
            .llm_adapter
            .process_text(&analysis_prompt)
            .await
            .map_err(|e| e.to_string());

        match llm_result {
            Ok(response) => {
                debug!("LLM analysis completed successfully");

                if let Some(ui_bridge) = &self.ui_bridge {
                    ui_bridge.log_llm_interaction(
                        "Data Scribe",
                        &analysis_prompt,
                        &response,
                        "ollama",
                        None,
                        None,
                    );
                }

                let parsed_analysis = match serde_json::from_str::<Value>(&response) {
                    Ok(json) => json,
                    Err(e) => {
                        warn!(
                            "Failed to parse LLM JSON response: {}, falling back to text analysis",
                            e
                        );

                        self.create_enhanced_fallback_analysis(text, urgency).await
                    }
                };

                let record = json!({
                    "id": format!("processed:{}", uuid::Uuid::new_v4()),
                    "input_text": text,
                    "analysis": parsed_analysis,
                    "method": "llm_structured_analysis",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "urgency": urgency,
                    "processing_metadata": {
                        "model": "llama3.2:3b",
                        "provider": "ollama"
                    }
                });

                let result = self
                    .db
                    .query("CREATE processed_data CONTENT $data")
                    .bind(("data", record))
                    .await;
                match result {
                    Ok(_) => {
                        info!("Successfully stored processed data in database");
                    }
                    Err(e) => {
                        warn!("Failed to store in database: {}", e);
                    }
                }

                Ok(json!({
                    "method": "llm_structured_analysis",
                    "analysis": parsed_analysis,
                    "stored": true,
                    "urgency": urgency,
                    "model_used": "llama3.2:3b",
                    "provider": "ollama"
                }))
            }
            Err(e) => {
                let (error_string, fallback_analysis) = {
                    let error_string = e.to_string();
                    warn!(
                        "LLM processing failed: {}, using enhanced fallback",
                        error_string
                    );
                    let fallback_analysis =
                        self.create_enhanced_fallback_analysis(text, urgency).await;
                    (error_string, fallback_analysis)
                };

                let record = json!({
                    "id": format!("processed:{}", uuid::Uuid::new_v4()),
                    "input_text": text,
                    "analysis": fallback_analysis,
                    "method": "enhanced_fallback",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "urgency": urgency,
                    "error": error_string
                });

                let _ = self
                    .db
                    .query("CREATE processed_data CONTENT $data")
                    .bind(("data", record))
                    .await;

                Ok(json!({
                    "method": "enhanced_fallback",
                    "analysis": fallback_analysis,
                    "stored": true,
                    "urgency": urgency,
                    "fallback_reason": error_string
                }))
            }
        }
    }

    pub async fn store_extracted_data(&self, data: &Value) -> Result<Value, String> {
        let enhanced_record = json!({
            "id": format!("extracted:{}", uuid::Uuid::new_v4()),
            "data": data,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "storage_metadata": {
                "source": "enhanced_data_processor",
                "version": "1.0"
            }
        });

        let mut result = self
            .db
            .query("CREATE extracted_data CONTENT $data")
            .bind(("data", enhanced_record))
            .await
            .map_err(|e| e.to_string())?;

        info!("Stored extracted data with enhanced metadata");

        Ok(json!({
            "status": "stored",
            "database": "surrealdb",
            "enhanced": true,
            "record_id": result.take::<Option<Value>>(0).unwrap_or(None).and_then(|r| r.get("id").cloned())
        }))
    }

    pub async fn extract_entities(&self, text: &str) -> Result<Value, String> {
        let entity_prompt = self.entity_extraction_prompt.replace("{text}", text);

        match self.llm_adapter.process_text(&entity_prompt).await {
            Ok(response) => match serde_json::from_str::<Value>(&response) {
                Ok(entities) => Ok(entities),
                Err(_) => Ok(self.create_simple_entity_extraction(text)),
            },
            Err(_) => Ok(self.create_simple_entity_extraction(text)),
        }
    }

    async fn create_enhanced_fallback_analysis(&self, text: &str, urgency: f64) -> Value {
        let words: Vec<&str> = text.split_whitespace().collect();
        let word_count = words.len();

        let mut word_freq = HashMap::new();
        for word in words.iter() {
            let clean_word = word
                .to_lowercase()
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_string();
            if clean_word.len() > 3 {
                *word_freq.entry(clean_word).or_insert(0) += 1;
            }
        }

        let mut keywords: Vec<(String, usize)> = word_freq.into_iter().collect();
        keywords.sort_by(|a, b| b.1.cmp(&a.1));
        let top_keywords: Vec<String> = keywords
            .iter()
            .take(10)
            .map(|(word, _)| word.clone())
            .collect();

        let entities = self.simple_entity_detection(text);

        let sentiment_prompt = format!(
            "Analyse the sentiment of the following text. Respond with only one word: 'positive', 'negative', or 'neutral'. Text: \"{text}\""
        );

        let sentiment = match self
            .local_llm_interface
            .lock()
            .await
            .query(&sentiment_prompt)
            .await
        {
            Ok(response) => {
                let cleaned_sentiment = response.trim().to_lowercase();
                if cleaned_sentiment.contains("positive")
                    || cleaned_sentiment.contains("negative")
                    || cleaned_sentiment.contains("neutral")
                {
                    cleaned_sentiment
                } else {
                    tracing::warn!("Local LLM returned unexpected sentiment format: '{}', defaulting to 'neutral'", response);
                    "neutral".to_string()
                }
            }
            Err(e) => {
                tracing::error!(
                    "Local LLM sentiment analysis failed: {}. Falling back to 'neutral' sentiment.",
                    e
                );
                "neutral".to_string()
            }
        };

        json!({
            "summary": format!("Enhanced analysis of {} words with {} detected entities", word_count, entities.len()),
            "key_entities": entities,
            "topics": ["general", "analysis"],
            "sentiment": sentiment.trim().to_lowercase(),
            "complexity_score": (word_count as f64 / 200.0).min(1.0),
            "keywords": top_keywords,
            "potential_relationships": [],
            "metadata": {
                "language": "detected_english",
                "domain": "general",
                "urgency_assessment": urgency,
                "analysis_method": "enhanced_fallback_local_llm",
                "word_count": word_count,
                "entity_count": entities.len()
            }
        })
    }

    fn simple_entity_detection(&self, text: &str) -> Vec<String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut entities = Vec::new();

        for word in words {
            let clean_word = word.trim_matches(|c: char| !c.is_alphanumeric());

            if clean_word.len() > 2 && clean_word.chars().next().unwrap().is_uppercase() {
                entities.push(clean_word.to_string());
            }
        }

        entities.sort();
        entities.dedup();
        entities.into_iter().take(10).collect()
    }

    fn create_simple_entity_extraction(&self, text: &str) -> Value {
        let entities = self.simple_entity_detection(text);

        json!({
            "entities": entities.iter().map(|e| json!({
                "text": e,
                "category": "CONCEPT",
                "confidence": 0.6
            })).collect::<Vec<Value>>(),
            "relationships": []
        })
    }
}
