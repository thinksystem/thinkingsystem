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

#[derive(Debug, Clone)]
pub struct InputAnalysis {
    pub length: usize,
    pub word_count: usize,
    pub complexity_score: f64,
    pub contains_question_words: bool,
    pub ends_with_question_mark: bool,
    pub detected_domains: Vec<String>,
}
pub fn analyse(input: &str) -> InputAnalysis {
    let length = input.len();
    let word_count = input.split_whitespace().count();
    let complexity_score = calculate_complexity_score(input);
    let contains_question_words = contains_question_words(input);
    let ends_with_question_mark = input.trim().ends_with('?');
    let detected_domains = detect_domains(input);
    InputAnalysis {
        length,
        word_count,
        complexity_score,
        contains_question_words,
        ends_with_question_mark,
        detected_domains,
    }
}
fn calculate_complexity_score(input: &str) -> f64 {
    let mut score = 0.0;
    score += (input.len() as f64) / 1000.0;
    let sentence_count = input.split('.').count();
    score += (sentence_count as f64) * 0.1;
    if input.contains("if") || input.contains("when") || input.contains("while") {
        score += 0.2;
    }
    if input.matches("and").count() > 2 {
        score += 0.3;
    }
    score.min(1.0)
}
fn contains_question_words(input: &str) -> bool {
    let question_words = [
        "what", "where", "when", "who", "why", "how", "which", "can", "could", "would", "should",
    ];
    let input_lower = input.to_lowercase();
    question_words
        .iter()
        .any(|&word| input_lower.contains(word))
}
fn detect_domains(input: &str) -> Vec<String> {
    let mut domains = Vec::new();
    let input_lower = input.to_lowercase();
    if input_lower.contains("agent")
        || input_lower.contains("user")
        || input_lower.contains("person")
    {
        domains.push("agent".to_string());
    }
    if input_lower.contains("event")
        || input_lower.contains("happen")
        || input_lower.contains("occur")
    {
        domains.push("event".to_string());
    }
    if input_lower.contains("prefer")
        || input_lower.contains("like")
        || input_lower.contains("setting")
    {
        domains.push("personalisation".to_string());
    }
    if domains.is_empty() {
        domains.push("general".to_string());
    }
    domains
}
