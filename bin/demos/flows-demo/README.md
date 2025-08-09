# Flows Demo - Adaptive LLM-Powered API Processing

This demo showcases an adaptive flow execution system that dynamically generates and executes workflows for arbitrary API endpoints using LLM intelligence.

## Basic Usage

```bash
RUST_LOG=debug cargo run --bin flows-demo -- --endpoint "API_URL" --processing-goal "your analysis goal"
```

### Example Commands

```bash
# Singapore government APIs (no key required)
RUST_LOG=debug cargo run --bin flows-demo -- --endpoint "https://data.gov.sg/api/action/datastore_search?resource_id=f1765b54-a209-4718-8d38-a39237f502b3&limit=5" --processing-goal "analyse housing data"

RUST_LOG=debug cargo run --bin flows-demo -- --endpoint "https://api.data.gov.sg/v1/transport/taxi-availability" --processing-goal "analyse taxi availability data"

# Other public APIs
RUST_LOG=debug cargo run --bin flows-demo -- --endpoint "https://dog.ceo/api/breeds/image/random" --processing-goal "analyse dog breed image data structure"

RUST_LOG=debug cargo run --bin flows-demo -- --endpoint "https://api.ipify.org/?format=json" --processing-goal "analyse IP address API data structure"
```

## Key Features

- **Sophisticated LLM Selection Engine**: Dynamic model selection with local-first priority (Ollama → Anthropic → OpenAI)
- **Adaptive Flow Generation**: LLM creates custom workflows for any API endpoint
- **Local Model Preference**: Prioritizes free Ollama models (llama3.1:8b, llama3.2:3b, gemma3:4b, deepseek-r1:8b) with cloud fallback
- **Dynamic Model Scoring**: Real-time performance tracking and model selection based on quality, speed, and cost
- **Self-Correcting Validation**: Automatic error detection and flow regeneration
- **Real-time API Integration**: Works with live external APIs (no mocked responses)
- **Intelligent Error Recovery**: 3-iteration adaptive system with automatic parameter detection

## Adaptive Mode

Add `--adaptive` flag for enhanced error recovery that automatically fixes malformed endpoints and missing parameters:

```bash
# Adaptive mode examples - handles broken/incomplete endpoints
RUST_LOG=debug cargo run --bin flows-demo -- --endpoint "https://api.nationalize.io/" --processing-goal "analyse nationality prediction API structure for name samuel" --adaptive

RUST_LOG=debug cargo run --bin flows-demo -- --endpoint "http://universities.hipolabs.com/search" --processing-goal "explore university search API and find universities in a specific country" --adaptive
```

## LLM Selection Engine

The flows-demo uses a sophisticated **Dynamic Model Selector** that intelligently chooses the optimal LLM for each task:

### Model Priority Strategy

1. **Local-First Approach**: Prioritizes free Ollama models running locally

   - `llama3.1:8b` - Best for complex reasoning (32K context, score: 1.07)
   - `llama3.2:3b` - Fast for simple tasks (8K context, 50 TPS)
   - `gemma3:4b` - Balanced performance
   - `deepseek-r1:8b` - Specialised reasoning model

2. **Cloud Fallback**: Falls back to API providers when local models are insufficient
   - **Anthropic**: `claude-3-7-sonnet-latest`, `claude-3-5-sonnet-latest`, `claude-3-5-haiku-latest`
   - **OpenAI**: `gpt-4o`, `gpt-4o-mini` (requires `OPENAI_API_KEY`)

### Dynamic Scoring Algorithm

Models are scored using weighted criteria optimized for cost efficiency:

```
Score = (Quality × 0.1) + (Speed × 0.2) + (Cost × 0.7) + Reliability + Context - Load
```

- **Cost Optimization**: Free local models get maximum cost scores
- **Performance Tracking**: Real-time TPS and response time metrics
- **Capability Matching**: Tasks matched to models with required capabilities

### Example Selection Behavior

```bash
# Health Check → llama3.1:8b (local, free, score: 1.07)
# Complex Flow Generation → claude-3-7-sonnet-latest (cloud, score: 1.069)
# Simple Analysis → llama3.2:3b (local, fast, score: 1.05)
```

The system automatically learns from performance and updates model preferences over time.

# System Architecture Flow

The following sequence diagram accurately represents the validated system behavior based on actual execution logs and testing:

::: mermaid

---

config:
theme: neutral

---

sequenceDiagram
participant User
participant FlowDemo as FlowsDemo Main
participant Orchestrator as Flow Orchestrator
participant LLM as Unified LLM Adapter
participant ModelSelector as Dynamic Model Selector
participant Ollama as Ollama (Local)
participant Anthropic as Anthropic API
participant OpenAI as OpenAI API
participant ValidationService as Validation Service
participant FlowEngine as Flow Execution Engine
participant ExternalAPI as External API
Note over User,ExternalAPI: Adaptive API Processing with Dynamic LLM Selection
User->>FlowDemo: cargo run --bin flows-demo<br/>--endpoint "api_url"<br/>--processing-goal "analysis goal"
FlowDemo->>Orchestrator: Initialise with unified configuration
Note over Orchestrator: Loading config, schema provider, validation
Orchestrator->>LLM: Initialise UnifiedLLMAdapter with defaults
LLM->>ModelSelector: Load llm_models.yml configuration
Note over ModelSelector: Local-first priority:<br/>Ollama → Anthropic → OpenAI
LLM-->>Orchestrator: LLM adapter with dynamic selection ready
FlowDemo->>Orchestrator: Perform system health check
Orchestrator->>LLM: Test basic flow generation ("Hello" flow)
LLM->>ModelSelector: Select optimal model for health check
Note over ModelSelector: Dynamic Selection Algorithm:<br/>Quality×0.1 + Speed×0.2 + Cost×0.7<br/>+ Reliability×0.2 + Context×0.0 - Load×0.0
ModelSelector->>Ollama: Health check local models
Ollama-->>ModelSelector: Available: [llama3.1:8b, llama3.2:3b, gemma3:4b, deepseek-r1:8b]
Note over ModelSelector: Score: llama3.1:8b = 1.07<br/>Local model selected (cost_tier: free)
LLM->>Ollama: Generate simple hello flow
Note over Ollama: Local inference: llama3.1:8b<br/>45.6 tokens/sec, 8662ms response
Ollama-->>LLM: JSON flow definition (local response)
LLM-->>ValidationService: Validate hello flow
ValidationService-->>Orchestrator: Health check passed<br/>Performance updated: llama3.1:8b success=true, TPS=45.6
Orchestrator-->>FlowDemo: All systems operational<br/>Health: {core_orchestrator: true, llm_wrapper: true, flow_generator: true}
Note over FlowDemo,SingaporeAPI: API Exploration Phase
FlowDemo->>FlowEngine: Execute api_exploration_flow
FlowEngine->>FlowEngine: Register flow (4 blocks: explore_api, preserve_data, end, default)
FlowEngine->>FlowEngine: Process block "explore_api" (ExternalData)
FlowEngine->>SingaporeAPI: GET api_endpoint
Note over SingaporeAPI: Connect to external API
SingaporeAPI-->>FlowEngine: API response data
FlowEngine->>FlowEngine: Store in state[api_response]<br/>Navigate to "preserve_data"
FlowEngine->>FlowEngine: Process block "preserve_data" (Compute)<br/>Navigate to "end"
FlowEngine->>FlowEngine: Process block "end" (Compute)<br/>Navigate to "default"
FlowEngine->>FlowEngine: Process block "default" (Compute)<br/>Flow completed successfully
Note over FlowEngine: API exploration completed with proper termination
Note over FlowDemo,ExternalAPI: Adaptive Flow Generation with Intelligent Model Selection
FlowDemo->>Orchestrator: Start guided flow generation
Orchestrator->>LLM: Generate API processing flow<br/>Endpoint: target API<br/>Goal: user-specified processing goal
LLM->>ModelSelector: Select model for complex flow generation
Note over ModelSelector: Task: Complex reasoning + code generation<br/>Evaluating models for capability match...
ModelSelector->>ModelSelector: Calculate scores for all models:<br/>claude-3-7-sonnet: 1.069 (Q:0.09 + C:0.70 + S:0.08)<br/>llama3.1:8b: 1.07 (local preference)<br/>claude-3-5-sonnet: 1.065
Note over ModelSelector: Selected: claude-3-7-sonnet-latest<br/>Reason: Highest score for complex reasoning
LLM->>Anthropic: Generate custom analysis flow<br/>Schema: ExternalData, Compute, Display, Input blocks
Note over Anthropic: Dynamic JSON schema generation<br/>Complex flow with 13 blocks, interactive analysis<br/>Response time: 15.75s, 50.5 TPS
Anthropic-->>LLM: Complete flow definition:<br/>trademark_analysis_flow with validation logic
LLM-->>ValidationService: Validate generated flow (13 blocks)
ValidationService-->>Orchestrator: Flow validation successful (iteration 1)<br/>Performance updated: claude-3-7-sonnet success=true, TPS=50.5
Orchestrator-->>FlowDemo: Flow generation successful!<br/>Status: SUCCESS with dynamic model selection
Note over FlowDemo,ExternalAPI: Generated Flow Execution with Smart API Processing
FlowDemo->>FlowEngine: Execute trademark_analysis_flow
FlowEngine->>FlowEngine: Register flow (13 blocks with interactive analysis)
FlowEngine->>FlowEngine: Process block "welcome_message" (Display)
FlowEngine->>FlowEngine: Process block "fetch_trademark_data" (ExternalData)
FlowEngine->>ExternalAPI: GET https://api.data.gov.sg/v1/technology/ipos/trademarks
Note over ExternalAPI: Singapore IPOS API connection<br/>No authentication required
ExternalAPI-->>FlowEngine: Response: {"lodgement_date": "2025-08-06", "count": 0, "items": []}<br/>Empty dataset discovered
FlowEngine->>FlowEngine: Store in state[external_data]<br/>Navigate to "process_trademark_data"
FlowEngine->>FlowEngine: Process block "process_trademark_data" (Compute)<br/>Handle empty dataset gracefully<br/>Navigate to "display_structure"
FlowEngine->>FlowEngine: Process block "display_structure" (Display)<br/>Show data structure analysis<br/>Navigate to "display_samples"
FlowEngine->>FlowEngine: Process block "display_samples" (Display)<br/>Navigate to "ask_for_field"
FlowEngine->>FlowEngine: Process block "ask_for_field" (Input)<br/>Await user interaction: "Which field to analyse?"
Note over FlowEngine: Flow paused for user input<br/>Demonstrates interactive capabilities
FlowDemo-->>User: Flow execution paused at input block<br/>Status: AWAITING_USER_INPUT<br/>API successfully processed, interactive analysis ready
Note over User,ExternalAPI: System Capabilities Demonstrated<br/>• Dynamic LLM Selection: Local-first with cloud fallback<br/>• Model Scoring Algorithm: Quality×0.1 + Speed×0.2 + Cost×0.7<br/>• Performance Tracking: Real-time TPS and response time metrics<br/>• Intelligent Model Matching: Task complexity → optimal model selection<br/>• Health Check: PASSED (llama3.1:8b local inference)<br/>• Complex Flow Generation: SUCCESS (claude-3-7-sonnet, 13-block flow)<br/>• API Integration: SUCCESS (Singapore IPOS trademark API)<br/>• Interactive Capabilities: Flow paused at user input (ask_for_field)<br/>• Error Handling: Graceful empty dataset processing<br/>• Local Model Priority: 4 Ollama models available, cost-optimized selection

:::

Copyright (C) 2024 Jonathan Lee.
