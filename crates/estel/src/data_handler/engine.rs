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

use crate::data_handler::common::{DatasetMetadata, Result, DataHandlerError};
use crate::data_handler::dataframe::DataFrame;
use crate::data_handler::io::{CsvReader, CsvWriter};
use crate::data_handler::transformation::{
    TransformationEngine, FilterExpression, GroupByOperation,
    CreateColumnOperation, ComparisonOperator, FilterCondition, FilterValue
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock, Mutex};
use rayon::prelude::*;

#[derive(Debug)]
pub struct DataEngine {
    datasets: Arc<RwLock<HashMap<String, Arc<DataFrame>>>>,
    transformation_engine: Arc<TransformationEngine>,
    csv_reader: CsvReader,
    csv_writer: CsvWriter,
    operation_history: Arc<Mutex<Vec<OperationRecord>>>,
}

#[derive(Debug, Clone)]
pub struct OperationRecord {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub operation_type: String,
    pub dataset_id: String,
    pub result_id: Option<String>,
    pub parameters: HashMap<String, String>,
}
impl DataEngine {
    pub fn new() -> Self {
        Self {
            datasets: Arc::new(RwLock::new(HashMap::new())),
            transformation_engine: Arc::new(TransformationEngine::new()),
            csv_reader: CsvReader::new(),
            csv_writer: CsvWriter::new(),
            operation_history: Arc::new(Mutex::new(Vec::new())),
        }
    }
    pub fn with_chunk_size(mut self, chunk_size: usize) -> Self {
        self.csv_reader = self.csv_reader.with_chunk_size(chunk_size);
        self
    }
    pub fn ingest_csv(&self, path: &Path, dataset_name: String) -> Result<String> {
        let dataframe = self.csv_reader.read_file(path, dataset_name)?;
        let dataset_id = dataframe.metadata.id.to_string();
        {
            let mut datasets = self.datasets.write()
                .map_err(|_| DataHandlerError::ThreadSafety("Failed to acquire write lock".to_string()))?;
            datasets.insert(dataset_id.clone(), Arc::new(dataframe));
        }
        self.record_operation("ingest_csv", &dataset_id, Some(&dataset_id), &[
            ("path".to_string(), path.to_string_lossy().to_string()),
        ])?;
        Ok(dataset_id)
    }
    pub fn ingest_csv_streaming<F>(&self, path: &Path, processor: F) -> Result<()>
    where
        F: FnMut(DataFrame) -> Result<()> + Send + Sync,
    {
        self.csv_reader.read_streaming(path, processor)
    }
    pub fn get_dataset(&self, id: &str) -> Result<Option<Arc<DataFrame>>> {
        let datasets = self.datasets.read()
            .map_err(|_| DataHandlerError::ThreadSafety("Failed to acquire read lock".to_string()))?;
        Ok(datasets.get(id).cloned())
    }
    pub fn list_datasets(&self) -> Result<Vec<DatasetMetadata>> {
        let datasets = self.datasets.read()
            .map_err(|_| DataHandlerError::ThreadSafety("Failed to acquire read lock".to_string()))?;
        Ok(datasets.values().map(|df| df.metadata.clone()).collect())
    }
    pub fn filter_dataset(&self, dataset_id: &str, expression: FilterExpression) -> Result<String> {
        let source_dataset = self.get_dataset(dataset_id)?
            .ok_or_else(|| DataHandlerError::ColumnNotFound(dataset_id.to_string()))?;
        let filtered_dataframe = self.transformation_engine.filter(&source_dataset, &expression)?;
        let new_id = filtered_dataframe.metadata.id.to_string();
        {
            let mut datasets = self.datasets.write()
                .map_err(|_| DataHandlerError::ThreadSafety("Failed to acquire write lock".to_string()))?;
            datasets.insert(new_id.clone(), Arc::new(filtered_dataframe));
        }
        self.record_operation("filter", dataset_id, Some(&new_id), &[
            ("expression".to_string(), format!("{expression:?}")),
        ])?;
        Ok(new_id)
    }
    pub fn group_by_dataset(&self, dataset_id: &str, operation: GroupByOperation) -> Result<String> {
        let source_dataset = self.get_dataset(dataset_id)?
            .ok_or_else(|| DataHandlerError::ColumnNotFound(dataset_id.to_string()))?;
        let grouped_dataframe = self.transformation_engine.group_by(&source_dataset, &operation)?;
        let new_id = grouped_dataframe.metadata.id.to_string();
        {
            let mut datasets = self.datasets.write()
                .map_err(|_| DataHandlerError::ThreadSafety("Failed to acquire write lock".to_string()))?;
            datasets.insert(new_id.clone(), Arc::new(grouped_dataframe));
        }
        self.record_operation("group_by", dataset_id, Some(&new_id), &[
            ("group_columns".to_string(), operation.group_columns.join(",")),
        ])?;
        Ok(new_id)
    }
    pub fn create_column(&self, dataset_id: &str, operation: CreateColumnOperation) -> Result<String> {
        let source_dataset = self.get_dataset(dataset_id)?
            .ok_or_else(|| DataHandlerError::ColumnNotFound(dataset_id.to_string()))?;
        let new_dataframe = self.transformation_engine.create_column(&source_dataset, &operation)?;
        let new_id = new_dataframe.metadata.id.to_string();
        {
            let mut datasets = self.datasets.write()
                .map_err(|_| DataHandlerError::ThreadSafety("Failed to acquire write lock".to_string()))?;
            datasets.insert(new_id.clone(), Arc::new(new_dataframe));
        }
        self.record_operation("create_column", dataset_id, Some(&new_id), &[
            ("column_name".to_string(), operation.name),
        ])?;
        Ok(new_id)
    }
    pub fn select_columns(&self, dataset_id: &str, columns: &[String]) -> Result<String> {
        let source_dataset = self.get_dataset(dataset_id)?
            .ok_or_else(|| DataHandlerError::ColumnNotFound(dataset_id.to_string()))?;
        let selected_dataframe = self.transformation_engine.select(&source_dataset, columns)?;
        let new_id = selected_dataframe.metadata.id.to_string();
        {
            let mut datasets = self.datasets.write()
                .map_err(|_| DataHandlerError::ThreadSafety("Failed to acquire write lock".to_string()))?;
            datasets.insert(new_id.clone(), Arc::new(selected_dataframe));
        }
        self.record_operation("select_columns", dataset_id, Some(&new_id), &[
            ("columns".to_string(), columns.join(",")),
        ])?;
        Ok(new_id)
    }
    pub fn export_csv(&self, dataset_id: &str, output_path: &Path) -> Result<()> {
        let dataset = self.get_dataset(dataset_id)?
            .ok_or_else(|| DataHandlerError::ColumnNotFound(dataset_id.to_string()))?;
        self.csv_writer.write_file(&dataset, output_path)?;
        self.record_operation("export_csv", dataset_id, None, &[
            ("output_path".to_string(), output_path.to_string_lossy().to_string()),
        ])?;
        Ok(())
    }
    pub fn remove_dataset(&self, dataset_id: &str) -> Result<()> {
        let mut datasets = self.datasets.write()
            .map_err(|_| DataHandlerError::ThreadSafety("Failed to acquire write lock".to_string()))?;
        datasets.remove(dataset_id);
        self.record_operation("remove_dataset", dataset_id, None, &[])?;
        Ok(())
    }
    pub fn dataset_info(&self, dataset_id: &str) -> Result<Option<(DatasetMetadata, Vec<crate::data_handler::common::ColumnMetadata>)>> {
        let datasets = self.datasets.read()
            .map_err(|_| DataHandlerError::ThreadSafety("Failed to acquire read lock".to_string()))?;
        Ok(datasets.get(dataset_id).map(|df| {
            (df.metadata.clone(), df.column_metadata())
        }))
    }
    pub fn filter_numeric_column(&self, dataset_id: &str, column: &str, operator: ComparisonOperator, value: f64) -> Result<String> {
        let expression = FilterExpression::Condition(FilterCondition {
            column: column.to_string(),
            operator,
            value: FilterValue::Float64(value),
        });
        self.filter_dataset(dataset_id, expression)
    }
    pub fn filter_string_column(&self, dataset_id: &str, column: &str, operator: ComparisonOperator, value: String) -> Result<String> {
        let expression = FilterExpression::Condition(FilterCondition {
            column: column.to_string(),
            operator,
            value: FilterValue::String(value),
        });
        self.filter_dataset(dataset_id, expression)
    }
    pub fn process_in_chunks<F, R>(&self, dataset_id: &str, chunk_size: usize, processor: F) -> Result<Vec<R>>
    where
        F: Fn(&DataFrame) -> Result<R> + Send + Sync,
        R: Send,
    {
        let dataset = self.get_dataset(dataset_id)?
            .ok_or_else(|| DataHandlerError::ColumnNotFound(dataset_id.to_string()))?;
        dataset.process_chunks(chunk_size, processor)
    }
    pub fn memory_usage(&self) -> Result<HashMap<String, usize>> {
        let datasets = self.datasets.read()
            .map_err(|_| DataHandlerError::ThreadSafety("Failed to acquire read lock".to_string()))?;
        Ok(datasets
            .iter()
            .map(|(id, df)| {
                let size = df.columns.len() * df.row_count() * 8;
                (id.clone(), size)
            })
            .collect())
    }
    pub fn cleanup_unused(&self) -> Result<()> {
        let mut datasets = self.datasets.write()
            .map_err(|_| DataHandlerError::ThreadSafety("Failed to acquire write lock".to_string()))?;
        let mut to_remove = Vec::new();
        for (id, dataset) in datasets.iter() {
            if Arc::strong_count(dataset) == 1 {
                to_remove.push(id.clone());
            }
        }
        for id in to_remove {
            datasets.remove(&id);
        }
        Ok(())
    }
    pub fn get_operation_history(&self) -> Result<Vec<OperationRecord>> {
        let history = self.operation_history.lock()
            .map_err(|_| DataHandlerError::ThreadSafety("Failed to acquire history lock".to_string()))?;
        Ok(history.clone())
    }
    fn record_operation(&self, operation_type: &str, dataset_id: &str, result_id: Option<&str>, parameters: &[(String, String)]) -> Result<()> {
        let mut history = self.operation_history.lock()
            .map_err(|_| DataHandlerError::ThreadSafety("Failed to acquire history lock".to_string()))?;
        let record = OperationRecord {
            timestamp: chrono::Utc::now(),
            operation_type: operation_type.to_string(),
            dataset_id: dataset_id.to_string(),
            result_id: result_id.map(|s| s.to_string()),
            parameters: parameters.iter().cloned().collect(),
        };
        history.push(record);
        if history.len() > 1000 {
            let new_len = history.len() - 1000;
            history.drain(0..new_len);
        }
        Ok(())
    }
}
impl Default for DataEngine {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Debug, Clone)]
pub struct ConcurrentDataEngine {
    inner: Arc<DataEngine>,
}
impl ConcurrentDataEngine {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DataEngine::new()),
        }
    }
    pub fn with_chunk_size(self, chunk_size: usize) -> Self {
        Self {
            inner: Arc::new(DataEngine::new().with_chunk_size(chunk_size)),
        }
    }
    pub fn ingest_csv(&self, path: &Path, dataset_name: String) -> Result<String> {
        self.inner.ingest_csv(path, dataset_name)
    }
    pub fn get_dataset(&self, id: &str) -> Result<Option<Arc<DataFrame>>> {
        self.inner.get_dataset(id)
    }
    pub fn filter_dataset(&self, dataset_id: &str, expression: FilterExpression) -> Result<String> {
        self.inner.filter_dataset(dataset_id, expression)
    }
    pub fn group_by_dataset(&self, dataset_id: &str, operation: GroupByOperation) -> Result<String> {
        self.inner.group_by_dataset(dataset_id, operation)
    }
    pub fn create_column(&self, dataset_id: &str, operation: CreateColumnOperation) -> Result<String> {
        self.inner.create_column(dataset_id, operation)
    }
    pub fn select_columns(&self, dataset_id: &str, columns: &[String]) -> Result<String> {
        self.inner.select_columns(dataset_id, columns)
    }
    pub fn export_csv(&self, dataset_id: &str, output_path: &Path) -> Result<()> {
        self.inner.export_csv(dataset_id, output_path)
    }
    pub fn list_datasets(&self) -> Result<Vec<DatasetMetadata>> {
        self.inner.list_datasets()
    }
    pub fn memory_usage(&self) -> Result<HashMap<String, usize>> {
        self.inner.memory_usage()
    }
    pub fn cleanup_unused(&self) -> Result<()> {
        self.inner.cleanup_unused()
    }
    pub fn get_operation_history(&self) -> Result<Vec<OperationRecord>> {
        self.inner.get_operation_history()
    }
    pub fn parallel_filter(&self, dataset_ids: &[String], expressions: Vec<FilterExpression>) -> Result<Vec<String>> {
        if dataset_ids.len() != expressions.len() {
            return Err(DataHandlerError::InvalidOperation(
                "Dataset IDs and expressions must have the same length".to_string()
            ));
        }
        let results: Result<Vec<String>> = dataset_ids
            .par_iter()
            .zip(expressions.into_par_iter())
            .map(|(id, expr)| self.filter_dataset(id, expr))
            .collect();
        results
    }
    pub fn parallel_export(&self, dataset_ids: &[String], output_paths: &[&Path]) -> Result<()> {
        if dataset_ids.len() != output_paths.len() {
            return Err(DataHandlerError::InvalidOperation(
                "Dataset IDs and output paths must have the same length".to_string()
            ));
        }
        dataset_ids
            .par_iter()
            .zip(output_paths.par_iter())
            .try_for_each(|(id, path)| self.export_csv(id, path))
    }
}
impl Default for ConcurrentDataEngine {
    fn default() -> Self {
        Self::new()
    }
}
