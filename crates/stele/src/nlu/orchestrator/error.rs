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

use llm_contracts::LLMError;
use serde_json;
use serde_yaml;
use toml;
#[derive(Debug)]
pub struct OrchestratorError {
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}
impl std::fmt::Display for OrchestratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for OrchestratorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| {
            let e: &(dyn std::error::Error + Send + Sync) = e.as_ref();
            e as &(dyn std::error::Error + 'static)
        })
    }
}
impl OrchestratorError {
    pub fn new(message: String) -> Self {
        Self {
            message,
            source: None,
        }
    }
    pub fn with_source<E: std::error::Error + Send + Sync + 'static>(
        message: String,
        source: E,
    ) -> Self {
        Self {
            message,
            source: Some(Box::new(source)),
        }
    }
}
impl From<String> for OrchestratorError {
    fn from(message: String) -> Self {
        OrchestratorError::new(message)
    }
}
impl From<&str> for OrchestratorError {
    fn from(message: &str) -> Self {
        OrchestratorError::new(message.to_string())
    }
}
impl From<serde_yaml::Error> for OrchestratorError {
    fn from(err: serde_yaml::Error) -> Self {
        OrchestratorError::with_source("YAML parsing error".to_string(), err)
    }
}
impl From<tokio::io::Error> for OrchestratorError {
    fn from(err: tokio::io::Error) -> Self {
        OrchestratorError::with_source("IO error".to_string(), err)
    }
}
impl From<serde_json::Error> for OrchestratorError {
    fn from(err: serde_json::Error) -> Self {
        OrchestratorError::with_source("JSON error".to_string(), err)
    }
}
impl From<toml::de::Error> for OrchestratorError {
    fn from(err: toml::de::Error) -> Self {
        OrchestratorError::with_source("TOML parsing error".to_string(), err)
    }
}
impl From<LLMError> for OrchestratorError {
    fn from(err: LLMError) -> Self {
        OrchestratorError::new(format!("LLM error: {err}"))
    }
}
