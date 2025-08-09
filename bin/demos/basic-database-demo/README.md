# Basic Database Demo Overview

This demonstration provides a simple illustration of how the flexible library components can be used to build a sophisticated "Thinking System" which can understand natural language, extract knowledge, and store it without requiring users to learn a query language.

The `talking-database-demo` in the parent directory builds on this concept with deeper NLU/LLM integration, creating an even more intuitive conversational interface.

The library is being used to develop an open-source Thinking System detailed in `white_paper.md` (located in the root directory of the workspace).

## System Components Demonstrated

- **Interactive CLI**: The entry point that detects input mode (search vs. statement) and orchestrates the appropriate processing flow.
- **DynamicDataAccessLayer (DDAL)**: The core orchestrator for search queries that handles intent analysis, query generation, and result hydration.
- **QueryProcessor**: Manages statement processing through NLU orchestration and dynamic storage.
- **NLUOrchestrator**: A sophisticated natural language understanding pipeline with policy-based processing and multi-task execution.
- **UnifiedLLMAdapter**: A unified interface to multiple LLM providers with dynamic model selection and load balancing.
- **DynamicStorage**: An intelligent storage layer that persists extracted knowledge nodes and relationships to SurrealDB.

## Key Flows

- **Search Mode**: Natural language → Intent analysis → Query generation (simple or LLM-assisted) → SurrealDB execution → Knowledge node hydration → Formatted results.
- **Statement Mode**: Natural language → NLU orchestration → Multi-task LLM processing → Knowledge extraction → Graph storage → Success confirmation.

## Notable Features

- **Dynamic complexity routing**: Simple queries bypass LLM processing, while complex ones use the full AI pipeline.
- **Unified LLM layer**: A single adapter supports multiple providers (Anthropic, OpenAI, Ollama) with automatic failover.
- **Graph-native storage**: Enables direct storage of knowledge nodes and relationships in a SurrealDB graph database.
- **Policy-driven NLU**: Employs configurable processing policies that adapt to input characteristics.
- **Local-first architecture**: Prioritises local models (Ollama) with cloud fallback options.

## Prerequisites

A local instance of SurrealDB is required. Install it on your system, then start it with the following command:

```sh
surreal start --log trace --user root --pass root --bind 127.0.0.1:8000 memory
```

## How to Run

Execute the following command from the workspace root to run the demo:

```sh
cargo run --bin basic-database-demo
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
participant User
participant CLI as Interactive CLI
participant QP as QueryProcessor
participant DDAL as DynamicDataAccessLayer
participant NLU as NLUOrchestrator
participant ULA as UnifiedLLMAdapter
participant MS as DynamicModelSelector
participant IA as IntentAnalyser
participant QG as QueryGenerator
participant DS as DynamicStorage
participant DBConn as DatabaseConnection
participant SDB as SurrealDB
Note over User,SDB: Basic Database Demo Architecture Flow
User->>CLI: Enter input text
CLI->>CLI: is_search_query(input)?
alt Search Mode
Note over CLI,SDB: Natural Language Query Processing
CLI->>DDAL: query_natural_language(query)
DDAL->>IA: analyse_intent(query)
IA->>IA: determine complexity
IA-->>DDAL: AdvancedQueryIntent
alt Simple Lookup
DDAL->>QG: build_query_from_intent(intent)
QG-->>DDAL: SelectQuery
else Complex Query
DDAL->>IA: build_prompt_for_query(query)
IA-->>DDAL: enhanced_prompt
DDAL->>ULA: process_text(prompt)
ULA->>MS: select_model(requirements)
MS-->>ULA: ModelSelection
ULA->>ULA: execute_llm_request()
ULA-->>DDAL: llm_response
DDAL->>IA: parse_response_to_plan(response)
IA-->>DDAL: Vec<ToolCall>
DDAL->>QG: build_query_from_plan(plan, intent)
QG-->>DDAL: SelectQuery
end
DDAL->>DDAL: execute_and_hydrate(query)
DDAL->>SDB: query(sql)
SDB-->>DDAL: raw_results
DDAL->>DDAL: hydrate_to_knowledge_nodes()
DDAL-->>CLI: Vec<KnowledgeNode>
CLI->>CLI: format_search_results()
CLI-->>User: Display formatted results
else Statement Mode
Note over CLI,SDB: Natural Language Statement Processing
CLI->>QP: process_and_store_input(statement, user, channel)
QP->>NLU: process_input(statement)
NLU->>NLU: analyse_input(statement)
NLU->>NLU: select_policy(analysis)
NLU->>NLU: create_processing_plan(policy)
loop For each task in plan
NLU->>ULA: execute_task(task_prompt)
ULA->>MS: select_model(task_requirements)
MS-->>ULA: ModelSelection
ULA->>ULA: execute_llm_request()
ULA-->>NLU: task_result
end
NLU->>NLU: consolidate_results(task_outputs)
NLU-->>QP: UnifiedNLUData
QP->>DS: store_llm_output(user, channel, input, nlu_data)
DS->>DS: create_utterance_record()
loop For each extracted node
DS->>DBConn: store_node(node)
DBConn->>SDB: CREATE node_record
SDB-->>DBConn: node_result
end
loop For each relationship
DS->>DBConn: store_relationship(rel)
DBConn->>SDB: RELATE source->type->target
SDB-->>DBConn: rel_result
end
DS-->>QP: storage_results
QP-->>CLI: JSON response with metadata
CLI->>CLI: format_storage_results()
CLI-->>User: Display success summary
end
Note over CLI,SDB: Error flows omitted for clarity but handled at each layer
Note over CLI,SDB: System Initialisation (happens once at startup)
rect rgb(240, 248, 255)
CLI->>DBConn: Connect via DatabaseCommand
DBConn->>SDB: establish_connection()
DBConn->>SDB: initialise_schema()
SDB-->>DBConn: connection_ready
CLI->>ULA: new(model_selector)
ULA->>MS: load_configuration()
MS-->>ULA: ready
CLI->>NLU: with_unified_adapter(config, adapter)
NLU->>NLU: load_config(prompts, policies, models)
NLU-->>CLI: orchestrator_ready
CLI->>DDAL: new(db_client, llm_adapter)
DDAL->>DDAL: initialise_components()
DDAL-->>CLI: access_layer_ready
CLI->>QP: new(orchestrator, storage, config)
QP-->>CLI: processor_ready
end

:::

</details>

Copyright (C) 2024 Jonathan Lee.
