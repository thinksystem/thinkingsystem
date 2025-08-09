# GTR Fabric

GTR Fabric is a high-performance routing and economic modelling engine designed to be run as a NIF (Native Implemented Function) within an Elixir application. It provides tools for decentralised networks to dynamically price services, incentivise reliable node performance, and penalise failures through a sophisticated economic feedback loop.

GTR Fabric includes **Verifiable Credentials (VC) integration**, enabling automatic issuance of cryptographically signed credentials that formally attest to supplier reputation and performance metrics. This integration with the Thinking System ecosystem forms a verifiable-trust based bridge with GTR Fabric.

## Core Concepts

The system is built on three primary pillars:

1.  **Geodesic Traversal Routing (GTR)**: This describes the _pathfinding_ algorithm that operates on the conceptual graph of the entire network. Before a transaction, the engine calculates a `potential` score for all possible nodes based on trust, throughput, and latency. It uses this to find the most efficient "geodesic" path for a potential transaction, akin to finding the shortest route over a curved surface.

2.  **Proof-of-Performance Staking (PoPS)**: This is the economic model that underpins the network's integrity. Suppliers stake collateral to offer services, and their potential rewards and required collateral are determined by their historical reliability (Trust Score). Failure to meet an SLA results in the staked collateral being "slashed".

3.  **Dynamic Parameter Engine**: This is the crucial feedback loop that makes the system adaptive. The engine takes in the overall `NetworkState` (e.g., failure rates, supply/demand) at the end of an "epoch" and adjusts the `DynamicParameters` (e.g., penalty severity, collateral multipliers) for the next one. This allows the network to automatically tighten requirements and increase penalties when performance is poor, and relax them when the network is healthy.

## Key API Modules

The primary interface is split across several modules, each with a distinct responsibility.

### `GtrFabric.Protocol`

This module defines the core, non-negotiable rules of the GTR Fabric protocol.

- `adjust_parameters_for_epoch(params, state)`: Takes the current parameters and network state, and returns the adjusted parameters for the next epoch.
- `calculate_slash_percentage(required_perf, actual_perf, params)`: Calculates the penalty percentage based on performance shortfall.

### `GtrFabric.Strategy`

This module contains strategic, "opinionated" functions that agents can use to participate in the economy. Agents could override this logic with their own.

- `calculate_supplier_offering(trust_score, params)`: Determines the price and required collateral for a supplier based on their trust and the current economic climate.
- `calculate_consumer_utility(offering, trust_score, consumer_factors)`: Calculates a value score for a consumer to help them choose between different supplier offerings.

### `GtrFabric.Reputation`

This module handles the calculation of trust based on historical performance.

- `calculate_score_from_ledger(ledger, params)`: The primary function for reputation. It processes an agent's entire `PerformanceLedger` chronologically—applying time decay and the outcome of each `InteractionRecord`—to produce a final, up-to-date `TrustScore`. This is the most accurate way to assess an agent's reputation.

### `GtrFabric` (Core)

The main module contains the core routing and analysis functions.

- `analyse_dag(packet_trails, sla, total_packets)`: The final step of a task. It analyses the `packet_trails` from a completed transaction—which form a **Directed Acyclic Graph (DAG)** of the actual route taken—to generate a `ResolutionReport` indicating SLA success or failure.
- `calculate_forwarding_decision(candidates, multipath_threshold)`: Determines the next hop for a packet based on candidate potentials and a multipath tolerance.
- `calculate_potential_value(node_metrics, sla)`: Calculates the "potential" of a single node for a given task.
- `issue_trust_score_credential(subject_did, trust_score, performance_ledger, issuer_token)`: **NEW** - Issues cryptographically signed Verifiable Credentials (VCs) that formally attest to a supplier's trust score and performance history.

## Architecture & Data Flow

The system is built on an event-sourced model where an agent's reputation is derived from a history of immutable facts, with the addition of cryptographic verification through Verifiable Credentials.

1.  **Interaction**: An agent performs a task.
2.  **Record**: The outcome (success or failure, performance metrics) is stored as an `InteractionRecord`.
3.  **Ledger**: This record is added to the agent's `PerformanceLedger`.
4.  **Reputation**: The `calculate_score_from_ledger` function is called on the ledger to compute the agent's current `TrustScore`.
5.  **Strategy**: This `TrustScore` is then used in the `Strategy` module to determine the agent's offerings and value in the marketplace.
6.  **Verification**: **NEW** - The system can issue cryptographically signed Verifiable Credentials (VCs) that formally attest to the supplier's reputation and performance metrics, enabling cross-system verification and regulatory compliance.

The following sequence diagram illustrates the call flow for the reputation system and VC integration:

::: mermaid

---

config:
theme: neutral

---

sequenceDiagram
actor User as Elixir App
participant GF as GtrFabric
participant CN as CoreNifs
participant Lib as lib.rs (NIF)
participant DP as dynamic_parameters.rs
participant Core as core.rs
participant Steel as steel::iam
box Elixir
participant User
participant GF
participant CN
end
box Rust NIF
participant Lib
participant DP
participant Core
end
box Steel Crate
participant Steel
end
Note over User, Steel: Core Reputation & Economic Flow
User->>GF: adjust_parameters_for_epoch(params, state)
GF->>CN: adjust_parameters_for_epoch_nif(params, state)
CN->>Lib: adjust_parameters_for_epoch_nif(params, state)
Lib->>DP: adjust_parameters_for_epoch(&current_params, &state)
DP-->>Lib: new_params
Lib-->>CN: new_params
CN-->>GF: new_params
GF-->>User: new_params
User->>GF: calculate_slash_percentage(req, actual, params)
GF->>CN: calculate_slash_percentage_nif(req, actual, params)
CN->>Lib: calculate_slash_percentage_nif(req, actual, params)
Lib->>Core: calculate_slash_percentage(req, actual, &params)
Core-->>Lib: slash_amount
Lib-->>CN: slash_amount
CN-->>GF: slash_amount
GF-->>User: slash_amount
Note over User, Steel: NEW: Verifiable Credential Integration
User->>GF: issue_trust_score_credential(did, trust, ledger, token)
GF->>CN: create_trust_score_credential_nif(did, score, perf_map, token)
CN->>Lib: create_trust_score_credential_nif(did, score, perf_map, token)
Lib->>Steel: VcManager.create_trust_score_credential(did, score, perf, token)
Steel->>Steel: Cryptographic signing with Ed25519
Steel-->>Lib: VerifiableCredential (signed)
Lib-->>CN: VerifiableCredential
CN-->>GF: VerifiableCredential
GF-->>User: {:ok, vc} | {:error, reason}

:::

## Verifiable Credentials Integration

The system includes a VC bridge that enables automatic issuance of cryptographically signed Verifiable Credentials for trust scores and performance metrics.

### Key Features

- **Cryptographic Signatures**: VCs are signed using Ed25519 cryptographic signatures for maximum security and verification
- **Performance Attestation**: Each VC contains detailed performance summaries including success rates, total tasks, and SLA compliance
- **Trust Score Verification**: VCs formally attest to calculated trust scores using the GtrFabric reputation algorithm
- **Cross-System Compatibility**: VCs follow W3C standards and can be verified by external systems
- **Real-time Issuance**: VCs are issued automatically as part of the reputation calculation workflow

### VC Structure

Each issued VC contains:

```json
{
  "id": "urn:uuid:...",
  "credentialSubject": {
    "id": "did:gtr:supplier:supplier_id",
    "trustScore": "0.85",
    "performanceSummary": "{\"successes\":\"19\",\"total\":\"21\",\"success_rate\":\"0.9047619047619048\",\"failures\":\"2\"}",
    "trustAlgorithm": "GtrFabric.Reputation.Heuristics.v1"
  },
  "proof": {
    "type": "Ed25519Signature2018",
    "created": "2025-07-29T...",
    "verificationMethod": "did:steel:issuer#key-1",
    "proofValue": "..."
  }
}
```

### Usage

```elixir
# Issue a VC for a supplier after reputation calculation
case GtrFabric.issue_trust_score_credential(
  "did:gtr:supplier:example",
  trust_score_struct,
  performance_ledger,
  issuer_token
) do
  {:ok, vc} ->
    # VC successfully issued with cryptographic proof
    Logger.info("VC issued: #{vc.id}")
  {:error, reason} ->
    # Handle error (e.g., invalid token, network issues)
    Logger.error("VC issuance failed: #{reason}")
end
```

## Testing & Validation

The system includes comprehensive testing across all components:

### Test Coverage

- **Unit Tests**: Core reputation algorithms, economic calculations, and VC issuance
- **Integration Tests**: NIF bridge functionality and cross-language data serialization
- **Performance Tests**: Load testing for concurrent VC creation and reputation calculations
- **End-to-End Tests**: Complete multi-epoch simulations with VC integration demonstrating:
  - Supplier reputation evolution over time
  - Economic parameter adjustments based on network conditions
  - Automatic VC issuance reflecting real-time performance changes
  - Cross-supplier ecosystem state tracking

### Key Test Scenarios

- **Success Workflows**: VCs issued after successful task completion with high trust scores
- **Failure Recovery**: VCs reflecting degraded performance and trust score reductions
- **Concurrent Operations**: Thread-safe VC creation under high load (20+ parallel operations)
- **Error Handling**: Graceful handling of invalid tokens, network failures, and malformed data

Run the complete test suite:

```bash
mix test                    # All tests
mix test --trace           # Verbose output with timing
mix test test/e2e_test.exs # End-to-end simulation
```

## Development Principles: A Framework for Universal Trust

GTR Fabric's development is guided by a set of core principles designed to ensure it remains a robust, adaptable, and universal standard for reputation. Our architecture is intentionally layered to promote separation of concerns and long-term extensibility.

### Strict Neutrality of the Core Fabric

- **Principle**: `GTR Fabric` itself is, and must remain, completely network-agnostic. Its responsibility is the pure mathematical and economic logic of reputation, routing, and incentives.
- **Implementation**: The core GTR Fabric engine contains no blockchain-specific code. It operates on abstract data structures and relies on external modules to handle any on-chain interactions.

### The `steel` Crate as the Universal Integration Hub

- **Principle**: All blockchain-specific logic is delegated to the `steel` crate. `steel` acts as the bridge between GTR's abstract logic and the concrete implementations of various decentralised networks.
- **Implementation**: `steel` contains the necessary modules and adapters to interface with blockchains like Solana, Ethereum, and others. While the foundational interfaces exist, ongoing work involves fully "wiring in" and expanding these modules to support the full feature set of each target network (e.g., publishing VCs, interacting with on-chain registries).

### Pluggable Architecture via a "Network Adapter" Interface

- **Principle**: Interoperability is achieved through a formal, pluggable interface rather than hardcoded integrations.
- **Implementation**: A "Network Adapter" interface (defined as a Rust trait and Elixir behaviour) provides a standard contract for network capabilities. This allows any developer to add support for a new network by simply implementing the required adapter within the `steel` crate, without ever needing to modify the core GTR Fabric.

### Extensibility for Schemas and Cryptography

- **Principle**: The system must be able to adapt to new standards and technologies without a full rewrite.
- **Implementation**: The architecture supports a flexible credential schema registry and allows for the integration of new cryptographic methods like Zero-Knowledge Proofs (ZKPs). This ensures GTR Fabric can evolve to meet future demands for privacy and regulatory compliance.

This principled approach ensures that GTR Fabric can serve as a universal trust oracle, securely bridging reputation across countless decentralised networks, both today and in the future.

Copyright (C) 2024 Jonathan Lee.
