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

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::messaging::insight::{
    analysis::{ContentAnalyser, ContentAnalysis},
    config::SecurityConfig,
    distribution::ScoreDistribution,
    feedback::FeedbackLoop,
    hybrid_analyser::HybridContentAnalyser,
    llm_integration::HybridAnalyser,
    security::MessageAnalysisResult,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScribeSecurityEvent {
    AnalyseContent {
        content: String,
    },
    ProvideFeedback {
        original_content: String,
        original_analysis: ContentAnalysis,
        scribe_judgment: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScribeSecurityResponse {
    Analysis(MessageAnalysisResult),
    FeedbackProcessed { training_examples_generated: usize },
    Error(String),
}

pub struct ScribeSecurityBridge {
    analyser: Box<dyn Analyser>,
    feedback_loop: FeedbackLoop,
    distribution: ScoreDistribution,
    state_path: PathBuf,
}

trait Analyser {
    fn analyse(
        &mut self,
        text: &str,
        distribution: &mut ScoreDistribution,
    ) -> MessageAnalysisResult;
}

impl Analyser for ContentAnalyser {
    fn analyse(
        &mut self,
        text: &str,
        distribution: &mut ScoreDistribution,
    ) -> MessageAnalysisResult {
        MessageAnalysisResult::Syntactic(ContentAnalyser::analyse(self, text, distribution))
    }
}

impl Analyser for HybridContentAnalyser {
    fn analyse(
        &mut self,
        text: &str,
        distribution: &mut ScoreDistribution,
    ) -> MessageAnalysisResult {
        match self.analyse_hybrid(text, distribution) {
            Ok(analysis) => MessageAnalysisResult::Hybrid(analysis),
            Err(e) => {
                tracing::warn!("Hybrid analysis failed, falling back to syntactic: {}", e);

                let temp_analyser = ContentAnalyser::default();
                MessageAnalysisResult::Syntactic(temp_analyser.analyse(text, distribution))
            }
        }
    }
}

impl Analyser for HybridAnalyser {
    fn analyse(
        &mut self,
        text: &str,
        distribution: &mut ScoreDistribution,
    ) -> MessageAnalysisResult {
        MessageAnalysisResult::Syntactic(HybridAnalyser::analyse(self, text, distribution))
    }
}

#[derive(Debug, Clone)]
pub enum BridgeMode {
    SyntacticOnly,
    Hybrid,
    LlmEnhanced,
}

impl ScribeSecurityBridge {
    pub fn new(state_dir: &Path, bridge_mode: BridgeMode) -> Self {
        let config = SecurityConfig::load_or_default();
        let state_path = state_dir.join(&config.paths.state_file);
        let distribution = ScoreDistribution::load_from_file(&state_path)
            .unwrap_or_else(|_| ScoreDistribution::default());

        let analyser: Box<dyn Analyser> = match bridge_mode {
            BridgeMode::SyntacticOnly => {
                println!("Bridge mode: Syntactic-only analysis.");
                Box::new(ContentAnalyser::new(config.to_scoring_config()))
            }
            BridgeMode::Hybrid => {
                println!("Bridge mode: Hybrid analysis (Syntactic + NER).");
                let mut hybrid_analyser = HybridContentAnalyser::from_security_config(&config);

                if let Err(e) = hybrid_analyser.initialise_ner() {
                    println!("Warning: Failed to initialise NER model: {e}. Some features may be limited.");
                }
                Box::new(hybrid_analyser)
            }
            BridgeMode::LlmEnhanced => {
                println!("Bridge mode: LLM-enhanced analysis.");
                Box::new(HybridAnalyser::from_config(&config))
            }
        };

        Self {
            analyser,
            feedback_loop: FeedbackLoop::new(&config),
            distribution,
            state_path,
        }
    }

    pub fn new_legacy(state_dir: &Path, enable_llm: bool) -> Self {
        let bridge_mode = if enable_llm {
            BridgeMode::LlmEnhanced
        } else {
            BridgeMode::SyntacticOnly
        };
        Self::new(state_dir, bridge_mode)
    }

    pub fn process_event(&mut self, event: ScribeSecurityEvent) -> ScribeSecurityResponse {
        match event {
            ScribeSecurityEvent::AnalyseContent { content } => {
                let analysis = self.analyser.analyse(&content, &mut self.distribution);

                if let Err(e) = self.distribution.save_to_file(&self.state_path) {
                    eprintln!("Failed to save score distribution: {e}");
                }
                ScribeSecurityResponse::Analysis(analysis)
            }
            ScribeSecurityEvent::ProvideFeedback {
                original_content,
                original_analysis,
                scribe_judgment,
            } => {
                let was_false_positive =
                    original_analysis.requires_scribes_review && !scribe_judgment;

                match self
                    .feedback_loop
                    .process_correction(&original_content, was_false_positive)
                {
                    Ok(count) => ScribeSecurityResponse::FeedbackProcessed {
                        training_examples_generated: count,
                    },
                    Err(e) => {
                        ScribeSecurityResponse::Error(format!("Feedback processing failed: {e}"))
                    }
                }
            }
        }
    }
}
