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
pub enum ChartSuggestionError {
    #[error("API configuration error: {0}")]
    Api(#[from] ApiError),
    #[error("Data profiling error: {0}")]
    Data(#[from] DataError),
    #[error("Chart matching error: {0}")]
    Chart(#[from] ChartError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),
    #[error("Serialisation error: {0}")]
    Serialisation(#[from] SerialisationError),
}
#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Failed to parse YAML configuration: {source}")]
    YamlParseError {
        #[from]
        source: serde_yaml::Error,
    },
    #[error("Failed to read API configuration file '{path}': {source}")]
    ConfigFileError {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("Duplicate chart name found: '{name}'")]
    DuplicateChartName { name: String },
    #[error("Chart '{name}' not found in API configuration")]
    ChartNotFound { name: String },
    #[error("Invalid chart definition for '{name}': {reason}")]
    InvalidChartDefinition { name: String, reason: String },
    #[error("Missing required argument '{arg}' for chart '{chart}'")]
    MissingRequiredArgument { chart: String, arg: String },
    #[error("Invalid data type specification: {details}")]
    InvalidDataTypeSpec { details: String },
    #[error("Library '{library}' not supported")]
    UnsupportedLibrary { library: String },
    #[error("API graph is empty or invalid")]
    EmptyApiGraph,
}
#[derive(Error, Debug)]
pub enum DataError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parsing error: {0}")]
    Parsing(String),
    #[error("Profiling error: {source}")]
    ProfilingError {
        #[from]
        source: anyhow::Error,
    },
    #[error("Failed to read data file '{path}': {source}")]
    DataFileError {
        path: String,
        #[source]
        source: polars::error::PolarsError,
    },
    #[error("Failed to profile column '{column}': {reason}")]
    ColumnProfilingError { column: String, reason: String },
    #[error("Invalid data type detected for column '{column}': {details}")]
    InvalidDataType { column: String, details: String },
    #[error(
        "Data type detection failed for column '{column}': confidence too low ({confidence:.2})"
    )]
    LowTypeConfidence { column: String, confidence: f64 },
    #[error("Failed to calculate statistics for column '{column}': {source}")]
    StatisticsError {
        column: String,
        #[source]
        source: polars::error::PolarsError,
    },
    #[error("Empty dataset provided for profiling")]
    EmptyDataset,
    #[error("Column '{column}' not found in dataset")]
    ColumnNotFound { column: String },
    #[error("Unsupported data format: {format}")]
    UnsupportedFormat { format: String },
    #[error("Data quality too low: {reason}")]
    LowDataQuality { reason: String },
    #[error("Temporal parsing failed for column '{column}': {value}")]
    TemporalParsingError { column: String, value: String },
    #[error("Numeric conversion failed for column '{column}': {value}")]
    NumericConversionError { column: String, value: String },
    #[error("Cardinality calculation failed for column '{column}': {source}")]
    CardinalityError {
        column: String,
        #[source]
        source: polars::error::PolarsError,
    },
}
#[derive(Error, Debug)]
pub enum ChartError {
    #[error("No compatible charts found for the given dataset")]
    NoCompatibleCharts,
    #[error("Chart '{name}' cannot be rendered with available data: {reason}")]
    IncompatibleChart { name: String, reason: String },
    #[error("Invalid render specification for chart '{name}': {reason}")]
    InvalidRenderSpec { name: String, reason: String },
    #[error("Argument mapping error for chart '{chart}': {details}")]
    ArgumentMappingError { chart: String, details: String },
    #[error("Insufficient data dimensions: need {required}, have {available}")]
    InsufficientDimensions { required: usize, available: usize },
    #[error("Data type mismatch for argument '{arg}' in chart '{chart}': expected {expected}, found {found}")]
    DataTypeMismatch {
        chart: String,
        arg: String,
        expected: String,
        found: String,
    },
    #[error("Quality threshold not met: score {score:.2} below minimum {threshold:.2}")]
    QualityThresholdNotMet { score: f64, threshold: f64 },
    #[error("Chart matching timeout: operation took too long")]
    MatchingTimeout,
    #[error("Domain hint validation failed: {reason}")]
    InvalidDomainHint { reason: String },
    #[error("Constraint satisfaction failed: {constraint}")]
    ConstraintSatisfactionError { constraint: String },
    #[error("Backtracking limit exceeded: too many combinations to evaluate")]
    BacktrackingLimitExceeded,
}
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Invalid matching configuration: {field} is out of range")]
    InvalidMatchingConfig { field: String },
    #[error("Invalid profiling configuration: {field} = {value}")]
    InvalidProfilingConfig { field: String, value: String },
    #[error("Conflicting configuration options: {details}")]
    ConflictingOptions { details: String },
    #[error("Missing required configuration: {field}")]
    MissingRequiredConfig { field: String },
    #[error("Configuration validation failed: {reason}")]
    ValidationFailed { reason: String },
    #[error("Unsupported configuration version: {version}")]
    UnsupportedVersion { version: String },
}
#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Column '{column}' mapped to argument '{arg}' does not exist in dataset")]
    MissingColumn { column: String, arg: String },
    #[error("Column '{column}' is mapped to multiple arguments")]
    DuplicateColumnMapping { column: String },
    #[error("Required argument '{arg}' is not mapped to any column")]
    UnmappedRequiredArgument { arg: String },
    #[error("Circular dependency detected in argument mapping")]
    CircularDependency,
    #[error("Invalid cardinality value: {value}")]
    InvalidCardinality { value: usize },
    #[error("Quality score out of range: {score}")]
    InvalidQualityScore { score: f64 },
    #[error("Percentage value out of range: {value}")]
    InvalidPercentage { value: f64 },
    #[error("Empty field not allowed: {field}")]
    EmptyField { field: String },
    #[error("Invalid chart name: '{name}' contains illegal characters")]
    InvalidChartName { name: String },
    #[error("Invalid library name: '{name}'")]
    InvalidLibraryName { name: String },
}
#[derive(Error, Debug)]
pub enum SerialisationError {
    #[error("JSON serialisation failed: {source}")]
    JsonSerialisationError {
        #[from]
        source: serde_json::Error,
    },
    #[error("YAML serialization failed: {source}")]
    YamlSerialisationError {
        #[from]
        source: serde_yaml::Error,
    },
    #[error("Failed to export profiles: {reason}")]
    ExportError { reason: String },
    #[error("Failed to import profiles: {reason}")]
    ImportError { reason: String },
    #[error("Unsupported export format: {format}")]
    UnsupportedExportFormat { format: String },
    #[error("Data corruption detected during serialisation")]
    DataCorruption,
}
pub type Result<T> = std::result::Result<T, ChartSuggestionError>;
pub type ApiResult<T> = std::result::Result<T, ApiError>;
pub type DataResult<T> = std::result::Result<T, DataError>;
pub type ChartResult<T> = std::result::Result<T, ChartError>;
pub type ValidationResult<T> = std::result::Result<T, ValidationError>;
pub type ConfigResult<T> = std::result::Result<T, ConfigError>;
pub type SerialisationResult<T> = std::result::Result<T, SerialisationError>;
pub trait ErrorExt<T> {
    fn with_context(self, msg: &'static str) -> Result<T>;
    fn with_context_fn<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String;
}
impl From<anyhow::Error> for ChartSuggestionError {
    fn from(err: anyhow::Error) -> Self {
        ChartSuggestionError::Config(ConfigError::ValidationFailed {
            reason: err.to_string(),
        })
    }
}
impl From<serde_json::Error> for ChartSuggestionError {
    fn from(err: serde_json::Error) -> Self {
        ChartSuggestionError::Serialisation(SerialisationError::JsonSerialisationError {
            source: err,
        })
    }
}
impl<T> ErrorExt<T> for Result<T> {
    fn with_context(self, msg: &'static str) -> Result<T> {
        self.map_err(|e| {
            ChartSuggestionError::Config(ConfigError::ValidationFailed {
                reason: format!("{msg}: {e}"),
            })
        })
    }
    fn with_context_fn<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| {
            ChartSuggestionError::Config(ConfigError::ValidationFailed {
                reason: format!("{}: {}", f(), e),
            })
        })
    }
}
impl ChartSuggestionError {
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            ChartSuggestionError::Chart(ChartError::NoCompatibleCharts)
                | ChartSuggestionError::Chart(ChartError::QualityThresholdNotMet { .. })
                | ChartSuggestionError::Data(DataError::LowTypeConfidence { .. })
                | ChartSuggestionError::Validation(_)
        )
    }
    pub fn category(&self) -> &'static str {
        match self {
            ChartSuggestionError::Api(_) => "API",
            ChartSuggestionError::Data(_) => "Data",
            ChartSuggestionError::Chart(_) => "Chart",
            ChartSuggestionError::Io(_) => "I/O",
            ChartSuggestionError::Validation(_) => "Validation",
            ChartSuggestionError::Config(_) => "Configuration",
            ChartSuggestionError::Serialisation(_) => "Serialisation",
        }
    }
    pub fn suggestions(&self) -> Vec<String> {
        match self {
            ChartSuggestionError::Chart(ChartError::NoCompatibleCharts) => vec![
                "Check if your data has at least 2 dimensions".to_string(),
                "Verify data types are correctly detected".to_string(),
                "Try lowering the quality threshold".to_string(),
            ],
            ChartSuggestionError::Data(DataError::LowDataQuality { .. }) => vec![
                "Clean your data to reduce null values".to_string(),
                "Check for outliers and invalid values".to_string(),
                "Consider data transformation or filtering".to_string(),
            ],
            ChartSuggestionError::Api(ApiError::ChartNotFound { .. }) => vec![
                "Check the chart name spelling".to_string(),
                "Verify the API configuration is loaded".to_string(),
                "List available charts to see valid options".to_string(),
            ],
            _ => vec!["Check the error message for specific guidance".to_string()],
        }
    }
    pub fn user_message(&self) -> String {
        match self {
            ChartSuggestionError::Chart(ChartError::NoCompatibleCharts) => {
                "No suitable charts found for your data. Try checking data quality or adjusting settings.".to_string()
            }
            ChartSuggestionError::Data(DataError::EmptyDataset) => {
                "The dataset appears to be empty. Please provide data with at least one row.".to_string()
            }
            ChartSuggestionError::Api(ApiError::ConfigFileError { .. }) => {
                "Unable to load chart configuration. Please check the configuration file.".to_string()
            }
            _ => self.to_string(),
        }
    }
}
#[macro_export]
macro_rules! chart_error {
    ($kind:expr, $($arg:tt)*) => {
        ChartSuggestionError::Chart($kind(format!($($arg)*)))
    };
}
#[macro_export]
macro_rules! data_error {
    ($kind:expr, $($arg:tt)*) => {
        ChartSuggestionError::Data($kind(format!($($arg)*)))
    };
}
#[macro_export]
macro_rules! api_error {
    ($kind:expr, $($arg:tt)*) => {
        ChartSuggestionError::Api($kind(format!($($arg)*)))
    };
}
#[macro_export]
macro_rules! validation_error {
    ($kind:expr, $($arg:tt)*) => {
        ChartSuggestionError::Validation($kind(format!($($arg)*)))
    };
}
#[macro_export]
macro_rules! config_error {
    ($kind:expr, $($arg:tt)*) => {
        ChartSuggestionError::Config($kind(format!($($arg)*)))
    };
}
#[derive(Debug, Clone)]
pub enum RecoveryStrategy {
    Retry { max_attempts: usize, delay_ms: u64 },
    Fallback(String),
    Skip,
    UserInput(String),
    AutoCorrect(String),
    None,
}
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub operation: String,
    pub component: String,
    pub input_data: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub recovery_strategy: RecoveryStrategy,
}
impl ErrorContext {
    pub fn new(operation: &str, component: &str) -> Self {
        Self {
            operation: operation.to_string(),
            component: component.to_string(),
            input_data: None,
            timestamp: chrono::Utc::now(),
            recovery_strategy: RecoveryStrategy::None,
        }
    }
    pub fn with_input_data(mut self, data: String) -> Self {
        self.input_data = Some(data);
        self
    }
    pub fn with_recovery_strategy(mut self, strategy: RecoveryStrategy) -> Self {
        self.recovery_strategy = strategy;
        self
    }
}
#[derive(Debug)]
pub struct EnhancedError {
    pub error: ChartSuggestionError,
    pub context: ErrorContext,
    pub chain: Vec<String>,
}
impl EnhancedError {
    pub fn new(error: ChartSuggestionError, context: ErrorContext) -> Self {
        Self {
            error,
            context,
            chain: Vec::new(),
        }
    }
    pub fn with_chain(mut self, chain: Vec<String>) -> Self {
        self.chain = chain;
        self
    }
    pub fn add_to_chain(mut self, message: String) -> Self {
        self.chain.push(message);
        self
    }
    pub fn full_chain(&self) -> String {
        let mut chain = vec![self.error.to_string()];
        chain.extend(self.chain.iter().cloned());
        chain.join(" -> ")
    }
    pub fn can_recover(&self) -> bool {
        self.error.is_recoverable()
            && !matches!(self.context.recovery_strategy, RecoveryStrategy::None)
    }
    pub fn recovery_suggestions(&self) -> Vec<String> {
        let mut suggestions = self.error.suggestions();
        match &self.context.recovery_strategy {
            RecoveryStrategy::Retry { max_attempts, .. } => {
                suggestions.push(format!("Retry up to {max_attempts} times"));
            }
            RecoveryStrategy::Fallback(msg) => {
                suggestions.push(format!("Use fallback: {msg}"));
            }
            RecoveryStrategy::Skip => {
                suggestions.push("Skip this item and continue".to_string());
            }
            RecoveryStrategy::UserInput(prompt) => {
                suggestions.push(format!("User input required: {prompt}"));
            }
            RecoveryStrategy::AutoCorrect(description) => {
                suggestions.push(format!("Auto-correct: {description}"));
            }
            RecoveryStrategy::None => {}
        }
        suggestions
    }
}
impl std::fmt::Display for EnhancedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}:{}] {}",
            self.context.component, self.context.operation, self.error
        )?;
        if !self.chain.is_empty() {
            write!(f, " (Chain: {})", self.chain.join(" -> "))?;
        }
        Ok(())
    }
}
impl std::error::Error for EnhancedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
pub mod utils {
    use super::*;
    pub fn file_not_found(path: &str, _operation: &str) -> ChartSuggestionError {
        ChartSuggestionError::Api(ApiError::ConfigFileError {
            path: path.to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"),
        })
    }
    pub fn data_type_mismatch(
        chart: &str,
        arg: &str,
        expected: &str,
        found: &str,
    ) -> ChartSuggestionError {
        ChartSuggestionError::Chart(ChartError::DataTypeMismatch {
            chart: chart.to_string(),
            arg: arg.to_string(),
            expected: expected.to_string(),
            found: found.to_string(),
        })
    }
    pub fn missing_column(column: &str, arg: &str) -> ChartSuggestionError {
        ChartSuggestionError::Validation(ValidationError::MissingColumn {
            column: column.to_string(),
            arg: arg.to_string(),
        })
    }
    pub fn low_quality(reason: &str) -> ChartSuggestionError {
        ChartSuggestionError::Data(DataError::LowDataQuality {
            reason: reason.to_string(),
        })
    }
    pub fn invalid_config(field: &str, value: &str) -> ChartSuggestionError {
        ChartSuggestionError::Config(ConfigError::InvalidProfilingConfig {
            field: field.to_string(),
            value: value.to_string(),
        })
    }
    pub fn wrap_error<E>(error: E, context: &str) -> ChartSuggestionError
    where
        E: std::error::Error + 'static,
    {
        ChartSuggestionError::Config(ConfigError::ValidationFailed {
            reason: format!("{context}: {error}"),
        })
    }
    pub fn enhanced_error(
        error: ChartSuggestionError,
        operation: &str,
        component: &str,
        recovery: RecoveryStrategy,
    ) -> EnhancedError {
        let context = ErrorContext::new(operation, component).with_recovery_strategy(recovery);
        EnhancedError::new(error, context)
    }
    pub fn is_temporary_failure(error: &ChartSuggestionError) -> bool {
        matches!(
            error,
            ChartSuggestionError::Chart(ChartError::MatchingTimeout)
                | ChartSuggestionError::Io(_)
                | ChartSuggestionError::Data(DataError::DataFileError { .. })
        )
    }
    pub fn error_severity(error: &ChartSuggestionError) -> ErrorSeverity {
        match error {
            ChartSuggestionError::Chart(ChartError::NoCompatibleCharts) => ErrorSeverity::Warning,
            ChartSuggestionError::Chart(ChartError::QualityThresholdNotMet { .. }) => {
                ErrorSeverity::Warning
            }
            ChartSuggestionError::Data(DataError::LowTypeConfidence { .. }) => {
                ErrorSeverity::Warning
            }
            ChartSuggestionError::Validation(_) => ErrorSeverity::Error,
            ChartSuggestionError::Config(_) => ErrorSeverity::Error,
            ChartSuggestionError::Api(ApiError::EmptyApiGraph) => ErrorSeverity::Critical,
            ChartSuggestionError::Io(_) => ErrorSeverity::Error,
            _ => ErrorSeverity::Error,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Critical,
}
impl ErrorSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorSeverity::Info => "INFO",
            ErrorSeverity::Warning => "WARNING",
            ErrorSeverity::Error => "ERROR",
            ErrorSeverity::Critical => "CRITICAL",
        }
    }
    pub fn color_code(&self) -> &'static str {
        match self {
            ErrorSeverity::Info => "\x1b[36m",
            ErrorSeverity::Warning => "\x1b[33m",
            ErrorSeverity::Error => "\x1b[31m",
            ErrorSeverity::Critical => "\x1b[35m",
        }
    }
}
pub struct ErrorReporter {
    pub show_suggestions: bool,
    pub show_context: bool,
    pub colored_output: bool,
}
impl ErrorReporter {
    pub fn new() -> Self {
        Self {
            show_suggestions: true,
            show_context: true,
            colored_output: true,
        }
    }
    pub fn report(&self, error: &ChartSuggestionError) -> String {
        let severity = utils::error_severity(error);
        let mut output = String::new();
        if self.colored_output {
            output.push_str(severity.color_code());
        }
        output.push_str(&format!("[{}] {}\n", severity.as_str(), error));
        if self.colored_output {
            output.push_str("\x1b[0m");
        }
        if self.show_suggestions {
            let suggestions = error.suggestions();
            if !suggestions.is_empty() {
                output.push_str("\nSuggestions:\n");
                for suggestion in suggestions {
                    output.push_str(&format!("  • {suggestion}\n"));
                }
            }
        }
        output
    }
    pub fn report_enhanced(&self, error: &EnhancedError) -> String {
        let severity = utils::error_severity(&error.error);
        let mut output = String::new();
        if self.colored_output {
            output.push_str(severity.color_code());
        }
        output.push_str(&format!("[{}] {}\n", severity.as_str(), error));
        if self.colored_output {
            output.push_str("\x1b[0m");
        }
        if self.show_context {
            output.push_str(&format!(
                "Context: {} in {}\n",
                error.context.operation, error.context.component
            ));
            output.push_str(&format!("Time: {}\n", error.context.timestamp));
            if !error.chain.is_empty() {
                output.push_str(&format!("Chain: {}\n", error.chain.join(" -> ")));
            }
        }
        if self.show_suggestions {
            let suggestions = error.recovery_suggestions();
            if !suggestions.is_empty() {
                output.push_str("\nRecovery suggestions:\n");
                for suggestion in suggestions {
                    output.push_str(&format!("  • {suggestion}\n"));
                }
            }
        }
        output
    }
}
impl Default for ErrorReporter {
    fn default() -> Self {
        Self::new()
    }
}
