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

use crate::intent_system::{UserIntent, IntentResult, IntentProcessor};
use crate::data_engine::DataEngine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMDataAPI {
    data_engine: DataEngine,
    intent_processor: IntentProcessor,
    session_state: HashMap<String, SessionContext>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    pub session_id: String,
    pub active_dataset_id: Option<String>,
    pub interaction_history: Vec<InteractionRecord>,
    pub user_preferences: UserPreferences,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionRecord {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub user_input: String,
    pub intent: Option<UserIntent>,
    pub result: Option<IntentResult>,
    pub success: bool,
    pub error_message: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub preferred_chart_types: Vec<String>,
    pub complexity_preference: ComplexityLevel,
    pub explanation_level: ExplanationLevel,
    pub auto_suggest_charts: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExplanationLevel {
    Minimal,
    Basic,
    Detailed,
    Expert,
}
impl LLMDataAPI {
    pub fn new() -> Self {
        let data_engine = DataEngine::new();
        let intent_processor = IntentProcessor::new(data_engine.clone());
        Self {
            data_engine,
            intent_processor,
            session_state: HashMap::new(),
        }
    }
    pub fn execute_intent(
        &mut self,
        session_id: String,
        intent: UserIntent,
        raw_user_input: String,
    ) -> Result<IntentExecutionResult, APIError> {
        let mut session = self.session_state.get(&session_id)
            .cloned()
            .unwrap_or_else(|| SessionContext {
                session_id: session_id.clone(),
                active_dataset_id: None,
                interaction_history: Vec::new(),
                user_preferences: UserPreferences::default(),
            });
        let dataset_id = match &intent.intent_type {
            crate::intent_system::IntentType::DataExploration { .. } |
            crate::intent_system::IntentType::ChartGeneration { .. } |
            crate::intent_system::IntentType::DataTransformation { .. } |
            crate::intent_system::IntentType::Comparison { .. } |
            crate::intent_system::IntentType::TrendAnalysis { .. } |
            crate::intent_system::IntentType::Summarization { .. } => {
                session.active_dataset_id.as_ref()
                    .ok_or(APIError::NoActiveDataset)?
                    .clone()
            }
        };
        let start_time = std::time::Instant::now();
        let result = self.intent_processor.process_intent(intent.clone(), &dataset_id);
        let execution_time = start_time.elapsed();
        let interaction = InteractionRecord {
            timestamp: chrono::Utc::now(),
            user_input: raw_user_input,
            intent: Some(intent.clone()),
            result: result.as_ref().ok().cloned(),
            success: result.is_ok(),
            error_message: result.as_ref().err().map(|e| e.to_string()),
        };
        session.interaction_history.push(interaction);
        self.session_state.insert(session_id, session);
        match result {
            Ok(intent_result) => Ok(IntentExecutionResult {
                success: true,
                result: Some(intent_result),
                execution_time_ms: execution_time.as_millis() as u64,
                session_updates: self.generate_session_updates(&intent, &dataset_id)?,
                follow_up_suggestions: self.generate_follow_up_suggestions(&intent)?,
                error: None,
            }),
            Err(e) => Ok(IntentExecutionResult {
                success: false,
                result: None,
                execution_time_ms: execution_time.as_millis() as u64,
                session_updates: vec![],
                follow_up_suggestions: self.generate_error_recovery_suggestions(&e)?,
                error: Some(e.to_string()),
            }),
        }
    }
    pub fn ingest_data_from_path(
        &mut self,
        session_id: String,
        file_path: std::path::PathBuf,
        dataset_name: Option<String>,
    ) -> Result<DataIngestionResult, APIError> {
        let name = dataset_name.unwrap_or_else(|| {
            file_path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("dataset")
                .to_string()
        });
        let dataset_id = self.data_engine.ingest_csv(&file_path, name)?;
        let mut session = self.session_state.get(&session_id)
            .cloned()
            .unwrap_or_else(|| SessionContext {
                session_id: session_id.clone(),
                active_dataset_id: None,
                interaction_history: Vec::new(),
                user_preferences: UserPreferences::default(),
            });
        session.active_dataset_id = Some(dataset_id.clone());
        self.session_state.insert(session_id, session);
        let dataset = self.data_engine.get_dataset(&dataset_id)?;
        let initial_charts = self.generate_overview_charts(&dataset)?;
        let data_insights = self.extract_initial_insights(&dataset)?;
        Ok(DataIngestionResult {
            dataset_id,
            dataset_summary: dataset.summary.clone(),
            semantic_context: dataset.semantic_context.clone(),
            initial_chart_suggestions: initial_charts,
            data_insights,
            recommended_explorations: self.suggest_initial_explorations(&dataset)?,
        })
    }
    pub fn get_data_context(&self, session_id: &str) -> Result<DataContext, APIError> {
        let session = self.session_state.get(session_id)
            .ok_or(APIError::SessionNotFound)?;
        let dataset_id = session.active_dataset_id.as_ref()
            .ok_or(APIError::NoActiveDataset)?;
        let dataset = self.data_engine.get_dataset(dataset_id)?;
        Ok(DataContext {
            dataset_name: dataset.name.clone(),
            column_names: dataset.profiles.iter().map(|p| p.name.clone()).collect(),
            column_types: dataset.profiles.iter()
                .map(|p| (p.name.clone(), format!("{:?}", p.data_type)))
                .collect(),
            row_count: dataset.data.len(),
            semantic_context: dataset.semantic_context.clone(),
            data_quality_summary: self.summarise_data_quality(&dataset.profiles),
            sample_values: self.get_sample_values(&dataset, 3),
            recent_transformations: dataset.transformations.iter()
                .rev()
                .take(5)
                .cloned()
                .collect(),
            interaction_history: session.interaction_history.iter()
                .rev()
                .take(10)
                .cloned()
                .collect(),
        })
    }
    pub fn validate_intent_feasibility(
        &self,
        session_id: &str,
        intent: &UserIntent,
    ) -> Result<IntentValidationResult, APIError> {
        let session = self.session_state.get(session_id)
            .ok_or(APIError::SessionNotFound)?;
        let dataset_id = session.active_dataset_id.as_ref()
            .ok_or(APIError::NoActiveDataset)?;
        let dataset = self.data_engine.get_dataset(dataset_id)?;
        let mut validation_result = IntentValidationResult {
            is_valid: true,
            issues: Vec::new(),
            suggestions: Vec::new(),
            alternative_intents: Vec::new(),
        };
        match &intent.intent_type {
            crate::intent_system::IntentType::ChartGeneration { required_columns, .. } => {
                for col in required_columns {
                    if !dataset.profiles.iter().any(|p| &p.name == col) {
                        validation_result.is_valid = false;
                        validation_result.issues.push(ValidationIssue {
                            severity: IssueSeverity::Error,
                            message: format!("Column '{col}' not found in dataset"),
                            suggested_fix: Some(format!("Available columns: {}",
                                dataset.profiles.iter()
                                    .map(|p| &p.name)
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )),
                        });
                    }
                }
                for col in required_columns {
                    if let Some(profile) = dataset.profiles.iter().find(|p| &p.name == col) {
                        if profile.null_percentage > 0.5 {
                            validation_result.issues.push(ValidationIssue {
                                severity: IssueSeverity::Warning,
                                message: format!("Column '{}' has {:.1}% missing values", col, profile.null_percentage * 100.0),
                                suggested_fix: Some("Consider filtering out missing values or using imputation".to_string()),
                            });
                        }
                    }
                }
            },
            crate::intent_system::IntentType::TrendAnalysis { time_column, value_columns, .. } => {
                if let Some(time_profile) = dataset.profiles.iter().find(|p| &p.name == time_column) {
                    if !matches!(time_profile.data_type, crate::api_graph::DataType::Temporal) {
                        validation_result.is_valid = false;
                        validation_result.issues.push(ValidationIssue {
                            severity: IssueSeverity::Error,
                            message: format!("Column '{time_column}' is not a time/date column"),
                            suggested_fix: Some("Use a temporal column for trend analysis".to_string()),
                        });
                    }
                } else {
                    validation_result.is_valid = false;
                    validation_result.issues.push(ValidationIssue {
                        severity: IssueSeverity::Error,
                        message: format!("Time column '{time_column}' not found"),
                        suggested_fix: None,
                    });
                }
                for col in value_columns {
                    if let Some(profile) = dataset.profiles.iter().find(|p| &p.name == col) {
                        if !matches!(profile.data_type, crate::api_graph::DataType::Numeric) {
                            validation_result.issues.push(ValidationIssue {
                                severity: IssueSeverity::Warning,
                                message: format!("Column '{col}' is not numeric - trend analysis may not be meaningful"),
                                suggested_fix: Some("Consider using numeric columns for trend analysis".to_string()),
                            });
                        }
                    }
                }
            },
            _ => {
            }
        }
        Ok(validation_result)
    }
    pub fn get_contextual_suggestions(&self, session_id: &str) -> Result<Vec<ContextualSuggestion>, APIError> {
        let session = self.session_state.get(session_id)
            .ok_or(APIError::SessionNotFound)?;
        let dataset_id = session.active_dataset_id.as_ref()
            .ok_or(APIError::NoActiveDataset)?;
        let dataset = self.data_engine.get_dataset(dataset_id)?;
        let mut suggestions = Vec::new();
        let numeric_cols: Vec<_> = dataset.profiles.iter()
            .filter(|p| matches!(p.data_type, crate::api_graph::DataType::Numeric))
            .collect();
        let categorical_cols: Vec<_> = dataset.profiles.iter()
            .filter(|p| matches!(p.data_type, crate::api_graph::DataType::Categorical))
            .collect();
        let temporal_cols: Vec<_> = dataset.profiles.iter()
            .filter(|p| matches!(p.data_type, crate::api_graph::DataType::Temporal))
            .collect();
        if !temporal_cols.is_empty() && !numeric_cols.is_empty() {
            suggestions.push(ContextualSuggestion {
                category: SuggestionCategory::TrendAnalysis,
                title: "Analyse trends over time".to_string(),
                description: format!("Show how {} changes over {}",
                    numeric_cols.iter().take(2).map(|p| &p.name).collect::<Vec<_>>().join(", "),
                    temporal_cols[0].name
                ),
                example_query: format!("Show me the trend of {} over time", numeric_cols[0].name),
                complexity: ComplexityLevel::Beginner,
                estimated_chart_types: vec!["line".to_string(), "area".to_string()],
            });
        }
        if categorical_cols.len() >= 1 && numeric_cols.len() >= 1 {
            suggestions.push(ContextualSuggestion {
                category: SuggestionCategory::Comparison,
                title: "Compare across categories".to_string(),
                description: format!("Compare {} by {}", numeric_cols[0].name, categorical_cols[0].name),
                example_query: format!("Compare {} across different {}", numeric_cols[0].name, categorical_cols[0].name),
                complexity: ComplexityLevel::Beginner,
                estimated_chart_types: vec!["bar".to_string(), "box".to_string()],
            });
        }
        if numeric_cols.len() >= 2 {
            suggestions.push(ContextualSuggestion {
                category: SuggestionCategory::Relationship,
                title: "Explore relationships between metrics".to_string(),
                description: format!("See how {} relates to {}", numeric_cols[0].name, numeric_cols[1].name),
                example_query: format!("Show the relationship between {} and {}", numeric_cols[0].name, numeric_cols[1].name),
                complexity: ComplexityLevel::Intermediate,
                estimated_chart_types: vec!["scatter".to_string(), "hexbin".to_string()],
            });
        }
        if !numeric_cols.is_empty() {
            suggestions.push(ContextualSuggestion {
                category: SuggestionCategory::Distribution,
                title: "Analyse data distribution".to_string(),
                description: format!("Understand the spread and distribution of {}", numeric_cols[0].name),
                example_query: format!("Show me the distribution of {}", numeric_cols[0].name),
                complexity: ComplexityLevel::Beginner,
                estimated_chart_types: vec!["histogram".to_string(), "box".to_string()],
            });
        }
        if dataset.transformations.is_empty() {
            suggestions.push(ContextualSuggestion {
                category: SuggestionCategory::DataTransformation,
                title: "Transform your data".to_string(),
                description: "Create calculated fields, filter data, or aggregate by categories".to_string(),
                example_query: "Create a ratio between revenue and cost".to_string(),
                complexity: ComplexityLevel::Intermediate,
                estimated_chart_types: vec![],
            });
        }
        Ok(suggestions)
    }
    pub fn get_session_summary(&self, session_id: &str) -> Result<SessionSummary, APIError> {
        let session = self.session_state.get(session_id)
            .ok_or(APIError::SessionNotFound)?;
        let dataset_summary = if let Some(dataset_id) = &session.active_dataset_id {
            let dataset = self.data_engine.get_dataset(dataset_id)?;
            Some(DatasetSummaryForLLM {
                name: dataset.name.clone(),
                rows: dataset.data.len(),
                columns: dataset.profiles.len(),
                column_summary: dataset.profiles.iter().map(|p| ColumnSummaryForLLM {
                    name: p.name.clone(),
                    data_type: format!("{:?}", p.data_type),
                    quality_score: p.quality_score,
                    has_missing_values: p.null_percentage > 0.0,
                    unique_values: p.cardinality,
                }).collect(),
                semantic_tags: dataset.semantic_tags.clone(),
                transformations_applied: dataset.transformations.len(),
            })
        } else {
            None
        };
        Ok(SessionSummary {
            session_id: session.session_id.clone(),
            interactions_count: session.interaction_history.len(),
            successful_interactions: session.interaction_history.iter()
                .filter(|i| i.success)
                .count(),
            dataset_summary,
            user_preferences: session.user_preferences.clone(),
            last_interaction: session.interaction_history.last().cloned(),
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentExecutionResult {
    pub success: bool,
    pub result: Option<IntentResult>,
    pub execution_time_ms: u64,
    pub session_updates: Vec<SessionUpdate>,
    pub follow_up_suggestions: Vec<String>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataIngestionResult {
    pub dataset_id: String,
    pub dataset_summary: crate::data_profiler::DatasetSummary,
    pub semantic_context: Option<crate::data_engine::SemanticContext>,
    pub initial_chart_suggestions: Vec<crate::chart_matcher::RenderSpec>,
    pub data_insights: Vec<DataInsight>,
    pub recommended_explorations: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataContext {
    pub dataset_name: String,
    pub column_names: Vec<String>,
    pub column_types: HashMap<String, String>,
    pub row_count: usize,
    pub semantic_context: Option<crate::data_engine::SemanticContext>,
    pub data_quality_summary: DataQualitySummary,
    pub sample_values: HashMap<String, Vec<String>>,
    pub recent_transformations: Vec<crate::data_transformations::TransformationStep>,
    pub interaction_history: Vec<InteractionRecord>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextualSuggestion {
    pub category: SuggestionCategory,
    pub title: String,
    pub description: String,
    pub example_query: String,
    pub complexity: ComplexityLevel,
    pub estimated_chart_types: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuggestionCategory {
    TrendAnalysis,
    Comparison,
    Relationship,
    Distribution,
    DataTransformation,
    Statistical,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentValidationResult {
    pub is_valid: bool,
    pub issues: Vec<ValidationIssue>,
    pub suggestions: Vec<String>,
    pub alternative_intents: Vec<UserIntent>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: IssueSeverity,
    pub message: String,
    pub suggested_fix: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IssueSeverity {
    Error,
    Warning,
    Info,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionUpdate {
    DatasetChanged(String),
    NewChartGenerated(crate::chart_matcher::RenderSpec),
    TransformationApplied(crate::data_transformations::TransformationStep),
    PreferencesUpdated(UserPreferences),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum APIError {
    SessionNotFound,
    NoActiveDataset,
    DatasetNotFound(String),
    InvalidIntent(String),
    TransformationFailed(String),
    ValidationFailed(String),
    Internal(String),
}
impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            preferred_chart_types: vec!["line".to_string(), "bar".to_string(), "scatter".to_string()],
            complexity_preference: ComplexityLevel::Beginner,
            explanation_level: ExplanationLevel::Basic,
            auto_suggest_charts: true,
        }
    }
}
