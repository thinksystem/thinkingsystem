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

use llm_contracts::{LLMError, LLMResult};
use regex::Regex;
use std::collections::HashMap;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub struct SecurityProcessor {
    pii_patterns: HashMap<String, Regex>,
    blocked_patterns: Vec<Regex>,
    max_content_length: usize,
}

impl SecurityProcessor {
    pub fn new() -> Self {
        let mut pii_patterns = HashMap::new();

        pii_patterns.insert(
            "email".to_string(),
            Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap(),
        );

        pii_patterns.insert(
            "phone".to_string(),
            Regex::new(r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b").unwrap(),
        );

        pii_patterns.insert(
            "ssn".to_string(),
            Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(),
        );

        pii_patterns.insert(
            "credit_card".to_string(),
            Regex::new(r"\b\d{4}[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{4}\b").unwrap(),
        );

        Self {
            pii_patterns,
            blocked_patterns: Vec::new(),
            max_content_length: 1_000_000,
        }
    }

    pub fn add_pii_pattern(&mut self, name: String, pattern: Regex) {
        self.pii_patterns.insert(name, pattern);
    }

    pub fn add_blocked_pattern(&mut self, pattern: Regex) {
        self.blocked_patterns.push(pattern);
    }

    pub fn set_max_content_length(&mut self, max_length: usize) {
        self.max_content_length = max_length;
    }

    pub fn scan_for_pii(&self, content: &str) -> Vec<String> {
        let mut detected_pii = Vec::new();

        for (pii_type, pattern) in &self.pii_patterns {
            if pattern.is_match(content) {
                detected_pii.push(pii_type.clone());
            }
        }

        if !detected_pii.is_empty() {
            debug!("Detected PII types: {:?}", detected_pii);
        }

        detected_pii
    }

    pub fn scrub_pii(&self, content: &str) -> String {
        let mut scrubbed = content.to_string();

        for (pii_type, pattern) in &self.pii_patterns {
            let placeholder = format!("[{}]", pii_type.to_uppercase());
            scrubbed = pattern
                .replace_all(&scrubbed, placeholder.as_str())
                .to_string();
        }

        scrubbed
    }

    pub fn validate_content(&self, content: &str) -> LLMResult<()> {
        if content.len() > self.max_content_length {
            return Err(LLMError::Validation(format!(
                "Content length {} exceeds maximum allowed length {}",
                content.len(),
                self.max_content_length
            )));
        }

        for (index, pattern) in self.blocked_patterns.iter().enumerate() {
            if pattern.is_match(content) {
                warn!("Content blocked by security pattern {}", index);
                return Err(LLMError::Validation(
                    "Content contains blocked patterns".to_string(),
                ));
            }
        }

        Ok(())
    }

    pub fn process_content(&self, content: &str, scrub_pii: bool) -> LLMResult<String> {
        self.validate_content(content)?;

        if scrub_pii {
            let pii_types = self.scan_for_pii(content);
            if !pii_types.is_empty() {
                debug!("Scrubbing PII types: {:?}", pii_types);
                Ok(self.scrub_pii(content))
            } else {
                Ok(content.to_string())
            }
        } else {
            let pii_types = self.scan_for_pii(content);
            if !pii_types.is_empty() {
                warn!("Content contains PII types: {:?}", pii_types);
            }
            Ok(content.to_string())
        }
    }
}

impl Default for SecurityProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_detection() {
        let processor = SecurityProcessor::new();
        let content = "Contact me at john.doe@example.com for more info.";

        let pii_types = processor.scan_for_pii(content);
        assert!(pii_types.contains(&"email".to_string()));

        let scrubbed = processor.scrub_pii(content);
        assert!(scrubbed.contains("[EMAIL]"));
        assert!(!scrubbed.contains("john.doe@example.com"));
    }

    #[test]
    fn test_phone_detection() {
        let processor = SecurityProcessor::new();
        let content = "Call me at 555-123-4567 or 555.987.6543";

        let pii_types = processor.scan_for_pii(content);
        assert!(pii_types.contains(&"phone".to_string()));

        let scrubbed = processor.scrub_pii(content);
        assert!(scrubbed.contains("[PHONE]"));
    }

    #[test]
    fn test_content_length_validation() {
        let mut processor = SecurityProcessor::new();
        processor.set_max_content_length(100);

        let short_content = "This is a short message.";
        assert!(processor.validate_content(short_content).is_ok());

        let long_content = "x".repeat(101);
        assert!(processor.validate_content(&long_content).is_err());
    }
}
