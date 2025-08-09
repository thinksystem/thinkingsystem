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

use crate::nlu::orchestrator::NLUOrchestrator;
pub use crate::nlu::orchestrator::{
    Action, Entity, ExtractedData, NumericalValue, Relationship, TemporalMarker,
};
use crate::nlu::{InputSegment, SegmentType};
use std::collections::HashMap;
pub struct InputSegmenter {
    orchestrator: Option<NLUOrchestrator>,
}
impl InputSegmenter {
    pub async fn new(config_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let orchestrator = NLUOrchestrator::new(config_path)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        Ok(Self {
            orchestrator: Some(orchestrator),
        })
    }
    pub fn new_fallback() -> Self {
        Self { orchestrator: None }
    }
    pub async fn segment_input(
        &self,
        input: &str,
    ) -> Result<Vec<InputSegment>, Box<dyn std::error::Error>> {
        if let Some(orchestrator) = &self.orchestrator {
            let nlu_data = orchestrator
                .process_input(input)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            Ok(nlu_data.segments)
        } else {
            Ok(self.fallback_segment_input(input))
        }
    }
    fn fallback_segment_input(&self, input: &str) -> Vec<InputSegment> {
        let mut segments = Vec::new();
        let mut current_segment = String::new();
        let mut chars = input.chars().peekable();
        while let Some(ch) = chars.next() {
            current_segment.push(ch);
            if matches!(ch, '.' | '!' | '?') {
                let should_split = chars
                    .peek()
                    .is_none_or(|&next_ch| next_ch.is_whitespace() || next_ch.is_uppercase());
                if should_split {
                    let segment_text = current_segment.trim().to_string();
                    if !segment_text.is_empty() {
                        segments.push(segment_text);
                    }
                    current_segment.clear();
                }
            }
        }
        let remaining = current_segment.trim();
        if !remaining.is_empty() {
            segments.push(remaining.to_string());
        }
        if segments.is_empty() {
            segments.push(input.to_string());
        }
        segments
            .into_iter()
            .enumerate()
            .map(|(i, text)| self.create_fallback_segment(&text, i))
            .collect()
    }
    fn create_fallback_segment(&self, text: &str, index: usize) -> InputSegment {
        let segment_type = if text.contains('?') {
            SegmentType::Question {
                expected_answer_type: "unknown".to_string(),
            }
        } else if self.contains_command_words(text) {
            SegmentType::Command {
                operation: "unknown".to_string(),
            }
        } else {
            SegmentType::Statement {
                intent: "unknown".to_string(),
            }
        };
        let priority = if text.contains('?') { 80 } else { 50 };
        InputSegment {
            text: text.to_string(),
            segment_type,
            priority,
            dependencies: if index > 0 { vec![index - 1] } else { vec![] },
            metadata: HashMap::from([
                (
                    "fallback".to_string(),
                    serde_json::Value::String("true".to_string()),
                ),
                (
                    "method".to_string(),
                    serde_json::Value::String("rule_based".to_string()),
                ),
            ]),
            tokens: text.split_whitespace().map(|s| s.to_string()).collect(),
        }
    }
    fn contains_command_words(&self, text: &str) -> bool {
        let command_words = [
            "create", "update", "delete", "add", "remove", "set", "get", "find", "search",
        ];
        let text_lower = text.to_lowercase();
        command_words.iter().any(|&word| text_lower.contains(word))
    }
    pub fn prioritise_segments(&self, mut segments: Vec<InputSegment>) -> Vec<InputSegment> {
        segments.sort_by(|a, b| {
            let a_deps = a.dependencies.len();
            let b_deps = b.dependencies.len();
            if a_deps != b_deps {
                return a_deps.cmp(&b_deps);
            }
            b.priority.cmp(&a.priority)
        });
        segments
    }
}
