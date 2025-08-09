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

#[test]
fn test_model_optimisation() {
    println!("\n=== Model Optimisation Test ===");

    let mut optimiser = ModelOptimiser::new();

    optimiser.generate_synthetic_training_data(20);
    println!(
        "Generated {} training examples",
        optimiser.training_data().len()
    );

    let (default_performance, _) = optimiser.evaluate_config(&ScoringConfig::default());
    println!("\nDefault Config Performance:");
    println!(
        "  -> Precision: {:.3}, Recall: {:.3}, F1 Score: {:.3}",
        default_performance.precision, default_performance.recall, default_performance.f1_score
    );
    println!(
        "  -> TP: {}, FP: {}, TN: {}, FN: {}",
        default_performance.true_positives,
        default_performance.false_positives,
        default_performance.true_negatives,
        default_performance.false_negatives
    );

    println!("\nRunning comprehensive grid search optimisation...");
    let optimised_config = optimiser.optimise_grid_search();
    let optimised_performance = optimiser.get_best_performance();

    println!("\nOptimised Config:");
    println!(
        "  -> at_symbol_bonus: {:.3}, all_digits_bonus: {:.3}",
        optimised_config.at_symbol_bonus, optimised_config.all_digits_bonus
    );
    println!(
        "  -> uuid_like_bonus: {:.3}, alphanumeric_bonus: {:.3}",
        optimised_config.uuid_like_bonus, optimised_config.alphanumeric_bonus
    );
    println!(
        "  -> api_key_like_bonus: {:.3}, length_bonus_8: {:.3}",
        optimised_config.api_key_like_bonus, optimised_config.length_bonus_8
    );

    println!("Optimised Performance:");
    println!(
        "  -> Precision: {:.3}, Recall: {:.3}, F1 Score: {:.3}",
        optimised_performance.precision,
        optimised_performance.recall,
        optimised_performance.f1_score
    );
    println!(
        "  -> TP: {}, FP: {}, TN: {}, FN: {}",
        optimised_performance.true_positives,
        optimised_performance.false_positives,
        optimised_performance.true_negatives,
        optimised_performance.false_negatives
    );

    let improvement = optimised_performance.f1_score - default_performance.f1_score;
    println!("Improvement in F1 Score: {improvement:+.3}");

    if improvement > 0.001 {
        println!("✓ Optimiser successfully found a better configuration!");
    } else {
        println!("⚠ No significant improvement found - training data may still need to be more challenging");
    }

    println!("\n=== Testing Optimised Model on New Challenging Cases ===");
    let state_dir = tempfile::tempdir().unwrap();
    let mut optimised_security_service = MessageSecurity::new_with_state(
        optimised_config,
        optimiser.best_distribution().clone(),
        state_dir.path(),
    );

    let challenging_test_cases = [
        ("My email is final.test@domain.org", true),
        ("Log file: debug@build.log", false),
        ("The card ending in 4012345678901234 should be used.", true),
        ("Order number ORDER-4012345678901234 was processed.", false),
        ("API key: sk-1234567890ABCDEF1234567890ABCDEF", true),
        ("Product code: PROD-1234567890ABCDEF", false),
        ("SSN: 123-45-6789", true),
        ("Test case number: TEST-123-45-6789", false),
        ("Just a regular follow-up message.", false),
    ];

    let mut correct_predictions = 0;
    for (text, expected_sensitive) in &challenging_test_cases {
        let analysis = optimised_security_service.assess_message_risk(text);
        let predicted_sensitive = analysis.requires_scribes_review();
        let is_correct = predicted_sensitive == *expected_sensitive;
        if is_correct {
            correct_predictions += 1;
        }

        println!(
            "Text: '{}' | Expected: {} | Predicted: {} | Score: {:.3} | {}",
            text,
            expected_sensitive,
            predicted_sensitive,
            analysis.overall_risk_score(),
            if is_correct { "✓" } else { "✗" }
        );
    }

    let accuracy = correct_predictions as f64 / challenging_test_cases.len() as f64;
    println!(
        "\nTest Accuracy: {:.1}% ({}/{})",
        accuracy * 100.0,
        correct_predictions,
        challenging_test_cases.len()
    );

    assert!(
        accuracy >= 0.7,
        "Optimised model should achieve at least 70% accuracy on challenging test cases"
    );
}
