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

use crate::agents::schemas::{AgentCapabilities, PerformanceMetrics, TechnicalSkill};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct CapabilityMatcherConfig {
    pub preferred_skill_weight_multiplier: f64,

    pub domain_mismatch_penalty: f64,

    pub performance_penalty_multiplier: f64,
}

impl Default for CapabilityMatcherConfig {
    fn default() -> Self {
        Self {
            preferred_skill_weight_multiplier: 0.5,
            domain_mismatch_penalty: 0.5,
            performance_penalty_multiplier: 0.7,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CapabilityMatcher {
    pub required_skills: Vec<SkillRequirement>,
    pub preferred_skills: Vec<SkillRequirement>,
    pub performance_requirements: Option<PerformanceRequirements>,
    pub domain_requirements: Vec<String>,
    pub config: CapabilityMatcherConfig,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRequirement {
    pub skill_name: String,
    pub min_proficiency: f64,
    pub weight: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceRequirements {
    pub min_success_rate: Option<f64>,
    pub min_quality_score: Option<f64>,
    pub min_collaboration_score: Option<f64>,
    pub max_avg_completion_time: Option<f64>,
}
#[derive(Debug, Clone)]
pub struct CapabilityMatch {
    pub score: f64,
    pub required_skills_met: bool,
    pub skill_scores: HashMap<String, f64>,
    pub performance_match: PerformanceMatch,
    pub missing_skills: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct PerformanceMatch {
    pub success_rate_met: bool,
    pub quality_score_met: bool,
    pub collaboration_score_met: bool,
    pub completion_time_met: bool,
}

#[derive(Debug)]
struct SkillEvaluationResult {
    skill_scores: HashMap<String, f64>,
    total_score: f64,
    total_weight: f64,
    all_met: bool,
}
impl CapabilityMatcher {
    pub fn new() -> Self {
        Self {
            required_skills: Vec::new(),
            preferred_skills: Vec::new(),
            performance_requirements: None,
            domain_requirements: Vec::new(),
            config: CapabilityMatcherConfig::default(),
        }
    }

    pub fn with_config(mut self, config: CapabilityMatcherConfig) -> Self {
        self.config = config;
        self
    }
    pub fn require_skill(
        mut self,
        skill_name: impl Into<String>,
        min_proficiency: f64,
        weight: f64,
    ) -> Self {
        self.required_skills.push(SkillRequirement {
            skill_name: skill_name.into(),
            min_proficiency,
            weight,
        });
        self
    }
    pub fn prefer_skill(
        mut self,
        skill_name: impl Into<String>,
        min_proficiency: f64,
        weight: f64,
    ) -> Self {
        self.preferred_skills.push(SkillRequirement {
            skill_name: skill_name.into(),
            min_proficiency,
            weight,
        });
        self
    }
    pub fn with_performance_requirements(mut self, requirements: PerformanceRequirements) -> Self {
        self.performance_requirements = Some(requirements);
        self
    }
    pub fn require_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain_requirements.push(domain.into());
        self
    }
    pub fn evaluate_match(&self, capabilities: &AgentCapabilities) -> CapabilityMatch {
        let (required_results, missing_skills) =
            self.evaluate_required_skills(&capabilities.technical_skills);
        let preferred_results = self.evaluate_preferred_skills(&capabilities.technical_skills);

        let mut skill_scores = HashMap::new();
        skill_scores.extend(required_results.skill_scores);
        skill_scores.extend(preferred_results.skill_scores);

        let normalised_score = self.calculate_normalised_score(
            required_results.total_score,
            required_results.total_weight,
            preferred_results.total_score,
            preferred_results.total_weight,
        );

        let performance_match = self.evaluate_performance_match(&capabilities.performance_metrics);
        let final_score = self.apply_performance_penalty(normalised_score, &performance_match);

        CapabilityMatch {
            score: final_score.min(1.0),
            required_skills_met: required_results.all_met,
            skill_scores,
            performance_match,
            missing_skills,
        }
    }

    fn evaluate_required_skills(
        &self,
        skills: &[TechnicalSkill],
    ) -> (SkillEvaluationResult, Vec<String>) {
        let mut skill_scores = HashMap::new();
        let mut all_met = true;
        let mut missing_skills = Vec::new();
        let mut total_score = 0.0;
        let mut total_weight = 0.0;

        for req in &self.required_skills {
            let skill_score = self.evaluate_skill_match(req, skills);
            skill_scores.insert(req.skill_name.clone(), skill_score);

            if skill_score < req.min_proficiency {
                all_met = false;
                missing_skills.push(req.skill_name.clone());
            }

            total_score += skill_score * req.weight;
            total_weight += req.weight;
        }

        (
            SkillEvaluationResult {
                skill_scores,
                total_score,
                total_weight,
                all_met,
            },
            missing_skills,
        )
    }

    fn evaluate_preferred_skills(&self, skills: &[TechnicalSkill]) -> SkillEvaluationResult {
        let mut skill_scores = HashMap::new();
        let mut total_score = 0.0;
        let mut total_weight = 0.0;

        for pref in &self.preferred_skills {
            let skill_score = self.evaluate_skill_match(pref, skills);
            skill_scores.insert(pref.skill_name.clone(), skill_score);

            let weighted_score =
                skill_score * pref.weight * self.config.preferred_skill_weight_multiplier;
            let weighted_weight = pref.weight * self.config.preferred_skill_weight_multiplier;

            total_score += weighted_score;
            total_weight += weighted_weight;
        }

        SkillEvaluationResult {
            skill_scores,
            total_score,
            total_weight,
            all_met: true,
        }
    }

    fn calculate_normalised_score(
        &self,
        required_score: f64,
        required_weight: f64,
        preferred_score: f64,
        preferred_weight: f64,
    ) -> f64 {
        let total_score = required_score + preferred_score;
        let total_weight = required_weight + preferred_weight;

        if total_weight > 0.0 {
            total_score / total_weight
        } else {
            0.0
        }
    }

    fn apply_performance_penalty(&self, score: f64, performance_match: &PerformanceMatch) -> f64 {
        if self.performance_requirements.is_some()
            && !self.all_performance_requirements_met(performance_match)
        {
            score * self.config.performance_penalty_multiplier
        } else {
            score
        }
    }

    fn all_performance_requirements_met(&self, performance_match: &PerformanceMatch) -> bool {
        performance_match.success_rate_met
            && performance_match.quality_score_met
            && performance_match.collaboration_score_met
            && performance_match.completion_time_met
    }
    fn evaluate_skill_match(
        &self,
        requirement: &SkillRequirement,
        skills: &[TechnicalSkill],
    ) -> f64 {
        skills
            .iter()
            .find(|skill| skill.name == requirement.skill_name)
            .map(|skill| {
                let domain_match = self.calculate_domain_match_multiplier(skill);
                skill.proficiency * domain_match
            })
            .unwrap_or_default()
    }

    fn calculate_domain_match_multiplier(&self, skill: &TechnicalSkill) -> f64 {
        if self.domain_requirements.is_empty() {
            1.0
        } else {
            match self
                .domain_requirements
                .iter()
                .any(|req_domain| skill.domains.contains(req_domain))
            {
                true => 1.0,
                false => self.config.domain_mismatch_penalty,
            }
        }
    }
    fn evaluate_performance_match(&self, metrics: &PerformanceMetrics) -> PerformanceMatch {
        if let Some(req) = &self.performance_requirements {
            PerformanceMatch {
                success_rate_met: req
                    .min_success_rate
                    .map(|min| metrics.success_rate >= min)
                    .unwrap_or(true),
                quality_score_met: req
                    .min_quality_score
                    .map(|min| metrics.quality_score >= min)
                    .unwrap_or(true),
                collaboration_score_met: req
                    .min_collaboration_score
                    .map(|min| metrics.collaboration_score >= min)
                    .unwrap_or(true),
                completion_time_met: req
                    .max_avg_completion_time
                    .map(|max| metrics.avg_completion_time <= max)
                    .unwrap_or(true),
            }
        } else {
            PerformanceMatch {
                success_rate_met: true,
                quality_score_met: true,
                collaboration_score_met: true,
                completion_time_met: true,
            }
        }
    }
}
impl Default for CapabilityMatcher {
    fn default() -> Self {
        Self::new()
    }
}
