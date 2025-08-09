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

use crate::llm::{LLMError, LLMResult};
use serde_json::Value;
use tracing::{debug, warn};

pub fn extract_json_from_text(text: &str) -> LLMResult<Value> {
    debug!("Attempting to extract JSON from text");

    if let Ok(json_val) = serde_json::from_str::<Value>(text) {
        debug!("Successfully parsed entire text as JSON");
        return Ok(json_val);
    }

    let code_blocks = extract_code_blocks(text);
    for (language, code_content) in code_blocks {
        if language.as_deref() == Some("json") || language.is_none() {
            if let Ok(json_val) = serde_json::from_str::<Value>(&code_content) {
                debug!("Successfully extracted JSON from markdown code fence");
                return Ok(json_val);
            }
        }
    }

    if let Some(json_obj) = find_json_delimiters(text, '{', '}') {
        if let Ok(val) = serde_json::from_str(&json_obj) {
            debug!("Successfully extracted JSON object from text");
            return Ok(val);
        }
        warn!(
            "Found JSON-like substring, but it failed to parse: {}",
            json_obj
        );
    }

    if let Some(json_arr) = find_json_delimiters(text, '[', ']') {
        if let Ok(val) = serde_json::from_str(&json_arr) {
            debug!("Successfully extracted JSON array from text");
            return Ok(val);
        }
        warn!(
            "Found JSON-like array substring, but it failed to parse: {}",
            json_arr
        );
    }

    warn!("No valid JSON structure found in text, returning fallback response");
    Ok(serde_json::json!({
        "response": text.trim(),
        "error": "Failed to parse structured JSON from response."
    }))
}

fn find_json_delimiters(text: &str, start_char: char, end_char: char) -> Option<String> {
    let mut balance = 0;
    let mut start_index = None;

    for (i, ch) in text.char_indices() {
        if ch == start_char {
            if balance == 0 {
                start_index = Some(i);
            }
            balance += 1;
        } else if ch == end_char {
            balance -= 1;
            if balance == 0 {
                if let Some(start) = start_index {
                    return Some(text[start..=i].to_string());
                }
            }
        }
    }
    None
}

pub fn extract_between_delimiters(
    text: &str,
    start_delimiter: &str,
    end_delimiter: &str,
) -> Option<String> {
    let start_pos = text.find(start_delimiter)?;
    let search_start = start_pos + start_delimiter.len();
    let end_pos = text[search_start..].find(end_delimiter)?;
    Some(text[search_start..search_start + end_pos].to_string())
}

pub fn clean_text(text: &str) -> String {
    text.trim()
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn chunk_text(text: &str, max_chunk_size: usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();

    for word in words {
        if current_chunk.len() + word.len() + 1 > max_chunk_size && !current_chunk.is_empty() {
            chunks.push(current_chunk.trim().to_string());
            current_chunk.clear();
        }
        if !current_chunk.is_empty() {
            current_chunk.push(' ');
        }
        current_chunk.push_str(word);
    }
    if !current_chunk.is_empty() {
        chunks.push(current_chunk.trim().to_string());
    }
    chunks
}

pub fn estimate_token_count(text: &str) -> usize {
    text.len() / 4
}

pub fn extract_code_blocks(text: &str) -> Vec<(Option<String>, String)> {
    let mut code_blocks = Vec::new();
    let mut lines = text.lines().peekable();

    while let Some(line) = lines.next() {
        if line.trim().starts_with("```") {
            let language = {
                let lang_str = line.trim().strip_prefix("```").unwrap().trim();
                if lang_str.is_empty() {
                    None
                } else {
                    Some(lang_str.to_string())
                }
            };

            let mut code_content = String::new();
            for code_line in lines.by_ref() {
                if code_line.trim().starts_with("```") {
                    break;
                }
                if !code_content.is_empty() {
                    code_content.push('\n');
                }
                code_content.push_str(code_line);
            }
            code_blocks.push((language, code_content));
        }
    }
    code_blocks
}

pub fn is_valid_json(text: &str) -> bool {
    serde_json::from_str::<Value>(text).is_ok()
}

pub fn format_json(value: &Value) -> LLMResult<String> {
    serde_json::to_string_pretty(value)
        .map_err(|e| LLMError::JsonError(format!("Failed to format JSON: {e}")))
}
