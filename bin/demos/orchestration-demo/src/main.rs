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

use serde_json::{json, Value};
use sleet::agents::{
    schemas::GenerationMethod, schemas::PerformanceMetrics, schemas::RuntimeCapabilities,
    schemas::TechnicalSkill, schemas::TrustLevel, Agent, AgentCapabilities, AgentMetadata,
    AgentStatus, AgentSystem, AgentSystemConfig,
};
use sleet::orchestration::{
    self,
    adapters::{AgentSelector, ExecutionStrategy, InteractionType},
    coordinator::{OrchestrationConfig, OrchestrationCoordinator},
    CompletionCriteria, ExecutionConfiguration, MergeStrategy, OrchestrationBlockDefinition,
    OrchestrationBlockType, OrchestrationFlowDefinition, ResourceRequirement, ResourceRequirements,
    RetryConfig, TaskDefinition, TaskExecutionConfig,
};
use sleet::runtime::ExecutionStatus;
use sleet::tasks::{
    ResourceRequirement as TaskResourceRequirement, TaskConfig, TaskPriority as TaskPriorityLevel,
    TaskSystem, TaskSystemConfig,
};
use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::Arc;

fn create_web_search_agent() -> Agent {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    Agent {
        id: "agent_web_search".to_string(),
        name: "Web Search Agent".to_string(),
        role: "Search Specialist".to_string(),
        specialisation: "Web Search and Information Retrieval".to_string(),
        capabilities: AgentCapabilities {
            personality_traits: vec!["thorough".to_string(), "analytical".to_string()],
            strengths: vec![
                "web_search".to_string(),
                "information_gathering".to_string(),
            ],
            approach_style: "systematic search methodology".to_string(),
            competitive_edge: "exceptional at finding relevant information quickly".to_string(),
            risk_tolerance: 5.0,
            collaboration_preference: "provides comprehensive research support".to_string(),
            technical_skills: vec![
                TechnicalSkill {
                    name: "Web Search".to_string(),
                    proficiency: 0.95,
                    experience_years: 5.0,
                    domains: vec!["Information Retrieval".to_string(), "Research".to_string()],
                },
                TechnicalSkill {
                    name: "web_search".to_string(),
                    proficiency: 0.95,
                    experience_years: 5.0,
                    domains: vec!["Information Retrieval".to_string(), "Research".to_string()],
                },
            ],
            performance_metrics: PerformanceMetrics {
                success_rate: 0.90,
                avg_completion_time: 15.0,
                quality_score: 0.88,
                collaboration_score: 0.85,
                tasks_completed: 150,
                innovation_score: 0.70,
            },
            runtime_capabilities: RuntimeCapabilities {
                can_create_subtasks: false,
                can_modify_workflow: false,
                can_access_external_apis: true,
                can_delegate_tasks: false,
                max_concurrent_executions: 1,
                trusted_execution_level: TrustLevel::Standard,
                ffi_permissions: vec!["http_request".to_string()],
                gas_limit: 10000,
                execution_timeout_secs: 60,
            },
        },
        status: AgentStatus::Available,
        metadata: AgentMetadata {
            created_at: now,
            updated_at: now,
            version: 1,
            created_by: "demo-system".to_string(),
            generation_method: GenerationMethod::Manual {
                creator: "demo-system".to_string(),
            },
            tags: vec!["search".to_string(), "research".to_string()],
            performance_tracking: true,
        },
        custom_properties: HashMap::new(),
    }
}

fn create_creative_agent() -> Agent {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    Agent {
        id: "agent_creative_alpha".to_string(),
        name: "Creative Strategy Agent".to_string(),
        role: "Creative Strategist".to_string(),
        specialisation: "Creative Strategy and Innovation".to_string(),
        capabilities: AgentCapabilities {
            personality_traits: vec![
                "creative".to_string(),
                "visionary".to_string(),
                "innovative".to_string(),
            ],
            strengths: vec![
                "creative_ideation".to_string(),
                "strategic_thinking".to_string(),
            ],
            approach_style: "expansive and opportunity-focused".to_string(),
            competitive_edge: "transforms challenges into growth opportunities".to_string(),
            risk_tolerance: 8.0,
            collaboration_preference: "inspires through innovative thinking".to_string(),
            technical_skills: vec![
                TechnicalSkill {
                    name: "Strategic Planning".to_string(),
                    proficiency: 0.92,
                    experience_years: 7.0,
                    domains: vec!["Innovation".to_string(), "Strategy".to_string()],
                },
                TechnicalSkill {
                    name: "creative_ideation".to_string(),
                    proficiency: 0.92,
                    experience_years: 7.0,
                    domains: vec!["Innovation".to_string(), "Strategy".to_string()],
                },
            ],
            performance_metrics: PerformanceMetrics {
                success_rate: 0.85,
                avg_completion_time: 30.0,
                quality_score: 0.90,
                collaboration_score: 0.88,
                tasks_completed: 120,
                innovation_score: 0.95,
            },
            runtime_capabilities: RuntimeCapabilities {
                can_create_subtasks: true,
                can_modify_workflow: false,
                can_access_external_apis: false,
                can_delegate_tasks: true,
                max_concurrent_executions: 2,
                trusted_execution_level: TrustLevel::Standard,
                ffi_permissions: vec!["creative_tools".to_string()],
                gas_limit: 15000,
                execution_timeout_secs: 120,
            },
        },
        status: AgentStatus::Available,
        metadata: AgentMetadata {
            created_at: now,
            updated_at: now,
            version: 1,
            created_by: "demo-system".to_string(),
            generation_method: GenerationMethod::Manual {
                creator: "demo-system".to_string(),
            },
            tags: vec!["creative".to_string(), "strategy".to_string()],
            performance_tracking: true,
        },
        custom_properties: HashMap::new(),
    }
}

fn create_technical_agent() -> Agent {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    Agent {
        id: "agent_technical_beta".to_string(),
        name: "Technical Analysis Agent".to_string(),
        role: "Technical Analyst".to_string(),
        specialisation: "Technical Analysis and Implementation".to_string(),
        capabilities: AgentCapabilities {
            personality_traits: vec![
                "analytical".to_string(),
                "precise".to_string(),
                "methodical".to_string(),
            ],
            strengths: vec![
                "technical_analysis".to_string(),
                "problem_solving".to_string(),
            ],
            approach_style: "systematic and detail-oriented".to_string(),
            competitive_edge: "deep technical expertise and risk assessment".to_string(),
            risk_tolerance: 3.0,
            collaboration_preference: "provides technical foundation and validation".to_string(),
            technical_skills: vec![
                TechnicalSkill {
                    name: "Technical Analysis".to_string(),
                    proficiency: 0.93,
                    experience_years: 8.0,
                    domains: vec![
                        "Systems Analysis".to_string(),
                        "Risk Assessment".to_string(),
                    ],
                },
                TechnicalSkill {
                    name: "technical_analysis".to_string(),
                    proficiency: 0.93,
                    experience_years: 8.0,
                    domains: vec![
                        "Systems Analysis".to_string(),
                        "Risk Assessment".to_string(),
                    ],
                },
            ],
            performance_metrics: PerformanceMetrics {
                success_rate: 0.92,
                avg_completion_time: 25.0,
                quality_score: 0.94,
                collaboration_score: 0.82,
                tasks_completed: 200,
                innovation_score: 0.65,
            },
            runtime_capabilities: RuntimeCapabilities {
                can_create_subtasks: false,
                can_modify_workflow: false,
                can_access_external_apis: true,
                can_delegate_tasks: false,
                max_concurrent_executions: 1,
                trusted_execution_level: TrustLevel::Elevated,
                ffi_permissions: vec!["security_tools".to_string(), "analysis_tools".to_string()],
                gas_limit: 20000,
                execution_timeout_secs: 180,
            },
        },
        status: AgentStatus::Available,
        metadata: AgentMetadata {
            created_at: now,
            updated_at: now,
            version: 1,
            created_by: "demo-system".to_string(),
            generation_method: GenerationMethod::Manual {
                creator: "demo-system".to_string(),
            },
            tags: vec![
                "technical".to_string(),
                "analysis".to_string(),
                "technical_analyst".to_string(),
            ],
            performance_tracking: true,
        },
        custom_properties: HashMap::new(),
    }
}

fn create_academic_api_task() -> sleet::tasks::Task {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    sleet::tasks::Task {
        id: "academic_api_fetcher".to_string(),
        title: "Academic API Fetcher".to_string(),
        description: "Fetches academic papers from external API".to_string(),
        task_type: "api_fetch".to_string(),
        domain: "research".to_string(),
        priority: TaskPriorityLevel::High,
        status: sleet::tasks::TaskStatus::Created,
        success_criteria: vec![
            "API call successful".to_string(),
            "Data retrieved and formatted".to_string(),
        ],
        resource_requirements: vec![TaskResourceRequirement {
            resource_type: "network".to_string(),
            amount: 1,
            unit: "connection".to_string(),
            required: true,
        }],
        max_duration_secs: 60,
        difficulty_level: 3,
        collaboration_required: false,
        created_at: now,
        updated_at: now,
        assigned_agents: vec![],
        metadata: HashMap::new(),
    }
}

fn create_data_collection_workflow() -> OrchestrationFlowDefinition {
    let blocks = vec![
        OrchestrationBlockDefinition {
            id: "start_collection".to_string(),
            block_type: OrchestrationBlockType::ParallelExecution {
                branch_blocks: vec!["web_search".to_string(), "api_fetch".to_string()],
                merge_strategy: MergeStrategy::WaitAll,
                timeout_secs: Some(120),
                next_block: "consolidate_data".to_string(),
            },
            metadata: None,
        },
        OrchestrationBlockDefinition {
            id: "web_search".to_string(),
            block_type: OrchestrationBlockType::AgentInteraction {
                agent_selector: AgentSelector::ByCapability(vec!["web_search".to_string()]),
                task_definition: TaskDefinition {
                    task_type: "search_web".to_string(),
                    parameters: HashMap::from([("query".to_string(), json!("{{topic}}"))]),
                    expected_output: None,
                },
                interaction_type: InteractionType::Query {
                    expect_response: true,
                    response_format: orchestration::adapters::ResponseFormat::Json,
                },
                timeout_secs: Some(60),
                retry_config: None,
                next_block: "".to_string(),
            },
            metadata: Some(HashMap::from([(
                "output_key".to_string(),
                json!("web_results"),
            )])),
        },
        OrchestrationBlockDefinition {
            id: "api_fetch".to_string(),
            block_type: OrchestrationBlockType::TryCatch {
                try_block_id: "fetch_from_api".to_string(),
                catch_block_id: "handle_api_failure".to_string(),
            },
            metadata: None,
        },
        OrchestrationBlockDefinition {
            id: "fetch_from_api".to_string(),
            block_type: OrchestrationBlockType::TaskExecution {
                task_config: TaskExecutionConfig {
                    task_id: "academic_api_fetcher".to_string(),
                    priority: orchestration::TaskPriority::High,
                    dependencies: vec![],
                    completion_criteria: CompletionCriteria {
                        success_conditions: vec!["status == 'complete'".to_string()],
                        failure_conditions: vec!["status == 'failed'".to_string()],
                        timeout_secs: Some(60),
                    },
                },
                resource_requirements: ResourceRequirement {
                    cpu_cores: Some(2),
                    memory_mb: Some(1024),
                    storage_mb: Some(500),
                    network_bandwidth_mbps: Some(10),
                    specialised_hardware: vec![],
                },
                execution_strategy: ExecutionStrategy::Immediate,
                next_block: "".to_string(),
            },
            metadata: Some(HashMap::from([(
                "output_key".to_string(),
                json!("api_results"),
            )])),
        },
        OrchestrationBlockDefinition {
            id: "handle_api_failure".to_string(),
            block_type: OrchestrationBlockType::Compute {
                expression: r#"{"error": "API fetch failed, proceeding with partial data."}"#
                    .to_string(),
                output_key: "api_results".to_string(),
                next_block: "".to_string(),
            },
            metadata: None,
        },
        OrchestrationBlockDefinition {
            id: "consolidate_data".to_string(),
            block_type: OrchestrationBlockType::LLMProcessing {
                llm_config: orchestration::LLMProcessingConfig {
                    provider: "ollama".to_string(),
                    model: "llama3.2:latest".to_string(),
                    temperature: Some(0.3),
                    max_tokens: Some(2048),
                    additional_params: HashMap::new(),
                },
                prompt_template: "Consolidate and deduplicate findings from web search ({{web_results}}) and API results ({{api_results}}). Format as a JSON list of articles.".to_string(),
                context_keys: vec!["web_results".to_string(), "api_results".to_string()],
                output_key: "collected_articles".to_string(),
                processing_options: orchestration::LLMProcessingOptions {
                    streaming: false,
                    cache_results: true,
                    response_format: orchestration::ResponseFormat::Json,
                },
                next_block: "terminate_collection".to_string(),
            },
            metadata: None,
        },
        OrchestrationBlockDefinition {
            id: "terminate_collection".to_string(),
            block_type: OrchestrationBlockType::Terminate,
            metadata: None,
        },
    ];

    OrchestrationFlowDefinition {
        id: "data_collection_workflow".to_string(),
        start_block_id: "start_collection".to_string(),
        blocks,
        participants: vec!["system".to_string()],
        permissions: HashMap::new(),
        initial_state: Some(json!({ "topic": "placeholder" })),
        state_schema: None,
        resource_requirements: ResourceRequirements {
            total_limits: ResourceRequirement {
                cpu_cores: Some(2),
                memory_mb: Some(1024),
                storage_mb: Some(500),
                network_bandwidth_mbps: Some(10),
                specialised_hardware: vec![],
            },
            agents: HashMap::new(),
            llm: HashMap::new(),
            tasks: HashMap::new(),
            workflows: HashMap::new(),
        },
        execution_config: ExecutionConfiguration {
            max_parallel_blocks: 2,
            default_timeout_secs: 30,
            enable_checkpointing: false,
            checkpoint_interval_secs: Some(30),
            enable_debugging: true,
            performance_monitoring: true,
        },
    }
}

async fn setup_coordinator() -> OrchestrationCoordinator {
    let config = OrchestrationConfig::default();
    let mut coordinator = OrchestrationCoordinator::new(config).await.unwrap();

    let mut agent_system = AgentSystem::new(AgentSystemConfig::default()).unwrap();

    let web_agent = create_web_search_agent();
    let creative_agent = create_creative_agent();
    let technical_agent = create_technical_agent();

    agent_system.register_agent(web_agent.clone()).unwrap();
    agent_system.register_agent(creative_agent.clone()).unwrap();
    agent_system
        .register_agent(technical_agent.clone())
        .unwrap();

    println!(
        "Created demo agents: {}, {}, {}",
        web_agent.name, creative_agent.name, technical_agent.name
    );

    let stats = agent_system.get_statistics();
    println!(
        "Agent System Statistics: total={}, active={}, available={}",
        stats.total_agents, stats.active_agents, stats.available_agents
    );

    match agent_system.list_active_agents() {
        Ok(agents) => {
            println!("Active agents found: {}", agents.len());
            for agent in &agents {
                println!(
                    "  - {} ({}): strengths: {:?}, skills: {:?}, tags: {:?}",
                    agent.name,
                    agent.id,
                    agent.capabilities.strengths,
                    agent
                        .capabilities
                        .technical_skills
                        .iter()
                        .map(|s| &s.name)
                        .collect::<Vec<_>>(),
                    agent.metadata.tags
                );
            }
        }
        Err(e) => println!("Failed to list active agents: {e}"),
    }

    println!("\n--- Testing Capability Matching ---");

    let web_search_matcher =
        sleet::agents::CapabilityMatcher::new().require_skill("web_search", 0.5, 1.0);
    match agent_system.find_agents(web_search_matcher).await {
        Ok(matches) => {
            println!("Found {} agents for 'web_search' capability", matches.len());
            for (agent, match_result) in &matches {
                println!(
                    "  - {}: score={:.2}, required_met={}",
                    agent.name, match_result.score, match_result.required_skills_met
                );
            }
        }
        Err(e) => println!("Error finding agents for web_search: {e}"),
    }

    let creative_matcher =
        sleet::agents::CapabilityMatcher::new().require_skill("creative_ideation", 0.5, 1.0);
    match agent_system.find_agents(creative_matcher).await {
        Ok(matches) => {
            println!(
                "Found {} agents for 'creative_ideation' capability",
                matches.len()
            );
            for (agent, match_result) in &matches {
                println!(
                    "  - {}: score={:.2}, required_met={}",
                    agent.name, match_result.score, match_result.required_skills_met
                );
            }
        }
        Err(e) => println!("Error finding agents for creative_ideation: {e}"),
    }

    let tag_matcher = sleet::agents::CapabilityMatcher::new().require_domain("technical_analyst");
    match agent_system.find_agents(tag_matcher).await {
        Ok(matches) => {
            println!(
                "Found {} agents for 'technical_analyst' domain",
                matches.len()
            );
            for (agent, match_result) in &matches {
                println!(
                    "  - {}: score={:.2}, required_met={}",
                    agent.name, match_result.score, match_result.required_skills_met
                );
            }
        }
        Err(e) => println!("Error finding agents for technical_analyst: {e}"),
    }

    let mut task_system = TaskSystem::new(TaskSystemConfig::default());

    let api_task_config = TaskConfig {
        title: "Academic API Fetcher".to_string(),
        description: "Fetches academic papers from external API".to_string(),
        task_type: Some("api_fetch".to_string()),
        domain: Some("research".to_string()),
        priority: sleet::tasks::TaskPriority::High,
        max_duration_secs: Some(60),
        collaboration_required: false,
        success_criteria: vec![
            "API call successful".to_string(),
            "Data retrieved and formatted".to_string(),
        ],
        resource_requirements: vec![TaskResourceRequirement {
            resource_type: "network".to_string(),
            amount: 1,
            unit: "connection".to_string(),
            required: true,
        }],
        metadata: HashMap::new(),
    };

    let _api_task = task_system.create_task(api_task_config).unwrap();

    let llm_config = sleet::llm::LLMManagerConfig {
        primary_provider: "ollama".to_string(),
        primary_model: "llama3.2:latest".to_string(),
        preferred_provider: None,
        preferred_model: None,
        fallback_providers: vec![("ollama".to_string(), "llama3.2:latest".to_string())],
        retry_attempts: 2,
        enable_fallback: true,
    };

    let llm_adapter = sleet::llm::UnifiedLLMAdapter::ollama("llama3.2:latest".to_string())
        .await
        .unwrap();
    let conversation_config = sleet::llm::ConversationConfig {
        max_history_length: 10,
        context_window_tokens: 4000,
        preserve_system_messages: true,
    };
    let llm_processor = sleet::llm::LLMProcessor::new(Box::new(llm_adapter), conversation_config);

    coordinator
        .initialise(Some(agent_system), Some(llm_processor), Some(task_system))
        .await
        .unwrap();

    println!("Coordinator initialized successfully");

    coordinator.debug_resource_state().await;

    coordinator
}

fn create_dynamic_research_flow() -> OrchestrationFlowDefinition {
    let blocks = vec![

        OrchestrationBlockDefinition {
            id: "initialise_research".to_string(),
            block_type: OrchestrationBlockType::AwaitInput {
                interaction_id: "user_research_topic".to_string(),
                agent_id: "system".to_string(),
                prompt: "What topic would you like to research and strategize about?".to_string(),
                state_key: "user_input".to_string(),
                next_block: "set_agent_availability".to_string(),
            },
            metadata: Some(HashMap::from([(
                "description".to_string(),
                json!("Collect research topic from user"),
            )])),
        },

        OrchestrationBlockDefinition {
            id: "set_agent_availability".to_string(),
            block_type: OrchestrationBlockType::Compute {
                expression: "true".to_string(),
                output_key: "agents_available".to_string(),
                next_block: "check_agent_availability".to_string(),
            },
            metadata: Some(HashMap::from([(
                "description".to_string(),
                json!("Set agents_available variable to true"),
            )])),
        },

        OrchestrationBlockDefinition {
            id: "check_agent_availability".to_string(),
            block_type: OrchestrationBlockType::Conditional {
                condition: "agents_available".to_string(),
                true_block: "data_collection".to_string(),
                false_block: "handle_no_agent".to_string(),
            },
            metadata: None,
        },


        OrchestrationBlockDefinition {
            id: "data_collection".to_string(),
            block_type: OrchestrationBlockType::Compute {
                expression: r#"{"articles": ["Mock article about AI in healthcare"]}"#.to_string(),
                output_key: "collected_articles".to_string(),
                next_block: "parallel_analysis".to_string(),
            },
            metadata: None,
        },


        OrchestrationBlockDefinition {
            id: "parallel_analysis".to_string(),
            block_type: OrchestrationBlockType::ParallelExecution {
                branch_blocks: vec![
                    "creative_analysis".to_string(),
                    "technical_analysis".to_string(),
                ],
                merge_strategy: MergeStrategy::WaitAll,
                timeout_secs: Some(180),
                next_block: "synthesis_llm".to_string(),
            },
            metadata: None,
        },


        OrchestrationBlockDefinition {
            id: "creative_analysis".to_string(),
            block_type: OrchestrationBlockType::AgentInteraction {
                agent_selector: AgentSelector::ByCapability(vec!["creative_ideation".to_string()]),
                task_definition: TaskDefinition {
                    task_type: "creative_ideation".to_string(),
                    parameters: HashMap::from([
                        ("articles".to_string(), json!("{{articles}}")),
                        ("focus".to_string(), json!("innovation and opportunities")),
                    ]),
                    expected_output: Some(json!("Strategic insights and creative opportunities")),
                },
                interaction_type: InteractionType::Analysis {
                    analysis_type: orchestration::adapters::AnalysisType::Custom("creative_strategy".to_string()),
                    depth_level: orchestration::adapters::AnalysisDepth::Deep,
                },
                timeout_secs: Some(120),
                retry_config: Some(RetryConfig {
                    max_attempts: 2,
                    backoff_strategy: orchestration::BackoffStrategy::Fixed { interval_ms: 10000 },
                    retry_conditions: vec![
                        orchestration::RetryCondition { error_type: "timeout".to_string(), should_retry: true },
                        orchestration::RetryCondition { error_type: "agent_error".to_string(), should_retry: true },
                    ],
                }),
                next_block: "".to_string(),
            },
            metadata: Some(HashMap::from([(
                "output_key".to_string(),
                json!("creative_insights"),
            )])),
        },


        OrchestrationBlockDefinition {
            id: "technical_analysis".to_string(),
            block_type: OrchestrationBlockType::AgentInteraction {
                agent_selector: AgentSelector::ByCapability(vec!["technical_analysis".to_string()]),
                task_definition: TaskDefinition {
                    task_type: "technical_analysis".to_string(),
                    parameters: HashMap::from([
                        ("articles".to_string(), json!("{{articles}}")),
                        ("focus".to_string(), json!("risks and implementation")),
                    ]),
                    expected_output: Some(json!("Technical feasibility and risk assessment")),
                },
                interaction_type: InteractionType::Analysis {
                    analysis_type: orchestration::adapters::AnalysisType::Custom("technical_assessment".to_string()),
                    depth_level: orchestration::adapters::AnalysisDepth::Deep,
                },
                timeout_secs: Some(120),
                retry_config: None,
                next_block: "".to_string(),
            },
            metadata: Some(HashMap::from([(
                "output_key".to_string(),
                json!("technical_assessment"),
            )])),
        },


        OrchestrationBlockDefinition {
            id: "synthesis_llm".to_string(),
            block_type: OrchestrationBlockType::Compute {
                expression: r#"{"strategy": "Mock strategic analysis: Combining creative innovation with technical feasibility for comprehensive solution development"}"#.to_string(),
                output_key: "strategy".to_string(),
                next_block: "user_feedback".to_string(),
            },
            metadata: Some(HashMap::from([(
                "description".to_string(),
                json!("Generate synthesised strategy (mock)"),
            )])),
        },


        OrchestrationBlockDefinition {
            id: "user_feedback".to_string(),
            block_type: OrchestrationBlockType::AwaitInput {
                interaction_id: "user_feedback_choice".to_string(),
                agent_id: "system".to_string(),
                prompt: "Review the strategy above. Would you like to: (1) Generate final report, (2) Request refinements, or (3) Start over?".to_string(),
                state_key: "user_choice".to_string(),
                next_block: "decision_branch".to_string(),
            },
            metadata: None,
        },


        OrchestrationBlockDefinition {
            id: "decision_branch".to_string(),
            block_type: OrchestrationBlockType::Conditional {
                condition: r#"user_choice == "Generate final report""#.to_string(),
                true_block: "extract_takeaways".to_string(),
                false_block: "handle_refinement".to_string(),
            },
            metadata: None,
        },


        OrchestrationBlockDefinition {
            id: "extract_takeaways".to_string(),
            block_type: OrchestrationBlockType::Compute {
                expression: r#"{"takeaways": "Mock takeaways: 1) Focus on innovation, 2) Ensure technical feasibility, 3) Consider user needs, 4) Plan implementation phases, 5) Monitor progress"}"#.to_string(),
                output_key: "takeaways".to_string(),
                next_block: "generate_final_report".to_string(),
            },
            metadata: Some(HashMap::from([(
                "description".to_string(),
                json!("Extract key takeaways (mock)"),
            )])),
        },


        OrchestrationBlockDefinition {
            id: "handle_refinement".to_string(),
            block_type: OrchestrationBlockType::Conditional {
                condition: r#"user_choice == "Request refinements""#.to_string(),
                true_block: "synthesis_llm".to_string(),
                false_block: "initialise_research".to_string(),
            },
            metadata: None,
        },


        OrchestrationBlockDefinition {
            id: "generate_final_report".to_string(),
            block_type: OrchestrationBlockType::Compute {
                expression: r#"{"final_report": "Mock Final Report: Strategic analysis complete with comprehensive recommendations for implementation."}"#.to_string(),
                output_key: "final_report".to_string(),
                next_block: "terminate_success".to_string(),
            },
            metadata: Some(HashMap::from([(
                "description".to_string(),
                json!("Generate final report (mock)"),
            )])),
        },


        OrchestrationBlockDefinition {
            id: "terminate_success".to_string(),
            block_type: OrchestrationBlockType::Terminate,
            metadata: Some(HashMap::from([("status".to_string(), json!("Success"))])),
        },
        OrchestrationBlockDefinition {
            id: "terminate_manual".to_string(),
            block_type: OrchestrationBlockType::Terminate,
            metadata: Some(HashMap::from([(
                "status".to_string(),
                json!("Terminated by user"),
            )])),
        },
        OrchestrationBlockDefinition {
            id: "handle_no_agent".to_string(),
            block_type: OrchestrationBlockType::Terminate,
            metadata: Some(HashMap::from([(
                "status".to_string(),
                json!("Failed: No suitable agent found"),
            )])),
        },
    ];

    OrchestrationFlowDefinition {
        id: "dynamic_research_strategy_flow".to_string(),
        start_block_id: "initialise_research".to_string(),
        blocks,
        participants: vec!["user".to_string(), "system".to_string()],
        permissions: HashMap::from([
            (
                "read".to_string(),
                vec!["user".to_string(), "system".to_string()],
            ),
            ("write".to_string(), vec!["system".to_string()]),
        ]),
        initial_state: Some(json!({
            "flow_version": "1.0",
            "created_at": chrono::Utc::now().to_rfc3339(),
            "topic": "AI in healthcare"
        })),
        state_schema: Some(json!({
            "type": "object",
            "properties": {
                "topic": {"type": "string"},
                "user_input": {"type": "string"},
                "articles": {"type": "array"},
                "creative_insights": {"type": "string"},
                "technical_assessment": {"type": "string"},
                "strategy": {"type": "string"},
                "takeaways": {"type": "string"},
                "final_report": {"type": "string"}
            }
        })),
        resource_requirements: ResourceRequirements {
            total_limits: ResourceRequirement {
                cpu_cores: Some(4),
                memory_mb: Some(2048),
                storage_mb: Some(500),
                network_bandwidth_mbps: Some(10),
                specialised_hardware: vec![],
            },
            agents: HashMap::new(),
            llm: HashMap::new(),
            tasks: HashMap::new(),
            workflows: HashMap::new(),
        },
        execution_config: ExecutionConfiguration {
            max_parallel_blocks: 3,
            default_timeout_secs: 60,
            enable_checkpointing: true,
            checkpoint_interval_secs: Some(30),
            enable_debugging: true,
            performance_monitoring: true,
        },
    }
}

#[tokio::main]
async fn main() {
    let coordinator = Arc::new(setup_coordinator().await);
    let flow_definition = create_dynamic_research_flow();

    println!("===================================================================");
    println!("  Starting 'Dynamic Research & Strategy Formulation' flow");
    println!("===================================================================");
    println!(
        "Topic: {}",
        flow_definition.initial_state.as_ref().unwrap()["topic"]
    );
    println!("-------------------------------------------------------------------\n");

    let mut session_id: Option<String> = None;
    let mut next_input: Option<Value> = None;

    loop {
        let result = match session_id.clone() {
            Some(id) => {
                let input = next_input.take().unwrap_or(json!(null));
                coordinator.resume_session(&id, input).await
            }
            None => {
                coordinator
                    .execute_flow(flow_definition.clone(), None)
                    .await
            }
        };

        match result {
            Ok(status) => match status {
                ExecutionStatus::AwaitingInput {
                    session_id: session_id_from_status,
                    interaction_id: _,
                    agent_id: _,
                    prompt,
                } => {
                    session_id = Some(session_id_from_status);
                    println!(
                        "\n-------------------------------------------------------------------"
                    );
                    println!(">>> FLOW PAUSED: WAITING FOR USER INPUT <<<");
                    println!(
                        "PROMPT: {}",
                        prompt.as_str().unwrap_or("No prompt provided")
                    );
                    print!("Your response: ");
                    io::stdout().flush().unwrap();

                    let mut user_input = String::new();
                    io::stdin().read_line(&mut user_input).unwrap();
                    next_input = Some(json!(user_input.trim()));
                    println!(
                        "-------------------------------------------------------------------\n"
                    );
                }
                ExecutionStatus::Completed(final_state) => {
                    println!(
                        "\n==================================================================="
                    );
                    println!(">>> FLOW COMPLETED <<<");
                    println!("===================================================================");
                    if let Ok(result_json) = serde_json::to_string_pretty(&final_state) {
                        println!("\nFinal Result:\n{result_json}");
                    }
                    break;
                }
                ExecutionStatus::Running => {
                    println!("...flow is running...");
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            },
            Err(e) => {
                eprintln!("\n!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                eprintln!(">>> FLOW EXECUTION FAILED <<<");
                eprintln!("ERROR: {e}");
                eprintln!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                break;
            }
        }
    }
}
