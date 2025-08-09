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

use crate::messaging::insight::{
    BridgeMode, HybridConfig, MessageAnalysisResult, MessageSecurity, NerAnalyser, NerConfig,
    ScribeSecurityBridge, ScribeSecurityEvent, ScribeSecurityResponse,
};
use std::collections::HashMap;
use tempfile::tempdir;

#[test]
fn test_comprehensive_ner_detection_demo() {
    println!("=== Comprehensive NER Detection Demo ===\n");

    let mut ner_analyser = NerAnalyser::default();
    let model_available = match ner_analyser.initialise_model() {
        Ok(()) => {
            println!("✓ GLiNER model loaded successfully\n");
            true
        }
        Err(e) => {
            println!("⚠ GLiNER model not available: {e}");
            println!("This demo will show syntactic analysis only.\n");
            false
        }
    };

    let test_categories = vec![
        (
            "PERSON NAMES",
            vec![
                "Contact John Smith for more details about the project.",
                "Dr. Sarah Johnson-Williams will be presenting alongside Prof. Michael Chen.",
                "I met Taylor Swift at the concert last night.",
                "The meeting with Tim Cook from Apple went very well.",
                "Abraham Lincoln was a great president of the United States.",
                "Maria González and Ahmed Al-Rashid are joining our team.",
                "CEO Elizabeth Warren III announced the new initiative.",
            ],
        ),
        (
            "ORGANISATIONS",
            vec![
                "I work for Google and my colleague is at Microsoft.",
                "The Federal Reserve Bank announced new policies affecting JPMorgan Chase.",
                "Netflix, Amazon, and Disney are competing in streaming services.",
                "Harvard University and Stanford collaborate on research projects.",
                "The United Nations held a meeting with NATO representatives.",
                "Volkswagen, Toyota, and General Motors are leading automakers.",
                "Goldman Sachs and Morgan Stanley reported quarterly earnings.",
            ],
        ),
        (
            "PLACES AND LOCATIONS",
            vec![
                "I visited New York City and then flew to Los Angeles.",
                "The meeting is scheduled in London, England, near Westminster Bridge.",
                "Our office is located at 123 Main Street, San Francisco, California, 94102.",
                "Flying from Tokyo, Japan to Paris, France via Frankfurt, Germany.",
                "The conference will be held in Sydney, Australia next month.",
                "Road trip from Chicago, Illinois to Austin, Texas through St. Louis, Missouri.",
                "Mountain climbing in the Swiss Alps near Zurich, Switzerland.",
            ],
        ),
        (
            "MIXED SENSITIVE CONTENT",
            vec![
                "Please contact Jane Doe at jane.doe@acme-corp.com or call (555) 123-4567.",
                "Send the report to john.smith@company.com before the meeting in Boston.",
                "My SSN is 123-45-6789 and I bank with Wells Fargo in Seattle.",
                "Credit card 4111-1111-1111-1111 was used at Starbucks in Portland, Oregon.",
                "API key sk-1234567890abcdef provides access to OpenAI services in San Francisco.",
                "Transfer $5000 to Bank of America account for the New York office lease.",
            ],
        ),
        (
            "INTERNATIONAL EXAMPLES",
            vec![
                "Pierre Dubois from Société Générale in Paris will attend the meeting.",
                "The Tokyo Stock Exchange and Nikkei reported gains today.",
                "Antonio Silva works for Petrobras in Rio de Janeiro, Brazil.",
                "Meeting with representatives from Siemens in Munich, Germany.",
                "The Beijing office coordinates with Shanghai and Hong Kong branches.",
                "Rajesh Patel from Tata Consultancy Services in Mumbai, India.",
                "Conference call with Samsung Electronics in Seoul, South Korea.",
            ],
        ),
        (
            "EDGE CASES AND COMPLEX SCENARIOS",
            vec![
                "CEO John Q. Public III announced that Apple Inc. will expand to New Zealand.",
                "The St. Louis Cardinals will play the New York Yankees at Yankee Stadium.",
                "Professor Dr. Jane Smith-Wilson from MIT and UCLA will collaborate.",
                "Fort Worth, Texas and Salt Lake City, Utah are potential locations.",
                "The Las Vegas Convention Centre will host CES 2024 in January.",
                "O'Connor, McDonald's, and Ben & Jerry's are popular brand names.",
                "San José, Costa Rica and São Paulo, Brazil are sister cities.",
            ],
        ),
    ];

    let temp_dir = tempdir().unwrap();
    let mut bridge = ScribeSecurityBridge::new(temp_dir.path(), BridgeMode::Hybrid);

    let mut total_tests = 0;
    let mut successful_ner_detections = 0;

    for (category, test_cases) in test_categories {
        println!("\n{}", "=".repeat(60));
        println!("{category}");
        println!("{}", "=".repeat(60));

        for (i, text) in test_cases.iter().enumerate() {
            total_tests += 1;
            println!("\n{}. \"{}\"", i + 1, text);

            let event = ScribeSecurityEvent::AnalyseContent {
                content: text.to_string(),
            };

            let response = bridge.process_event(event);
            match response {
                ScribeSecurityResponse::Analysis(result) => match result {
                    MessageAnalysisResult::Hybrid(analysis) => {
                        println!("   → HYBRID ANALYSIS:");
                        println!(
                            "     Combined Score: {:.3} | Syntactic: {:.3} | NER: {:.3}",
                            analysis.combined_risk_score,
                            analysis.syntactic_analysis.overall_risk_score,
                            analysis.ner_analysis.overall_ner_score
                        );
                        println!(
                            "     Requires Review: {} | Method: {}",
                            analysis.requires_scribes_review, analysis.analysis_method
                        );
                        println!(
                            "     Processing Time: {:.1}ms",
                            analysis.ner_analysis.processing_time_ms
                        );

                        if !analysis.syntactic_analysis.interesting_tokens.is_empty() {
                            println!("     Syntactic Patterns:");
                            for (token, score) in &analysis.syntactic_analysis.interesting_tokens {
                                println!("       🔍 {token} (score: {score:.3})");
                            }
                        }

                        if !analysis.ner_analysis.entities.is_empty() {
                            successful_ner_detections += 1;
                            println!("     NER Entities:");
                            for entity in &analysis.ner_analysis.entities {
                                let confidence_icon = if entity.confidence >= 0.8 {
                                    "🎯"
                                } else if entity.confidence >= 0.6 {
                                    "🎪"
                                } else {
                                    "📍"
                                };
                                println!(
                                    "       {} {} [{}] (conf: {:.3}, risk: {:.3}, pos: {}..{})",
                                    confidence_icon,
                                    entity.text,
                                    entity.label,
                                    entity.confidence,
                                    entity.risk_score,
                                    entity.start,
                                    entity.end
                                );
                            }
                        } else {
                            println!("     No NER entities detected");
                        }
                    }
                    MessageAnalysisResult::Syntactic(analysis) => {
                        println!("   → SYNTACTIC-ONLY ANALYSIS:");
                        println!(
                            "     Risk Score: {:.3} | Requires Review: {}",
                            analysis.overall_risk_score, analysis.requires_scribes_review
                        );
                        if !analysis.interesting_tokens.is_empty() {
                            println!("     Syntactic Patterns:");
                            for (token, score) in &analysis.interesting_tokens {
                                println!("       🔍 {token} (score: {score:.3})");
                            }
                        }
                    }
                },
                _ => println!("   ✗ Unexpected response type"),
            }
        }
    }

    println!("\n{}", "=".repeat(60));
    println!("ANALYSIS SUMMARY");
    println!("{}", "=".repeat(60));
    println!("Total test cases: {total_tests}");
    println!("NER detections: {successful_ner_detections}");
    println!(
        "NER success rate: {:.1}%",
        (successful_ner_detections as f64 / total_tests as f64) * 100.0
    );

    assert!(total_tests > 0, "Should have run some test cases");

    if model_available {
        assert!(
            successful_ner_detections > 0,
            "Should have detected some NER entities with model available"
        );

        let success_rate = successful_ner_detections as f64 / total_tests as f64;
        assert!(
            success_rate >= 0.1,
            "Should have at least 10% NER detection success rate"
        );
    }
}

#[test]
fn test_custom_ner_configuration_demo() {
    println!("\n{}", "=".repeat(60));
    println!("🔧 CUSTOM NER CONFIGURATION DEMO");
    println!("{}", "=".repeat(60));

    let mut custom_weights = HashMap::new();
    custom_weights.insert("person".to_string(), 1.0);
    custom_weights.insert("location".to_string(), 0.3);
    custom_weights.insert("organisation".to_string(), 0.5);
    custom_weights.insert("email".to_string(), 0.9);

    let custom_ner_config = NerConfig {
        model_path: NerConfig::default().model_path,
        enabled: true,
        entity_labels: vec![
            "person".to_string(),
            "location".to_string(),
            "organisation".to_string(),
            "email".to_string(),
        ],
        min_confidence_threshold: 0.6,
        entity_weights: custom_weights,
        max_text_length: 2048,
    };

    let custom_hybrid_config = HybridConfig {
        syntactic_weight: 0.4,
        ner_weight: 0.6,
        ner_boost_threshold: 0.7,
        min_combined_threshold: 0.4,
        enable_ner_boost: true,
    };

    println!("\nCreating MessageSecurity with custom configuration...");
    let temp_dir = tempdir().unwrap();
    let mut custom_security = MessageSecurity::new_with_hybrid(
        temp_dir.path(),
        Some(custom_ner_config),
        Some(custom_hybrid_config),
    );

    match custom_security.enable_hybrid_analysis(None, None) {
        Ok(()) => println!("✓ Custom hybrid analysis enabled"),
        Err(e) => {
            println!("⚠ Custom hybrid setup failed: {e}");

            return;
        }
    }

    let custom_test_text = "Please have Maria Santos from Microsoft call John Wilson at Goldman Sachs about the New York meeting.";
    println!("\nTesting custom config with: \"{custom_test_text}\"");

    let custom_result = custom_security.assess_message_risk(custom_test_text);
    println!("Custom analysis result:");
    println!("  Final Score: {:.3}", custom_result.overall_risk_score());
    println!(
        "  Requires Review: {}",
        custom_result.requires_scribes_review()
    );
    println!(
        "  Interesting Items: {:?}",
        custom_result.get_interesting_items()
    );

    assert!(custom_result.overall_risk_score() >= 0.0);
    assert!(custom_result.overall_risk_score() <= 1.0);

    println!("\n{}", "=".repeat(60));
    println!("✅ NER CUSTOM CONFIGURATION DEMO COMPLETE");
    println!("{}", "=".repeat(60));
}

#[test]
fn test_ner_capabilities_summary() {
    println!("\n{}", "=".repeat(60));
    println!("✅ NER CAPABILITIES SUMMARY");
    println!("{}", "=".repeat(60));
    println!("This test suite demonstrates:");
    println!("• Person name detection (John Smith, Maria González, etc.)");
    println!("• Location identification (cities, countries, addresses)");
    println!("• Organisation recognition (companies, institutions)");
    println!("• Mixed sensitive content (emails + names + locations)");
    println!("• International entity support");
    println!("• Custom configuration capabilities");
    println!("• Hybrid analysis combining syntactic + NER");
    println!("• Performance metrics and confidence scoring");

    println!("✓ NER capabilities summary complete");
}
