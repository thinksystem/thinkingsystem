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
#[command(
    name = "flows-function-demo",
    version,
    about = "Plan / IR driven dynamic function + flow demo (no hardcoded algorithm logic)"
)]
pub struct Args {
    #[arg(
        long = "plan-file",
        help = "Path to existing plan JSON (omit when using --llm-plan)"
    )]
    pub plan_file: Option<String>,
    #[arg(
        long = "offline",
        default_value_t = false,
        help = "Disable LLM usage; requires --plan-file; skips plan and WAT generation/repair"
    )]
    pub offline: bool,
    #[arg(
        long = "llm-plan",
        default_value_t = false,
        help = "Generate plan JSON via LLM instead of providing --plan-file"
    )]
    pub llm_plan: bool,
    #[arg(
        long = "persist-plan",
        default_value_t = true,
        help = "Persist raw LLM plan JSON to artifacts directory when using --llm-plan"
    )]
    pub persist_plan: bool,
    #[arg(
        long = "artifacts-dir",
        default_value = "artifacts",
        help = "Directory to write generated plan artifacts"
    )]
    pub artifacts_dir: String,
    #[arg(
        long = "max-repair-attempts",
        default_value_t = 1,
        help = "How many validation repair attempts to run if LLM plan invalid"
    )]
    pub max_repair_attempts: u8,

    #[arg(
        long = "max-feasibility-attempts",
        default_value_t = 2u8,
        help = "Max LLM feasibility validation loops (with optional LLM-guided repair)"
    )]
    pub max_feasibility_attempts: u8,

    #[arg(
        long = "persist-feasibility",
        action,
        help = "Persist LLM feasibility assessment reports to artifacts directory"
    )]
    pub persist_feasibility: bool,
    #[arg(
        long = "max-wat-repairs",
        default_value_t = 4u8,
        help = "Maximum number of WAT assembly/registration repair attempts (LLM-assisted)."
    )]
    pub max_wat_repairs: u8,
    #[arg(long = "directive", default_value = "user directive")]
    pub directive: String,
    #[arg(
    long = "llm-rust-fn",
    alias = "llm-native-fn", 
    default_value_t = false,
    help = "Generate a Rust function via LLM, compile to a temporary dylib, load and execute via dynamic executor (DEFAULT when neither --llm-plan nor --plan-file provided)"
    )]
    pub llm_rust_fn: bool,
    #[arg(
        long = "persist-rust-fn",
        default_value_t = true,
        help = "Persist LLM-generated Rust function source + metadata artifacts when using --llm-rust-fn"
    )]
    pub persist_rust_fn: bool,
    #[arg(
        long = "max-plan-attempts",
        default_value_t = 10u16,
        help = "Maximum total LLM plan generation attempts before giving up (schema + repair cycles)."
    )]
    pub max_plan_attempts: u16,
    #[arg(
        long = "max-null-retries",
        default_value_t = 3u16,
        help = "If result deserializes to JSON null, automatically request a fresh LLM code generation up to this many additional attempts (null indicates serialization/path anomaly)."
    )]
    pub max_null_retries: u16,
    #[arg(
        long = "wasi",
        default_value_t = false,
        help = "Force dynamic WASI code generation/execution path (native is default). Requires binary compiled with dynamic-wasi feature."
    )]
    pub use_wasi: bool,
    #[arg(
        long = "debug",
        default_value_t = false,
        help = "Enable debug-level logging (tracing::Level::DEBUG)."
    )]
    pub debug: bool,
    #[arg(
        long = "ui",
        default_value_t = false,
        help = "Launch the interactive UI (requires 'ui' feature)"
    )]
    pub ui: bool,
}

impl Args {
    pub fn directive_with_hint(&self, hint: Option<String>) -> String {
        if let Some(h) = hint {
            if h.trim().is_empty() {
                return self.directive.clone();
            }
            format!("{}\n\nADAPTIVE_HINT: {}", self.directive, h)
        } else {
            self.directive.clone()
        }
    }
}
