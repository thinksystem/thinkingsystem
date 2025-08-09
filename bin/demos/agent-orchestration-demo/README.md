# Agent Orchestration Demo

This demonstration illustrates how LLM-powered agent orchestration works through dynamic team generation and collaborative planning sessions. The demo showcases the Sleet crate's workflow automation capabilities by generating specialist AI agents and coordinating their interactions to solve complex problems.

## System Components Demonstrated

- **LLM Manager**: Unified interface to multiple LLM providers (Ollama, Anthropic, OpenAI) with automatic fallback handling.
- **Agent Generator**: Dynamic creation of specialist agents with unique capabilities based on LLM-generated configurations.
- **Team Generation**: Automated assembly of diverse agent teams optimized for specific tasks.
- **Planning Session**: Iterative collaborative planning with proposal generation, feedback collection, and quality assessment.
- **Progress Tracking**: Momentum-based progress analysis with plateau detection and breakout strategies.

This demonstration is a placeholder for the recently revamped multi-agent runtime. It will be updated soon.

## Key Flows

- **Team Assembly**: Goal analysis → Specialist agent generation → Arbiter agent creation → Team validation.
- **Collaborative Planning**: Initial proposal → Specialist feedback → Feedback distillation → Quality assessment → Progress evaluation → Iterative refinement.
- **Quality Control**: Arbiter assessment → Confidence scoring → Plateau detection → Breakout strategies.

## Notable Features

- **Dynamic Agent Creation**: Agents are generated with specific expertise, personality traits, and performance expectations based on the task requirements.
- **Robust Fallback System**: Comprehensive fallback mechanisms when LLM calls fail, using predefined agent templates.
- **Adaptive Planning**: Plateau detection triggers context reset and strategy changes to overcome stuck situations.
- **Multi-Provider Support**: Seamless switching between different LLM providers based on availability and task requirements.
- **Structured Logging**: Comprehensive event tracking throughout the orchestration process.

## Prerequisites

Configure your LLM providers by setting appropriate environment variables:

For Ollama (default):

```sh
# Ensure Ollama is running locally
ollama serve
```

For Anthropic:

```sh
export ANTHROPIC_API_KEY="your-api-key"
```

For OpenAI:

```sh
export OPENAI_API_KEY="your-api-key"
```

## How to Run

Execute the following command from the workspace root:

```sh
cargo run --bin agent-orchestration-demo -- --goal "Create a REST API for user management" --team-size 3 --provider ollama --model llama3.2
```

### Command Line Options

- `--goal, -g`: The goal for the agent team to achieve (required)
- `--team-size, -s`: Number of specialist agents (1-10, default: 3)
- `--provider`: Primary LLM provider (ollama, anthropic, default: ollama)
- `--model, -m`: Model for the primary provider (default: llama3.2)

### Usage Examples

```sh
# Basic usage with default settings
cargo run --bin agent-orchestration-demo -- --goal "Design a microservices architecture"

# Large team with specific provider
cargo run --bin agent-orchestration-demo -- --goal "Build a machine learning pipeline" --team-size 5 --provider anthropic --model claude-3-5-haiku-latest

# Small focused team
cargo run --bin agent-orchestration-demo -- --goal "Optimise database queries" --team-size 2 --provider ollama --model llama3.1
```

## Architecture Diagram

<details>
<summary>Click to expand the sequence diagram</summary>

::: mermaid

---

config:
theme: neutral

---

sequenceDiagram
participant User as User
participant Demo as agent-orchestration-demo
participant LLMManager as LLM Manager
participant TeamGen as Team Generator
participant AgentGen as Agent Generator
participant LLMAdapter as LLM Adapter
participant PlanningSession as Planning Session
participant Logger as Event Logger
Note over User, Logger: Demo Initialization
User->>Demo: Start with goal and config
Demo->>Logger: log_event("DEMO_STARTUP", config)
Demo->>LLMManager: new(config)
Note over LLMManager: LLM Manager Setup
LLMManager->>LLMAdapter: create_adapter(primary_provider, model)
alt Primary adapter success
LLMAdapter-->>LLMManager: Primary adapter ready
else Primary fails
LLMManager->>LLMAdapter: create_adapter(fallback_provider, model)
LLMAdapter-->>LLMManager: Fallback adapter ready
end
Note over Demo, AgentGen: Team Generation Phase
Demo->>TeamGen: generate_complete_team(goal, team_size, config)
TeamGen->>AgentGen: generate_specialist_team()
AgentGen->>LLMAdapter: generate_structured_response(prompt)
alt LLM Response Success
LLMAdapter-->>AgentGen: JSON agent definitions
AgentGen->>AgentGen: parse_agent_team_response_robustly()
AgentGen->>AgentGen: convert_llm_response_to_agents()
else LLM Response Fails
Note over AgentGen: Fallback to predefined templates
AgentGen->>AgentGen: create_fallback_team_response()
AgentGen->>AgentGen: convert_fallback_template_to_llm_agent()
end
AgentGen-->>TeamGen: Vec<Agent> specialists
TeamGen->>AgentGen: generate_arbiter_agent()
AgentGen->>LLMAdapter: generate_structured_response(arbiter_prompt)
alt Success
LLMAdapter-->>AgentGen: Arbiter definition
else Fallback
AgentGen->>AgentGen: create_fallback_arbiter()
end
AgentGen-->>TeamGen: Agent arbiter
TeamGen-->>Demo: (specialists, arbiter)
Note over Demo, PlanningSession: Planning Session Phase
Demo->>PlanningSession: new(task, specialists, arbiter, llm_manager)
Demo->>PlanningSession: run()
loop For each iteration (max 10)
PlanningSession->>PlanningSession: increment_iteration()
Note over PlanningSession: Proposal Phase
alt First iteration
PlanningSession->>LLMManager: get_initial_proposal(lead_agent, task)
LLMManager->>LLMAdapter: generate_structured_response_with_fallback()
LLMAdapter-->>LLMManager: Initial proposal JSON
LLMManager-->>PlanningSession: proposal
PlanningSession->>Logger: log_event("INITIAL_PROPOSAL_GENERATED")
else Subsequent iterations
PlanningSession->>LLMManager: refine_proposal(current, feedback)
LLMManager->>LLMAdapter: generate_structured_response_with_fallback()
LLMAdapter-->>LLMManager: Refined proposal JSON
LLMManager-->>PlanningSession: refined_proposal
PlanningSession->>Logger: log_event("PROPOSAL_REFINED")
end
Note over PlanningSession: Feedback Phase
loop For each specialist
PlanningSession->>LLMManager: get_specialist_feedback(agent, proposal)
LLMManager->>LLMAdapter: generate_structured_response_with_fallback()
LLMAdapter-->>LLMManager: Feedback JSON
LLMManager-->>PlanningSession: specialist_feedback
end
PlanningSession->>LLMManager: distil_feedback(all_feedback)
LLMManager->>LLMAdapter: generate_structured_response_with_fallback()
LLMAdapter-->>LLMManager: Distilled feedback JSON
LLMManager-->>PlanningSession: distilled_feedback
PlanningSession->>Logger: log_event("FEEDBACK_DISTILLED")
Note over PlanningSession: Assessment Phase
PlanningSession->>LLMManager: assess_proposal_quality(arbiter, proposal, feedback)
LLMManager->>LLMAdapter: generate_structured_response_with_fallback()
LLMAdapter-->>LLMManager: Assessment JSON
LLMManager-->>PlanningSession: assessment
PlanningSession->>Logger: log_event("ARBITER_ASSESSMENT")
Note over PlanningSession: Progress Evaluation
PlanningSession->>LLMManager: evaluate_progress_score()
LLMManager->>LLMAdapter: generate_structured_response_with_fallback()
LLMAdapter-->>LLMManager: Progress score
LLMManager-->>PlanningSession: progress_score
PlanningSession->>Logger: log_event("PROGRESS_EVALUATION")
Note over PlanningSession: Check Exit Conditions
alt Goal achieved (confidence >= 0.8)
PlanningSession->>Logger: log_event("GOAL_ACHIEVED")
Note over PlanningSession: Exit loop - success
else Plateau detected
PlanningSession->>LLMManager: apply_breakout_strategy()
PlanningSession->>Logger: log_event("PLATEAU_DETECTED")
Note over PlanningSession: Continue with reset context
else Max iterations reached
PlanningSession->>PlanningSession: fail_session()
Note over PlanningSession: Exit loop - failure
end
end
PlanningSession-->>Demo: Result (success/failure)
Demo->>Logger: log_event("DEMO_COMPLETED", result)
Demo-->>User: Display final results

:::

</details>

Copyright (C) 2024 Jonathan Lee.
