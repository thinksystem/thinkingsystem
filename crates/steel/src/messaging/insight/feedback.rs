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

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::messaging::insight::{
    config::SecurityConfig, llm_integration::LlmClient, metrics::TrainingExample,
};

pub struct FeedbackLoop {
    llm_client: LlmClient,
    training_data_path: PathBuf,
}

impl FeedbackLoop {
    pub fn new(config: &SecurityConfig) -> Self {
        let state_dir = PathBuf::from(&config.paths.state_dir);
        Self {
            llm_client: LlmClient::new(config.llm.api_endpoint.clone()),
            training_data_path: state_dir.join(&config.paths.training_data_file),
        }
    }

    pub fn process_correction(
        &self,
        corrected_text: &str,
        was_false_positive: bool,
    ) -> Result<usize> {
        println!("Processing scribe correction for text: '{corrected_text}'");

        let new_examples = self
            .llm_client
            .generate_training_examples(corrected_text, was_false_positive)?;
        let count = new_examples.len();
        println!("LLM generated {count} new training examples.");

        self.append_training_data(&new_examples)?;

        Ok(count)
    }

    fn append_training_data(&self, examples: &[TrainingExample]) -> Result<()> {
        if let Some(parent) = self.training_data_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.training_data_path)?;

        for example in examples {
            let json_line = serde_json::to_string(example)?;
            writeln!(file, "{json_line}")?;
        }

        println!(
            "Successfully appended {} examples to {:?}",
            examples.len(),
            self.training_data_path
        );

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingStats {
    pub total_examples: usize,
    pub sensitive_examples: usize,
    pub non_sensitive_examples: usize,
    pub balance_ratio: f64,
}
