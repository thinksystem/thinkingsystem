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

use super::data_structures::*;

pub type RulePredicate = dyn Fn(&ChartSpec, &AnalysisGoal) -> bool + Send + Sync;

pub struct SymbolicRule {
    pub name: String,
    pub condition: Box<RulePredicate>,
    pub score_adjustment: f64,
    pub feedback: String,
}
impl std::fmt::Debug for SymbolicRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SymbolicRule")
            .field("name", &self.name)
            .field("score_adjustment", &self.score_adjustment)
            .field("feedback", &self.feedback)
            .finish()
    }
}
#[derive(Debug)]
pub struct SymbolicEngine {
    rules: Vec<SymbolicRule>,
}
impl Default for SymbolicEngine {
    fn default() -> Self {
        let rules = vec![
            SymbolicRule {
                name: "Pie Chart High Cardinality".to_string(),
                condition: Box::new(|spec, _| {
                    if spec.chart_type != ChartType::Pie {
                        return false;
                    }
                    if let Some(colour_field) = &spec.colour_field {
                        spec.column_profiles
                            .get(colour_field)
                            .is_some_and(|p| p.cardinality.unwrap_or(0) > 7)
                    } else {
                        false
                    }
                }),
                score_adjustment: -0.5,
                feedback: "Pie charts become unreadable with more than 7 slices.".to_string(),
            },
            SymbolicRule {
                name: "Line Chart Invalid Y-Axis".to_string(),
                condition: Box::new(|spec, goal| {
                    if *goal != AnalysisGoal::ShowTrend || spec.chart_type != ChartType::Line {
                        return false;
                    }
                    spec.y_axis_fields.iter().any(|field| {
                        spec.column_profiles
                            .get(field)
                            .is_some_and(|p| p.data_type != DataType::Numeric)
                    })
                }),
                score_adjustment: -0.8,
                feedback: "Trend analysis requires a numeric Y-axis.".to_string(),
            },
            SymbolicRule {
                name: "Good Chart for Trend".to_string(),
                condition: Box::new(|spec, goal| {
                    *goal == AnalysisGoal::ShowTrend && spec.chart_type == ChartType::Line
                }),
                score_adjustment: 0.3,
                feedback: "Line charts are excellent for showing trends over time.".to_string(),
            },
            SymbolicRule {
                name: "Good Chart for Comparison".to_string(),
                condition: Box::new(|spec, goal| {
                    *goal == AnalysisGoal::Compare && spec.chart_type == ChartType::Bar
                }),
                score_adjustment: 0.3,
                feedback: "Bar charts are effective for comparing categories.".to_string(),
            },
        ];
        Self { rules }
    }
}
impl SymbolicEngine {
    pub fn evaluate(&self, spec: &ChartSpec, goal: &AnalysisGoal) -> (f64, Vec<String>) {
        self.rules
            .iter()
            .filter(|rule| (rule.condition)(spec, goal))
            .map(|rule| (rule.score_adjustment, rule.feedback.clone()))
            .fold(
                (0.0, Vec::new()),
                |(total_adj, mut feedbacks), (adj, feedback)| {
                    feedbacks.push(feedback);
                    (total_adj + adj, feedbacks)
                },
            )
    }
    pub fn add_rule(&mut self, rule: SymbolicRule) {
        self.rules.push(rule);
    }
}
