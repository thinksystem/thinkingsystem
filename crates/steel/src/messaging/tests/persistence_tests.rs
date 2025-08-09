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
use std::path::PathBuf;

#[test]
fn test_yaml_persistence() {
    println!("\n=== YAML Persistence Test ===");
    let state_dir = tempfile::tempdir().unwrap();
    let state_file_path = state_dir.path().join("score_distribution.yml");

    let mut original_dist = ScoreDistribution::new(5);
    original_dist.add_score(0.1);
    original_dist.add_score(0.5);
    original_dist.add_score(0.9);

    original_dist
        .save_to_file(&state_file_path)
        .expect("Should save to YAML file");
    println!("Saved distribution to {:?}", &state_file_path);

    let yaml_content = fs::read_to_string(&state_file_path).unwrap();
    println!("File content:\n{yaml_content}");

    let loaded_dist =
        ScoreDistribution::load_from_file(&state_file_path).expect("Should load from YAML file");
    println!("Loaded distribution successfully.");

    assert_eq!(
        original_dist.score_history, loaded_dist.score_history,
        "Loaded history should match original"
    );
    assert_eq!(
        original_dist.max_history_size, loaded_dist.max_history_size,
        "Loaded max history size should match original"
    );
}

#[test]
fn test_config_file_loading() {
    println!("\n=== Configuration File Loading Test ===");

    let config = SecurityConfig::load_or_default();
    println!("Loaded configuration:");
    println!(
        "  - max_history_size: {}",
        config.distribution.max_history_size
    );
    println!(
        "  - scribes_review_percentile: {}",
        config.thresholds.scribes_review_percentile
    );
    println!(
        "  - absolute_threshold_override: {}",
        config.thresholds.absolute_threshold_override
    );
    println!("- at_symbol_bonus: {}", config.scoring.at_symbol_bonus);

    let scoring_config = config.to_scoring_config();
    assert_eq!(scoring_config.at_symbol_bonus, 0.6);
    assert_eq!(scoring_config.absolute_threshold_override, 0.75);
    assert_eq!(scoring_config.min_history_for_percentile, 20);

    println!("Configuration loaded successfully from TOML file");

    let analyser = ContentAnalyser::default();
    let score = analyser.score_token_test("test@example.com");
    println!("Email token score with loaded config: {score:.3}");

    assert!(
        score > 0.5,
        "Email should have high score with default config"
    );
}

#[test]
fn test_persistent_config_directory() {
    println!("\n=== Persistent Config Directory Test ===");

    let mut security = MessageSecurity::default();

    let test_messages = [
        "Regular message",
        "Contact me at user@example.com",
        "Transaction ID: 1234567890123456",
    ];

    for msg in &test_messages {
        let analysis = security.assess_message_risk(msg);
        println!(
            "Processed: '{}' -> Score: {:.3}, Review: {}",
            msg,
            analysis.overall_risk_score(),
            analysis.requires_scribes_review()
        );
    }

    let config_path = PathBuf::from("src/messaging/state/score_distribution.yml");
    println!("Checking for YAML file at: {:?}", &config_path);

    assert!(
        config_path.exists(),
        "YAML file was not created at expected location: {config_path:?}"
    );

    let yaml_content = fs::read_to_string(&config_path).unwrap();
    println!("YAML file content:\n{yaml_content}");

    assert!(yaml_content.contains("score_history:"));
    assert!(yaml_content.contains("max_history_size:"));

    println!("YAML file successfully created and contains expected data");
    println!("Location: {}", config_path.display());
}
