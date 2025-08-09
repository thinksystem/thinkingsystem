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

use thiserror::Error;
#[derive(Error, Debug)]
pub enum QueryProcessorError {
    #[error("Database command failed: {0}")]
    Database(String),
    #[error("SQL parsing failed: {0}")]
    SqlParsing(#[from] sqlparser::parser::ParserError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Configuration is invalid: {0}")]
    Config(String),
    #[error("Input validation failed: {0}")]
    Validation(String),
    #[error("JSON serialisation/deserialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Security violation: {0}")]
    Security(String),
    #[error("Processing failed: {0}")]
    Processing(String),
    #[error("Lock acquisition failed: {0}")]
    Lock(String),
    #[error("Timeout occurred: {0}")]
    Timeout(String),
    #[error("External service error: {0}")]
    External(String),
}
pub type Result<T> = std::result::Result<T, QueryProcessorError>;
unsafe impl Send for QueryProcessorError {}
unsafe impl Sync for QueryProcessorError {}
impl QueryProcessorError {
    pub fn database<S: Into<String>>(msg: S) -> Self {
        Self::Database(msg.into())
    }
    pub fn config<S: Into<String>>(msg: S) -> Self {
        Self::Config(msg.into())
    }
    pub fn validation<S: Into<String>>(msg: S) -> Self {
        Self::Validation(msg.into())
    }
    pub fn security<S: Into<String>>(msg: S) -> Self {
        Self::Security(msg.into())
    }
    pub fn processing<S: Into<String>>(msg: S) -> Self {
        Self::Processing(msg.into())
    }
    pub fn lock<S: Into<String>>(msg: S) -> Self {
        Self::Lock(msg.into())
    }
    pub fn timeout<S: Into<String>>(msg: S) -> Self {
        Self::Timeout(msg.into())
    }
    pub fn external<S: Into<String>>(msg: S) -> Self {
        Self::External(msg.into())
    }
}
