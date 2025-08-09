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

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserIntent {
    pub intent_type: IntentType,
    pub confidence: f64,
    pub context_requirements: Vec<ContextRequirement>,
    pub validation_rules: Vec<ValidationRule>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IntentType {
    DataExploration {
        exploration_type: ExplorationType,
        target_columns: Vec<String>,
        filters: Option<Vec<FilterCondition>>,
    },
    ChartGeneration {
        chart_preference: Option<String>,
        analysis_goal: AnalysisGoal,
        required_columns: Vec<String>,
        optional_columns: Vec<String>,
    },
    DataTransformation {
        operations: Vec<TransformationIntent>,
        preserve_original: bool,
    },
    Comparison {
        comparison_type: ComparisonType,
        entities: Vec<String>,
        metrics: Vec<String>,
    },
    TrendAnalysis {
        time_column: String,
        value_columns: Vec<String>,
        granularity: Option<TimeGranularity>,
    },
    Summarization {
        summary_type: SummaryType,
        group_by: Option<Vec<String>>,
        metrics: Vec<String>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExplorationType {
    Overview,
    Distribution,
    Outliers,
    MissingValues,
    Correlations,
    UniqueValues,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnalysisGoal {
    ShowTrend,
    Compare,
    ShowDistribution,
    FindRelationship,
    ShowComposition,
    Highlight,
    Summarise,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComparisonType {
    Temporal,
    Categorical,
    Geographical,
    Dimensional,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SummaryType {
    Statistical,
    Categorical,
    Temporal,
    TopN,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformationIntent {
    pub operation_type: TransformationOperationType,
    pub target_columns: Vec<String>,
    pub parameters: HashMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransformationOperationType {
    Filter,
    GroupBy,
    CreateRatio,
    CreatePercentage,
    Normalise,
    Bin,
    DateExtract,
    StringTransform,
    Calculate,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimeGranularity {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}
pub struct IntentProcessor {
    data_engine: DataEngine,
    transformation_engine: TransformationEngine,
    chart_matcher: crate::chart_matcher::ChartMatcher,
}
impl IntentProcessor {
    pub fn new(data_engine: DataEngine) -> Self {
        Self {
            data_engine,
            transformation_engine: TransformationEngine::new(),
            chart_matcher: crate::chart_matcher::ChartMatcher::new(),
        }
    }
    pub fn process_intent(&mut self, intent: UserIntent, dataset_id: &str) -> Result<IntentResult, IntentError> {
        self.validate_intent(&intent, dataset_id)?;
        match intent.intent_type {
            IntentType::DataExploration { exploration_type, target_columns, filters } => {
                self.handle_exploration_intent(exploration_type, target_columns, filters, dataset_id)
            },
            IntentType::ChartGeneration { chart_preference, analysis_goal, required_columns, optional_columns } => {
                self.handle_chart_generation_intent(chart_preference, analysis_goal, required_columns, optional_columns, dataset_id)
            },
            IntentType::DataTransformation { operations, preserve_original } => {
                self.handle_transformation_intent(operations, preserve_original, dataset_id)
            },
            IntentType::Comparison { comparison_type, entities, metrics } => {
                self.handle_comparison_intent(comparison_type, entities, metrics, dataset_id)
            },
            IntentType::TrendAnalysis { time_column, value_columns, granularity } => {
                self.handle_trend_analysis_intent(time_column, value_columns, granularity, dataset_id)
            },
            IntentType::Summarization { summary_type, group_by, metrics } => {
                self.handle_summarization_intent(summary_type, group_by, metrics, dataset_id)
            },
        }
    }
    fn handle_chart_generation_intent(
        &mut self,
        chart_preference: Option<String>,
        analysis_goal: AnalysisGoal,
        required_columns: Vec<String>,
        optional_columns: Vec<String>,
        dataset_id: &str
    ) -> Result<IntentResult, IntentError> {
        let dataset = self.data_engine.get_dataset(dataset_id)?;
        let matching_config = self.build_intent_based_config(&analysis_goal, &chart_preference);
        let relevant_profiles: Vec<_> = dataset.profiles.iter()
            .filter(|p| required_columns.contains(&p.name) || optional_columns.contains(&p.name))
            .cloned()
            .collect();
        let suggestions = self.chart_matcher.find_qualified_charts_with_context(
            &relevant_profiles,
            &matching_config,
            &ChartContext {
                analysis_goal: analysis_goal.clone(),
                preferred_chart: chart_preference.clone(),
                semantic_context: dataset.semantic_context.clone(),
            }
        )?;
        let ranked_suggestions = self.rank_by_intent_alignment(suggestions, &analysis_goal)?;
        Ok(IntentResult::ChartSuggestions {
            suggestions: ranked_suggestions,
            reasoning: self.explain_chart_choices(&analysis_goal, &required_columns),
            alternative_approaches: self.suggest_alternatives(&analysis_goal, &dataset.profiles),
        })
    }
    fn handle_transformation_intent(
        &mut self,
        operations: Vec<TransformationIntent>,
        preserve_original: bool,
        dataset_id: &str
    ) -> Result<IntentResult, IntentError> {
        let dataset = self.data_engine.get_dataset(dataset_id)?;
        let mut current_dataset = dataset.clone();
        let mut applied_transformations = Vec::new();
        for operation_intent in operations {
            let transformation_step = self.convert_intent_to_transformation(&operation_intent, &current_dataset)?;
            current_dataset = self.transformation_engine.apply_transformation(&current_dataset, &transformation_step)?;
            applied_transformations.push(transformation_step);
        }
        let new_dataset_id = self.data_engine.add_dataset(current_dataset)?;
        let auto_suggestions = self.suggest_charts_for_transformed_data(&new_dataset_id)?;
        Ok(IntentResult::TransformationComplete {
            new_dataset_id,
            transformations_applied: applied_transformations,
            suggested_charts: auto_suggestions,
            data_summary: self.summarise_transformation_impact(&applied_transformations),
        })
    }
    fn handle_trend_analysis_intent(
        &mut self,
        time_column: String,
        value_columns: Vec<String>,
        granularity: Option<TimeGranularity>,
        dataset_id: &str
    ) -> Result<IntentResult, IntentError> {
        let dataset = self.data_engine.get_dataset(dataset_id)?;
        let time_profile = dataset.profiles.iter()
            .find(|p| p.name == time_column)
            .ok_or(IntentError::ColumnNotFound(time_column.clone()))?;
        if !matches!(time_profile.data_type, crate::api_graph::DataType::Temporal) {
            return Err(IntentError::InvalidColumnType {
                column: time_column,
                expected: "Temporal".to_string(),
                actual: format!("{:?}", time_profile.data_type),
            });
        }
        let processed_dataset = if let Some(grain) = granularity {
            let time_transform = self.create_time_granularity_transformation(&time_column, grain)?;
            self.transformation_engine.apply_transformation(dataset, &time_transform)?
        } else {
            dataset.clone()
        };
        let trend_charts = self.generate_trend_charts(&processed_dataset, &time_column, &value_columns)?;
        Ok(IntentResult::TrendAnalysis {
            charts: trend_charts,
            insights: self.extract_trend_insights(&processed_dataset, &time_column, &value_columns)?,
            recommendations: self.suggest_trend_enhancements(&value_columns),
        })
    }
    fn convert_intent_to_transformation(
        &self,
        intent: &TransformationIntent,
        dataset: &Dataset
    ) -> Result<TransformationStep, IntentError> {
        match intent.operation_type {
            TransformationOperationType::Filter => {
                let conditions = self.parse_filter_parameters(&intent.parameters, dataset)?;
                Ok(TransformationStep::Filter(FilterOperation {
                    conditions,
                    combinator: LogicalCombinator::And,
                }))
            },
            TransformationOperationType::GroupBy => {
                let group_columns = intent.target_columns.clone();
                let agg_columns = self.infer_aggregation_columns(dataset, &group_columns)?;
                Ok(TransformationStep::Aggregate(AggregateOperation {
                    group_by: group_columns,
                    aggregations: agg_columns,
                }))
            },
            TransformationOperationType::CreateRatio => {
                if intent.target_columns.len() != 2 {
                    return Err(IntentError::InvalidParameters("Ratio requires exactly 2 columns".to_string()));
                }
                let ratio_name = intent.parameters.get("name")
                    .cloned()
                    .unwrap_or_else(|| format!("{}_to_{}_ratio", intent.target_columns[0], intent.target_columns[1]));
                Ok(TransformationStep::CreateColumn(CreateColumnOperation {
                    name: ratio_name,
                    expression: ColumnExpression::Arithmetic {
                        left: Box::new(ColumnExpression::Column(intent.target_columns[0].clone())),
                        operator: ArithmeticOperator::Divide,
                        right: Box::new(ColumnExpression::Column(intent.target_columns[1].clone())),
                    },
                }))
            },
            TransformationOperationType::CreatePercentage => {
                if intent.target_columns.len() != 1 {
                    return Err(IntentError::InvalidParameters("Percentage requires exactly 1 column".to_string()));
                }
                let column = &intent.target_columns[0];
                let pct_name = format!("{column}_percentage");
                Ok(TransformationStep::CreateColumn(CreateColumnOperation {
                    name: pct_name,
                    expression: ColumnExpression::Arithmetic {
                        left: Box::new(ColumnExpression::Arithmetic {
                            left: Box::new(ColumnExpression::Column(column.clone())),
                            operator: ArithmeticOperator::Divide,
                            right: Box::new(ColumnExpression::Function {
                                name: "sum".to_string(),
                                args: vec![ColumnExpression::Column(column.clone())],
                            }),
                        }),
                        operator: ArithmeticOperator::Multiply,
                        right: Box::new(ColumnExpression::Literal(FilterValue::Number(100.0))),
                    },
                }))
            },
            TransformationOperationType::DateExtract => {
                let date_part = intent.parameters.get("part")
                    .ok_or_else(|| IntentError::InvalidParameters("Date extraction requires 'part' parameter".to_string()))?;
                let new_column_name = format!("{}_{}", intent.target_columns[0], date_part);
                Ok(TransformationStep::CreateColumn(CreateColumnOperation {
                    name: new_column_name,
                    expression: ColumnExpression::Function {
                        name: format!("extract_{date_part}"),
                        args: vec![ColumnExpression::Column(intent.target_columns[0].clone())],
                    },
                }))
            },
            _ => Err(IntentError::UnsupportedOperation(format!("{:?}", intent.operation_type))),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IntentResult {
    ChartSuggestions {
        suggestions: Vec<crate::chart_matcher::RenderSpec>,
        reasoning: String,
        alternative_approaches: Vec<AlternativeApproach>,
    },
    TransformationComplete {
        new_dataset_id: String,
        transformations_applied: Vec<TransformationStep>,
        suggested_charts: Vec<crate::chart_matcher::RenderSpec>,
        data_summary: TransformationSummary,
    },
    TrendAnalysis {
        charts: Vec<crate::chart_matcher::RenderSpec>,
        insights: Vec<TrendInsight>,
        recommendations: Vec<String>,
    },
    DataExploration {
        summary: ExplorationSummary,
        interesting_findings: Vec<Finding>,
        suggested_next_steps: Vec<String>,
    },
    ComparisonResult {
        charts: Vec<crate::chart_matcher::RenderSpec>,
        key_differences: Vec<ComparisonInsight>,
        statistical_significance: Option<StatisticalTest>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativeApproach {
    pub description: String,
    pub chart_type: String,
    pub rationale: String,
    pub complexity_level: ComplexityLevel,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplexityLevel {
    Beginner,
    Intermediate,
    Advanced,
}
