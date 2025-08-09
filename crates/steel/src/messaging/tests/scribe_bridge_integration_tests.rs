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

use crate::messaging::insight::scribe_bridge::{
    BridgeMode, ScribeSecurityBridge, ScribeSecurityEvent, ScribeSecurityResponse,
};
use tempfile::tempdir;

#[test]
fn test_scribe_bridge_basic_functionality() {
    println!("\n=== Testing ScribeSecurityBridge Basic Functionality ===");

    let temp_dir = tempdir().unwrap();
    let mut bridge = ScribeSecurityBridge::new(temp_dir.path(), BridgeMode::SyntacticOnly);
    let analysis_event = ScribeSecurityEvent::AnalyseContent {
        content: "My email is john@example.com".to_string(),
    };

    let response = bridge.process_event(analysis_event);
    match response {
        ScribeSecurityResponse::Analysis(analysis_result) => {
            let (score, requires_review, tokens) = match analysis_result {
                crate::messaging::insight::security::MessageAnalysisResult::Syntactic(analysis) => {
                    (
                        analysis.overall_risk_score,
                        analysis.requires_scribes_review,
                        analysis.interesting_tokens.len(),
                    )
                }
                crate::messaging::insight::security::MessageAnalysisResult::Hybrid(analysis) => (
                    analysis.combined_risk_score,
                    analysis.requires_scribes_review,
                    analysis.syntactic_analysis.interesting_tokens.len(),
                ),
            };
            println!("Analysis successful: score={score:.3}, requires_review={requires_review}");
            assert!(score > 0.0);
            assert!(tokens > 0);
        }
        _ => panic!("Expected analysis response"),
    }

    let feedback_event = ScribeSecurityEvent::ProvideFeedback {
        original_content: "My email is john@example.com".to_string(),
        original_analysis: crate::messaging::insight::analysis::ContentAnalysis {
            overall_risk_score: 0.8,
            interesting_tokens: vec![("john@example.com".to_string(), 0.8)],
            requires_scribes_review: true,
        },
        scribe_judgment: true,
    };

    let response = bridge.process_event(feedback_event);
    match response {
        ScribeSecurityResponse::FeedbackProcessed {
            training_examples_generated,
        } => {
            println!(
                "Feedback processed: {training_examples_generated} training examples generated"
            );
            assert!(
                training_examples_generated > 0,
                "Should generate training examples even without LLM"
            );
        }
        _ => panic!("Expected feedback processed response"),
    }
}

#[test]
fn test_scribe_bridge_with_llm() {
    println!("\n=== Testing ScribeSecurityBridge with LLM Integration ===");

    let temp_dir = tempdir().unwrap();
    let mut bridge = ScribeSecurityBridge::new(temp_dir.path(), BridgeMode::LlmEnhanced);

    let analysis_event = ScribeSecurityEvent::AnalyseContent {
        content: "Build ID: build-123@internal.server".to_string(),
    };

    let response = bridge.process_event(analysis_event);
    match response {
        ScribeSecurityResponse::Analysis(analysis_result) => {
            let (score, requires_review) = match analysis_result {
                crate::messaging::insight::security::MessageAnalysisResult::Syntactic(analysis) => {
                    (
                        analysis.overall_risk_score,
                        analysis.requires_scribes_review,
                    )
                }
                crate::messaging::insight::security::MessageAnalysisResult::Hybrid(analysis) => (
                    analysis.combined_risk_score,
                    analysis.requires_scribes_review,
                ),
            };
            println!("LLM-enhanced analysis: score={score:.3}, requires_review={requires_review}");

            assert!(score > 0.0);
        }
        _ => panic!("Expected analysis response"),
    }

    let feedback_event = ScribeSecurityEvent::ProvideFeedback {
        original_content: "Build ID: build-123@internal.server".to_string(),
        original_analysis: crate::messaging::insight::analysis::ContentAnalysis {
            overall_risk_score: 0.5,
            interesting_tokens: vec![("build-123@internal.server".to_string(), 0.5)],
            requires_scribes_review: false,
        },
        scribe_judgment: false,
    };

    let response = bridge.process_event(feedback_event);
    match response {
        ScribeSecurityResponse::FeedbackProcessed {
            training_examples_generated,
        } => {
            println!(
                "LLM feedback processed: {training_examples_generated} training examples generated"
            );
            assert_eq!(training_examples_generated, 2);
        }
        _ => panic!("Expected feedback processed response"),
    }
}

#[test]
fn test_scribe_bridge_with_hybrid_analysis() {
    println!("\n=== Testing ScribeSecurityBridge with Hybrid Analysis (Syntactic + NER) ===");

    let temp_dir = tempdir().unwrap();
    let mut bridge = ScribeSecurityBridge::new(temp_dir.path(), BridgeMode::Hybrid);

    let analysis_event = ScribeSecurityEvent::AnalyseContent {
        content: "Contact john.doe@company.com or call 555-123-4567 for assistance.".to_string(),
    };

    let response = bridge.process_event(analysis_event);
    match response {
        ScribeSecurityResponse::Analysis(analysis_result) => match analysis_result {
            crate::messaging::insight::security::MessageAnalysisResult::Hybrid(analysis) => {
                println!(
                        "Hybrid analysis: combined_score={:.3}, syntactic_score={:.3}, ner_score={:.3}, requires_review={}",
                        analysis.combined_risk_score,
                        analysis.syntactic_analysis.overall_risk_score,
                        analysis.ner_analysis.overall_ner_score,
                        analysis.requires_scribes_review
                    );

                assert!(analysis.combined_risk_score > 0.0);
                assert!(analysis.syntactic_analysis.overall_risk_score > 0.0);

                for entity in &analysis.ner_analysis.entities {
                    println!(
                        "  NER Entity: {} ({}) - confidence: {:.3}, risk: {:.3}",
                        entity.text, entity.label, entity.confidence, entity.risk_score
                    );
                }

                for (token, score) in &analysis.syntactic_analysis.interesting_tokens {
                    println!("  Syntactic Token: {token} - score: {score:.3}");
                }
            }
            crate::messaging::insight::security::MessageAnalysisResult::Syntactic(analysis) => {
                println!("Warning: Expected hybrid analysis but got syntactic (NER may not be available)");
                println!(
                    "Syntactic analysis: score={:.3}, requires_review={}",
                    analysis.overall_risk_score, analysis.requires_scribes_review
                );
                assert!(analysis.overall_risk_score > 0.0);
            }
        },
        _ => panic!("Expected analysis response"),
    }

    let feedback_event = ScribeSecurityEvent::ProvideFeedback {
        original_content: "Contact john.doe@company.com".to_string(),
        original_analysis: crate::messaging::insight::analysis::ContentAnalysis {
            overall_risk_score: 0.8,
            interesting_tokens: vec![("john.doe@company.com".to_string(), 0.8)],
            requires_scribes_review: true,
        },
        scribe_judgment: true,
    };

    let response = bridge.process_event(feedback_event);
    match response {
        ScribeSecurityResponse::FeedbackProcessed {
            training_examples_generated,
        } => {
            println!(
                "Hybrid feedback processed: {training_examples_generated} training examples generated"
            );
            assert!(training_examples_generated > 0);
        }
        _ => panic!("Expected feedback processed response"),
    }
}
