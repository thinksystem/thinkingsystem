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

use serde_json::json;
use sleet::llm::utils::{chunk_text, clean_text, extract_code_blocks, extract_json_from_text};

#[test]
fn test_extract_json_from_text_object() {
    let text = r#"Here's some text {"key": "value", "number": 42} and more text"#;
    let result = extract_json_from_text(text).unwrap();
    assert_eq!(result, json!({"key": "value", "number": 42}));
}

#[test]
fn test_extract_json_from_text_array() {
    let text = r#"Some text [1, 2, "test"] more text"#;
    let result = extract_json_from_text(text).unwrap();
    assert_eq!(result, json!([1, 2, "test"]));
}

#[test]
fn test_extract_json_from_text_full() {
    let text = r#"{"key": "value"}"#;
    let result = extract_json_from_text(text).unwrap();
    assert_eq!(result, json!({"key": "value"}));
}

#[test]
fn test_extract_json_fallback() {
    let text = "This is just plain text.";
    let result = extract_json_from_text(text).unwrap();
    assert_eq!(result["response"], "This is just plain text.");
    assert!(result["error"].is_string());
}

#[test]
fn test_clean_text() {
    let messy_text = "  Hello  \n  world  \n\n  ";
    let cleaned = clean_text(messy_text);
    assert_eq!(cleaned, "Hello world");
}

#[test]
fn test_chunk_text() {
    let text = "This is a test string with multiple words";
    let chunks = chunk_text(text, 15);
    assert_eq!(
        chunks,
        vec!["This is a test", "string with", "multiple words"]
    );
    assert!(chunks.iter().all(|chunk| chunk.len() <= 15));
}

#[test]
fn test_extract_code_blocks() {
    let text = r#"
Here's some code:
```rust
fn main() {
    println!("Hello, world!");
}
```
And some Python:
```python
print("Hello, world!")
```
"#;
    let blocks = extract_code_blocks(text);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].0, Some("rust".to_string()));
    assert_eq!(
        blocks[0].1,
        "fn main() {\n    println!(\"Hello, world!\");\n}"
    );
    assert_eq!(blocks[1].0, Some("python".to_string()));
}
