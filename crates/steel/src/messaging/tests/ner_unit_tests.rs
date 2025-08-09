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

use crate::messaging::insight::ner_analysis::{NerAnalyser, NerConfig};

#[test]
fn test_ner_config_default() {
    let config = NerConfig::default();
    assert!(config.enabled);
    assert!(!config.entity_labels.is_empty());
    assert!(config.min_confidence_threshold > 0.0);
    assert!(!config.entity_weights.is_empty());
}

#[test]
fn test_ner_analyser_creation() {
    let analyser = NerAnalyser::default();
    assert!(analyser.get_config().enabled);
    assert!(!analyser.is_model_loaded());
}

#[test]
fn test_disabled_ner_analysis() {
    let config = NerConfig {
        enabled: false,
        ..Default::default()
    };
    let mut analyser = NerAnalyser::new(config);

    let result = analyser.analyse_text("John Doe lives in New York").unwrap();
    assert!(result.entities.is_empty());
    assert_eq!(result.overall_ner_score, 0.0);
}

#[test]
fn test_empty_text_analysis() {
    let mut analyser = NerAnalyser::default();
    let result = analyser.analyse_text("").unwrap();
    assert!(result.entities.is_empty());
    assert_eq!(result.overall_ner_score, 0.0);
}

#[test]
fn test_text_truncation() {
    let config = NerConfig {
        max_text_length: 10,
        enabled: true,
        ..Default::default()
    };
    let mut analyser = NerAnalyser::new(config);

    let long_text = "This is a very long text that should be truncated";
    let result = analyser.analyse_text(long_text).unwrap();

    assert!(result.text_truncated);
}

#[test]
fn test_risk_score_calculation() {
    let analyser = NerAnalyser::default();

    let score1 = analyser.calculate_entity_risk_score_test("credit_card", 0.9);
    assert!(score1 > 0.8);

    let score2 = analyser.calculate_entity_risk_score_test("date", 0.9);
    assert!(score2 < score1);

    let score3 = analyser.calculate_entity_risk_score_test("unknown", 0.8);
    assert!(score3 > 0.0 && score3 < 0.5);
}
