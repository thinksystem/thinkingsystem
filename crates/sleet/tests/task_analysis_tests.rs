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

use sleet::tasks::analysis::ClassificationConfig;
use sleet::tasks::{
    create_task_from_input, CompositeAnalyser, ConfigurableTaskAnalyser, NerReadyAnalyser,
    SimpleTaskAnalyser, TaskAnalyser, TaskAnalysis, TaskSystem, TaskSystemConfig,
};

#[tokio::test]
async fn test_simple_task_analyser() {
    let simple_analyser = SimpleTaskAnalyser::new();
    let simple_task = create_task_from_input(
        "Build a high-performance REST API in Rust using Axum and PostgreSQL for an e-commerce platform",
        &simple_analyser,
    ).unwrap();

    assert!(!simple_task.title.is_empty());
    println!("Task type: {}", simple_task.task_type);
    println!("Task domain: {}", simple_task.domain);
    assert!(
        matches!(
            simple_task.task_type.as_str(),
            "development" | "technology" | "design"
        ),
        "unexpected task_type: {}",
        simple_task.task_type
    );
    assert!(
        matches!(
            simple_task.domain.as_str(),
            "technology" | "development" | "design"
        ),
        "unexpected domain: {}",
        simple_task.domain
    );
    assert!(!simple_task.collaboration_required);
    assert!(simple_task.max_duration_secs > 0);

    if let Some(analysis_data) = simple_task.metadata.get("analysis") {
        if let Ok(analysis) = serde_json::from_value::<TaskAnalysis>(analysis_data.clone()) {
            assert!(analysis.confidence > 0.0);
            assert!(!analysis.high_confidence_tags(0.6).is_empty());
            assert!(analysis.tags.contains_key("development"));
            assert!(analysis.tags.contains_key("technology"));
        }
    }
}

#[tokio::test]
async fn test_ner_ready_analyser() {
    let config = ClassificationConfig::default();
    let base_analyser = ConfigurableTaskAnalyser::new(config);
    let ner_analyser = NerReadyAnalyser::new(base_analyser);
    let task_description = "Create a machine learning pipeline for customer sentiment analysis using transformers and deploy to AWS";

    println!("Testing NER with input: '{task_description}'");

    let ner_task = create_task_from_input(task_description, &ner_analyser).unwrap();

    assert!(!ner_task.title.is_empty());
    assert!(ner_task.max_duration_secs > 0);

    if let Some(analysis_data) = ner_task.metadata.get("analysis") {
        if let Ok(analysis) = serde_json::from_value::<TaskAnalysis>(analysis_data.clone()) {
            println!("Analysis confidence: {}", analysis.confidence);

            println!("Entities detected ({} total):", analysis.entities.len());
            for (entity_type, entity_values) in &analysis.entities {
                println!("  {entity_type} -> {entity_values:?}");
            }

            println!("Tags generated ({} total):", analysis.tags.len());
            for (tag, score) in &analysis.tags {
                println!("  {tag} -> {score:.3}");
            }

            let high_conf_tags = analysis.high_confidence_tags(0.3);
            println!("High confidence tags (>0.3): {high_conf_tags:?}");

            assert!(analysis.confidence > 0.0);
            assert!(!high_conf_tags.is_empty());

            assert!(
                !analysis.entities.is_empty(),
                "NER should detect entities in the task description"
            );

            assert!(
                !analysis.tags.is_empty(),
                "Analysis should generate tags from entities or classification"
            );
        }
    }
}

#[tokio::test]
async fn test_composite_analyser() {
    let config = ClassificationConfig::default();
    let base_analyser = ConfigurableTaskAnalyser::new(config);
    let ner_analyser = NerReadyAnalyser::new(base_analyser);
    let composite_analyser =
        CompositeAnalyser::new(Box::new(ner_analyser), Box::new(SimpleTaskAnalyser::new()));

    let composite_task = create_task_from_input(
        "Design a scalable microservices architecture for a fintech startup with regulatory compliance requirements",
        &composite_analyser,
    ).unwrap();

    assert!(!composite_task.title.is_empty());
    assert!(composite_task.max_duration_secs > 0);

    if let Some(analysis_data) = composite_task.metadata.get("analysis") {
        if let Ok(analysis) = serde_json::from_value::<TaskAnalysis>(analysis_data.clone()) {
            assert!(analysis.confidence > 0.5);
            assert!(!analysis.required_skills.is_empty());
        }
    }
}

#[tokio::test]
async fn test_task_system_integration() {
    let mut task_system = TaskSystem::new(TaskSystemConfig::default());

    let simple_analyser = SimpleTaskAnalyser::new();
    let simple_task =
        create_task_from_input("Implement user authentication system", &simple_analyser).unwrap();

    let config = ClassificationConfig::default();
    let base_analyser = ConfigurableTaskAnalyser::new(config);
    let ner_analyser = NerReadyAnalyser::new(base_analyser);
    let ner_task = create_task_from_input(
        "Build machine learning model for fraud detection using neural networks",
        &ner_analyser,
    )
    .unwrap();

    let simple_config = sleet::tasks::TaskConfig {
        title: simple_task.title.clone(),
        description: simple_task.description.clone(),
        task_type: Some(simple_task.task_type.clone()),
        domain: Some(simple_task.domain.clone()),
        priority: simple_task.priority.clone(),
        max_duration_secs: Some(simple_task.max_duration_secs),
        collaboration_required: simple_task.collaboration_required,
        success_criteria: simple_task.success_criteria.clone(),
        resource_requirements: simple_task.resource_requirements.clone(),
        metadata: simple_task.metadata.clone(),
    };

    let ner_config = sleet::tasks::TaskConfig {
        title: ner_task.title.clone(),
        description: ner_task.description.clone(),
        task_type: Some(ner_task.task_type.clone()),
        domain: Some(ner_task.domain.clone()),
        priority: ner_task.priority.clone(),
        max_duration_secs: Some(ner_task.max_duration_secs),
        collaboration_required: ner_task.collaboration_required,
        success_criteria: ner_task.success_criteria.clone(),
        resource_requirements: ner_task.resource_requirements.clone(),
        metadata: ner_task.metadata.clone(),
    };

    let task_1 = task_system.create_task(simple_config).unwrap();
    let task_2 = task_system.create_task(ner_config).unwrap();

    assert!(!task_1.id.is_empty());
    assert!(!task_2.id.is_empty());
    assert_ne!(task_1.id, task_2.id);
}

#[tokio::test]
async fn test_analyser_trait_polymorphism() {
    let config = ClassificationConfig::default();
    let base_analyser = ConfigurableTaskAnalyser::new(config.clone());
    let ner_analyser = NerReadyAnalyser::new(base_analyser);

    let config2 = ClassificationConfig::default();
    let base_analyser2 = ConfigurableTaskAnalyser::new(config2);
    let ner_analyser2 = NerReadyAnalyser::new(base_analyser2);

    let analysers: Vec<Box<dyn sleet::tasks::TaskAnalyser>> = vec![
        Box::new(SimpleTaskAnalyser::new()),
        Box::new(ner_analyser),
        Box::new(CompositeAnalyser::new(
            Box::new(ner_analyser2),
            Box::new(SimpleTaskAnalyser::new()),
        )),
    ];

    let test_input = "Create a web application using JavaScript and React for project management";

    for analyser in analysers {
        let result = analyser.analyse(test_input).unwrap();

        assert!(!result.tags.is_empty());
        assert!(result.complexity_score >= 0.0 && result.complexity_score <= 1.0);
        assert!(result.confidence >= 0.0 && result.confidence <= 1.0);
        assert!(result.estimated_duration_secs > 0);

        assert!(!analyser.analyser_type().is_empty());
        assert!(analyser.is_available());
    }
}

#[tokio::test]
async fn test_task_analysis_methods() {
    let config = ClassificationConfig::default();
    let base_analyser = ConfigurableTaskAnalyser::new(config);
    let analyser = NerReadyAnalyser::new(base_analyser);
    let result = analyser
        .analyse("Develop a Rust API microservice with PostgreSQL database integration")
        .unwrap();

    let primary_type = result.primary_task_type();
    assert!(primary_type.is_some());

    assert!(
        !result.entities.is_empty(),
        "Should detect entities like 'Rust', 'API', 'PostgreSQL'"
    );

    let requires_collab_low = result.requires_collaboration(0.3);
    let requires_collab_high = result.requires_collaboration(0.9);
    assert!(requires_collab_low || !requires_collab_high);

    let high_conf_tags = result.high_confidence_tags(0.5);
    assert!(!high_conf_tags.is_empty());

    for (_, score) in high_conf_tags {
        assert!((0.5..=1.0).contains(&score));
    }
}
