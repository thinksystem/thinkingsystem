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

use crate::messaging::insight::generators::*;
use crate::messaging::insight::*;

#[test]
fn test_token_scoring_random() {
    let analyser = ContentAnalyser::default();
    println!("\n=== Token Scoring Observations ===");
    let patterns = vec![
        ("Email", generate_random_email()),
        ("Phone", generate_random_phone()),
        ("UUID", generate_random_uuid()),
        ("API Key", generate_random_api_key()),
        ("Credit Card", generate_random_credit_card()),
        ("Date", generate_random_date()),
        ("Normal Word", generate_random_normal_word()),
    ];
    for (name, token) in patterns {
        let score = analyser.score_token_test(&token);
        println!("- {name}: '{token}' -> Score: {score:.3}");
    }
    let normal_word_score = analyser.score_token_test("word");
    assert_eq!(
        normal_word_score, 0.0,
        "A plain, short word should have a score of zero."
    );
}

#[test]
fn test_scoring_distribution_observation() {
    let analyser = ContentAnalyser::default();
    println!("\n=== Scoring Distribution Observations ===");

    let mut email_scores = Vec::new();
    let mut uuid_scores = Vec::new();
    let mut phone_scores = Vec::new();

    for _ in 0..50 {
        email_scores.push(analyser.score_token_test(&generate_random_email()));
        uuid_scores.push(analyser.score_token_test(&generate_random_uuid()));
        phone_scores.push(analyser.score_token_test(&generate_random_phone()));
    }

    let calc_stats = |scores: &[f64], name: &str| {
        let min = scores.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max = scores.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        let avg = scores.iter().sum::<f64>() / scores.len() as f64;
        println!("{name}: min={min:.3}, max={max:.3}, avg={avg:.3}");
    };
    calc_stats(&email_scores, "Emails");
    calc_stats(&uuid_scores, "UUIDs");
    calc_stats(&phone_scores, "Phones");
    println!("(Note: Assertions on averages removed as they are non-deterministic)");
}
