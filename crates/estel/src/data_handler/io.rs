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

use crate::data_handler::column::{ColumnBuilder, ColumnData};
use crate::data_handler::common::{
    DataHandlerError, DatasetId, DatasetMetadata, Result, DEFAULT_CHUNK_SIZE,
};
use crate::data_handler::dataframe::DataFrame;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
const MAX_LINE_LENGTH: usize = 10 * 1024 * 1024;
const MAX_FIELD_SIZE: usize = 1024 * 1024;
const MAX_FIELDS: usize = 10000;
#[derive(Debug)]
pub struct CsvReader {
    chunk_size: usize,
    has_headers: bool,
    delimiter: u8,
    quote_char: u8,
    escape_char: u8,
    max_line_length: usize,
    max_field_size: usize,
}
impl CsvReader {
    pub fn new() -> Self {
        Self {
            chunk_size: DEFAULT_CHUNK_SIZE,
            has_headers: true,
            delimiter: b',',
            quote_char: b'"',
            escape_char: b'"',
            max_line_length: MAX_LINE_LENGTH,
            max_field_size: MAX_FIELD_SIZE,
        }
    }
    pub fn with_chunk_size(mut self, chunk_size: usize) -> Self {
        self.chunk_size = chunk_size;
        self
    }
    pub fn with_headers(mut self, has_headers: bool) -> Self {
        self.has_headers = has_headers;
        self
    }
    pub fn with_delimiter(mut self, delimiter: u8) -> Self {
        self.delimiter = delimiter;
        self
    }
    pub fn with_limits(mut self, max_line_length: usize, max_field_size: usize) -> Self {
        self.max_line_length = max_line_length;
        self.max_field_size = max_field_size;
        self
    }
    pub fn read_file(&self, path: &Path, dataset_name: String) -> Result<DataFrame> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let headers = if self.has_headers {
            let mut header_line = String::new();
            reader.read_line(&mut header_line)?;
            self.parse_csv_line_safe(header_line.trim())?
        } else {
            let first_line = {
                let mut line = String::new();
                reader.read_line(&mut line)?;
                line
            };
            let field_count = self.parse_csv_line_safe(&first_line)?.len();
            (0..field_count).map(|i| format!("column_{i}")).collect()
        };
        let mut column_builders: HashMap<String, ColumnBuilder> = headers
            .iter()
            .map(|name| (name.clone(), ColumnBuilder::new()))
            .collect();
        let mut row_count = 0;
        let mut buffer = String::new();
        let mut chunk_lines = Vec::new();
        while reader.read_line(&mut buffer)? > 0 {
            if buffer.len() > self.max_line_length {
                return Err(DataHandlerError::SizeLimit(format!(
                    "Line length {} exceeds limit {}",
                    buffer.len(),
                    self.max_line_length
                )));
            }
            chunk_lines.push(buffer.trim().to_string());
            buffer.clear();
            if chunk_lines.len() >= self.chunk_size {
                self.process_chunk_safe(&chunk_lines, &headers, &mut column_builders)?;
                row_count += chunk_lines.len();
                chunk_lines.clear();
            }
        }
        if !chunk_lines.is_empty() {
            self.process_chunk_safe(&chunk_lines, &headers, &mut column_builders)?;
            row_count += chunk_lines.len();
        }
        let metadata = DatasetMetadata {
            id: DatasetId::new(),
            name: dataset_name,
            row_count,
            column_count: headers.len(),
            created_at: chrono::Utc::now(),
            source_path: Some(path.to_path_buf()),
        };
        let mut dataframe = DataFrame::new(metadata);
        for header in headers {
            let column = column_builders.remove(&header).unwrap().build()?;
            dataframe.add_column(header, column)?;
        }
        Ok(dataframe)
    }
    pub fn read_streaming<F>(&self, path: &Path, mut processor: F) -> Result<()>
    where
        F: FnMut(DataFrame) -> Result<()>,
    {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let headers = if self.has_headers {
            let mut header_line = String::new();
            reader.read_line(&mut header_line)?;
            self.parse_csv_line_safe(header_line.trim())?
        } else {
            let first_line = {
                let mut line = String::new();
                reader.read_line(&mut line)?;
                line
            };
            let field_count = self.parse_csv_line_safe(&first_line)?.len();
            (0..field_count).map(|i| format!("column_{i}")).collect()
        };
        let mut buffer = String::new();
        let mut chunk_lines = Vec::new();
        let mut chunk_id = 0;
        while reader.read_line(&mut buffer)? > 0 {
            if buffer.len() > self.max_line_length {
                return Err(DataHandlerError::SizeLimit(format!(
                    "Line length {} exceeds limit {}",
                    buffer.len(),
                    self.max_line_length
                )));
            }
            chunk_lines.push(buffer.trim().to_string());
            buffer.clear();
            if chunk_lines.len() >= self.chunk_size {
                let chunk_df = self.build_chunk_dataframe_safe(&chunk_lines, &headers, chunk_id)?;
                processor(chunk_df)?;
                chunk_lines.clear();
                chunk_id += 1;
            }
        }
        if !chunk_lines.is_empty() {
            let chunk_df = self.build_chunk_dataframe_safe(&chunk_lines, &headers, chunk_id)?;
            processor(chunk_df)?;
        }
        Ok(())
    }
    fn process_chunk_safe(
        &self,
        lines: &[String],
        headers: &[String],
        column_builders: &mut HashMap<String, ColumnBuilder>,
    ) -> Result<()> {
        for (line_num, line) in lines.iter().enumerate() {
            let fields = self.parse_csv_line_safe(line).map_err(|e| {
                DataHandlerError::CsvParseError(format!("Line {}: {}", line_num + 1, e))
            })?;
            if fields.len() > headers.len() {
                return Err(DataHandlerError::CsvParseError(format!(
                    "Line {}: Expected {} fields, got {}",
                    line_num + 1,
                    headers.len(),
                    fields.len()
                )));
            }
            for (i, field) in fields.iter().enumerate() {
                if let Some(header) = headers.get(i) {
                    let value = if field.trim().is_empty() {
                        None
                    } else {
                        Some(field.clone())
                    };
                    column_builders.get_mut(header).unwrap().push(value)?;
                }
            }
            for i in fields.len()..headers.len() {
                if let Some(header) = headers.get(i) {
                    column_builders.get_mut(header).unwrap().push(None)?;
                }
            }
        }
        Ok(())
    }
    fn build_chunk_dataframe_safe(
        &self,
        lines: &[String],
        headers: &[String],
        chunk_id: usize,
    ) -> Result<DataFrame> {
        let mut column_builders: HashMap<String, ColumnBuilder> = headers
            .iter()
            .map(|name| (name.clone(), ColumnBuilder::with_capacity(lines.len())))
            .collect();
        self.process_chunk_safe(lines, headers, &mut column_builders)?;
        let metadata = DatasetMetadata {
            id: DatasetId::new(),
            name: format!("chunk_{chunk_id}"),
            row_count: lines.len(),
            column_count: headers.len(),
            created_at: chrono::Utc::now(),
            source_path: None,
        };
        let mut dataframe = DataFrame::new(metadata);
        for header in headers {
            let column = column_builders.remove(header).unwrap().build()?;
            dataframe.add_column(header.clone(), column)?;
        }
        Ok(dataframe)
    }
    fn parse_csv_line_safe(&self, line: &str) -> Result<Vec<String>> {
        if line.len() > self.max_line_length {
            return Err(DataHandlerError::SizeLimit(format!(
                "Line length {} exceeds limit {}",
                line.len(),
                self.max_line_length
            )));
        }
        let estimated_fields = line.matches(self.delimiter as char).count() + 1;
        if estimated_fields > MAX_FIELDS {
            return Err(DataHandlerError::SizeLimit(format!(
                "Estimated field count {estimated_fields} exceeds limit {MAX_FIELDS}"
            )));
        }
        let mut fields = Vec::with_capacity(std::cmp::min(estimated_fields, MAX_FIELDS));
        let mut current_field = String::new();
        let mut in_quotes = false;
        let mut chars = line.chars().peekable();
        let mut field_char_count = 0;
        while let Some(ch) = chars.next() {
            field_char_count += 1;
            if field_char_count > self.max_field_size {
                return Err(DataHandlerError::SizeLimit(format!(
                    "Field size exceeds limit {}",
                    self.max_field_size
                )));
            }
            match ch {
                ch if ch as u8 == self.quote_char => {
                    if in_quotes && chars.peek() == Some(&(self.escape_char as char)) {
                        current_field.push(self.quote_char as char);
                        chars.next();
                    } else {
                        in_quotes = !in_quotes;
                    }
                }
                ch if ch as u8 == self.delimiter && !in_quotes => {
                    fields.push(current_field.trim().to_string());
                    current_field.clear();
                    field_char_count = 0;
                    if fields.len() >= MAX_FIELDS {
                        return Err(DataHandlerError::SizeLimit(format!(
                            "Field count exceeds limit {MAX_FIELDS}"
                        )));
                    }
                }
                ch => {
                    current_field.push(ch);
                }
            }
        }
        fields.push(current_field.trim().to_string());
        Ok(fields)
    }
}
impl Default for CsvReader {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Debug)]
pub struct CsvWriter {
    delimiter: u8,
    quote_all: bool,
    quote_char: u8,
    _escape_char: u8,
    buffer_size: usize,
}
impl CsvWriter {
    pub fn new() -> Self {
        Self {
            delimiter: b',',
            quote_all: false,
            quote_char: b'"',
            _escape_char: b'"',
            buffer_size: 64 * 1024,
        }
    }
    pub fn with_delimiter(mut self, delimiter: u8) -> Self {
        self.delimiter = delimiter;
        self
    }
    pub fn with_quote_all(mut self, quote_all: bool) -> Self {
        self.quote_all = quote_all;
        self
    }
    pub fn with_buffer_size(mut self, buffer_size: usize) -> Self {
        self.buffer_size = buffer_size;
        self
    }
    pub fn write_file(&self, dataframe: &DataFrame, path: &Path) -> Result<()> {
        let file = File::create(path)?;
        let mut writer = BufWriter::with_capacity(self.buffer_size, file);
        let delimiter_str = self.delimiter as char;
        let header_line = dataframe.column_names().join(&delimiter_str.to_string());
        writeln!(writer, "{header_line}")?;
        const WRITE_CHUNK_SIZE: usize = 1000;
        for chunk_start in (0..dataframe.row_count()).step_by(WRITE_CHUNK_SIZE) {
            let chunk_end = std::cmp::min(chunk_start + WRITE_CHUNK_SIZE, dataframe.row_count());
            for i in chunk_start..chunk_end {
                let row_values: Result<Vec<String>> = dataframe
                    .column_names()
                    .iter()
                    .map(|col_name| {
                        let value = dataframe
                            .get_column(col_name)
                            .and_then(|col| col.get_string(i))
                            .unwrap_or_default();
                        self.escape_field(&value)
                    })
                    .collect();
                writeln!(writer, "{}", row_values?.join(&delimiter_str.to_string()))?;
            }
        }
        writer.flush()?;
        Ok(())
    }
    fn escape_field(&self, value: &str) -> Result<String> {
        if value.len() > MAX_FIELD_SIZE {
            return Err(DataHandlerError::SizeLimit(format!(
                "Field size {} exceeds limit {}",
                value.len(),
                MAX_FIELD_SIZE
            )));
        }
        let delimiter_char = self.delimiter as char;
        let quote_char = self.quote_char as char;
        if self.quote_all
            || value.contains(delimiter_char)
            || value.contains(quote_char)
            || value.contains('\n')
            || value.contains('\r')
        {
            let escaped = value.replace(quote_char, &format!("{quote_char}{quote_char}"));
            Ok(format!("{quote_char}{escaped}{quote_char}"))
        } else {
            Ok(value.to_string())
        }
    }
}
impl Default for CsvWriter {
    fn default() -> Self {
        Self::new()
    }
}
pub fn infer_schema_from_sample(
    sample_data: &[Vec<String>],
) -> Vec<crate::data_handler::common::DataType> {
    if sample_data.is_empty() {
        return Vec::new();
    }
    let num_columns = sample_data[0].len();
    let mut inferred_types = vec![crate::data_handler::common::DataType::String; num_columns];
    for (col_idx, ty) in inferred_types.iter_mut().enumerate().take(num_columns) {
        let mut all_int = true;
        let mut all_float = true;
        let mut all_bool = true;
        let mut sample_count = 0;
        for row in sample_data.iter().take(1000) {
            if let Some(value) = row.get(col_idx) {
                if !value.trim().is_empty() {
                    sample_count += 1;
                    if value.parse::<i64>().is_err() {
                        all_int = false;
                    }
                    if value.parse::<f64>().is_err() {
                        all_float = false;
                    }
                    if !matches!(
                        value.to_lowercase().as_str(),
                        "true" | "false" | "t" | "f" | "1" | "0" | "yes" | "no" | "y" | "n"
                    ) {
                        all_bool = false;
                    }
                    if !all_int && !all_float && !all_bool {
                        break;
                    }
                }
            }
        }
        if sample_count >= 10 {
            if all_int {
                *ty = crate::data_handler::common::DataType::Int64;
            } else if all_float {
                *ty = crate::data_handler::common::DataType::Float64;
            } else if all_bool {
                *ty = crate::data_handler::common::DataType::Boolean;
            }
        }
    }
    inferred_types
}
