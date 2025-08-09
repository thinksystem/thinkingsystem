# Data Exchange IAM Demo - Policy-Based Access Control

This demo showcases the Steel crate's advanced Identity and Access Management (IAM) integration with data exchange systems, demonstrating policy-based access control for enterprise data flows in a retail scenario.

## Overview

The Data Exchange IAM Demo implements a comprehensive policy-driven security system that combines:

- **Real IAM Provider**: JWT-based authentication with role management
- **Policy Engine**: YAML-configured access control with condition evaluation
- **Data Exchange System**: HTTP-based data publishing with provider routing
- **Role-Based Authorisation**: Dynamic permission evaluation based on user roles and data content

The demo simulates a retail company ("Fusion Retail") with three distinct user roles accessing different data exchange endpoints with varying levels of authorisation.

---

## Architecture

### Core Components

- **DataExchangeService**: Main orchestrator integrating IAM with policy-based data exchange.
- **PolicyEngine**: Evaluates access policies with condition-based rules.
- **IdentityProvider**: Manages user authentication, roles, and JWT tokens.
- **HttpDataExchangeImpl**: Handles REST-based data publishing to external endpoints.

### System Flow

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   User Request  │───▶│  IAM Provider   │───▶│  Policy Engine  │
│  with JWT Token │    │   (Verify &     │    │   (Evaluate     │
│                 │    │   Extract Roles)│    │    Conditions)  │
└─────────────────┘    └─────────────────┘    └─────────────────┘
                                                        │
                                                        ▼
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│  Data Exchange  │◀───│  Authorisation  │◀───│   Allow/Deny    │
│   (HTTP API)    │    │    Decision     │    │    Decision     │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

---

## Demo Scenarios

The demo executes five realistic business scenarios:

### Scenario A: Store Manager Sales Data (Success)

- **User**: Alice (StoreManager role)
- **Action**: Publish sales data to `store_data_exchange`
- **Expected**: SUCCESS - Policy allows StoreManager to publish SalesData
- **Policy Match**: `allow_store_manager_data`

### Scenario B: Store Manager Logistics Attempt (Failure)

- **User**: Alice (StoreManager role)
- **Action**: Attempt logistics command to `logistics_api`
- **Expected**: FAILURE - StoreManager lacks logistics permissions
- **Result**: Implicitly denied (no matching allow policy)

### Scenario C: Logistics Coordinator Command (Success)

- **User**: Bob (LogisticsCoordinator role)
- **Action**: Issue inventory command to `logistics_api`
- **Expected**: SUCCESS - LogisticsCoordinator has proper permissions
- **Policy Match**: `allow_logistics_commands`

### Scenario D: Analyst PII Data Attempt (Failure)

- **User**: Charlie (RegionalAnalyst role)
- **Action**: Attempt to publish data containing PII to `analytics_service`
- **Expected**: FAILURE - PII data explicitly blocked for all users
- **Policy Match**: `block_pii_analytics` (explicit deny)

### Scenario E: Analyst Clean Query (Success)

- **User**: Charlie (RegionalAnalyst role)
- **Action**: Publish clean analytics query to `analytics_service`
- **Expected**: SUCCESS - Clean analytics allowed for RegionalAnalyst
- **Policy Match**: `allow_analyst_queries`

---

## Policy Configuration

The `policy.yaml` file defines:

### Providers

- **store_data_exchange**: Store data collection endpoint
- **logistics_api**: Warehouse and inventory management
- **analytics_service**: Business intelligence and reporting

### Policies

1.  **allow_store_manager_data**: Permits StoreManager to publish sales/inventory data
2.  **allow_logistics_commands**: Enables LogisticsCoordinator to issue inventory commands
3.  **block_pii_analytics**: Explicitly blocks PII data for all users (security rule)
4.  **allow_analyst_queries**: Permits RegionalAnalyst to run clean analytics queries

### Condition Evaluation

- Data classification matching (`data.classification == 'SalesData'`)
- Content inspection (`data.contains('customer_pii')`)
- Multi-value conditions (`data.classification in ['SalesData', 'InventoryData']`)

---

## Running the Demo

### Prerequisites

- Rust toolchain (1.70+)
- No external dependencies required (uses in-memory database)

### Execution

```bash
# From project root
cargo run --bin data-exchange-iam-demo

# Or from demo directory
cd bin/demos/data-exchange-iam-demo
cargo run
```

### Expected Output

The demo produces structured logs showing:

- IAM provider initialisation with SurrealDB backend
- Policy engine loading with rule distribution analysis
- User creation with role assignments
- Real-time authorisation decisions for each scenario
- HTTP data exchange attempts (expected to fail as demo endpoints)
- Performance timing for each operation

---

## Key Features Demonstrated

### 1\. JWT-Based Authentication

- Secure token generation with role claims
- Token verification and role extraction
- Database-backed user management with persistent roles

### 2\. Policy-Driven Authorisation

- YAML-based policy configuration
- Dynamic condition evaluation
- Explicit allow/deny rules with precedence
- Role-based and content-based access control

### 3\. Real Data Exchange Integration

- HTTP REST endpoint integration
- Message metadata handling
- Provider-specific routing
- Error handling and fallback mechanisms

### 4\. Enterprise Security Patterns

- Principle of least privilege
- Defence in depth with multiple authorisation layers
- PII protection with explicit blocking rules
- Audit logging for compliance

---

## Configuration Details

### User Roles

- **StoreManager**: Can publish sales and inventory data to store systems.
- **LogisticsCoordinator**: Can issue inventory commands to logistics systems.
- **RegionalAnalyst**: Can run analytics queries but is blocked from PII access.

### Policy Engine Logic

1.  **Explicit Deny First**: PII blocking rules take precedence.
2.  **Role Matching**: User must have the required role for the policy.
3.  **Condition Evaluation**: Data content must match policy conditions.
4.  **Implicit Deny**: No matching allow policy results in access denial.

### HTTP Providers

All providers use localhost endpoints (expected to fail in the demo):

- `store_data_exchange`: Port 8080
- `logistics_api`: Port 8081
- `analytics_service`: Port 8082

---

## Technical Implementation

### Dependencies

- **steel**: Core IAM and policy engine
- **surrealdb**: In-memory database for user/role persistence
- **tokio**: Async runtime
- **serde/serde_yaml**: Configuration and data serialisation
- **tracing**: Structured logging

### Performance Characteristics

- **IAM Initialisation**: \~40ms (including database setup)
- **Policy Engine Loading**: \~6ms (4 policies, 3 providers)
- **Authorisation Decisions**: \<1ms per evaluation
- **User Creation**: \~280ms per user (includes database operations)

---

## Security Considerations

### Implemented Protections

- JWT token expiry and validation
- Role-based access control with database persistence
- Content inspection for sensitive data (PII)
- Structured audit logging for security events

### Production Recommendations

1.  **External Databases**: Replace the in-memory database with production-grade storage.
2.  **Secret Management**: Use secure secret stores for JWT signing keys.
3.  **Rate Limiting**: Implement request throttling for API endpoints.
4.  **TLS Encryption**: Secure all HTTP communications.
5.  **Policy Versioning**: Implement policy change management and rollback.

---

## What This Demo Demonstrates

### Genuine Security Implementation

- **Real JWT Authentication**: HS256 signature validation with 24-hour expiry.
- **Database-Backed IAM**: Persistent user/role storage using SurrealDB.
- **Legitimate Policy Engine**: YAML-based policies with AST condition parsing.
- **Content-Aware Authorisation**: Real-time data inspection for PII and classification.
- **Two-Phase Security**: Explicit deny policies override allow rules.

### Important Note About Success Messages

When the demo logs **"[SUCCESS]: StoreManager published sales data as expected"**, this means:

- **Authorisation was granted** by the policy engine.
- **The JWT token was verified** and roles were extracted.
- **Policy conditions matched** the request.
- **Actual HTTP transmission failed** (connection refused to localhost:8080).

The demo uses mock HTTP endpoints that are expected to fail. Success refers to **authorisation success**, not data transmission success.

### Measured Performance

From actual execution:

- **IAM Initialisation**: 39ms (including SurrealDB setup)
- **Policy Engine Loading**: 6ms (4 policies, 3 providers)
- **Authorisation Decisions**: \<1ms per evaluation
- **User Creation**: \~290ms per user (database operations)
- **Total Demo Runtime**: \~1233ms

---

## Learning Objectives

This demo illustrates:

1.  **Enterprise IAM Integration**: Real-world identity provider with JWT tokens.
2.  **Policy-Based Security**: Flexible, configurable access control systems.
3.  **Content-Aware Authorisation**: Decision-making based on data content.
4.  **Multi-Provider Architecture**: Routing requests to different backend systems.
5.  **Security Defence in Depth**: Multiple layers of authorisation and validation.
6.  **Compliance Patterns**: Audit logging and PII protection mechanisms.

---

## Extending the Demo

### Add New Roles

1.  Define a role in the user creation code.
2.  Add corresponding policies in `policy.yaml`.
3.  Create test scenarios for the new role.

### Custom Conditions

1.  Extend the policy condition syntax in `policy.yaml`.
2.  Implement condition evaluation logic in the policy engine.
3.  Test with various data payloads.

### Additional Providers

1.  Add provider configuration to `policy.yaml`.
2.  Implement provider-specific HTTP implementations.
3.  Define role-based access policies for new providers.

---

## Technical Architecture Deep Dive

The Steel IAM-Data Exchange engine implements a sophisticated multi-layer security architecture with real-time policy evaluation.

### Key Technical Innovations

#### 1\. Two-Phase Policy Evaluation

- **Phase 1**: Explicit deny policies are evaluated first (security-first approach).
- **Phase 2**: Allow policies are evaluated only if no denies were triggered.
- **Principle**: "Explicit Deny Overrides Allow" ensures maximum security.

#### 2\. Dynamic AST-Based Condition Evaluation

```rust
// Policy conditions parsed into AST at engine initialisation
Expression::Equals(
    Box::new(Expression::Field("data.classification")),
    Box::new(Expression::Value(Value::String("SalesData")))
)

// Runtime evaluation with JSONPath-like field access
context.get_field("data.classification") // Returns Some(Value::String("SalesData"))
```

#### 3\. JWT Claims with Role Persistence

- **Stable Identity**: Email-based subject claims for consistent user identification.
- **Database-Backed Roles**: SurrealDB persistence with separate admin connections.
- **Token Verification**: HS256 signature validation with issuer/audience verification.

#### 4\. Provider-Agnostic Data Exchange

- **HTTP Implementation**: RESTful POST with JSON payloads.
- **Kafka Support**: Async message publishing with UUID keys.
- **gRPC Integration**: Protobuf serialisation with connection pooling.
- **MQTT Bridge**: IoT device communication patterns.

#### 5\. Structured Metadata Handling

```rust
MessageMetadata::new()
    .with_type("SalesData")
    .with_metadata("timestamp", MetadataValue::String(Utc::now().to_rfc3339()))
    .with_metadata("audit_trail", MetadataValue::Boolean(true))
```

### Performance Characteristics

| Component        | Operation               | Latency | Notes                            |
| :--------------- | :---------------------- | :------ | :------------------------------- |
| JWT Verification | Token decode + validate | \<1ms   | In-memory signature verification |
| Policy Engine    | Authorisation decision  | \<1ms   | Pre-parsed AST conditions        |
| SurrealDB        | Role lookup             | \~1ms   | In-memory database operations    |
| HTTP Exchange    | REST API call           | 5-50ms  | Network-dependent                |
| Full Request     | End-to-end processing   | \<10ms  | Excluding external API latency   |

### Security Guarantees

1.  **Defence in Depth**: Multiple authorisation layers (JWT + Roles + Policies + Content).
2.  **Fail-Safe Defaults**: Implicit deny when no allow policies match.
3.  **Explicit Deny Priority**: Security policies always override permissive rules.
4.  **Content Inspection**: Real-time data payload analysis for sensitive content.
5.  **Audit Trail**: Structured logging for compliance and security monitoring.

---

<summary>Click to expand the sequence diagram</summary>
<details>

::: mermaid

---

config:
theme: neutral

---

sequenceDiagram
participant User as Demo Runner
participant Main as main()
participant IAM as IdentityProvider
participant SDB as SurrealDB
participant DS as DataExchangeService
participant PE as PolicyEngine
participant HTTP as HttpDataExchangeImpl
participant EP as External Endpoint
User->>Main: cargo run
Main->>IAM: IdentityProvider::new()
IAM->>SDB: Initialise in-memory database
SDB-->>IAM: Database ready
IAM-->>Main: IAM provider initialized (39ms)
Main->>DS: DataExchangeService::new(policy.yaml)
DS->>PE: PolicyLoader::load_from_file()
Note over PE: Load 4 policies, 3 providers from YAML
PE-->>DS: Policy engine with parsed conditions
DS->>HTTP: HttpDataExchangeImpl::new() for each provider
Note over HTTP: Creates reqwest clients for localhost:8080-8082
HTTP-->>DS: HTTP implementations created
DS-->>Main: DataExchangeService ready (6ms)
Main->>IAM: bootstrap_admin()
IAM->>SDB: CREATE admin user record
IAM->>SDB: ASSIGN admin roles
SDB-->>IAM: Admin user with ["admin", "user"] roles
IAM-->>Main: Admin token
loop For each demo user (Alice, Bob, Charlie)
Main->>IAM: signup(name, email, password)
IAM->>SDB: CREATE user record with UUID
SDB-->>IAM: User created
Main->>IAM: assign_role(email, role, admin_token)
IAM->>SDB: UPDATE user roles
SDB-->>IAM: Roles assigned
Main->>IAM: create_token_with_database_roles()
IAM->>SDB: SELECT roles for user
SDB-->>IAM: User roles retrieved
IAM-->>Main: JWT token with role claims
end
Note over Main: Users created: StoreManager, LogisticsCoordinator, RegionalAnalyst
Main->>Main: run_policy_scenarios()
Main->>DS: publish_data(alice_token, "store_data_exchange", sales_data)
DS->>IAM: verify_token(alice_token)
IAM-->>DS: Claims{roles: ["user", "StoreManager"]}
DS->>PE: authorise(roles, "publish", "store_data_exchange", sales_data)
Note over PE: Policy evaluation - 2 phases
PE->>PE: Phase 1: Check explicit deny policies
Note over PE: No deny policies match
PE->>PE: Phase 2: Check allow policies
Note over PE: "allow_store_manager_data" matches:<br/>✓ Role: StoreManager ∈ user roles<br/>✓ Action: publish<br/>✓ Resource: store_data_exchange<br/>✓ Condition: data.classification="SalesData"
PE-->>DS: AuthorisationDecision::Allow
DS->>HTTP: exchange_data(sales_data_json)
HTTP->>EP: POST localhost:8080 (store_data_exchange)
Note over EP: Connection refused (expected)
EP-->>HTTP: Connection error
HTTP-->>DS: DataExchangeError::Http
DS-->>Main: Ok(()) - Authorization succeeded
Main->>Main: Log "Scenario A [SUCCESS]"
Main->>DS: publish_data(alice_token, "logistics_api", logistics_data)
DS->>IAM: verify_token(alice_token)
IAM-->>DS: Claims{roles: ["user", "StoreManager"]}
DS->>PE: authorise(roles, "publish", "logistics_api", logistics_data)
PE->>PE: Phase 1: Check explicit deny policies
Note over PE: No deny policies match
PE->>PE: Phase 2: Check allow policies
Note over PE: No allow policies match StoreManager + logistics_api
PE-->>DS: AuthorisationDecision::Deny("Implicitly denied")
DS-->>Main: Err("🚫 Implicitly denied")
Main->>Main: Log "Scenario B [SUCCESS]: Correctly denied"
Main->>DS: publish_data(bob_token, "logistics_api", inventory_command)
DS->>IAM: verify_token(bob_token)
IAM-->>DS: Claims{roles: ["user", "LogisticsCoordinator"]}
DS->>PE: authorise(roles, "publish", "logistics_api", inventory_command)
PE->>PE: Phase 1: Check explicit deny policies
Note over PE: No deny policies match
PE->>PE: Phase 2: Check allow policies
Note over PE: "allow_logistics_commands" matches:<br/>✓ Role: LogisticsCoordinator ∈ user roles<br/>✓ Action: publish<br/>✓ Resource: logistics_api<br/>✓ Condition: data.classification="InventoryCommand"
PE-->>DS: AuthorisationDecision::Allow
DS->>HTTP: exchange_data(inventory_command_json)
HTTP->>EP: POST localhost:8081 (logistics_api)
Note over EP: Connection refused (expected)
EP-->>HTTP: Connection error
HTTP-->>DS: DataExchangeError::Http
DS-->>Main: Ok(()) - Authorization succeeded
Main->>Main: Log "Scenario C [SUCCESS]"
Main->>DS: publish_data(charlie_token, "analytics_service", pii_data)
DS->>IAM: verify_token(charlie_token)
IAM-->>DS: Claims{roles: ["user", "RegionalAnalyst"]}
DS->>PE: authorise(roles, "publish", "analytics_service", pii_data)
PE->>PE: Phase 1: Check explicit deny policies
Note over PE: "block_pii_analytics" matches:<br/>✓ Role: "any" (matches all roles)<br/>✓ Action: publish<br/>✓ Resource: analytics_service<br/>✓ Condition: data.contains("customer_pii") = true
PE-->>DS: AuthorisationDecision::Deny("Blocked by policy 'block_pii_analytics'")
DS-->>Main: Err("🚫 Denied by policy")
Main->>Main: Log "Scenario D [SUCCESS]: Correctly blocked PII"
Main->>DS: publish_data(charlie_token, "analytics_service", clean_query)
DS->>IAM: verify_token(charlie_token)
IAM-->>DS: Claims{roles: ["user", "RegionalAnalyst"]}
DS->>PE: authorise(roles, "publish", "analytics_service", clean_query)
PE->>PE: Phase 1: Check explicit deny policies
Note over PE: PII policy doesn't match (no customer_pii field)
PE->>PE: Phase 2: Check allow policies
Note over PE: "allow_analyst_queries" matches:<br/>✓ Role: RegionalAnalyst ∈ user roles<br/>✓ Action: publish<br/>✓ Resource: analytics_service<br/>✓ Condition: data.classification="AnalyticsQuery"
PE-->>DS: AuthorisationDecision::Allow
DS->>HTTP: exchange_data(clean_query_json)
HTTP->>EP: POST localhost:8082 (analytics_service)
Note over EP: Connection refused (expected)
EP-->>HTTP: Connection error
HTTP-->>DS: DataExchangeError::Http
DS-->>Main: Ok(()) - Authorization succeeded
Main->>Main: Log "Scenario E [SUCCESS]"
Main->>User: Demo completed (1233ms total)
Note over IAM,SDB: Verified Components:<br/>• Real JWT signature validation<br/>• Persistent database operations<br/>• Genuine policy condition parsing<br/>• Authentic HTTP client attempts<br/>• Proper error handling throughout
Note over PE: Policy Engine Verification:<br/>• 4 policies loaded from YAML<br/>• 2-phase evaluation (deny-first)<br/>• AST-based condition parsing<br/>• Content-aware authorization
Note over HTTP,EP: Data Exchange Reality:<br/>• Real HTTP POST attempts<br/>• Expected connection failures<br/>• "Success" = authorization granted<br/>• Documented demo behavior

:::

</details>

Copyright (C) 2024 Jonathan Lee.
