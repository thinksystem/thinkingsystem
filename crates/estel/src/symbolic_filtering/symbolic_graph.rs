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

use super::data_structures::{ChartType};
use super::field_analysis::FieldEdge;

#[derive(Debug, Clone)]
pub struct FieldRelationshipAnalysis {
    pub field_pair: (String, String),
    pub relationship: FieldEdge,
    pub visualisation_impact: VisualisationImpact,
}
#[derive(Debug, Clone)]
pub struct VisualisationImpact {
    pub effectiveness_score: f64,
    pub potential_issues: Vec<String>,
    pub optimisation_opportunities: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct OptimisationSuggestion {
    pub suggestion_type: SuggestionType,
    pub current_chart: ChartType,
    pub suggested_chart: ChartType,
    pub confidence: f64,
    pub reasoning: String,
    pub expected_improvement: f64,
}
#[derive(Debug, Clone)]
pub enum SuggestionType {
    ChartTypeChange,
    FieldRoleChange,
    AdditionalField,
    RemoveField,
}
