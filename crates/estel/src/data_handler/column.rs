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

use crate::data_handler::common::{DataType, Result, DataHandlerError};
use rayon::prelude::*;
use std::sync::Arc;
const MAX_CHUNK_SIZE: usize = 50000;
const MAX_STRING_LENGTH: usize = 1024 * 1024;
pub trait ColumnData: Send + Sync + std::fmt::Debug {
    fn len(&self) -> usize;
    fn data_type(&self) -> DataType;
    fn null_count(&self) -> usize;
    fn get_string(&self, index: usize) -> Option<String>;
    fn to_f64(&self, index: usize) -> Option<f64>;
    fn clone_data(&self) -> Column;
    
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
#[derive(Debug, Clone)]
pub enum Column {
    Int64(Arc<[Option<i64>]>),
    Float64(Arc<[Option<f64>]>),
    String(Arc<[Option<Arc<str>>]>),
    Boolean(Arc<[Option<bool>]>),
    Chunked(ChunkedColumn),
}
#[derive(Debug, Clone)]
pub struct ChunkedColumn {
    chunks: Vec<Arc<Column>>,
    chunk_size: usize,
    total_length: usize,
    data_type: DataType,
}
impl ChunkedColumn {
    pub fn new(data_type: DataType, chunk_size: usize) -> Self {
        Self {
            chunks: Vec::new(),
            chunk_size: chunk_size.min(MAX_CHUNK_SIZE),
            total_length: 0,
            data_type,
        }
    }
    pub fn add_chunk(&mut self, chunk: Column) -> Result<()> {
        if chunk.data_type() != self.data_type {
            return Err(DataHandlerError::TypeMismatch(
                format!("Expected {:?}, got {:?}", self.data_type, chunk.data_type())
            ));
        }
        self.total_length += chunk.len();
        self.chunks.push(Arc::new(chunk));
        Ok(())
    }
    pub fn get_chunk_and_offset(&self, index: usize) -> Result<(usize, usize)> {
        if index >= self.total_length {
            return Err(DataHandlerError::OutOfBounds(index));
        }
        let chunk_idx = index / self.chunk_size;
        let offset = index % self.chunk_size;
        Ok((chunk_idx, offset))
    }
}
impl ColumnData for Column {
    fn len(&self) -> usize {
        match self {
            Column::Int64(data) => data.len(),
            Column::Float64(data) => data.len(),
            Column::String(data) => data.len(),
            Column::Boolean(data) => data.len(),
            Column::Chunked(chunked) => chunked.total_length,
        }
    }
    fn data_type(&self) -> DataType {
        match self {
            Column::Int64(_) => DataType::Int64,
            Column::Float64(_) => DataType::Float64,
            Column::String(_) => DataType::String,
            Column::Boolean(_) => DataType::Boolean,
            Column::Chunked(chunked) => chunked.data_type.clone(),
        }
    }
    fn null_count(&self) -> usize {
        match self {
            Column::Int64(data) => data.par_iter().filter(|v| v.is_none()).count(),
            Column::Float64(data) => data.par_iter().filter(|v| v.is_none()).count(),
            Column::String(data) => data.par_iter().filter(|v| v.is_none()).count(),
            Column::Boolean(data) => data.par_iter().filter(|v| v.is_none()).count(),
            Column::Chunked(chunked) => {
                chunked.chunks.par_iter()
                    .map(|chunk| chunk.null_count())
                    .sum()
            }
        }
    }
    fn get_string(&self, index: usize) -> Option<String> {
        match self {
            Column::Int64(data) => data.get(index)?.as_ref().map(|v| v.to_string()),
            Column::Float64(data) => data.get(index)?.as_ref().map(|v| v.to_string()),
            Column::String(data) => data.get(index)?.as_ref().map(|s| s.to_string()),
            Column::Boolean(data) => data.get(index)?.as_ref().map(|v| v.to_string()),
            Column::Chunked(chunked) => {
                let (chunk_idx, offset) = chunked.get_chunk_and_offset(index).ok()?;
                chunked.chunks.get(chunk_idx)?.get_string(offset)
            }
        }
    }
    fn to_f64(&self, index: usize) -> Option<f64> {
        match self {
            Column::Int64(data) => data.get(index).and_then(|opt| opt.map(|v| v as f64)),
            Column::Float64(data) => data.get(index).copied()?,
            Column::String(data) => {
                data.get(index).and_then(|opt| {
                    opt.as_ref().and_then(|s| s.parse::<f64>().ok())
                })
            }
            Column::Boolean(data) => {
                data.get(index).and_then(|opt| opt.map(|v| if v { 1.0 } else { 0.0 }))
            }
            Column::Chunked(chunked) => {
                let (chunk_idx, offset) = chunked.get_chunk_and_offset(index).ok()?;
                chunked.chunks.get(chunk_idx)?.to_f64(offset)
            }
        }
    }
    fn clone_data(&self) -> Column {
        self.clone()
    }
}
impl Column {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn from_strings(values: &[Option<String>], data_type: DataType) -> Result<Self> {
        let total_size: usize = values.iter()
            .flatten()
            .map(|s| s.len())
            .sum();
        if total_size > MAX_STRING_LENGTH * values.len() {
            return Err(DataHandlerError::SizeLimit(
                format!("Total string data size {total_size} exceeds limit")
            ));
        }
        if values.len() > MAX_CHUNK_SIZE {
            return Self::from_strings_chunked(values, data_type, MAX_CHUNK_SIZE);
        }
        Ok(match data_type {
            DataType::Int64 => {
                let parsed: Result<Vec<Option<i64>>> = values
                    .par_iter()
                    .map(|opt_str| {
                        match opt_str {
                            None => Ok(None),
                            Some(s) if s.trim().is_empty() => Ok(None),
                            Some(s) => s.parse::<i64>().map(Some).map_err(|e| e.into()),
                        }
                    })
                    .collect();
                Column::Int64(parsed?.into())
            }
            DataType::Float64 => {
                let parsed: Result<Vec<Option<f64>>> = values
                    .par_iter()
                    .map(|opt_str| {
                        match opt_str {
                            None => Ok(None),
                            Some(s) if s.trim().is_empty() => Ok(None),
                            Some(s) => s.parse::<f64>().map(Some).map_err(|e| e.into()),
                        }
                    })
                    .collect();
                Column::Float64(parsed?.into())
            }
            DataType::Boolean => {
                let parsed: Result<Vec<Option<bool>>> = values
                    .par_iter()
                    .map(|opt_str| {
                        match opt_str {
                            None => Ok(None),
                            Some(s) if s.trim().is_empty() => Ok(None),
                            Some(s) => {
                                let lower = s.to_lowercase();
                                match lower.as_str() {
                                    "true" | "t" | "1" | "yes" | "y" => Ok(Some(true)),
                                    "false" | "f" | "0" | "no" | "n" => Ok(Some(false)),
                                    _ => Err(DataHandlerError::ParseError(format!("Cannot parse '{s}' as boolean"))),
                                }
                            }
                        }
                    })
                    .collect();
                Column::Boolean(parsed?.into())
            }
            DataType::String => {
                let strings: Vec<Option<Arc<str>>> = values
                    .iter()
                    .map(|opt| {
                        opt.as_ref().map(|s| {
                            if s.len() > MAX_STRING_LENGTH {
                                Arc::from(&s[..MAX_STRING_LENGTH])
                            } else {
                                Arc::from(s.as_str())
                            }
                        })
                    })
                    .collect();
                Column::String(strings.into())
            }
        })
    }
    pub fn from_strings_chunked(values: &[Option<String>], data_type: DataType, chunk_size: usize) -> Result<Self> {
        let mut chunked = ChunkedColumn::new(data_type.clone(), chunk_size);
        for chunk in values.chunks(chunk_size) {
            let column = Column::from_strings(chunk, data_type.clone())?;
            chunked.add_chunk(column)?;
        }
        Ok(Column::Chunked(chunked))
    }
    pub fn select_rows(&self, indices: &[usize]) -> Result<Column> {
        match self {
            Column::Int64(data) => {
                let new_data: Result<Vec<Option<i64>>> = indices
                    .par_iter()
                    .map(|&i| {
                        if i >= data.len() {
                            Err(DataHandlerError::OutOfBounds(i))
                        } else {
                            Ok(data.get(i).copied().unwrap_or(None))
                        }
                    })
                    .collect();
                Ok(Column::Int64(new_data?.into()))
            }
            Column::Float64(data) => {
                let new_data: Result<Vec<Option<f64>>> = indices
                    .par_iter()
                    .map(|&i| {
                        if i >= data.len() {
                            Err(DataHandlerError::OutOfBounds(i))
                        } else {
                            Ok(data.get(i).copied().unwrap_or(None))
                        }
                    })
                    .collect();
                Ok(Column::Float64(new_data?.into()))
            }
            Column::String(data) => {
                let new_data: Result<Vec<Option<Arc<str>>>> = indices
                    .par_iter()
                    .map(|&i| {
                        if i >= data.len() {
                            Err(DataHandlerError::OutOfBounds(i))
                        } else {
                            Ok(data.get(i).cloned().unwrap_or(None))
                        }
                    })
                    .collect();
                Ok(Column::String(new_data?.into()))
            }
            Column::Boolean(data) => {
                let new_data: Result<Vec<Option<bool>>> = indices
                    .par_iter()
                    .map(|&i| {
                        if i >= data.len() {
                            Err(DataHandlerError::OutOfBounds(i))
                        } else {
                            Ok(data.get(i).copied().unwrap_or(None))
                        }
                    })
                    .collect();
                Ok(Column::Boolean(new_data?.into()))
            }
            Column::Chunked(chunked) => {
                let new_values: Result<Vec<Option<String>>> = indices
                    .par_iter()
                    .map(|&i| {
                        if i >= chunked.total_length {
                            Err(DataHandlerError::OutOfBounds(i))
                        } else {
                            Ok(self.get_string(i))
                        }
                    })
                    .collect();
                Column::from_strings(&new_values?, chunked.data_type.clone())
            }
        }
    }
}
#[derive(Debug)]
pub struct ColumnBuilder {
    values: Vec<Option<String>>,
    inferred_type: Option<DataType>,
    use_chunked: bool,
    chunk_size: usize,
}
impl ColumnBuilder {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            inferred_type: None,
            use_chunked: false,
            chunk_size: MAX_CHUNK_SIZE,
        }
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            values: Vec::with_capacity(capacity),
            inferred_type: None,
            use_chunked: capacity > MAX_CHUNK_SIZE,
            chunk_size: MAX_CHUNK_SIZE,
        }
    }
    pub fn with_chunked(mut self, chunked: bool, chunk_size: usize) -> Self {
        self.use_chunked = chunked;
        self.chunk_size = chunk_size.min(MAX_CHUNK_SIZE);
        self
    }
    pub fn push(&mut self, value: Option<String>) -> Result<()> {
        if let Some(ref s) = value {
            if s.len() > MAX_STRING_LENGTH {
                return Err(DataHandlerError::SizeLimit(
                    format!("String length {} exceeds limit {}", s.len(), MAX_STRING_LENGTH)
                ));
            }
        }
        if self.inferred_type.is_none() && value.is_some() {
            self.inferred_type = Some(Self::infer_type(value.as_ref().unwrap()));
        }
        self.values.push(value);
        Ok(())
    }
    pub fn build(self) -> Result<Column> {
        let data_type = self.inferred_type.unwrap_or(DataType::String);
        if self.use_chunked || self.values.len() > MAX_CHUNK_SIZE {
            Column::from_strings_chunked(&self.values, data_type, self.chunk_size)
        } else {
            Column::from_strings(&self.values, data_type)
        }
    }
    fn infer_type(sample: &str) -> DataType {
        if sample.parse::<i64>().is_ok() {
            DataType::Int64
        } else if sample.parse::<f64>().is_ok() {
            DataType::Float64
        } else if matches!(sample.to_lowercase().as_str(), "true" | "false" | "t" | "f" | "1" | "0" | "yes" | "no" | "y" | "n") {
            DataType::Boolean
        } else {
            DataType::String
        }
    }
}
impl Default for ColumnBuilder {
    fn default() -> Self {
        Self::new()
    }
}
