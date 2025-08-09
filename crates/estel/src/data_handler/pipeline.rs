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

use crate::data_handler::common::{Result, DEFAULT_CHUNK_SIZE};
use crate::data_handler::engine::DataEngine;
use crate::data_handler::transformation::{
    AggregateFunction, AggregateOperation, ColumnExpression, ComparisonOperator,
    CreateColumnOperation, GroupByOperation,
};
use std::path::Path;

pub struct DataPipeline {
    engine: DataEngine,
}

impl DataPipeline {
    pub fn new() -> Self {
        Self {
            engine: DataEngine::new().with_chunk_size(DEFAULT_CHUNK_SIZE),
        }
    }

    pub fn with_chunk_size(mut self, chunk_size: usize) -> Self {
        self.engine = self.engine.with_chunk_size(chunk_size);
        self
    }

    
    pub fn load_csv(&self, path: &Path, name: String) -> Result<String> {
        println!("Loading CSV: {} -> {}", path.display(), name);
        if !path.exists() {
            println!("File not found: {}", path.display());
            println!("Please provide a valid CSV file.");
            return Err("File not found".into());
        }

        let dataset_id = self.engine.ingest_csv(path, name)?;

        if let Some(dataset) = self.engine.get_dataset(&dataset_id)? {
            println!("Loaded dataset: {dataset_id}");
            println!("Rows: {}", dataset.metadata.row_count);
            println!("Columns: {}", dataset.metadata.column_count);
            let column_metadata = dataset.column_metadata();
            println!("Schema:");
            for col in &column_metadata {
                println!(
                    "{}: {:?} ({} nulls)",
                    col.name, col.data_type, col.null_count
                );
            }
        }

        Ok(dataset_id)
    }

    pub fn show_sample(&self, dataset_id: &str, limit: Option<usize>) -> Result<()> {
        let dataset = self
            .engine
            .get_dataset(dataset_id)?
            .ok_or("Dataset not found")?;
        println!("\nDataset Sample ({})", dataset.metadata.name);
        println!("{}", "=".repeat(50));
        dataset.print_sample(limit.unwrap_or(10));
        Ok(())
    }

    pub fn filter_high_values(
        &self,
        dataset_id: &str,
        column_name: &str,
        threshold: f64,
    ) -> Result<String> {
        println!("Filtering {column_name} > {threshold}");
        let filtered_id = self.engine.filter_numeric_column(
            dataset_id,
            column_name,
            ComparisonOperator::GreaterThan,
            threshold,
        )?;
        if let Ok(Some(df)) = self.engine.get_dataset(&filtered_id) {
            println!("Filtered dataset created: {filtered_id}");
            df.print_sample(10);
        }
        Ok(filtered_id)
    }

    pub fn group_and_aggregate(
        &self,
        dataset_id: &str,
        group_columns: &[String],
        agg_column: &str,
    ) -> Result<String> {
        println!("Grouping by {group_columns:?} and aggregating {agg_column}");
        let operation = GroupByOperation {
            group_columns: group_columns.to_vec(),
            aggregations: vec![
                AggregateOperation {
                    column: agg_column.to_string(),
                    function: AggregateFunction::Sum,
                    alias: Some(format!("{agg_column}_sum")),
                },
                AggregateOperation {
                    column: agg_column.to_string(),
                    function: AggregateFunction::Average,
                    alias: Some(format!("{agg_column}_avg")),
                },
                AggregateOperation {
                    column: agg_column.to_string(),
                    function: AggregateFunction::Count,
                    alias: Some("count".to_string()),
                },
            ],
        };
        let grouped_id = self.engine.group_by_dataset(dataset_id, operation)?;
        if let Ok(Some(df)) = self.engine.get_dataset(&grouped_id) {
            println!("Grouped dataset created: {grouped_id}");
            df.print_sample(10);
        }
        Ok(grouped_id)
    }

    pub fn create_calculated_column(
        &self,
        dataset_id: &str,
        new_column: &str,
        expression: ColumnExpression,
    ) -> Result<String> {
        println!("Creating calculated column: {new_column}");
        let operation = CreateColumnOperation {
            name: new_column.to_string(),
            expression,
        };
        let new_id = self.engine.create_column(dataset_id, operation)?;
        if let Ok(Some(df)) = self.engine.get_dataset(&new_id) {
            println!("Column added: {new_column}");
            df.print_sample(10);
        }
        Ok(new_id)
    }

    pub fn export(&self, dataset_id: &str, output_path: &Path) -> Result<()> {
        println!("Exporting to: {}", output_path.display());
        self.engine.export_csv(dataset_id, output_path)?;
        println!("Export completed");
        Ok(())
    }

    pub fn process_streaming<F>(&self, path: &Path, processor: F) -> Result<()>
    where
        F: FnMut(crate::data_handler::dataframe::DataFrame) -> Result<()> + Send + Sync,
    {
        println!("Processing {} in streaming mode", path.display());
        self.engine.ingest_csv_streaming(path, processor)
    }

    pub fn memory_report(&self) -> Result<()> {
        let usage = self.engine.memory_usage()?;
        let datasets = self.engine.list_datasets()?;
        println!("\nMemory Report:");
        println!("{}", "=".repeat(40));
        println!("Active datasets: {}", datasets.len());
        let total_memory: usize = usage.values().sum();
        println!(
            "Estimated memory usage: {} MB",
            total_memory / (1024 * 1024)
        );
        for dataset in datasets {
            let id_str = dataset.id.to_string();
            let size = usage.get(&id_str).copied().unwrap_or(0);
            println!(
                "{}: {} rows, {} MB",
                dataset.name,
                dataset.row_count,
                size / (1024 * 1024)
            );
        }
        Ok(())
    }

    pub fn cleanup(&self) -> Result<()> {
        println!("Cleaning up unused datasets...");
        self.engine.cleanup_unused()?;
        println!("Cleanup completed");
        Ok(())
    }
}

impl Default for DataPipeline {
    fn default() -> Self {
        Self::new()
    }
}

pub fn run_data_pipeline() -> Result<()> {
    let pipeline = DataPipeline::new();

    let csv_path = Path::new("data.csv");
    let dataset_id: String = match pipeline.load_csv(csv_path, "Sales Data".to_string()) {
        Ok(id) => id,
        Err(_) => {
            println!("Main data file not found, creating sample data...");
            create_sample_data()?;
            pipeline.load_csv(Path::new("sample_data.csv"), "Sample Data".to_string())?
        }
    };

    pipeline.show_sample(&dataset_id, Some(5))?;

    let dataset = pipeline
        .engine
        .get_dataset(&dataset_id)?
        .expect("Dataset not found");
    let column_metadata = dataset.column_metadata();

    let numeric_columns: Vec<&str> = column_metadata
        .iter()
        .filter(|col| {
            matches!(
                col.data_type,
                crate::data_handler::common::DataType::Int64
                    | crate::data_handler::common::DataType::Float64
            )
        })
        .map(|col| col.name.as_str())
        .collect();

    if let Some(&first_numeric_column) = numeric_columns.first() {
        let filtered_id = pipeline.filter_high_values(&dataset_id, first_numeric_column, 100.0)?;
        pipeline.show_sample(&filtered_id, Some(3))?;

        let string_columns: Vec<&str> = column_metadata
            .iter()
            .filter(|col| matches!(col.data_type, crate::data_handler::common::DataType::String))
            .map(|col| col.name.as_str())
            .collect();

        if let Some(&first_string_column) = string_columns.first() {
            let grouped_id = pipeline.group_and_aggregate(
                &filtered_id,
                &[first_string_column.to_string()],
                first_numeric_column,
            )?;
            pipeline.show_sample(&grouped_id, Some(5))?;

            let export_path = Path::new("pipeline_output.csv");
            pipeline.export(&grouped_id, export_path)?;
        }

        if numeric_columns.len() >= 2 {
            let calc_id = pipeline.create_calculated_column(
                &dataset_id,
                "calculated_field",
                ColumnExpression::Add(
                    Box::new(ColumnExpression::Column(numeric_columns[0].to_string())),
                    Box::new(ColumnExpression::Column(numeric_columns[1].to_string())),
                ),
            )?;
            pipeline.show_sample(&calc_id, Some(3))?;
        }
    }

    pipeline.memory_report()?;
    pipeline.cleanup()?;

    println!("\nPipeline completed successfully!");
    Ok(())
}

fn create_sample_data() -> Result<()> {
    use std::fs::File;
    use std::io::Write;

    let sample_csv = r#"Product,Sales,Region,Year,Active
Product A,150.5,North,2023,true
Product B,89.2,South,2023,true
Product C,200.0,East,2023,false
Product D,175.8,West,2023,true
Product E,95.5,North,2022,true
Product F,210.3,South,2022,true
Product G,165.7,East,2022,false
Product H,188.9,West,2022,true
Product I,145.2,North,2022,true
Product J,198.4,South,2021,false"#;

    let mut file = File::create("sample_data.csv")?;
    file.write_all(sample_csv.as_bytes())?;
    println!("Created sample_data.csv");
    Ok(())
}
