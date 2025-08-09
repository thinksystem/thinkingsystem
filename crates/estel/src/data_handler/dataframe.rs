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
use crate::data_handler::common::{
    ColumnMetadata, DataHandlerError, DatasetId, DatasetMetadata, Result,
};
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
#[derive(Debug, Clone)]
pub struct DataFrame {
    pub columns: HashMap<String, Arc<Column>>,
    pub metadata: DatasetMetadata,
    column_order: Vec<String>,
}
#[derive(Debug)]
pub struct DataFrameView<'a> {
    source: &'a DataFrame,
    row_indices: Option<Arc<[usize]>>,
    column_selection: Option<Arc<[String]>>,
}
impl<'a> DataFrameView<'a> {
    pub fn new(source: &'a DataFrame) -> Self {
        Self {
            source,
            row_indices: None,
            column_selection: None,
        }
    }
    pub fn filter<P>(source: &'a DataFrame, predicate: P) -> Result<Self>
    where
        P: Fn(usize) -> bool + Send + Sync,
    {
        let indices: Vec<usize> = (0..source.row_count())
            .into_par_iter()
            .filter(|&i| predicate(i))
            .collect();
        Ok(Self {
            source,
            row_indices: Some(indices.into()),
            column_selection: None,
        })
    }
    pub fn select(mut self, columns: &[String]) -> Result<Self> {
        for col in columns {
            if !self.source.columns.contains_key(col) {
                return Err(DataHandlerError::ColumnNotFound(col.clone()));
            }
        }
        self.column_selection = Some(columns.to_vec().into());
        Ok(self)
    }
    pub fn row_count(&self) -> usize {
        self.row_indices
            .as_ref()
            .map_or(self.source.row_count(), |indices| indices.len())
    }
    pub fn column_count(&self) -> usize {
        self.column_selection
            .as_ref()
            .map_or(self.source.column_count(), |cols| cols.len())
    }
    pub fn collect(self) -> Result<DataFrame> {
        let mut new_df = DataFrame::new(DatasetMetadata {
            id: DatasetId::new(),
            name: format!("{}_view", self.source.metadata.name),
            row_count: self.row_count(),
            column_count: self.column_count(),
            created_at: chrono::Utc::now(),
            source_path: None,
        });
        let columns_to_process: &[String] = self
            .column_selection
            .as_ref()
            .map_or(self.source.column_order.as_slice(), |cols| cols.as_ref());
        for name in columns_to_process {
            let column = &self.source.columns[name];
            let new_column = if let Some(ref indices) = self.row_indices {
                column.select_rows(indices)?
            } else {
                column.as_ref().clone()
            };
            new_df.add_column(name.clone(), new_column)?;
        }
        Ok(new_df)
    }
    pub fn iter_rows(&self) -> DataFrameRowIterator<'a, '_> {
        DataFrameRowIterator::new(self)
    }
}
pub struct DataFrameRowIterator<'a, 'b> {
    view: &'b DataFrameView<'a>,
    current_row: usize,
}
impl<'a, 'b> DataFrameRowIterator<'a, 'b> {
    fn new(view: &'b DataFrameView<'a>) -> Self {
        Self {
            view,
            current_row: 0,
        }
    }
}
impl<'a, 'b> Iterator for DataFrameRowIterator<'a, 'b> {
    type Item = HashMap<String, Option<String>>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current_row >= self.view.row_count() {
            return None;
        }
        let actual_row_index = self
            .view
            .row_indices
            .as_ref()
            .map_or(self.current_row, |indices| indices[self.current_row]);
        let columns_to_process: &[String] = self
            .view
            .column_selection
            .as_ref()
            .map_or(self.view.source.column_order.as_slice(), |cols| {
                cols.as_ref()
            });
        let mut row = HashMap::new();
        for col_name in columns_to_process {
            if let Some(column) = self.view.source.columns.get(col_name) {
                row.insert(col_name.clone(), column.get_string(actual_row_index));
            }
        }
        self.current_row += 1;
        Some(row)
    }
}
impl DataFrame {
    pub fn new(metadata: DatasetMetadata) -> Self {
        Self {
            columns: HashMap::new(),
            metadata,
            column_order: Vec::new(),
        }
    }
    pub fn add_column(&mut self, name: String, column: Column) -> Result<()> {
        if !self.columns.is_empty() {
            let first_col_len = self.columns.values().next().unwrap().len();
            if column.len() != first_col_len {
                return Err(DataHandlerError::InvalidOperation(format!(
                    "Column length mismatch: expected {}, got {}",
                    first_col_len,
                    column.len()
                )));
            }
        }
        if !self.columns.contains_key(&name) {
            self.column_order.push(name.clone());
        }
        self.columns.insert(name, Arc::new(column));
        self.metadata.column_count = self.columns.len();
        if let Some(first_col) = self.columns.values().next() {
            self.metadata.row_count = first_col.len();
        }
        Ok(())
    }
    pub fn row_count(&self) -> usize {
        self.metadata.row_count
    }
    pub fn column_count(&self) -> usize {
        self.metadata.column_count
    }
    pub fn column_names(&self) -> &[String] {
        &self.column_order
    }
    pub fn get_column(&self, name: &str) -> Option<&Column> {
        self.columns.get(name).map(|arc| arc.as_ref())
    }
    pub fn column_metadata(&self) -> Vec<ColumnMetadata> {
        self.column_order
            .par_iter()
            .map(|name| {
                let column = &self.columns[name];
                ColumnMetadata {
                    name: name.clone(),
                    data_type: column.data_type(),
                    null_count: column.null_count(),
                    min_value: None,
                    max_value: None,
                    cardinality: None,
                }
            })
            .collect()
    }
    pub fn select(&self, column_names: &[String]) -> Result<DataFrame> {
        let view = DataFrameView::new(self).select(column_names)?;
        view.collect()
    }
    pub fn lazy_filter<P>(&self, predicate: P) -> Result<DataFrameView>
    where
        P: Fn(usize) -> bool + Send + Sync,
    {
        DataFrameView::filter(self, predicate)
    }
    pub fn lazy_select(&self, column_names: &[String]) -> Result<DataFrameView> {
        DataFrameView::new(self).select(column_names)
    }
    pub fn filter<P>(&self, predicate: P) -> Result<DataFrame>
    where
        P: Fn(usize) -> bool + Send + Sync,
    {
        self.lazy_filter(predicate)?.collect()
    }
    pub fn select_rows(&self, indices: &[usize]) -> Result<DataFrame> {
        let mut new_df = DataFrame::new(DatasetMetadata {
            id: DatasetId::new(),
            name: format!("{}_filtered", self.metadata.name),
            row_count: indices.len(),
            column_count: self.column_count(),
            created_at: chrono::Utc::now(),
            source_path: None,
        });
        for name in &self.column_order {
            let column = &self.columns[name];
            let new_column = column.select_rows(indices)?;
            new_df.add_column(name.clone(), new_column)?;
        }
        Ok(new_df)
    }
    pub fn print_sample(&self, limit: usize) {
        let sample_size = std::cmp::min(limit, self.row_count());
        let header = self.column_order.join(" | ");
        println!("{header}");
        println!("{}", "-".repeat(header.len()));
        for i in 0..sample_size {
            let row: Vec<String> = self
                .column_order
                .iter()
                .map(|name| {
                    self.columns[name]
                        .get_string(i)
                        .unwrap_or_else(|| "NULL".to_string())
                })
                .collect();
            println!("{}", row.join(" | "));
        }
        if self.row_count() > sample_size {
            println!("... ({} more rows)", self.row_count() - sample_size);
        }
    }
    pub fn chunk_iter(&self, chunk_size: usize) -> ChunkIterator {
        ChunkIterator::new(self, chunk_size)
    }
    pub fn process_chunks<F, R>(&self, chunk_size: usize, processor: F) -> Result<Vec<R>>
    where
        F: Fn(&DataFrame) -> Result<R> + Send + Sync,
        R: Send,
    {
        let chunks: Result<Vec<_>> = (0..self.row_count())
            .step_by(chunk_size)
            .collect::<Vec<_>>()
            .into_par_iter()
            .map(|start| {
                let end = std::cmp::min(start + chunk_size, self.row_count());
                let indices: Vec<usize> = (start..end).collect();
                let chunk = self.select_rows(&indices)?;
                processor(&chunk)
            })
            .collect();
        chunks
    }
    pub fn sort_by(&self, column_name: &str, ascending: bool) -> Result<DataFrame> {
        let column = self
            .get_column(column_name)
            .ok_or_else(|| DataHandlerError::ColumnNotFound(column_name.to_string()))?;
        let mut indices: Vec<usize> = (0..self.row_count()).collect();
        indices.par_sort_by(|&a, &b| {
            let val_a = column.get_string(a);
            let val_b = column.get_string(b);
            let cmp = match (val_a, val_b) {
                (Some(a), Some(b)) => a.cmp(&b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            };
            if ascending {
                cmp
            } else {
                cmp.reverse()
            }
        });
        self.select_rows(&indices)
    }
    pub fn drop_duplicates(&self, subset: Option<&[String]>) -> Result<DataFrame> {
        use std::collections::HashSet;
        let columns_to_check = subset.unwrap_or(&self.column_order);
        let mut seen = HashSet::new();
        let mut unique_indices = Vec::new();
        for i in 0..self.row_count() {
            let key: Vec<String> = columns_to_check
                .iter()
                .map(|col| {
                    self.get_column(col)
                        .and_then(|column| column.get_string(i))
                        .unwrap_or_else(|| "NULL".to_string())
                })
                .collect();
            if seen.insert(key) {
                unique_indices.push(i);
            }
        }
        self.select_rows(&unique_indices)
    }
}
pub struct ChunkIterator<'a> {
    dataframe: &'a DataFrame,
    chunk_size: usize,
    current_offset: usize,
}
impl<'a> ChunkIterator<'a> {
    fn new(dataframe: &'a DataFrame, chunk_size: usize) -> Self {
        Self {
            dataframe,
            chunk_size,
            current_offset: 0,
        }
    }
}
impl<'a> Iterator for ChunkIterator<'a> {
    type Item = Result<DataFrame>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current_offset >= self.dataframe.row_count() {
            return None;
        }
        let end = std::cmp::min(
            self.current_offset + self.chunk_size,
            self.dataframe.row_count(),
        );
        let indices: Vec<usize> = (self.current_offset..end).collect();
        let result = self.dataframe.select_rows(&indices);
        self.current_offset = end;
        Some(result)
    }
}
