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

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenCategory {
    pub category_type: String,
    pub value: String,
    pub confidence: f32,
    pub subcategories: Vec<TokenCategory>,
    pub metadata: HashMap<String, Value>,
}
pub fn extract_basic_categories(text: &str) -> Vec<TokenCategory> {
    let mut categories = Vec::new();
    for word in text.split_whitespace() {
        let category = if word.chars().all(|c| c.is_uppercase()) && word.len() > 1 {
            "ENTITY"
        } else if word.ends_with('?') {
            "QUESTION_MARKER"
        } else if word.parse::<f64>().is_ok() {
            "NUMBER"
        } else {
            "TERM"
        };
        categories.push(TokenCategory {
            category_type: category.to_string(),
            value: word.to_string(),
            confidence: 0.6,
            subcategories: Vec::new(),
            metadata: HashMap::new(),
        });
    }
    categories
}
pub fn analyse_basic_sentiment(text: &str) -> f32 {
    let positive_words = [
        "good",
        "great",
        "excellent",
        "amazing",
        "wonderful",
        "fantastic",
        "love",
        "like",
        "happy",
    ];
    let negative_words = [
        "bad",
        "terrible",
        "awful",
        "horrible",
        "hate",
        "dislike",
        "sad",
        "angry",
        "frustrated",
    ];
    let text_lower = text.to_lowercase();
    let mut positive_count = 0;
    let mut negative_count = 0;
    for word in positive_words {
        if text_lower.contains(word) {
            positive_count += 1;
        }
    }
    for word in negative_words {
        if text_lower.contains(word) {
            negative_count += 1;
        }
    }
    let total_sentiment_words = positive_count + negative_count;
    if total_sentiment_words == 0 {
        return 0.0;
    }
    let sentiment_ratio =
        (positive_count as f32 - negative_count as f32) / total_sentiment_words as f32;
    sentiment_ratio.clamp(-1.0, 1.0)
}
pub fn extract_basic_topics(text: &str, min_word_length: usize, top_n: usize) -> Vec<String> {
    let mut word_freq: HashMap<String, usize> = HashMap::new();
    for word in text.split_whitespace() {
        let cleaned_word = word
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphabetic())
            .collect::<String>();
        if cleaned_word.len() >= min_word_length {
            *word_freq.entry(cleaned_word).or_insert(0) += 1;
        }
    }
    let mut freq_vec: Vec<(String, usize)> = word_freq.into_iter().collect();
    freq_vec.sort_by(|a, b| b.1.cmp(&a.1));
    freq_vec
        .into_iter()
        .take(top_n)
        .map(|(word, _)| word)
        .collect()
}
