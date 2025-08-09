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
use std::fmt;
use chrono::{DateTime, Utc};
use uuid::Uuid;
pub const DEFAULT_CHUNK_SIZE: usize = 10000;
#[derive(Debug)]
pub enum DataHandlerError {
    IoError(std::io::Error),
    ParseError(String),
    ColumnNotFound(String),
    TypeMismatch(String),
    OutOfBounds(usize),
    InvalidOperation(String),
    CsvParseError(String),
    SizeLimit(String),
    ThreadSafety(String),
    SchemaValidation(String),
}
impl std::error::Error for DataHandlerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::IoError(e) => Some(e),
            _ => None,
        }
    }
}
impl fmt::Display for DataHandlerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "IO error: {e}"),
            Self::ParseError(s) => write!(f, "Parse error: {s}"),
            Self::ColumnNotFound(s) => write!(f, "Column not found: {s}"),
            Self::TypeMismatch(s) => write!(f, "Type mismatch: {s}"),
            Self::OutOfBounds(i) => write!(f, "Index out of bounds: {i}"),
            Self::InvalidOperation(s) => write!(f, "Invalid operation: {s}"),
            Self::CsvParseError(s) => write!(f, "CSV parse error: {s}"),
            Self::SizeLimit(s) => write!(f, "Size limit exceeded: {s}"),
            Self::ThreadSafety(s) => write!(f, "Thread safety error: {s}"),
            Self::SchemaValidation(s) => write!(f, "Schema validation error: {s}"),
        }
    }
}
impl From<std::io::Error> for DataHandlerError {
    fn from(error: std::io::Error) -> Self {
        Self::IoError(error)
    }
}
impl From<std::num::ParseIntError> for DataHandlerError {
    fn from(error: std::num::ParseIntError) -> Self {
        Self::ParseError(error.to_string())
    }
}
impl From<std::num::ParseFloatError> for DataHandlerError {
    fn from(error: std::num::ParseFloatError) -> Self {
        Self::ParseError(error.to_string())
    }
}
impl From<&str> for DataHandlerError {
    fn from(error: &str) -> Self {
        Self::InvalidOperation(error.to_string())
    }
}
impl From<String> for DataHandlerError {
    fn from(error: String) -> Self {
        Self::InvalidOperation(error)
    }
}
pub type Result<T> = std::result::Result<T, DataHandlerError>;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DataType {
    Int64,
    Float64,
    String,
    Boolean,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetId(String);
impl DatasetId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    pub fn from_string(id: String) -> Self {
        Self(id)
    }
}
impl Default for DatasetId {
    fn default() -> Self { Self::new() }
}
impl fmt::Display for DatasetId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl AsRef<str> for DatasetId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetMetadata {
    pub id: DatasetId,
    pub name: String,
    pub row_count: usize,
    pub column_count: usize,
    pub created_at: DateTime<Utc>,
    pub source_path: Option<std::path::PathBuf>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnMetadata {
    pub name: String,
    pub data_type: DataType,
    pub null_count: usize,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub cardinality: Option<usize>,
}
pub trait SchemaValidator {
    fn validate_schema(&self, schema: &Schema) -> Result<()>;
    fn enforce_constraints(&self, constraints: &[Constraint]) -> Result<()>;
}
pub trait DataQuality {
    fn check_null_percentage(&self, threshold: f64) -> Result<QualityReport>;
    fn detect_outliers(&self, method: OutlierMethod) -> Result<Vec<usize>>;
    fn profile_data(&self) -> Result<DataProfile>;
}
#[derive(Debug, Clone)]
pub struct Schema {
    pub columns: Vec<ColumnSchema>,
}
#[derive(Debug, Clone)]
pub struct ColumnSchema {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
    pub constraints: Vec<Constraint>,
}
#[derive(Debug, Clone)]
pub enum Constraint {
    NotNull,
    Unique,
    MinValue(f64),
    MaxValue(f64),
    MinLength(usize),
    MaxLength(usize),
    Pattern(String),
}
#[derive(Debug, Clone)]
pub struct QualityReport {
    pub null_percentages: std::collections::HashMap<String, f64>,
    pub outlier_counts: std::collections::HashMap<String, usize>,
    pub duplicate_rows: usize,
    pub completeness_score: f64,
}
#[derive(Debug, Clone)]
pub struct DataProfile {
    pub row_count: usize,
    pub column_profiles: std::collections::HashMap<String, ColumnProfile>,
}
#[derive(Debug, Clone)]
pub struct ColumnProfile {
    pub data_type: DataType,
    pub null_count: usize,
    pub unique_count: Option<usize>,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub mean: Option<f64>,
    pub std_dev: Option<f64>,
}
#[derive(Debug, Clone)]
pub enum OutlierMethod {
    IQR,
    ZScore(f64),
    ModifiedZScore(f64),
}
