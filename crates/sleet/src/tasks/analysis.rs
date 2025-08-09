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

use crate::tasks::TaskError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

use steel::messaging::insight::ner_analysis::{NerAnalyser, NerConfig};

pub type TaskResult<T> = Result<T, TaskError>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClassificationPattern {
    pub name: String,
    pub keywords: Vec<String>,
    pub confidence: f64,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationConfig {
    pub patterns: Vec<ClassificationPattern>,
    pub default_pattern: ClassificationPattern,
    pub case_sensitive: bool,
    pub min_confidence: f64,
}

impl Default for ClassificationConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl ClassificationConfig {
    pub fn new() -> Self {
        Self {
            patterns: Vec::new(),
            default_pattern: ClassificationPattern {
                name: "general".to_string(),
                keywords: Vec::new(),
                confidence: 0.5,
                metadata: HashMap::new(),
            },
            case_sensitive: false,
            min_confidence: 0.0,
        }
    }

    pub fn add_pattern(mut self, pattern: ClassificationPattern) -> Self {
        self.patterns.push(pattern);
        self
    }

    pub fn with_default(mut self, pattern: ClassificationPattern) -> Self {
        self.default_pattern = pattern;
        self
    }

    pub fn case_sensitive(mut self, sensitive: bool) -> Self {
        self.case_sensitive = sensitive;
        self
    }

    pub fn min_confidence(mut self, threshold: f64) -> Self {
        self.min_confidence = threshold;
        self
    }

    pub fn classify(&self, text: &str) -> (String, f64, HashMap<String, serde_json::Value>) {
        let search_text = if self.case_sensitive {
            text.to_string()
        } else {
            text.to_lowercase()
        };

        for pattern in &self.patterns {
            let keywords = if self.case_sensitive {
                pattern.keywords.clone()
            } else {
                pattern.keywords.iter().map(|k| k.to_lowercase()).collect()
            };

            if keywords.iter().any(|keyword| search_text.contains(keyword))
                && pattern.confidence >= self.min_confidence
            {
                return (
                    pattern.name.clone(),
                    pattern.confidence,
                    pattern.metadata.clone(),
                );
            }
        }

        (
            self.default_pattern.name.clone(),
            self.default_pattern.confidence,
            self.default_pattern.metadata.clone(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAnalysis {
    pub tags: HashMap<String, f64>,

    pub complexity_score: f64,

    pub estimated_duration_secs: u64,

    pub required_skills: Vec<String>,

    pub collaboration_score: f64,

    pub resource_needs: HashMap<String, f64>,

    pub confidence: f64,

    pub entities: HashMap<String, Vec<String>>,
}

impl TaskAnalysis {
    pub fn primary_task_type(&self) -> Option<String> {
        self.tags
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(tag, _)| tag.clone())
    }

    pub fn get_tags_by_pattern(&self, pattern: &str) -> Vec<(String, f64)> {
        self.tags
            .iter()
            .filter(|(tag, _)| tag.contains(pattern))
            .map(|(tag, score)| (tag.clone(), *score))
            .collect()
    }

    pub fn requires_collaboration(&self, threshold: f64) -> bool {
        self.collaboration_score > threshold
    }

    pub fn high_confidence_tags(&self, threshold: f64) -> Vec<(String, f64)> {
        self.tags
            .iter()
            .filter(|(_, score)| **score > threshold)
            .map(|(tag, score)| (tag.clone(), *score))
            .collect()
    }

    pub fn get_entities(&self, entity_type: &str) -> Vec<String> {
        self.entities.get(entity_type).cloned().unwrap_or_default()
    }
}

pub trait TaskAnalyser: Send + Sync {
    fn analyse(&self, description: &str) -> TaskResult<TaskAnalysis>;

    fn analyser_type(&self) -> &'static str;

    fn is_available(&self) -> bool {
        true
    }
}

pub struct ConfigurableTaskAnalyser {
    task_type_classifier: ClassificationConfig,
    domain_classifier: Option<ClassificationConfig>,
    complexity_config: ComplexityConfig,
    resource_config: ResourceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexityConfig {
    pub word_count_factor: f64,
    pub base_complexity: f64,
    pub max_complexity: f64,
    pub keyword_multipliers: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    pub base_duration_mapping: HashMap<String, u64>,
    pub complexity_duration_multiplier: f64,
    pub skill_mapping: HashMap<String, Vec<String>>,
    pub resource_estimation: HashMap<String, HashMap<String, f64>>,
}

impl Default for ComplexityConfig {
    fn default() -> Self {
        Self {
            word_count_factor: 50.0,
            base_complexity: 0.3,
            max_complexity: 1.0,
            keyword_multipliers: HashMap::new(),
        }
    }
}

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            base_duration_mapping: HashMap::new(),
            complexity_duration_multiplier: 1.0,
            skill_mapping: HashMap::new(),
            resource_estimation: HashMap::new(),
        }
    }
}

impl ConfigurableTaskAnalyser {
    pub fn new(task_type_classifier: ClassificationConfig) -> Self {
        Self {
            task_type_classifier,
            domain_classifier: None,
            complexity_config: ComplexityConfig::default(),
            resource_config: ResourceConfig::default(),
        }
    }

    pub fn with_domain_classifier(mut self, domain_classifier: ClassificationConfig) -> Self {
        self.domain_classifier = Some(domain_classifier);
        self
    }

    pub fn with_complexity_config(mut self, config: ComplexityConfig) -> Self {
        self.complexity_config = config;
        self
    }

    pub fn with_resource_config(mut self, config: ResourceConfig) -> Self {
        self.resource_config = config;
        self
    }

    fn calculate_complexity(&self, description: &str, task_type: &str) -> f64 {
        let word_count = description.split_whitespace().count() as f64;
        let base_score = (word_count / self.complexity_config.word_count_factor).min(1.0)
            * (1.0 - self.complexity_config.base_complexity)
            + self.complexity_config.base_complexity;

        let multiplier = self
            .complexity_config
            .keyword_multipliers
            .get(task_type)
            .unwrap_or(&1.0);

        (base_score * multiplier).min(self.complexity_config.max_complexity)
    }

    fn estimate_duration(&self, task_type: &str, complexity: f64) -> u64 {
        let base_duration = self
            .resource_config
            .base_duration_mapping
            .get(task_type)
            .unwrap_or(&3600);

        (*base_duration as f64
            * (1.0 + complexity * self.resource_config.complexity_duration_multiplier))
            as u64
    }

    fn get_required_skills(&self, task_type: &str) -> Vec<String> {
        self.resource_config
            .skill_mapping
            .get(task_type)
            .cloned()
            .unwrap_or_else(|| vec!["general".to_string()])
    }

    fn estimate_resources(&self, task_type: &str, complexity: f64) -> HashMap<String, f64> {
        if let Some(resource_map) = self.resource_config.resource_estimation.get(task_type) {
            resource_map
                .iter()
                .map(|(k, v)| (k.clone(), v * (1.0 + complexity)))
                .collect()
        } else {
            let mut default_resources = HashMap::new();
            default_resources.insert("compute_units".to_string(), 10.0 * (1.0 + complexity));
            default_resources
        }
    }

    fn find_all_matching_patterns(
        &self,
        description: &str,
    ) -> Vec<(String, f64, HashMap<String, serde_json::Value>)> {
        let search_text = if self.task_type_classifier.case_sensitive {
            description.to_string()
        } else {
            description.to_lowercase()
        };

        let mut matches = Vec::new();

        for pattern in &self.task_type_classifier.patterns {
            let keywords = if self.task_type_classifier.case_sensitive {
                pattern.keywords.clone()
            } else {
                pattern.keywords.iter().map(|k| k.to_lowercase()).collect()
            };

            if keywords.iter().any(|keyword| search_text.contains(keyword))
                && pattern.confidence >= self.task_type_classifier.min_confidence
            {
                matches.push((
                    pattern.name.clone(),
                    pattern.confidence,
                    pattern.metadata.clone(),
                ));
            }
        }

        if matches.is_empty() {
            matches.push((
                self.task_type_classifier.default_pattern.name.clone(),
                self.task_type_classifier.default_pattern.confidence,
                self.task_type_classifier.default_pattern.metadata.clone(),
            ));
        }

        matches
    }
}

impl TaskAnalyser for ConfigurableTaskAnalyser {
    fn analyse(&self, description: &str) -> TaskResult<TaskAnalysis> {
        let matches = self.find_all_matching_patterns(description);

        let (primary_task_type, task_confidence, _) = matches.first().unwrap().clone();

        let mut tags = HashMap::new();

        for (task_type, confidence, task_metadata) in matches {
            tags.insert(task_type.clone(), confidence);

            for (key, value) in task_metadata {
                tags.insert(key, value.as_f64().unwrap_or(0.0));
            }
        }

        if let Some(domain_classifier) = &self.domain_classifier {
            let (domain, domain_confidence, domain_metadata) =
                domain_classifier.classify(description);
            tags.insert(domain, domain_confidence);

            for (key, value) in domain_metadata {
                tags.insert(key, value.as_f64().unwrap_or(0.0));
            }
        }

        let complexity_score = self.calculate_complexity(description, &primary_task_type);
        let estimated_duration_secs = self.estimate_duration(&primary_task_type, complexity_score);
        let required_skills = self.get_required_skills(&primary_task_type);
        let resource_needs = self.estimate_resources(&primary_task_type, complexity_score);

        let description_lower = description.to_lowercase();
        let collaboration_score =
            if description_lower.contains("team") || description_lower.contains("collaborative") {
                0.8
            } else if description.split_whitespace().count() > 50 {
                0.5
            } else {
                0.2
            };

        Ok(TaskAnalysis {
            tags,
            complexity_score,
            estimated_duration_secs,
            required_skills,
            collaboration_score,
            resource_needs,
            confidence: task_confidence,
            entities: HashMap::new(),
        })
    }

    fn analyser_type(&self) -> &'static str {
        "ConfigurableTaskAnalyser"
    }
}

pub struct SimpleTaskAnalyser {
    inner: ConfigurableTaskAnalyser,
}

impl Default for SimpleTaskAnalyser {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleTaskAnalyser {
    pub fn new() -> Self {
        let task_type_config = ClassificationConfig::new()
            .add_pattern(ClassificationPattern {
                name: "development".to_string(),
                keywords: vec!["code", "develop", "implement", "build", "program"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                confidence: 0.8,
                metadata: HashMap::new(),
            })
            .add_pattern(ClassificationPattern {
                name: "design".to_string(),
                keywords: vec!["design", "create", "ui", "ux", "interface"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                confidence: 0.8,
                metadata: HashMap::new(),
            })
            .add_pattern(ClassificationPattern {
                name: "analysis".to_string(),
                keywords: vec!["analyse", "research", "investigate", "study"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                confidence: 0.7,
                metadata: HashMap::new(),
            })
            .add_pattern(ClassificationPattern {
                name: "planning".to_string(),
                keywords: vec!["plan", "strategy", "organise", "schedule"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                confidence: 0.7,
                metadata: HashMap::new(),
            })
            .add_pattern(ClassificationPattern {
                name: "testing".to_string(),
                keywords: vec!["test", "verify", "validate", "qa"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                confidence: 0.8,
                metadata: HashMap::new(),
            })
            .add_pattern(ClassificationPattern {
                name: "technology".to_string(),
                keywords: vec![
                    "api",
                    "rest",
                    "framework",
                    "library",
                    "rust",
                    "axum",
                    "postgresql",
                    "database",
                    "tech",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                confidence: 0.8,
                metadata: HashMap::new(),
            });

        let mut resource_config = ResourceConfig::default();
        resource_config
            .base_duration_mapping
            .insert("development".to_string(), 7200);
        resource_config
            .base_duration_mapping
            .insert("design".to_string(), 5400);
        resource_config
            .base_duration_mapping
            .insert("analysis".to_string(), 3600);
        resource_config
            .base_duration_mapping
            .insert("planning".to_string(), 2700);

        resource_config.skill_mapping.insert(
            "development".to_string(),
            vec!["programming".to_string(), "problem-solving".to_string()],
        );
        resource_config.skill_mapping.insert(
            "design".to_string(),
            vec!["creativity".to_string(), "visual-design".to_string()],
        );
        resource_config.skill_mapping.insert(
            "analysis".to_string(),
            vec!["analytical-thinking".to_string(), "research".to_string()],
        );
        resource_config.skill_mapping.insert(
            "planning".to_string(),
            vec!["strategic-thinking".to_string(), "organisation".to_string()],
        );

        let inner =
            ConfigurableTaskAnalyser::new(task_type_config).with_resource_config(resource_config);

        Self { inner }
    }
}

impl TaskAnalyser for SimpleTaskAnalyser {
    fn analyse(&self, description: &str) -> TaskResult<TaskAnalysis> {
        self.inner.analyse(description)
    }

    fn analyser_type(&self) -> &'static str {
        "SimpleTaskAnalyser"
    }
}

pub struct NerReadyAnalyser {
    base_analyser: ConfigurableTaskAnalyser,
    ner_analyser: RwLock<NerAnalyser>,
}

impl NerReadyAnalyser {
    pub fn new(base_analyser: ConfigurableTaskAnalyser) -> Self {
        let ner_config = NerConfig::default();
        Self {
            base_analyser,
            ner_analyser: RwLock::new(NerAnalyser::new(ner_config)),
        }
    }

    pub fn with_ner_config(base_analyser: ConfigurableTaskAnalyser, ner_config: NerConfig) -> Self {
        Self {
            base_analyser,
            ner_analyser: RwLock::new(NerAnalyser::new(ner_config)),
        }
    }

    fn extract_entities(&self, description: &str) -> HashMap<String, Vec<String>> {
        let mut entities = HashMap::new();

        if let Ok(mut ner_analyser) = self.ner_analyser.write() {
            match ner_analyser.analyse_text(description) {
                Ok(ner_result) => {
                    for detected_entity in ner_result.entities {
                        entities
                            .entry(detected_entity.label.to_uppercase())
                            .or_insert_with(Vec::new)
                            .push(detected_entity.text);
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "NER analysis failed: {}. Using fallback entity extraction.",
                        e
                    );
                }
            }
        } else {
            tracing::warn!(
                "Could not acquire write lock for NER analyser. Using fallback entity extraction."
            );
        }

        let text_lower = description.to_lowercase();

        if text_lower.contains("rust") {
            entities
                .entry("TECHNOLOGY".to_string())
                .or_insert_with(Vec::new)
                .push("Rust".to_string());
        }
        if text_lower.contains("postgresql") || text_lower.contains("postgres") {
            entities
                .entry("DATABASE".to_string())
                .or_insert_with(Vec::new)
                .push("PostgreSQL".to_string());
        }
        if text_lower.contains("python") {
            entities
                .entry("TECHNOLOGY".to_string())
                .or_insert_with(Vec::new)
                .push("Python".to_string());
        }
        if text_lower.contains("api") {
            entities
                .entry("TECHNOLOGY".to_string())
                .or_insert_with(Vec::new)
                .push("API".to_string());
        }

        entities
    }
}

impl TaskAnalyser for NerReadyAnalyser {
    fn analyse(&self, description: &str) -> TaskResult<TaskAnalysis> {
        let mut analysis = self.base_analyser.analyse(description)?;

        analysis.entities = self.extract_entities(description);

        for entity_type in analysis.entities.keys() {
            let tag = entity_type.to_lowercase();
            analysis.tags.entry(tag).or_insert(0.8);
        }

        Ok(analysis)
    }

    fn analyser_type(&self) -> &'static str {
        "NerReadyAnalyser"
    }
}

pub struct CompositeAnalyser {
    primary: Box<dyn TaskAnalyser>,
    fallback: Box<dyn TaskAnalyser>,
}

impl CompositeAnalyser {
    pub fn new(primary: Box<dyn TaskAnalyser>, fallback: Box<dyn TaskAnalyser>) -> Self {
        Self { primary, fallback }
    }
}

impl TaskAnalyser for CompositeAnalyser {
    fn analyse(&self, description: &str) -> TaskResult<TaskAnalysis> {
        if self.primary.is_available() {
            match self.primary.analyse(description) {
                Ok(analysis) if analysis.confidence > 0.7 => Ok(analysis),
                _ => self.fallback.analyse(description),
            }
        } else {
            self.fallback.analyse(description)
        }
    }

    fn analyser_type(&self) -> &'static str {
        "CompositeAnalyser"
    }

    fn is_available(&self) -> bool {
        self.primary.is_available() || self.fallback.is_available()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_analyser_development_task() {
        let analyser = SimpleTaskAnalyser::new();
        let result = analyser
            .analyse("Build a REST API in Rust using Axum framework")
            .unwrap();

        println!("Tags: {:?}", result.tags);
        assert!(result.tags.contains_key("development"));
        assert!(result.tags.contains_key("technology"));
        assert!(result.required_skills.contains(&"programming".to_string()));
        assert!(result.complexity_score > 0.3);
    }

    #[test]
    fn test_ner_ready_analyser_rust_api_task() {
        let config = ClassificationConfig::default();
        let base_analyser = ConfigurableTaskAnalyser::new(config);
        let analyser = NerReadyAnalyser::new(base_analyser);

        let result = analyser
            .analyse("Create a high-performance REST API in Rust using PostgreSQL")
            .unwrap();

        assert!(result.tags.contains_key("technology"));
        assert!(result.entities.contains_key("TECHNOLOGY"));
        assert!(result.entities.contains_key("DATABASE"));
        assert!(result.complexity_score > 0.3);
    }

    #[test]
    fn test_task_analysis_methods() {
        let mut tags = HashMap::new();
        tags.insert("software_development".to_string(), 0.9);
        tags.insert("technology".to_string(), 0.8);
        tags.insert("rust_programming".to_string(), 0.7);

        let analysis = TaskAnalysis {
            tags,
            complexity_score: 0.8,
            estimated_duration_secs: 7200,
            required_skills: vec!["programming".to_string()],
            collaboration_score: 0.6,
            resource_needs: HashMap::new(),
            confidence: 0.85,
            entities: HashMap::new(),
        };

        assert_eq!(
            analysis.primary_task_type(),
            Some("software_development".to_string())
        );
        assert!(analysis.requires_collaboration(0.5));
        assert!(!analysis.requires_collaboration(0.7));

        let high_conf_tags = analysis.high_confidence_tags(0.75);
        assert_eq!(high_conf_tags.len(), 2);
    }
}
