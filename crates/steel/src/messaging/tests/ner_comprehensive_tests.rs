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
    ner_analysis::{NerAnalyser, NerConfig},
    scribe_bridge::{
        BridgeMode, ScribeSecurityBridge, ScribeSecurityEvent, ScribeSecurityResponse,
    },
    security::MessageAnalysisResult,
};
use std::collections::HashMap;
use tempfile::tempdir;

struct NerTestCase {
    text: String,
    description: String,
    expected_entities: Vec<ExpectedEntity>,
}

struct ExpectedEntity {
    entity_type: String,
    text_contains: String,
    min_confidence: f64,
    min_risk_score: f64,
}

fn get_comprehensive_ner_test_cases() -> Vec<NerTestCase> {
    vec![

        NerTestCase {
            text: "Contact John Smith at the office tomorrow.".to_string(),
            description: "Simple person name detection".to_string(),
            expected_entities: vec![
                ExpectedEntity {
                    entity_type: "person".to_string(),
                    text_contains: "John Smith".to_string(),
                    min_confidence: 0.7,
                    min_risk_score: 0.5,
                }
            ],
        },
        NerTestCase {
            text: "Dr. Sarah Johnson-Williams will be presenting alongside Prof. Michael Chen.".to_string(),
            description: "Multiple person names with titles and hyphens".to_string(),
            expected_entities: vec![
                ExpectedEntity {
                    entity_type: "person".to_string(),
                    text_contains: "Sarah".to_string(),
                    min_confidence: 0.6,
                    min_risk_score: 0.4,
                },
                ExpectedEntity {
                    entity_type: "person".to_string(),
                    text_contains: "Michael".to_string(),
                    min_confidence: 0.6,
                    min_risk_score: 0.4,
                }
            ],
        },
        NerTestCase {
            text: "I visited New York City and then flew to Los Angeles for a conference.".to_string(),
            description: "Major city detection".to_string(),
            expected_entities: vec![
                ExpectedEntity {
                    entity_type: "location".to_string(),
                    text_contains: "New York".to_string(),
                    min_confidence: 0.8,
                    min_risk_score: 0.3,
                },
                ExpectedEntity {
                    entity_type: "location".to_string(),
                    text_contains: "Los Angeles".to_string(),
                    min_confidence: 0.8,
                    min_risk_score: 0.3,
                }
            ],
        },
        NerTestCase {
            text: "The meeting is scheduled in London, England, near Westminster Bridge.".to_string(),
            description: "International location with landmarks".to_string(),
            expected_entities: vec![
                ExpectedEntity {
                    entity_type: "location".to_string(),
                    text_contains: "London".to_string(),
                    min_confidence: 0.8,
                    min_risk_score: 0.3,
                },
                ExpectedEntity {
                    entity_type: "location".to_string(),
                    text_contains: "England".to_string(),
                    min_confidence: 0.7,
                    min_risk_score: 0.3,
                }
            ],
        },
        NerTestCase {
            text: "Our office is located at 123 Main Street, San Francisco, California, 94102.".to_string(),
            description: "Full address with city, state, and zip code".to_string(),
            expected_entities: vec![
                ExpectedEntity {
                    entity_type: "location".to_string(),
                    text_contains: "San Francisco".to_string(),
                    min_confidence: 0.8,
                    min_risk_score: 0.3,
                },
                ExpectedEntity {
                    entity_type: "location".to_string(),
                    text_contains: "California".to_string(),
                    min_confidence: 0.7,
                    min_risk_score: 0.3,
                }
            ],
        },


        NerTestCase {
            text: "I work for Google and my colleague is at Microsoft.".to_string(),
            description: "Tech company detection".to_string(),
            expected_entities: vec![
                ExpectedEntity {
                    entity_type: "organisation".to_string(),
                    text_contains: "Google".to_string(),
                    min_confidence: 0.8,
                    min_risk_score: 0.4,
                },
                ExpectedEntity {
                    entity_type: "organisation".to_string(),
                    text_contains: "Microsoft".to_string(),
                    min_confidence: 0.8,
                    min_risk_score: 0.4,
                }
            ],
        },
        NerTestCase {
            text: "The Federal Reserve Bank announced new policies, affecting JPMorgan Chase and Bank of America.".to_string(),
            description: "Financial institutions detection".to_string(),
            expected_entities: vec![
                ExpectedEntity {
                    entity_type: "organisation".to_string(),
                    text_contains: "Federal Reserve".to_string(),
                    min_confidence: 0.7,
                    min_risk_score: 0.4,
                },
                ExpectedEntity {
                    entity_type: "organisation".to_string(),
                    text_contains: "JPMorgan".to_string(),
                    min_confidence: 0.7,
                    min_risk_score: 0.4,
                }
            ],
        },


        NerTestCase {
            text: "Please contact Jane Doe at jane.doe@acme-corp.com or call her at (555) 123-4567.".to_string(),
            description: "Person name with email and phone (high sensitivity)".to_string(),
            expected_entities: vec![
                ExpectedEntity {
                    entity_type: "person".to_string(),
                    text_contains: "Jane Doe".to_string(),
                    min_confidence: 0.7,
                    min_risk_score: 0.5,
                },
                ExpectedEntity {
                    entity_type: "email".to_string(),
                    text_contains: "jane.doe@acme-corp.com".to_string(),
                    min_confidence: 0.8,
                    min_risk_score: 0.7,
                }
            ],
        },


        NerTestCase {
            text: "Abraham Lincoln was born in Kentucky and later moved to Illinois.".to_string(),
            description: "Historical figure with states".to_string(),
            expected_entities: vec![
                ExpectedEntity {
                    entity_type: "person".to_string(),
                    text_contains: "Abraham Lincoln".to_string(),
                    min_confidence: 0.8,
                    min_risk_score: 0.6,
                },
                ExpectedEntity {
                    entity_type: "location".to_string(),
                    text_contains: "Kentucky".to_string(),
                    min_confidence: 0.7,
                    min_risk_score: 0.3,
                },
                ExpectedEntity {
                    entity_type: "location".to_string(),
                    text_contains: "Illinois".to_string(),
                    min_confidence: 0.7,
                    min_risk_score: 0.3,
                }
            ],
        },


        NerTestCase {
            text: "Maria González lives in Madrid, Spain, and works for Banco Santander.".to_string(),
            description: "International names, places, and organisations".to_string(),
            expected_entities: vec![
                ExpectedEntity {
                    entity_type: "person".to_string(),
                    text_contains: "Maria".to_string(),
                    min_confidence: 0.6,
                    min_risk_score: 0.4,
                },
                ExpectedEntity {
                    entity_type: "location".to_string(),
                    text_contains: "Madrid".to_string(),
                    min_confidence: 0.8,
                    min_risk_score: 0.3,
                },
                ExpectedEntity {
                    entity_type: "location".to_string(),
                    text_contains: "Spain".to_string(),
                    min_confidence: 0.8,
                    min_risk_score: 0.3,
                },
                ExpectedEntity {
                    entity_type: "organisation".to_string(),
                    text_contains: "Santander".to_string(),
                    min_confidence: 0.7,
                    min_risk_score: 0.4,
                }
            ],
        },


        NerTestCase {
            text: "CEO John Q. Public III announced that Apple Inc. will expand to New Zealand.".to_string(),
            description: "Complex name with title, suffix, and corporate entity".to_string(),
            expected_entities: vec![
                ExpectedEntity {
                    entity_type: "person".to_string(),
                    text_contains: "John".to_string(),
                    min_confidence: 0.6,
                    min_risk_score: 0.4,
                },
                ExpectedEntity {
                    entity_type: "organisation".to_string(),
                    text_contains: "Apple".to_string(),
                    min_confidence: 0.8,
                    min_risk_score: 0.4,
                },
                ExpectedEntity {
                    entity_type: "location".to_string(),
                    text_contains: "New Zealand".to_string(),
                    min_confidence: 0.8,
                    min_risk_score: 0.3,
                }
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ner_analyser_comprehensive_entity_detection() {
        println!("\n=== Comprehensive NER Entity Detection Test ===");

        let mut analyser = NerAnalyser::default();

        match analyser.initialise_model() {
            Ok(()) => println!("✓ GLiNER model loaded successfully"),
            Err(e) => {
                println!("⚠ GLiNER model not available: {e}. Skipping NER-specific tests.");
                return;
            }
        }

        let test_cases = get_comprehensive_ner_test_cases();
        let mut passed_tests = 0;
        let mut failed_tests = 0;

        for (i, test_case) in test_cases.iter().enumerate() {
            println!("\n--- Test Case {}: {} ---", i + 1, test_case.description);
            println!("Text: \"{}\"", test_case.text);

            match analyser.analyse_text(&test_case.text) {
                Ok(result) => {
                    println!("Processing time: {:.1}ms", result.processing_time_ms);
                    println!("Overall NER score: {:.3}", result.overall_ner_score);
                    println!("Entities detected: {}", result.entities.len());

                    for entity in &result.entities {
                        println!(
                            "  - {} [{}]: confidence={:.3}, risk={:.3}, pos={}..{}",
                            entity.text,
                            entity.label,
                            entity.confidence,
                            entity.risk_score,
                            entity.start,
                            entity.end
                        );
                    }

                    let mut test_passed = true;
                    for expected in &test_case.expected_entities {
                        let found_entity = result.entities.iter().find(|e| {
                            e.label == expected.entity_type
                                && e.text.contains(&expected.text_contains)
                                && e.confidence >= expected.min_confidence
                                && e.risk_score >= expected.min_risk_score
                        });

                        match found_entity {
                            Some(entity) => {
                                println!(
                                    "  ✓ Expected {} '{}' found: {} (confidence: {:.3})",
                                    expected.entity_type,
                                    expected.text_contains,
                                    entity.text,
                                    entity.confidence
                                );
                            }
                            None => {
                                println!(
                                    "  ✗ Expected {} '{}' not found or doesn't meet criteria",
                                    expected.entity_type, expected.text_contains
                                );
                                test_passed = false;
                            }
                        }
                    }

                    if test_passed {
                        passed_tests += 1;
                        println!("→ Test PASSED");
                    } else {
                        failed_tests += 1;
                        println!("→ Test FAILED");
                    }
                }
                Err(e) => {
                    println!("✗ Analysis failed: {e}");
                    failed_tests += 1;
                }
            }
        }

        println!("\n=== Test Summary ===");
        println!("Passed: {passed_tests}");
        println!("Failed: {failed_tests}");
        println!("Total: {}", passed_tests + failed_tests);

        let success_rate = passed_tests as f64 / (passed_tests + failed_tests) as f64;
        println!("Success rate: {:.1}%", success_rate * 100.0);

        assert!(
            success_rate >= 0.7,
            "NER detection success rate too low: {:.1}%",
            success_rate * 100.0
        );
    }

    #[test]
    fn test_hybrid_analyser_with_ner_entities() {
        println!("\n=== Hybrid Analyser NER Integration Test ===");

        let temp_dir = tempdir().unwrap();
        let mut bridge = ScribeSecurityBridge::new(temp_dir.path(), BridgeMode::Hybrid);

        let test_scenarios = vec![
            (
                "Celebrity encounter at famous location",
                "I met Taylor Swift at Times Square in New York yesterday.",
                vec!["person", "location"],
            ),
            (
                "Business meeting with executives",
                "The meeting with Tim Cook from Apple is scheduled at the Marriott Hotel in Cupertino.",
                vec!["person", "organisation", "location"],
            ),
            (
                "International travel itinerary",
                "Flying from Tokyo, Japan to Paris, France via Lufthansa Airlines.",
                vec!["location", "organisation"],
            ),
            (
                "Academic conference details",
                "Professor Elena Rodriguez from Stanford University will speak at MIT.",
                vec!["person", "organisation"],
            ),
            (
                "Sports event information",
                "The Lakers will play against the Warriors at Madison Square Garden.",
                vec!["organisation", "location"],
            ),
        ];

        for (description, text, expected_entity_types) in test_scenarios {
            println!("\n--- Scenario: {description} ---");
            println!("Text: \"{text}\"");
            println!("Expected entity types: {expected_entity_types:?}");

            let event = ScribeSecurityEvent::AnalyseContent {
                content: text.to_string(),
            };

            let response = bridge.process_event(event);
            match response {
                ScribeSecurityResponse::Analysis(result) => match result {
                    MessageAnalysisResult::Hybrid(analysis) => {
                        println!("Combined score: {:.3}", analysis.combined_risk_score);
                        println!(
                            "  Syntactic score: {:.3}",
                            analysis.syntactic_analysis.overall_risk_score
                        );
                        println!(
                            "  NER score: {:.3}",
                            analysis.ner_analysis.overall_ner_score
                        );
                        println!("Analysis method: {}", analysis.analysis_method);

                        let detected_types: std::collections::HashSet<String> = analysis
                            .ner_analysis
                            .entities
                            .iter()
                            .map(|e| e.label.clone())
                            .collect();

                        println!("Detected entity types: {detected_types:?}");

                        for entity in &analysis.ner_analysis.entities {
                            println!(
                                "    - {} [{}]: confidence={:.3}, risk={:.3}",
                                entity.text, entity.label, entity.confidence, entity.risk_score
                            );
                        }

                        let found_expected = expected_entity_types
                            .iter()
                            .any(|expected_type| detected_types.contains(*expected_type));

                        if found_expected {
                            println!("✓ At least one expected entity type detected");
                        } else {
                            println!("⚠ No expected entity types detected (model limitations possible)");
                        }
                    }
                    MessageAnalysisResult::Syntactic(analysis) => {
                        println!("→ Fallback to syntactic analysis");
                        println!("Risk score: {:.3}", analysis.overall_risk_score);
                    }
                },
                _ => panic!("Expected analysis response"),
            }
        }
    }

    #[test]
    fn test_ner_performance_benchmarks() {
        println!("\n=== NER Performance Benchmark Test ===");

        let mut analyser = NerAnalyser::default();

        if analyser.initialise_model().is_err() {
            println!("⚠ GLiNER model not available. Skipping performance tests.");
            return;
        }

        let benchmark_texts = ["Short text with John Smith.".to_string(),
            "Medium length text with multiple entities like Jane Doe from Microsoft in Seattle, Washington, and Bob Johnson from Google in Mountain View, California.".to_string(),
            "Very long text with many entities: Alice Cooper from Apple in Cupertino works with Bob Dylan from Spotify in Stockholm, Sweden. They collaborate with Carol King from Netflix in Los Gatos and David Bowie from Amazon in Seattle. The project involves travelling to London, England, Paris, France, Tokyo, Japan, and Sydney, Australia for meetings with local partners.".repeat(3)];

        let text_lengths = ["Short", "Medium", "Long"];

        for (i, text) in benchmark_texts.iter().enumerate() {
            println!("\n--- {} Text Performance ---", text_lengths[i]);
            println!("Text length: {} characters", text.len());

            let start_time = std::time::Instant::now();
            match analyser.analyse_text(text) {
                Ok(result) => {
                    let total_time = start_time.elapsed().as_millis();
                    println!("Total processing time: {total_time}ms");
                    println!(
                        "Reported processing time: {:.1}ms",
                        result.processing_time_ms
                    );
                    println!("Entities detected: {}", result.entities.len());
                    println!("Overall NER score: {:.3}", result.overall_ner_score);
                    println!("Text truncated: {}", result.text_truncated);

                    assert!(
                        total_time < 5000,
                        "Processing took too long: {total_time}ms"
                    );
                    assert!(
                        result.entities.len() <= 50,
                        "Too many entities detected: {}",
                        result.entities.len()
                    );
                }
                Err(e) => {
                    println!("Analysis failed: {e}");
                    panic!("Performance test failed");
                }
            }
        }
    }

    #[test]
    fn test_ner_confidence_thresholds() {
        println!("\n=== NER Confidence Threshold Test ===");

        let confidence_thresholds = vec![0.3, 0.5, 0.7, 0.9];
        let test_text = "Meeting with Sarah Johnson from IBM in Chicago and Michael Chen from Oracle in San Francisco.";

        for threshold in confidence_thresholds {
            println!("\n--- Testing with confidence threshold: {threshold:.1} ---");

            let config = NerConfig {
                min_confidence_threshold: threshold,
                ..Default::default()
            };
            let mut analyser = NerAnalyser::new(config);

            if analyser.initialise_model().is_err() {
                println!("⚠ GLiNER model not available. Skipping confidence threshold tests.");
                return;
            }

            match analyser.analyse_text(test_text) {
                Ok(result) => {
                    println!("Entities detected: {}", result.entities.len());

                    for entity in &result.entities {
                        println!(
                            "  - {} [{}]: confidence={:.3}",
                            entity.text, entity.label, entity.confidence
                        );
                        assert!(
                            entity.confidence >= threshold,
                            "Entity confidence {:.3} below threshold {:.1}",
                            entity.confidence,
                            threshold
                        );
                    }

                    if threshold >= 0.7 {
                        println!(
                            "High threshold results in {} entities",
                            result.entities.len()
                        );
                    }
                }
                Err(e) => {
                    println!("Analysis failed: {e}");
                }
            }
        }
    }

    #[test]
    fn test_ner_entity_weight_customisation() {
        println!("\n=== NER Entity Weight Customisation Test ===");

        let test_text = "Contact person@company.com or call 555-123-4567. Meeting with John Smith at Google in NYC.";

        let mut custom_weights = HashMap::new();
        custom_weights.insert("person".to_string(), 1.0);
        custom_weights.insert("email".to_string(), 0.9);
        custom_weights.insert("phone".to_string(), 0.8);
        custom_weights.insert("organisation".to_string(), 0.3);
        custom_weights.insert("location".to_string(), 0.2);

        let config = NerConfig {
            entity_weights: custom_weights,
            ..Default::default()
        };
        let mut analyser = NerAnalyser::new(config);

        if analyser.initialise_model().is_err() {
            println!("⚠ GLiNER model not available. Skipping weight customisation tests.");
            return;
        }

        match analyser.analyse_text(test_text) {
            Ok(result) => {
                println!("Entities with custom weights:");

                for entity in &result.entities {
                    println!(
                        "  - {} [{}]: confidence={:.3}, risk_score={:.3}",
                        entity.text, entity.label, entity.confidence, entity.risk_score
                    );

                    match entity.label.as_str() {
                        "person" => assert!(
                            entity.risk_score >= 0.7,
                            "Person entities should have high risk scores"
                        ),
                        "location" => assert!(
                            entity.risk_score <= 0.3,
                            "Location entities should have low risk scores"
                        ),
                        _ => {}
                    }
                }

                println!("Overall NER score: {:.3}", result.overall_ner_score);
            }
            Err(e) => {
                println!("Analysis failed: {e}");
            }
        }
    }
}
