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

use std::io::Write;
use steel::policy::policy_loader::PolicyLoader;
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_load_valid_policy_file() {
    let yaml_content = r#"
providers:
  - name: test_provider
    connection_type: rest
    config:
      base_url: "http://localhost:8080"

policies:
  - name: test_policy
    role: "TestRole"
    action: "publish"
    resource: "test_resource"
    effect: "allow"
    conditions:
      - "data.classification == 'TestData'"
"#;

    let mut temp_file = NamedTempFile::new().unwrap();
    write!(temp_file, "{yaml_content}").unwrap();
    let file_path = temp_file.path().to_str().unwrap();

    let engine = PolicyLoader::load_from_file(file_path).await.unwrap();
    assert_eq!(engine.get_policy_summary().len(), 1);
}

#[tokio::test]
async fn test_load_invalid_policy_file_with_bad_effect() {
    let yaml_content = r#"
providers:
  - name: test_provider
    connection_type: rest
    config:
      base_url: "http://localhost:8080"

policies:
  - name: invalid_policy
    role: "TestRole"
    action: "publish"
    resource: "test_resource"
    effect: "invalid_effect"
    conditions: []
"#;

    let mut temp_file = NamedTempFile::new().unwrap();
    write!(temp_file, "{yaml_content}").unwrap();
    let file_path = temp_file.path().to_str().unwrap();

    let result = PolicyLoader::load_from_file(file_path).await;
    assert!(result.is_err());

    if let Err(error) = result {
        let error_msg = error.to_string();
        assert!(error_msg.contains("invalid_effect") || error_msg.contains("Invalid effect"));
    }
}
