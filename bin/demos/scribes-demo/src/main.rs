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

#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::redundant_pattern_matching)]
#![allow(clippy::needless_range_loop)]

mod app;
mod cli;
mod data_loader;
mod demo_processor;
mod identity;
mod llm_logging;
mod local_llm_interface;
mod logging_adapter;
mod scenario_generator;
mod setup;
mod tests;
mod ui;

use app::{run_with_gui, run_without_gui};
use clap::Parser;
use cli::Args;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if args.gui {
        run_with_gui(args)
    } else {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(run_without_gui(args))
    }
}
