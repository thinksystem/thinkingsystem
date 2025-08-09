# Enhanced STELE Scribes Demo - Detailed Sequence Diagram

This sequence diagram shows the complete flow of the enhanced scribes demo with real LLM processing, comprehensive logging, and database integration.

## ::: mermaid

config:
theme: neutral

---

sequenceDiagram
participant U as User
participant M as Main Demo
participant L as LLM Logger
participant LA as Logging LLM Adapter
participant API as Anthropic/OpenAI API
participant EP as Enhanced Data Processor
participant DB as SurrealDB
participant IV as Identity Verifier
participant IAM as Steel IAM Provider
participant KS as Knowledge Scribe
participant QL as Q-Learning Core
participant FS as File System
Note over U, FS: Enhanced STELE Scribes Demo Initialisation
U->>M: Start Demo
M->>FS: Setup Logging Infrastructure
FS-->>M: Log Files Created (logs/)
M->>L: Initialise LLM Logger
L-->>M: Logger Ready with Session ID
M->>LA: Initialise Logging LLM Adapter
LA->>API: Test Connection (Anthropic)
alt Anthropic Available
API-->>LA: Connection Success
LA-->>M: Anthropic Adapter Ready
else Anthropic Failed
LA->>API: Test Connection (OpenAI)
API-->>LA: OpenAI Connection Success
LA-->>M: OpenAI Adapter Ready
end
M->>DB: Initialise Enhanced Database
DB-->>M: SurrealDB Ready (enhanced*demo.scribes)
M->>IAM: Initialise Steel IAM Provider
IAM-->>M: IAM Provider Ready
M->>EP: Initialise Enhanced Data Processor
EP-->>M: Enhanced Processor Ready
M->>IV: Initialise Enhanced Identity Verifier
IV-->>M: Identity Verifier Ready
Note over U, FS: Test Phase 1: Enhanced Data Processing
M->>EP: Process Test Scenarios
loop For Each Test Scenario
EP->>L: Start LLM Call Tracking
L-->>EP: Call ID Generated
EP->>LA: Process Text with LLM Analysis
LA->>L: Log Request Details
LA->>API: Send Structured Analysis Request
API-->>LA: Return JSON Analysis
LA->>L: Log Response & Calculate Cost
LA-->>EP: Structured Analysis Result
EP->>EP: Parse JSON Response
alt JSON Valid
EP->>DB: Store Processed Data
DB-->>EP: Storage Confirmation
else JSON Invalid
EP->>EP: Create Fallback Analysis
EP->>DB: Store Fallback Data
DB-->>EP: Storage Confirmation
end
EP->>LA: Extract Named Entities
LA->>L: Log Entity Extraction Request
LA->>API: Send Entity Extraction Request
API-->>LA: Return Entity Data
LA->>L: Log Entity Response
LA-->>EP: Entity Extraction Result
EP->>DB: Store Extracted Entities
DB-->>EP: Entity Storage Confirmation
end
EP-->>M: All Scenarios Processed
Note over U, FS: Test Phase 2: Enhanced Identity Verification
M->>IV: Verify Test Identities
loop For Each Identity Context
IV->>IAM: Bootstrap Admin User
alt Admin Exists
IAM-->>IV: Admin Already Exists
else Admin New
IAM->>DB: Create Admin User
DB-->>IAM: User Created
IAM-->>IV: Admin Bootstrapped
end
IV->>IAM: Create System User
IAM->>DB: Create System User with Roles
DB-->>IAM: User Created with Roles
IAM-->>IV: System User Ready
IV->>IAM: Create Access Token
IAM-->>IV: Token Generated
IV->>IAM: Verify Token Claims
IAM->>DB: Lookup User Roles
DB-->>IAM: User Roles Retrieved
IAM-->>IV: Token Claims Verified
IV->>IV: Calculate Trust Score
end
IV-->>M: Identity Verification Complete
Note over U, FS: Test Phase 3: Knowledge Graph Integration
M->>KS: Link Data to Knowledge Graph
KS->>KS: Process Entity Relationships
KS->>DB: Store Knowledge Links
DB-->>KS: Links Stored
KS-->>M: Knowledge Integration Complete
Note over U, FS: Test Phase 4: Multi-Specialist Coordination
M->>M: Start Coordination Test
M->>IV: Verify Integration Scenario Identity
IV->>IAM: Verify Source Identity
IAM-->>IV: Identity Verified (Trust: 0.95)
IV-->>M: Identity Result
M->>EP: Process Integration Scenario Data
EP->>LA: Analyse Complex Technical Text
LA->>L: Log Complex Analysis Request
LA->>API: Send Analysis Request
Note over API: "Demonstrating comprehensive integration<br/>of real LLM processing, database storage..."
API-->>LA: Return Detailed Analysis
LA->>L: Log Analysis Response & Cost
LA-->>EP: Analysis Complete
EP->>DB: Store Integration Analysis
DB-->>EP: Storage Complete
EP-->>M: Data Processing Result
M->>KS: Integrate Knowledge Graph
KS->>DB: Link Entities to Graph
DB-->>KS: Graph Updated
KS-->>M: Knowledge Integration Result
Note over U, FS: Test Phase 5: Q-Learning Adaptation
M->>QL: Update Learning System
QL->>QL: Calculate State/Action/Reward
QL->>QL: Update Q-Values
QL-->>M: Learning Update Complete
Note over U, FS: Test Phase 6: Enhanced Ecosystem Integration
M->>M: Run Full Integration Test
M->>IV: Enhanced Identity Verification
IV->>IAM: Full IAM Workflow
IAM-->>IV: Identity Result
IV-->>M: Verification Success
M->>EP: Enhanced Data Processing
EP->>LA: Real LLM Processing
LA->>API: API Call
API-->>LA: Response
LA-->>EP: Processed Data
EP->>DB: Store Results
DB-->>EP: Storage Success
EP-->>M: Processing Success
M->>KS: Knowledge Graph Integration
KS-->>M: Integration Success
M->>QL: Q-Learning Adaptation
QL-->>M: Adaptation Success
Note over U, FS: Results and Logging
M->>L: Finalize Session Logs
L->>FS: Write LLM Interactions Log
FS-->>L: llm_interactions.jsonl Created
L->>FS: Write System Logs
FS-->>L: scribes-demo.log.* Created
L-->>M: Logging Complete
M->>M: Aggregate Final Results
Note over M: Final Results:<br/>• 10 LLM Calls ($0.0034 cost)<br/>• 4,339 Tokens Used<br/>• 100% Success Rate<br/>• Real Entity Extraction<br/>• Database Storage<br/>• IAM Integration<br/>• Knowledge Graph Links
M-->>U: Demo Complete with Comprehensive Results
Note over U, FS: Log Files Generated
Note over FS: logs/llm*interactions.jsonl<br/>logs/scribes-demo.log.*<br/>All interactions tracked with:<br/>• Cost estimation<br/>• Token usage<br/>• Response timing<br/>• Success rates
:::

## Key Flows Demonstrated

### 1. **Real LLM Processing Flow**

- Text input → LLM Adapter → API call → Structured JSON response
- Cost tracking and token usage monitoring
- Fallback analysis when JSON parsing fails
- Entity extraction with confidence scores

### 2. **Identity & Access Management Flow**

- Admin bootstrapping → User creation → Role assignment
- Token generation → Token verification → Trust scoring
- Real Steel IAM integration with SurrealDB persistence

### 3. **Data Processing & Storage Flow**

- Raw text → LLM analysis → Structured insights → Database storage
- Entity extraction → Relationship mapping → Knowledge graph integration
- Comprehensive metadata capture and storage

### 4. **Learning & Adaptation Flow**

- Operation results → State calculation → Q-learning updates
- Strategy evolution based on real performance metrics
- Continuous improvement through experience

### 5. **Comprehensive Logging Flow**

- Every LLM interaction logged with full details
- Cost estimation and performance metrics
- Structured JSON logs for analysis and monitoring
- File-based persistence for audit trails

## Example Performance Metrics Captured

- **Total LLM Calls**: 10 successful API interactions
- **Total Cost**: $0.00336575 (cost-effective processing)
- **Total Tokens**: 4,339 tokens across all calls
- **Average Response Time**: 3.57 seconds per call
- **Success Rate**: 100% for all operations
- **Processing Scenarios**: 6 different text analysis scenarios
- **Entity Extraction**: Real named entity recognition with confidence scores
- **Database Operations**: Full CRUD operations with SurrealDB
- **IAM Operations**: Complete identity lifecycle management

This sequence diagram shows the transformation from mock data to real, production-ready LLM processing with comprehensive observability and logging.

cargo run --bin scribes-demo -- --gui

cargo run --bin scribes-demo -- --gui --trace

cargo run --bin scribes-demo -- --gui --log-level info

Copyright (C) 2024 Jonathan Lee.
