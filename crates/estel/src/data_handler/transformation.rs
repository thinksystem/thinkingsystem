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

use crate::data_handler::column::{Column, ColumnData};
use crate::data_handler::common::{DataHandlerError, DataType, DatasetId, Result};
use crate::data_handler::dataframe::DataFrame;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComparisonOperator {
    Equal,
    NotEqual,
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
    Contains,
    StartsWith,
    EndsWith,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogicalOperator {
    And,
    Or,
    Not,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterValue {
    Int64(i64),
    Float64(f64),
    String(String),
    Boolean(bool),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterCondition {
    pub column: String,
    pub operator: ComparisonOperator,
    pub value: FilterValue,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterExpression {
    Condition(FilterCondition),
    And(Box<FilterExpression>, Box<FilterExpression>),
    Or(Box<FilterExpression>, Box<FilterExpression>),
    Not(Box<FilterExpression>),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggregateFunction {
    Count,
    Sum,
    Average,
    Min,
    Max,
    CountDistinct,
    Median,
    StdDev,
    Variance,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateOperation {
    pub column: String,
    pub function: AggregateFunction,
    pub alias: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupByOperation {
    pub group_columns: Vec<String>,
    pub aggregations: Vec<AggregateOperation>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ColumnExpression {
    Column(String),
    Constant(FilterValue),
    Add(Box<ColumnExpression>, Box<ColumnExpression>),
    Subtract(Box<ColumnExpression>, Box<ColumnExpression>),
    Multiply(Box<ColumnExpression>, Box<ColumnExpression>),
    Divide(Box<ColumnExpression>, Box<ColumnExpression>),
    Concat(Box<ColumnExpression>, Box<ColumnExpression>),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateColumnOperation {
    pub name: String,
    pub expression: ColumnExpression,
}
#[derive(Debug)]
pub enum CompiledPredicate {
    Int64Equal(Arc<Column>, i64),
    Int64GreaterThan(Arc<Column>, i64),
    Int64LessThan(Arc<Column>, i64),
    Float64Equal(Arc<Column>, f64),
    Float64GreaterThan(Arc<Column>, f64),
    Float64LessThan(Arc<Column>, f64),
    StringContains(Arc<Column>, String),
    StringEqual(Arc<Column>, String),
    BooleanEqual(Arc<Column>, bool),
    And(Box<CompiledPredicate>, Box<CompiledPredicate>),
    Or(Box<CompiledPredicate>, Box<CompiledPredicate>),
    Not(Box<CompiledPredicate>),
}
impl CompiledPredicate {
    pub fn evaluate(&self, index: usize) -> bool {
        match self {
            CompiledPredicate::Int64Equal(column, val) => match column.as_ref() {
                Column::Int64(data) => data
                    .get(index)
                    .is_some_and(|opt| opt.is_some_and(|v| v == *val)),
                Column::Chunked(_) => column.to_f64(index).is_some_and(|v| (v as i64) == *val),
                _ => false,
            },
            CompiledPredicate::Int64GreaterThan(column, val) => match column.as_ref() {
                Column::Int64(data) => data
                    .get(index)
                    .is_some_and(|opt| opt.is_some_and(|v| v > *val)),
                Column::Chunked(_) => column.to_f64(index).is_some_and(|v| (v as i64) > *val),
                _ => false,
            },
            CompiledPredicate::Float64GreaterThan(column, val) => match column.as_ref() {
                Column::Float64(data) => data
                    .get(index)
                    .is_some_and(|opt| opt.is_some_and(|v| v > *val)),
                _ => column.to_f64(index).is_some_and(|v| v > *val),
            },
            CompiledPredicate::StringContains(column, val) => {
                column.get_string(index).is_some_and(|s| s.contains(val))
            }
            CompiledPredicate::StringEqual(column, val) => {
                column.get_string(index).is_some_and(|s| s == *val)
            }
            CompiledPredicate::BooleanEqual(column, val) => match column.as_ref() {
                Column::Boolean(data) => data
                    .get(index)
                    .is_some_and(|opt| opt.is_some_and(|v| v == *val)),
                _ => false,
            },
            CompiledPredicate::And(left, right) => left.evaluate(index) && right.evaluate(index),
            CompiledPredicate::Or(left, right) => left.evaluate(index) || right.evaluate(index),
            CompiledPredicate::Not(pred) => !pred.evaluate(index),
            _ => false,
        }
    }
    pub fn evaluate_batch(&self, start_index: usize, batch_size: usize) -> Vec<bool> {
        (start_index..start_index + batch_size)
            .into_par_iter()
            .map(|i| self.evaluate(i))
            .collect()
    }
}
#[derive(Debug)]
pub struct TransformationEngine {
    parallel_threshold: usize,
}
impl TransformationEngine {
    pub fn new() -> Self {
        Self {
            parallel_threshold: 10000,
        }
    }
    pub fn with_parallel_threshold(mut self, threshold: usize) -> Self {
        self.parallel_threshold = threshold;
        self
    }
    pub fn filter(
        &self,
        dataframe: &DataFrame,
        expression: &FilterExpression,
    ) -> Result<DataFrame> {
        let predicate = Self::compile_predicate(dataframe, expression)?;
        dataframe.filter(move |i| predicate.evaluate(i))
    }
    pub fn group_by(
        &self,
        dataframe: &DataFrame,
        operation: &GroupByOperation,
    ) -> Result<DataFrame> {
        let groups = if dataframe.row_count() > self.parallel_threshold {
            self.build_groups_parallel(dataframe, &operation.group_columns)?
        } else {
            self.build_groups_sequential(dataframe, &operation.group_columns)?
        };
        self.apply_aggregations(dataframe, groups, operation)
    }
    pub fn create_column(
        &self,
        dataframe: &DataFrame,
        operation: &CreateColumnOperation,
    ) -> Result<DataFrame> {
        let new_column = self.evaluate_expression(dataframe, &operation.expression)?;
        let mut result = dataframe.clone();
        result.add_column(operation.name.clone(), new_column)?;
        Ok(result)
    }
    pub fn select(&self, dataframe: &DataFrame, columns: &[String]) -> Result<DataFrame> {
        dataframe.select(columns)
    }
    fn compile_predicate(
        dataframe: &DataFrame,
        expression: &FilterExpression,
    ) -> Result<CompiledPredicate> {
        match expression {
            FilterExpression::Condition(condition) => {
                let column = dataframe
                    .get_column(&condition.column)
                    .ok_or_else(|| DataHandlerError::ColumnNotFound(condition.column.clone()))?;
                let column_arc = Arc::new(column.clone());
                match (&condition.operator, &condition.value) {
                    (ComparisonOperator::Equal, FilterValue::Int64(val)) => {
                        Ok(CompiledPredicate::Int64Equal(column_arc, *val))
                    }
                    (ComparisonOperator::Equal, FilterValue::Float64(val)) => {
                        Ok(CompiledPredicate::Float64Equal(column_arc, *val))
                    }
                    (ComparisonOperator::Equal, FilterValue::String(val)) => {
                        Ok(CompiledPredicate::StringEqual(column_arc, val.clone()))
                    }
                    (ComparisonOperator::Equal, FilterValue::Boolean(val)) => {
                        Ok(CompiledPredicate::BooleanEqual(column_arc, *val))
                    }
                    (ComparisonOperator::GreaterThan, FilterValue::Int64(val)) => {
                        Ok(CompiledPredicate::Int64GreaterThan(column_arc, *val))
                    }
                    (ComparisonOperator::GreaterThan, FilterValue::Float64(val)) => {
                        Ok(CompiledPredicate::Float64GreaterThan(column_arc, *val))
                    }
                    (ComparisonOperator::LessThan, FilterValue::Int64(val)) => {
                        Ok(CompiledPredicate::Int64LessThan(column_arc, *val))
                    }
                    (ComparisonOperator::LessThan, FilterValue::Float64(val)) => {
                        Ok(CompiledPredicate::Float64LessThan(column_arc, *val))
                    }
                    (ComparisonOperator::Contains, FilterValue::String(val)) => {
                        Ok(CompiledPredicate::StringContains(column_arc, val.clone()))
                    }
                    _ => Err(DataHandlerError::InvalidOperation(format!(
                        "Unsupported filter combination: {:?} with {:?}",
                        condition.operator, condition.value
                    ))),
                }
            }
            FilterExpression::And(left, right) => {
                let left_pred = Self::compile_predicate(dataframe, left)?;
                let right_pred = Self::compile_predicate(dataframe, right)?;
                Ok(CompiledPredicate::And(
                    Box::new(left_pred),
                    Box::new(right_pred),
                ))
            }
            FilterExpression::Or(left, right) => {
                let left_pred = Self::compile_predicate(dataframe, left)?;
                let right_pred = Self::compile_predicate(dataframe, right)?;
                Ok(CompiledPredicate::Or(
                    Box::new(left_pred),
                    Box::new(right_pred),
                ))
            }
            FilterExpression::Not(expr) => {
                let pred = Self::compile_predicate(dataframe, expr)?;
                Ok(CompiledPredicate::Not(Box::new(pred)))
            }
        }
    }
    fn build_groups_sequential(
        &self,
        dataframe: &DataFrame,
        group_columns: &[String],
    ) -> Result<HashMap<Vec<String>, Vec<usize>>> {
        let mut groups: HashMap<Vec<String>, Vec<usize>> = HashMap::new();
        for i in 0..dataframe.row_count() {
            let key: Result<Vec<String>> = group_columns
                .iter()
                .map(|col_name| {
                    dataframe
                        .get_column(col_name)
                        .ok_or_else(|| DataHandlerError::ColumnNotFound(col_name.clone()))?
                        .get_string(i)
                        .ok_or_else(|| {
                            DataHandlerError::InvalidOperation(
                                "Null value in grouping column".to_string(),
                            )
                        })
                })
                .collect();
            groups.entry(key?).or_default().push(i);
        }
        Ok(groups)
    }
    fn build_groups_parallel(
        &self,
        dataframe: &DataFrame,
        group_columns: &[String],
    ) -> Result<HashMap<Vec<String>, Vec<usize>>> {
        use std::sync::Mutex;
        let columns: Result<Vec<_>> = group_columns
            .iter()
            .map(|name| {
                dataframe
                    .get_column(name)
                    .ok_or_else(|| DataHandlerError::ColumnNotFound(name.clone()))
                    .map(|col| (name.clone(), col))
            })
            .collect();
        let columns = columns?;
        let groups: Mutex<HashMap<Vec<String>, Vec<usize>>> = Mutex::new(HashMap::new());
        let chunk_size = std::cmp::max(1000, dataframe.row_count() / rayon::current_num_threads());
        (0..dataframe.row_count())
            .collect::<Vec<_>>()
            .chunks(chunk_size)
            .collect::<Vec<_>>()
            .into_par_iter()
            .try_for_each(|chunk| -> Result<()> {
                let mut local_groups: HashMap<Vec<String>, Vec<usize>> = HashMap::new();
                for &i in chunk {
                    let key: Result<Vec<String>> = columns
                        .iter()
                        .map(|(_, column)| {
                            column.get_string(i).unwrap_or_else(|| "NULL".to_string())
                        })
                        .map(Ok)
                        .collect();
                    local_groups.entry(key?).or_default().push(i);
                }
                let mut global_groups = groups.lock().map_err(|_| {
                    DataHandlerError::ThreadSafety("Failed to acquire groups lock".to_string())
                })?;
                for (key, indices) in local_groups {
                    global_groups.entry(key).or_default().extend(indices);
                }
                Ok(())
            })?;
        groups.into_inner().map_err(|_| {
            DataHandlerError::ThreadSafety("Failed to extract groups from mutex".to_string())
        })
    }
    fn apply_aggregations(
        &self,
        dataframe: &DataFrame,
        groups: HashMap<Vec<String>, Vec<usize>>,
        operation: &GroupByOperation,
    ) -> Result<DataFrame> {
        let mut result_columns: HashMap<String, Vec<Option<String>>> = HashMap::new();
        for col_name in &operation.group_columns {
            result_columns.insert(col_name.clone(), Vec::with_capacity(groups.len()));
        }
        for agg in &operation.aggregations {
            let col_name = agg.alias.as_ref().unwrap_or(&agg.column);
            result_columns.insert(col_name.clone(), Vec::with_capacity(groups.len()));
        }
        let group_results: Result<Vec<_>> = groups
            .into_par_iter()
            .map(
                |(group_key, indices)| -> Result<(Vec<String>, Vec<String>)> {
                    let mut agg_values = Vec::new();
                    for agg in &operation.aggregations {
                        let agg_value = self.calculate_aggregation(
                            dataframe,
                            &agg.column,
                            &agg.function,
                            &indices,
                        )?;
                        agg_values.push(agg_value);
                    }
                    Ok((group_key, agg_values))
                },
            )
            .collect();
        let group_results = group_results?;
        for (group_key, agg_values) in group_results {
            for (i, col_name) in operation.group_columns.iter().enumerate() {
                result_columns
                    .get_mut(col_name)
                    .unwrap()
                    .push(Some(group_key[i].clone()));
            }
            for (agg, value) in operation.aggregations.iter().zip(agg_values) {
                let col_name = agg.alias.as_ref().unwrap_or(&agg.column);
                result_columns.get_mut(col_name).unwrap().push(Some(value));
            }
        }
        let mut result_df = DataFrame::new(crate::data_handler::common::DatasetMetadata {
            id: DatasetId::new(),
            name: format!("{}_grouped", dataframe.metadata.name),
            row_count: result_columns.values().next().map_or(0, |v| v.len()),
            column_count: result_columns.len(),
            created_at: chrono::Utc::now(),
            source_path: None,
        });
        for (col_name, values) in result_columns {
            let column = Column::from_strings(&values, DataType::String)?;
            result_df.add_column(col_name, column)?;
        }
        Ok(result_df)
    }
    fn calculate_aggregation(
        &self,
        dataframe: &DataFrame,
        column_name: &str,
        function: &AggregateFunction,
        indices: &[usize],
    ) -> Result<String> {
        let column = dataframe
            .get_column(column_name)
            .ok_or_else(|| DataHandlerError::ColumnNotFound(column_name.to_string()))?;
        match function {
            AggregateFunction::Count => Ok(indices.len().to_string()),
            AggregateFunction::Sum => match column {
                Column::Int64(data) => {
                    let sum: i64 = indices
                        .par_iter()
                        .filter_map(|&i| data.get(i).and_then(|opt| *opt))
                        .sum();
                    Ok(sum.to_string())
                }
                Column::Float64(data) => {
                    let sum: f64 = indices
                        .par_iter()
                        .filter_map(|&i| data.get(i).and_then(|opt| *opt))
                        .sum();
                    Ok(sum.to_string())
                }
                _ => {
                    let sum: f64 = indices.par_iter().filter_map(|&i| column.to_f64(i)).sum();
                    Ok(sum.to_string())
                }
            },
            AggregateFunction::Average => {
                let values: Vec<f64> = indices
                    .par_iter()
                    .filter_map(|&i| column.to_f64(i))
                    .collect();
                if values.is_empty() {
                    Ok("NULL".to_string())
                } else {
                    let avg = values.iter().sum::<f64>() / values.len() as f64;
                    Ok(avg.to_string())
                }
            }
            AggregateFunction::Min | AggregateFunction::Max => {
                let mut values: Vec<String> = indices
                    .par_iter()
                    .filter_map(|&i| column.get_string(i))
                    .collect();
                if values.is_empty() {
                    Ok("NULL".to_string())
                } else {
                    values.sort();
                    match function {
                        AggregateFunction::Min => Ok(values.first().unwrap().clone()),
                        AggregateFunction::Max => Ok(values.last().unwrap().clone()),
                        _ => unreachable!(),
                    }
                }
            }
            AggregateFunction::CountDistinct => {
                let mut values: Vec<String> = indices
                    .par_iter()
                    .filter_map(|&i| column.get_string(i))
                    .collect();
                values.sort();
                values.dedup();
                Ok(values.len().to_string())
            }
            AggregateFunction::Median => {
                let mut values: Vec<f64> = indices
                    .par_iter()
                    .filter_map(|&i| column.to_f64(i))
                    .collect();
                if values.is_empty() {
                    Ok("NULL".to_string())
                } else {
                    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    let median = if values.len() % 2 == 0 {
                        (values[values.len() / 2 - 1] + values[values.len() / 2]) / 2.0
                    } else {
                        values[values.len() / 2]
                    };
                    Ok(median.to_string())
                }
            }
            AggregateFunction::StdDev | AggregateFunction::Variance => {
                let values: Vec<f64> = indices
                    .par_iter()
                    .filter_map(|&i| column.to_f64(i))
                    .collect();
                if values.len() < 2 {
                    Ok("NULL".to_string())
                } else {
                    let mean = values.iter().sum::<f64>() / values.len() as f64;
                    let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
                        / (values.len() - 1) as f64;
                    match function {
                        AggregateFunction::Variance => Ok(variance.to_string()),
                        AggregateFunction::StdDev => Ok(variance.sqrt().to_string()),
                        _ => unreachable!(),
                    }
                }
            }
        }
    }
    fn evaluate_expression(
        &self,
        dataframe: &DataFrame,
        expression: &ColumnExpression,
    ) -> Result<Column> {
        match expression {
            ColumnExpression::Column(name) => Ok(dataframe
                .get_column(name)
                .ok_or_else(|| DataHandlerError::ColumnNotFound(name.clone()))?
                .clone()),
            ColumnExpression::Constant(value) => {
                let constant_values: Vec<Option<String>> = (0..dataframe.row_count())
                    .map(|_| Some(self.filter_value_to_string(value)))
                    .collect();
                Column::from_strings(&constant_values, self.infer_type_from_filter_value(value))
            }
            ColumnExpression::Add(left, right) => {
                let left_col = self.evaluate_expression(dataframe, left)?;
                let right_col = self.evaluate_expression(dataframe, right)?;
                self.apply_numeric_operation(&left_col, &right_col, |a, b| a + b)
            }
            ColumnExpression::Subtract(left, right) => {
                let left_col = self.evaluate_expression(dataframe, left)?;
                let right_col = self.evaluate_expression(dataframe, right)?;
                self.apply_numeric_operation(&left_col, &right_col, |a, b| a - b)
            }
            ColumnExpression::Multiply(left, right) => {
                let left_col = self.evaluate_expression(dataframe, left)?;
                let right_col = self.evaluate_expression(dataframe, right)?;
                self.apply_numeric_operation(&left_col, &right_col, |a, b| a * b)
            }
            ColumnExpression::Divide(left, right) => {
                let left_col = self.evaluate_expression(dataframe, left)?;
                let right_col = self.evaluate_expression(dataframe, right)?;
                self.apply_numeric_operation(&left_col, &right_col, |a, b| {
                    if b != 0.0 {
                        a / b
                    } else {
                        f64::NAN
                    }
                })
            }
            ColumnExpression::Concat(left, right) => {
                let left_col = self.evaluate_expression(dataframe, left)?;
                let right_col = self.evaluate_expression(dataframe, right)?;
                self.apply_string_concat(&left_col, &right_col)
            }
        }
    }
    fn apply_numeric_operation<F>(&self, left: &Column, right: &Column, op: F) -> Result<Column>
    where
        F: Fn(f64, f64) -> f64 + Send + Sync,
    {
        if left.len() != right.len() {
            return Err(DataHandlerError::InvalidOperation(format!(
                "Column length mismatch: {} vs {}",
                left.len(),
                right.len()
            )));
        }
        let result: Result<Vec<Option<f64>>> = (0..left.len())
            .into_par_iter()
            .map(|i| {
                let left_val = left.to_f64(i);
                let right_val = right.to_f64(i);
                match (left_val, right_val) {
                    (Some(a), Some(b)) => Ok(Some(op(a, b))),
                    _ => Ok(None),
                }
            })
            .collect();
        Ok(Column::Float64(result?.into()))
    }
    fn apply_string_concat(&self, left: &Column, right: &Column) -> Result<Column> {
        if left.len() != right.len() {
            return Err(DataHandlerError::InvalidOperation(format!(
                "Column length mismatch: {} vs {}",
                left.len(),
                right.len()
            )));
        }
        let result: Vec<Option<Arc<str>>> = (0..left.len())
            .into_par_iter()
            .map(|i| match (left.get_string(i), right.get_string(i)) {
                (Some(a), Some(b)) => Some(Arc::from(format!("{a}{b}").as_str())),
                (Some(a), None) => Some(Arc::from(a.as_str())),
                (None, Some(b)) => Some(Arc::from(b.as_str())),
                (None, None) => None,
            })
            .collect();
        Ok(Column::String(result.into()))
    }
    fn filter_value_to_string(&self, value: &FilterValue) -> String {
        match value {
            FilterValue::Int64(v) => v.to_string(),
            FilterValue::Float64(v) => v.to_string(),
            FilterValue::String(v) => v.clone(),
            FilterValue::Boolean(v) => v.to_string(),
        }
    }
    fn infer_type_from_filter_value(&self, value: &FilterValue) -> DataType {
        match value {
            FilterValue::Int64(_) => DataType::Int64,
            FilterValue::Float64(_) => DataType::Float64,
            FilterValue::String(_) => DataType::String,
            FilterValue::Boolean(_) => DataType::Boolean,
        }
    }
}
impl Default for TransformationEngine {
    fn default() -> Self {
        Self::new()
    }
}
