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

use crate::messaging::insight::*;
use std::fs;

#[test]
fn test_content_analysis_with_state() {
    let state_dir = tempfile::tempdir().unwrap();
    let mut security = MessageSecurity::new(state_dir.path());
    println!("\n=== Stateful Content Analysis Flow ===");

    let messages = [
        "Hello this is a normal message",
        "Please check transaction urn:uuid:f47ac10b-58cc-4372-a567-0e02b2c3d479",
        "My email is test@example.com, please get in touch.",
        "Another normal message for the system.",
        "URGENT contact security at sec-alert@mycorp.com immediately",
    ];

    for msg in &messages {
        let analysis = security.assess_message_risk(msg);
        let threshold = security.get_current_review_threshold();
        println!(
            "Msg: \"{}\"\n  -> Score: {:.3}, Review: {}, Current Threshold: {:.3}",
            msg,
            analysis.overall_risk_score(),
            analysis.requires_scribes_review(),
            threshold
        );
    }

    let state_file_path = state_dir.path().join("score_distribution.yml");
    assert!(state_file_path.exists(), "State file was not created.");
    let yaml_content = fs::read_to_string(&state_file_path).unwrap();
    println!("\n--- Generated score_distribution.yml ---\n{yaml_content}");
    assert!(yaml_content.contains("score_history:"));

    let new_security = MessageSecurity::new(state_dir.path());
    assert_eq!(
        new_security.distribution.score_history.len(),
        messages.len(),
        "New service should load the history from the YAML file."
    );
    println!(
        "\nSuccessfully loaded state into new service. History size: {}",
        new_security.distribution.score_history.len()
    );
}

#[test]
fn test_percentile_threshold_system() {
    println!("\n=== Percentile Threshold System Test ===");

    let config = ScoringConfig {
        scribes_review_percentile: 0.8,
        absolute_threshold_override: 0.9,
        ..Default::default()
    };

    let analyser = ContentAnalyser::new(config);

    let mut distribution = ScoreDistribution::default();

    let test_messages = [
        ("Hello world", 0.0),
        ("Phone 123-456-7890", 0.9),
        ("user@example.com", 0.7),
        ("Transaction: abc123-def456-ghi789", 0.6),
        ("Card: 1234567890123456", 1.0),
        ("Normal message text", 0.0),
    ];

    println!("\nBuilding score distribution...");
    for (message, score) in &test_messages {
        distribution.add_score(*score);
        let review = analyser.requires_scribes_review_test(*score, &mut distribution);
        println!("Added score {score:.3} for '{message}' -> Review required: {review}");
    }

    println!("\nDistribution Analysis:");
    let current_threshold = distribution.get_percentile_threshold(0.8).unwrap_or(0.0);
    println!("Current 80th percentile threshold: {current_threshold:.3}");

    println!("\n=== Absolute Override Test ===");
    let high_score = 0.95;
    let review_needed = analyser.requires_scribes_review_test(high_score, &mut distribution);
    println!(
        "Score {high_score:.3} (>= 0.9 absolute override) -> Review required: {review_needed}"
    );
    assert!(
        review_needed,
        "Score above absolute threshold must trigger a review."
    );

    let medium_score = 0.75;
    let review_not_needed = analyser.requires_scribes_review_test(medium_score, &mut distribution);
    println!(
        "Score {medium_score:.3} (< 0.9 absolute override, < 80th percentile) -> Review required: {review_not_needed}"
    );
    assert!(
        !review_not_needed,
        "Score below all thresholds should not trigger a review."
    );
}
