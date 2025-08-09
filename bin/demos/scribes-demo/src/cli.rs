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

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "scribes-demo")]
#[command(
    about = "A comprehensive demonstration of the STELE cognitive system with LLM integration"
)]
#[command(version)]
pub struct Args {
    #[arg(long, help = "Enable trace-level logging for maximum verbosity")]
    pub trace: bool,

    #[arg(long, value_enum, help = "Set the logging level")]
    pub log_level: Option<LogLevel>,

    #[arg(long, help = "Run with egui GUI visualisation of SCRIBE operations")]
    pub gui: bool,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    }
}
