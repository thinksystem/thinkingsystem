sequenceDiagram
participant User
participant Main
participant AgentSystem
participant LLMAdapter
participant FlowTranspiler
participant SleetRuntime as RemarkableInterpreter
participant Agent1 as Planner Agent
participant Agent2 as Reviewer Agent
participant Agent3 as Arbiter Agent

    %% Initialisation Phase
    User->>Main: cargo run --goal "Plan birthday party" --mode simple
    Main->>Main: parse CLI arguments
    Main->>Main: log_event("demo_startup")
    Main->>Main: run_simple_mode()

    %% System Setup
    Main->>AgentSystem: AgentSystem::new(config)
    Main->>LLMAdapter: initialise_llm_adapter("ollama", "llama3.2")
    LLMAdapter-->>Main: CustomLLMAdapter instance
    Main->>AgentSystem: set_llm_adapter(adapter)

    %% Agent Generation
    Main->>AgentSystem: generate_team("Create planner", 1)
    AgentSystem->>LLMAdapter: generate_structured_response(system_prompt, task)
    LLMAdapter-->>AgentSystem: agent_definition_json
    AgentSystem-->>Main: [Planner Agent]

    Main->>AgentSystem: generate_team("Create reviewer", 1)
    AgentSystem->>LLMAdapter: generate_structured_response(system_prompt, task)
    LLMAdapter-->>AgentSystem: agent_definition_json
    AgentSystem-->>Main: [Reviewer Agent]

    Main->>AgentSystem: generate_team("Create arbiter", 1)
    AgentSystem->>LLMAdapter: generate_structured_response(system_prompt, task)
    LLMAdapter-->>AgentSystem: agent_definition_json
    AgentSystem-->>Main: [Arbiter Agent]

    %% Workflow Creation & Compilation
    Main->>Main: create_workflow_definition(goal, agent_ids)
    Main->>FlowTranspiler: FlowTranspiler::transpile(flow_def)
    FlowTranspiler-->>Main: orchestration_contract
    Main->>Main: sleet::convert_contract(contract)
    Main-->>Main: sleet_contract
    Main->>SleetRuntime: RemarkableInterpreter::new(memory, contract, registry)

    %% Workflow Execution Loop
    loop Workflow Execution
        Main->>SleetRuntime: runtime.run(contract)

        alt AwaitingInput Status
            SleetRuntime-->>Main: ExecutionStatus::AwaitingInput{interaction_id, agent_id, prompt}
            Main->>Main: log_event("agent_processing")

            alt Planner Agent Request
                Main->>AgentSystem: get_agent(planner_id)
                AgentSystem-->>Main: planner_agent
                Main->>Agent1: get_system_prompt()
                Agent1-->>Main: system_prompt
                Main->>LLMAdapter: generate_structured_response(system_prompt, user_prompt)
                LLMAdapter-->>Main: plan_response
                Main->>SleetRuntime: resume_with_input(interaction_id, response)
            else Reviewer Agent Request
                Main->>AgentSystem: get_agent(reviewer_id)
                AgentSystem-->>Main: reviewer_agent
                Main->>Agent2: get_system_prompt()
                Agent2-->>Main: system_prompt
                Main->>LLMAdapter: generate_structured_response(system_prompt, user_prompt)
                LLMAdapter-->>Main: review_response
                Main->>SleetRuntime: resume_with_input(interaction_id, response)
            else Arbiter Agent Request
                Main->>AgentSystem: get_agent(arbiter_id)
                AgentSystem-->>Main: arbiter_agent
                Main->>Agent3: get_system_prompt()
                Agent3-->>Main: system_prompt
                Main->>LLMAdapter: generate_structured_response(system_prompt, user_prompt)
                LLMAdapter-->>Main: assessment_response
                Main->>SleetRuntime: resume_with_input(interaction_id, response)
            end

        else Completed Status
            SleetRuntime-->>Main: ExecutionStatus::Completed(final_result)
            Main->>Main: break execution loop

        else Running Status
            SleetRuntime-->>Main: ExecutionStatus::Running
            Note over Main: Continue processing
        end
    end

    %% Final Output
    Main->>Main: log_event("workflow_completed", final_result)
    Main->>User: print formatted final result

    %% Multi-Workflow Mode (Alternative Path)
    Note over Main: Alternative: run_multi_workflow_mode()
    Main->>Main: WorkflowOrchestrator::new()
    Main->>Main: generate_specialised_team("strategic", 2)
    Main->>Main: generate_specialised_team("technical", 2)
    Main->>Main: generate_specialised_team("synthesis", 1)
    Main->>Main: create_strategic_workflow()
    Main->>Main: create_technical_workflow()
    Main->>Main: create_synthesis_workflow()
    Main->>Main: orchestrator.execute_parallel(["strategic", "technical"])
    Main->>Main: orchestrator.execute_sequential("synthesis")

cargo run -- --goal "Create a simple web application" --mode simple

Copyright (C) 2024 Jonathan Lee.
