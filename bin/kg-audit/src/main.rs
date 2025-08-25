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


use stele::database::query_kg::QueryKgBuilder;
use tracing::{info, Level};

fn main() {

    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_env_filter("info")
        .init();


    let docs_path = "config/instructions";
    info!("Building Knowledge Graph from {}", docs_path);

    let builder = QueryKgBuilder::new(docs_path);
    match builder.build_and_save_analysis("parsing_analysis.json") {
        Ok(kg) => {
            let summary = kg.parsing_registry.get_summary();
            println!("\n=== KG Audit Summary ===\n{summary}\n");

            let mut ops = kg.list_operator_names();
            ops.sort();
            let preview: Vec<_> = ops.into_iter().take(10).collect();
            println!("Operators seen (first 10): {preview:?}");
        }
        Err(e) => {
            eprintln!("KG build failed: {e}");
            std::process::exit(1);
        }
    }
}
