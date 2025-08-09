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

pub mod common;
pub mod column;
pub mod dataframe;
pub mod transformation;
pub mod io;
pub mod engine;
pub mod pipeline;
pub use common::{DataType, DatasetMetadata, ColumnMetadata, Result, DEFAULT_CHUNK_SIZE, DatasetId};
pub use column::{Column, ColumnBuilder, ColumnData};
pub use dataframe::{DataFrame, ChunkIterator};
pub use transformation::{
    TransformationEngine, FilterExpression, FilterCondition, FilterValue,
    ComparisonOperator, LogicalOperator, GroupByOperation, AggregateOperation,
    AggregateFunction, CreateColumnOperation, ColumnExpression
};
pub use io::{CsvReader, CsvWriter, infer_schema_from_sample};
pub use engine::{DataEngine, ConcurrentDataEngine};
pub use pipeline::{DataPipeline, run_data_pipeline};
pub fn new_engine() -> DataEngine {
    DataEngine::new()
}
pub fn new_concurrent_engine() -> ConcurrentDataEngine {
    ConcurrentDataEngine::new()
}
pub fn new_pipeline() -> DataPipeline {
    DataPipeline::new()
}
pub fn load_csv<P: AsRef<std::path::Path>>(path: P, name: String) -> Result<DataFrame> {
    let reader = CsvReader::new();
    reader.read_file(path.as_ref(), name)
}
pub fn export_csv<P: AsRef<std::path::Path>>(dataframe: &DataFrame, path: P) -> Result<()> {
    let writer = CsvWriter::new();
    writer.write_file(dataframe, path.as_ref())
}
pub fn process_csv_streaming<P, F>(path: P, processor: F) -> Result<()>
where
    P: AsRef<std::path::Path>,
    F: FnMut(DataFrame) -> Result<()> + Send + Sync,
{
    let reader = CsvReader::new();
    reader.read_streaming(path.as_ref(), processor)
}
pub fn main() -> Result<()> {
    println!("Starting high-performance data pipeline...");
    run_data_pipeline()
}
