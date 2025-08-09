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

use crate::messaging::insight::{
    analysis::ContentAnalysis,
    config::ScoringConfig,
    hybrid_analyser::{HybridConfig, HybridContentAnalyser},
    ner_analysis::{DetectedEntity, NerAnalysisResult, NerConfig},
};

#[test]
fn test_hybrid_analyser_creation() {
    let analyser = HybridContentAnalyser::default();
    assert!(analyser.get_hybrid_config().syntactic_weight > 0.0);
    assert!(analyser.get_hybrid_config().ner_weight > 0.0);
}

#[test]
fn test_combine_analysis_results() {
    let analyser = HybridContentAnalyser::default();

    let syntactic = ContentAnalysis {
        overall_risk_score: 0.6,
        interesting_tokens: vec![("test@example.com".to_string(), 0.6)],
        requires_scribes_review: true,
    };

    let ner = NerAnalysisResult {
        entities: vec![DetectedEntity {
            text: "john@example.com".to_string(),
            label: "email".to_string(),
            start: 0,
            end: 16,
            confidence: 0.9,
            risk_score: 0.8,
        }],
        overall_ner_score: 0.8,
        processing_time_ms: 50.0,
        text_truncated: false,
    };

    let (combined_score, needs_review, method) =
        analyser.combine_analysis_results_test(&syntactic, &ner);

    assert!(combined_score > 0.0);
    assert!(needs_review);
    assert!(method.contains("Hybrid"));
}

#[test]
fn test_ner_boost_activation() {
    let config = HybridConfig {
        ner_boost_threshold: 0.7,
        enable_ner_boost: true,
        ..Default::default()
    };

    let analyser =
        HybridContentAnalyser::new(ScoringConfig::default(), NerConfig::default(), config);

    let syntactic = ContentAnalysis {
        overall_risk_score: 0.3,
        interesting_tokens: vec![],
        requires_scribes_review: false,
    };

    let ner = NerAnalysisResult {
        entities: vec![DetectedEntity {
            text: "4111111111111111".to_string(),
            label: "credit_card".to_string(),
            start: 0,
            end: 16,
            confidence: 0.95,
            risk_score: 0.9,
        }],
        overall_ner_score: 0.9,
        processing_time_ms: 50.0,
        text_truncated: false,
    };

    let (combined_score, needs_review, method) =
        analyser.combine_analysis_results_test(&syntactic, &ner);

    assert!(combined_score > 0.5);
    assert!(needs_review);
    assert!(method.contains("NER-boosted"));
}

#[test]
fn test_high_risk_ner_entities() {
    let analyser = HybridContentAnalyser::default();

    let ner_with_high_risk = NerAnalysisResult {
        entities: vec![DetectedEntity {
            text: "123-45-6789".to_string(),
            label: "ssn".to_string(),
            start: 0,
            end: 11,
            confidence: 0.9,
            risk_score: 0.9,
        }],
        overall_ner_score: 0.9,
        processing_time_ms: 50.0,
        text_truncated: false,
    };

    assert!(analyser.has_high_risk_ner_entities_test(&ner_with_high_risk));

    let ner_with_low_risk = NerAnalysisResult {
        entities: vec![DetectedEntity {
            text: "New York".to_string(),
            label: "location".to_string(),
            start: 0,
            end: 8,
            confidence: 0.8,
            risk_score: 0.4,
        }],
        overall_ner_score: 0.4,
        processing_time_ms: 50.0,
        text_truncated: false,
    };

    assert!(!analyser.has_high_risk_ner_entities_test(&ner_with_low_risk));
}

#[test]
fn test_prioritised_entities() {
    let analyser = HybridContentAnalyser::default();

    let analysis = crate::messaging::insight::hybrid_analyser::HybridContentAnalysis {
        syntactic_analysis: ContentAnalysis {
            overall_risk_score: 0.6,
            interesting_tokens: vec![("suspicious123".to_string(), 0.6)],
            requires_scribes_review: true,
        },
        ner_analysis: NerAnalysisResult {
            entities: vec![DetectedEntity {
                text: "john@example.com".to_string(),
                label: "email".to_string(),
                start: 10,
                end: 26,
                confidence: 0.9,
                risk_score: 0.8,
            }],
            overall_ner_score: 0.8,
            processing_time_ms: 50.0,
            text_truncated: false,
        },
        combined_risk_score: 0.7,
        requires_scribes_review: true,
        analysis_method: "Hybrid".to_string(),
    };

    let entities = analyser.get_prioritised_entities(&analysis);
    assert_eq!(entities.len(), 2);

    assert!(entities[0].risk_score >= entities[1].risk_score);

    let sources: Vec<&str> = entities.iter().map(|e| e.source.as_str()).collect();
    assert!(sources.contains(&"Syntactic"));
    assert!(sources.contains(&"NER"));
}
